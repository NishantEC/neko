//! Per-run checkpoints in the daemon: a snapshot when a ticket's writable run
//! starts and ends, the task view's per-run lines and diffs, undo and redo,
//! and the housekeeping. The git work is `neko_core::checkpoints`; see
//! docs/knowledge/checkpoints-and-undo.md. A failure to snapshot is logged and
//! never fails or delays a run.
use super::*;
use neko_core::checkpoints as cp;
use neko_protocol::checkpoints::{CheckpointCommand, CheckpointList, CheckpointRun, REFUSAL_PREFIX};
use std::path::{Path, PathBuf};

/// The run a ticket's execution is in, so it is ended however the execution ends.
#[derive(Default)]
pub(super) struct RunSlot(Mutex<Option<(String, PathBuf, u32)>>);

impl RunSlot {
    pub(super) fn begin(&self, db: &Mutex<Db>, task_id: &str, directory: &Path) {
        match cp::begin_run(db, task_id, directory, store::now_ms()) {
            Ok(n) => *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((task_id.to_owned(), directory.to_path_buf(), n)),
            Err(error) => eprintln!("neko checkpoints {task_id}: not taken at the start: {error}"),
        }
    }

    pub(super) fn end(&self, db: &Mutex<Db>, ok: bool, cancel: &AtomicBool) {
        let Some((task_id, directory, n)) = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() else { return };
        let outcome = if cancel.load(Ordering::SeqCst) { cp::Outcome::Cancelled } else if ok { cp::Outcome::Done } else { cp::Outcome::Failed };
        if let Err(error) = cp::end_run(db, &task_id, n, &directory, outcome, store::now_ms()) {
            eprintln!("neko checkpoints {task_id}: not taken at the end: {error}");
        }
    }
}

/// A refusal as the error text clients read: `checkpoint:<code>: <message>`.
pub(super) fn refusal_text(refusal: &cp::Refusal) -> String {
    format!("{REFUSAL_PREFIX}{}: {}", refusal.code(), refusal.message())
}

#[cfg(test)]
thread_local! {
    /// Tests use their own folder root (the real one is under the data directory).
    pub(super) static TEST_ROOT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

fn worktrees_root() -> PathBuf {
    #[cfg(test)]
    if let Some(root) = TEST_ROOT.with(|r| r.borrow().clone()) {
        return root;
    }
    neko_protocol::support_dir().join("task-worktrees")
}

/// A ticket's folder if it is one of Neko's own task worktrees and exists.
pub(super) fn folder_of(task: &Task) -> Result<PathBuf, String> {
    if task.parked.is_some() {
        return Err("This ticket's folder is parked (removed until it runs again), so there is nothing to restore. Run it again first.".into());
    }
    let worktree = task.worktree.as_deref().ok_or("This ticket has no working folder yet")?;
    let directory = Path::new(worktree).canonicalize().map_err(|_| "The ticket's working folder is missing")?;
    let root = worktrees_root();
    if !root.canonicalize().is_ok_and(|r| directory.starts_with(r)) {
        return Err("Only Neko's own task folders can be restored".into());
    }
    Ok(directory)
}

impl Controller {
    fn task_for_checkpoints(&self, task_id: &str) -> Result<Task, String> {
        let snapshot = store::load(&self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
        snapshot.tasks.into_iter().find(|t| t.id == task_id).ok_or_else(|| "That ticket no longer exists".to_owned())
    }

    fn working_on(&self, task_id: &str) -> bool {
        self.active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().any(|a| a.task_id == task_id)
    }

    /// The directory a read uses: the source checkout holds the objects even when the ticket's folder is parked.
    fn read_dir(&self, task: &Task) -> Result<PathBuf, String> {
        let row = cp::row(&self.db, &task.id)?;
        let repo = PathBuf::from(&row.repo);
        if !row.repo.is_empty() && repo.is_dir() {
            return Ok(repo);
        }
        folder_of(task)
    }

    /// One line per run, for the task view.
    pub fn checkpoint_list(&self, task_id: &str) -> Result<CheckpointList, String> {
        let task = self.task_for_checkpoints(task_id)?;
        let row = cp::row(&self.db, task_id)?;
        let folder = folder_of(&task).ok();
        let busy = self.working_on(task_id);
        let unsaved = match (&folder, row.expected_tree.is_empty()) {
            (Some(dir), false) if !busy => cp::unsaved(dir, &row).unwrap_or(false),
            _ => false,
        };
        Ok(CheckpointList {
            runs: row.runs.iter().map(|r| CheckpointRun {
                n: r.n,
                outcome: format!("{:?}", if r.end.is_none() && !self.working_on(task_id) { cp::Outcome::Cancelled } else { r.outcome }).to_lowercase(),
                started_ms: r.started_ms,
                ended_ms: r.ended_ms,
                files: r.files,
                added: r.added,
                removed: r.removed,
                undone: r.undone,
                skipped: r.start.skipped.iter().chain(r.end.iter().flat_map(|e| e.skipped.iter())).map(|s| s.path.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect(),
                skipped_count: r.start.skipped_count.max(r.end.as_ref().map_or(0, |e| e.skipped_count)),
            }).collect(),
            redo: row.redo.as_ref().map(|r| r.label.clone()),
            unsaved,
            busy,
            restorable: folder.is_some(),
        })
    }

    /// One run's diff, or the whole task's.
    pub fn checkpoint_diff(&self, task_id: &str, run: Option<u32>) -> Result<(Vec<String>, String, bool), String> {
        let task = self.task_for_checkpoints(task_id)?;
        let dir = self.read_dir(&task)?;
        let diff = cp::diff_of(&self.db, task_id, &dir, run)?;
        Ok((diff.files, diff.patch, diff.truncated))
    }

    pub(super) fn checkpoint_command(&self, command: CheckpointCommand) -> Result<Snapshot, String> {
        let task_id = match &command {
            CheckpointCommand::Undo { task_id, .. } | CheckpointCommand::Redo { task_id } | CheckpointCommand::SnapshotUnsaved { task_id } => task_id.clone(),
        };
        let task = self.task_for_checkpoints(&task_id)?;
        let dir = folder_of(&task)?;
        // The folder stays for the length of the restore.
        let _held = parking::InUse::take(&task_id);
        let running = self.working_on(&task_id);
        let now = store::now_ms();
        let message = match command {
            CheckpointCommand::Undo { run, through_later, snapshot_unsaved, revert, .. } => {
                let options = cp::UndoOptions { run, through_later, snapshot_unsaved, revert, pull_request_open: task.pull_request.as_ref().is_some_and(|p| p.is_open()) };
                let done = cp::undo(&self.db, &task_id, &dir, options, running, now).map_err(|r| refusal_text(&r))?;
                done.message
            }
            CheckpointCommand::Redo { .. } => cp::redo(&self.db, &task_id, &dir, running).map_err(|r| refusal_text(&r))?.message,
            CheckpointCommand::SnapshotUnsaved { .. } => {
                if running {
                    return Err(refusal_text(&cp::Refusal::Busy));
                }
                cp::snapshot_unsaved(&self.db, &task_id, &dir, now)?;
                "Kept a snapshot of the changes made outside any run.".to_owned()
            }
        };
        self.update_task(&task_id, |t| store::append_event(t, "neko", &format!("checkpoint: {message}")))?;
        store::load(&self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    /// The sweep's share: expire old checkpoints, and delete a Completed ticket's once its folder is parked.
    pub(super) fn checkpoint_housekeeping(&self) {
        let dropped = cp::expire(&self.db, store::now_ms());
        if dropped > 0 {
            eprintln!("neko checkpoints: dropped {dropped} checkpoints older than 14 days");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::crew::CrewRole;
    use serde_json::{json, Value};

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git").current_dir(dir)
            .args(["-c", "user.name=T", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// A controller whose ticket "t" has a worktree under a temporary root.
    fn fixture(status: TaskStatus) -> (Controller, tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "base"]);
        let root = dir.path().join("task-worktrees");
        std::fs::create_dir(&root).unwrap();
        let worktree = root.join("t");
        git(&repo, &["worktree", "add", "-q", "-b", "codex/neko-t", worktree.to_str().unwrap()]);
        TEST_ROOT.with(|r| *r.borrow_mut() = Some(root));
        let controller = super::super::tests::controller_with_task(status);
        {
            let db = controller.db.lock().unwrap();
            let mut snapshot = store::load(&db).unwrap();
            snapshot.tasks[0].worktree = Some(worktree.to_string_lossy().into_owned());
            snapshot.workspaces[0].repository = repo.to_string_lossy().into_owned();
            store::save(&db, &snapshot).unwrap();
        }
        (controller, dir, worktree)
    }

    fn a_run(c: &Controller, worktree: &Path, content: &str) {
        let slot = RunSlot::default();
        slot.begin(&c.db, "t", worktree);
        std::fs::write(worktree.join("a.txt"), content).unwrap();
        slot.end(&c.db, true, &AtomicBool::new(false));
    }

    fn undo(run: u32, through_later: bool) -> CheckpointCommand {
        CheckpointCommand::Undo { task_id: "t".into(), run, through_later, snapshot_unsaved: false, revert: false }
    }

    #[test]
    fn runs_are_listed_diffed_undone_and_redone_and_the_ticket_records_it() {
        let (c, _dir, worktree) = fixture(TaskStatus::ReadyForReview);
        a_run(&c, &worktree, "two\n");
        a_run(&c, &worktree, "three\n");
        let list = c.checkpoint_list("t").unwrap();
        assert_eq!(list.runs.len(), 2);
        assert_eq!((list.runs[0].files, list.runs[0].added, list.runs[0].removed), (1, 1, 1));
        assert_eq!(list.runs[1].outcome, "done");
        assert!(list.restorable && !list.unsaved && !list.busy && list.redo.is_none());
        let (files, patch, _) = c.checkpoint_diff("t", Some(2)).unwrap();
        assert_eq!(files, vec!["a.txt"]);
        assert!(patch.contains("+three") && patch.contains("-two"));
        let (_, whole, _) = c.checkpoint_diff("t", None).unwrap();
        assert!(whole.contains("-one") && whole.contains("+three"));

        // Later runs: refused with a code the app keys its offer on, and nothing changed.
        let error = c.checkpoint_command(undo(1, false)).unwrap_err();
        assert!(error.starts_with("checkpoint:later_runs:"), "{error}");
        assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "three\n");
        // A worker in the folder: refused.
        c.active.lock().unwrap().push(Active { task_id: "t".into(), cancelled: Arc::new(AtomicBool::new(false)) });
        assert!(c.checkpoint_command(undo(2, false)).unwrap_err().starts_with("checkpoint:busy:"));
        assert!(c.checkpoint_list("t").unwrap().busy);
        c.active.lock().unwrap().clear();

        let snapshot = c.checkpoint_command(undo(1, true)).unwrap();
        assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\n");
        assert!(snapshot.tasks[0].events.iter().any(|e| e.message.starts_with("checkpoint: Undid run 1, run 2")), "the ticket's history says so");
        let list = c.checkpoint_list("t").unwrap();
        assert!(list.runs.iter().all(|r| r.undone));
        assert_eq!(list.redo.as_deref(), Some("undo of run 1"));
        // The next agent turn is told, once.
        assert!(neko_core::checkpoints::take_notice(&c.db, "t").unwrap().contains("undid run 1, run 2"));
        assert!(neko_core::checkpoints::take_notice(&c.db, "t").is_none());

        c.checkpoint_command(CheckpointCommand::Redo { task_id: "t".into() }).unwrap();
        assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "three\n");

        // Edits no checkpoint holds: refused, then kept on request.
        std::fs::write(worktree.join("a.txt"), "mine\n").unwrap();
        assert!(c.checkpoint_list("t").unwrap().unsaved);
        assert!(c.checkpoint_command(undo(2, false)).unwrap_err().starts_with("checkpoint:unsaved_changes:"));
        c.checkpoint_command(CheckpointCommand::SnapshotUnsaved { task_id: "t".into() }).unwrap();
        assert!(!c.checkpoint_list("t").unwrap().unsaved);
    }

    #[test]
    fn a_stale_running_run_is_closed_by_the_next_one_and_a_failed_run_is_marked() {
        let (c, _dir, worktree) = fixture(TaskStatus::ReadyForReview);
        let crashed = RunSlot::default();
        crashed.begin(&c.db, "t", &worktree);
        std::fs::write(worktree.join("a.txt"), "half\n").unwrap();
        // The daemon stopped mid-run: no end. The next run's start closes it.
        let slot = RunSlot::default();
        slot.begin(&c.db, "t", &worktree);
        slot.end(&c.db, false, &AtomicBool::new(false));
        let list = c.checkpoint_list("t").unwrap();
        assert_eq!(list.runs[0].outcome, "cancelled");
        assert_eq!(list.runs[0].added, 1);
        assert_eq!(list.runs[1].outcome, "failed");
    }

    #[test]
    fn a_parked_ticket_has_no_folder_to_restore_but_its_diff_is_still_there() {
        let (c, _dir, worktree) = fixture(TaskStatus::ReadyForReview);
        a_run(&c, &worktree, "two\n");
        {
            let db = c.db.lock().unwrap();
            let mut snapshot = store::load(&db).unwrap();
            snapshot.tasks[0].parked = Some(ParkedWork { branch: "codex/neko-t".into(), repository: String::new(), wip: false });
            store::save(&db, &snapshot).unwrap();
        }
        assert!(c.checkpoint_command(undo(1, false)).unwrap_err().contains("parked"));
        assert!(!c.checkpoint_list("t").unwrap().restorable);
        assert!(c.checkpoint_diff("t", Some(1)).unwrap().1.contains("+two"));
    }

    #[test]
    fn a_completed_ticket_loses_its_checkpoints_when_its_folder_is_parked() {
        let (c, dir, worktree) = fixture(TaskStatus::Completed);
        a_run(&c, &worktree, "two\n");
        assert_eq!(neko_core::checkpoints::refs_of(&worktree, "t").len(), 2);
        // The sweep (QUIET_MS has passed for a task last touched long ago).
        {
            let db = c.db.lock().unwrap();
            let mut snapshot = store::load(&db).unwrap();
            snapshot.tasks[0].updated_at_ms = 1;
            store::save(&db, &snapshot).unwrap();
        }
        assert_eq!(c.parking_pass(&dir.path().join("task-worktrees")).unwrap(), 1);
        assert!(neko_core::checkpoints::refs_of(&dir.path().join("repo"), "t").is_empty());
        assert!(neko_core::checkpoints::row(&c.db, "t").unwrap().runs.is_empty());
    }

    fn call(c: &Controller, lease: &crate::mcp_host::Lease, tool: &str, args: Value) -> Result<Value, String> {
        c.crew_call(lease.token(), tool, &args.to_string(), &|_, _| Err("no artifacts".into())).map(|text| serde_json::from_str(&text).unwrap())
    }

    #[test]
    fn the_crew_tools_follow_the_role() {
        let (c, _dir, worktree) = fixture(TaskStatus::ReadyForReview);
        a_run(&c, &worktree, "two\n");
        let full = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
        let read_only = c.mcp.crew_lease("chat:t2", CrewRole::ReadOnly).unwrap();
        let own = c.mcp.crew_lease("run-1", CrewRole::Task { ticket_id: "t".into(), may_create: false }).unwrap();
        let other = c.mcp.crew_lease("run-2", CrewRole::Task { ticket_id: "other".into(), may_create: false }).unwrap();

        // A builder reads its own ticket's diff (task_id optional) and cannot undo.
        let shown = call(&c, &own, "checkpoint_diff", json!({"run": 1})).unwrap();
        assert_eq!(shown["runs"][0]["run"], 1);
        assert!(shown["diff"]["patch"].as_str().unwrap().contains("+two"));
        assert!(call(&c, &own, "undo_run", json!({"task_id": "t", "run": 1})).unwrap_err().contains("not available"));
        assert!(call(&c, &other, "checkpoint_diff", json!({"task_id": "t"})).unwrap_err().contains("own task"));
        assert!(call(&c, &read_only, "checkpoint_diff", json!({"task_id": "t"})).unwrap_err().contains("not available"));

        // The full chat gets a check and the block that asks the person; nothing changes.
        let asked = call(&c, &full, "undo_run", json!({"task_id": "t", "run": 1})).unwrap();
        assert_eq!(asked["can_undo"], true);
        assert_eq!(asked["confirm_with"], json!({"task_id": "t", "kind": "undo_run", "text": "1"}));
        assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "two\n", "the tool alone never changes files");
        // A refusal comes back as an answer with its reason.
        a_run(&c, &worktree, "three\n");
        let refused = call(&c, &full, "undo_run", json!({"task_id": "t", "run": 1})).unwrap();
        assert_eq!(refused["can_undo"], false);
        assert_eq!(refused["reason"], "later_runs");
        let back = call(&c, &full, "undo_run", json!({"task_id": "t", "run": 1, "back_to_here": true})).unwrap();
        assert_eq!(back["confirm_with"]["text"], "1 back");

        // The person's confirmation is the existing chat action; its text round-trips.
        let action = neko_protocol::workbench::ChatAction {
            id: "a".into(), task_id: "t".into(), kind: "undo_run".into(), text: "1 back".into(),
            status: neko_protocol::workbench::ChatActionStatus::Proposed, note: String::new(),
        };
        let command = neko_core::chief_of_staff::command(&action).unwrap();
        c.command(command).unwrap();
        assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\n");
    }
}

//! Keeps one worktree per *running* ticket instead of one per ticket ever.
//!
//! A sweep parks tickets no agent is working on: their work goes onto their
//! own branch and the folder is removed (`native_runner::parking`). Anything
//! that needs the folder again calls `ensure_restored` first. Both happen
//! under one lock, and the sweep skips tickets with a live worker or an
//! in-flight publish, so a folder is never removed while something uses it.

use super::*;
use neko_core::native_runner::parking;
use std::collections::HashSet;

/// How long a ticket must sit untouched before it is parked, so a worker
/// that is about to be readmitted (a repair, a reply) keeps its folder.
const QUIET_MS: i64 = 2 * 60 * 1000;
/// Parking is a few Git commands each; bound one pass anyway.
const PER_PASS: usize = 5;
const SWEEP_EVERY: Duration = Duration::from_secs(60);

static LOCK: Mutex<()> = Mutex::new(());
static IN_USE: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Holds a ticket's folder for the life of the guard: the sweep will not park
/// it. Taken by work that runs outside an admitted worker (publishing).
pub(super) struct InUse(String);

impl InUse {
    pub(super) fn take(task_id: &str) -> Self {
        IN_USE.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert_with(HashSet::new).insert(task_id.to_owned());
        Self(task_id.to_owned())
    }
}

impl Drop for InUse {
    fn drop(&mut self) {
        if let Some(set) = IN_USE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut() {
            set.remove(&self.0);
        }
    }
}

fn in_use(task_id: &str) -> bool {
    IN_USE.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref().is_some_and(|set| set.contains(task_id))
}

/// Statuses with no agent working: the folder is only evidence until the
/// ticket runs again.
fn idle(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::ReadyForReview | TaskStatus::AwaitingApproval | TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
}

/// Parent and child tickets of a split share folders (children check out
/// from the parent and integrate back into it), so they are left alone.
fn in_split(snapshot: &Snapshot, id: &str) -> bool {
    snapshot.splits.iter().any(|s| s.parent_id == id || s.subtasks.iter().any(|p| p.task_id.as_deref() == Some(id)))
}

fn record(db: &Mutex<Db>, id: &str, parked: Option<ParkedWork>, message: &str) -> Result<(), String> {
    let db = db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut snapshot = store::load(&db)?;
    let task = snapshot.tasks.iter_mut().find(|t| t.id == id).ok_or("Task no longer exists")?;
    task.parked = parked;
    store::append_event(task, "host", message);
    store::save(&db, &snapshot)
}

/// Brings a parked ticket's folder back before anything uses it. A ticket
/// that is not parked is left exactly as it is.
pub(super) fn ensure_restored(db: &Mutex<Db>, task_id: &str, cancel: &AtomicBool) -> Result<(), String> {
    let _lock = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let snapshot = store::load(&db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let Some(task) = snapshot.tasks.iter().find(|t| t.id == task_id) else { return Ok(()) };
    let (Some(parked), Some(path)) = (&task.parked, &task.worktree) else { return Ok(()) };
    let path = std::path::Path::new(path);
    parking::restore_worktree(
        std::path::Path::new(&parked.repository),
        path,
        &parking::Parked { branch: parked.branch.clone(), wip: parked.wip },
        cancel,
    )?;
    record(db, task_id, None, &format!("Restored its working folder from branch {}", parked.branch))
}

impl Controller {
    pub(super) fn start_parking(self: &Arc<Self>) {
        let controller = self.clone();
        std::thread::Builder::new()
            .name("neko-parking".into())
            .spawn(move || loop {
                std::thread::sleep(SWEEP_EVERY);
                if admission::load_average().is_some_and(|load| load >= 20.0) {
                    continue;
                }
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| controller.parking_tick())) {
                    Ok(Err(error)) => eprintln!("neko parking: {error}"),
                    Err(_) => eprintln!("neko parking: pass panicked; continuing"),
                    Ok(Ok(_)) => {}
                }
            })
            .expect("spawn parking thread");
    }

    /// Parks up to `PER_PASS` idle tickets. Returns how many were parked.
    pub(super) fn parking_tick(&self) -> Result<usize, String> {
        self.parking_pass(&neko_protocol::support_dir().join("task-worktrees"))
    }

    /// Only folders under `root` (Neko's own task worktrees) are ever parked.
    pub(super) fn parking_pass(&self, root: &std::path::Path) -> Result<usize, String> {
        let _lock = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let snapshot = store::load(&self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
        let active: HashSet<String> = self.active.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter().map(|a| a.task_id.clone()).collect();
        let root = root.canonicalize().ok();
        let now = store::now_ms();
        let candidates = snapshot.tasks.iter().filter(|t| {
            t.parked.is_none()
                && idle(t.status)
                && now - t.updated_at_ms >= QUIET_MS
                && !active.contains(&t.id)
                && !in_use(&t.id)
                && !in_split(&snapshot, &t.id)
                && t.worktree.as_deref().is_some_and(|w| {
                    let path = std::path::Path::new(w);
                    path.is_dir() && root.as_ref().is_some_and(|r| path.canonicalize().is_ok_and(|p| p.starts_with(r)))
                })
        });
        let mut parked = 0;
        for task in candidates.take(PER_PASS) {
            let path = std::path::PathBuf::from(task.worktree.as_deref().unwrap_or_default());
            let cancel = AtomicBool::new(false);
            let source = match native_runner::publish::source_checkout(&path, &cancel) {
                Ok(source) => source,
                Err(error) => { eprintln!("neko parking {}: {error}", task.id); continue; }
            };
            match parking::park_worktree(&path, &cancel) {
                Ok(saved) => {
                    let message = if saved.wip {
                        format!("Parked: its unfinished work is saved on branch {} and its folder is removed until it runs again", saved.branch)
                    } else {
                        format!("Parked: its folder is removed until it runs again; the work is on branch {}", saved.branch)
                    };
                    record(&self.db, &task.id, Some(ParkedWork {
                        branch: saved.branch,
                        repository: source.to_string_lossy().into_owned(),
                        wip: saved.wip,
                    }), &message)?;
                    parked += 1;
                }
                Err(error) => eprintln!("neko parking {}: {error}", task.id),
            }
        }
        Ok(parked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &std::path::Path, args: &[&str]) {
        let out = std::process::Command::new("git").current_dir(dir)
            .args(["-c", "user.name=T", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn an_idle_ticket_is_parked_and_comes_back_when_it_runs_again() {
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
        std::fs::write(worktree.join("a.txt"), "unfinished\n").unwrap();

        let controller = super::super::tests::controller_with_task(TaskStatus::Failed);
        {
            let db = controller.db.lock().unwrap();
            let mut snapshot = store::load(&db).unwrap();
            snapshot.tasks[0].worktree = Some(worktree.to_string_lossy().into_owned());
            store::save(&db, &snapshot).unwrap();
        }

        controller.active.lock().unwrap().push(Active { task_id: "t".into(), cancelled: Arc::new(AtomicBool::new(false)) });
        assert_eq!(controller.parking_pass(&root).unwrap(), 0, "a ticket with a live worker keeps its folder");
        controller.active.lock().unwrap().clear();
        {
            let _held = InUse::take("t");
            assert_eq!(controller.parking_pass(&root).unwrap(), 0, "a ticket being published keeps its folder");
        }

        assert_eq!(controller.parking_pass(&root).unwrap(), 1);
        assert!(!worktree.exists());
        let parked = store::load(&controller.db.lock().unwrap()).unwrap().tasks[0].parked.clone().unwrap();
        assert_eq!(parked.branch, "codex/neko-t");
        assert!(parked.wip);

        ensure_restored(&controller.db, "t", &AtomicBool::new(false)).unwrap();
        assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "unfinished\n");
        assert!(store::load(&controller.db.lock().unwrap()).unwrap().tasks[0].parked.is_none());
        ensure_restored(&controller.db, "t", &AtomicBool::new(false)).unwrap();
    }
}

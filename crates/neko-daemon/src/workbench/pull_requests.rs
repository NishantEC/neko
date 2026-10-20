//! Neko's pull request watch: after a task opens a pull request, Neko keeps
//! reading it on GitHub and acts on what changed (`neko_core::pull_requests`
//! decides; this runs `gh` outside the storage lock and saves the outcome).
//! It also proposes the cross-repository follow-ups agents ask for.
//!
//! Merging and closing touch GitHub on your behalf, so each needs your
//! click (`SetMergeWhenReady`, `ClosePullRequest`); the watch never does
//! either on its own.
use super::*;
use neko_core::pull_requests::{self as prs, Next};
use std::sync::atomic::AtomicBool as Flag;

/// How often every open pull request is read. Each read is one `gh` call;
/// at this pace forty open pull requests cost about 800 API calls an hour,
/// well inside GitHub's 5,000.
const EVERY: Duration = Duration::from_secs(180);
const GH_TIMEOUT: Duration = Duration::from_secs(30);
const MERGE_TIMEOUT: Duration = Duration::from_secs(90);

/// Set by `CheckPullRequests` to run a pass now.
static WAKE: Flag = Flag::new(false);

pub(super) fn wake() {
    WAKE.store(true, Ordering::Release);
}

impl Controller {
    pub(super) fn start_pull_requests(self: &Arc<Self>) {
        let controller = self.clone();
        std::thread::Builder::new()
            .name("neko-pull-requests".into())
            .spawn(move || {
                // Let startup recovery and the first admission pass go first.
                std::thread::sleep(Duration::from_secs(20));
                loop {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| controller.pull_request_pass())) {
                        Ok(Err(error)) => eprintln!("neko pull requests: {error}"),
                        Err(_) => eprintln!("neko pull requests: pass panicked; continuing"),
                        Ok(Ok(())) => {}
                    }
                    let started = std::time::Instant::now();
                    while started.elapsed() < EVERY && !WAKE.swap(false, Ordering::AcqRel) {
                        std::thread::sleep(Duration::from_secs(2));
                    }
                }
            })
            .expect("spawn pull request thread");
    }

    /// One pass: propose follow-ups, then read each open pull request.
    pub(super) fn pull_request_pass(&self) -> Result<(), String> {
        self.propose_follow_ups()?;
        let snapshot = store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
        let watched: Vec<(String, String)> = snapshot.tasks.iter()
            .filter(|t| !matches!(t.status, TaskStatus::Cancelled))
            .filter_map(|t| t.pull_request.as_ref().filter(|p| p.is_open()).map(|p| (t.id.clone(), p.url.clone())))
            .collect();
        let cancel = AtomicBool::new(false);
        for (task_id, url) in watched {
            if let Err(error) = self.watch_one(&task_id, &url, &cancel) {
                eprintln!("neko pull requests: {url}: {error}");
            }
        }
        Ok(())
    }

    fn watch_one(&self, task_id: &str, url: &str, cancel: &AtomicBool) -> Result<(), String> {
        let (owner, name, number) = prs::locate(url).ok_or("Not a GitHub pull request URL")?;
        let observed = prs::parse(&native_runner::publish::gh(&[
            "api", "graphql", "-F", &format!("owner={owner}"), "-F", &format!("name={name}"),
            "-F", &format!("number={number}"), "-f", &format!("query={}", prs::QUERY),
        ], cancel, GH_TIMEOUT)?)?;
        let working = self.active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().any(|a| a.task_id == task_id)
            || publish::in_flight_now(task_id);
        let next = {
            let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
            let mut snapshot = store::load(&db)?;
            let admin = snapshot.task_roots.get(task_id)
                .and_then(|root| std::path::Path::new(root).canonicalize().ok())
                .is_some_and(|root| snapshot.merge_without_review.contains(&*root.to_string_lossy()));
            let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == task_id) else { return Ok(()) };
            if task.pull_request.as_ref().map(|p| p.url.as_str()) != Some(url) { return Ok(()) }
            let (status_before, events_before) = (task.status, task.events.len());
            let next = prs::reconcile(task, &observed, working, admin);
            let decided = task.status != status_before || task.events.len() != events_before || !matches!(next, Next::Nothing);
            match &next {
                Next::HandBack(note) => {
                    store::append_event(task, prs::ROLE, "Handed the pull request back to its agent (see the note below).");
                    prs::hand_back(&mut snapshot, task_id, note)?;
                }
                Next::MergeBlocked(why) => {
                    store::append_event(task, prs::ROLE, &format!("Not merged: {why} The merge request is cancelled; ask again once it is fixed."));
                }
                Next::Merge { .. } | Next::UpdateBranch { .. } | Next::Nothing => {}
            }
            if decided {
                task_touch(&mut snapshot, task_id);
                store::save(&db, &snapshot)?;
            } else {
                // Only what GitHub reports changed (checks moving from running
                // to passing): progress, not a decision, so it is deferred
                // rather than rewriting the multi-megabyte task store.
                store::save_deferred(&db, &snapshot)?;
            }
            next
        };
        match next {
            Next::Merge { head, admin } => self.merge(task_id, url, &head, admin, cancel),
            Next::UpdateBranch { head } => {
                // GitHub merges the base into the branch itself (no force push),
                // only if the head is still the one judged.
                let outcome = native_runner::publish::gh(&["api", "-X", "PUT", &format!("repos/{owner}/{name}/pulls/{number}/update-branch"),
                    "-f", &format!("expected_head_sha={head}")], cancel, GH_TIMEOUT);
                if let Err(error) = outcome {
                    self.note(task_id, &format!("Couldn't bring the pull request up to date: {error}"))?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn note(&self, task_id: &str, message: &str) -> Result<(), String> {
        let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
        let mut snapshot = store::load(&db)?;
        if let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == task_id) {
            store::append_event(task, prs::ROLE, message);
            task.updated_at_ms = store::now_ms();
        }
        store::save(&db, &snapshot)
    }

    /// Squash-merges exactly the head commit it judged (`--match-head-commit`,
    /// so a push in between can't slip in unchecked) with an empty body, as
    /// you asked, and deletes the branch. Skips GitHub's required review
    /// only where you allowed it for that repository.
    fn merge(&self, task_id: &str, url: &str, head: &str, admin: bool, cancel: &AtomicBool) -> Result<(), String> {
        let mut args = vec!["pr", "merge", url, "--squash", "--body", "", "--match-head-commit", head, "--delete-branch"];
        if admin { args.push("--admin"); }
        let outcome = native_runner::publish::gh(&args, cancel, MERGE_TIMEOUT);
        let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
        let mut snapshot = store::load(&db)?;
        let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == task_id) else { return Ok(()) };
        if let Some(pr) = task.pull_request.as_mut() { pr.merge_when_ready = false; pr.follow_up.merge_note = None; }
        match outcome {
            Ok(_) => {
                if let Some(status) = task.pull_request.as_mut().and_then(|p| p.status.as_mut()) { status.state = "MERGED".into(); }
                task.status = TaskStatus::Completed;
                store::append_event(task, prs::ROLE, if admin {
                    "Merged the pull request (squashed, empty body) and deleted its branch, without waiting for an approving review as this repository allows."
                } else { "Merged the pull request (squashed, empty body) and deleted its branch." });
            }
            Err(error) => store::append_event(task, prs::ROLE, &format!("Couldn't merge the pull request: {error}")),
        }
        task.updated_at_ms = store::now_ms();
        store::save(&db, &snapshot)
    }

    /// Closes a pull request on GitHub with your comment and cancels its task.
    /// Runs in the background; the outcome arrives as task activity.
    pub(super) fn close_pull_request(&self, task_id: String, comment: String) -> Result<Snapshot, String> {
        let comment = comment.trim().to_owned();
        if comment.is_empty() {
            return Err("Say why it is being closed; the comment goes on the pull request".into());
        }
        let snapshot = store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
        let url = snapshot.tasks.iter().find(|t| t.id == task_id).ok_or("Task no longer exists")?
            .pull_request.as_ref().filter(|p| p.is_open()).ok_or("This task has no open pull request")?.url.clone();
        let db = self.db.clone();
        std::thread::spawn(move || {
            let outcome = native_runner::publish::gh(&["pr", "close", &url, "--comment", &comment], &AtomicBool::new(false), GH_TIMEOUT);
            let Ok(db) = db.lock() else { return };
            let Ok(mut snapshot) = store::load(&db) else { return };
            let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == task_id) else { return };
            match outcome {
                Ok(_) => {
                    if let Some(pr) = task.pull_request.as_mut() {
                        pr.merge_when_ready = false;
                        pr.status.get_or_insert_with(Default::default).state = "CLOSED".into();
                    }
                    task.status = TaskStatus::Cancelled;
                    store::append_event(task, prs::ROLE, &format!("Closed the pull request: {comment}"));
                }
                Err(error) => store::append_event(task, prs::ROLE, &format!("Couldn't close the pull request: {error}")),
            }
            task.updated_at_ms = store::now_ms();
            if let Err(error) = store::save(&db, &snapshot) { eprintln!("neko pull requests: {error}"); }
        });
        Ok(snapshot)
    }

    fn propose_follow_ups(&self) -> Result<(), String> {
        // Scanning folders for checkouts reads the disk, so it happens
        // before taking the storage lock, for the workspaces that need it.
        let wanted: std::collections::HashMap<String, Vec<String>> = {
            let snapshot = store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
            let workspaces: std::collections::HashSet<&str> = snapshot.tasks.iter()
                .filter(|t| t.result.contains("FOLLOW_UP:")).map(|t| t.workspace_id.as_str()).collect();
            snapshot.workspaces.iter().filter(|w| workspaces.contains(w.id.as_str()))
                .map(|w| (w.id.clone(), snapshot.folders_for(w))).collect()
        };
        if wanted.is_empty() { return Ok(()) }
        let cancel = AtomicBool::new(false);
        let found: std::collections::HashMap<String, Vec<store::RepositoryCandidate>> = wanted.into_iter()
            .map(|(id, folders)| (id, store::discover_repositories(&folders, &cancel))).collect();
        let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
        let mut snapshot = store::load(&db)?;
        if prs::propose_follow_ups(&mut snapshot, |workspace| found.get(workspace).cloned().unwrap_or_default())? {
            store::save(&db, &snapshot)?;
        }
        Ok(())
    }
}

fn task_touch(snapshot: &mut Snapshot, task_id: &str) {
    if let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == task_id) {
        task.updated_at_ms = store::now_ms();
    }
}

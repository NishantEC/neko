//! Delivering a reviewed ticket. Neko does this on its own as soon as an
//! independent review passes, and again when asked. Repositories listed in
//! `Snapshot::land_on_main` fast-forward local `main` (nothing pushed);
//! every other repository gets a pull request, which Neko never merges.
//!
//! Network work runs outside the storage lock. A ticket that is no longer
//! ready for review when the push is about to happen is left alone.
use super::*;
use std::collections::HashSet;
use std::sync::OnceLock;

const MAX_BODY: usize = 12 * 1024;

fn in_flight() -> &'static Mutex<HashSet<String>> {
    static IN_FLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(Default::default)
}

fn publishable(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::ReadyForReview | TaskStatus::Completed)
}

/// Publish one ticket's reviewed worktree and record what happened as ticket
/// activity. Failures are reported on the ticket, never by failing the task:
/// the local result is still there for the person to accept.
pub(super) fn publish_task(db: &Arc<Mutex<Db>>, task_id: &str, cancel: &AtomicBool) -> Result<(), String> {
    if !in_flight().lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(task_id.to_owned()) {
        return Ok(());
    }
    let outcome = publish_unlocked(db, task_id, cancel);
    in_flight().lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(task_id);
    let main = native_runner::publish::LANDING_BRANCH;
    let message = match &outcome {
        Ok(Some(Delivered::PullRequest(published))) if published.created => format!("Opened a pull request: {}", published.url),
        Ok(Some(Delivered::PullRequest(published))) => format!("Pushed the reviewed changes to the open pull request: {}", published.url),
        Ok(Some(Delivered::Landed(landed))) => format!(
            "Landed on {main} as {}{}. Nothing was pushed.",
            &landed.commit[..landed.commit.len().min(10)],
            if landed.rebased { ", rebased onto newer work first (the review covered the change before the rebase)" } else { "" }
        ),
        Ok(None) => "Nothing new to deliver since the last time.".into(),
        Err(Failure { landing: true, error }) => format!("Couldn't land on {main}: {error}. The reviewed result stays in its worktree."),
        Err(Failure { error, .. }) => format!("Couldn't open a pull request: {error}. The reviewed result stays local."),
    };
    let db = db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut snapshot = store::load(&db)?;
    let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == task_id) else { return Ok(()) };
    match &outcome {
        Ok(Some(Delivered::PullRequest(published))) => {
            task.pull_request = Some(PullRequest { url: published.url.clone(), branch: published.branch.clone() });
        }
        Ok(Some(Delivered::Landed(landed))) => {
            task.landed_commit = Some(landed.commit.clone());
            // Keep Changes meaningful after a rebase onto newer work.
            task.base_commit = Some(landed.onto.clone());
        }
        _ => {}
    }
    store::append_event(task, "supervisor", &message);
    task.updated_at_ms = store::now_ms();
    store::save(&db, &snapshot)
}

enum Delivered {
    PullRequest(native_runner::publish::Published),
    Landed(native_runner::publish::Landed),
}

struct Failure {
    landing: bool,
    error: String,
}

impl From<String> for Failure {
    fn from(error: String) -> Self {
        Self { landing: false, error }
    }
}
impl From<&str> for Failure {
    fn from(error: &str) -> Self {
        error.to_owned().into()
    }
}

fn publish_unlocked(db: &Arc<Mutex<Db>>, task_id: &str, cancel: &AtomicBool) -> Result<Option<Delivered>, Failure> {
    let snapshot = store::load(&db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let task = snapshot.tasks.iter().find(|t| t.id == task_id).ok_or("That ticket no longer exists")?;
    if !publishable(task.status) {
        return Err("Only a reviewed result can be published".into());
    }
    if snapshot.splits.iter().any(|s| s.subtasks.iter().any(|p| p.task_id.as_deref() == Some(task_id))) {
        return Err("Subtasks are published together with their parent ticket".into());
    }
    let directory = owned_worktree(task)?;
    let base = match &task.base_commit {
        Some(base) => base.clone(),
        None => native_runner::head(&directory, cancel)?,
    };
    let still_reviewed = || {
        store::load(&db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
            .ok()
            .and_then(|s| s.tasks.into_iter().find(|t| t.id == task_id))
            .is_some_and(|t| publishable(t.status))
    };
    let source = native_runner::publish::source_checkout(&directory, cancel)?;
    if snapshot.land_on_main.contains(&*source.to_string_lossy()) {
        return native_runner::publish::land(&directory, &base, &task.title, &still_reviewed, cancel)
            .map(|landed| landed.map(Delivered::Landed))
            .map_err(|error| Failure { landing: true, error });
    }
    Ok(native_runner::publish::publish(&directory, &base, &task.title, &body(task), &still_reviewed, cancel)?.map(Delivered::PullRequest))
}

/// Only Neko's own task worktrees are ever committed to and pushed.
fn owned_worktree(task: &Task) -> Result<std::path::PathBuf, String> {
    let worktree = task.worktree.as_deref().ok_or("This ticket has no worktree yet")?;
    let directory = std::path::Path::new(worktree).canonicalize().map_err(|_| "The ticket's worktree is missing")?;
    let root = neko_protocol::support_dir().join("task-worktrees");
    if !root.canonicalize().is_ok_and(|r| directory.starts_with(r)) {
        return Err("Only Neko's own task worktrees can be published".into());
    }
    Ok(directory)
}

/// What the agent reported doing plus the reviewer's summary and the checks
/// it ran. The ticket's source text is not copied into the pull request.
fn body(task: &Task) -> String {
    let (work, review) = task.result.rsplit_once("\n\nIndependent review:\n").unwrap_or((&task.result, ""));
    let mut body = String::new();
    let work = work.trim();
    if !work.is_empty() {
        body.push_str(work);
        body.push_str("\n\n");
    }
    if let Ok(verdict) = serde_json::from_str::<neko_core::verification::Verdict>(review.trim()) {
        body.push_str("### Independent review\n\n");
        body.push_str(verdict.summary.trim());
        body.push('\n');
        if !verdict.tests.is_empty() {
            body.push_str("\nChecks run:\n");
            for test in &verdict.tests {
                body.push_str(&format!("- `{}`\n", test.replace('`', "'")));
            }
        }
        body.push('\n');
    }
    body.push_str("---\nOpened by Neko after an independent review passed. Not merged.\n");
    if body.len() > MAX_BODY {
        let mut end = MAX_BODY;
        while !body.is_char_boundary(end) { end -= 1; }
        body.truncate(end);
        body.push_str("\n… (shortened)\n");
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(result: &str) -> Task {
        Task {
            id: "t1".into(), workspace_id: "w".into(), issue_id: None, title: "Fix".into(),
            goal: "SECRET source text".into(), status: TaskStatus::ReadyForReview, plan: String::new(),
            result: result.into(), worktree: None, events: vec![], created_at_ms: 0, updated_at_ms: 0,
            source_revision: None, supervision: None, base_commit: None, pull_request: None, landed_commit: None,
        }
    }

    #[test]
    fn body_carries_the_work_and_review_but_not_the_source_text() {
        let review = r#"{"passed":true,"findings":[],"files":["a.rs"],"tests":["cargo test `x`"],"summary":"Looks right."}"#;
        let body = body(&task(&format!("Changed a.rs to fix the crash.\n\nIndependent review:\n{review}")));
        assert!(body.starts_with("Changed a.rs to fix the crash.\n\n### Independent review\n\nLooks right.\n"), "{body}");
        assert!(body.contains("- `cargo test 'x'`"), "{body}");
        assert!(body.ends_with("Not merged.\n"));
        assert!(!body.contains("SECRET"));
    }

    #[test]
    fn only_reviewed_results_publish() {
        assert!(publishable(TaskStatus::ReadyForReview) && publishable(TaskStatus::Completed));
        for status in [TaskStatus::Queued, TaskStatus::Planning, TaskStatus::AwaitingApproval, TaskStatus::Building, TaskStatus::Reviewing, TaskStatus::Failed, TaskStatus::Cancelled] {
            assert!(!publishable(status), "{status:?}");
        }
    }
}

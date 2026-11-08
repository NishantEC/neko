//! "Done, not yet seen" (herdr's unseen `done`, docs/research/herdr.md): a
//! task that finished (Ready to review, Completed or Failed) since the person
//! last opened it carries a small dot in the web app until they open it.
//!
//! Kept in one quiet settings row (`task_seen_v1`), outside the workbench
//! snapshot, so marking a task seen never rewrites the snapshot or wakes held
//! `WorkbenchIfChanged` requests; every window reads the same row. Per task it
//! records the status the person last saw it in. A finished task is unseen
//! when there is no record or the record names another status. Rules:
//!
//! - The first read records every task already finished as seen, so turning
//!   this on does not flag the whole history.
//! - Seen "Ready to review" also covers "Completed" (the person reviewed it;
//!   merging or marking done is their own act).
//! - A record for a task that is running again is dropped on the next read,
//!   so a task that fails, is started again and fails again is flagged again.
//! - Marking unread stores an empty status, which matches nothing.
use crate::Db;
use neko_protocol::workbench::{Task, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEY: &str = "task_seen_v1";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stored {
    /// Task id → the status the person last saw it in ("" = marked unread).
    #[serde(default)]
    seen: BTreeMap<String, String>,
}

fn finished(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::ReadyForReview | TaskStatus::Completed | TaskStatus::Failed)
}

fn name(status: TaskStatus) -> String {
    serde_json::to_value(status).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

fn covers(seen: &str, status: TaskStatus) -> bool {
    seen == name(status) || (status == TaskStatus::Completed && seen == "ReadyForReview")
}

fn read(db: &Db) -> Result<Option<Stored>, String> {
    match db.get_setting(KEY).map_err(|e| e.to_string())? {
        Some(text) => Ok(Some(serde_json::from_str(&text).unwrap_or_default())),
        None => Ok(None),
    }
}

fn write(db: &Db, stored: &Stored) -> Result<(), String> {
    let text = serde_json::to_string(stored).map_err(|e| e.to_string())?;
    db.set_setting_quiet(KEY, &text).map_err(|e| e.to_string())
}

/// The stored records, created (every finished task seen) on first use, with
/// records for tasks that are gone or running again dropped. Writes only when
/// that changed something.
fn current(db: &Db, tasks: &[Task]) -> Result<Stored, String> {
    let Some(mut stored) = read(db)? else {
        let stored = Stored { seen: tasks.iter().filter(|t| finished(t.status)).map(|t| (t.id.clone(), name(t.status))).collect() };
        write(db, &stored)?;
        return Ok(stored);
    };
    let before = stored.seen.len();
    // Cancelled keeps its record (nothing to flag); anything back in the queue or running loses it.
    let by_id: BTreeMap<&str, TaskStatus> = tasks.iter().map(|t| (t.id.as_str(), t.status)).collect();
    stored.seen.retain(|id, _| by_id.get(id.as_str()).is_some_and(|s| finished(*s) || *s == TaskStatus::Cancelled));
    if stored.seen.len() != before {
        write(db, &stored)?;
    }
    Ok(stored)
}

fn unseen_in(stored: &Stored, tasks: &[Task]) -> Vec<String> {
    tasks
        .iter()
        .filter(|t| finished(t.status) && !stored.seen.get(&t.id).is_some_and(|s| covers(s, t.status)))
        .map(|t| t.id.clone())
        .collect()
}

/// Ids of the finished tasks the person has not opened since they finished.
pub fn unseen(db: &Db, tasks: &[Task]) -> Result<Vec<String>, String> {
    Ok(unseen_in(&current(db, tasks)?, tasks))
}

/// Records that the person opened the task (`seen`), or asked to keep it
/// flagged (`!seen`). Returns the unseen ids after the change.
pub fn mark(db: &Db, tasks: &[Task], task_id: &str, seen: bool) -> Result<Vec<String>, String> {
    let task = tasks.iter().find(|t| t.id == task_id).ok_or("That task no longer exists")?;
    let mut stored = current(db, tasks)?;
    let value = if seen { name(task.status) } else { String::new() };
    if stored.seen.get(task_id) != Some(&value) {
        stored.seen.insert(task_id.to_owned(), value);
        write(db, &stored)?;
    }
    Ok(unseen_in(&stored, tasks))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Built through serde so a field added to `Task` with a default does not break these tests.
    fn task(id: &str, status: TaskStatus) -> Task {
        serde_json::from_value(serde_json::json!({
            "id": id, "workspace_id": "w", "issue_id": null, "title": format!("Task {id}"), "goal": "",
            "status": status, "plan": "", "result": "", "worktree": null, "events": [],
            "created_at_ms": 0, "updated_at_ms": 0, "source_revision": null, "base_commit": null,
            "pull_request": null, "landed_commit": null, "supervision": null, "display_title": null, "parked": null,
        }))
        .unwrap()
    }

    #[test]
    fn first_read_counts_the_history_as_seen() {
        let db = Db::open_in_memory().unwrap();
        let tasks = vec![task("a", TaskStatus::Completed), task("b", TaskStatus::Building), task("c", TaskStatus::Failed)];
        assert!(unseen(&db, &tasks).unwrap().is_empty());
        assert!(db.get_setting(KEY).unwrap().is_some(), "the baseline is stored");
        // b finishes after the baseline: unseen until opened.
        let tasks = vec![task("a", TaskStatus::Completed), task("b", TaskStatus::ReadyForReview), task("c", TaskStatus::Failed)];
        assert_eq!(unseen(&db, &tasks).unwrap(), vec!["b".to_string()]);
        assert!(mark(&db, &tasks, "b", true).unwrap().is_empty());
        assert!(unseen(&db, &tasks).unwrap().is_empty());
    }

    #[test]
    fn a_new_finished_status_is_unseen_but_review_covers_completed() {
        let db = Db::open_in_memory().unwrap();
        let mut tasks = vec![task("a", TaskStatus::ReadyForReview), task("b", TaskStatus::Failed)];
        assert!(unseen(&db, &tasks).unwrap().is_empty());
        tasks[0].status = TaskStatus::Completed;
        assert!(unseen(&db, &tasks).unwrap().is_empty(), "seen in review covers completed");
        tasks[1].status = TaskStatus::ReadyForReview;
        assert_eq!(unseen(&db, &tasks).unwrap(), vec!["b".to_string()], "failed then ready to review is news");
    }

    #[test]
    fn running_again_forgets_the_record() {
        let db = Db::open_in_memory().unwrap();
        let mut tasks = vec![task("a", TaskStatus::Failed)];
        assert!(unseen(&db, &tasks).unwrap().is_empty());
        tasks[0].status = TaskStatus::Building;
        assert!(unseen(&db, &tasks).unwrap().is_empty());
        tasks[0].status = TaskStatus::Failed;
        assert_eq!(unseen(&db, &tasks).unwrap(), vec!["a".to_string()], "failing again after a restart is flagged again");
    }

    #[test]
    fn mark_unread_and_unknown_tasks() {
        let db = Db::open_in_memory().unwrap();
        let tasks = vec![task("a", TaskStatus::Completed), task("q", TaskStatus::Queued)];
        assert!(unseen(&db, &tasks).unwrap().is_empty());
        assert_eq!(mark(&db, &tasks, "a", false).unwrap(), vec!["a".to_string()]);
        assert_eq!(unseen(&db, &tasks).unwrap(), vec!["a".to_string()]);
        assert!(mark(&db, &tasks, "a", true).unwrap().is_empty());
        assert!(mark(&db, &tasks, "gone", true).is_err());
        // A queued task is never flagged, seen or not.
        assert!(mark(&db, &tasks, "q", false).unwrap().is_empty());
    }

    #[test]
    fn deleted_tasks_are_pruned_and_reads_do_not_rewrite() {
        let db = Db::open_in_memory().unwrap();
        let tasks = vec![task("a", TaskStatus::Completed), task("b", TaskStatus::Completed)];
        unseen(&db, &tasks).unwrap();
        let writes = db.setting_writes();
        unseen(&db, &tasks).unwrap();
        assert_eq!(db.setting_writes(), writes, "an unchanged read writes nothing");
        unseen(&db, &tasks[..1]).unwrap();
        let stored: Stored = serde_json::from_str(&db.get_setting(KEY).unwrap().unwrap()).unwrap();
        assert_eq!(stored.seen.keys().collect::<Vec<_>>(), vec!["a"]);
    }
}

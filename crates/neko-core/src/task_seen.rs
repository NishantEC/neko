//! "Done, not yet seen" (herdr's unseen `done`, docs/research/herdr.md): a
//! task that finished (Ready to review, Completed or Failed) since the person
//! last opened it carries a small dot in the web app until they open it.
//!
//! Kept in one quiet settings row (`task_seen_v1`), outside the workbench
//! snapshot, so marking a task seen never rewrites the snapshot or wakes held
//! `WorkbenchIfChanged` requests; every window reads the same row. Per task it
//! records the status the person last saw it in and when. A finished task is
//! unseen when there is no record, the record names another status, or an
//! agent worked on the task after the record (it ran again and finished
//! again, whether or not any page was open meanwhile). Rules:
//!
//! - The first read records every task already finished as seen, so turning
//!   this on does not flag the whole history. A row that cannot be read is
//!   treated the same way.
//! - Seen "Ready to review" also covers "Completed" (the person reviewed it;
//!   merging or marking done is their own act), unless an agent worked on it
//!   since.
//! - Only agent work counts as news (scout, builder, reviewer events): a
//!   pull request note or parking the folder does not flag a task again.
//! - Marking unread stores an empty status, which matches nothing.
use crate::Db;
use neko_protocol::workbench::{Task, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEY: &str = "task_seen_v1";

/// Event roles that mean an agent worked on the task.
const WORK_ROLES: [&str; 3] = ["scout", "builder", "reviewer"];

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Seen {
    /// The status the person saw ("" = marked unread).
    status: String,
    at_ms: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    seen: BTreeMap<String, Seen>,
}

fn finished(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::ReadyForReview | TaskStatus::Completed | TaskStatus::Failed)
}

fn name(status: TaskStatus) -> String {
    serde_json::to_value(status).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

/// When an agent last worked on the task (0 if never).
fn last_work_ms(task: &Task) -> i64 {
    task.events.iter().filter(|e| WORK_ROLES.contains(&e.role.as_str())).map(|e| e.at_ms).max().unwrap_or(0)
}

fn is_unseen(seen: Option<&Seen>, task: &Task) -> bool {
    if !finished(task.status) {
        return false;
    }
    let Some(seen) = seen else { return true };
    let same = seen.status == name(task.status) || (task.status == TaskStatus::Completed && seen.status == "ReadyForReview");
    !same || last_work_ms(task) > seen.at_ms
}

fn write(db: &Db, stored: &Stored) -> Result<(), String> {
    let text = serde_json::to_string(stored).map_err(|e| e.to_string())?;
    db.set_setting_quiet(KEY, &text).map_err(|e| e.to_string())
}

/// The stored records, created (every finished task seen) on first use or
/// when the row cannot be read, with records for deleted tasks dropped.
/// Writes only when that changed something.
fn current(db: &Db, tasks: &[Task], now: i64) -> Result<Stored, String> {
    let read = db.get_setting(KEY).map_err(|e| e.to_string())?.and_then(|text| serde_json::from_str::<Stored>(&text).ok());
    let Some(mut stored) = read else {
        let seen = tasks.iter().filter(|t| finished(t.status)).map(|t| (t.id.clone(), Seen { status: name(t.status), at_ms: now })).collect();
        let stored = Stored { seen };
        write(db, &stored)?;
        return Ok(stored);
    };
    let before = stored.seen.len();
    stored.seen.retain(|id, _| tasks.iter().any(|t| &t.id == id));
    if stored.seen.len() != before {
        write(db, &stored)?;
    }
    Ok(stored)
}

fn unseen_in(stored: &Stored, tasks: &[Task]) -> Vec<String> {
    tasks.iter().filter(|t| is_unseen(stored.seen.get(&t.id), t)).map(|t| t.id.clone()).collect()
}

/// Ids of the finished tasks the person has not opened since they finished.
pub fn unseen(db: &Db, tasks: &[Task]) -> Result<Vec<String>, String> {
    unseen_at(db, tasks, crate::workbench::now_ms())
}

fn unseen_at(db: &Db, tasks: &[Task], now: i64) -> Result<Vec<String>, String> {
    Ok(unseen_in(&current(db, tasks, now)?, tasks))
}

/// Records that the person opened the task (`seen`), or asked to keep it
/// flagged (`!seen`). Returns the unseen ids after the change.
pub fn mark(db: &Db, tasks: &[Task], task_id: &str, seen: bool) -> Result<Vec<String>, String> {
    mark_at(db, tasks, task_id, seen, crate::workbench::now_ms())
}

fn mark_at(db: &Db, tasks: &[Task], task_id: &str, seen: bool, now: i64) -> Result<Vec<String>, String> {
    let task = tasks.iter().find(|t| t.id == task_id).ok_or("That task no longer exists")?;
    let mut stored = current(db, tasks, now)?;
    let record = Seen { status: if seen { name(task.status) } else { String::new() }, at_ms: now };
    // Opening it again changes nothing unless an agent worked on it since.
    let unchanged = stored.seen.get(task_id).is_some_and(|old| old.status == record.status && old.at_ms >= last_work_ms(task));
    if !unchanged {
        stored.seen.insert(task_id.to_owned(), record);
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

    fn event(task: &mut Task, role: &str, at_ms: i64) {
        task.events.push(neko_protocol::workbench::TaskEvent { at_ms, role: role.into(), message: "x".into() });
    }

    #[test]
    fn first_read_counts_the_history_as_seen() {
        let db = Db::open_in_memory().unwrap();
        let tasks = vec![task("a", TaskStatus::Completed), task("b", TaskStatus::Building), task("c", TaskStatus::Failed)];
        assert!(unseen_at(&db, &tasks, 100).unwrap().is_empty());
        assert!(db.get_setting(KEY).unwrap().is_some(), "the baseline is stored");
        // b finishes after the baseline: unseen until opened.
        let tasks = vec![task("a", TaskStatus::Completed), task("b", TaskStatus::ReadyForReview), task("c", TaskStatus::Failed)];
        assert_eq!(unseen_at(&db, &tasks, 200).unwrap(), vec!["b".to_string()]);
        assert!(mark_at(&db, &tasks, "b", true, 300).unwrap().is_empty());
        assert!(unseen_at(&db, &tasks, 400).unwrap().is_empty());
    }

    #[test]
    fn a_new_finished_status_is_unseen_but_review_covers_completed() {
        let db = Db::open_in_memory().unwrap();
        let mut tasks = vec![task("a", TaskStatus::ReadyForReview), task("b", TaskStatus::Failed)];
        assert!(unseen_at(&db, &tasks, 100).unwrap().is_empty());
        tasks[0].status = TaskStatus::Completed;
        assert!(unseen_at(&db, &tasks, 200).unwrap().is_empty(), "seen in review covers completed");
        tasks[1].status = TaskStatus::ReadyForReview;
        assert_eq!(unseen_at(&db, &tasks, 300).unwrap(), vec!["b".to_string()], "failed then ready to review is news");
    }

    #[test]
    fn running_again_and_finishing_again_is_news_even_with_no_read_between() {
        let db = Db::open_in_memory().unwrap();
        let mut tasks = vec![task("a", TaskStatus::Failed), task("r", TaskStatus::ReadyForReview)];
        assert!(unseen_at(&db, &tasks, 100).unwrap().is_empty());
        // Started again, failed again, and a review reworked then completed: nobody asked meanwhile.
        event(&mut tasks[0], "builder", 150);
        event(&mut tasks[1], "builder", 160);
        tasks[1].status = TaskStatus::Completed;
        assert_eq!(unseen_at(&db, &tasks, 200).unwrap(), vec!["a".to_string(), "r".to_string()]);
        // A pull request note or a parked folder is not agent work.
        let mut quiet = vec![task("q", TaskStatus::Completed)];
        let db = Db::open_in_memory().unwrap();
        assert!(unseen_at(&db, &quiet, 100).unwrap().is_empty());
        event(&mut quiet[0], "github", 150);
        event(&mut quiet[0], "system", 160);
        assert!(unseen_at(&db, &quiet, 200).unwrap().is_empty());
    }

    #[test]
    fn mark_unread_and_unknown_tasks() {
        let db = Db::open_in_memory().unwrap();
        let tasks = vec![task("a", TaskStatus::Completed), task("q", TaskStatus::Queued)];
        assert!(unseen_at(&db, &tasks, 100).unwrap().is_empty());
        assert_eq!(mark_at(&db, &tasks, "a", false, 200).unwrap(), vec!["a".to_string()]);
        assert_eq!(unseen_at(&db, &tasks, 300).unwrap(), vec!["a".to_string()]);
        assert!(mark_at(&db, &tasks, "a", true, 400).unwrap().is_empty());
        assert!(mark_at(&db, &tasks, "gone", true, 500).is_err());
        // A queued task is never flagged, seen or not.
        assert!(mark_at(&db, &tasks, "q", false, 600).unwrap().is_empty());
    }

    #[test]
    fn a_damaged_row_is_a_new_baseline_not_a_flood() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting_quiet(KEY, "{not json").unwrap();
        let tasks = vec![task("a", TaskStatus::Completed), task("b", TaskStatus::Failed)];
        assert!(unseen_at(&db, &tasks, 100).unwrap().is_empty());
    }

    #[test]
    fn deleted_tasks_are_pruned_and_reads_do_not_rewrite() {
        let db = Db::open_in_memory().unwrap();
        let tasks = vec![task("a", TaskStatus::Completed), task("b", TaskStatus::Completed)];
        unseen_at(&db, &tasks, 100).unwrap();
        let writes = db.setting_writes();
        unseen_at(&db, &tasks, 200).unwrap();
        mark_at(&db, &tasks, "a", true, 300).unwrap();
        assert_eq!(db.setting_writes(), writes, "an unchanged read, or opening a seen task again, writes nothing");
        unseen_at(&db, &tasks[..1], 400).unwrap();
        let stored: Stored = serde_json::from_str(&db.get_setting(KEY).unwrap().unwrap()).unwrap();
        assert_eq!(stored.seen.keys().collect::<Vec<_>>(), vec!["a"]);
    }
}

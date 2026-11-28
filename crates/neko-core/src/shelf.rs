//! Pin, settle, archive (docs/knowledge/threads.md): three ways the person puts a thread where they want it, and the one way Neko
//! puts one away by itself. The rules are here and nowhere else, so the daemon, the tests and the housekeeping agree.
//!
//! - **Pinned** threads sit at the top of the list and are never settled by themselves. Pinning brings a settled thread back, and
//!   settling a pinned one unpins it: a thread is never both.
//! - **Settled** threads wait in their group's Settled fold. New activity (a message, a wake, a run's end) brings them back
//!   (`Store::set_task_status` and the agent's words clear it in the same transaction). After [`AUTO_SETTLE_AFTER_MS`] with nothing
//!   happening the housekeeping settles a thread by itself, unless it is pinned, working or waiting, needs the person, has a report
//!   waiting to wake it, or failed and was not opened.
//! - **Archived** threads sleep, out of the lists. The person's next message in one starts a run and wakes it. Nothing a thread
//!   waits on survives its archiving: a thread that is working, or waits on its subagents, is refused, so there is nothing to stop.
//!   (Timers and watches do not exist yet; when they do, archiving ends them.)

use neko_protocol::{Task, ThreadStatus};
use rusqlite::params;

use crate::records::log_event;
use crate::store::{Store, StoreError};

/// How long a thread must be left alone before the housekeeping settles it: seven days.
pub const AUTO_SETTLE_AFTER_MS: i64 = 7 * 24 * 3_600_000;
/// The longest the housekeeping goes without looking, so a thread that became idle is settled on time (the scheduler sleeps this
/// long at most while any thread could still be settled).
pub const LOOK_EVERY_MS: i64 = 6 * 3_600_000;

/// Which of the three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shelf {
    Pin,
    Settle,
    Archive,
}

/// Why this cannot be done now, in the words a person reads; None when it can. Taking one off (`on` false) is always allowed.
pub fn refusal(t: &Task, shelf: Shelf, on: bool) -> Option<&'static str> {
    if !t.is_thread() {
        return Some("A kitten is not a thread of its own.");
    }
    if t.cattain && on {
        return Some("A Cattain's own thread stays where it is. Retire the Cattain in its Profile instead.");
    }
    if !on {
        return None;
    }
    match (shelf, t.thread_status) {
        (Shelf::Settle, ThreadStatus::Working) => Some("It is working. Settle it when it stops."),
        (Shelf::Archive, ThreadStatus::Working) => Some("It is working. Stop it before archiving it."),
        (Shelf::Archive, ThreadStatus::Waiting) => Some("Its kittens are still working. Stop it before archiving it."),
        _ => None,
    }
}

impl Store {
    /// Puts a thread on or takes it off a shelf, with its event. Returns the task whole. The caller has checked [`refusal`].
    pub fn set_shelf(&mut self, id: &neko_protocol::TaskId, shelf: Shelf, on: bool, now_ms: i64) -> Result<(Task, u64), StoreError> {
        let tx = self.conn.transaction()?;
        let at = on.then_some(now_ms);
        let (sql, kind) = match (shelf, on) {
            (Shelf::Pin, true) => ("UPDATE tasks SET pinned_ms = ?2, settled_ms = NULL WHERE id = ?1", "task_pinned"),
            (Shelf::Pin, false) => ("UPDATE tasks SET pinned_ms = ?2 WHERE id = ?1", "task_unpinned"),
            (Shelf::Settle, true) => ("UPDATE tasks SET settled_ms = ?2, pinned_ms = NULL WHERE id = ?1", "task_settled"),
            (Shelf::Settle, false) => ("UPDATE tasks SET settled_ms = ?2 WHERE id = ?1", "task_unsettled"),
            (Shelf::Archive, _) => ("UPDATE tasks SET archived_ms = ?2 WHERE id = ?1", if on { "task_archived" } else { "task_unarchived" }),
        };
        tx.execute(sql, params![id.0, at])?;
        let seq = log_event(&tx, now_ms, &id.0, kind, serde_json::json!({}))?;
        tx.commit()?;
        let task = self.get_task(id)?.ok_or(StoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
        Ok((task, seq))
    }

    /// The housekeeping: settles every thread that has been left alone for `idle_ms` and may be settled. Returns the threads it
    /// settled (each with its event) and when to look again: at the next thread's time, but no later than [`LOOK_EVERY_MS`]; None
    /// when no thread could be settled at all (so the scheduler need not wake for this).
    pub fn auto_settle(&mut self, now_ms: i64, idle_ms: i64) -> Result<(Vec<(Task, u64)>, Option<i64>), StoreError> {
        // "Left alone": nothing was said, ended or started in it. The person's own message starts a run, so it counts.
        let candidates: Vec<(String, i64)> = {
            let mut st = self.conn.prepare_cached(
                "SELECT id, MAX(created_ms, activity_ms, COALESCE((SELECT MAX(COALESCE(ended_ms, started_ms)) FROM runs WHERE runs.task_id = tasks.id), 0))
                 FROM tasks WHERE parent_id IS NULL AND cattain = 0 AND pinned_ms IS NULL AND settled_ms IS NULL AND archived_ms IS NULL",
            )?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
        };
        let mut settled = Vec::new();
        let mut next: Option<i64> = None;
        for (id, touched) in &candidates {
            let due = touched + idle_ms;
            if due > now_ms {
                next = Some(next.map_or(due, |n| n.min(due)));
                continue;
            }
            let id = neko_protocol::TaskId(id.clone());
            let Some(t) = self.get_task(&id)? else { continue };
            let fine = t.thread_status == ThreadStatus::Idle && !t.needs_you && !self.has_pending_wake(&id)?;
            if fine {
                settled.push(self.set_shelf(&id, Shelf::Settle, true, now_ms)?);
            }
        }
        let wake = (!candidates.is_empty() && settled.len() < candidates.len()).then(|| next.map_or(now_ms + LOOK_EVERY_MS, |n| n.min(now_ms + LOOK_EVERY_MS)));
        Ok((settled, wake))
    }
}

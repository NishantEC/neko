//! The messages waiting above a thread's composer (docs/knowledge/composer.md): words the person sent while the agent was working.
//! Three kinds: queued (the daemon holds them and the next run starts with them), steered (written into the running agent, not read
//! yet) and interrupting (the turn is being stopped; a new one starts with them). The rows are the store's, so a restart keeps what
//! was queued; the daemon decides when each is sent (`crates/neko-daemon/src/queue.rs`).

use neko_protocol::{Queued, QueuedId, QueuedKind, TaskId};
use rusqlite::{OptionalExtension, params};
use serde_json::json;

use crate::records::log_event;
use crate::store::{Store, StoreError};

/// The most messages that may wait above one composer. A guard against a stuck key, not a plan.
pub const MAX_WAITING: usize = 50;

fn kind_name(k: QueuedKind) -> &'static str {
    match k {
        QueuedKind::Queued => "queued",
        QueuedKind::Steered => "steered",
        QueuedKind::Interrupting => "interrupting",
    }
}

fn kind_of(s: &str) -> QueuedKind {
    match s {
        "steered" => QueuedKind::Steered,
        "interrupting" => QueuedKind::Interrupting,
        _ => QueuedKind::Queued,
    }
}

const COLUMNS: &str = "id, task_id, words, kind, at_ms";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Queued> {
    Ok(Queued { id: QueuedId(r.get(0)?), task: TaskId(r.get(1)?), words: r.get(2)?, kind: kind_of(&r.get::<_, String>(3)?), at_ms: r.get(4)? })
}

impl Store {
    /// A thread's waiting messages, oldest first.
    pub fn queued_of(&self, task: &TaskId) -> Result<Vec<Queued>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {COLUMNS} FROM queued WHERE task_id = ?1 ORDER BY rowid"))?;
        let rows = st.query_map([&task.0], row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every waiting message of every thread (a restart settles them).
    pub fn queued_all(&self) -> Result<Vec<Queued>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {COLUMNS} FROM queued ORDER BY rowid"))?;
        let rows = st.query_map([], row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_queued(&self, id: &QueuedId) -> Result<Option<Queued>, StoreError> {
        Ok(self.conn.query_row(&format!("SELECT {COLUMNS} FROM queued WHERE id = ?1"), [&id.0], row).optional()?)
    }

    /// A message starts waiting. Returns the store's revision.
    pub fn insert_queued(&mut self, q: &Queued, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("INSERT INTO queued (id, task_id, words, kind, at_ms) VALUES (?1, ?2, ?3, ?4, ?5)", params![q.id.0, q.task.0, q.words, kind_name(q.kind), q.at_ms])?;
        let seq = log_event(&tx, now_ms, &q.task.0, "queued", json!({ "queued": q.id.0, "kind": kind_name(q.kind) }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// A waiting message changes what it is (an interrupt that could not go through is held, a steer is read). None: it was gone.
    pub fn set_queued_kind(&mut self, id: &QueuedId, kind: QueuedKind, now_ms: i64) -> Result<Option<u64>, StoreError> {
        let tx = self.conn.transaction()?;
        let changed = tx.execute("UPDATE queued SET kind = ?2 WHERE id = ?1", params![id.0, kind_name(kind)])?;
        if changed == 0 {
            return Ok(None);
        }
        let task: String = tx.query_row("SELECT task_id FROM queued WHERE id = ?1", [&id.0], |r| r.get(0))?;
        let seq = log_event(&tx, now_ms, &task, "queued_changed", json!({ "queued": id.0, "kind": kind_name(kind) }))?;
        tx.commit()?;
        Ok(Some(seq))
    }

    /// A waiting message leaves (sent, read, discarded). None: it was already gone.
    pub fn delete_queued(&mut self, id: &QueuedId, why: &str, now_ms: i64) -> Result<Option<u64>, StoreError> {
        let tx = self.conn.transaction()?;
        let task: Option<String> = tx.query_row("SELECT task_id FROM queued WHERE id = ?1", [&id.0], |r| r.get(0)).optional()?;
        let Some(task) = task else { return Ok(None) };
        tx.execute("DELETE FROM queued WHERE id = ?1", [&id.0])?;
        let seq = log_event(&tx, now_ms, &task, "queued_left", json!({ "queued": id.0, "why": why }))?;
        tx.commit()?;
        Ok(Some(seq))
    }

    /// Every message of one kind leaves a thread's list at once (the steers a run read or ended with). None: there were none.
    pub fn delete_queued_kind(&mut self, task: &TaskId, kind: QueuedKind, why: &str, now_ms: i64) -> Result<Option<u64>, StoreError> {
        let tx = self.conn.transaction()?;
        let n = tx.execute("DELETE FROM queued WHERE task_id = ?1 AND kind = ?2", params![task.0, kind_name(kind)])?;
        if n == 0 {
            return Ok(None);
        }
        let seq = log_event(&tx, now_ms, &task.0, "queued_left", json!({ "kind": kind_name(kind), "why": why, "count": n }))?;
        tx.commit()?;
        Ok(Some(seq))
    }

    /// A thread goes: so do its waiting messages.
    pub fn delete_queue_of_task(&mut self, task: &TaskId) -> Result<(), StoreError> {
        self.conn.execute("DELETE FROM queued WHERE task_id = ?1", [&task.0])?;
        Ok(())
    }
}

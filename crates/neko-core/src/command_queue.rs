//! The durable command queue: messages for running agents (send, steer,
//! interrupt, answer) wait in rows of their own, never the workbench
//! snapshot, so they survive a restart and a retry never doubles one.
//! One row per task (`neko_cmdq_v1:<task_id>`) and a small index of the tasks
//! that have any. Delivery is the daemon's (`workbench/crew.rs`); this module
//! only stores and orders. See docs/knowledge/the-chat-directs-the-crew.md.
use crate::db::Db;
use neko_protocol::crew::{DeliveryMode, QueueKind, QueueState, QueuedMessage, MAX_QUEUED_TEXT};

const ROW: &str = "neko_cmdq_v1:";
const INDEX: &str = "neko_cmdq_index_v1";
/// Queued messages one task may hold at once.
pub const MAX_QUEUED_PER_TASK: usize = 32;
/// Settled messages kept per task, for the record.
const KEEP_SETTLED: usize = 12;

fn read<T: for<'de> serde::Deserialize<'de> + Default>(db: &Db, key: &str) -> Result<T, String> {
    match db.get_setting(key).map_err(|e| e.to_string())? {
        Some(text) => serde_json::from_str(&text).map_err(|e| format!("Cannot read {key}: {e}")),
        None => Ok(T::default()),
    }
}

fn write<T: serde::Serialize>(db: &Db, key: &str, value: &T) -> Result<(), String> {
    db.set_setting(key, &serde_json::to_string(value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Every message of one task, oldest first.
pub fn of_task(db: &Db, task_id: &str) -> Result<Vec<QueuedMessage>, String> {
    read(db, &format!("{ROW}{task_id}"))
}

fn save_task(db: &Db, task_id: &str, mut rows: Vec<QueuedMessage>) -> Result<(), String> {
    // Settled ones age out; queued ones never do.
    let settled = rows.iter().filter(|m| m.state != QueueState::Queued).count();
    let mut drop_settled = settled.saturating_sub(KEEP_SETTLED);
    rows.retain(|m| {
        if m.state != QueueState::Queued && drop_settled > 0 {
            drop_settled -= 1;
            false
        } else {
            true
        }
    });
    let mut ids: Vec<String> = read(db, INDEX)?;
    if rows.is_empty() {
        db.delete_setting(&format!("{ROW}{task_id}")).map_err(|e| e.to_string())?;
        if ids.contains(&task_id.to_owned()) {
            ids.retain(|id| id != task_id);
            write(db, INDEX, &ids)?;
        }
        return Ok(());
    }
    write(db, &format!("{ROW}{task_id}"), &rows)?;
    if !ids.contains(&task_id.to_owned()) {
        ids.push(task_id.to_owned());
        write(db, INDEX, &ids)?;
    }
    Ok(())
}

/// Queues a message. The same `(task, id)` never queues twice: a retry gets
/// the entry that is already there, whatever state it has reached.
/// Returns the entry and whether it was already known.
pub fn enqueue(
    db: &Db,
    task_id: &str,
    id: Option<&str>,
    text: &str,
    mode: DeliveryMode,
    kind: QueueKind,
    source: &str,
    now_ms: i64,
) -> Result<(QueuedMessage, bool), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("A message cannot be empty".into());
    }
    if text.len() > MAX_QUEUED_TEXT {
        return Err(format!("A message is limited to {MAX_QUEUED_TEXT} bytes"));
    }
    let mut rows = of_task(db, task_id)?;
    if let Some(id) = id.filter(|id| !id.is_empty()) {
        if id.len() > 128 {
            return Err("A command id is limited to 128 characters".into());
        }
        if let Some(known) = rows.iter().find(|m| m.id == id) {
            return Ok((known.clone(), true));
        }
    }
    if rows.iter().filter(|m| m.state == QueueState::Queued).count() >= MAX_QUEUED_PER_TASK {
        return Err(format!("{MAX_QUEUED_PER_TASK} messages already wait for this agent. Let it catch up first."));
    }
    let entry = QueuedMessage {
        id: id.filter(|id| !id.is_empty()).map(str::to_owned).unwrap_or_else(crate::workbench::new_id),
        task_id: task_id.to_owned(),
        kind,
        mode,
        text: text.to_owned(),
        state: QueueState::Queued,
        source: source.to_owned(),
        created_ms: now_ms,
        settled_ms: 0,
        detail: String::new(),
    };
    rows.push(entry.clone());
    save_task(db, task_id, rows)?;
    Ok((entry, false))
}

/// Moves one entry out of `Queued`. Returns it, or None if it was not queued
/// (already delivered or taken back).
pub fn settle(db: &Db, task_id: &str, id: &str, state: QueueState, detail: &str, now_ms: i64) -> Result<Option<QueuedMessage>, String> {
    let mut rows = of_task(db, task_id)?;
    let Some(entry) = rows.iter_mut().find(|m| m.id == id && m.state == QueueState::Queued) else { return Ok(None) };
    entry.state = state;
    entry.settled_ms = now_ms;
    entry.detail = detail.to_owned();
    let settled = entry.clone();
    save_task(db, task_id, rows)?;
    Ok(Some(settled))
}

/// Adds a note to a queued entry (why it is waiting) without changing its state.
pub fn annotate(db: &Db, task_id: &str, id: &str, detail: &str) -> Result<(), String> {
    let mut rows = of_task(db, task_id)?;
    if let Some(entry) = rows.iter_mut().find(|m| m.id == id && m.state == QueueState::Queued) {
        entry.detail = detail.to_owned();
        save_task(db, task_id, rows)?;
    }
    Ok(())
}

/// Tasks with at least one entry.
pub fn tasks_with_entries(db: &Db) -> Result<Vec<String>, String> {
    read(db, INDEX)
}

/// The queued entries of every task, oldest first per task (the delivery order).
pub fn queued(db: &Db) -> Result<Vec<QueuedMessage>, String> {
    let mut out = Vec::new();
    for task in tasks_with_entries(db)? {
        out.extend(of_task(db, &task)?.into_iter().filter(|m| m.state == QueueState::Queued));
    }
    Ok(out)
}

/// Forgets a deleted task's rows.
pub fn forget_task(db: &Db, task_id: &str) -> Result<(), String> {
    save_task(db, task_id, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn a_retry_with_the_same_id_is_one_entry_even_after_delivery() {
        let db = db();
        let (first, dup) = enqueue(&db, "t", Some("c1"), " hello ", DeliveryMode::AfterTurn, QueueKind::Send, "you", 10).unwrap();
        assert!(!dup);
        assert_eq!(first.text, "hello");
        let (again, dup) = enqueue(&db, "t", Some("c1"), "hello", DeliveryMode::AfterTurn, QueueKind::Send, "you", 11).unwrap();
        assert!(dup && again.id == first.id);
        settle(&db, "t", "c1", QueueState::Delivered, "ok", 12).unwrap();
        let (late, dup) = enqueue(&db, "t", Some("c1"), "hello", DeliveryMode::AfterTurn, QueueKind::Send, "you", 13).unwrap();
        assert!(dup && late.state == QueueState::Delivered, "a retry after delivery does not send it again");
        assert_eq!(of_task(&db, "t").unwrap().len(), 1);
    }

    #[test]
    fn entries_survive_reopening_the_database_and_settle_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("neko.db");
        {
            let db = Db::open(&path).unwrap();
            enqueue(&db, "t", Some("a"), "one", DeliveryMode::SteerNow, QueueKind::Steer, "chat", 1).unwrap();
            enqueue(&db, "t", Some("b"), "two", DeliveryMode::InterruptRestart, QueueKind::Interrupt, "you", 2).unwrap();
        }
        let db = Db::open(&path).unwrap();
        let waiting = queued(&db).unwrap();
        assert_eq!(waiting.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(waiting[0].mode, DeliveryMode::SteerNow);
        assert!(settle(&db, "t", "a", QueueState::Delivered, "", 3).unwrap().is_some());
        assert!(settle(&db, "t", "a", QueueState::Delivered, "", 4).unwrap().is_none(), "settling twice does nothing");
        assert_eq!(queued(&db).unwrap().len(), 1);
    }

    #[test]
    fn bounds_and_cleanup() {
        let db = db();
        assert!(enqueue(&db, "t", None, "  ", DeliveryMode::AfterTurn, QueueKind::Send, "you", 1).is_err());
        for n in 0..MAX_QUEUED_PER_TASK {
            enqueue(&db, "t", Some(&format!("m{n}")), "x", DeliveryMode::AfterTurn, QueueKind::Send, "you", 1).unwrap();
        }
        assert!(enqueue(&db, "t", Some("over"), "x", DeliveryMode::AfterTurn, QueueKind::Send, "you", 1).is_err());
        for n in 0..MAX_QUEUED_PER_TASK {
            settle(&db, "t", &format!("m{n}"), QueueState::Delivered, "", 2).unwrap();
        }
        assert_eq!(of_task(&db, "t").unwrap().len(), KEEP_SETTLED, "settled entries age out");
        forget_task(&db, "t").unwrap();
        assert!(tasks_with_entries(&db).unwrap().is_empty());
    }
}

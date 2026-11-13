//! The chat directs the crew: small rows behind the orchestration tools.
//! Who delegated which task (so one chat cannot flood the machine), the
//! person's delegation cap, and a bounded cache of results so a retried tool
//! call does the work once. Rows of their own, never the snapshot.
//! See docs/knowledge/the-chat-directs-the-crew.md.
use crate::db::Db;
use neko_protocol::crew::DEFAULT_DELEGATION_CAP;
use neko_protocol::workbench::{Snapshot, TaskStatus};
use std::collections::BTreeMap;

const ORIGINS: &str = "neko_crew_origin_v1";
const CAP: &str = "neko_crew_cap_v1";
const IDEM: &str = "neko_crew_idem_v1";
const MAX_IDEM: usize = 200;
/// An identical call this soon after the last is a retry; later it is a new request.
pub const IDEM_WINDOW_MS: i64 = 120_000;

/// Who delegated a task: `chat` for the full-power chat, `ticket:<id>` for a sub-task.
pub const CHAT: &str = "chat";

fn read<T: for<'de> serde::Deserialize<'de> + Default>(db: &Db, key: &str) -> Result<T, String> {
    match db.get_setting(key).map_err(|e| e.to_string())? {
        Some(text) => serde_json::from_str(&text).map_err(|e| format!("Cannot read {key}: {e}")),
        None => Ok(T::default()),
    }
}

fn write<T: serde::Serialize>(db: &Db, key: &str, value: &T) -> Result<(), String> {
    db.set_setting_quiet(key, &serde_json::to_string(value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn cap(db: &Db) -> Result<u32, String> {
    Ok(read::<Option<u32>>(db, CAP)?.unwrap_or(DEFAULT_DELEGATION_CAP))
}

pub fn set_cap(db: &Db, limit: u32) -> Result<(), String> {
    if !(1..=64).contains(&limit) {
        return Err("The delegation cap is between 1 and 64".into());
    }
    write(db, CAP, &Some(limit))
}

pub fn record_origin(db: &Db, task_id: &str, origin: &str) -> Result<(), String> {
    let mut origins: BTreeMap<String, String> = read(db, ORIGINS)?;
    origins.insert(task_id.to_owned(), origin.to_owned());
    write(db, ORIGINS, &origins)
}

/// Forgets a deleted task's origin (rows are removed with their task, never by count,
/// so an open task never loses the origin the cap counts it under).
pub fn forget_origin(db: &Db, task_id: &str) -> Result<(), String> {
    let mut origins: BTreeMap<String, String> = read(db, ORIGINS)?;
    if origins.remove(task_id).is_some() {
        write(db, ORIGINS, &origins)?;
    }
    Ok(())
}

pub fn origin_of(db: &Db, task_id: &str) -> Result<Option<String>, String> {
    Ok(read::<BTreeMap<String, String>>(db, ORIGINS)?.get(task_id).cloned())
}

/// Tasks this origin delegated that are not finished.
pub fn open_delegated(db: &Db, snapshot: &Snapshot, origin: &str) -> Result<Vec<String>, String> {
    let origins: BTreeMap<String, String> = read(db, ORIGINS)?;
    Ok(snapshot
        .tasks
        .iter()
        .filter(|t| origins.get(&t.id).is_some_and(|o| o == origin) && !matches!(t.status, TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed))
        .map(|t| t.id.clone())
        .collect())
}

/// Refuses a delegation past the cap.
pub fn check_cap(db: &Db, snapshot: &Snapshot, origin: &str) -> Result<(), String> {
    let open = open_delegated(db, snapshot, origin)?.len();
    let limit = cap(db)? as usize;
    if open >= limit {
        return Err(format!("{open} delegated tasks are already open (the limit is {limit}). Wait for some to finish with wait_for, or ask the person to raise the limit."));
    }
    Ok(())
}

/// The key a retried call shares: the lease's run, the tool and its arguments
/// (or the caller's own `idempotency_key`).
pub fn idem_key(run_id: &str, tool: &str, arguments: &serde_json::Value) -> String {
    use std::hash::{Hash, Hasher};
    if let Some(own) = arguments.get("idempotency_key").and_then(|k| k.as_str()).filter(|k| !k.is_empty()) {
        return format!("{run_id}|{tool}|key:{own}");
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // serde_json::Value prints object keys in a stable order within one build.
    arguments.to_string().hash(&mut hasher);
    format!("{run_id}|{tool}|{:016x}", hasher.finish())
}

pub fn cached_result(db: &Db, key: &str, now_ms: i64) -> Result<Option<String>, String> {
    Ok(read::<Vec<(String, String, i64)>>(db, IDEM)?.into_iter().find(|(k, _, at)| k == key && now_ms - at < IDEM_WINDOW_MS).map(|(_, v, _)| v))
}

pub fn cache_result(db: &Db, key: &str, result: &str, now_ms: i64) -> Result<(), String> {
    let mut rows: Vec<(String, String, i64)> = read(db, IDEM)?;
    rows.retain(|(k, _, at)| k != key && now_ms - at < IDEM_WINDOW_MS);
    rows.push((key.to_owned(), result.chars().take(4000).collect(), now_ms));
    let excess = rows.len().saturating_sub(MAX_IDEM);
    rows.drain(..excess);
    write(db, IDEM, &rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_defaults_to_eight_and_can_be_raised() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(cap(&db).unwrap(), 8);
        assert!(set_cap(&db, 0).is_err() && set_cap(&db, 65).is_err());
        set_cap(&db, 12).unwrap();
        assert_eq!(cap(&db).unwrap(), 12);
    }

    #[test]
    fn a_retry_shares_its_key_and_a_different_call_does_not() {
        let a = idem_key("run", "create_task", &serde_json::json!({"title": "x"}));
        assert_eq!(a, idem_key("run", "create_task", &serde_json::json!({"title": "x"})));
        assert_ne!(a, idem_key("run", "create_task", &serde_json::json!({"title": "y"})));
        assert_ne!(a, idem_key("other-run", "create_task", &serde_json::json!({"title": "x"})));
        assert_ne!(idem_key("r", "t", &serde_json::json!({"idempotency_key": "1"})), idem_key("r", "t", &serde_json::json!({"idempotency_key": "2"})));
    }

    #[test]
    fn cached_results_are_bounded_and_found_by_key() {
        let db = Db::open_in_memory().unwrap();
        cache_result(&db, "k", "{\"ok\":1}", 1000).unwrap();
        assert_eq!(cached_result(&db, "k", 1500).unwrap().as_deref(), Some("{\"ok\":1}"));
        assert!(cached_result(&db, "k", 1000 + IDEM_WINDOW_MS).unwrap().is_none(), "a repeat after the window is a new request");
        for n in 0..(MAX_IDEM + 10) {
            cache_result(&db, &format!("n{n}"), "x", 2000).unwrap();
        }
        assert!(cached_result(&db, "k", 2000).unwrap().is_none());
        assert!(cached_result(&db, &format!("n{}", MAX_IDEM + 9), 2000).unwrap().is_some());
    }
}

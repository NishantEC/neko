//! The store: create, list, reopen, migrate, and two connections on one file.

use neko_core::{Core, FixedClock, SequentialIds, Store, StoreError};
use neko_core::store::SCHEMA_VERSION;
use neko_protocol::{TaskId, TaskStatus};

fn temp() -> (std::path::PathBuf, impl Drop) {
    struct Gone(std::path::PathBuf);
    impl Drop for Gone {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    // A counter as well as the time: tests run in parallel threads, and the clock's grain is a microsecond.
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("neko-core-test-{}-{n}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&dir).unwrap();
    (dir.join("neko.sqlite3"), Gone(dir))
}

fn core(path: &std::path::Path) -> Core {
    Core::open(path, Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

#[test]
fn a_task_keeps_the_exact_words_and_lists_newest_first() {
    let (path, _g) = temp();
    let mut c = core(&path);
    assert!(c.store.list_tasks("default").unwrap().is_empty(), "a new store is empty");
    let (first, r1) = c.create_task("  Fix the build\n\nthen tell Lee  ").unwrap();
    let (second, r2) = c.create_task("Rename the sidebar label").unwrap();
    assert_eq!(first.words, "  Fix the build\n\nthen tell Lee  ", "the words are kept exactly");
    assert_eq!(first.title, "Fix the build");
    assert_eq!(first.status, TaskStatus::New);
    assert_eq!(first.created_ms, 1_764_000_001_000, "the clock handed in, not the system's");
    assert!(first.id.0.starts_with("task_") && first.id.0.len() == 31);
    assert_eq!((r1, r2), (1, 2), "one event per change");
    let listed = c.store.list_tasks("default").unwrap();
    assert_eq!(listed, vec![second.clone(), first.clone()]);
    assert_eq!(c.store.get_task(&first.id).unwrap(), Some(first));
    assert_eq!(c.store.get_task(&TaskId("task_missing".into())).unwrap(), None);
}

#[test]
fn empty_and_huge_words_are_refused() {
    let (path, _g) = temp();
    let mut c = core(&path);
    assert_eq!(c.create_task(" \n\t ").unwrap_err(), neko_core::NewTaskError::Empty);
    assert_eq!(c.create_task(&"x".repeat(neko_protocol::MAX_WORDS_BYTES + 1)).unwrap_err(), neko_core::NewTaskError::TooLong);
    assert_eq!(c.store.revision().unwrap(), 0, "nothing was written");
}

#[test]
fn tasks_survive_closing_and_reopening() {
    let (path, _g) = temp();
    let made = {
        let mut c = core(&path);
        let (t, _) = c.create_task("Write the release notes").unwrap();
        c.store.checkpoint().unwrap();
        t
    };
    let c = core(&path);
    assert_eq!(c.store.list_tasks("default").unwrap(), vec![made]);
    assert_eq!(c.store.workspaces().unwrap(), vec!["default".to_owned()], "the default workspace is made once");
}

#[test]
fn migrations_run_once_from_version_one_and_a_newer_store_is_left_alone() {
    let (path, _g) = temp();
    let s = Store::open(&path).unwrap();
    assert_eq!(s.version().unwrap(), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 2);
    let mode: String = s.connection().query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(s.version().unwrap(), 2, "opening again changes nothing");
    s.connection().pragma_update(None, "user_version", 99).unwrap();
    drop(s);
    match Store::open(&path) {
        Err(StoreError::Newer { found: 99, known: 2 }) => {}
        Err(e) => panic!("wrong error: {e}"),
        Ok(_) => panic!("a store from a newer Neko must not be opened"),
    }
}

#[test]
fn one_row_per_task_and_the_event_log_is_append_only() {
    let (path, _g) = temp();
    let mut c = core(&path);
    c.create_task("one").unwrap();
    c.create_task("two").unwrap();
    let conn = c.store.connection();
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 2);
    assert!(conn.execute("UPDATE events SET kind = 'x'", []).is_err(), "an event cannot be changed");
    assert!(conn.execute("DELETE FROM events", []).is_err(), "an event cannot be removed");
    let events = c.store.events_since(0).unwrap();
    assert_eq!(events.iter().map(|e| (e.0, e.1.as_str())).collect::<Vec<_>>(), vec![(1, "task_created"), (2, "task_created")]);
}

#[test]
fn two_connections_see_each_others_writes() {
    let (path, _g) = temp();
    let mut writer = core(&path);
    let reader = Store::open(&path).unwrap();
    let (t, _) = writer.create_task("Check the second connection").unwrap();
    assert_eq!(reader.list_tasks("default").unwrap(), vec![t]);
    assert_eq!(reader.revision().unwrap(), 1);
    let mut other = Core::open(&path, Box::new(FixedClock::new(1_765_000_000_000, 1000)), Box::new(SequentialIds(100))).unwrap();
    other.create_task("From the other side").unwrap();
    assert_eq!(writer.store.list_tasks("default").unwrap().len(), 2);
}

#[test]
fn an_s0_store_moves_to_version_two_with_its_tasks() {
    let (path, _g) = temp();
    {
        // The S0 schema, as version 1 wrote it, with one task.
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE workspaces (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_ms INTEGER NOT NULL);
             CREATE TABLE tasks (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id), words TEXT NOT NULL, title TEXT NOT NULL, status TEXT NOT NULL, created_ms INTEGER NOT NULL);
             CREATE INDEX tasks_by_workspace ON tasks(workspace_id, created_ms DESC);
             CREATE TABLE events (seq INTEGER PRIMARY KEY AUTOINCREMENT, at_ms INTEGER NOT NULL, workspace_id TEXT NOT NULL, subject TEXT NOT NULL, kind TEXT NOT NULL, data TEXT NOT NULL);
             INSERT INTO workspaces VALUES ('default', 'Home', 1);
             INSERT INTO tasks VALUES ('task_1', 'default', 'Fix it', 'Fix it', 'new', 5);
             PRAGMA user_version = 1;",
        )
        .unwrap();
    }
    let c = core(&path);
    assert_eq!(c.store.version().unwrap(), 2);
    let t = c.store.get_task(&TaskId("task_1".into())).unwrap().unwrap();
    assert_eq!((t.words.as_str(), t.status, t.folder), ("Fix it", TaskStatus::New, None));
    assert!(c.store.runs_of(&t.id).unwrap().is_empty());
}

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
    let ws: Vec<String> = c.store.list_workspaces().unwrap().into_iter().map(|w| w.id).collect();
    assert_eq!(ws, vec!["default".to_owned()], "General is made once");
}

#[test]
fn migrations_run_once_from_version_one_and_a_newer_store_is_left_alone() {
    let (path, _g) = temp();
    let s = Store::open(&path).unwrap();
    assert_eq!(s.version().unwrap(), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 17);
    let mode: String = s.connection().query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(s.version().unwrap(), 17, "opening again changes nothing");
    s.connection().pragma_update(None, "user_version", 99).unwrap();
    drop(s);
    match Store::open(&path) {
        Err(StoreError::Newer { found: 99, known: 17 }) => {}
        Err(e) => panic!("wrong error: {e}"),
        Ok(_) => panic!("a store from a newer Neko must not be opened"),
    }
}

/// Version 5 (undo): a store as version 4 left it, with a finished run, opens with that run whole and no undo of its own; a
/// checkpoint row and a run's undo state are then stored, read back, and go with their task.
#[test]
fn version_5_adds_checkpoints_and_a_run_s_undo() {
    let (path, _g) = temp();
    {
        let s = Store::open_at(&path, 4).unwrap();
        assert_eq!(s.version().unwrap(), 4);
        let c = s.connection();
        c.execute("INSERT INTO workspaces (id, name, created_ms, position) VALUES ('default', 'General', 1, 0)", []).unwrap();
        c.execute("INSERT INTO tasks (id, workspace_id, words, title, status, created_ms) VALUES ('task_1', 'default', 'Tidy', 'Tidy', 'done', 1)", []).unwrap();
        c.execute("INSERT INTO runs (id, task_id, runtime, status, started_ms, ended_ms, workdir, files, added, removed) VALUES ('run_1', 'task_1', 'claude', 'done', 1, 2, '/Users/example/acme-api', 2, 5, 1)", []).unwrap();
    }
    let mut s = Store::open(&path).unwrap();
    assert_eq!(s.version().unwrap(), SCHEMA_VERSION);
    let run = s.get_run(&neko_protocol::RunId("run_1".into())).unwrap().unwrap();
    assert_eq!(run.undo, None, "a run from before undo has none");
    assert_eq!(run.changes, Some(neko_protocol::Changes { files: 2, added: 5, removed: 1 }));
    let cp = neko_core::history::Checkpoint {
        id: "cp_1".into(),
        task: neko_protocol::TaskId("task_1".into()),
        run: run.id.clone(),
        kind: "start".into(),
        place: neko_core::checkpoint::Kind::Repo,
        work: "/Users/example/acme-api".into(),
        shadow: None,
        refname: "refs/neko/checkpoints/task_1/run_1/start".into(),
        commit: "c0ffee".into(),
        tree: "7ree".into(),
        head: Some("abc".into()),
        files: 12,
        bytes: 345,
        took_ms: 20,
        made_ms: 3,
        state: "kept".into(),
    };
    s.insert_checkpoint(&cp, 3).unwrap();
    assert_eq!(s.checkpoints_of(&cp.task).unwrap(), vec![cp.clone()]);
    let u = neko_protocol::RunUndo { state: neko_protocol::UndoState::Undone, why: None, changes: Some(neko_protocol::Changes { files: 1, added: 2, removed: 0 }), committed: false };
    let (r, _) = s.set_run_undo(&run.id, &u, 4).unwrap();
    assert_eq!(r.undo, Some(u));
    s.remove_checkpoints(&["cp_1".into()], 5).unwrap();
    assert_eq!(s.kept_checkpoints().unwrap(), vec![]);
    s.delete_task(&cp.task, 6).unwrap();
    let left: i64 = s.connection().query_row("SELECT COUNT(*) FROM checkpoints", [], |r| r.get(0)).unwrap();
    assert_eq!(left, 0, "a removed task's checkpoint rows go with it");
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
fn an_s0_store_moves_to_the_newest_version_with_its_tasks_and_follows_claude_codes_rules() {
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
    assert_eq!(c.store.version().unwrap(), SCHEMA_VERSION);
    assert_eq!(c.store.settings().unwrap().permission_mode, neko_protocol::PermissionMode::Follow, "no mode stored: Claude Code's own rules");
    let t = c.store.get_task(&TaskId("task_1".into())).unwrap().unwrap();
    assert_eq!((t.words.as_str(), t.status, t.folder), ("Fix it", TaskStatus::New, None));
    assert!(c.store.runs_of(&t.id).unwrap().is_empty());
}

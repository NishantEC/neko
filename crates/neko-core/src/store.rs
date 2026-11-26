//! The SQLite store (docs/knowledge/store.md): one row per entity, and an append-only event row for every change.
//!
//! The earlier project kept everything in one JSON row and rewrote megabytes on every tick. Here a write touches only the rows it
//! changes, in one transaction with the event that records it. The event log is append-only: triggers refuse an update or delete.

use std::path::Path;

use neko_protocol::{Task, TaskId, TaskStatus};
use rusqlite::{Connection, OptionalExtension, params};

use crate::title::title;

/// The id of the workspace every task lives in until workspaces have their own slice.
pub const DEFAULT_WORKSPACE: &str = "default";

/// Each migration, in order: the store's `user_version` is how many have run.
const MIGRATIONS: &[&str] = &[
    // 1: workspaces, tasks and the event log.
    "CREATE TABLE workspaces (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        created_ms INTEGER NOT NULL
    );
    CREATE TABLE tasks (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        words TEXT NOT NULL,
        title TEXT NOT NULL,
        status TEXT NOT NULL,
        created_ms INTEGER NOT NULL
    );
    CREATE INDEX tasks_by_workspace ON tasks(workspace_id, created_ms DESC);
    CREATE TABLE events (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        at_ms INTEGER NOT NULL,
        workspace_id TEXT NOT NULL,
        subject TEXT NOT NULL,
        kind TEXT NOT NULL,
        data TEXT NOT NULL
    );
    CREATE TRIGGER events_no_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
    CREATE TRIGGER events_no_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;",
];

/// The schema version this build writes.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    /// The store was made by a newer Neko; this one will not touch it.
    Newer { found: i64, known: i64 },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Sqlite(e) => write!(f, "{e}"),
            StoreError::Newer { found, known } => write!(f, "the store is at version {found}, made by a newer Neko (this one knows {known})"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

/// One connection to the store.
pub struct Store {
    conn: Connection,
}

/// A status as stored: its protocol name ("new").
fn status_name(s: TaskStatus) -> String {
    serde_json::to_value(s).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

fn status_of(name: &str) -> rusqlite::Result<TaskStatus> {
    serde_json::from_value(serde_json::Value::String(name.to_owned())).map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))
}

fn task_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: TaskId(row.get(0)?),
        workspace: row.get(1)?,
        words: row.get(2)?,
        title: row.get(3)?,
        status: status_of(&row.get::<_, String>(4)?)?,
        created_ms: row.get(5)?,
    })
}

const TASK_COLUMNS: &str = "id, workspace_id, words, title, status, created_ms";

impl Store {
    /// Opens the store at `path` (made if missing), in WAL mode, and runs any migration it lacks.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_size_limit", 8 * 1024 * 1024)?;
        let mut store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    /// The schema version on disk.
    pub fn version(&self) -> Result<i64, StoreError> {
        Ok(self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    fn migrate(&mut self) -> Result<(), StoreError> {
        let found = self.version()?;
        if found > SCHEMA_VERSION {
            return Err(StoreError::Newer { found, known: SCHEMA_VERSION });
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Makes the default workspace if it is not there yet.
    pub fn ensure_default_workspace(&self, now_ms: i64) -> Result<(), StoreError> {
        self.conn.execute("INSERT OR IGNORE INTO workspaces (id, name, created_ms) VALUES (?1, 'Home', ?2)", params![DEFAULT_WORKSPACE, now_ms])?;
        Ok(())
    }

    /// The workspaces' ids, in the order they were made.
    pub fn workspaces(&self) -> Result<Vec<String>, StoreError> {
        let mut st = self.conn.prepare("SELECT id FROM workspaces ORDER BY created_ms, id")?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Stores a task and its `task_created` event in one transaction. Returns the task and the event's sequence number.
    pub fn create_task(&mut self, id: &TaskId, workspace: &str, words: &str, now_ms: i64) -> Result<(Task, u64), StoreError> {
        let task = Task { id: id.clone(), workspace: workspace.to_owned(), words: words.to_owned(), title: title(words), created_ms: now_ms, status: TaskStatus::New };
        let tx = self.conn.transaction()?;
        tx.execute(
            &format!("INSERT INTO tasks ({TASK_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
            params![task.id.0, task.workspace, task.words, task.title, status_name(task.status), task.created_ms],
        )?;
        let data = serde_json::json!({ "title": task.title }).to_string();
        tx.execute("INSERT INTO events (at_ms, workspace_id, subject, kind, data) VALUES (?1, ?2, ?3, 'task_created', ?4)", params![now_ms, workspace, task.id.0, data])?;
        let seq = tx.last_insert_rowid() as u64;
        tx.commit()?;
        Ok((task, seq))
    }

    /// The workspace's tasks, newest first.
    pub fn list_tasks(&self, workspace: &str) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE workspace_id = ?1 ORDER BY created_ms DESC, id DESC"))?;
        let rows = st.query_map([workspace], task_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_task(&self, id: &TaskId) -> Result<Option<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"))?;
        Ok(st.query_row([&id.0], task_of).optional()?)
    }

    /// The sequence number of the last event (0 for none).
    pub fn revision(&self) -> Result<u64, StoreError> {
        Ok(self.conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// The events after `after`, oldest first, as (seq, kind, subject).
    pub fn events_since(&self, after: u64) -> Result<Vec<(u64, String, String)>, StoreError> {
        let mut st = self.conn.prepare("SELECT seq, kind, subject FROM events WHERE seq > ?1 ORDER BY seq")?;
        let rows = st.query_map([after as i64], |r| Ok((r.get::<_, i64>(0)? as u64, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Moves the write-ahead log into the main file, so a clean shutdown leaves one file behind.
    pub fn checkpoint(&self) -> Result<(), StoreError> {
        self.conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    /// The raw connection, for tests that check what is on disk.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

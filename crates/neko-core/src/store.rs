//! The SQLite store (docs/knowledge/store.md): one row per entity, and an append-only event row for every change.
//!
//! The earlier project kept everything in one JSON row and rewrote megabytes on every tick. Here a write touches only the rows it
//! changes, in one transaction with the event that records it. The event log is append-only: triggers refuse an update or delete.

use std::path::Path;

use neko_protocol::{Task, TaskId, TaskStatus};
use rusqlite::{Connection, OptionalExtension, params};

use crate::title::title;

/// The id of General, the built-in workspace with no folder (`neko_protocol::GENERAL`).
pub const DEFAULT_WORKSPACE: &str = neko_protocol::GENERAL;

/// One migration: SQL, or SQL then a step that needs code (reading folders on disk).
enum Migration {
    Sql(&'static str),
    Then(&'static str, fn(&rusqlite::Transaction<'_>) -> rusqlite::Result<()>),
}

/// Each migration, in order: the store's `user_version` is how many have run.
const MIGRATIONS: &[Migration] = &[
    // 1: workspaces, tasks and the event log.
    Migration::Sql("CREATE TABLE workspaces (
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
    CREATE TRIGGER events_no_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;"),
    // 2: tasks run (S1). Where a task works, its runs, the transcript's parts, permission requests, each run's usage, settings.
    Migration::Sql("ALTER TABLE tasks ADD COLUMN folder TEXT;
    ALTER TABLE tasks ADD COLUMN workdir TEXT;
    ALTER TABLE tasks ADD COLUMN branch TEXT;
    ALTER TABLE tasks ADD COLUMN base_commit TEXT;
    ALTER TABLE tasks ADD COLUMN repo TEXT;
    ALTER TABLE tasks ADD COLUMN plain INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE runs (
        id TEXT PRIMARY KEY,
        task_id TEXT NOT NULL REFERENCES tasks(id),
        runtime TEXT NOT NULL,
        model TEXT,
        status TEXT NOT NULL,
        started_ms INTEGER NOT NULL,
        ended_ms INTEGER,
        session TEXT,
        workdir TEXT NOT NULL,
        branch TEXT,
        reason TEXT,
        files INTEGER,
        added INTEGER,
        removed INTEGER
    );
    CREATE INDEX runs_by_task ON runs(task_id, started_ms);
    CREATE INDEX runs_by_status ON runs(status);
    CREATE TABLE usage (
        run_id TEXT PRIMARY KEY REFERENCES runs(id),
        input_tokens INTEGER NOT NULL,
        output_tokens INTEGER NOT NULL,
        cached_tokens INTEGER NOT NULL,
        cost_usd REAL,
        model TEXT,
        pricing TEXT NOT NULL,
        reported_by TEXT NOT NULL
    );
    CREATE TABLE parts (
        id TEXT PRIMARY KEY,
        task_id TEXT NOT NULL REFERENCES tasks(id),
        run_id TEXT,
        seq INTEGER NOT NULL,
        kind TEXT NOT NULL,
        body TEXT NOT NULL,
        at_ms INTEGER NOT NULL,
        ended_ms INTEGER
    );
    CREATE UNIQUE INDEX parts_by_task ON parts(task_id, seq);
    CREATE TABLE permissions (
        id TEXT PRIMARY KEY,
        task_id TEXT NOT NULL REFERENCES tasks(id),
        run_id TEXT NOT NULL,
        tool TEXT NOT NULL,
        title TEXT NOT NULL,
        why TEXT NOT NULL,
        detail TEXT NOT NULL,
        destructive INTEGER NOT NULL,
        rule TEXT NOT NULL,
        state TEXT NOT NULL,
        asked_ms INTEGER NOT NULL,
        answered_ms INTEGER
    );
    CREATE INDEX permissions_by_task ON permissions(task_id, asked_ms);
    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );"),
    // 3: how full the conversation's context was at a run's last call, and the model's window, as reported.
    Migration::Sql("ALTER TABLE usage ADD COLUMN context_tokens INTEGER;
    ALTER TABLE usage ADD COLUMN context_window INTEGER;"),
    // 4: workspaces (docs/knowledge/workspaces.md). The default row becomes General; each folder tasks were made in becomes a
    // workspace of its own, and those tasks move into it.
    Migration::Then(
        "ALTER TABLE workspaces ADD COLUMN folder TEXT;
        ALTER TABLE workspaces ADD COLUMN removed_ms INTEGER;
        ALTER TABLE workspaces ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
        UPDATE workspaces SET name = 'General', position = 0 WHERE id = 'default';
        CREATE UNIQUE INDEX workspaces_by_folder ON workspaces(folder) WHERE folder IS NOT NULL;",
        crate::workspaces::migrate_task_folders,
    ),
    // 5: undo (docs/knowledge/undo.md). A row per checkpoint, and where each run's undo stands.
    Migration::Sql("CREATE TABLE checkpoints (
        id TEXT PRIMARY KEY,
        task_id TEXT NOT NULL REFERENCES tasks(id),
        run_id TEXT NOT NULL REFERENCES runs(id),
        kind TEXT NOT NULL,
        place TEXT NOT NULL,
        work TEXT NOT NULL,
        shadow TEXT,
        ref TEXT NOT NULL,
        commit_id TEXT NOT NULL,
        tree TEXT NOT NULL,
        head TEXT,
        files INTEGER NOT NULL,
        bytes INTEGER NOT NULL,
        took_ms INTEGER NOT NULL,
        made_ms INTEGER NOT NULL,
        state TEXT NOT NULL
    );
    CREATE INDEX checkpoints_by_task ON checkpoints(task_id, made_ms);
    CREATE INDEX checkpoints_by_run ON checkpoints(run_id, kind);
    ALTER TABLE runs ADD COLUMN undo_state TEXT;
    ALTER TABLE runs ADD COLUMN undo_why TEXT;
    ALTER TABLE runs ADD COLUMN undo_files INTEGER;
    ALTER TABLE runs ADD COLUMN undo_added INTEGER;
    ALTER TABLE runs ADD COLUMN undo_removed INTEGER;
    ALTER TABLE runs ADD COLUMN undo_committed INTEGER;"),
    // 6: cats (docs/knowledge/cats.md). A row per cat; every task has one; a run records its cat and the version of its rules.
    // Existing tasks go to the three starter cats by the rule new tasks follow, oldest first.
    Migration::Then(
        "CREATE TABLE cats (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            name_key TEXT NOT NULL,
            face TEXT NOT NULL,
            purpose TEXT,
            rules TEXT NOT NULL DEFAULT '',
            previous_rules TEXT,
            rules_saved_ms INTEGER,
            workspace_id TEXT,
            created_ms INTEGER NOT NULL,
            retired_ms INTEGER
        );
        CREATE UNIQUE INDEX cats_by_name ON cats(name_key);
        ALTER TABLE tasks ADD COLUMN cat_id TEXT;
        CREATE INDEX tasks_by_cat ON tasks(cat_id, status);
        ALTER TABLE runs ADD COLUMN cat_id TEXT;
        ALTER TABLE runs ADD COLUMN rules_hash TEXT;",
        crate::cats::migrate_tasks,
    ),
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
    pub(crate) conn: Connection,
}

/// A status as stored: its protocol name ("new").
pub(crate) fn status_name<T: serde::Serialize>(s: T) -> String {
    serde_json::to_value(s).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

pub(crate) fn status_of<T: serde::de::DeserializeOwned>(name: &str) -> rusqlite::Result<T> {
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
        folder: row.get(6)?,
        cat: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
    })
}

const TASK_COLUMNS: &str = "id, workspace_id, words, title, status, created_ms, folder, cat_id";

impl Store {
    /// Opens the store at `path` (made if missing), in WAL mode, and runs any migration it lacks.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        Self::open_at(path, SCHEMA_VERSION)
    }

    /// [`Store::open`], migrating no further than `version`: tests build a store as an older Neko left it, then open it.
    pub fn open_at(path: &Path, version: i64) -> Result<Store, StoreError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_size_limit", 8 * 1024 * 1024)?;
        let mut store = Store { conn };
        store.migrate(version.min(SCHEMA_VERSION))?;
        Ok(store)
    }

    /// The schema version on disk.
    pub fn version(&self) -> Result<i64, StoreError> {
        Ok(self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    fn migrate(&mut self, upto: i64) -> Result<(), StoreError> {
        let found = self.version()?;
        if found > SCHEMA_VERSION {
            return Err(StoreError::Newer { found, known: SCHEMA_VERSION });
        }
        for (i, m) in MIGRATIONS.iter().enumerate().take(upto.max(0) as usize).skip(found as usize) {
            let tx = self.conn.transaction()?;
            match m {
                Migration::Sql(sql) => tx.execute_batch(sql)?,
                Migration::Then(sql, then) => {
                    tx.execute_batch(sql)?;
                    then(&tx)?;
                }
            }
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Makes General if it is not there yet.
    pub fn ensure_default_workspace(&self, now_ms: i64) -> Result<(), StoreError> {
        self.conn.execute("INSERT OR IGNORE INTO workspaces (id, name, created_ms, position) VALUES (?1, 'General', ?2, 0)", params![DEFAULT_WORKSPACE, now_ms])?;
        Ok(())
    }

    /// Stores a task for `cat` and its `task_created` event in one transaction. Returns the task and the event's sequence number.
    pub fn create_task(&mut self, id: &TaskId, workspace: &str, words: &str, folder: Option<&str>, cat: &str, now_ms: i64) -> Result<(Task, u64), StoreError> {
        let task = Task { id: id.clone(), workspace: workspace.to_owned(), words: words.to_owned(), title: title(words), created_ms: now_ms, status: TaskStatus::New, folder: folder.map(str::to_owned), cat: cat.to_owned() };
        let tx = self.conn.transaction()?;
        tx.execute(
            &format!("INSERT INTO tasks ({TASK_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
            params![task.id.0, task.workspace, task.words, task.title, status_name(task.status), task.created_ms, task.folder, task.cat],
        )?;
        let data = serde_json::json!({ "title": task.title, "cat": task.cat }).to_string();
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

    /// Every task of every workspace, newest first.
    pub fn all_tasks(&self) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {TASK_COLUMNS} FROM tasks ORDER BY created_ms DESC, id DESC"))?;
        let rows = st.query_map([], task_of)?;
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

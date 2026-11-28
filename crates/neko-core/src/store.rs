//! The SQLite store (docs/knowledge/store.md): one row per entity, and an append-only event row for every change.
//!
//! The earlier project kept everything in one JSON row and rewrote megabytes on every tick. Here a write touches only the rows it
//! changes, in one transaction with the event that records it. The event log is append-only: triggers refuse an update or delete.

use std::path::Path;

use neko_protocol::{SubagentOf, Task, TaskId, TaskRound, TaskStatus};
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
    // 7: rounds (docs/knowledge/rounds.md). A row per round with where its schedule has got to (`cursor_ms`: the last due time dealt
    // with), a bounded history per round, and on a task the round that made it and the time it was due. Removing a round removes
    // its history, never its tasks.
    Migration::Sql("CREATE TABLE rounds (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        words TEXT NOT NULL,
        cat_id TEXT NOT NULL,
        workspace_id TEXT NOT NULL,
        schedule TEXT NOT NULL,
        enabled INTEGER NOT NULL,
        anchor_ms INTEGER NOT NULL,
        cursor_ms INTEGER NOT NULL,
        fail_streak INTEGER NOT NULL DEFAULT 0,
        brake_note TEXT,
        created_ms INTEGER NOT NULL,
        changed_ms INTEGER NOT NULL
    );
    CREATE TABLE round_runs (
        id TEXT PRIMARY KEY,
        round_id TEXT NOT NULL REFERENCES rounds(id) ON DELETE CASCADE,
        due_ms INTEGER,
        at_ms INTEGER NOT NULL,
        kind TEXT NOT NULL,
        task_id TEXT,
        reason TEXT,
        ended TEXT
    );
    CREATE INDEX round_runs_by_round ON round_runs(round_id, at_ms DESC);
    CREATE INDEX round_runs_open ON round_runs(ended) WHERE ended IS NULL;
    ALTER TABLE tasks ADD COLUMN round_id TEXT;
    ALTER TABLE tasks ADD COLUMN round_due_ms INTEGER;
    ALTER TABLE tasks ADD COLUMN round_name TEXT;
    CREATE INDEX tasks_by_round ON tasks(round_id) WHERE round_id IS NOT NULL;"),
    // 8: plugins (docs/knowledge/plugins.md). A row per plugin with its manifest, the digest of it and the manifest and digest the
    // person approved (`approved_*`), or declined; and a row per setting the person set. A secret's row says only that it is set: the
    // secret itself is in the Keychain, never here. Removing a plugin removes its settings.
    Migration::Sql("CREATE TABLE plugins (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        name_key TEXT NOT NULL,
        kind TEXT NOT NULL,
        folder TEXT,
        manifest TEXT NOT NULL,
        digest TEXT NOT NULL,
        approved_digest TEXT,
        approved_manifest TEXT,
        approved_ms INTEGER,
        declined_digest TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        access_mode TEXT NOT NULL DEFAULT 'all',
        access_cats TEXT NOT NULL DEFAULT '[]',
        last_check TEXT,
        problem TEXT,
        created_ms INTEGER NOT NULL,
        changed_ms INTEGER NOT NULL,
        manifest_ms INTEGER NOT NULL
    );
    CREATE UNIQUE INDEX plugins_by_name ON plugins(name_key);
    CREATE TABLE plugin_values (
        plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
        key TEXT NOT NULL,
        value TEXT,
        secret INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (plugin_id, key)
    );"),
    // 9: threads (docs/knowledge/threads.md). When the person last opened a task, when it last had activity of its own, and the last
    // line of its newest agent words. Everything that exists is read, as of its last activity.
    Migration::Then(
        "ALTER TABLE tasks ADD COLUMN last_read_ms INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE tasks ADD COLUMN activity_ms INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE tasks ADD COLUMN latest_line TEXT NOT NULL DEFAULT '';
        UPDATE tasks SET activity_ms = MAX(created_ms, COALESCE((SELECT MAX(COALESCE(ended_ms, started_ms)) FROM runs WHERE runs.task_id = tasks.id), 0));
        UPDATE tasks SET last_read_ms = activity_ms;",
        migrate_latest_lines,
    ),
    // 10: subagents (docs/knowledge/subagents.md). A task may belong to another: its parent, its number ("1", "1.2") and the thread at
    // the top. And the reports that wake a thread: one row per subagent that ended, `pending` until the thread read it.
    Migration::Sql("ALTER TABLE tasks ADD COLUMN parent_id TEXT;
    ALTER TABLE tasks ADD COLUMN sub_number TEXT;
    ALTER TABLE tasks ADD COLUMN root_id TEXT;
    CREATE INDEX tasks_by_parent ON tasks(parent_id) WHERE parent_id IS NOT NULL;
    CREATE INDEX tasks_by_root ON tasks(root_id) WHERE root_id IS NOT NULL;
    CREATE TABLE wakes (
        id TEXT PRIMARY KEY,
        task_id TEXT NOT NULL,
        subagent_id TEXT NOT NULL,
        status TEXT NOT NULL,
        summary TEXT NOT NULL,
        files TEXT NOT NULL,
        branch TEXT,
        state TEXT NOT NULL,
        created_ms INTEGER NOT NULL,
        delivered_ms INTEGER
    );
    CREATE INDEX wakes_by_task ON wakes(task_id, state);
    CREATE INDEX wakes_by_subagent ON wakes(subagent_id);"),
    // 11: new thread (docs/knowledge/threads.md). A thread needs no cat: `cat_id` was already nullable, and every task made before keeps
    // the one it was given as history. What the person chose for where it works: the project's own folder instead of a worktree, and
    // the branch a worktree starts from.
    Migration::Sql("ALTER TABLE tasks ADD COLUMN in_folder INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE tasks ADD COLUMN from_branch TEXT;"),
    // 12: steer, queue, interrupt (docs/knowledge/composer.md). The messages waiting above a thread's composer: queued (held for the
    // next run), steered (written into the running agent, not read yet) and interrupting (a new turn starts with them).
    Migration::Sql("CREATE TABLE queued (
        id TEXT PRIMARY KEY,
        task_id TEXT NOT NULL,
        words TEXT NOT NULL,
        kind TEXT NOT NULL,
        at_ms INTEGER NOT NULL
    );
    CREATE INDEX queued_by_task ON queued(task_id);"),
    // 13: pin, settle, archive (docs/knowledge/threads.md). When the person pinned a thread, when it was settled (by them or after a
    // week idle) and when it was archived; empty for a thread that is none of these. Nothing that exists is any of them.
    Migration::Sql("ALTER TABLE tasks ADD COLUMN pinned_ms INTEGER;
    ALTER TABLE tasks ADD COLUMN settled_ms INTEGER;
    ALTER TABLE tasks ADD COLUMN archived_ms INTEGER;"),
    // 14: search (docs/knowledge/search.md). Two FTS5 tables, each row numbered with the rowid of the row it indexes (a task's title; the
    // words of a part of the person or the agent), kept by triggers in the same transaction as the change, and filled here from what
    // exists. `tasks_by_activity` serves the recent threads and the filters that need no words.
    Migration::Sql("CREATE VIRTUAL TABLE find_titles USING fts5(title, words, tokenize = 'unicode61 remove_diacritics 2', prefix = '2 3');
    CREATE VIRTUAL TABLE find_words USING fts5(text, tokenize = 'unicode61 remove_diacritics 2', prefix = '2 3');
    INSERT INTO find_titles (rowid, title, words) SELECT rowid, title, words FROM tasks;
    INSERT INTO find_words (rowid, text) SELECT rowid, COALESCE(json_extract(body, '$.text'), '') FROM parts WHERE kind IN ('person', 'agent');
    CREATE TRIGGER find_titles_insert AFTER INSERT ON tasks BEGIN
        INSERT INTO find_titles (rowid, title, words) VALUES (new.rowid, new.title, new.words);
    END;
    CREATE TRIGGER find_titles_update AFTER UPDATE OF title, words ON tasks BEGIN
        DELETE FROM find_titles WHERE rowid = old.rowid;
        INSERT INTO find_titles (rowid, title, words) VALUES (new.rowid, new.title, new.words);
    END;
    CREATE TRIGGER find_titles_delete AFTER DELETE ON tasks BEGIN
        DELETE FROM find_titles WHERE rowid = old.rowid;
    END;
    CREATE TRIGGER find_words_insert AFTER INSERT ON parts WHEN new.kind IN ('person', 'agent') BEGIN
        INSERT INTO find_words (rowid, text) VALUES (new.rowid, COALESCE(json_extract(new.body, '$.text'), ''));
    END;
    CREATE TRIGGER find_words_update AFTER UPDATE OF body ON parts WHEN new.kind IN ('person', 'agent') BEGIN
        DELETE FROM find_words WHERE rowid = old.rowid;
        INSERT INTO find_words (rowid, text) VALUES (new.rowid, COALESCE(json_extract(new.body, '$.text'), ''));
    END;
    CREATE TRIGGER find_words_delete AFTER DELETE ON parts WHEN old.kind IN ('person', 'agent') BEGIN
        DELETE FROM find_words WHERE rowid = old.rowid;
    END;
    CREATE INDEX tasks_by_activity ON tasks(activity_ms DESC);"),
];

/// Migration 9's step: each task's `latest_line` from its newest agent words.
fn migrate_latest_lines(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
    let ids: Vec<String> = {
        let mut st = tx.prepare("SELECT id FROM tasks")?;
        st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    for id in ids {
        let body: Option<String> = tx.query_row("SELECT body FROM parts WHERE task_id = ?1 AND kind = 'agent' ORDER BY seq DESC LIMIT 1", [&id], |r| r.get(0)).optional()?;
        let Some(body) = body else { continue };
        if let Ok(neko_protocol::PartBody::Agent { text, .. }) = serde_json::from_str::<neko_protocol::PartBody>(&body) {
            tx.execute("UPDATE tasks SET latest_line = ?2 WHERE id = ?1", params![id, crate::thread::latest_line(&text)])?;
        }
    }
    Ok(())
}

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

/// A task as stored. What the daemon derives (`thread_status`, `needs_you`, `failure_line`) is filled by [`Store::derived`].
pub(crate) fn task_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: TaskId(row.get(0)?),
        workspace: row.get(1)?,
        words: row.get(2)?,
        title: row.get(3)?,
        status: status_of(&row.get::<_, String>(4)?)?,
        created_ms: row.get(5)?,
        folder: row.get(6)?,
        cat: row.get::<_, Option<String>>(7)?.filter(|c| !c.is_empty()),
        in_folder: row.get::<_, i64>(17)? != 0,
        from_branch: row.get(18)?,
        round: row.get::<_, Option<String>>(8)?.map(|id| -> rusqlite::Result<TaskRound> {
            Ok(TaskRound { id, name: row.get::<_, Option<String>>(10)?.unwrap_or_default(), due_ms: row.get(9)? })
        }).transpose()?,
        last_read_ms: row.get(11)?,
        activity_ms: row.get(12)?,
        latest_line: row.get(13)?,
        failure_line: String::new(),
        thread_status: neko_protocol::ThreadStatus::Idle,
        needs_you: false,
        pinned: row.get::<_, Option<i64>>(19)?.is_some(),
        settled: row.get::<_, Option<i64>>(20)?.is_some(),
        archived: row.get::<_, Option<i64>>(21)?.is_some(),
        subagent: match (row.get::<_, Option<String>>(14)?, row.get::<_, Option<String>>(15)?, row.get::<_, Option<String>>(16)?) {
            (Some(parent), Some(number), Some(root)) => Some(SubagentOf { parent: TaskId(parent), number, root: TaskId(root) }),
            _ => None,
        },
    })
}

/// What a new task row writes (the rest of the columns begin empty).
const INSERT_COLUMNS: &str = "id, workspace_id, words, title, status, created_ms, folder, cat_id, round_id, round_due_ms, round_name, last_read_ms, activity_ms, latest_line, parent_id, sub_number, root_id, in_folder, from_branch";
/// What a task is read from, in the order [`task_of`] takes it.
pub(crate) const TASK_COLUMNS: &str = "id, workspace_id, words, title, status, created_ms, folder, cat_id, round_id, round_due_ms, round_name, last_read_ms, activity_ms, latest_line, parent_id, sub_number, root_id, in_folder, from_branch, pinned_ms, settled_ms, archived_ms";

/// Writes a new task and its `task_created` event inside `tx`. Returns the event's sequence number.
pub(crate) fn insert_task(tx: &rusqlite::Transaction<'_>, task: &Task) -> Result<u64, StoreError> {
    tx.execute(
        &format!("INSERT INTO tasks ({INSERT_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)"),
        params![
            task.id.0,
            task.workspace,
            task.words,
            task.title,
            status_name(task.status),
            task.created_ms,
            task.folder,
            task.cat,
            task.round.as_ref().map(|r| &r.id),
            task.round.as_ref().and_then(|r| r.due_ms),
            task.round.as_ref().map(|r| &r.name),
            task.last_read_ms,
            task.activity_ms,
            task.latest_line,
            task.subagent.as_ref().map(|s| &s.parent.0),
            task.subagent.as_ref().map(|s| &s.number),
            task.subagent.as_ref().map(|s| &s.root.0),
            task.in_folder as i64,
            task.from_branch
        ],
    )?;
    let data = serde_json::json!({ "title": task.title, "cat": task.cat, "round": task.round.as_ref().map(|r| &r.id), "subagent": task.subagent.as_ref().map(|s| &s.number) }).to_string();
    tx.execute("INSERT INTO events (at_ms, workspace_id, subject, kind, data) VALUES (?1, ?2, ?3, 'task_created', ?4)", params![task.created_ms, task.workspace, task.id.0, data])?;
    Ok(tx.last_insert_rowid() as u64)
}

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

    /// Stores a task (for `cat` when it has one) and its `task_created` event in one transaction. Returns the task and the event's
    /// sequence number.
    pub fn create_task(&mut self, id: &TaskId, workspace: &str, words: &str, folder: Option<&str>, cat: Option<&str>, how: &crate::workplace::Placement, now_ms: i64) -> Result<(Task, u64), StoreError> {
        let task = Task { id: id.clone(), workspace: workspace.to_owned(), words: words.to_owned(), title: title(words), created_ms: now_ms, status: TaskStatus::New, folder: folder.map(str::to_owned), cat: cat.map(str::to_owned), in_folder: how.in_folder, from_branch: how.from_branch.clone(), round: None, last_read_ms: now_ms, activity_ms: now_ms, latest_line: String::new(), failure_line: String::new(), thread_status: neko_protocol::ThreadStatus::Idle, needs_you: false, subagent: None, pinned: false, settled: false, archived: false };
        let tx = self.conn.transaction()?;
        let seq = insert_task(&tx, &task)?;
        tx.commit()?;
        Ok((task, seq))
    }

    /// The workspace's tasks, newest first.
    pub fn list_tasks(&self, workspace: &str) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE workspace_id = ?1 ORDER BY created_ms DESC, id DESC"))?;
        let rows = st.query_map([workspace], task_of)?;
        let tasks: Vec<Task> = rows.collect::<Result<_, _>>()?;
        tasks.into_iter().map(|t| self.derived(t)).collect()
    }

    /// Every task of every workspace, newest first.
    pub fn all_tasks(&self) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {TASK_COLUMNS} FROM tasks ORDER BY created_ms DESC, id DESC"))?;
        let rows = st.query_map([], task_of)?;
        let tasks: Vec<Task> = rows.collect::<Result<_, _>>()?;
        tasks.into_iter().map(|t| self.derived(t)).collect()
    }

    pub fn get_task(&self, id: &TaskId) -> Result<Option<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"))?;
        let task = st.query_row([&id.0], task_of).optional()?;
        task.map(|t| self.derived(t)).transpose()
    }

    /// What the daemon works out about a task, from what is stored (`thread.rs` has the rules): its status as a thread, whether
    /// something waits for the person, and why it failed. Only a failed task costs a read (its newest error notice).
    pub(crate) fn derived(&self, mut t: Task) -> Result<Task, StoreError> {
        let mut actionable = false;
        if t.status == TaskStatus::Failed
            && let Some(n) = self.newest_error_notice(&t.id)?
        {
            actionable = crate::needs::failure_of(&n).is_some();
            t.failure_line = crate::thread::failure_line(n.body.text());
        }
        t.thread_status = crate::thread::thread_status(t.status, t.last_read_ms, t.activity_ms);
        let subagent_asks = t.subagent.is_none() && self.conn.query_row("SELECT 1 FROM permissions p JOIN tasks s ON s.id = p.task_id WHERE s.root_id = ?1 AND p.state = 'pending' LIMIT 1", [&t.id.0], |_| Ok(())).optional()?.is_some();
        t.needs_you = !t.archived && crate::thread::needs_you(t.status, actionable, subagent_asks);
        Ok(t)
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

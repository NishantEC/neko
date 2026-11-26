//! Undo's rows (store version 5, docs/knowledge/undo.md): one row per checkpoint, and where each run's undo stands in its own
//! columns on `runs`. Every change appends an event, as everything else does.

use neko_protocol::{Changes, Run, RunId, RunUndo, TaskId, UndoState};
use rusqlite::{OptionalExtension, params};

use crate::checkpoint::Kind;
use crate::store::{Store, StoreError, status_name, status_of};

/// The kinds of checkpoint.
pub const START: &str = "start";
pub const END: &str = "end";
/// The folder as it was just before an "Undo anyway": Redo puts this back.
pub const BEFORE_UNDO: &str = "before_undo";
/// The folder as it was just before a "Redo anyway": kept under its ref so nothing is lost.
pub const BEFORE_REDO: &str = "before_redo";

/// One checkpoint, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub id: String,
    pub task: TaskId,
    pub run: RunId,
    pub kind: String,
    /// Kept in the repository itself, or in Neko's shadow repository.
    pub place: Kind,
    /// The work tree it is of.
    pub work: String,
    /// The shadow repository, when it is in one.
    pub shadow: Option<String>,
    pub refname: String,
    pub commit: String,
    pub tree: String,
    /// The repository's HEAD commit when it was taken.
    pub head: Option<String>,
    pub files: u64,
    /// Bytes of the new or changed files it stored.
    pub bytes: u64,
    pub took_ms: u64,
    pub made_ms: i64,
    /// `kept`, or `removed` once its ref is gone.
    pub state: String,
}

const COLUMNS: &str = "id, task_id, run_id, kind, place, work, shadow, ref, commit_id, tree, head, files, bytes, took_ms, made_ms, state";

fn checkpoint_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Checkpoint> {
    Ok(Checkpoint {
        id: r.get(0)?,
        task: TaskId(r.get(1)?),
        run: RunId(r.get(2)?),
        kind: r.get(3)?,
        place: Kind::from_name(&r.get::<_, String>(4)?),
        work: r.get(5)?,
        shadow: r.get(6)?,
        refname: r.get(7)?,
        commit: r.get(8)?,
        tree: r.get(9)?,
        head: r.get(10)?,
        files: r.get::<_, i64>(11)? as u64,
        bytes: r.get::<_, i64>(12)? as u64,
        took_ms: r.get::<_, i64>(13)? as u64,
        made_ms: r.get(14)?,
        state: r.get(15)?,
    })
}

/// Reads a run's undo from its columns (`undo_state`, `undo_why`, `undo_files`, `undo_added`, `undo_removed`, `undo_committed`,
/// from `first` on).
pub(crate) fn undo_of(r: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Option<RunUndo>> {
    let Some(state) = r.get::<_, Option<String>>(first)? else { return Ok(None) };
    let changes = match (r.get::<_, Option<u32>>(first + 2)?, r.get::<_, Option<u32>>(first + 3)?, r.get::<_, Option<u32>>(first + 4)?) {
        (Some(files), Some(added), Some(removed)) => Some(Changes { files, added, removed }),
        _ => None,
    };
    Ok(Some(RunUndo { state: status_of::<UndoState>(&state)?, why: r.get(first + 1)?, changes, committed: r.get::<_, Option<i64>>(first + 5)?.unwrap_or(0) != 0 }))
}

impl Store {
    /// Stores a checkpoint and its event.
    pub fn insert_checkpoint(&mut self, c: &Checkpoint, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            &format!("INSERT INTO checkpoints ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)"),
            params![
                c.id,
                c.task.0,
                c.run.0,
                c.kind,
                c.place.name(),
                c.work,
                c.shadow,
                c.refname,
                c.commit,
                c.tree,
                c.head,
                c.files as i64,
                c.bytes as i64,
                c.took_ms as i64,
                c.made_ms,
                c.state
            ],
        )?;
        let seq = crate::records::log_event(&tx, now_ms, &c.id, "checkpoint_made", serde_json::json!({ "task": c.task.0, "run": c.run.0, "kind": c.kind, "files": c.files, "bytes": c.bytes }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// A task's checkpoints kept or removed, in the order they were made.
    pub fn checkpoints_of(&self, task: &TaskId) -> Result<Vec<Checkpoint>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {COLUMNS} FROM checkpoints WHERE task_id = ?1 ORDER BY rowid"))?;
        let rows = st.query_map([&task.0], checkpoint_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every checkpoint still kept.
    pub fn kept_checkpoints(&self) -> Result<Vec<Checkpoint>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {COLUMNS} FROM checkpoints WHERE state = 'kept' ORDER BY rowid"))?;
        let rows = st.query_map([], checkpoint_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// A run's newest kept checkpoint of `kind`.
    pub fn checkpoint_of_run(&self, run: &RunId, kind: &str) -> Result<Option<Checkpoint>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {COLUMNS} FROM checkpoints WHERE run_id = ?1 AND kind = ?2 AND state = 'kept' ORDER BY rowid DESC LIMIT 1"))?;
        Ok(st.query_row(params![run.0, kind], checkpoint_of).optional()?)
    }

    /// Checkpoints whose refs are gone: marked removed, with one event.
    pub fn remove_checkpoints(&mut self, ids: &[String], now_ms: i64) -> Result<Option<u64>, StoreError> {
        if ids.is_empty() {
            return Ok(None);
        }
        let tx = self.conn.transaction()?;
        for id in ids {
            tx.execute("UPDATE checkpoints SET state = 'removed' WHERE id = ?1", [id])?;
        }
        let seq = crate::records::log_event(&tx, now_ms, "checkpoints", "checkpoints_removed", serde_json::json!({ "count": ids.len() }))?;
        tx.commit()?;
        Ok(Some(seq))
    }

    /// Where a run's undo stands now, with its event. Returns the run as it is now.
    pub fn set_run_undo(&mut self, run: &RunId, u: &RunUndo, now_ms: i64) -> Result<(Run, u64), StoreError> {
        let tx = self.conn.transaction()?;
        let (files, added, removed) = u.changes.map_or((None, None, None), |c| (Some(c.files), Some(c.added), Some(c.removed)));
        tx.execute(
            "UPDATE runs SET undo_state = ?2, undo_why = ?3, undo_files = ?4, undo_added = ?5, undo_removed = ?6, undo_committed = ?7 WHERE id = ?1",
            params![run.0, status_name(u.state), u.why, files, added, removed, u.committed as i64],
        )?;
        let seq = crate::records::log_event(&tx, now_ms, &run.0, "run_undo", serde_json::json!({ "state": status_name(u.state) }))?;
        tx.commit()?;
        let r = self.get_run(run)?.ok_or(StoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
        Ok((r, seq))
    }
}

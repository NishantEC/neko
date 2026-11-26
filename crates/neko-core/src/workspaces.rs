//! Workspaces (docs/knowledge/workspaces.md): folders the person added, each with a name, and General, the one with no folder.
//!
//! - One workspace per folder. Folders are compared with links resolved; adding a folder already added answers with its workspace
//!   (and brings it back if it was removed). A folder inside another workspace's folder is a workspace of its own.
//! - Nothing is scanned or found by itself: only folders the person adds.
//! - Removing one only takes it off the list: nothing on disk is touched, its tasks stay readable, no new task is made in it.
//! - A task belongs to exactly one workspace, chosen when it is made, never changed.

use std::path::{Path, PathBuf};

use neko_protocol::{TaskId, Workspace};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::store::{DEFAULT_WORKSPACE, Store, StoreError};

/// The longest name a workspace keeps, in characters.
pub const MAX_NAME: usize = 80;

const WS_COLUMNS: &str = "id, name, folder, created_ms, removed_ms, position";

/// General first, then the person's order, then the removed ones (most recently removed last).
const WS_ORDER: &str = "ORDER BY (id != 'default'), (removed_ms IS NOT NULL), position, removed_ms, created_ms, id";

fn workspace_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace { id: r.get(0)?, name: r.get(1)?, folder: r.get(2)?, created_ms: r.get(3)?, removed_ms: r.get(4)?, position: r.get(5)?, missing: false })
}

/// Whether the workspace's folder is gone, as the disk says now.
pub fn with_missing(mut w: Workspace) -> Workspace {
    w.missing = w.folder.as_deref().is_some_and(|f| !Path::new(f).is_dir());
    w
}

/// A folder's own name, for a new workspace ("acme-api" for `/Users/example/acme-api`).
pub fn folder_name(folder: &Path) -> String {
    folder.file_name().map_or_else(|| folder.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned())
}

/// Why a folder cannot be a workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FolderError {
    NotThere(String),
    NotAFolder(String),
    Unreadable(String),
}

impl std::fmt::Display for FolderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FolderError::NotThere(p) => write!(f, "There is no folder at {p}."),
            FolderError::NotAFolder(p) => write!(f, "{p} is a file, not a folder."),
            FolderError::Unreadable(p) => write!(f, "Neko cannot read {p}. Check its permissions in the Finder."),
        }
    }
}

/// The folder as it is stored: absolute, with links resolved. It must be there, be a folder and be readable.
pub fn check_folder(folder: &str) -> Result<PathBuf, FolderError> {
    let given = Path::new(folder.trim());
    if !given.is_absolute() {
        return Err(FolderError::NotThere(folder.to_owned()));
    }
    let real = given.canonicalize().map_err(|_| FolderError::NotThere(folder.to_owned()))?;
    if !real.is_dir() {
        return Err(FolderError::NotAFolder(folder.to_owned()));
    }
    std::fs::read_dir(&real).map_err(|_| FolderError::Unreadable(folder.to_owned()))?;
    Ok(real)
}

fn log(tx: &Transaction<'_>, now_ms: i64, workspace: &str, subject: &str, kind: &str, data: serde_json::Value) -> rusqlite::Result<u64> {
    tx.execute("INSERT INTO events (at_ms, workspace_id, subject, kind, data) VALUES (?1, ?2, ?3, ?4, ?5)", params![now_ms, workspace, subject, kind, data.to_string()])?;
    Ok(tx.last_insert_rowid() as u64)
}

fn next_position(tx: &Transaction<'_>) -> rusqlite::Result<i64> {
    tx.query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM workspaces WHERE id != 'default' AND removed_ms IS NULL", [], |r| r.get(0))
}

/// Migration 4's step: every folder a task was made in becomes a workspace (one per folder, links resolved where the folder is
/// still there), named after the folder, and its tasks move into it. Tasks with no folder stay in General. The new workspace's id
/// comes from its oldest task's, so the step needs no clock and no randomness and gives the same result every time.
pub(crate) fn migrate_task_folders(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    let found: Vec<(String, String, i64)> = {
        let mut st = tx.prepare("SELECT folder, MIN(id), MIN(created_ms) FROM tasks WHERE workspace_id = 'default' AND folder IS NOT NULL AND folder != '' GROUP BY folder ORDER BY MIN(created_ms), MIN(id)")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?
    };
    for (folder, first_task, created) in found {
        let real = Path::new(&folder).canonicalize().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| folder.clone());
        let existing: Option<String> = tx.query_row("SELECT id FROM workspaces WHERE folder = ?1", [&real], |r| r.get(0)).optional()?;
        let id = match existing {
            Some(id) => id,
            None => {
                let id = format!("ws_{}", first_task.trim_start_matches("task_"));
                let position = next_position(tx)?;
                tx.execute(
                    "INSERT INTO workspaces (id, name, created_ms, folder, position) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, folder_name(Path::new(&real)), created, real, position],
                )?;
                log(tx, created, &id, &id, "workspace_added", serde_json::json!({ "folder": real, "migrated": true }))?;
                id
            }
        };
        tx.execute("UPDATE tasks SET workspace_id = ?1 WHERE workspace_id = 'default' AND folder = ?2", params![id, folder])?;
    }
    Ok(())
}

impl Store {
    /// Every workspace: General first, then the person's order, then the removed ones. `missing` is not filled in here.
    pub fn list_workspaces(&self) -> Result<Vec<Workspace>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {WS_COLUMNS} FROM workspaces {WS_ORDER}"))?;
        let rows = st.query_map([], workspace_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_workspace(&self, id: &str) -> Result<Option<Workspace>, StoreError> {
        Ok(self.conn.query_row(&format!("SELECT {WS_COLUMNS} FROM workspaces WHERE id = ?1"), [id], workspace_of).optional()?)
    }

    /// The workspace of a folder (as stored: absolute, links resolved), removed or not.
    pub fn workspace_by_folder(&self, folder: &str) -> Result<Option<Workspace>, StoreError> {
        Ok(self.conn.query_row(&format!("SELECT {WS_COLUMNS} FROM workspaces WHERE folder = ?1"), [folder], workspace_of).optional()?)
    }

    fn read_workspace(tx: &Transaction<'_>, id: &str) -> rusqlite::Result<Workspace> {
        tx.query_row(&format!("SELECT {WS_COLUMNS} FROM workspaces WHERE id = ?1"), [id], workspace_of)
    }

    /// A new workspace at the end of the list.
    pub fn add_workspace(&mut self, id: &str, folder: &str, name: &str, now_ms: i64) -> Result<(Workspace, u64), StoreError> {
        let tx = self.conn.transaction()?;
        let position = next_position(&tx)?;
        tx.execute("INSERT INTO workspaces (id, name, created_ms, folder, position) VALUES (?1, ?2, ?3, ?4, ?5)", params![id, name, now_ms, folder, position])?;
        let seq = log(&tx, now_ms, id, id, "workspace_added", serde_json::json!({ "folder": folder }))?;
        let w = Self::read_workspace(&tx, id)?;
        tx.commit()?;
        Ok((w, seq))
    }

    /// A removed workspace back on the list, at its end.
    pub fn restore_workspace(&mut self, id: &str, now_ms: i64) -> Result<(Workspace, u64), StoreError> {
        let tx = self.conn.transaction()?;
        let position = next_position(&tx)?;
        tx.execute("UPDATE workspaces SET removed_ms = NULL, position = ?2 WHERE id = ?1", params![id, position])?;
        let seq = log(&tx, now_ms, id, id, "workspace_restored", serde_json::json!({}))?;
        let w = Self::read_workspace(&tx, id)?;
        tx.commit()?;
        Ok((w, seq))
    }

    pub fn rename_workspace(&mut self, id: &str, name: &str, now_ms: i64) -> Result<(Workspace, u64), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE workspaces SET name = ?2 WHERE id = ?1", params![id, name])?;
        let seq = log(&tx, now_ms, id, id, "workspace_renamed", serde_json::json!({ "name": name }))?;
        let w = Self::read_workspace(&tx, id)?;
        tx.commit()?;
        Ok((w, seq))
    }

    /// Off the list. Nothing else changes: its tasks keep their rows, its folder is not touched.
    pub fn remove_workspace(&mut self, id: &str, now_ms: i64) -> Result<(Workspace, u64), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE workspaces SET removed_ms = ?2 WHERE id = ?1 AND removed_ms IS NULL", params![id, now_ms])?;
        let seq = log(&tx, now_ms, id, id, "workspace_removed", serde_json::json!({}))?;
        let w = Self::read_workspace(&tx, id)?;
        tx.commit()?;
        Ok((w, seq))
    }

    /// The workspace's folder is now `folder`. Its tasks' folder follows, and so does the working folder of a task that works in
    /// the folder directly (not a repository).
    pub fn locate_workspace(&mut self, id: &str, folder: &str, now_ms: i64) -> Result<(Workspace, u64), StoreError> {
        let tx = self.conn.transaction()?;
        let old: Option<String> = tx.query_row("SELECT folder FROM workspaces WHERE id = ?1", [id], |r| r.get(0))?;
        tx.execute("UPDATE workspaces SET folder = ?2 WHERE id = ?1", params![id, folder])?;
        tx.execute("UPDATE tasks SET folder = ?2 WHERE workspace_id = ?1", params![id, folder])?;
        if let Some(old) = &old {
            tx.execute("UPDATE tasks SET workdir = ?3 WHERE workspace_id = ?1 AND plain = 1 AND workdir = ?2", params![id, old, folder])?;
            tx.execute("UPDATE tasks SET repo = ?3 WHERE workspace_id = ?1 AND repo = ?2", params![id, old, folder])?;
        }
        let seq = log(&tx, now_ms, id, id, "workspace_located", serde_json::json!({ "folder": folder }))?;
        let w = Self::read_workspace(&tx, id)?;
        tx.commit()?;
        Ok((w, seq))
    }

    /// Puts a workspace at `to` among the ones the person added and has not removed (0 is first, after General). Returns every
    /// workspace whose place changed, and the revision.
    pub fn move_workspace(&mut self, id: &str, to: usize, now_ms: i64) -> Result<(Vec<Workspace>, u64), StoreError> {
        let tx = self.conn.transaction()?;
        let mut order: Vec<(String, i64)> = {
            let mut st = tx.prepare(&format!("SELECT id, position FROM workspaces WHERE id != 'default' AND removed_ms IS NULL {}", "ORDER BY position, created_ms, id"))?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
        };
        let Some(at) = order.iter().position(|(w, _)| w == id) else {
            tx.commit()?;
            return Ok((Vec::new(), 0));
        };
        let moved = order.remove(at);
        order.insert(to.min(order.len()), moved);
        let mut changed = Vec::new();
        for (i, (w, before)) in order.iter().enumerate() {
            let position = i as i64 + 1;
            if *before != position {
                tx.execute("UPDATE workspaces SET position = ?2 WHERE id = ?1", params![w, position])?;
                changed.push(w.clone());
            }
        }
        let seq = log(&tx, now_ms, id, id, "workspace_moved", serde_json::json!({ "to": to }))?;
        let changed = changed.iter().map(|w| Self::read_workspace(&tx, w)).collect::<Result<Vec<_>, _>>()?;
        tx.commit()?;
        Ok((changed, seq))
    }

    /// Forgets a task: its runs, their usage, its parts and requests, and its row. The event log keeps that it was removed.
    pub fn delete_task(&mut self, id: &TaskId, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        let workspace: String = tx.query_row("SELECT workspace_id FROM tasks WHERE id = ?1", [&id.0], |r| r.get(0)).optional()?.unwrap_or_else(|| DEFAULT_WORKSPACE.to_owned());
        tx.execute("DELETE FROM usage WHERE run_id IN (SELECT id FROM runs WHERE task_id = ?1)", [&id.0])?;
        tx.execute("DELETE FROM permissions WHERE task_id = ?1", [&id.0])?;
        tx.execute("DELETE FROM checkpoints WHERE task_id = ?1", [&id.0])?;
        tx.execute("DELETE FROM parts WHERE task_id = ?1", [&id.0])?;
        tx.execute("DELETE FROM runs WHERE task_id = ?1", [&id.0])?;
        tx.execute("DELETE FROM tasks WHERE id = ?1", [&id.0])?;
        let seq = log(&tx, now_ms, &workspace, &id.0, "task_removed", serde_json::json!({}))?;
        tx.commit()?;
        Ok(seq)
    }
}

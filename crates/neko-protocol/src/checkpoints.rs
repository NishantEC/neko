//! Per-run checkpoints on the wire: what the task view lists, and the commands
//! that undo, redo and snapshot. See docs/knowledge/checkpoints-and-undo.md.
use serde::{Deserialize, Serialize};

/// One run's line in the task view.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CheckpointRun {
    pub n: u32,
    /// `running`, `done`, `failed` or `cancelled`.
    pub outcome: String,
    pub started_ms: i64,
    pub ended_ms: i64,
    pub files: usize,
    pub added: usize,
    pub removed: usize,
    /// Restored away by an undo (until a redo).
    pub undone: bool,
    /// Files left out of its snapshots for being over the size guard.
    pub skipped: Vec<String>,
    pub skipped_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CheckpointList {
    pub runs: Vec<CheckpointRun>,
    /// What a redo would bring back, when an undo can be redone.
    pub redo: Option<String>,
    /// The folder has edits no checkpoint holds.
    pub unsaved: bool,
    /// A run is working in the folder, so undo is off.
    pub busy: bool,
    /// Whether the folder exists to restore (not parked or gone).
    pub restorable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointCommand {
    /// Restore the folder to the start of a run.
    Undo {
        task_id: String,
        run: u32,
        /// Also undo the runs after it ("Undo back to here").
        #[serde(default)]
        through_later: bool,
        /// Snapshot edits no checkpoint holds, then undo.
        #[serde(default)]
        snapshot_unsaved: bool,
        /// Keep the history and add a commit that reverts the run.
        #[serde(default)]
        revert: bool,
    },
    /// Undo the last undo.
    Redo { task_id: String },
    /// Keep a snapshot of edits made outside any run.
    SnapshotUnsaved { task_id: String },
}

/// A refusal travels as an error message beginning with this and the code:
/// `checkpoint:<code>: <message>`.
pub const REFUSAL_PREFIX: &str = "checkpoint:";

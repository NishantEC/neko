//! Undo (docs/knowledge/undo.md): where a run's undo stands, its changes as a diff, and what a run's card offers.
//!
//! [`offers`] is the one place that decides which run may be undone or redone. The daemon refuses a request it does not offer;
//! the app draws what it offers. Both call it with the task's runs, so they cannot disagree.

use serde::{Deserialize, Serialize};

use crate::{Changes, Run, RunId, RunStatus, TaskId};

/// How many lines of one file's diff come at first; "Show all" asks for the rest.
pub const CUT_LINES: usize = 1_000;
/// The most lines one file's diff ever brings, even with "Show all".
pub const ALL_LINES: usize = 50_000;

/// Where a run's undo stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoState {
    /// The run is working: its start checkpoint is taken (or being taken), its end one is not yet.
    Taking,
    /// No checkpoint was made for it (too big, a repository mid-merge, …): `why` says why. The run itself still worked.
    Unavailable,
    /// Both checkpoints exist and the run's changes are in the folder.
    Applied,
    /// Undone: the folder is as it was before the run. Redo puts the changes back.
    Undone,
    /// Undone, and a newer run started from there: it can no longer be redone.
    Dropped,
    /// Its checkpoints were removed (cleared, pruned, or tidied away with its worktree).
    Removed,
}

/// A run's undo, as the daemon keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunUndo {
    pub state: UndoState,
    /// Why there is no checkpoint, as the end of a sentence ("the folder has 12,000 new or changed files").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// What the run itself changed: its start checkpoint against its end one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<Changes>,
    /// The run made commits on the task's branch: it is not undone (revert the commit instead), but its diff stays readable.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub committed: bool,
}

impl RunUndo {
    /// Both checkpoints exist.
    pub fn ready(&self) -> bool {
        matches!(self.state, UndoState::Applied | UndoState::Undone | UndoState::Dropped)
    }

    /// It changed something: a file, or a commit.
    pub fn changed(&self) -> bool {
        self.ready() && (self.committed || self.changes.is_some_and(|c| c.files > 0))
    }
}

/// How one file changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    /// A file became a link, or the other way round.
    TypeChanged,
    /// A submodule now points at another commit.
    Submodule,
}

/// One file of a diff, without its lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFile {
    /// Its path in the task's folder (the new one, when renamed).
    pub path: String,
    /// Where it was before, when renamed or copied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub added: u32,
    pub removed: u32,
    /// Its lines are not text: it is named with its sizes, never shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// Bytes before and after (binary files only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_size: Option<u64>,
    /// Its mode before and after, when only that or that too changed ("100644", "100755").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_mode: Option<String>,
}

/// A run's changes (or the task's, with no run), file by file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub task: TaskId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    /// The folder the paths are in (the worktree's top folder, or the plain folder).
    pub root: String,
    pub files: Vec<DiffFile>,
    pub added: u32,
    pub removed: u32,
}

/// One file's lines: a unified diff's hunks, each line with its mark first (`@`, `+`, `-`, a space, or `\`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffPatch {
    pub path: String,
    pub lines: Vec<String>,
    /// How many lines the whole diff has.
    pub total: u32,
    /// Cut: `lines` is the first part only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
}

/// The room undo takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UndoStorage {
    /// Bytes on disk in Neko's own undo folder (the shadow repositories).
    pub bytes: u64,
    /// Checkpoints kept, in repositories and shadow repositories.
    pub checkpoints: u32,
    /// Tasks that have them.
    pub tasks: u32,
}

/// What a run's card can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Undo,
    Redo,
}

/// What a run's card shows for undo.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Offer {
    /// Review opens the run's diff.
    pub review: bool,
    /// Undo or Redo, and whether it can be pressed now.
    pub action: Option<(Action, bool)>,
    /// One quiet line for the special cases.
    pub line: Option<String>,
    /// The run's changes are not in the folder now: its steps are marked "undone".
    pub undone: bool,
}

pub const LINE_COMMITTED: &str = "This run committed its work; revert the commit instead.";
pub const LINE_NEWER_FIRST: &str = "Undo the newer run first: runs are undone newest first.";
pub const LINE_NEWER_UNSAVED: &str = "A newer run has no checkpoint, so this one cannot be undone safely.";
pub const LINE_WAIT: &str = "Undo waits until the run in progress ends.";
pub const LINE_UNDONE: &str = "Undone: the files are as they were before this run.";
pub const LINE_REDO_EARLIER: &str = "Undone. Redo the earlier run first.";
pub const LINE_DROPPED: &str = "Undone. A newer message started from here, so it cannot be redone.";
pub const LINE_REMOVED: &str = "The undo history of this run was removed.";

/// What each run's card offers, for a task's runs oldest first (one [`Offer`] per run). Undo goes newest first: only the newest
/// run that changed something and is not undone may be undone, and only when no newer run lacks a checkpoint. Redo goes the other
/// way: the oldest undone run first. Nothing can be pressed while a run works.
pub fn offers(runs: &[Run]) -> Vec<Offer> {
    let working = runs.iter().any(|r| r.status == RunStatus::Running);
    let state = |r: &Run| r.undo.as_ref().map(|u| u.state);
    runs.iter()
        .enumerate()
        .map(|(i, r)| {
            let Some(u) = &r.undo else { return Offer::default() };
            let later = &runs[i + 1..];
            match u.state {
                UndoState::Taking => Offer::default(),
                UndoState::Unavailable => {
                    Offer { line: Some(format!("Undo is not available for this run: {}.", u.why.as_deref().unwrap_or("Neko could not take a checkpoint"))), ..Offer::default() }
                }
                UndoState::Removed if u.committed || u.changes.is_some_and(|c| c.files > 0) => Offer { line: Some(LINE_REMOVED.to_owned()), ..Offer::default() },
                UndoState::Removed => Offer::default(),
                _ if !u.changed() => Offer::default(),
                UndoState::Applied => {
                    let review = true;
                    if u.committed {
                        return Offer { review, line: Some(LINE_COMMITTED.to_owned()), ..Offer::default() };
                    }
                    let newer = later.iter().any(|x| x.undo.as_ref().is_some_and(|v| v.state == UndoState::Applied && v.changed()));
                    let unsaved = later.iter().any(|x| x.status != RunStatus::Running && matches!(state(x), Some(UndoState::Unavailable | UndoState::Removed)));
                    let (enabled, line) = if newer {
                        (false, Some(LINE_NEWER_FIRST))
                    } else if unsaved {
                        (false, Some(LINE_NEWER_UNSAVED))
                    } else if working {
                        (false, Some(LINE_WAIT))
                    } else {
                        (true, None)
                    };
                    Offer { review, action: Some((Action::Undo, enabled)), line: line.map(str::to_owned), undone: false }
                }
                UndoState::Undone => {
                    let earlier = runs[..i].iter().any(|x| state(x) == Some(UndoState::Undone));
                    let (enabled, line) = if earlier { (false, LINE_REDO_EARLIER) } else if working { (false, LINE_WAIT) } else { (true, LINE_UNDONE) };
                    Offer { review: true, action: Some((Action::Redo, enabled)), line: Some(line.to_owned()), undone: true }
                }
                UndoState::Dropped => Offer { review: true, action: None, line: Some(LINE_DROPPED.to_owned()), undone: true },
            }
        })
        .collect()
}

/// The words a follow-up carries first after the person undid runs, so the agent does not assume its edits are still there.
pub fn told(undone: usize) -> String {
    if undone == 1 {
        "(Neko: the person undid the changes of your last run, so the files are as they were before it. Do not assume those edits are still there.)".to_owned()
    } else {
        format!("(Neko: the person undid the changes of your last {undone} runs, so the files are as they were before them. Do not assume those edits are still there.)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(n: u32, status: RunStatus, undo: Option<RunUndo>) -> Run {
        Run {
            id: RunId(format!("run_{n}")),
            task: TaskId("task_1".into()),
            runtime: "claude".into(),
            model: None,
            status,
            started_ms: n as i64,
            ended_ms: None,
            session: None,
            workdir: "/Users/example/acme-api".into(),
            branch: None,
            reason: None,
            usage: None,
            changes: None,
            undo,
            cat: None,
            rules_hash: None,
        }
    }

    fn u(state: UndoState, files: u32) -> Option<RunUndo> {
        Some(RunUndo { state, why: None, changes: Some(Changes { files, added: files, removed: 0 }), committed: false })
    }

    #[test]
    fn only_the_newest_changed_run_is_undone_and_redo_goes_oldest_first() {
        let runs = vec![run(1, RunStatus::Done, u(UndoState::Applied, 2)), run(2, RunStatus::Done, u(UndoState::Applied, 1)), run(3, RunStatus::Done, u(UndoState::Applied, 0))];
        let o = offers(&runs);
        assert_eq!(o[0].action, Some((Action::Undo, false)));
        assert_eq!(o[0].line.as_deref(), Some(LINE_NEWER_FIRST));
        assert_eq!(o[1].action, Some((Action::Undo, true)), "the newest run that changed something");
        assert_eq!(o[2], Offer::default(), "a run that changed nothing offers nothing");

        let runs = vec![run(1, RunStatus::Done, u(UndoState::Undone, 2)), run(2, RunStatus::Done, u(UndoState::Undone, 1))];
        let o = offers(&runs);
        assert_eq!(o[0].action, Some((Action::Redo, true)));
        assert_eq!(o[1].action, Some((Action::Redo, false)));
        assert_eq!(o[1].line.as_deref(), Some(LINE_REDO_EARLIER));
        assert!(o[0].undone && o[1].undone);
    }

    #[test]
    fn the_special_cases_say_why_in_one_line() {
        let mut committed = u(UndoState::Applied, 1);
        committed.as_mut().unwrap().committed = true;
        let missing = Some(RunUndo { state: UndoState::Unavailable, why: Some("the repository is in the middle of a merge".into()), changes: None, committed: false });
        let runs = vec![run(1, RunStatus::Done, u(UndoState::Applied, 3)), run(2, RunStatus::Done, missing), run(3, RunStatus::Done, committed)];
        let o = offers(&runs);
        assert_eq!(o[2].line.as_deref(), Some(LINE_COMMITTED));
        assert_eq!(o[2].action, None);
        assert!(o[2].review, "the diff of a run that committed stays readable");
        assert_eq!(o[1].line.as_deref(), Some("Undo is not available for this run: the repository is in the middle of a merge."));
        assert_eq!(o[0].line.as_deref(), Some(LINE_NEWER_FIRST), "a newer run that committed still comes first");
        let o = offers(&runs[..2]);
        assert_eq!(o[0].action, Some((Action::Undo, false)));
        assert_eq!(o[0].line.as_deref(), Some(LINE_NEWER_UNSAVED));

        let runs = vec![run(1, RunStatus::Done, u(UndoState::Dropped, 3)), run(2, RunStatus::Done, u(UndoState::Applied, 1)), run(3, RunStatus::Running, u(UndoState::Taking, 0))];
        let o = offers(&runs);
        assert_eq!(o[0].line.as_deref(), Some(LINE_DROPPED));
        assert_eq!(o[1].action, Some((Action::Undo, false)), "nothing is pressed while a run works");
        assert_eq!(o[1].line.as_deref(), Some(LINE_WAIT));
        assert_eq!(o[2], Offer::default());
        assert_eq!(offers(&[run(1, RunStatus::Done, None)]), vec![Offer::default()], "a run from before undo offers nothing");
    }
}

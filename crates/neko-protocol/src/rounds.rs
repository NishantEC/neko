//! Rounds (docs/knowledge/rounds.md): a cat's standing work that starts by itself. The daemon keeps them and works out when each is
//! next due; a screen lists and changes them through requests and hears every change as an event.

use serde::{Deserialize, Serialize};

use crate::schedule::Schedule;
use crate::TaskId;

/// The longest round name, in characters.
pub const MAX_NAME: usize = 60;
/// How many rounds there may be.
pub const MAX_ROUNDS: usize = 64;
/// How many history lines a round keeps.
pub const HISTORY_KEPT: usize = 50;
/// A due time older than this when it is noticed (the Mac slept, the daemon was not running) is recorded as missed, not run.
pub const CATCH_UP_MS: i64 = 24 * 3_600_000;
/// Failed runs in a row after which a round turns itself off.
pub const BRAKE_AFTER: u32 = 3;

/// A cat's standing work that starts by itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Round {
    /// `round_` and 26 letters and digits.
    pub id: String,
    /// One line, 1 to [`MAX_NAME`] characters; the title of every task the round makes.
    pub name: String,
    /// What to do: the words of every task the round makes.
    pub words: String,
    /// The cat that does it (a [`crate::Cat`]'s id).
    pub cat: String,
    /// The workspace it works in (`default` is General).
    pub workspace: String,
    pub schedule: Schedule,
    /// The person's switch. A round the brake turned off is off too (see `note`).
    pub on: bool,
    pub created_ms: i64,
    pub changed_ms: i64,
    /// Why it does not run, or whether it will ([`RoundStatus`]).
    pub status: RoundStatus,
    /// The next time it is due, worked out by the daemon; present only while the round will run by itself (`status` scheduled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_due_ms: Option<i64>,
    /// A line about the round itself in words: the brake's ("Turned itself off after 3 failed runs in a row."), or why it does not
    /// run now. None when there is nothing to say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Its latest history line, if it has any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<RoundRun>,
}

/// Whether a round will run by itself, and if not, why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundStatus {
    /// On, and due at `next_due_ms`.
    Scheduled,
    /// On, with a manual schedule: only Run now.
    Manual,
    /// The person turned it off.
    Off,
    /// It failed [`BRAKE_AFTER`] runs in a row and turned itself off.
    Braked,
    /// Every round is paused.
    Paused,
    /// Its cat is retired.
    CatRetired,
    /// Its workspace's folder is not there.
    FolderMissing,
    /// Its workspace was removed from Neko.
    WorkspaceRemoved,
}

impl RoundStatus {
    /// Due by itself: nothing in the way.
    pub fn runs(self) -> bool {
        matches!(self, RoundStatus::Scheduled)
    }
}

/// What a due time came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    /// A task was made and its agent started.
    Started,
    /// The time came and nothing was started; `reason` says why.
    Skipped,
    /// The time passed while Neko was not running or the Mac slept, too long ago to run now; `reason` says so.
    Missed,
}

/// How a round's task ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEnd {
    Done,
    /// It ended with a question for the person.
    Asked,
    Failed,
    Stopped,
    /// The daemon stopped while it ran.
    Interrupted,
    /// The task was removed before it ended.
    Removed,
}

/// One line of a round's history: a due time and what came of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundRun {
    pub id: String,
    pub round: String,
    /// The time it was due (None: the person pressed Run now).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_ms: Option<i64>,
    /// When the daemon dealt with it: the start, the skip, the noticing.
    pub at_ms: i64,
    pub kind: RunKind,
    /// The task it made ([`RunKind::Started`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskId>,
    /// For a skip or a miss: why, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// How the task ended; None while it works or waits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<RunEnd>,
}

/// Where a task came from: the round that made it and the time it was due.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRound {
    pub id: String,
    /// The round's name when the task was made.
    pub name: String,
    /// The time it was due (None: Run now).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_ms: Option<i64>,
}

/// A round's name as it is kept: one line, spaces collapsed, 1 to [`MAX_NAME`] characters. Too long is refused, not cut.
pub fn clean_name(name: &str) -> Result<String, String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("A round needs a name.".into());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("A round's name can be at most {MAX_NAME} characters."));
    }
    Ok(name)
}

/// Whether a task's status stands for "still working or waiting for the person" (a round's next time is then skipped).
pub fn is_unfinished(status: crate::TaskStatus) -> bool {
    use crate::TaskStatus as S;
    matches!(status, S::New | S::Working | S::NeedsYou | S::Asked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_one_line_and_at_most_sixty_characters() {
        assert_eq!(clean_name("  Docs   check\n").unwrap(), "Docs check");
        assert!(clean_name("  ").is_err());
        assert!(clean_name(&"é".repeat(60)).is_ok());
        assert!(clean_name(&"é".repeat(61)).is_err());
    }
}

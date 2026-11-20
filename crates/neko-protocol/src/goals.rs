//! Goals that keep working: the wire types. See
//! docs/knowledge/goals-that-keep-working.md.
//!
//! A goal is the person's objective, an optional shell check that proves it,
//! constraints and a budget. Neko works its ticket turn by turn, runs the
//! check after each turn, asks a cheap judge for a verdict and keeps going
//! until the evidence says done. State lives in small rows of its own, never
//! the workbench snapshot; the snapshot only carries them to readers.
use serde::{Deserialize, Serialize};

/// Default turn budget.
pub const DEFAULT_MAX_TURNS: u32 = 20;
/// Default wall-clock budget: 12 hours of running (waiting and holds do not count).
pub const DEFAULT_MAX_WALL_MS: i64 = 12 * 60 * 60 * 1000;
pub const MAX_OBJECTIVE: usize = 4 * 1024;
pub const MAX_CHECK: usize = 2 * 1024;
pub const MAX_CONSTRAINTS: usize = 2 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum GoalStatus {
    /// A turn is running or the next one is due.
    #[default]
    Running,
    /// Parked until a time, a pull request state or a process exit.
    Waiting,
    /// One question for the person, with a default answer.
    Blocked,
    /// A usage limit; parked until it resets.
    Held,
    /// A cap was reached, or the person paused it.
    Paused,
    Done,
    Stopped,
}

impl GoalStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Waiting | Self::Blocked | Self::Held)
    }
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Done | Self::Stopped)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Budget {
    pub max_turns: u32,
    /// Running time, in ms. 0 means no wall-clock cap.
    pub max_wall_ms: i64,
    /// Reported agent cost, in cents. None means no cost cap.
    pub max_cost_cents: Option<u32>,
}

impl Default for Budget {
    fn default() -> Self {
        Self { max_turns: DEFAULT_MAX_TURNS, max_wall_ms: DEFAULT_MAX_WALL_MS, max_cost_cents: None }
    }
}

/// What a waiting goal waits for. Any one that is met ends the wait.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Wait {
    pub reason: String,
    pub until_ms: Option<i64>,
    /// `owner/repo#12` and the state to wait for ("merged", "closed").
    pub pull_request: Option<String>,
    pub pr_state: Option<String>,
    /// A process id that must exit.
    pub pid: Option<u32>,
    /// Waiting on the ticket itself (its plan needs the person's approval);
    /// the turn in flight carries on when the ticket moves.
    pub ticket: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Blocked {
    pub question: String,
    pub default_answer: String,
    /// The Inbox item raised for it (its `seq`), so it is raised once.
    pub inbox_seq: Option<u64>,
    /// When the question was asked (0 on a question from before deadline defaults: it is never auto-answered).
    pub asked_ms: i64,
    /// The judge supplied the default; the generic "use your best judgement" filler never counts.
    pub real_default: bool,
    /// Do not answer this question by itself (an undo set it, or no checkpoint exists to undo to).
    pub deadline_off: bool,
}

/// What Neko did when a deadline passed: the answer it took, and the undo it keeps for seven days.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DefaultTaken {
    pub at_ms: i64,
    pub after_hours: u32,
    pub question: String,
    pub answer: String,
    /// The run whose start checkpoint is the folder as it was right before the post-answer work.
    pub undo_run: u32,
    pub expires_ms: i64,
    pub inbox_seq: Option<u64>,
}

/// The longest deadline, in hours (a week).
pub const MAX_DEFAULT_AFTER_HOURS: u32 = 168;
/// How long the undo of a deadline answer is kept.
pub const DEFAULT_UNDO_KEPT_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CheckResult {
    pub command: String,
    pub exit_code: Option<i64>,
    /// The end of the output, bounded.
    pub tail: String,
    pub at_ms: i64,
    /// Who ran it: "neko" (the host ran the goal's check), "review" (an
    /// independent review's receipt) or "agent" (a command the agent ran).
    pub source: String,
}

impl CheckResult {
    pub fn passed(&self) -> bool {
        self.exit_code == Some(0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct JournalEntry {
    pub at_ms: i64,
    pub turn: u32,
    pub plan: String,
    pub done: String,
    pub next: String,
    /// The agent session the turn used, so a resume can continue it.
    pub session_id: Option<String>,
    /// What the judge ruled (DONE, CONTINUE, BLOCKED, WAIT) or what happened
    /// instead ("resumed after a restart", "stalled", "held").
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Goal {
    pub id: String,
    pub workspace_id: String,
    /// The person's words.
    pub objective: String,
    /// A shell command run in the task's folder; passing means done. Empty
    /// means "none": done then needs a concrete artifact or link.
    pub done_check: String,
    pub constraints: String,
    pub budget: Budget,
    /// The ticket that does the work.
    pub ticket_id: Option<String>,
    /// The chat the goal was set in ("home:<profile>:<workspace>").
    pub conversation_id: Option<String>,
    pub status: GoalStatus,
    /// Turns issued so far.
    pub turns: u32,
    /// A turn is in flight on the ticket.
    pub in_turn: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
    /// Running time spent so far, in ms.
    pub wall_ms: i64,
    pub last_tick_ms: i64,
    pub last_check: Option<CheckResult>,
    /// The judge's last ruling and reason.
    pub verdict: String,
    pub reason: String,
    pub wait: Option<Wait>,
    pub blocked: Option<Blocked>,
    /// Held until this time (a usage limit).
    pub hold_until_ms: Option<i64>,
    /// Why the goal is paused, when it is.
    pub paused_reason: String,
    /// The Inbox item "Extend by N turns?", raised once per pause.
    pub extend_seq: Option<u64>,
    /// Stalled turns resumed since the last good turn (one allowed).
    pub stall_resumes: u32,
    /// Consecutive judge failures, counted toward the turn cap anyway.
    pub judge_failures: u32,
    /// Capped at the last 12 for readers, newest last.
    pub ledger: Vec<CheckResult>,
    /// Capped at the last 20 for readers (the log keeps 50), newest last.
    pub journal: Vec<JournalEntry>,
    /// The receipt written when the goal reached Done (a markdown artifact).
    pub receipt: Option<ReceiptRef>,
    /// Failed attempts to write the receipt; the goal tick retries up to three.
    pub receipt_tries: u8,
    /// Off (`None`) unless the person set it: hours (1 to 168) after which a blocked goal's own default
    /// answer is taken, when its ticket has a checkpoint to undo to.
    pub default_after_hours: Option<u32>,
    /// The last deadline answer, while its undo is still kept.
    pub default_taken: Option<DefaultTaken>,
}

/// Where a finished goal's receipt is kept.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ReceiptRef {
    pub artifact_id: String,
    pub version: u32,
    pub at_ms: i64,
}

impl Goal {
    /// What its ledger and journal are filed under: the ticket, else itself.
    pub fn subject(&self) -> String {
        self.ticket_id.clone().unwrap_or_else(|| self.id.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoalCommand {
    /// Standalone goal; `ticket_id` attaches it to an existing ticket,
    /// otherwise a ticket is made for it in the workspace.
    Create {
        workspace_id: String,
        objective: String,
        #[serde(default)]
        done_check: String,
        #[serde(default)]
        constraints: String,
        #[serde(default)]
        max_turns: Option<u32>,
        #[serde(default)]
        ticket_id: Option<String>,
        #[serde(default)]
        conversation_id: Option<String>,
        #[serde(default)]
        default_after_hours: Option<u32>,
    },
    Pause { id: String },
    Resume { id: String },
    /// More turns for a paused goal, and resume it.
    Extend { id: String, turns: u32 },
    /// The person says it is done.
    MarkDone { id: String },
    Stop { id: String },
    /// Answers a blocked goal; empty text takes the default answer.
    Answer { id: String, text: String },
    /// Hours (1 to 168) after which a blocked goal takes its own default answer; `None` turns it off.
    SetDefaultAfter { id: String, hours: Option<u32> },
    /// Undoes a deadline answer: the folder goes back to before the work done on it, and the question comes
    /// back. Destructive: confirmed in the UI.
    UndoDefault { goal: String },
}

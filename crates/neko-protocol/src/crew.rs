//! The chat directs the crew: wire types for the durable command queue and the
//! orchestration tools. See docs/knowledge/the-chat-directs-the-crew.md.
use serde::{Deserialize, Serialize};

pub const MAX_QUEUED_TEXT: usize = 8 * 1024;
/// Open delegated tasks one chat (or one ticket) may have, unless raised.
pub const DEFAULT_DELEGATION_CAP: u32 = 8;
/// The longest `wait_for` may block, in seconds.
pub const MAX_WAIT_SECONDS: u64 = 30 * 60;

/// When a queued message reaches the agent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    /// Into the live session if the runtime can take input mid-turn, else at the next turn boundary.
    SteerNow,
    /// Held until the running turn ends.
    #[default]
    AfterTurn,
    /// Stops the running turn and restarts it with the message.
    InterruptRestart,
}

impl DeliveryMode {
    pub fn word(self) -> &'static str {
        match self {
            Self::SteerNow => "steer now",
            Self::AfterTurn => "after the turn",
            Self::InterruptRestart => "interrupt and restart",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "steer_now" => Some(Self::SteerNow),
            "after_turn" => Some(Self::AfterTurn),
            "interrupt_restart" => Some(Self::InterruptRestart),
            _ => None,
        }
    }
}

/// What the person or agent meant by it; the mode decides delivery.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QueueKind {
    #[default]
    Send,
    Steer,
    Interrupt,
    /// The answer to something the agent asked.
    Answer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QueueState {
    #[default]
    Queued,
    Delivered,
    Cancelled,
    /// Never delivered (the task is gone or was cancelled).
    Dropped,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct QueuedMessage {
    /// Caller-chosen or derived; the same id never queues twice.
    pub id: String,
    pub task_id: String,
    pub kind: QueueKind,
    pub mode: DeliveryMode,
    pub text: String,
    pub state: QueueState,
    /// "you", "chat" or "ticket <id>".
    pub source: String,
    pub created_ms: i64,
    pub settled_ms: i64,
    /// How it was delivered, or why it was not.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CrewCommand {
    /// Queue a message for a task's agent. `command_id` makes a retry idempotent.
    Enqueue {
        task_id: String,
        #[serde(default)]
        command_id: Option<String>,
        text: String,
        #[serde(default)]
        mode: DeliveryMode,
        #[serde(default)]
        kind: QueueKind,
    },
    /// Takes back a message that is still queued.
    CancelQueued { task_id: String, id: String },
    /// Raises or lowers how many open delegated tasks one chat may have.
    SetDelegationCap { limit: u32 },
}

/// What a run holding a lease may ask of the crew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrewRole {
    /// The full-power chat: every tool.
    Full,
    /// Read-only chat: list and status.
    ReadOnly,
    /// A ticket's own run: list, status, wait, and sub-tasks of its ticket.
    Task { ticket_id: String, may_create: bool },
}

pub const TOOLS: [&str; 9] = ["list_tasks", "create_task", "steer_task", "wait_for", "cancel_task", "schedule", "task_status", "save_artifact", "show_artifact"];

impl CrewRole {
    pub fn allows(&self, tool: &str) -> bool {
        match self {
            Self::Full => TOOLS.contains(&tool),
            Self::ReadOnly => matches!(tool, "list_tasks" | "task_status"),
            Self::Task { may_create, .. } => match tool {
                "list_tasks" | "task_status" | "wait_for" => true,
                "create_task" => *may_create,
                _ => false,
            },
        }
    }
    pub fn tools(&self) -> Vec<&'static str> {
        TOOLS.iter().copied().filter(|t| self.allows(t)).collect()
    }
    /// Who a call is audited as.
    pub fn label(&self) -> String {
        match self {
            Self::Full => "chat".into(),
            Self::ReadOnly => "read-only chat".into(),
            Self::Task { ticket_id, .. } => format!("ticket {ticket_id}"),
        }
    }
}

//! The self-improvement loop on the wire: what Neko suggests after it notices
//! the same trouble again, and how the person applies, rejects or undoes it.
//! See docs/knowledge/self-improvement.md.
use serde::{Deserialize, Serialize};

/// What a suggestion changes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImproveKind {
    /// Opens the pack's preview; the person installs it there.
    InstallPack,
    /// A revised copy of an existing skill, through the curator's rules.
    SkillPatch,
    /// A new skill, always reviewed in Skills.
    NewSkill,
    /// One line added to a workspace's standing orders.
    StandingOrder,
    /// A task in the Development checkout.
    CodeTask,
}

impl ImproveKind {
    /// The words the UIs show, muted, beside the title.
    pub fn label(self) -> &'static str {
        match self {
            Self::InstallPack => "Install a pack",
            Self::SkillPatch => "Improve a skill",
            Self::NewSkill => "New skill",
            Self::StandingOrder => "Standing order",
            Self::CodeTask => "Change Neko",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImproveState {
    /// Waiting for the person.
    #[default]
    Open,
    /// A skill change that now waits for the person's approval in Skills.
    Sent,
    /// Applied; its signal is being watched for 7 days.
    Applied,
    /// The signal stopped or fired less often.
    Helped,
    /// The signal kept firing. An Inbox item offers Undo.
    DidntHelp,
    /// Rejected, or applied and then undone. Not suggested again for 30 days.
    Rejected,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ImproveEvent {
    pub at_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Improvement {
    pub id: String,
    /// `missing_tool:<name>`, `correction:<topic>` or `ask:<title key>`.
    pub signal_key: String,
    pub kind: ImproveKind,
    pub title: String,
    pub why: String,
    /// What would change (a skill's complete revised file, a standing-order line,
    /// the request for a code task).
    pub change: String,
    /// What the Inbox default does ("Install the Linear pack").
    pub default_label: String,
    /// The pack or skill the change is for.
    pub target: String,
    pub workspace_id: String,
    /// The signal's evidence, at most 4 KB.
    pub evidence: String,
    /// A unified diff, for a skill patch.
    pub diff: String,
    pub state: ImproveState,
    pub created_ms: i64,
    pub applied_ms: Option<i64>,
    pub resolved_ms: Option<i64>,
    /// How often the signal fired in the 7 days before it was applied, and in the days since.
    pub before: u32,
    pub after: u32,
    pub inbox_seq: Option<u64>,
    pub undo_seq: Option<u64>,
    pub history: Vec<ImproveEvent>,
    /// Computed when listed: an Undo is possible, and whether it asks first.
    pub can_undo: bool,
    pub undo_confirms: bool,
    /// The task a code change created.
    pub task_id: Option<String>,
}

impl Default for Improvement {
    fn default() -> Self {
        Self {
            id: String::new(),
            signal_key: String::new(),
            kind: ImproveKind::StandingOrder,
            title: String::new(),
            why: String::new(),
            change: String::new(),
            default_label: String::new(),
            target: String::new(),
            workspace_id: String::new(),
            evidence: String::new(),
            diff: String::new(),
            state: ImproveState::Open,
            created_ms: 0,
            applied_ms: None,
            resolved_ms: None,
            before: 0,
            after: 0,
            inbox_seq: None,
            undo_seq: None,
            history: Vec::new(),
            can_undo: false,
            undo_confirms: false,
            task_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImproveCommand {
    List,
    /// Applies by kind: a pack opens its preview (`ImproveReply::open_pack`), nothing installs.
    Apply { id: String },
    Reject { id: String },
    /// Reverses an applied suggestion: a skill from its archive, a standing order's line, a pack by Remove,
    /// a code task by its checkpoints (or a revert task once it was pushed).
    Undo { id: String },
    /// "Suggestions" in Settings, AI. Off means no signal gathering at all.
    SetEnabled { enabled: bool },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ImproveReply {
    pub enabled: bool,
    /// A Development checkout is set, so code changes may be suggested.
    pub dev_checkout: bool,
    /// Newest first.
    pub proposals: Vec<Improvement>,
    /// After applying a pack suggestion: the source (`catalog:<name>`) whose preview the client opens.
    pub open_pack: Option<String>,
    /// One line about what the last command did.
    pub note: Option<String>,
}

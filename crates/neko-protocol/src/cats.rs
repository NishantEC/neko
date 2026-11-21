//! Cats: a named agent that looks after one workspace's crew of sessions.
//! The wire types live here; storage and the rules are `neko_core::cats`
//! and `neko_core::cat_files`. See `docs/knowledge/cats.md`.
use crate::workbench::RuntimePreferences;
use serde::{Deserialize, Deserializer, Serialize};

/// The faces a cat may wear. The one list: the daemon refuses anything else
/// and the clients draw these from vectors in code (never from files).
pub const FACE_PRESETS: [&str; 6] = ["black-glasses", "persian-glasses", "tabby-headphones", "grey-scarf", "cream-bowtie", "calico-welding"];

/// Name limits, in characters.
pub const MAX_NAME_CHARS: usize = 40;
pub const MAX_AREA_CHARS: usize = 40;
pub const MAX_PURPOSE_CHARS: usize = 2_000;
/// The most a cat may be limited to per day: 1,000,000 dollars, in micro-dollars.
pub const MAX_DAILY_LIMIT_MICRO_USD: u64 = 1_000_000_000_000;
/// Active (not archived) cats at once.
pub const MAX_ACTIVE_CATS: usize = 16;
/// The most one cat file holds.
pub const MAX_CAT_FILE: usize = 16 * 1024;
/// The longest message a person may post to a cat, in characters.
pub const MAX_SAY_CHARS: usize = 8_000;
/// Messages one `CatThread` answer carries at most.
pub const THREAD_PAGE: usize = 200;

/// A preset id such as `black-glasses` (one of `FACE_PRESETS`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(transparent)]
pub struct FaceRef(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ModelPolicy {
    /// The cat's own turns and the default for its sessions; all `None` = the stored default runtime.
    pub preferences: RuntimePreferences,
    /// The cat may pick a model for each session it starts.
    pub per_session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Cat {
    pub id: String,
    /// 1 to 40 characters, trimmed; unique among active cats, ignoring case.
    pub name: String,
    pub face: FaceRef,
    /// Must exist; the cat only works inside it, and it never changes.
    pub workspace_id: String,
    /// A short label ("Web"), at most 40 characters.
    pub area: String,
    pub model_policy: ModelPolicy,
    /// `None` = no limit of its own.
    pub daily_limit_micro_usd: Option<u64>,
    /// At most 2,000 characters; seeds `RULES.md`.
    pub purpose: String,
    pub pinned: bool,
    pub order: i32,
    pub created_at_ms: i64,
    pub archived: bool,
}

/// `Some(None)` for an explicit JSON `null`, `None` when the field is missing.
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<u64>>, D::Error> {
    Option::<u64>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CatCommand {
    /// Active cats in order, plus archived ones (last) when asked. Answered with `Listing`.
    List {
        #[serde(default)]
        include_archived: bool,
    },
    /// Makes a cat and its folder of files. An empty `face` takes the default for the name. Answered with `Listing`.
    Create {
        name: String,
        #[serde(default)]
        face: FaceRef,
        workspace_id: String,
        #[serde(default)]
        area: String,
        #[serde(default)]
        model_policy: ModelPolicy,
        #[serde(default)]
        daily_limit_micro_usd: Option<u64>,
        #[serde(default)]
        purpose: String,
    },
    /// Changes the given fields only (never the workspace). `daily_limit_micro_usd: null` removes the limit. Answered with `Listing`.
    Update {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        face: Option<FaceRef>,
        #[serde(default)]
        area: Option<String>,
        #[serde(default)]
        model_policy: Option<ModelPolicy>,
        // Not written when unchanged: a `null` on the wire means "remove the limit", so a client that serialised `None` as `null` would clear it.
        #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
        daily_limit_micro_usd: Option<Option<u64>>,
        #[serde(default)]
        purpose: Option<String>,
    },
    /// Hides the cat and unpins it. Its files stay; nothing is deleted. Destructive: clients confirm. Answered with `Listing`.
    Archive { id: String },
    Pin { id: String, pinned: bool },
    /// `ids` must be exactly the active cats, in the new order.
    Reorder { ids: Vec<String> },
    /// A proposed first cat from the busiest workspace. A read; creates nothing. Answered with `Suggestion`.
    Suggest,
    /// The four files of a cat. Answered with `Files`.
    Files { id: String },
    /// Replaces one file (the previous text is kept as `.prev`). Empty text clears it: destructive, clients confirm. Answered with `Files`.
    WriteFile { id: String, kind: CatFileKind, text: String },
    /// A read: the cat's own thread, messages with `seq > since_seq`, oldest first, at most `THREAD_PAGE`
    /// (with `since_seq` 0, the latest `THREAD_PAGE`). Answered with `Thread`. Clients ask again when
    /// `Snapshot::cats_rev` moves.
    CatThread {
        cat: String,
        #[serde(default)]
        since_seq: u64,
    },
    /// The person speaks to the cat: the message is stored in its thread and wakes it (the cat answers in
    /// the thread, never in the Home chat). Answered with `Thread` holding the stored message.
    CatSay { cat: String, text: String },
}

/// Who wrote a line of a cat's thread.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CatMessageRole {
    #[default]
    Person,
    Cat,
    /// Something that happened (a turn failed, a turn was held), not a spoken line.
    Event,
}

impl CatMessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Cat => "cat",
            Self::Event => "event",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "person" => Some(Self::Person),
            "cat" => Some(Self::Cat),
            "event" => Some(Self::Event),
            _ => None,
        }
    }
}

/// One row of `cat_messages`: append-only, `seq` counts from 1 within a cat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct CatMessage {
    pub cat_id: String,
    pub seq: u64,
    pub role: CatMessageRole,
    pub text: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct CatThreadView {
    pub cat_id: String,
    pub messages: Vec<CatMessage>,
    /// The newest `seq` in the thread (0 when empty), whatever this answer carries.
    pub last_seq: u64,
    /// A turn of this cat is running now.
    pub running: bool,
}

/// Why a cat is woken (`cat_wakes.reason`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WakeReason {
    /// One of its sessions finished a turn.
    TurnFinished,
    /// One of its sessions failed.
    Failed,
    /// One of its sessions needs the person (a plan to approve, a question).
    NeedsYou,
    /// The person posted to the cat's thread.
    PersonMessage,
}

impl WakeReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TurnFinished => "turn_finished",
            Self::Failed => "failed",
            Self::NeedsYou => "needs_you",
            Self::PersonMessage => "person_message",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "turn_finished" => Some(Self::TurnFinished),
            "failed" => Some(Self::Failed),
            "needs_you" => Some(Self::NeedsYou),
            "person_message" => Some(Self::PersonMessage),
            _ => None,
        }
    }
}

/// One row of `cat_wakes`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatWake {
    pub id: u64,
    pub cat_id: String,
    pub reason: WakeReason,
    /// The session concerned; empty for a person's message.
    pub task_id: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CatFileKind {
    Rules,
    TaskRules,
    Plan,
    Briefing,
}

impl CatFileKind {
    pub const ALL: [Self; 4] = [Self::Rules, Self::TaskRules, Self::Plan, Self::Briefing];

    pub fn file_name(self) -> &'static str {
        match self {
            Self::Rules => "RULES.md",
            Self::TaskRules => "TASK_RULES.md",
            Self::Plan => "PLAN.md",
            Self::Briefing => "BRIEFING.md",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatFile {
    pub kind: CatFileKind,
    pub text: String,
    /// File modification time; 0 when there is no file.
    pub updated_ms: i64,
    /// What the file held before its last write, when that is kept.
    pub prev: Option<String>,
    /// The file on disk is larger than 16 KB (hand-edited); `text` is its first 16 KB.
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatFiles {
    pub id: String,
    pub files: Vec<CatFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatSuggestion {
    pub name: String,
    pub face: FaceRef,
    pub workspace_id: String,
    pub area: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CatsReply {
    Listing(Vec<Cat>),
    Suggestion(CatSuggestion),
    Files(CatFiles),
    Thread(CatThreadView),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_json_object_parses_to_a_default_cat() {
        let cat: Cat = serde_json::from_str("{}").unwrap();
        assert_eq!(cat, Cat::default());
    }

    #[test]
    fn a_null_daily_limit_clears_and_a_missing_one_leaves_alone() {
        let clear: CatCommand = serde_json::from_str(r#"{"Update":{"id":"a","daily_limit_micro_usd":null}}"#).unwrap();
        let keep: CatCommand = serde_json::from_str(r#"{"Update":{"id":"a"}}"#).unwrap();
        let set: CatCommand = serde_json::from_str(r#"{"Update":{"id":"a","daily_limit_micro_usd":5}}"#).unwrap();
        let limit = |c: CatCommand| match c { CatCommand::Update { daily_limit_micro_usd, .. } => daily_limit_micro_usd, _ => panic!() };
        assert_eq!((limit(clear), limit(keep), limit(set)), (Some(None), None, Some(Some(5))));
    }
}

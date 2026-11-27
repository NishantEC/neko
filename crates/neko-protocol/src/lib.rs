//! What a screen and the daemon say to each other (docs/knowledge/protocol.md).
//!
//! Every message is one JSON object in one WebSocket text frame. A screen sends [`ClientMessage`]s, each a [`Request`] with an id
//! it chose; the daemon answers each with a [`ServerMessage::Reply`] carrying the same id, and, once the screen has asked to
//! [`Request::Subscribe`], pushes a [`ServerMessage::Event`] for every change. The first request on a connection is
//! [`Request::Hello`]; a screen speaking another [`PROTOCOL_VERSION`] is refused there.
//!
//! The types are the contract. Their JSON shape is pinned by the round-trip tests in `tests/round_trip.rs`; a change to a shape is a
//! change to the version.

use serde::{Deserialize, Serialize};

#[cfg(feature = "ws")]
pub mod client;
#[cfg(feature = "ws")]
pub mod pump;
pub mod briefing;
pub mod cats;
pub mod plugins;
pub mod rounds;
pub mod schedule;
pub mod title;
pub mod undo;

pub use briefing::{Briefing, BriefingCounts, BriefingSettings};
pub use cats::Cat;
pub use plugins::{Access, AccessMode, Approval, Check, FieldKind, FieldValue, Manifest, Plugin, PluginAsk, PluginKind, PluginState, SettingField};
pub use rounds::{Round, RoundRun, RoundStatus, TaskRound};
pub use schedule::Schedule;
pub use undo::{DiffFile, DiffPatch, DiffSummary, FileStatus, Offer, RunUndo, UndoState, UndoStorage};

/// The version of this contract. A screen and a daemon talk only when theirs are equal. 2: tasks run (S1). 3: three permission
/// modes, the context window, a command's exit code, Claude Code's own rules in words. 4: a task can be `asked` (its run ended with
/// a question for the person). 5: workspaces (a task is made in one; list, add, rename, remove, locate and move them; remove a
/// task). 6: "Needs you" (the items waiting for the person, listed and pushed whole; a failure's notice says what kind it is; a
/// follow-up may say which question it answers). 7: undo (a run's checkpoints and where its undo stands; a run's changes as a diff,
/// per file; undo, redo; the storage undo uses, and clearing it). 8: cats (named agents with standing rules; every task has one; a
/// run records its cat and the version of its rules; "Needs you" items name the cat). 9: rounds (a cat's standing work that starts
/// by itself: its schedule, its history, pause all; a task says which round made it and when it was due). 10: the Briefing (what
/// happened while the person was away, derived by the daemon; mark it read; the morning time; an event when it is due). 11: plugins (MCP
/// servers first: added by pasting JSON or from a folder, approved by digest, set up in Settings, left to chosen cats; a "Needs you"
/// item asks for the approval). 12: threads (what a thread last said and is doing, read and unread). 13: subagents (a task may belong to another, with a number; a status Waiting; a card and a wake note
/// in the transcript; a request from a subagent says who asked; a setting that gives runs the person's own MCP servers).
pub const PROTOCOL_VERSION: u32 = 13;

/// The WebSocket path, versioned with the contract.
pub const WS_PATH: &str = "/v1/ws";
/// The plain HTTP health and version check (same token).
pub const HELLO_PATH: &str = "/v1/hello";

/// The longest task a person may type, in bytes (a pasted page fits; a pasted log is refused with a reason).
pub const MAX_WORDS_BYTES: usize = 100_000;

macro_rules! id_type {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
    };
}

id_type!(
    /// A task's id, as the daemon made it (`task_` then 26 letters and digits).
    TaskId
);
id_type!(
    /// A run's id (`run_` then 26 letters and digits): one start of an agent on a task.
    RunId
);
id_type!(
    /// A part of a task's transcript (`part_` …).
    PartId
);
id_type!(
    /// A permission request (`perm_` …).
    PermissionId
);

/// Where a task stands: the state of its newest run, or new when none ever ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// No agent has worked on it.
    New,
    Working,
    /// Working, and waiting for the person to answer a request.
    NeedsYou,
    Done,
    Failed,
    Stopped,
    /// The daemon stopped while it ran; a follow-up continues it.
    Interrupted,
    /// The run finished with a question for the person (its last words end in a question mark, nothing pending): their turn. Not
    /// working; a follow-up answers it.
    Asked,
    /// No run, and the thread waits on its subagents: one is queued or working, or its report has not been read yet. The daemon wakes
    /// the thread when a report arrives (docs/knowledge/subagents.md).
    Waiting,
}

/// The id of General: the built-in workspace with no folder. Its tasks work in a scratch folder of their own.
pub const GENERAL: &str = "default";

/// A folder the person added, with a name (docs/knowledge/workspaces.md). General is the one with no folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// `default` for General, else `ws_` and 26 letters and digits.
    pub id: String,
    pub name: String,
    /// The folder, absolute and with links resolved. None: General.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    pub created_ms: i64,
    /// When the person removed it: it is then listed only for its tasks, and no new task can be made in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_ms: Option<i64>,
    /// Its place in the sidebar (General is always first).
    pub position: i64,
    /// Its folder was not there when the daemon last looked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

impl Workspace {
    pub fn is_general(&self) -> bool {
        self.id == GENERAL
    }

    /// Not removed: a new task can be made in it (when its folder is there).
    pub fn is_active(&self) -> bool {
        self.removed_ms.is_none()
    }
}

/// A task as a screen sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    /// The workspace it was made in; never changes.
    pub workspace: String,
    /// The person's words, exactly as sent.
    pub words: String,
    /// A short title made from the words (their first line, cut at a word near 60 characters: [`title::title`]).
    pub title: String,
    /// Milliseconds since the Unix epoch.
    pub created_ms: i64,
    pub status: TaskStatus,
    /// Its workspace's folder when it was made; None: a scratch folder of its own (General).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The cat doing it (a [`Cat`]'s id). Set when the task is made; changed only while no run has started.
    pub cat: String,
    /// The round that made it and the time it was due; None for a task the person made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<TaskRound>,
    /// When the person last opened it (version 12). Activity after this is unread.
    #[serde(default)]
    pub last_read_ms: i64,
    /// When something last happened in it that the person did not do themselves: the agent's words, a run's end, a request
    /// (version 12). Never earlier than `created_ms` for a task the store holds.
    #[serde(default)]
    pub activity_ms: i64,
    /// The last line of the newest agent words, cut to [`LATEST_LINE_CHARS`] (version 12). Empty before the agent has said anything.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub latest_line: String,
    /// Why the last run failed, in a line, while the task is `Failed` (version 12); else empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub failure_line: String,
    /// What the thread is doing, derived by the daemon (version 12): `neko_core::thread::thread_status`.
    #[serde(default)]
    pub thread_status: ThreadStatus,
    /// Something waits for the person in it: a request, a question, a failure they can act on (version 12). A flag, apart from
    /// `thread_status`; the list is [`NeedsItem`]s.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub needs_you: bool,
    /// Set when this task is a subagent of another: whose it is and its number. A subagent is not listed as a thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<SubagentOf>,
}

/// The longest a thread's `latest_line` is, in characters.
pub const LATEST_LINE_CHARS: usize = 120;

/// What a thread is doing, for a sidebar row (docs/knowledge/threads.md). Needs you is a flag beside it, not a value of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStatus {
    /// A run is going.
    Working,
    /// The last run failed and the person has not opened the thread since.
    Failed,
    /// No run, and the thread waits on its subagents (docs/knowledge/subagents.md).
    Waiting,
    /// None of the above.
    #[default]
    Idle,
}

impl Task {
    /// A task as it is at birth, made by the person: new, in a scratch folder, read. Tests and the app's placeholder data start
    /// here; the daemon fills the rest from what is stored.
    pub fn blank(id: TaskId, workspace: &str, words: &str, title: &str, created_ms: i64, cat: &str) -> Task {
        Task {
            id,
            workspace: workspace.to_owned(),
            words: words.to_owned(),
            title: title.to_owned(),
            created_ms,
            status: TaskStatus::New,
            folder: None,
            cat: cat.to_owned(),
            round: None,
            last_read_ms: created_ms,
            activity_ms: created_ms,
            latest_line: String::new(),
            failure_line: String::new(),
            thread_status: ThreadStatus::Idle,
            needs_you: false,
            subagent: None,
        }
    }

    /// Something happened since the person last opened it.
    pub fn unread(&self) -> bool {
        self.activity_ms > self.last_read_ms
    }
}

/// What makes a task a subagent (docs/knowledge/subagents.md): numbered under the thread that started it, no name, no face.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentOf {
    /// The task (a thread or another subagent) that started it, and that its report wakes.
    pub parent: TaskId,
    /// `1`, `2`, or `1.2` for the second subagent of Subagent 1: the thread's own counter, a dot for each level down.
    pub number: String,
    /// The top-level thread it belongs to.
    pub root: TaskId,
}

impl SubagentOf {
    /// How deep it is: `1` is 1, `1.2` is 2.
    pub fn depth(&self) -> usize {
        self.number.split('.').count()
    }
}

impl Task {
    /// Whether it is a thread of its own (the sidebar lists these), not a subagent.
    pub fn is_thread(&self) -> bool {
        self.subagent.is_none()
    }

    /// "Subagent 1.2" for a subagent.
    pub fn subagent_name(&self) -> Option<String> {
        self.subagent.as_ref().map(|s| format!("Subagent {}", s.number))
    }
}

/// Where a run stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Done,
    Failed,
    Stopped,
    Interrupted,
}

/// How the cost of a run is to be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Pricing {
    /// The CLI's login pays per call (an API key): the cost is money charged.
    Charged,
    /// The CLI's login is a subscription: the cost is what the same calls would cost on the API, not money charged.
    ApiEquivalent,
    /// The login kind is not known.
    #[default]
    Unknown,
}

/// What the agent CLI reported a run used. Never estimated: a figure the CLI did not report is absent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Input tokens read from the prompt cache.
    pub cached_tokens: u64,
    /// Dollars, as the CLI reported them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub pricing: Pricing,
    /// Who reported it ("Claude Code").
    pub reported_by: String,
    /// How many tokens the conversation's context held at the run's last model call, as reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
    /// The model's context window, when the CLI reported it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
}

/// What a task's branch changed, from git.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Changes {
    pub files: u32,
    pub added: u32,
    pub removed: u32,
}

/// One start of an agent on a task, until it ends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub task: TaskId,
    /// The runtime's id ("claude").
    pub runtime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub status: RunStatus,
    pub started_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<i64>,
    /// The CLI's own conversation id; a follow-up resumes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The folder it worked in.
    pub workdir: String,
    /// Its git branch, when it works in a worktree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Why it failed or stopped, in words a person can act on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<Changes>,
    /// Where its undo stands: its checkpoints, what it changed itself, undone or not (docs/knowledge/undo.md). None for a run that
    /// never started its agent, and for runs from before protocol 7.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<RunUndo>,
    /// The cat whose rules went with this run (None for runs from before protocol 8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cat: Option<String>,
    /// The version of the cat's rules it used ([`cats::rules_hash`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_hash: Option<String>,
}

/// A tool call's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolState {
    Running,
    Done,
    /// It could not run: an error, cut off, timed out.
    Failed,
    /// The person (or Neko on their behalf, when no one answered) said no: their choice, not a failure.
    Denied,
}

/// How a notice in the transcript reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Info,
    Warning,
    Error,
}

/// What a part of the transcript holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PartBody {
    /// The person's words.
    Person { text: String },
    /// The agent's words (Markdown); `done` once the block is whole.
    Agent { text: String, done: bool },
    Thinking { text: String, done: bool },
    /// One tool call. `summary` is a few words after the name; `input` and `output` are shortened for the fold. `exit` is a
    /// command's exit code when it ran and ended with one other than zero: the step still finished (`done`), it is not a failure.
    Tool {
        call: String,
        name: String,
        summary: String,
        input: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<String>,
        state: ToolState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit: Option<i32>,
    },
    /// A request for permission, by its id.
    Permission { permission: PermissionId },
    /// Something Neko says: a failure, an interruption. `next` is what to do about it. `failure` says which failure the person can
    /// act on it was (signing in, the usage limit, a missing folder or CLI); None for anything else.
    Notice {
        tone: Tone,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failure: Option<FailureKind>,
    },
    /// The cat's standing rules changed since the task's last run: this run is the first with the new ones. `name` is the cat's
    /// name then.
    RulesChanged { cat: String, name: String },
    /// A subagent was started here: the card in the conversation. Its state is the task's own (`subagent`).
    Subagent { subagent: TaskId, number: String, title: String },
    /// The thread was woken by a subagent's report: the centred note, and what the report carried. `status` is `done`, `failed`,
    /// `stopped`, `interrupted` or `asked`.
    Wake {
        number: String,
        title: String,
        status: String,
        /// Its final words, shortened to 16 KB.
        summary: String,
        /// Files it changed, with counts ("src/auth.rs +12 -3").
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
}

/// A failure the person can do something about (docs/knowledge/needs-you.md). Every other failure is a notice only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// The agent CLI is not installed.
    NotInstalled,
    NotSignedIn,
    UsageLimit,
    /// The folder the task works in is gone.
    FolderMissing,
}

/// What a "Needs you" item waits for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Need {
    /// A pending permission request, whole.
    Permission { permission: Permission },
    /// The run ended with a question: its last words, shortened to a line or two. A follow-up that names this run answers it.
    Question { run: RunId, question: String },
    /// The run failed for a reason the person can act on: the notice's words, what to do next, and the one line to copy (a
    /// sign-in or install command) when there is one.
    Failure {
        failure: FailureKind,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
    /// A plugin waits for the person to approve exactly what it will run (or, when it changed, to approve it again). It belongs to no
    /// task: the item's `task`, `workspace`, `cat` and `cat_name` are empty and its `title` is the plugin's name.
    Plugin { ask: PluginAsk },
}

/// One thing waiting for the person, derived by the daemon from what it stores (permissions, runs, notices). The list is oldest
/// first, grouped by workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsItem {
    /// Stable while it waits: `perm:<id>`, `question:<run id>`, `failure:<part id>`, `plugin:<id>`.
    pub id: String,
    pub task: TaskId,
    pub workspace: String,
    /// The task's title.
    pub title: String,
    /// The task's cat, and its name ("miso asks…").
    pub cat: String,
    pub cat_name: String,
    /// Since when it has waited (ms since the epoch).
    pub since_ms: i64,
    pub need: Need,
    /// The subagent's number when the item is its own ("1.2"): then `title` is the thread's, and the item reads "Subagent 1.2 asks".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<String>,
}

/// One piece of a task's transcript, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub id: PartId,
    pub task: TaskId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    /// Its place in the task's transcript.
    pub seq: u64,
    pub at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<i64>,
    pub body: PartBody,
}

/// Where a permission request stands. Pending is the only state that can be answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Pending,
    Allowed,
    /// Allowed, and the same kind of step is allowed for the rest of this task.
    AlwaysAllowed,
    Denied,
    /// The run ended before anyone answered.
    Expired,
}

/// A person's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    AllowOnce,
    AlwaysAllow,
    Deny,
}

/// The agent asking to do something risky.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permission {
    pub id: PermissionId,
    pub task: TaskId,
    pub run: RunId,
    /// The tool's own name ("Bash").
    pub tool: String,
    /// What it wants to do, in words ("Run a command").
    pub title: String,
    /// Why Neko asks.
    pub why: String,
    /// The exact command, path or address.
    pub detail: String,
    /// Deletes or overwrites: no default answer, no "always".
    pub destructive: bool,
    pub state: PermissionState,
    pub asked_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_ms: Option<i64>,
}

/// How a new task's agent may act (docs/knowledge/permissions.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Claude Code's own rules, as in the terminal: what the person's settings allow runs without asking; what Claude Code would
    /// ask about is asked in the task. Neko adds nothing. The default, and what a store with no mode stored reads as.
    #[default]
    Follow,
    /// Enforced by Neko whatever the person's settings say: reading and editing inside the task's folder go ahead; every command,
    /// other folders, the network and anything destructive ask.
    Ask,
    /// Everything goes ahead.
    Full,
}

/// The daemon's settings a screen may change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// The runtime new tasks use ("claude").
    pub runtime: String,
    pub permission_mode: PermissionMode,
    /// Also give runs the MCP servers in the person's own Claude Code settings. Off by default: a run gets only Neko's own tool and
    /// the plugins the person approved (docs/knowledge/subagents.md).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub own_mcp: bool,
}

/// An agent CLI Neko knows, as found on this Mac.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInfo {
    /// "claude", "codex".
    pub id: String,
    /// "Claude Code".
    pub name: String,
    pub found: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// None when the CLI cannot say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_in: Option<bool>,
    /// Which login, in words ("Claude subscription (Max)", "API key").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    /// The one line that signs in.
    pub sign_in: String,
    /// The one line that installs it.
    pub install: String,
    /// Whether this Neko can run it yet.
    pub supported: bool,
    /// What the CLI's own settings let it do without asking, in one line ("Your Claude Code settings allow Bash, Edit and Write
    /// without asking."). Read from its settings files, never written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<String>,
}

/// What a day's runs used, as reported.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct UsageTotal {
    pub runs: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// The sum of the costs reported; None when no run reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Some of the cost is an API-equivalent price, not money charged.
    pub api_equivalent: bool,
}

/// A task with everything said and done in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub task: Task,
    pub runs: Vec<Run>,
    pub parts: Vec<Part>,
    pub permissions: Vec<Permission>,
}

/// What a screen asks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// The first request: who the screen is and which contract it speaks.
    Hello { client: String, protocol: u32 },
    /// Every task of every workspace, newest first.
    ListTasks,
    /// A new task from the person's words, in `workspace` (None: General), for `cat` (None: the workspace's default cat, else the
    /// least busy). Its agent starts by itself.
    CreateTask {
        words: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cat: Option<String>,
    },
    /// Give a task to another cat. Refused once a run has started.
    SetTaskCat { task: TaskId, cat: String },
    /// Every cat: the active ones first (oldest first), then the retired.
    ListCats,
    /// A new cat. No face: the first of [`cats::FACES`] no active cat has.
    CreateCat {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        face: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        purpose: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
    },
    RenameCat { cat: String, name: String },
    SetCatFace { cat: String, face: String },
    /// An empty purpose clears it.
    SetCatPurpose { cat: String, purpose: String },
    /// None: no default workspace.
    SetCatWorkspace {
        cat: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
    },
    /// Save the cat's standing rules; the ones before become its previous rules.
    SaveCatRules { cat: String, rules: String },
    /// Put the previous rules back (the current ones become the previous).
    RestoreCatRules { cat: String },
    /// Retire a cat: it takes no new tasks; its tasks stay readable. Refused for the last active cat.
    RetireCat { cat: String },
    /// Bring a retired cat back. Refused when [`cats::MAX_ACTIVE`] are active.
    BringBackCat { cat: String },
    /// Every round, in the order they were made, and whether every round is paused.
    ListRounds,
    /// A new round. `workspace` None is General. It is on unless `on` is false.
    CreateRound {
        name: String,
        words: String,
        cat: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
        schedule: Schedule,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        on: bool,
    },
    /// Change a round whole (the screen sends every field). A new schedule counts from now.
    UpdateRound {
        round: String,
        name: String,
        words: String,
        cat: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
        schedule: Schedule,
    },
    /// Turn a round on or off. Turning one on that the brake turned off clears the brake and counts from now.
    SetRoundOn { round: String, on: bool },
    /// Remove a round. Its past tasks stay.
    RemoveRound { round: String },
    /// Run a round now, whatever its schedule says (also while it is off or every round is paused). Refused while its last task is
    /// still working or waiting for the person.
    RunRoundNow { round: String },
    /// Pause every round at once, or let them go on. Kept across restarts.
    SetRoundsPaused { paused: bool },
    /// A round's last runs, newest first, at most [`rounds::HISTORY_KEPT`].
    RoundHistory { round: String },
    /// Debug builds only, and only with `NEKO_DEBUG_CLOCK=1` in the daemon's environment: moves the rounds scheduler's clock forward
    /// by `offset_ms` from the real one (None: back to real time). Any other daemon refuses it. For the QA rig.
    DebugClock {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offset_ms: Option<i64>,
    },
    /// What happened in a window: from the person's last "mark read" (the first time, the last 24 hours) up to now. A window may be
    /// given (`from_ms` inclusive, `to_ms` exclusive; either may be left out), for tests and tools.
    GetBriefing {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_ms: Option<i64>,
    },
    /// The person has read the briefing: its next window starts now. Answers with the new briefing.
    MarkBriefingRead,
    /// The morning time and whether the notification is posted.
    GetBriefingSettings,
    SetBriefingSettings { settings: BriefingSettings },
    /// Every plugin, in the order they were added.
    ListPlugins,
    /// Add what people already have for other tools: one MCP server's JSON, or a `{"mcpServers": {…}}` block. Each server becomes a
    /// plugin that waits for approval; nothing runs. `name` names a single server that has none. Secret values in the paste go to the
    /// Keychain. Answers with the plugins added.
    AddMcpJson {
        json: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Add a folder with a `neko-plugin.json`. The plugin waits for approval; nothing runs.
    AddPluginFolder { folder: String },
    /// Change a pasted plugin's manifest. Its approval then no longer matches: it is `changed` until approved again. A plugin from
    /// a folder is changed by editing the folder's file.
    UpdatePluginManifest { plugin: String, manifest: Manifest },
    /// Approve exactly the manifest with this digest. Refused when the plugin's manifest is not that one any more.
    ApprovePlugin { plugin: String, digest: String },
    /// Say no to the manifest the plugin has now: it does not run and does not ask again until the manifest changes.
    DeclinePlugin { plugin: String },
    SetPluginEnabled { plugin: String, on: bool },
    /// Set (or, with no value, clear) one declared setting. A secret goes to the Keychain and is never sent back.
    SetPluginSetting {
        plugin: String,
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    /// Which cats may use the plugin.
    SetPluginAccess { plugin: String, access: Access },
    /// Start the plugin's server, say hello as MCP does and list its tools, within [`plugins::CHECK_SECONDS`]. The answer comes when
    /// the check ends (the daemon is never held up by it); the result is also the plugin's last check. Only an approved plugin is run.
    TestPlugin { plugin: String },
    /// Remove a plugin and its Keychain items.
    RemovePlugin { plugin: String },
    /// Forget a task: its record goes, and its worktree or scratch folder when nothing unsaved is in it. Refused while it runs and
    /// when its folder holds unsaved work.
    RemoveTask { task: TaskId },
    /// Every workspace, General first, then in the person's order; removed ones last.
    ListWorkspaces,
    /// Add a folder as a workspace. A folder already added is answered with its workspace (restored if it was removed).
    AddWorkspace { folder: String },
    RenameWorkspace { workspace: String, name: String },
    /// Remove a workspace from the list. Nothing on disk is touched; its tasks stay readable.
    RemoveWorkspace { workspace: String },
    /// The workspace's folder moved: here it is now.
    LocateWorkspace { workspace: String, folder: String },
    /// Put a workspace at `to` among the workspaces the person added and has not removed (0 is first, after General).
    MoveWorkspace { workspace: String, to: u32 },
    /// One task. (`task`, not `id`: the message's own `id` is the request's.)
    GetTask { task: TaskId },
    /// Push an event for every change from now on.
    Subscribe,
    /// A task's runs, parts and permission requests.
    GetTranscript { task: TaskId },
    /// Words in the task's composer: they continue the same conversation. With `answering`, they answer the question that run
    /// ended with, and are refused (`already_answered`) unless the task still waits on it.
    FollowUp {
        task: TaskId,
        words: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answering: Option<RunId>,
    },
    /// Stop the task's running agent.
    StopTask { task: TaskId },
    /// The person opened the thread: it is read as of now. Replies `task`.
    MarkRead { task: TaskId },
    /// Make the thread unread again (the person asked). Replies `task`.
    MarkUnread { task: TaskId },
    /// Answer a permission request. A request already answered is refused.
    AnswerPermission { permission: PermissionId, decision: Decision },
    /// Send this task's text as it streams (or none). Other changes reach every subscriber.
    WatchTask {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<TaskId>,
    },
    /// The agent CLIs Neko knows, as found on this Mac, and the settings.
    ListRuntimes,
    SetSettings { settings: Settings },
    /// What runs started since `from_ms` used.
    DayUsage { from_ms: i64 },
    /// Everything waiting for the person now ("Needs you").
    ListNeeds,
    /// What a run changed, between its start and end checkpoints: the files, with their counts (no lines yet). No run: the whole
    /// task, from its first run's start to where its newest run left the folder ("All changes").
    GetDiff {
        task: TaskId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },
    /// One file's lines of that diff (paged per file). `full`: all of them, not cut at [`undo::CUT_LINES`].
    GetDiffFile {
        task: TaskId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        path: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        full: bool,
    },
    /// Put the task's files back as they were before `run`. Refused unless it is the run [`undo::offers`] offers Undo for. When
    /// files changed since the run ended the answer is `changed_since` with them, and nothing is touched; `anyway` then saves
    /// the folder as it is as a checkpoint of its own first, and undoes.
    UndoRun {
        task: TaskId,
        run: RunId,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        anyway: bool,
    },
    /// Put the undone run's changes back, with the same check.
    RedoRun {
        task: TaskId,
        run: RunId,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        anyway: bool,
    },
    /// How much room undo takes (Settings ▸ Workspaces).
    UndoStorage,
    /// Remove every checkpoint: the hidden refs in repositories and Neko's shadow repositories. No run can be undone after.
    ClearUndo,
}

/// A request and the id its reply will carry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientMessage {
    pub id: u64,
    #[serde(flatten)]
    pub request: Request,
}

/// Why a request failed, as a code a screen can act on and words a person can read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The screen speaks another version.
    ProtocolMismatch,
    /// Hello must come first.
    HelloFirst,
    /// The message was not one this version knows.
    BadRequest,
    /// Empty words, or too many.
    InvalidWords,
    NotFound,
    /// The permission request was already answered, or has expired.
    AlreadyAnswered,
    /// The task's agent is busy and cannot take this now.
    Busy,
    /// The store failed; the words say how.
    Internal,
    /// The folder is not there, not a folder, or cannot be read.
    InvalidFolder,
    /// Not allowed for this workspace or task (General cannot be removed; a removed workspace takes no new task); the words say
    /// why.
    Refused,
    /// The task's folder holds work that is not saved (uncommitted changes, or files in its scratch folder): nothing is deleted.
    UnsavedWork,
    /// A cat's name is empty, too long, or another cat's; the words say which.
    InvalidName,
    /// What was pasted or read is not a plugin Neko can keep; the words say why.
    InvalidPlugin,
}

/// The daemon's answer to one request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    Hello { daemon: String, protocol: u32, workspace: String },
    Tasks { tasks: Vec<Task> },
    Task { task: Task },
    /// Subscribed; `revision` is the last change the store holds.
    Subscribed { revision: u64 },
    Transcript { transcript: Transcript },
    Runtimes { runtimes: Vec<RuntimeInfo>, settings: Settings },
    Settings { settings: Settings },
    DayUsage { usage: UsageTotal },
    Workspaces { workspaces: Vec<Workspace> },
    /// One workspace; `existed` when the folder added was already one.
    Workspace {
        workspace: Workspace,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        existed: bool,
    },
    /// What waits for the person, oldest first, grouped by workspace.
    Needs { items: Vec<NeedsItem> },
    /// A run's (or a task's) changes, file by file.
    Diff { diff: DiffSummary },
    /// One file's lines.
    DiffFile { file: DiffPatch },
    /// Undo or redo did nothing: these files changed since (paths in the task's folder). Asking again with `anyway` goes ahead.
    ChangedSince { files: Vec<String> },
    UndoStorage { storage: UndoStorage },
    Cats { cats: Vec<Cat> },
    Cat { cat: Cat },
    Briefing { briefing: Briefing },
    BriefingSettings { settings: BriefingSettings },
    /// Every round, and whether all are paused.
    Rounds { rounds: Vec<Round>, paused: bool },
    Plugins { plugins: Vec<Plugin> },
    Plugin { plugin: Plugin },
    Round { round: Round },
    /// A round's history, newest first.
    RoundRuns { runs: Vec<RoundRun> },
    /// Done; the change arrives as events.
    Ok,
    Error { code: ErrorCode, message: String },
}

/// A change, pushed to subscribed screens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Change {
    TaskCreated { task: Task },
    TaskUpdated { task: Task },
    /// The task was removed (`remove_task`).
    TaskRemoved { task: TaskId },
    WorkspaceAdded { workspace: Workspace },
    /// Renamed, removed, restored, located or moved: the whole row as it is now.
    WorkspaceUpdated { workspace: Workspace },
    RunChanged { run: Run },
    /// A part was added or replaced whole.
    PartChanged { part: Part },
    /// Words appended to a part as they stream; sent only to screens watching the task, and not stored as an event (`revision`
    /// is the last stored change).
    PartDelta { task: TaskId, part: PartId, text: String },
    PermissionChanged { permission: Permission },
    SettingsChanged { settings: Settings },
    /// Whether the agent is thinking right now (no words arrive while it does); sent only to screens watching the task, never
    /// stored.
    Activity { task: TaskId, thinking: bool },
    /// What waits for the person changed: the whole list as it is now (it is derived, never stored; `revision` is the change that
    /// caused it).
    NeedsChanged { items: Vec<NeedsItem> },
    CatAdded { cat: Cat },
    /// Renamed, a new face, purpose, default workspace or rules, retired or brought back: the whole cat as it is now.
    CatUpdated { cat: Cat },
    RoundAdded { round: Round },
    /// Changed in any way (edited, switched, run, skipped, braked, a new next time): the whole round as it is now.
    RoundUpdated { round: Round },
    RoundRemoved { round: String },
    /// Every round was paused, or let go on. The rounds follow as `round_updated`.
    RoundsPaused { paused: bool },
    /// The briefing's content changed: the whole briefing for the current window (derived, never stored; sent only when it differs,
    /// apart from the moment it was made).
    BriefingChanged { briefing: Briefing },
    BriefingSettingsChanged { settings: BriefingSettings },
    /// It is the morning time and the window has news: the screen posts the notification. Sent once a day at most, and not stored
    /// (`revision` is the last stored change). A daemon with no screen connected keeps it until one is, for that day.
    BriefingDue { counts: BriefingCounts },
    PluginAdded { plugin: Plugin },
    /// Approved, declined, changed, switched, set up, checked, left to other cats: the whole plugin as it is now.
    PluginUpdated { plugin: Plugin },
    PluginRemoved { plugin: String },
}

/// What the daemon sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMessage {
    Reply { id: u64, reply: Reply },
    /// `revision` is the store's event sequence number of this change.
    Event { revision: u64, change: Change },
}

/// The daemon's address, written to `daemon.json` in the data folder (mode 0600) while it runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    /// Always a loopback port: the daemon binds 127.0.0.1 only.
    pub port: u16,
    pub protocol: u32,
    pub version: String,
}

/// The file names in the data folder.
pub mod files {
    /// The daemon's address while it runs.
    pub const DAEMON: &str = "daemon.json";
    /// The per-install token (mode 0600). Every request carries it.
    pub const TOKEN: &str = "token";
    /// Held (an exclusive lock) by the one daemon of this folder.
    pub const LOCK: &str = "daemon.lock";
    /// The SQLite store.
    pub const STORE: &str = "neko.sqlite3";
    /// Scratch folders of tasks with no folder chosen, one per task.
    pub const SCRATCH: &str = "scratch";
    /// Git worktrees of tasks in a repository, one per task.
    pub const WORKTREES: &str = "worktrees";
    /// Undo's own folder: a shadow repository per task whose folder is not a git repository, and temporary index files.
    pub const UNDO: &str = "undo";
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

impl ClientMessage {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

impl ServerMessage {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

impl Reply {
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Reply {
        Reply::Error { code, message: message.into() }
    }
}

impl TaskStatus {
    /// Whether an agent is at work on it now.
    pub fn is_active(self) -> bool {
        matches!(self, TaskStatus::Working | TaskStatus::NeedsYou)
    }
}

impl PartBody {
    /// The words a person would copy from it.
    pub fn text(&self) -> &str {
        match self {
            PartBody::Person { text } | PartBody::Agent { text, .. } | PartBody::Thinking { text, .. } | PartBody::Notice { text, .. } => text,
            PartBody::Tool { summary, .. } => summary,
            PartBody::Wake { summary, .. } => summary,
            PartBody::Subagent { title, .. } => title,
            PartBody::Permission { .. } | PartBody::RulesChanged { .. } => "",
        }
    }
}

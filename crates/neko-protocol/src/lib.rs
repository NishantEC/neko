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

/// The version of this contract. A screen and a daemon talk only when theirs are equal. 2: tasks run (S1).
pub const PROTOCOL_VERSION: u32 = 2;

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
}

/// A task as a screen sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub workspace: String,
    /// The person's words, exactly as sent.
    pub words: String,
    /// A short title made from the words (their first line, cut at a word near 80 characters).
    pub title: String,
    /// Milliseconds since the Unix epoch.
    pub created_ms: i64,
    pub status: TaskStatus,
    /// The folder the person chose for it; None: a scratch folder of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
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
}

/// A tool call's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolState {
    Running,
    Done,
    Failed,
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
    /// One tool call. `summary` is a few words after the name; `input` and `output` are shortened for the fold.
    Tool { call: String, name: String, summary: String, input: String, #[serde(default, skip_serializing_if = "Option::is_none")] output: Option<String>, state: ToolState },
    /// A request for permission, by its id.
    Permission { permission: PermissionId },
    /// Something Neko says: a failure, an interruption. `next` is what to do about it.
    Notice { tone: Tone, text: String, #[serde(default, skip_serializing_if = "Option::is_none")] next: Option<String> },
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

/// How a new task's agent may act.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Reading and editing inside the task's folder go ahead; commands, other folders, the network and anything destructive ask.
    #[default]
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
    /// Every task of the default workspace, newest first.
    ListTasks,
    /// A new task from the person's words, in `folder` (None: a scratch folder). Its agent starts by itself.
    CreateTask {
        words: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        folder: Option<String>,
    },
    /// One task. (`task`, not `id`: the message's own `id` is the request's.)
    GetTask { task: TaskId },
    /// Push an event for every change from now on.
    Subscribe,
    /// A task's runs, parts and permission requests.
    GetTranscript { task: TaskId },
    /// Words in the task's composer: they continue the same conversation.
    FollowUp { task: TaskId, words: String },
    /// Stop the task's running agent.
    StopTask { task: TaskId },
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
    RunChanged { run: Run },
    /// A part was added or replaced whole.
    PartChanged { part: Part },
    /// Words appended to a part as they stream; sent only to screens watching the task, and not stored as an event (`revision`
    /// is the last stored change).
    PartDelta { task: TaskId, part: PartId, text: String },
    PermissionChanged { permission: Permission },
    SettingsChanged { settings: Settings },
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
            PartBody::Permission { .. } => "",
        }
    }
}

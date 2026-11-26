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

/// The version of this contract. A screen and a daemon talk only when theirs are equal.
pub const PROTOCOL_VERSION: u32 = 1;

/// The WebSocket path, versioned with the contract.
pub const WS_PATH: &str = "/v1/ws";
/// The plain HTTP health and version check (same token).
pub const HELLO_PATH: &str = "/v1/hello";

/// The longest task a person may type, in bytes (a pasted page fits; a pasted log is refused with a reason).
pub const MAX_WORDS_BYTES: usize = 100_000;

/// A task's id, as the daemon made it (`task_` then 26 letters and digits).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub String);

/// Where a task stands. S0 has no agent, so every task is new: no cat is working on it yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    New,
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
}

/// What a screen asks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// The first request: who the screen is and which contract it speaks.
    Hello { client: String, protocol: u32 },
    /// Every task of the default workspace, newest first.
    ListTasks,
    /// A new task from the person's words.
    CreateTask { words: String },
    /// One task. (`task`, not `id`: the message's own `id` is the request's.)
    GetTask { task: TaskId },
    /// Push an event for every change from now on.
    Subscribe,
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
    /// The store failed; the words say how.
    Internal,
}

/// The daemon's answer to one request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    Hello { daemon: String, protocol: u32, workspace: String },
    Tasks { tasks: Vec<Task> },
    Task { task: Task },
    /// Subscribed; `revision` is the last change the store holds.
    Subscribed { revision: u64 },
    Error { code: ErrorCode, message: String },
}

/// A change, pushed to subscribed screens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Change {
    TaskCreated { task: Task },
}

/// What the daemon sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

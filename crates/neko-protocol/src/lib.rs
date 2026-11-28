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
pub mod search;
pub mod title;
pub mod undo;

pub use briefing::{Briefing, BriefingCounts, BriefingSettings};
pub use cats::{Cat, CattainFile};
pub use plugins::{Access, AccessMode, Approval, Check, FieldKind, FieldValue, Manifest, Plugin, PluginAsk, PluginKind, PluginState, SettingField};
pub use rounds::{Round, RoundRun, RoundStatus, TaskRound};
pub use schedule::Schedule;
pub use search::{Found, FoundBy};
pub use undo::{DiffFile, DiffPatch, DiffSource, DiffSummary, FileStatus, Offer, Overview, RunUndo, UndoState, UndoStorage};

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
/// in the transcript; a request from a subagent says who asked; a setting that gives runs the person's own MCP servers). 14: a new thread needs no cat (`create_task` takes `in_folder` and `from_branch`
/// instead, a task's `cat` is optional, `set_task_cat` is gone, `list_branches` says where a thread can start from).
/// 15: the right pane (a diff has a source: a run, the thread, or what is uncommitted in the folder now; a thread's overview in one reply).
/// 16: steer, queue, interrupt (`follow_up` takes a `mode`; the messages waiting above a composer are a list the daemon keeps, in the transcript and in `queue_changed`; `send_queued_now`, `discard_queued`; a setting swaps Return and Option-Return).
/// 17: a thread can be pinned, settled or archived (`pin_task`, `settle_task`, `archive_task`; a task says which it is).
/// 18: search (`search` finds threads and kittens by title and by what was said in them; filters in the query).
/// 19: kittens get a place, an order and a draft (a subagent says whether it works in the shared folder or in a copy of its own, which
/// branch, which kitten it follows, whether it is a draft or was discarded; `start_draft`, `edit_draft`, `discard_draft`; a setting asks
/// before a thread's first kitten).
/// 20: questions go up (a run may carry the question it asked with Neko's tools, to its parent or to the person; a kitten says the
/// question it waits on and where it stands; a wake can be a question; a note says the agent answered or wrote to a kitten; a "Needs
/// you" question says which kitten it came from and through whom).
/// 21: a cat is a Cattain (its own thread, project, model and access; its four files read and saved with `get_cattain_file`,
/// `save_cattain_file`, `restore_cattain_file` and pushed in `cattain_file_changed`; `create_task` may start a thread in a Cattain's crew;
/// a task says whether it is a Cattain's own thread; the rules requests are gone, the rules being RULES.md).
pub const PROTOCOL_VERSION: u32 = 21;

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
id_type!(
    /// A message waiting above a thread's composer (`queued_` …).
    QueuedId
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
    /// The cat whose rules go with it (a [`Cat`]'s id), when it has one (version 14). A thread the person starts has none; a task made
    /// before version 14, or by a round, keeps the cat it was given, and that never changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cat: Option<String>,
    /// The person chose to work in the project's own folder, not in a worktree of its own (version 14).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_folder: bool,
    /// The branch its worktree started from, when the person chose one (version 14); else it began on the commit the checkout is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_branch: Option<String>,
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
    /// The person pinned it (version 16): it sits at the top of Threads, in any project. A pinned thread is never settled by itself.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    /// It is put away in its group's Settled fold (version 16), by the person or after a week idle. New activity brings it back.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub settled: bool,
    /// It sleeps (version 16): out of the lists until the person's next message in it, which wakes it. Never both pinned and settled.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
    /// It is a Cattain's own thread (version 19): its `cat`'s conversation with the person, which runs in the Cattain's folder. It is
    /// not listed under Threads; the Cattain is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cattain: bool,
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
    pub fn blank(id: TaskId, workspace: &str, words: &str, title: &str, created_ms: i64, cat: Option<&str>) -> Task {
        Task {
            id,
            workspace: workspace.to_owned(),
            words: words.to_owned(),
            title: title.to_owned(),
            created_ms,
            status: TaskStatus::New,
            folder: None,
            cat: cat.map(str::to_owned),
            in_folder: false,
            from_branch: None,
            round: None,
            last_read_ms: created_ms,
            activity_ms: created_ms,
            latest_line: String::new(),
            failure_line: String::new(),
            thread_status: ThreadStatus::Idle,
            needs_you: false,
            subagent: None,
            pinned: false,
            settled: false,
            archived: false,
            cattain: false,
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
    /// `1`, `2`, or `1.2` for the second subagent of Kitten 1: the thread's own counter, a dot for each level down.
    pub number: String,
    /// The top-level thread it belongs to.
    pub root: TaskId,
    /// Where it works (version 19): in its parent's folder, or in a copy of its own (a git worktree on its own branch).
    #[serde(default, skip_serializing_if = "SubPlace::is_shared")]
    pub placement: SubPlace,
    /// The sibling it starts after, from its branch (version 19): `1` for "After Kitten 1". It stays queued until that one is done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Written but not started (version 19): it waits for the person's Start (or the agent's `start_drafts`). A draft is not waited on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub draft: bool,
    /// A draft the person threw away (version 19). It never ran; its card says so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub discarded: bool,
    /// The branch its own copy works on (`neko/4f2a-1`), known from the start so a card can say it before the copy exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The question it waits on, while it has ended with one (version 20): derived by the daemon from its newest run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<KittenQuestion>,
}

/// A kitten's question to the agent that started it (docs/knowledge/subagents.md, "Questions go up").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KittenQuestion {
    pub text: String,
    /// The kittens below it that the question came up from, the one that first asked first ("1.2.1", "1.2"); empty when it is its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<String>,
    /// Where it stands now.
    pub with: QuestionWith,
}

/// Who has a kitten's question now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionWith {
    /// The agent that started it, to answer or pass up.
    #[default]
    Parent,
    /// Its parent passed it up to the agent above (a kitten's own parent is a kitten).
    Up,
    /// It reached the person, in Needs you.
    Person,
}

/// Whom a run's question is for (version 20).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskTo {
    /// A kitten asked the agent that started it (`ask_parent`).
    Parent,
    /// A thread's agent asked the person (`ask_person`).
    Person,
}

/// A question a run put with Neko's own tools (version 20). The run ends as `asked` whatever its last words are.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunAsk {
    pub to: AskTo,
    pub text: String,
    /// The kittens it came up from, the one that first asked first ("1.2", "1"); empty when it is the asker's own question.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<String>,
    /// Neko passed it up because the agent that read it ended its turn without answering it or passing it on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unanswered: bool,
}

/// Where a subagent works (docs/knowledge/subagents.md, "Where a kitten works").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubPlace {
    /// In its parent's folder, on its parent's branch: for reading, or for work that touches different files.
    #[default]
    Shared,
    /// In a git worktree of its own, on its own branch, started from the parent's folder as it was (uncommitted changes included).
    OwnCopy,
}

impl SubPlace {
    pub fn is_shared(&self) -> bool {
        *self == SubPlace::Shared
    }
}

/// How many subagents of one thread run at once; more wait their turn (docs/knowledge/subagents.md).
pub const SUBAGENTS_PER_THREAD: usize = 3;
/// How many runs go at once across Neko that Neko starts by itself; more wait.
pub const RUNS_ACROSS_NEKO: usize = 8;
/// How many levels subagents nest.
pub const SUBAGENT_DEPTH: usize = 3;

impl SubagentOf {
    /// A subagent in its parent's folder, not a draft, after no one.
    pub fn new(parent: TaskId, number: impl Into<String>, root: TaskId) -> SubagentOf {
        SubagentOf { parent, number: number.into(), root, placement: SubPlace::Shared, after: None, draft: false, discarded: false, branch: None, question: None }
    }

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

    /// A subagent that has not ended and is not a draft: its thread waits on it (a draft waits on the person).
    pub fn is_live_subagent(&self) -> bool {
        self.subagent.as_ref().is_some_and(|s| !s.draft) && matches!(self.status, TaskStatus::New | TaskStatus::Working | TaskStatus::NeedsYou | TaskStatus::Waiting)
    }

    /// Whether Threads lists it: a thread that is not a Cattain's own (a Cattain is listed by itself, version 19).
    pub fn listed(&self) -> bool {
        self.is_thread() && !self.cattain
    }

    /// The Cattain whose crew it is in: a thread that has a cat and is not that cat's own thread (version 19). A round's thread and a
    /// thread made before version 14 are in their cat's crew.
    pub fn crew_of(&self) -> Option<&str> {
        if self.cattain || self.subagent.is_some() { None } else { self.cat.as_deref() }
    }

    /// "Kitten 1.2" for a subagent.
    pub fn subagent_name(&self) -> Option<String> {
        self.subagent.as_ref().map(|s| format!("Kitten {}", s.number))
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
    /// The question it asked with Neko's tools, if it did (version 20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<RunAsk>,
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
    /// The words its cat hands the run changed since the task's last run: this run is the first with the new ones. `name` is the cat's
    /// name then; `file` (version 19) is the file they came from, RULES.md in a Cattain's own thread and CREW.md in its crew (absent
    /// before: the rules).
    RulesChanged {
        cat: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<String>,
    },
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
        /// Its question, when it ended `asked` (version 20).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        question: Option<String>,
    },
    /// The agent wrote to one of its kittens with `message_subagent` (version 20): the quiet note in its own conversation. `answer`: the
    /// kitten was waiting on a question, so these words are the answer.
    ToSubagent { number: String, text: String, answer: bool },
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
    /// The run ended with a question: its last words, shortened to a line or two (or what the agent asked with `ask_person`). A
    /// follow-up that names this run answers it. `kitten`: the kitten whose question the agent passed up, `via` the kittens it came
    /// through on the way (version 20); `unanswered`: Neko passed it up because the agent did not answer it.
    Question {
        run: RunId,
        question: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kitten: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        via: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        unanswered: bool,
    },
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
    /// The task's cat and its name; both empty for a thread that has none (version 14: the app does not say them).
    #[serde(default)]
    pub cat: String,
    #[serde(default)]
    pub cat_name: String,
    /// Since when it has waited (ms since the epoch).
    pub since_ms: i64,
    pub need: Need,
    /// The subagent's number when the item is its own ("1.2"): then `title` is the thread's, and the item reads "Kitten 1.2 asks".
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
    /// While the agent works, Return queues the words and ⌥Return steers them (version 16). Off: Return steers, ⌥Return queues. The
    /// screen reads it to choose the mode it sends; the daemon never reads it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub queue_first: bool,
    /// Ask before a thread's first kitten (version 19): it is made a draft the person Starts. Off: kittens start by themselves.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ask_first_subagent: bool,
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
    /// The messages waiting above the composer now, oldest first (version 16).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued: Vec<Queued>,
}

/// How words typed while the agent works reach it (version 16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendMode {
    /// Into the running agent's open input: it reads them at its next step. Today's behaviour, and the default.
    #[default]
    Steer,
    /// The daemon keeps them and sends them as the next run when this one ends.
    Queue,
    /// The turn is stopped and the words start a new turn at once.
    Interrupt,
}

/// What a message above the composer is doing (version 16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuedKind {
    /// Held by the daemon; the next run starts with it when this one ends well.
    Queued,
    /// Sent into the running agent, which has not read it yet. It cannot be taken back.
    Steered,
    /// The turn is being stopped; a new one starts with these words.
    Interrupting,
}

/// A message waiting above a thread's composer, until it is read, sent or discarded (version 16).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queued {
    pub id: QueuedId,
    pub task: TaskId,
    pub words: String,
    pub kind: QueuedKind,
    /// Milliseconds since the Unix epoch.
    pub at_ms: i64,
}

/// What a screen asks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// The first request: who the screen is and which contract it speaks.
    Hello { client: String, protocol: u32 },
    /// Every task of every workspace, newest first.
    ListTasks,
    /// A new task from the person's words, in `workspace` (None: General). It has no cat (version 14). In a git project it works in a
    /// worktree of its own, from `from_branch` when given (else from the commit the checkout is on); `in_folder` works in the
    /// project's folder itself instead. Its agent starts by itself.
    CreateTask {
        words: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        in_folder: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_branch: Option<String>,
        /// A Cattain's id (version 19): the thread is in its crew, and reads its CREW.md on every run. Refused for a retired one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crew: Option<String>,
    },
    /// The branches a new thread in this project can start from (version 14): whether its folder is a git repository, the branch the
    /// checkout is on, and the local branches.
    ListBranches { workspace: String },
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
    /// The model a Cattain's own thread asks for (version 19). None: the CLI's default.
    SetCatModel {
        cat: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// The access a Cattain's own thread runs with (version 19). None: the mode in Settings.
    SetCatAccess {
        cat: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        access: Option<PermissionMode>,
    },
    /// A Cattain's own thread (version 19), made the first time it is asked for: answers `task`. Every screen hears `task_created` and
    /// `cat_updated` when it was made now.
    OpenCattain { cat: String },
    /// One of a Cattain's files as it is on disk now ([`cats::FILES`]; version 19).
    GetCattainFile { cat: String, name: String },
    /// Saves RULES.md or CREW.md exactly as given (at most [`cats::MAX_RULES`] bytes); what was there becomes the previous version.
    /// NOTES.md and DIGEST.md are the Cattain's and are refused.
    SaveCattainFile { cat: String, name: String, text: String },
    /// Puts the previous version of RULES.md or CREW.md back; the current one becomes the previous.
    RestoreCattainFile { cat: String, name: String },
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
        /// How the words reach an agent that is working (version 16); none is `steer`. With no run going every mode starts one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<SendMode>,
    },
    /// Send a waiting message now: into the running agent, or as a new run when none is going (version 16). Replies `ok`, or
    /// `not_found` when it was already sent or discarded.
    SendQueuedNow { task: TaskId, queued: QueuedId },
    /// Take a waiting message back (Discard, or Edit, which puts its words in the composer) (version 16). Replies `ok`, or `not_found`
    /// when it was already sent. A steered message cannot be taken back: `refused`.
    DiscardQueued { task: TaskId, queued: QueuedId },
    /// Stop the task's running agent.
    StopTask { task: TaskId },
    /// The person opened the thread: it is read as of now. Replies `task`.
    MarkRead { task: TaskId },
    /// Make the thread unread again (the person asked). Replies `task`.
    MarkUnread { task: TaskId },
    /// Pin the thread to the top of the list, or unpin it (version 16). Pinning brings a settled thread back; a thread that is not a
    /// thread (a subagent) is refused. Replies `task`.
    PinTask { task: TaskId, pinned: bool },
    /// Put the thread in its group's Settled fold, or bring it back (version 16). Settling unpins; a thread that is working is
    /// refused (`refused`). Replies `task`.
    SettleTask { task: TaskId, settled: bool },
    /// Start a draft kitten (version 19): it runs when there is room. Replies `task`; a kitten that is not a draft is refused.
    StartDraft { task: TaskId },
    /// Change a draft kitten's prompt (version 19). Replies `task`; a kitten that is not a draft is refused.
    EditDraft { task: TaskId, prompt: String },
    /// Throw a draft kitten away (version 19): it never runs and its card says so. Replies `task`.
    DiscardDraft { task: TaskId },
    /// Archive the thread, or restore it without a run (version 16). A thread that is working or waits on its subagents is refused. A
    /// message to an archived thread wakes it. Replies `task`.
    ArchiveTask { task: TaskId, archived: bool },
    /// Find threads and kittens (version 18) by their titles and by what was said in them. `query` holds the words and any filters
    /// (`project:`, `status:`, `cat:`, `since:`: `docs/knowledge/search.md`); an empty query asks for the most recent threads. At most
    /// `limit` results (12 when none; never more than 50). Replies `found`.
    Search {
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
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
        /// Which changes: `run` (the default; with no `run` it means the thread, as it did before version 14), `thread`, or
        /// `uncommitted` (the folder now against its branch's newest commit).
        #[serde(default, skip_serializing_if = "undo::DiffSource::is_run")]
        source: undo::DiffSource,
    },
    /// One file's lines of that diff (paged per file). `full`: all of them, not cut at [`undo::CUT_LINES`].
    GetDiffFile {
        task: TaskId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        path: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        full: bool,
        #[serde(default, skip_serializing_if = "undo::DiffSource::is_run")]
        source: undo::DiffSource,
    },
    /// A thread's right pane in one reply: its folder and branch, what all its runs changed, and what is uncommitted now. The
    /// screen asks again when a run of the thread (or of one of its subagents) changes.
    GetOverview { task: TaskId },
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
    /// Where a new thread in a project can start from. `git` is false for a folder that is not a repository (or has no commit yet):
    /// then there is no branch to choose and no worktree. `current` is the branch the checkout is on (None: detached).
    Branches {
        workspace: String,
        git: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        branches: Vec<String>,
    },
    /// What a search found, best first (version 18). `problem` says in words why a filter could not be used (and then nothing was
    /// found). `took_ms` is how long the daemon took.
    Found {
        query: String,
        hits: Vec<Found>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        problem: Option<String>,
        #[serde(default)]
        took_ms: u32,
    },
    /// A run's (or a task's) changes, file by file.
    Diff { diff: DiffSummary },
    /// One file's lines.
    DiffFile { file: DiffPatch },
    /// A thread's overview (`get_overview`).
    Overview { overview: Overview },
    /// Undo or redo did nothing: these files changed since (paths in the task's folder). Asking again with `anyway` goes ahead.
    ChangedSince { files: Vec<String> },
    UndoStorage { storage: UndoStorage },
    Cats { cats: Vec<Cat> },
    Cat { cat: Cat },
    CattainFile { file: CattainFile },
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
    /// Renamed, a new face, purpose, project, model or access, retired or brought back: the whole cat as it is now.
    CatUpdated { cat: Cat },
    /// One of a Cattain's files changed (version 19): saved or restored by the person, or written by the Cattain (seen when one of its
    /// runs ends, and when the file is asked for).
    CattainFileChanged { file: CattainFile },
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
    /// The messages waiting above a thread's composer changed: the whole list as it is now, oldest first (version 16).
    QueueChanged { task: TaskId, queued: Vec<Queued> },
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
    /// Each Cattain's own folder, by its id: RULES.md, CREW.md, NOTES.md, DIGEST.md and reports/ (version 19). Its own thread runs
    /// here.
    pub const CATTAINS: &str = "cattains";
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
            PartBody::ToSubagent { text, .. } => text,
            PartBody::Permission { .. } | PartBody::RulesChanged { .. } => "",
        }
    }
}

//! The agent contract (docs/knowledge/runtimes.md): what every runtime Neko can start does, in Neko's own words.
//!
//! A runtime is an agent CLI already installed and signed in on this Mac (Claude Code today). It is found and read with
//! [`Runtime::detect`], which spends nothing. [`Runtime::start`] runs it on a task's words in a folder and hands back a [`Control`]
//! and a stream of [`AgentEvent`]s. The stream always ends with exactly one [`AgentEvent::Finished`], then closes. Through the
//! control the daemon sends a follow-up ([`Control::steer`]), answers a permission request ([`Control::answer`]) and stops the run
//! ([`Control::stop`]; the stream then ends within a second with [`Outcome::Stopped`]).
//!
//! The events are Neko's, not the CLI's: a runtime maps its CLI's protocol onto them, and nothing above this module knows which CLI
//! ran. Permission requests carry an [`Action`], so Neko's own policy (`policy.rs`) decides what goes ahead without asking.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use neko_protocol::{PermissionMode, Pricing};

/// What `detect` found.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Detection {
    pub found: bool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    /// None when the CLI cannot say without spending anything.
    pub signed_in: Option<bool>,
    pub login: Option<Login>,
}

/// Which login the CLI uses, as it says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Login {
    /// A subscription; the plan's name if the CLI gives one ("max", "team").
    Subscription(Option<String>),
    /// An API key: calls are charged.
    ApiKey,
    /// A cloud provider's account (Bedrock, Vertex): calls are charged there.
    Cloud(String),
    Other(String),
}

impl Login {
    /// How a reported cost is to be read under this login.
    pub fn pricing(&self) -> Pricing {
        match self {
            Login::Subscription(_) => Pricing::ApiEquivalent,
            Login::ApiKey | Login::Cloud(_) => Pricing::Charged,
            Login::Other(_) => Pricing::Unknown,
        }
    }

    /// In words, for Settings.
    pub fn words(&self, vendor: &str) -> String {
        match self {
            Login::Subscription(Some(plan)) => format!("{vendor} subscription ({})", capitalise(plan)),
            Login::Subscription(None) => format!("{vendor} subscription"),
            Login::ApiKey => "API key".to_owned(),
            Login::Cloud(c) => c.clone(),
            Login::Other(o) => o.clone(),
        }
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// What a run is asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartSpec {
    /// The person's words for this turn.
    pub words: String,
    /// The folder the agent works in (it exists).
    pub workdir: PathBuf,
    /// The CLI's conversation to continue.
    pub resume: Option<String>,
    pub mode: PermissionMode,
    /// In "Ask before risky steps": where the run's hook helper reaches the daemon. None in the other modes.
    pub hook: Option<HookLink>,
    /// Standing instructions for the agent on this run (the task's cat: `cats::block`), given the way the CLI takes them best. Text
    /// only: they change no permission, flag or hook.
    pub instructions: Option<String>,
    /// A file of MCP servers for this run alone (the plugins that go to the task's cat): handed on as `--mcp-config`. Written by the
    /// daemon with mode 0600 and removed when the run ends; the CLI keeps its own MCP servers too (docs/knowledge/plugins.md).
    pub mcp_config: Option<PathBuf>,
    /// How Neko's own MCP server (`neko-daemon --mcp`, named in `mcp_config`) reaches the daemon: set in the CLI's environment, so
    /// the server it starts inherits it. Never in argv or in the config file.
    pub mcp: Option<McpLink>,
    /// Also load the MCP servers in the person's own Claude Code settings. Off: `--strict-mcp-config`, so the run has only what
    /// `mcp_config` names.
    pub own_mcp: bool,
}

/// What a run's own MCP server needs to reach the daemon (docs/knowledge/subagents.md).
#[derive(Clone, PartialEq, Eq)]
pub struct McpLink {
    pub port: u16,
    pub secret: String,
}

impl std::fmt::Debug for McpLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "McpLink {{ port: {}, secret: <hidden> }}", self.port)
    }
}

pub const MCP_PORT_ENV: &str = "NEKO_MCP_PORT";
pub const MCP_SECRET_ENV: &str = "NEKO_MCP_SECRET";

/// How a run's `PreToolUse` hook helper (`neko-daemon --hook`) reaches the daemon: its loopback port and the run's own secret,
/// handed to the CLI's environment for that run only (docs/knowledge/permissions.md).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookLink {
    pub port: u16,
    pub secret: String,
    /// The helper Claude Code runs as the hook (the daemon's own binary).
    pub helper: PathBuf,
}

/// The environment variables a hook helper reads.
pub const HOOK_PORT_ENV: &str = "NEKO_HOOK_PORT";
pub const HOOK_SECRET_ENV: &str = "NEKO_HOOK_SECRET";

/// What a tool wants to do, as Neko's policy sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Read a file or a folder.
    Read { path: String },
    /// Write, edit or create a file.
    Edit { path: String },
    /// Run a command line.
    Command { command: String },
    /// Reach the network (a URL or a search).
    Network { target: String },
    /// A tool of an MCP server (`mcp__<server>__<tool>`): a plugin of Neko's, or one of the person's own Claude Code servers. `input` is
    /// what it was called with, as compact JSON (shortened).
    Mcp { server: String, tool: String, input: String },
    /// Anything else, named by the tool.
    Other { summary: String },
    /// Claude Code's own bookkeeping (its plan, a subagent, a question to the person): nothing on the Mac changes by it, so Neko
    /// leaves it to Claude Code.
    Internal { tool: String },
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Stopped,
    Failed(Failure),
}

/// Why a run failed, so the task can say what to do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    NotInstalled,
    NotSignedIn,
    /// The account's usage limit. `resets` is the CLI's own words or time for when it resets, if it said.
    UsageLimit { resets: Option<String> },
    /// The working folder is gone.
    FolderGone,
    /// The CLI ended without finishing: its exit code and its last words on stderr (shortened).
    Crashed { code: Option<i32>, words: String },
    /// The CLI said the run failed; its words.
    Reported(String),
    /// Ran past the wall-clock limit.
    TimedOut,
    /// Said nothing for too long.
    Stalled,
    /// Asking before risky steps, a step ran that Neko's hook could not check; the words name it.
    Unchecked { step: Option<String> },
    /// Asking before risky steps, the hook helper is missing or cannot run, so nothing started.
    HookMissing,
}

/// The usage a CLI reported for a run. Figures it did not report are zero or None, never guessed.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Reported {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub cost_usd: Option<f64>,
    pub model: Option<String>,
    /// The context the last model call held (its input, cache read and cache written), when a message reported its usage.
    pub context_tokens: Option<u64>,
    /// The model's context window, when the CLI reported it.
    pub context_window: Option<u64>,
}

/// Neko's events, the same for every runtime.
#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    /// The CLI is up: its conversation id (for a follow-up) and model, when it says.
    Started { session: Option<String>, model: Option<String> },
    /// Words of the answer, as they arrive.
    TextDelta(String),
    /// The current block of words is whole.
    TextDone,
    /// The agent began thinking (Claude Code sends no words for it, only that it thinks).
    ThinkingStarted,
    /// Thinking, as it arrives.
    ThinkingDelta(String),
    ThinkingDone,
    /// A tool call began: its id, its name, a few words of what it does, and its input (shortened).
    ToolStarted { id: String, name: String, summary: String, input: String },
    /// A tool call ended, with its output (shortened). `ok` is false only when the tool could not run (denied, an error, cut
    /// off); a command that ran and exited non-zero is `ok` with its `exit` code.
    ToolFinished { id: String, ok: bool, output: String, exit: Option<i32> },
    /// The agent asks to do something. Answer with [`Control::answer`] and the same id. `tool_use` is the call it is for.
    PermissionAsked { id: String, tool: String, action: Action, tool_use: Option<String> },
    /// The CLI took back a request it asked (it no longer needs the answer).
    PermissionWithdrawn { id: String },
    /// A hook gave Claude Code no usable verdict, and it ran the call unchecked (it fails open).
    Unchecked { tool_use: Option<String> },
    Usage(Reported),
    /// The account's limit news: `status` is as the CLI said it (`allowed`, `allowed_warning`, `rejected`…), `resets_at` seconds
    /// since the epoch when it said. The daemon holds back runs it starts by itself while the limit is reached.
    RateLimit { status: String, resets_at: Option<i64> },
    /// The last event.
    Finished(Outcome),
}

/// A permission answer, as a runtime passes it on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Allow,
    Deny,
}

/// Why a runtime could not do what it was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlError(pub String);

/// The running agent, as the daemon steers it. Safe to call from any thread.
pub trait Control: Send + Sync {
    /// More words from the person, while it runs.
    fn steer(&self, words: &str) -> Result<(), ControlError>;
    /// The answer to a [`AgentEvent::PermissionAsked`].
    fn answer(&self, id: &str, answer: Answer) -> Result<(), ControlError>;
    /// Stops it now. The event stream ends with [`Outcome::Stopped`] within a second. Calling it twice is harmless.
    fn stop(&self);
}

/// A started run: its control and its events.
pub struct Started {
    pub control: Arc<dyn Control>,
    pub events: Receiver<AgentEvent>,
}

/// An agent CLI Neko can run.
pub trait Runtime: Send + Sync {
    /// Short and stable ("claude").
    fn id(&self) -> &'static str;
    /// For people ("Claude Code").
    fn name(&self) -> &'static str;
    /// The one line that signs in, for Settings and for the notice when it is not signed in.
    fn sign_in_line(&self) -> &'static str;
    /// The one line that installs it.
    fn install_line(&self) -> &'static str;
    /// Finds the CLI and reads its version and login, spending nothing.
    fn detect(&self) -> Detection;
    /// Runs it. An error is a failure before anything ran (not installed, the folder gone).
    fn start(&self, spec: StartSpec) -> Result<Started, Failure>;
}

/// Shortens `s` to at most `max` bytes at a character boundary, saying how much was left out.
pub fn shorten(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\n… ({} more bytes)", &s[..cut], s.len() - cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_keeps_characters_whole() {
        assert_eq!(shorten("abc", 5), "abc");
        assert!(shorten("ééé", 3).starts_with('é'));
        assert!(shorten(&"x".repeat(100), 10).ends_with("(90 more bytes)"));
    }

    #[test]
    fn logins_say_how_a_cost_reads() {
        assert_eq!(Login::Subscription(Some("max".into())).pricing(), Pricing::ApiEquivalent);
        assert_eq!(Login::ApiKey.pricing(), Pricing::Charged);
        assert_eq!(Login::Subscription(Some("max".into())).words("Claude"), "Claude subscription (Max)");
    }
}

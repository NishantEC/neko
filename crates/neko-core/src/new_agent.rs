//! Starting a coding agent — the write half of what [`crate::agents`] reads.
//!
//! `agents.rs` answers "what is running right now" by reading Paseo's own
//! on-disk state. This module answers the other half of the same question the
//! captain asked, and which `AGENTS.md`'s "Agents" section listed as **not
//! built**: start a new one from the launcher, with the task typed straight
//! into the search field.
//!
//! **The shape: a command, a mode, and one provider whose rows are places to
//! work.** `commands::CommandsProvider` offers a `New Agent` row; confirming
//! it enters the `new-agent` mode (`crate::modes` in the client), which scopes
//! every keystroke to this provider. Inside that mode **the query is the
//! prompt, not a filter** — the one provider in this codebase where typing
//! does not narrow the list — and each row is a directory the agent could work
//! in. Enter starts it there.
//!
//! **Why the rows are directories, and why the working directory is never
//! guessed.** An agent's working directory decides what it can see, edit and
//! break, so it is not something to infer:
//!
//! - **The daemon's own `cwd` is meaningless.** `neko-daemon` is spawned by
//!   the client (`daemon_launcher`) or by a LaunchAgent, so it inherits
//!   whatever directory that process happened to be in — often `/`. Inheriting
//!   it would make "where does my agent start" depend on how neko itself was
//!   started.
//! - **`$HOME` is worse than nothing** — an agent rooted at the home directory
//!   has no project, and can wander into every one of them.
//! - **A compiled-in path is out** by the brief, and rightly: this app is not
//!   the owner of that fact.
//! - **A hidden preference** ("the agent directory") would put the most
//!   consequential input behind a settings window, invisible at the moment it
//!   matters.
//!
//! So the choice is made *at the moment of starting*, from the list of
//! projects **Paseo itself already knows about** —
//! `~/.paseo/projects/projects.json`, the same "read the tool's own on-disk
//! state" source `agents.rs` established. Every row shows its own absolute
//! path, so the directory an agent gets is never implied.
//!
//! **`projects.json`, not `workspaces.json`** — both sit in that directory and
//! either would parse. Measured on the verification machine: `projects.json`
//! has **8** entries, one per real project root, each with a name a person
//! recognises; `workspaces.json` has **200**, of which 21 are unarchived, with
//! the *same* directory repeated up to four times (one entry per branch or
//! checkout) plus Paseo-owned transient worktrees that may be deleted
//! underneath us. A list of eight named projects is something to arrow
//! through; a list of twenty-one mostly-duplicate branch rows is not.
//!
//! **Which agent tool it starts is not invented either — it is the one already
//! in use in that directory.** `paseo run` **requires** an explicit
//! `--provider`: verified live against the real CLI, which answers
//! `MISSING_PROVIDER` without one, and `~/.paseo/config.json` carries no
//! default to fall back on. Hard-coding `claude` would be this app making a
//! choice it has no business making, so each row instead names the provider the
//! most recent real agent in that project used (`agents::provider_usage`),
//! falling back to the most recent one anywhere, and **shows it**
//! (`claude · ~/Documents/neko`) so it is stated rather than implied. A machine
//! with no agent history at all cannot be answered honestly, and the row says
//! so instead of guessing.
//!
//! **Spawning shells out to the `paseo` CLI.** Paseo's daemon does listen on
//! `127.0.0.1:6767`, and a direct call would be milliseconds rather than the
//! CLI's ~1s of Electron/node boot — but that is a private, undocumented
//! protocol between two components that ship and version together, and
//! guessing at it would break silently on their next release. `paseo run` is
//! the documented, supported entry point, and this codebase already accepts a
//! subprocess for `mdfind` in `apps.rs`/`files.rs` for the same reason: it is
//! the interface that actually exists.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};
use serde::Deserialize;

use crate::agents;
use crate::provider::{Provider, ProviderError};
use crate::search::Candidate;

/// How long [`PaseoCli`] waits for `paseo run` to confirm the agent exists
/// before giving up on *confirming* it (it never kills it — see
/// [`PaseoCli::spawn`]).
///
/// Sized from a measurement, not a guess: `paseo status --json`, the cheapest
/// command the CLI has, takes **~0.95s wall** on the verification machine,
/// essentially all of it Electron/node boot (the CLI runs through
/// `Paseo Helper.app` with `ELECTRON_RUN_AS_NODE=1`). A real `paseo run -d` is
/// that plus one local daemon round-trip. 20s is far past both, so reaching it
/// means something is genuinely wrong rather than merely slow.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(20);

/// Stderr is echoed back into the panel's own footer, which is one line. Long
/// enough to carry a real CLI error, short enough that it stays a sentence.
const MAX_ERROR_CHARS: usize = 240;

/// Ordering is carried in the score because the daemon sorts a scoped search
/// by score (`handle_request`), and this provider has no relevance signal to
/// sort by — the query is the prompt, so it says nothing about which directory
/// is wanted. Each row scores one below the previous, most recently used
/// project first, so the list arrives in Paseo's own recency order.
const ORDER_BASE: f32 = 1000.0;

/// The row's own verb, once there is a task to give the agent.
const START_LABEL: &str = "Start agent  ↵";
/// …and before there is. `action_label` is per-row data the client renders
/// verbatim, so this is how the mode says "type something first" without a new
/// field, a new state, or a control that does nothing.
const NEEDS_PROMPT_LABEL: &str = "Type the task first";
/// …and when this machine has never run an agent at all, so there is no tool to
/// name and `paseo run` cannot be called honestly. Says what is missing rather
/// than offering an Enter that would fail.
const NEEDS_HISTORY_LABEL: &str = "Start one from Paseo first";

/// What one `paseo run` produced. Returned by [`AgentSpawner::spawn`] so a
/// caller can log or show the new agent's id; nothing on the wire uses it yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnedAgent {
    /// Paseo's own id for the created agent, when its `--json` output carried
    /// one. `None` is not a failure — a successful exit is the confirmation
    /// that matters, and the id's exact JSON key is Paseo's to change.
    pub id: Option<String>,
}

/// The seam that keeps this module's tests from starting real agents on a real
/// machine — injected exactly the way `agents::AgentsProvider::with_root`
/// injects its read source and `panel::AppearanceSetter` injects its native
/// call. There is no environment variable and no "dry run" flag: the only way
/// to run the real CLI is to hold a [`PaseoCli`], and no test does.
pub trait AgentSpawner: Send + Sync {
    /// Start a `provider` agent working in `cwd` on `prompt`. `Err` carries a
    /// message meant to be read by a person in the panel's own footer.
    fn spawn(&self, cwd: &Path, prompt: &str, provider: &str) -> Result<SpawnedAgent, String>;
}

/// The real spawner: `paseo run --background --json --cwd <dir> -- <prompt>`.
pub struct PaseoCli {
    executable: Option<PathBuf>,
    timeout: Duration,
}

impl Default for PaseoCli {
    fn default() -> Self {
        Self {
            executable: resolve_executable(),
            timeout: CONFIRM_TIMEOUT,
        }
    }
}

/// Where the `paseo` executable is looked for, **absolute paths before
/// `PATH`**.
///
/// `PATH` is not trustworthy here, and that is not defensive coding: this
/// daemon is started either by the GUI client or by a LaunchAgent, so it
/// inherits launchd's own `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`) rather than
/// a login shell's — `~/.local/bin`, where Paseo installs its CLI shim, is not
/// on it. A bare `paseo` is still the last resort, because somebody who
/// installed the CLI elsewhere has usually put it on `PATH`, and failing for
/// lack of one more attempt would be silly.
const PASEO_CANDIDATES: &[&str] = &[
    // Paseo's own installer target: a shell shim that re-enters the app
    // bundle's Electron helper.
    "$HOME/.local/bin/paseo",
    // That shim's own destination, for an install where `~/.local/bin` was
    // never created.
    "/Applications/Paseo.app/Contents/Resources/bin/paseo",
    "/usr/local/bin/paseo",
];

fn resolve_executable() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok();
    for candidate in PASEO_CANDIDATES {
        let expanded = match (candidate.strip_prefix("$HOME/"), &home) {
            (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
            (Some(_), None) => continue,
            (None, _) => PathBuf::from(candidate),
        };
        if expanded.is_file() {
            return Some(expanded);
        }
    }
    Some(PathBuf::from("paseo"))
}

/// The exact argument vector, factored out so a test can assert the command
/// shape without running anything.
///
/// - `--background` (`-d`) is what makes this return once the agent has been
///   *created and started*, rather than streaming its whole session; without
///   it the CLI stays attached until the agent finishes, which for a coding
///   task is minutes.
/// - `--json` so a machine reads the answer.
/// - `--` before the prompt, so a task that happens to begin with a dash is a
///   prompt and not a mis-parsed flag.
/// - `--provider` is **not optional**: the CLI answers `MISSING_PROVIDER`
///   without it (verified live) and there is no configured default to inherit.
///   *Which* provider is passed is decided by [`provider_for`] from real usage,
///   never chosen by this module.
/// - No `--model` and no `--mode`: Paseo's own per-provider defaults apply, and
///   those are preferences that live in Paseo. A model picker would be a second
///   axis in a mode whose one axis is already the prompt.
fn paseo_argv(cwd: &Path, prompt: &str, provider: &str) -> Vec<String> {
    vec![
        "run".to_string(),
        "--background".to_string(),
        "--json".to_string(),
        "--provider".to_string(),
        provider.to_string(),
        "--cwd".to_string(),
        cwd.to_string_lossy().to_string(),
        "--".to_string(),
        prompt.to_string(),
    ]
}

/// **`--cwd` is not authoritative if the CLI can tell it was launched from
/// inside another agent or a workspace terminal — measured live, and it
/// silently started an agent in the wrong repository.**
///
/// Found during this module's own end-to-end verification: a run with an
/// explicit `--cwd /tmp/neko-new-agent-verify-32104` produced an agent whose
/// real `Cwd` (`paseo inspect`) was `/Users/example/Documents/neko` — the
/// inherited `PASEO_AGENT_CWD` of the session that ran the check. Exactly the
/// "wrong repository, no error" outcome this whole module is arranged to
/// prevent.
///
/// The CLI's own bundled source states the rule
/// (`Contents/Resources/app.asar`, `resolveRunWorkspace`) — "Workspace policy
/// for `paseo run`. Precedence: 1. `--workspace <id>` … 2. `$PASEO_AGENT_ID`
/// -> daemon resolves the caller's workspace, 3. `$PASEO_WORKSPACE_ID` ->
/// exported by workspace terminals, … 5. bare run -> mint a new local-backed
/// workspace for cwd". Note that `--cwd` is *passed* in case 2 and still loses:
/// the CLI hands the daemon a `callerAgentId` and no workspace id, and the
/// daemon resolves the caller's workspace server-side. There is no flag that
/// turns this off.
///
/// This is not a test-only artifact. `neko-daemon` inherits the environment of
/// whatever started it, and a captain developing neko *from* an agent session
/// or a Paseo workspace terminal would hit it on every spawn. So the child gets
/// a deliberately de-scoped environment: with these gone, the CLI falls to
/// case 5 and `--cwd` is the only answer left.
///
/// `PASEO_CLI` is deliberately **not** removed — the shim sets it to its own
/// path for re-entry, and it says nothing about who is calling.
const AGENT_SCOPING_ENV: &[&str] = &["PASEO_AGENT_ID", "PASEO_AGENT_CWD", "PASEO_WORKSPACE_ID"];

/// The whole command, argv and environment together, so a test can assert both
/// without running anything.
fn paseo_command(executable: &Path, cwd: &Path, prompt: &str, provider: &str) -> Command {
    let mut command = Command::new(executable);
    command
        .args(paseo_argv(cwd, prompt, provider))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for scoping in AGENT_SCOPING_ENV {
        command.env_remove(scoping);
    }
    command
}

/// What to say when the CLI has not answered inside [`PaseoCli::timeout`].
///
/// It deliberately claims **neither** outcome. `paseo run` may already have
/// created the agent by the time the deadline passes, so "couldn't start it"
/// would be a lie as often as not; saying the outcome is unknown, and where to
/// look, is the only honest sentence available.
fn unconfirmed_message(timeout: Duration) -> String {
    format!(
        "paseo run has not answered in {}s — the agent may still be starting; check Paseo",
        timeout.as_secs()
    )
}

impl AgentSpawner for PaseoCli {
    /// **Bounded, and it never kills the child.**
    ///
    /// The wait is the `run_bounded_child` shape `files.rs` already uses for
    /// `mdfind` — a thread doing the blocking read, an `mpsc::recv_timeout`
    /// bounding it — for the same two reasons: reading the pipes on another
    /// thread means a chatty child can never deadlock on a full pipe buffer,
    /// and the deadline means a hung CLI cannot own this thread forever.
    ///
    /// **Unlike `files.rs`, the child is not killed on timeout.** An abandoned
    /// `mdfind` has no side effect worth keeping; `paseo run` may already have
    /// created the agent, and killing it then could leave a real agent running
    /// while the captain is told it failed. The child is handed to a detached
    /// reaper (so it is still waited on and never becomes a zombie) and the
    /// message says the outcome is unknown — see [`unconfirmed_message`].
    ///
    /// **This blocks the calling thread for as long as the CLI takes (~1s).**
    /// That is deliberate, and it is not the daemon blocking:
    /// `handle_connection` gives every request its own thread precisely so a
    /// slow one cannot queue the next keystroke behind it, and nothing here
    /// touches `AppState`'s `Db` mutex or the connection's reader loop.
    /// Spending that second is what buys an honest answer — a
    /// `Response::Error` the panel shows inline
    /// (`panel::Root::activation_error`) instead of a panel that closes as if
    /// the agent had started.
    fn spawn(&self, cwd: &Path, prompt: &str, provider: &str) -> Result<SpawnedAgent, String> {
        let executable = self.executable.clone().ok_or_else(|| {
            "the paseo CLI was not found (looked in ~/.local/bin and /Applications/Paseo.app)"
                .to_string()
        })?;

        let child = paseo_command(&executable, cwd, prompt, provider)
            .spawn()
            .map_err(|e| format!("couldn't run {}: {e}", executable.display()))?;

        let (tx, rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let outcome = child.wait_with_output();
            // The receiver is gone on the timeout path; that is precisely the
            // case this thread exists to keep tidy, so a failed send is
            // expected rather than an error.
            let _ = tx.send(outcome);
        });

        match rx.recv_timeout(self.timeout) {
            Ok(Ok(output)) if output.status.success() => {
                let _ = waiter.join();
                Ok(SpawnedAgent {
                    id: parse_agent_id(&String::from_utf8_lossy(&output.stdout)),
                })
            }
            Ok(Ok(output)) => {
                let _ = waiter.join();
                Err(describe_failure(
                    output.status.code(),
                    &String::from_utf8_lossy(&output.stdout),
                    &String::from_utf8_lossy(&output.stderr),
                ))
            }
            Ok(Err(e)) => {
                let _ = waiter.join();
                Err(format!("paseo run could not be waited on: {e}"))
            }
            Err(_) => {
                // Detached on purpose: see this method's doc comment. The
                // thread owns the child and reaps it whenever it exits.
                std::thread::spawn(move || {
                    let _ = waiter.join();
                });
                Err(unconfirmed_message(self.timeout))
            }
        }
    }
}

/// A CLI failure, phrased to read correctly after the panel footer's own
/// `"Couldn't open — "` prefix (`panel::render_footer`). That prefix predates
/// any activation that isn't an "open", and rewording shared, frozen copy for
/// one provider is the captain's call rather than this task's — so the message
/// is written to survive it instead.
fn describe_failure(code: Option<i32>, stdout: &str, stderr: &str) -> String {
    let detail = json_error_message(stdout)
        .or_else(|| flatten(stderr))
        .or_else(|| flatten(stdout))
        .map(|detail| detail.chars().take(MAX_ERROR_CHARS).collect::<String>());
    match (code, detail) {
        (_, Some(detail)) => format!("paseo run failed: {detail}"),
        (Some(code), None) => format!("paseo run exited with status {code}"),
        (None, None) => "paseo run was terminated by a signal".to_string(),
    }
}

/// **The CLI reports failures as JSON on `stdout`, not on `stderr`** — found
/// live, not assumed: a deliberately bad run exited `1` with an empty `stderr`
/// and `{"error":{"code":"MISSING_PROVIDER","message":…,"details":…}}` on
/// stdout. Reading stderr alone produced a useless "exited with status 1",
/// which is exactly the uninformative-failure shape this provider exists not to
/// have. `details` is appended when present, because it is the half that says
/// what to do about it.
fn json_error_message(stdout: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    let error = value.get("error")?;
    let message = error.get("message").and_then(|m| m.as_str())?;
    match error.get("details").and_then(|d| d.as_str()) {
        Some(details) => Some(format!("{message} — {details}")),
        None => Some(message.to_string()),
    }
}

/// Whatever a stream said, as one line — `None` if it said nothing.
fn flatten(raw: &str) -> Option<String> {
    let joined = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!joined.is_empty()).then_some(joined)
}

/// Best-effort: Paseo's `--json` output is its own shape to change, and the
/// agent really was created either way, so an unrecognised document costs a
/// log line rather than turning a success into a failure. Accepts either a
/// bare object or one wrapping the agent, and either spelling of the key.
fn parse_agent_id(stdout: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    for key in ["id", "agentId"] {
        if let Some(id) = value.get(key).and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
        if let Some(id) = value
            .get("agent")
            .and_then(|a| a.get(key))
            .and_then(|v| v.as_str())
        {
            return Some(id.to_string());
        }
    }
    None
}

/// The subset of one `projects.json` entry this provider reads. Partial on
/// purpose, exactly as `agents::PaseoAgent` is: `serde` ignores what is not
/// named here, so Paseo adding fields cannot break neko.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaseoProject {
    root_path: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
    /// Paseo's soft-delete. A project the captain archived is not somewhere to
    /// start new work.
    #[serde(default)]
    archived_at: Option<String>,
}

/// One offered working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Project {
    root: PathBuf,
    name: String,
}

/// `~/.paseo/projects/projects.json`. Not configurable, same reasoning as
/// `agents::agents_root`: it is Paseo's own layout rather than a neko setting,
/// and a wrong value would read as "no projects" instead of as an error.
fn default_projects_file() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".paseo/projects/projects.json"))
}

/// Reads the offerable projects, most recently used first.
///
/// Four filters, each for a defect that would otherwise be silent: archived
/// entries (the captain already said he is done with them), blank paths, a
/// directory that **no longer exists** (a row that could only ever fail on
/// Enter), and the same directory listed twice (Paseo keys projects by
/// identity, not by path, so one repo reachable two ways can appear twice).
///
/// Sorted by `updatedAt` descending as a **string**: these are RFC 3339 UTC
/// timestamps in one fixed layout (`2026-08-21T16:00:21.343Z`), so
/// lexicographic order is chronological order, and parsing them properly would
/// mean a date/time dependency this codebase has repeatedly declined to add for
/// one ordering (see `agents::short_time`). An entry with no timestamp sorts
/// last rather than being dropped.
///
/// An unreadable or malformed file is an empty list, not an error — the same
/// rule, for the same reason, as `agents::read_agents`: this is another
/// application's file, possibly mid-write, and there is nothing a person could
/// do with the complaint.
fn read_projects(path: &Path) -> Vec<Project> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(entries) = serde_json::from_str::<Vec<PaseoProject>>(&raw) else {
        return Vec::new();
    };

    let mut live: Vec<&PaseoProject> = entries
        .iter()
        .filter(|p| p.archived_at.is_none())
        .filter(|p| {
            p.root_path
                .as_deref()
                .map(str::trim)
                .is_some_and(|root| !root.is_empty())
        })
        .collect();
    live.sort_by(|a, b| {
        b.updated_at
            .as_deref()
            .unwrap_or("")
            .cmp(a.updated_at.as_deref().unwrap_or(""))
    });

    let mut seen: Vec<PathBuf> = Vec::new();
    let mut projects = Vec::new();
    for entry in live {
        let root = PathBuf::from(entry.root_path.as_deref().unwrap_or("").trim());
        if !root.is_dir() || seen.contains(&root) {
            continue;
        }
        seen.push(root.clone());
        let name = entry
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                root.file_name().map_or_else(
                    || root.to_string_lossy().to_string(),
                    |n| n.to_string_lossy().to_string(),
                )
            });
        projects.push(Project { root, name });
    }
    projects
}

/// Which agent tool a new agent in `root` should be started with: the one the
/// most recent real agent *in that directory* used, else the most recent one
/// used anywhere, else `None`.
///
/// The per-directory answer first because that is the actually informative one
/// — a repo where every past agent was `codex` should not suddenly get
/// `claude` because the last thing started machine-wide happened to be one. The
/// machine-wide fallback covers a project that has never had an agent, which is
/// the common case for a repo just registered with Paseo.
///
/// `None` — no agent has ever run on this machine — is left as `None` rather
/// than defaulted, on purpose: `paseo run` demands a provider, and choosing one
/// here would be this app deciding which coding tool somebody uses. The row
/// says it does not know instead (see [`NEEDS_HISTORY_LABEL`]).
fn provider_for(usage: &[agents::ProviderUse], root: &Path) -> Option<String> {
    usage
        .iter()
        .find(|used| used.cwd.starts_with(root))
        .or_else(|| usage.first())
        .map(|used| used.provider.clone())
}

/// The `⌘K` menu for a project row: start it with a *different* tool.
///
/// **Because running out of tokens is why anybody switches.** The row's own
/// Enter uses whatever the last real agent in that directory used, which is
/// right almost always and useless in the one moment it matters — a five-hour
/// window is spent and the work has to continue somewhere else. So each
/// vendor neko can read a quota for gets an entry, and the entry *carries the
/// headroom*, because the whole decision is "which of these has room left".
///
/// `current` is greyed out of the list rather than shown as a no-op: it is
/// already what Enter does.
///
/// Numbers come from [`usage::cached`] and never from a fetch — this runs on
/// every keystroke of the prompt. A cold cache shows plain names and
/// [`usage::warm_in_background`] fills them in a keystroke or two later,
/// which is the honest trade: a stalled first character would be worse than a
/// number that arrives second.
fn provider_actions(current: Option<&str>) -> Vec<ItemAction> {
    crate::usage::warm_in_background();
    let cached = crate::usage::cached();
    crate::usage::Vendor::ALL
        .iter()
        .filter(|vendor| Some(vendor.id()) != current)
        .map(|vendor| {
            let headroom = cached.as_ref().and_then(|all| {
                all.iter()
                    .find(|u| u.vendor == *vendor)
                    .and_then(|u| u.headroom_percent())
            });
            let label = match headroom {
                // Rounded down, deliberately: a quota reported as 3% left
                // when it is 3.7% is the safe direction to be wrong in.
                Some(left) => format!("Start with {} — {}% left", vendor.name(), left.floor()),
                None => format!("Start with {}", vendor.name()),
            };
            ItemAction {
                id: format!("provider:{}", vendor.id()),
                label,
                destructive: false,
            }
        })
        .collect()
}

fn tildify(path: &Path) -> String {
    let raw = path.to_string_lossy().to_string();
    let Some(home) = std::env::var("HOME").ok().filter(|home| !home.is_empty()) else {
        return raw;
    };
    match raw.strip_prefix(&home) {
        Some(rest) => format!("~{rest}"),
        None => raw,
    }
}

pub struct NewAgentProvider {
    projects_file: Option<PathBuf>,
    /// Paseo's agent directory — read for *which tool*, not for what is
    /// running (that is `agents::AgentsProvider`'s job). Shared source, one
    /// reader each.
    agents_root: Option<PathBuf>,
    spawner: Arc<dyn AgentSpawner>,
}

impl NewAgentProvider {
    /// Does no I/O — the projects file is read per search, so a project
    /// registered in Paseo after the daemon started shows up on the next
    /// keystroke without a restart or an invalidation message. Same choice, for
    /// the same reason, `files::FileProvider` makes about its configured
    /// scope; the file is 3KB, read once per keystroke of a mode nobody is in
    /// most of the time.
    pub fn new() -> Self {
        Self {
            projects_file: default_projects_file(),
            agents_root: agents::Backend::Paseo.root(),
            spawner: Arc::new(PaseoCli::default()),
        }
    }

    /// The hermetic constructor: fixture files and a spawner that records
    /// instead of running. **Every test in this repo uses this** — starting a
    /// real agent on the machine running `cargo test` is not something a test
    /// suite may do.
    pub fn with_sources_and_spawner(
        projects_file: PathBuf,
        agents_root: PathBuf,
        spawner: Arc<dyn AgentSpawner>,
    ) -> Self {
        Self {
            projects_file: Some(projects_file),
            agents_root: Some(agents_root),
            spawner,
        }
    }

    /// The tool each project's next agent would be started with, resolved once
    /// per call rather than per row — one pass over Paseo's agent documents,
    /// the same read `agents::AgentsProvider` already makes on every root-list
    /// search.
    fn usage(&self) -> Vec<agents::ProviderUse> {
        self.agents_root
            .as_deref()
            .map(agents::provider_usage)
            .unwrap_or_default()
    }
}

impl Default for NewAgentProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for NewAgentProvider {
    fn id(&self) -> &'static str {
        "new-agent"
    }

    fn section_label(&self) -> &'static str {
        "Start an Agent"
    }

    /// Registered in `AppState::mode_providers`, so a root-list query never
    /// reaches this provider at all and this is belt-and-braces. It is still
    /// the honest answer: a list of directories means nothing until somebody
    /// has said they want to start an agent.
    fn answers_empty_root_query(&self) -> bool {
        false
    }

    /// **The query is the prompt, not a filter** — the one provider here where
    /// typing does not narrow the list. Every registered project is returned
    /// every time, in the same order, so the row that is highlighted stays
    /// highlighted as the task is typed (`panel::resolve_selection` keys the
    /// highlight on `(kind, id)`, and `id` is the directory, which does not
    /// change).
    ///
    /// The consequence, stated rather than hidden: **there is no way to reach a
    /// directory Paseo has never seen.** A "type a path" row is the obvious
    /// next step and is deliberately not built — it is a second input in a
    /// surface with one field, and every real target on the verification
    /// machine was already in this list.
    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let Some(file) = &self.projects_file else {
            return Vec::new();
        };
        let prompt = query.trim();
        let usage = self.usage();
        read_projects(file)
            .into_iter()
            .enumerate()
            .map(|(index, project)| {
                let provider = provider_for(&usage, &project.root);
                Candidate {
                    score: ORDER_BASE - index as f32,
                    item: to_item(&project, prompt, provider.as_deref()),
                }
            })
            .collect()
    }

    /// Never reached: the daemon calls [`Provider::activate_with_query`], and a
    /// row here cannot be acted on without the prompt it exists *for*. Errors
    /// rather than starting an agent with an empty task, the same way
    /// `commands::CommandsProvider::activate` errors rather than silently doing
    /// nothing.
    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        Err(ProviderError(
            "starting an agent needs the task that was typed — this row is only activatable with a query"
                .to_string(),
        ))
    }

    /// `id` is the working directory; `query` is the prompt.
    ///
    /// Both are re-checked here rather than trusted from the row: the row was
    /// built at the last keystroke, and a directory can be moved or deleted
    /// between then and Enter. An empty prompt is refused with a sentence the
    /// captain can act on, rather than starting an agent with nothing to do.
    fn activate_with_query(&self, id: &str, query: &str) -> Result<(), ProviderError> {
        let prompt = query.trim();
        if prompt.is_empty() {
            return Err(ProviderError(
                "type the task for the agent first".to_string(),
            ));
        }
        let cwd = PathBuf::from(id);
        if !cwd.is_dir() {
            return Err(ProviderError(format!("no longer a folder: {id}")));
        }
        // Re-resolved rather than carried on the row for the same reason the
        // directory is re-checked: the row is one keystroke old, and this is
        // the value that actually goes on a command line.
        let provider = provider_for(&self.usage(), &cwd).ok_or_else(|| {
            ProviderError(
                "no agent provider to use — paseo run needs one and nothing has run yet; start an agent \
                 from Paseo once and New Agent will use the same one"
                    .to_string(),
            )
        })?;
        self.spawn_with(&cwd, prompt, &provider)
    }

    /// `⌘K` → "Start with Codex" — the same spawn, with the tool overridden.
    ///
    /// The prompt arrives here because `perform_action_with_query` passes it
    /// on; without that the action would know *where* to start an agent and
    /// not *what to ask it*, which is most of the point.
    fn perform_action_with_query(
        &self,
        id: &str,
        action_id: &str,
        query: &str,
    ) -> Result<(), ProviderError> {
        let Some(provider) = action_id.strip_prefix("provider:") else {
            return Err(ProviderError(format!(
                "no action '{action_id}' on this row"
            )));
        };
        let prompt = query.trim();
        if prompt.is_empty() {
            return Err(ProviderError(
                "type the task for the agent first".to_string(),
            ));
        }
        let cwd = PathBuf::from(id);
        if !cwd.is_dir() {
            return Err(ProviderError(format!("no longer a folder: {id}")));
        }
        // **No history check on this path, unlike Enter's.** Enter has to
        // infer a tool and refuses when it cannot; this one was *told*, which
        // is exactly what makes it the way out of a fresh machine as well as
        // the way out of a spent quota.
        self.spawn_with(&cwd, prompt, provider)
    }
}

impl NewAgentProvider {
    fn spawn_with(&self, cwd: &Path, prompt: &str, provider: &str) -> Result<(), ProviderError> {
        self.spawner
            .spawn(cwd, prompt, provider)
            .map(|_| ())
            .map_err(ProviderError)
    }
}

fn to_item(project: &Project, prompt: &str, provider: Option<&str>) -> SearchItem {
    let where_and_what = match provider {
        // The same `"claude · ~/Documents/neko"` shape `agents.rs` already uses
        // for a running agent's own row — the tool and the place, which
        // together are what distinguishes one agent from another.
        Some(provider) => format!("{provider} · {}", tildify(&project.root)),
        None => tildify(&project.root),
    };
    let action_label = match (prompt.is_empty(), provider) {
        (_, None) => NEEDS_HISTORY_LABEL.to_string(),
        (true, Some(_)) => NEEDS_PROMPT_LABEL.to_string(),
        (false, Some(_)) => START_LABEL.to_string(),
    };
    SearchItem {
        // The directory, and nothing else. It must be **stable across
        // keystrokes**: `panel::resolve_selection` follows the highlight by
        // `(kind, id)`, so folding the prompt in here would silently return the
        // selection to the first row on every character typed — and Enter would
        // then start the agent in the wrong repository. That is why the prompt
        // travels in `Request::Activate`'s own `query` field instead; see that
        // field's doc comment for the full account.
        id: project.root.to_string_lossy().to_string(),
        kind: "new-agent".to_string(),
        title: project.name.clone(),
        subtitle: Some(where_and_what),
        icon: Icon::Glyph(Glyph::Agent),
        section_label: "Start an Agent".to_string(),
        action_label,
        badge: None,
        accessory: None,
        enters_mode: None,
        group_label: None,
        actions: provider_actions(provider),
        source: Some(project.root.to_string_lossy().to_string()),
        meter: None,
        keeps_open: false,
        preview_markdown: false,
        speaker: None,
        images: Vec::new(),
        preview: None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_provider_is_not_offered_as_a_switch() {
        // Enter already does that one; listing it would be a menu entry whose
        // only effect is to be indistinguishable from pressing Enter.
        let actions = provider_actions(Some("claude"));
        let ids: Vec<&str> = actions.iter().map(|a| a.id.as_str()).collect();
        assert!(!ids.contains(&"provider:claude"));
        assert!(ids.contains(&"provider:codex"));
        assert!(ids.contains(&"provider:grok"));
    }

    #[test]
    fn a_machine_with_no_history_still_gets_every_choice() {
        // Enter refuses without history because it has to infer a tool. This
        // path was told which one, so it is the way out of that state.
        assert_eq!(
            provider_actions(None).len(),
            crate::usage::Vendor::ALL.len()
        );
    }

    #[test]
    fn a_switch_action_names_the_vendor_it_switches_to() {
        for action in provider_actions(Some("claude")) {
            let vendor = action
                .id
                .strip_prefix("provider:")
                .expect("a provider action");
            assert!(
                action.label.to_lowercase().contains(vendor),
                "{:?} does not name {vendor}",
                action.label
            );
            assert!(!action.destructive, "starting an agent is not destructive");
        }
    }

    use std::sync::Mutex;

    /// Records what it was asked to start, and starts nothing. The only spawner
    /// any test ever holds.
    #[derive(Default)]
    struct RecordingSpawner {
        /// `(cwd, prompt, provider)` — all three, because all three are things
        /// this module decides, and any one of them being wrong starts the
        /// wrong agent.
        calls: Mutex<Vec<(PathBuf, String, String)>>,
        fail_with: Option<String>,
    }

    impl RecordingSpawner {
        fn failing(message: &str) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_with: Some(message.to_string()),
            }
        }
        fn calls(&self) -> Vec<(PathBuf, String, String)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl AgentSpawner for RecordingSpawner {
        fn spawn(&self, cwd: &Path, prompt: &str, provider: &str) -> Result<SpawnedAgent, String> {
            self.calls.lock().unwrap().push((
                cwd.to_path_buf(),
                prompt.to_string(),
                provider.to_string(),
            ));
            match &self.fail_with {
                Some(message) => Err(message.clone()),
                None => Ok(SpawnedAgent {
                    id: Some("agent-1".to_string()),
                }),
            }
        }
    }

    /// Both of the real files this provider reads, plus real directories for
    /// the entries that are supposed to exist — `read_projects` genuinely stats
    /// them, so a fixture of invented paths would test nothing.
    ///
    /// - `neko` is the most recently used project, and `codex` is the tool last
    ///   used there *and* the most recent tool anywhere.
    /// - `notes_app` is older, and its own history says `claude` — so a row
    ///   naming the machine-wide winner instead of the per-project one fails.
    /// - `fresh` has no agent history at all, and exercises the fallback.
    struct Fixture {
        _dir: tempfile::TempDir,
        projects: PathBuf,
        agents_root: PathBuf,
        neko: PathBuf,
        notes_app: PathBuf,
        fresh: PathBuf,
        gone: PathBuf,
    }

    fn write_agent(root: &Path, workspace: &str, id: &str, provider: &str, cwd: &Path, at: &str) {
        let dir = root.join(workspace);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{id}.json")),
            format!(
                r#"{{"id":"{id}","provider":"{provider}","cwd":"{cwd}","lastStatus":"closed",
                    "lastActivityAt":"{at}"}}"#,
                cwd = cwd.display()
            ),
        )
        .unwrap();
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let neko = dir.path().join("neko");
        let notes_app = dir.path().join("notes_app");
        let fresh = dir.path().join("fresh");
        let gone = dir.path().join("deleted");
        for real in [&neko, &notes_app, &fresh] {
            std::fs::create_dir_all(real).unwrap();
        }

        let projects = dir.path().join("projects.json");
        std::fs::write(
            &projects,
            format!(
                r#"[
                  {{"projectId":"a","rootPath":"{notes_app}","displayName":"acme-corp/notes-app",
                    "updatedAt":"2026-08-15T11:51:09.536Z","archivedAt":null}},
                  {{"projectId":"b","rootPath":"{neko}","displayName":"neko",
                    "updatedAt":"2026-08-21T16:00:21.343Z","archivedAt":null}},
                  {{"projectId":"c","rootPath":"{gone}","displayName":"deleted-worktree",
                    "updatedAt":"2026-08-22T00:00:00.000Z","archivedAt":null}},
                  {{"projectId":"d","rootPath":"{notes_app}","displayName":"same dir again",
                    "updatedAt":"2026-08-01T00:00:00.000Z","archivedAt":null}},
                  {{"projectId":"e","rootPath":"{fresh}","displayName":"fresh",
                    "updatedAt":"2026-07-01T00:00:00.000Z","archivedAt":null}},
                  {{"projectId":"f","rootPath":"{archived}","displayName":"archived",
                    "updatedAt":"2026-08-23T00:00:00.000Z","archivedAt":"2026-08-23T00:00:00.000Z"}}
                ]"#,
                notes_app = notes_app.display(),
                neko = neko.display(),
                gone = gone.display(),
                fresh = fresh.display(),
                archived = dir.path().display(),
            ),
        )
        .unwrap();

        let agents_root = dir.path().join("agents");
        // Deliberately every one of these is `closed`: on the real machine 172
        // of 227 agent documents are, and a finished agent is the best evidence
        // of which tool is actually used somewhere.
        write_agent(
            &agents_root,
            "neko",
            "a1",
            "codex/gpt-5.4",
            &neko,
            "2026-08-22T09:00:00.000Z",
        );
        write_agent(
            &agents_root,
            "hush",
            "a2",
            "claude",
            &notes_app,
            "2026-08-10T09:00:00.000Z",
        );
        Fixture {
            _dir: dir,
            projects,
            agents_root,
            neko,
            notes_app,
            fresh,
            gone,
        }
    }

    fn provider_for_fixture(fixture: &Fixture, spawner: Arc<RecordingSpawner>) -> NewAgentProvider {
        NewAgentProvider::with_sources_and_spawner(
            fixture.projects.clone(),
            fixture.agents_root.clone(),
            spawner,
        )
    }

    #[test]
    fn an_unknown_action_is_refused_rather_than_guessed() {
        let fixture = fixture();
        let (provider, spawner) = recording(&fixture);
        let err = provider
            .perform_action_with_query(&fixture.neko.to_string_lossy(), "archive", "do the thing")
            .expect_err("not a provider action");
        assert!(err.0.contains("no action"), "{}", err.0);
        assert!(spawner.calls().is_empty(), "and nothing was started");
    }

    fn recording(fixture: &Fixture) -> (NewAgentProvider, Arc<RecordingSpawner>) {
        let spawner = Arc::new(RecordingSpawner::default());
        (provider_for_fixture(fixture, spawner.clone()), spawner)
    }

    #[test]
    fn every_registered_project_is_offered_most_recently_used_first() {
        let fixture = fixture();
        let (provider, _) = recording(&fixture);
        let found = provider.search("fix the parser", 0);
        let ids: Vec<String> = found.iter().map(|c| c.item.id.clone()).collect();
        assert_eq!(
            ids,
            vec![
                fixture.neko.to_string_lossy().to_string(),
                fixture.notes_app.to_string_lossy().to_string(),
                fixture.fresh.to_string_lossy().to_string(),
            ],
            "recency order, and nothing else"
        );
        assert!(
            found[0].score > found[1].score,
            "the daemon sorts a scoped search by score"
        );
    }

    #[test]
    fn an_archived_project_a_missing_directory_and_a_duplicate_are_all_left_out() {
        // One assertion per filter would repeat the same fixture three times;
        // the fixture carries all three cases and the count pins all three.
        let fixture = fixture();
        let (provider, _) = recording(&fixture);
        let found = provider.search("anything", 0);
        assert_eq!(found.len(), 3, "6 entries in, 3 offerable out");
        assert!(found.iter().all(|c| c.item.title != "archived"));
        assert!(found.iter().all(|c| c.item.title != "deleted-worktree"));
        assert!(found.iter().all(|c| c.item.title != "same dir again"));
    }

    #[test]
    fn the_typed_query_is_the_prompt_and_never_filters_the_list() {
        // The property the whole mode rests on: two completely different
        // prompts return the identical rows, with identical ids.
        let fixture = fixture();
        let (provider, _) = recording(&fixture);
        let one: Vec<String> = provider
            .search("neko", 0)
            .iter()
            .map(|c| c.item.id.clone())
            .collect();
        let two: Vec<String> = provider
            .search("zzz nothing matches this", 0)
            .iter()
            .map(|c| c.item.id.clone())
            .collect();
        assert_eq!(one, two);
        assert_eq!(one.len(), 3);
    }

    #[test]
    fn a_rows_id_does_not_change_as_the_prompt_is_typed() {
        // The defect this exists to prevent is silent and severe: an id that
        // moved with the prompt would reset `panel::resolve_selection`'s
        // highlight on every keystroke, so Enter would start the agent in
        // whichever repository happens to sort first.
        let fixture = fixture();
        let (provider, _) = recording(&fixture);
        let ids = |query: &str| -> Vec<String> {
            provider
                .search(query, 0)
                .iter()
                .map(|c| c.item.id.clone())
                .collect()
        };
        assert_eq!(ids("f"), ids("fi"));
        assert_eq!(ids("fi"), ids("fix the parser"));
    }

    #[test]
    fn a_row_says_to_type_the_task_first_until_something_is_typed() {
        let fixture = fixture();
        let (provider, _) = recording(&fixture);
        assert_eq!(
            provider.search("", 0)[0].item.action_label,
            NEEDS_PROMPT_LABEL
        );
        assert_eq!(
            provider.search("   ", 0)[0].item.action_label,
            NEEDS_PROMPT_LABEL
        );
        assert_eq!(
            provider.search("do the thing", 0)[0].item.action_label,
            START_LABEL
        );
    }

    #[test]
    fn each_row_names_the_tool_last_used_in_that_project_not_the_last_one_used_anywhere() {
        // `codex` is both the most recent tool overall and the one used in
        // `neko`; `notes_app` has its own history and must keep it, or a
        // per-project answer degrades into a global one without anybody
        // noticing.
        let fixture = fixture();
        let (provider, _) = recording(&fixture);
        let found = provider.search("task", 0);
        assert_eq!(
            found[0]
                .item
                .subtitle
                .as_deref()
                .unwrap()
                .split(" · ")
                .next(),
            Some("codex")
        );
        assert_eq!(
            found[1]
                .item
                .subtitle
                .as_deref()
                .unwrap()
                .split(" · ")
                .next(),
            Some("claude")
        );
        // …and the directory is still stated in full, never implied.
        assert!(
            found[1]
                .item
                .subtitle
                .as_deref()
                .unwrap()
                .ends_with(&fixture.notes_app.to_string_lossy().to_string())
        );
    }

    #[test]
    fn a_project_with_no_history_of_its_own_falls_back_to_the_most_recent_tool_anywhere() {
        let fixture = fixture();
        let (provider, spawner) = recording(&fixture);
        let fresh = provider
            .search("task", 0)
            .into_iter()
            .find(|c| c.item.id == fixture.fresh.to_string_lossy())
            .expect("a project with no agents yet is still somewhere to start one");
        assert_eq!(
            fresh.item.subtitle.as_deref().unwrap().split(" · ").next(),
            Some("codex")
        );

        provider
            .activate_with_query(&fixture.fresh.to_string_lossy(), "task")
            .unwrap();
        assert_eq!(spawner.calls()[0].2, "codex");
    }

    #[test]
    fn a_machine_with_no_agent_history_at_all_says_so_rather_than_choosing_a_tool() {
        // `paseo run` demands a provider and there is no default to inherit —
        // so with nothing to learn from, the honest answer is "I don't know",
        // not "claude".
        let fixture = fixture();
        let empty_agents = fixture._dir.path().join("no-agents");
        std::fs::create_dir_all(&empty_agents).unwrap();
        let spawner = Arc::new(RecordingSpawner::default());
        let provider = NewAgentProvider::with_sources_and_spawner(
            fixture.projects.clone(),
            empty_agents,
            spawner.clone(),
        );

        let row = &provider.search("task", 0)[0];
        assert_eq!(row.item.action_label, NEEDS_HISTORY_LABEL);
        assert!(
            !row.item.subtitle.as_deref().unwrap().contains(" · "),
            "no tool to name"
        );

        let refused = provider
            .activate_with_query(&fixture.neko.to_string_lossy(), "task")
            .unwrap_err();
        assert!(
            refused.to_string().contains("no agent provider"),
            "got {refused}"
        );
        assert!(
            spawner.calls().is_empty(),
            "nothing may reach the CLI without a provider to pass it"
        );
    }

    #[test]
    fn confirming_a_row_starts_the_agent_in_that_rows_own_directory_with_the_typed_prompt() {
        let fixture = fixture();
        let (provider, spawner) = recording(&fixture);
        // Deliberately the *second* row: a bug that always used the first
        // project would pass against the first one.
        provider
            .activate_with_query(&fixture.notes_app.to_string_lossy(), "  fix the parser  ")
            .unwrap();
        assert_eq!(
            spawner.calls(),
            vec![(
                fixture.notes_app.clone(),
                "fix the parser".to_string(),
                "claude".to_string()
            )]
        );
    }

    #[test]
    fn an_empty_prompt_is_refused_rather_than_starting_an_agent_with_no_task() {
        let fixture = fixture();
        let (provider, spawner) = recording(&fixture);
        for prompt in ["", "   ", "\n\t"] {
            let refused = provider.activate_with_query(&fixture.neko.to_string_lossy(), prompt);
            assert!(
                refused.is_err(),
                "prompt {prompt:?} must not start an agent"
            );
        }
        assert!(spawner.calls().is_empty(), "nothing may reach the spawner");
    }

    #[test]
    fn a_directory_that_disappeared_between_the_search_and_the_enter_is_refused() {
        let fixture = fixture();
        let (provider, spawner) = recording(&fixture);
        let refused = provider
            .activate_with_query(&fixture.gone.to_string_lossy(), "task")
            .unwrap_err();
        assert!(refused.to_string().contains("no longer a folder"));
        assert!(spawner.calls().is_empty());
    }

    #[test]
    fn a_spawn_failure_is_reported_verbatim_rather_than_swallowed() {
        let fixture = fixture();
        let provider = provider_for_fixture(
            &fixture,
            Arc::new(RecordingSpawner::failing(
                "paseo run failed: no such workspace",
            )),
        );
        let error = provider
            .activate_with_query(&fixture.neko.to_string_lossy(), "task")
            .unwrap_err();
        assert_eq!(error.to_string(), "paseo run failed: no such workspace");
    }

    #[test]
    fn activate_without_a_query_refuses_instead_of_starting_a_taskless_agent() {
        let fixture = fixture();
        let (provider, spawner) = recording(&fixture);
        assert!(provider.activate(&fixture.neko.to_string_lossy()).is_err());
        assert!(spawner.calls().is_empty());
    }

    #[test]
    fn a_malformed_or_missing_projects_file_is_an_empty_list_not_an_error() {
        let fixture = fixture();
        let broken = fixture._dir.path().join("broken.json");
        std::fs::write(&broken, "{ not json at all").unwrap();
        let provider = NewAgentProvider::with_sources_and_spawner(
            broken,
            fixture.agents_root.clone(),
            Arc::new(RecordingSpawner::default()),
        );
        assert!(provider.search("task", 0).is_empty());

        let absent = NewAgentProvider::with_sources_and_spawner(
            fixture._dir.path().join("nope.json"),
            fixture.agents_root.clone(),
            Arc::new(RecordingSpawner::default()),
        );
        assert!(absent.search("task", 0).is_empty());
    }

    #[test]
    fn a_project_with_no_display_name_falls_back_to_its_directorys_own_name() {
        let fixture = fixture();
        let root = fixture._dir.path().join("web-app");
        std::fs::create_dir_all(&root).unwrap();
        let file = fixture._dir.path().join("unnamed.json");
        std::fs::write(
            &file,
            format!(
                r#"[{{"rootPath":"{}","updatedAt":"2026-01-01T00:00:00.000Z"}}]"#,
                root.display()
            ),
        )
        .unwrap();
        let provider = NewAgentProvider::with_sources_and_spawner(
            file,
            fixture.agents_root.clone(),
            Arc::new(RecordingSpawner::default()),
        );
        assert_eq!(provider.search("t", 0)[0].item.title, "web-app");
    }

    #[test]
    fn the_command_line_names_a_provider_runs_in_the_background_and_ends_flag_parsing_early() {
        // Asserted rather than executed: this is the exact argv the real
        // spawner builds. `--provider` is here because the real CLI refuses
        // without it, and `--` because a prompt may start with a dash.
        let argv = paseo_argv(Path::new("/tmp/project"), "-x fix the parser", "codex");
        assert_eq!(
            argv,
            vec![
                "run",
                "--background",
                "--json",
                "--provider",
                "codex",
                "--cwd",
                "/tmp/project",
                "--",
                "-x fix the parser",
            ]
        );
    }

    #[test]
    fn the_child_is_stripped_of_the_env_that_would_override_its_working_directory() {
        // The regression this pins was found live, not imagined: with
        // `PASEO_AGENT_CWD` inherited, the CLI ignored `--cwd` entirely and put
        // the agent in the *calling* agent's workspace. `Command::get_envs`
        // reports a removal as `(key, None)`, which is exactly what must be
        // true of both scoping variables.
        let command = paseo_command(
            Path::new("/bin/paseo"),
            Path::new("/tmp/project"),
            "task",
            "codex",
        );
        let removed: Vec<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_string_lossy().to_string())
            .collect();
        for scoping in AGENT_SCOPING_ENV {
            assert!(
                removed.contains(&scoping.to_string()),
                "{scoping} must not reach the CLI, got {removed:?}"
            );
        }
        assert_eq!(command.get_current_dir(), Some(Path::new("/tmp/project")));
    }

    #[test]
    fn the_agent_id_is_read_from_paseos_own_json_and_a_shape_change_is_not_a_failure() {
        assert_eq!(
            parse_agent_id(r#"{"id":"abc-123"}"#),
            Some("abc-123".to_string())
        );
        assert_eq!(
            parse_agent_id(r#"{"agentId":"abc-123"}"#),
            Some("abc-123".to_string())
        );
        assert_eq!(
            parse_agent_id(r#"{"agent":{"id":"abc-123"}}"#),
            Some("abc-123".to_string())
        );
        assert_eq!(parse_agent_id("not json"), None);
        assert_eq!(parse_agent_id(r#"{"created":true}"#), None);
    }

    #[test]
    fn a_failure_message_reads_the_clis_own_json_error_off_stdout() {
        // The real shape, copied from a real failing run: exit 1, empty
        // stderr, the whole error on stdout. Reading stderr alone gave
        // "exited with status 1", which is why this exists.
        let real = r#"{"error":{"code":"MISSING_PROVIDER","message":"Provider is required","details":"Pass --provider <provider>"}}"#;
        assert_eq!(
            describe_failure(Some(1), real, ""),
            "paseo run failed: Provider is required — Pass --provider <provider>"
        );
        assert_eq!(
            describe_failure(Some(1), "", "boom\n\n"),
            "paseo run failed: boom"
        );
        assert_eq!(
            describe_failure(Some(2), "   ", "   "),
            "paseo run exited with status 2"
        );
        assert_eq!(
            describe_failure(None, "", ""),
            "paseo run was terminated by a signal"
        );
        let long = "x".repeat(MAX_ERROR_CHARS * 2);
        assert!(
            describe_failure(Some(1), "", &long).chars().count()
                <= MAX_ERROR_CHARS + "paseo run failed: ".len()
        );
    }

    #[test]
    fn an_unconfirmed_run_claims_neither_outcome() {
        // Pinned as text because the wording is the whole point: it must not
        // say the agent failed to start, since it may well have.
        let message = unconfirmed_message(Duration::from_secs(20));
        assert!(message.contains("may still be starting"), "got {message:?}");
        assert!(!message.contains("failed"), "got {message:?}");
    }

    #[test]
    fn a_missing_paseo_executable_is_reported_as_such_rather_than_as_a_generic_failure() {
        // Hermetic: with no executable there is nothing to run, so this
        // exercises the real `spawn` without any process being created.
        let spawner = PaseoCli {
            executable: None,
            timeout: Duration::from_millis(10),
        };
        let error = spawner
            .spawn(Path::new("/tmp"), "task", "codex")
            .unwrap_err();
        assert!(error.contains("paseo CLI was not found"), "got {error:?}");
    }
}

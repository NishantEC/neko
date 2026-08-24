//! Coding agents running on this machine.
//!
//! **Source of truth is Paseo's own on-disk state**, `~/.paseo/agents/
//! <workspace>/<uuid>.json` — one plain JSON document per agent, which is
//! everything this provider needs and nothing it has to ask permission for.
//! Chosen over the two alternatives after checking all three against the
//! same machine:
//!
//! - **Paseo's MCP API under-reported.** It returned one running agent where
//!   both the disk and `ps` said two. A source that disagrees with the
//!   process table about what is running is not the one to build on.
//! - **The `paseo` CLI would mean a subprocess per search.** `apps.rs` and
//!   `files.rs` already pay that cost for `mdfind` because Spotlight has no
//!   in-process API; here the data is a file, so there is nothing to buy.
//!
//! **Every live agent on the verification machine was Paseo-hosted** — both
//! running `claude` processes had `Paseo Daemon` as their parent and no
//! controlling terminal at all. An agent started by hand in a terminal is a
//! real possibility and is *not* covered here; see [`Backend`] for the
//! seam it plugs into, and `AGENTS.md` for why building terminal-tab
//! focusing before a single real instance existed would have been guesswork.
//!
//! **Transcript sources (`~/.codex/sessions`, `~/.claude/projects`,
//! `~/.grok/sessions`) are deliberately not read here.** They answer "what
//! did an agent do", not "what is running now", and `jazzyalex/agent-sessions`
//! — which does read all thirteen — prices each one at roughly a thousand
//! lines of source-specific parsing in its own `docs/adding-a-session-source.md`.
//! That is its own task, and it is a history feature rather than a live one.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};
use serde::Deserialize;

use crate::provider::{Provider, ProviderError};
use crate::search::{Candidate, fuzzy_score};

/// Where agents are read from.
///
/// **One backend today, and the seam is here because it was asked for, not
/// imagined**: Paseo is what this machine runs now, with an explicit "I may
/// change to something else later". A second backend adds a variant, a
/// `read_*` function, and one arm in `read_agents` — it does not touch
/// the provider, the wire protocol, or the client.
///
/// It is deliberately *not* a plugin system. Each backend reads a different
/// tool's own on-disk format; there is nothing generic to abstract until a
/// second one exists to compare against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Managed by the Paseo daemon: `~/.paseo/agents/<workspace>/<id>.json`,
    /// opened with a `paseo:` deep link.
    Paseo,
}

impl Backend {
    pub const ALL: &'static [Backend] = &[Backend::Paseo];

    pub fn id(self) -> &'static str {
        match self {
            Backend::Paseo => "paseo",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Backend::Paseo => "Paseo",
        }
    }

    /// Where this backend keeps its state, for the Preferences tab to show.
    /// Displaying it is the point: a source of truth a person cannot see is
    /// one they cannot debug when the list looks wrong.
    pub fn root(self) -> Option<PathBuf> {
        match self {
            Backend::Paseo => agents_root(),
        }
    }
}

/// Persisted settings, daemon-owned, same KV table as every other setting.
pub const AGENTS_ENABLED_KEY: &str = "agents_enabled";
pub const AGENTS_INCLUDE_IDLE_KEY: &str = "agents_include_idle";

/// Defaults **on**: a running agent is exactly the kind of thing worth
/// seeing without being asked for, and an install with no agents at all
/// simply returns nothing.
pub fn agents_enabled(db: &crate::Db) -> bool {
    db.get_setting(AGENTS_ENABLED_KEY).ok().flatten().as_deref() != Some("false")
}

/// Defaults **off**. Idle agents outnumber running ones roughly twenty-five
/// to one on a real machine (53 idle, 2 running), so including them by
/// default would make a query for a live agent worse, not better.
pub fn include_idle(db: &crate::Db) -> bool {
    db.get_setting(AGENTS_INCLUDE_IDLE_KEY).ok().flatten().as_deref() == Some("true")
}

pub fn set_agents_enabled(db: &crate::Db, enabled: bool) -> rusqlite::Result<()> {
    db.set_setting(AGENTS_ENABLED_KEY, if enabled { "true" } else { "false" })
}

pub fn set_include_idle(db: &crate::Db, include: bool) -> rusqlite::Result<()> {
    db.set_setting(AGENTS_INCLUDE_IDLE_KEY, if include { "true" } else { "false" })
}

/// How many agents each backend can currently see, for the Preferences tab.
/// Reported as (running, idle) so the tab can say something true and
/// specific rather than "configured".
pub fn backend_census(backend: Backend) -> (usize, usize) {
    let Some(root) = backend.root() else { return (0, 0) };
    let agents = read_agents(&root);
    let running = agents.iter().filter(|a| a.is_running()).count();
    (running, agents.len() - running)
}

/// How many agents the client's grid can show. The provider caps an empty
/// query at this so the resting panel is exactly the grid and nothing else —
/// a fifth agent would arrive as an ordinary row underneath, which is not
/// what the grid is for.
pub const GRID_CAPACITY: usize = 4;

/// A running agent outranks everything else this provider can return, by a
/// margin no recency bonus can close. "What is running right now" is the
/// question being asked; an idle agent is context, not an answer.
const RUNNING_BONUS: f32 = 6.0;
/// An agent whose session is over is never a result. They outnumber the live
/// ones roughly eighty to one on a real machine (227 files, 2 running), so
/// including them would bury the thing being looked for.
const STATUS_CLOSED: &str = "closed";
const STATUS_RUNNING: &str = "running";

/// The subset of Paseo's agent document this provider reads.
///
/// Deliberately partial: `serde` ignores unknown fields by default, so
/// Paseo adding or renaming anything outside this set cannot break neko. The
/// fields here are the ones with a visible job in a row.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaseoAgent {
    id: String,
    #[serde(default)]
    title: Option<String>,
    workspace_id: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    last_status: Option<String>,
    #[serde(default)]
    last_activity_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
    /// Paseo's own marker for agents it runs for itself. Never shown.
    #[serde(default)]
    internal: bool,
    #[serde(default)]
    requires_attention: bool,
    /// `config.model`, e.g. `"claude-opus-5"`. The nested shape is Paseo's;
    /// only the one field is read, so anything else it adds there is ignored.
    #[serde(default)]
    config: AgentConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AgentConfig {
    #[serde(default)]
    model: Option<String>,
}

impl PaseoAgent {
    fn is_running(&self) -> bool {
        self.last_status.as_deref() == Some(STATUS_RUNNING)
    }

    fn is_closed(&self) -> bool {
        self.last_status.as_deref() == Some(STATUS_CLOSED)
    }

    /// What the row is called: the **workspace** — the branch or worktree
    /// this session runs in. See [`WorkspaceName`] for why not the title.
    ///
    /// Falls back to the working directory's own name, which is how a
    /// person thinks about "the agent in neko" anyway, and only then to the
    /// first prompt.
    fn display_title(&self, names: &HashMap<String, WorkspaceName>) -> String {
        if let Some(workspace) = self.names(names).workspace {
            return workspace;
        }
        self.cwd
            .as_deref()
            .and_then(|cwd| Path::new(cwd).file_name().map(|n| n.to_string_lossy().to_string()))
            .or_else(|| self.prompt())
            .unwrap_or_else(|| "Agent".to_string())
    }

    /// **The project, not the model and no longer the raw path.** The model
    /// moved to its own mark in the corner of the tile
    /// (`SearchItem::source` carries it); the path said
    /// `~/Documents/tcc/triage-fe` where Paseo itself says
    /// `Care-Connect-AI/triage-fe`, which is what the repository is called
    /// everywhere else a person sees it.
    ///
    /// **A line that repeats the title is spent on nothing.** Paseo names a
    /// `kind: "directory"` workspace after its folder and gives it a
    /// project of the same name — no branch, no repository slug — so two
    /// agents in `~/Documents/hme` both resolved to `hme` over `hme` and
    /// were indistinguishable from each other. The lookup succeeded; it
    /// just had nothing to say. The prompt is the only field left that
    /// differs between two sessions in one directory, so it takes the line
    /// in exactly that case — and only that case, since where a repository
    /// name is real it beats a prompt every time.
    fn subtitle(&self, names: &HashMap<String, WorkspaceName>) -> Option<String> {
        let path = || self.cwd.as_deref().map(tildify);
        match self.names(names).project {
            Some(project) if project != self.display_title(names) => Some(project),
            // The repository resolved and repeats the title, so it has
            // nothing to add — but the path has nothing to add either,
            // since the title is that directory's own name. Only the prompt
            // is left.
            Some(_) => self.prompt().or_else(path),
            // Nothing resolved at all: the title is already the directory's
            // name or the prompt, and where it is running beats repeating
            // either.
            None => path().or_else(|| self.prompt()),
        }
    }

    fn names(&self, names: &HashMap<String, WorkspaceName>) -> WorkspaceName {
        self.workspace_id.as_deref().and_then(|id| names.get(id)).cloned().unwrap_or_default()
    }

    /// The first prompt, tidied — searchable, never shown. See
    /// [`WorkspaceName`].
    fn prompt(&self) -> Option<String> {
        self.title.as_deref().map(title_from_prompt).filter(|t| !t.is_empty())
    }

    /// Which tool is running the agent — `"claude"`, `"codex"` — for the
    /// small badge the client overlays on the host app's icon.
    ///
    /// The *model* is deliberately not this: a badge that sits on the corner
    /// of a 22px icon has room for about one character, and "which tool" is
    /// the distinction that survives being reduced to one. The model is still
    /// searchable through this provider's own `search`.
    fn provider_mark(&self) -> Option<String> {
        let provider = self.provider.as_deref().map(short_provider).filter(|p| !p.is_empty());
        provider.or_else(|| {
            // No `provider` field, but the model usually names its family.
            let model = self.config.model.as_deref()?;
            model.split(['-', '/']).next().map(str::to_string).filter(|p| !p.is_empty())
        })
    }

    fn activity_at(&self) -> Option<&str> {
        self.last_activity_at.as_deref().or(self.updated_at.as_deref())
    }
}

/// Paseo records the model as `"claude/claude-opus-5"` in some views and
/// `"claude"` in others; the row wants the family, not the exact build.
fn short_provider(raw: &str) -> String {
    raw.split('/').next().unwrap_or(raw).to_string()
}

/// Turns a raw first prompt into something title-shaped.
///
/// A title is one row tall, so a pasted multi-line task is cut to its first
/// line — the part that identifies it — before anything else. Then the
/// leading decoration people open a message with (`>` quotes, `#` headings,
/// list bullets, the `\u{258e}` bar a quoted block starts with, stray
/// backticks and quote marks) comes off, because none of it says anything
/// about the task and all of it eats the front of a 150px tile.
///
/// A prompt that is *nothing but* a URL — a pull request, a Figma file — is
/// the common case this exists for, and truncating it raw is the worst
/// possible cut: `https://github.co…` spends the whole tile on the scheme
/// and the host, the two parts every such link shares. [`compact_url`]
/// keeps the identifying end instead.
fn title_from_prompt(raw: &str) -> String {
    let line = raw.lines().next().unwrap_or(raw).trim();
    let line = line.trim_start_matches(|c: char| {
        matches!(c, '\u{258e}' | '>' | '#' | '-' | '*' | '`' | '"' | '\'' | ' ' | '\t')
    });
    if let Some(compact) = compact_url(line) {
        return compact;
    }
    // Collapse the runs a pasted prompt leaves behind, so one line of it
    // does not render as a title with a hole in the middle.
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `https://github.com/HealthifyMe/athena/pull/4501` → `github.com/…/pull/4501`.
///
/// `None` unless the whole string is one URL: a prompt that merely mentions
/// a link is still a sentence, and rewriting the link inside it would be
/// rewriting what was typed.
///
/// Two rules, both about spending a narrow tile on the parts that identify
/// the thing. **Opaque segments are dropped** — Figma and Notion put a
/// 22-character key in the middle of every path, and it is the least
/// informative run of characters in the URL. **Only the last two segments
/// are kept**, elided with `…`, because the tail is what names the specific
/// page (`pull/4501`, `Care-Comms`) while the head repeats across every
/// link from the same place.
fn compact_url(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("https://").or_else(|| raw.strip_prefix("http://"))?;
    if rest.split_whitespace().count() != 1 || rest.is_empty() {
        return None;
    }
    // Query strings and fragments are routing, never a name.
    let rest = rest.split(['?', '#']).next().unwrap_or(rest).trim_end_matches('/');
    let mut parts = rest.split('/');
    let host = parts.next()?.trim_start_matches("www.");
    if host.is_empty() {
        return None;
    }
    let segments: Vec<&str> = parts.filter(|s| !s.is_empty() && !is_opaque_id(s)).collect();
    Some(match segments.len() {
        0 => host.to_string(),
        n if n <= 2 => format!("{host}/{}", segments.join("/")),
        n => format!("{host}/\u{2026}/{}", segments[n - 2..].join("/")),
    })
}

/// A long run of letters and digits with no word structure — a Figma file
/// key, a Notion page id. Length alone would catch real words, and digits
/// alone would catch a PR number, so it takes both.
fn is_opaque_id(segment: &str) -> bool {
    segment.len() >= 16
        && segment.chars().all(|c| c.is_ascii_alphanumeric())
        && segment.chars().any(|c| c.is_ascii_digit())
        && segment.chars().any(|c| c.is_ascii_alphabetic())
}

/// `/Users/x/Documents/neko` → `~/Documents/neko`. Public because
/// `crate::terminals` shows the same paths and there should be one rule for
/// how a home directory is written, not two that can drift.
pub fn tildify_path(path: &str) -> String {
    tildify(path)
}

fn tildify(path: &str) -> String {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return path.to_string();
    };
    let home = home.to_string_lossy().to_string();
    match path.strip_prefix(&home) {
        Some(rest) => format!("~{rest}"),
        None => path.to_string(),
    }
}

/// `~/.paseo/agents`. Not configurable: it is Paseo's own layout, not a neko
/// setting, and a wrong value would silently mean "no agents" rather than an
/// error worth surfacing.
fn agents_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".paseo/agents"))
}

/// Reads every agent document under `root`, skipping anything unreadable or
/// unparseable rather than failing the search.
///
/// **A malformed file is skipped silently and deliberately.** These are
/// another application's files, written by a process neko does not control
/// and may be mid-write; one bad document must never take out the whole
/// list, and there is nothing a person could do about it if it were
/// reported.
/// What Paseo calls a session and where it lives.
///
/// Paseo keeps three names per agent and they are not interchangeable. The
/// **title** is the first prompt, verbatim and never updated — `"hi"` on a
/// session with 872 messages since. The **workspace** `displayName` is the
/// branch or worktree the session runs in (`feat/doctors-maps`, `main`).
/// The **project** `displayName` is the repository
/// (`Care-Connect-AI/triage-fe`). Only the last two actually distinguish
/// one session from another — three of this machine's agents share the
/// prompt "what's the update on the agents tasks" — which is why the tile
/// leads with them and the prompt stays searchable rather than shown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkspaceName {
    pub workspace: Option<String>,
    pub project: Option<String>,
}

/// `workspaceId` → its names, read from `~/.paseo/projects/`.
///
/// Takes the **agents** root and steps up, because `projects/` is that
/// directory's sibling rather than its child — `~/.paseo/agents` and
/// `~/.paseo/projects`. Getting that wrong is silent: every lookup misses
/// and every tile quietly falls back to its path, which looks like a
/// deliberate design rather than a bug.
///
/// Two small files (a 95KB workspace list, a 3KB project list) read once
/// per search, against the couple of hundred agent documents `read_agents`
/// is already opening on the same call — not worth a cache with an
/// invalidation story attached.
pub fn read_workspace_names(agents_root: &Path) -> HashMap<String, WorkspaceName> {
    let Some(home) = agents_root.parent() else {
        return HashMap::new();
    };
    let list = |name: &str| -> Vec<serde_json::Value> {
        std::fs::read_to_string(home.join("projects").join(name))
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<serde_json::Value>>(&raw).ok())
            .unwrap_or_default()
    };
    let name_of = |v: &serde_json::Value| -> Option<String> {
        v.get("displayName")?.as_str().map(str::to_string).filter(|s| !s.is_empty())
    };
    let projects: HashMap<String, String> = list("projects.json")
        .iter()
        .filter_map(|p| Some((p.get("projectId")?.as_str()?.to_string(), name_of(p)?)))
        .collect();
    list("workspaces.json")
        .iter()
        .filter_map(|w| {
            let id = w.get("workspaceId")?.as_str()?.to_string();
            let project = w
                .get("projectId")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| projects.get(id).cloned());
            Some((id, WorkspaceName { workspace: name_of(w), project }))
        })
        .collect()
}

fn read_agents(root: &Path) -> Vec<PaseoAgent> {
    read_agent_documents(root).into_iter().filter(|agent| !agent.is_closed()).collect()
}

/// Every non-internal agent document under `root`, **closed ones included**.
///
/// Split out from [`read_agents`] for [`provider_usage`], which wants exactly
/// what this provider does not: a *finished* agent is the best evidence there
/// is of which tool the captain actually uses in a given directory, and on the
/// verification machine 172 of 227 documents were closed. The malformed-file
/// and unreadable-directory rules above apply here unchanged.
fn read_agent_documents(root: &Path) -> Vec<PaseoAgent> {
    let Ok(workspaces) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut agents = Vec::new();
    for workspace in workspaces.flatten() {
        let Ok(entries) = std::fs::read_dir(workspace.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&path) else { continue };
            let Ok(agent) = serde_json::from_str::<PaseoAgent>(&raw) else { continue };
            if agent.internal {
                continue;
            }
            agents.push(agent);
        }
    }
    agents
}

/// One past agent, reduced to "which tool, working where, when".
///
/// This is the whole of what [`crate::new_agent`] needs from this module, and
/// it is deliberately not the `PaseoAgent` document itself: the two modules
/// share a *fact about the machine*, not a parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUse {
    /// Paseo's own provider id — `"claude"`, `"codex"`, … — already reduced
    /// from a `"claude/claude-opus-5"` style value by [`short_provider`].
    pub provider: String,
    pub cwd: PathBuf,
    /// RFC 3339, compared as a string: see [`ProviderUse`]'s only consumer and
    /// `new_agent::read_projects` for why that is chronological here.
    pub at: String,
}

/// Which agent providers have really been used, where, most recent first.
///
/// Exists because **`paseo run` requires an explicit `--provider`** — verified
/// live against the real CLI, which answers `MISSING_PROVIDER` without one, and
/// there is no default anywhere in `~/.paseo/config.json` to fall back on. So
/// `new_agent` has to name a tool, and the only honest way to name one without
/// hard-coding a choice this app has no business making is to use the one
/// already in use. Closed agents count (see [`read_agent_documents`]).
pub fn provider_usage(root: &Path) -> Vec<ProviderUse> {
    let mut uses: Vec<ProviderUse> = read_agent_documents(root)
        .into_iter()
        .filter_map(|agent| {
            let provider = agent.provider.as_deref().map(short_provider).filter(|p| !p.is_empty())?;
            let cwd = agent.cwd.as_deref().map(PathBuf::from)?;
            let at = agent.activity_at().unwrap_or_default().to_string();
            Some(ProviderUse { provider, cwd, at })
        })
        .collect();
    uses.sort_by(|a, b| b.at.cmp(&a.at));
    uses
}

pub struct AgentsProvider {
    root: Option<PathBuf>,
    /// `None` in tests that only exercise reading and ranking; the settings
    /// then take their documented defaults.
    db: Option<Arc<Mutex<crate::Db>>>,
}

impl AgentsProvider {
    pub fn new(db: Arc<Mutex<crate::Db>>) -> Self {
        Self { root: agents_root(), db: Some(db) }
    }

    /// A provider that reads a specific directory — for tests, which must
    /// never depend on whatever agents happen to exist on the machine
    /// running the suite.
    pub fn with_root(root: PathBuf) -> Self {
        Self { root: Some(root), db: None }
    }

    /// Settings are re-read per search rather than cached, so a toggle in
    /// Preferences takes effect on the next keystroke — the same choice, for
    /// the same reason, as `files::FileProvider`'s configured scope.
    fn settings(&self) -> (bool, bool) {
        let Some(db) = &self.db else { return (true, false) };
        let db = db.lock().unwrap();
        (agents_enabled(&db), include_idle(&db))
    }
}

impl Provider for AgentsProvider {
    fn id(&self) -> &'static str {
        "agent"
    }

    fn section_label(&self) -> &'static str {
        "Agents"
    }

    /// **An empty root query returns the running agents and nothing else.**
    /// Unlike themes or preferences — which answer nothing until asked —
    /// "what is running right now" is exactly the kind of thing worth seeing
    /// the moment the panel opens, and it is self-limiting: two rows on a
    /// real machine, not two hundred. Idle agents need a query, because they
    /// are context rather than news.
    fn answers_empty_root_query(&self) -> bool {
        true
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        let (enabled, include_idle) = self.settings();
        if !enabled {
            return Vec::new();
        }
        let trimmed = query.trim();
        let agents = read_agents(root);
        let names = read_workspace_names(root);
        let mut scored: Vec<(String, bool, Candidate)> = agents
            .into_iter()
            .filter_map(|agent| {
                let running = agent.is_running();
                let score = if trimmed.is_empty() {
                    // The grid's own list: running first, then the most
                    // recent, capped below. Idle agents are included here —
                    // unlike a *query*, where they stay behind the
                    // `include_idle` setting — because "what were you just
                    // working on" is the question the resting panel answers.
                    if running { RUNNING_BONUS } else { 0.0 }
                } else if !running && !include_idle {
                    return None;
                } else {
                    // Matched against the same three things the row shows,
                    // best wins — a person looks for an agent by what it is
                    // called, by what it is, or by where it is working, and
                    // has no reason to know which one they are using.
                    // The prompt is in here but never rendered: it is how a
                    // person remembers what they asked for, even though it
                    // does not distinguish two sessions on screen.
                    let haystacks = [
                        Some(agent.display_title(&names)),
                        agent.subtitle(&names),
                        agent.prompt(),
                        agent.provider.as_deref().map(short_provider),
                        agent.cwd.as_deref().map(tildify),
                        Some("agent".to_string()),
                    ];
                    let best = haystacks
                        .into_iter()
                        .flatten()
                        .filter_map(|hay| fuzzy_score(trimmed, &hay))
                        .fold(None, |best: Option<f32>, s| Some(best.map_or(s, |b| b.max(s))))?;
                    if running { best + RUNNING_BONUS } else { best }
                };
                Some((agent.activity_at().unwrap_or("").to_string(), running, Candidate { score, item: to_item(&agent, running, &names) }))
            })
            .collect::<Vec<_>>();

        if !trimmed.is_empty() {
            return scored.into_iter().map(|(_, _, candidate)| candidate).collect();
        }

        // The grid's own ordering: every running agent first, then the rest
        // by most recent activity. Sorted here rather than left to
        // `search::allocate`, which ranks by score alone and has no reason to
        // know that two agents with the same score are ordered by *time*.
        //
        // Timestamps are RFC 3339 from Paseo, which sort lexicographically in
        // chronological order — so comparing the strings is correct and needs
        // no date parsing (this codebase has declined a date/time dependency
        // more than once; see `to_item`'s own note on `short_time`).
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        scored.truncate(GRID_CAPACITY);
        // `allocate` orders rows by score, so the order settled above has to
        // survive as descending scores rather than as position in this Vec.
        scored
            .into_iter()
            .enumerate()
            .map(|(rank, (_, _, mut candidate))| {
                candidate.score = RUNNING_BONUS + (GRID_CAPACITY - rank) as f32;
                candidate
            })
            .collect()
    }

    /// Cancel and Archive, over `neko_core::mcp`.
    ///
    /// Paseo's daemon is the only thing that can do either — an agent is a
    /// process it supervises, not a file on disk — so unlike this provider's
    /// *reads*, which come from `~/.paseo/agents`, these have a hard
    /// dependency on the daemon being up. That asymmetry is deliberate and is
    /// why the read path was not rewritten onto MCP as well: seeing your
    /// agents should not stop working because a daemon restarted.
    fn perform_action(&self, id: &str, action: &str) -> Result<(), ProviderError> {
        let tool = match action {
            "cancel" => "cancel_agent",
            "archive" => "archive_agent",
            other => return Err(ProviderError(format!("no such action: {other}"))),
        };
        let client =
            crate::mcp::McpClient::discover().map_err(|e| ProviderError(e.to_string()))?;
        client
            .call(tool, serde_json::json!({ "agentId": id }))
            .map(|_| ())
            .map_err(|e| ProviderError(e.to_string()))
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        let Some(server) = server_id() else {
            return Err(ProviderError(
                "couldn't find ~/.paseo/server-id — is Paseo running?".to_string(),
            ));
        };
        crate::launch::open_url(&deep_link(&server, id)).map_err(|e| ProviderError(e.to_string()))
    }
}

/// Paseo's own deep-link shape: `paseo://h/<serverId>/agent/<agentId>`.
///
/// **Two slashes, and that is the whole bug this had.** Paseo's own
/// `buildAgentDeepLink` writes `` `paseo:/${route}` `` — one slash — and
/// copying it produced a URL its *own* `parseAgentDeepLink` rejects, because
/// that function tests `url.hostname === "h"`. With one slash `h` parses as
/// the first path segment and the hostname is empty, so the link resolved to
/// nothing and Paseo merely came to the front. Verified against the real
/// parse rules rather than assumed:
///
/// ```text
/// paseo:/h/srv/agent/ID    hostname ""   -> rejected
/// paseo://h/srv/agent/ID   hostname "h"  -> accepted
/// ```
///
/// **The server id is per-machine and must be read, not assumed.** A first
/// version of this guessed `local`, on the strength of `=== "local"`
/// comparisons elsewhere in Paseo's bundle, and was simply wrong: the real
/// value on the verification machine is `srv_Rj6twNQn7Qcc`, reported by
/// `paseo status --json` and stored verbatim in `~/.paseo/server-id`. That
/// file is read rather than shelling out to the CLI, for the same reason
/// this module reads agent JSON rather than running `paseo ls` — the data is
/// a file, so a subprocess buys nothing.
///
/// Returns `None` when the file is missing, rather than falling back to a
/// guess: a deep link with the wrong server id fails *silently* (Paseo opens
/// on nothing in particular), and a row that reports an honest error is
/// better than one that appears to work.
pub fn server_id() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let raw = std::fs::read_to_string(PathBuf::from(home).join(".paseo/server-id")).ok()?;
    let trimmed = raw.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn deep_link(server_id: &str, agent_id: &str) -> String {
    format!("paseo://h/{server_id}/agent/{agent_id}")
}

/// The host app's icon, cached once per process. `ensure_cached_icon` is real
/// AppKit work (`icons.rs`: "tens of ms each"), and this would otherwise run
/// once per agent per keystroke.
fn host_icon() -> Option<&'static PathBuf> {
    static ICON: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        let app = PathBuf::from("/Applications/Paseo.app");
        app.is_dir().then(|| crate::icons::ensure_cached_icon("com.paseo.app", &app))?
    })
    .as_ref()
}

/// What the `⌘K` menu offers on an agent row.
///
/// **Cancel is not destructive and Archive is.** Cancelling stops the
/// current run and leaves the agent to be prompted again — the everyday
/// "stop, that's wrong" — and putting it behind a second Enter would make
/// the recovery slower than the mistake it is recovering from. Archiving
/// interrupts *and* soft-deletes, which is the one a mis-key must not do
/// silently.
///
/// A closed agent has no run to cancel and nothing to interrupt, so it is
/// offered neither: a menu that lists what it cannot do is worse than a
/// shorter menu.
fn agent_actions(running: bool) -> Vec<ItemAction> {
    if !running {
        return Vec::new();
    }
    vec![
        ItemAction { id: "cancel".to_string(), label: "Cancel run".to_string(), destructive: false },
        ItemAction { id: "archive".to_string(), label: "Archive".to_string(), destructive: true },
    ]
}

fn to_item(agent: &PaseoAgent, running: bool, names: &HashMap<String, WorkspaceName>) -> SearchItem {
    SearchItem {
        id: agent.id.clone(),
        kind: "agent".to_string(),
        title: agent.display_title(names),
        subtitle: agent.subtitle(names),
        // The host application's own icon (Paseo's), extracted through the
        // same cache every app row uses — no second icon pipeline. Falls back
        // to the painted glyph when the app is not installed where expected,
        // which is also what a future non-Paseo backend gets for free.
        icon: host_icon().map_or(
            Icon::Glyph(if running { Glyph::AgentLive } else { Glyph::Agent }),
            |path| Icon::Image(path.display().to_string()),
        ),
        section_label: "Agents".to_string(),
        action_label: "Open in Paseo  ↵".to_string(),
        // The badge is what the client keys its live treatment off — the row
        // says what it is, the client decides how that looks, the same
        // "provider describes it" rule every other field follows.
        badge: running.then(|| "LIVE".to_string()),
        accessory: agent
            .requires_attention
            .then(|| "Needs you".to_string())
            .or_else(|| agent.activity_at().map(short_time)),
        enters_mode: None,
        group_label: None,
        // **L1 of `docs/plan-agent-control-plane.md`.** A running agent can
        // be stopped or thrown away without leaving the panel — the two
        // things a person actually does to an agent that is misbehaving.
        // Both go over `neko_core::mcp`.
        actions: agent_actions(running),
        // The tool running this agent, for the badge the client overlays on
        // the host icon. `source` is the wire's "bare value for a labelled
        // field" slot; how it renders is the client's business.
        source: agent.provider_mark(),
        meter: None,
        keeps_open: false,
        preview: None,
    }
}

/// `"2026-08-23T01:48:18.912Z"` → `"01:48"`. Deliberately not a relative
/// time: computing one needs the current instant, and `Provider::search`'s
/// `now_unix_ms` is milliseconds while these are RFC 3339 strings — parsing
/// them properly would mean a date/time dependency this codebase has so far
/// declined to take on for one label (see `AGENTS.md`, "Commands and modes",
/// on the same trade for clipboard timestamps).
fn short_time(timestamp: &str) -> String {
    timestamp
        .split('T')
        .nth(1)
        .and_then(|time| time.get(0..5))
        .map(str::to_string)
        .unwrap_or_else(|| timestamp.to_string())
}

// ------------------------------------------------------- the Agents mode

/// How long the per-provider mode list stays good. It is configuration, not
/// state — it changes when somebody edits `~/.paseo/config.json`, not while
/// you are looking at a list.
const PROVIDER_MODES_TTL: Duration = Duration::from_secs(120);

type ProviderModes = HashMap<String, Vec<(String, String)>>;

static MODES: Mutex<Option<(Instant, ProviderModes)>> = Mutex::new(None);

/// Each provider's session modes, as `(id, label)` — `plan` / "Plan Mode",
/// `bypassPermissions` / "Bypass", and so on.
///
/// Read from the daemon rather than hard-coded: the modes belong to the
/// provider, so a Codex agent and a Claude agent genuinely offer different
/// ones, and a compiled-in list would be wrong for whichever provider was
/// not used to write it.
pub fn provider_modes(client: &crate::mcp::McpClient) -> ProviderModes {
    if let Some((at, cached)) = MODES.lock().unwrap().as_ref()
        && at.elapsed() < PROVIDER_MODES_TTL
    {
        return cached.clone();
    }
    let found = client
        .call("list_providers", serde_json::json!({}))
        .map(|value| parse_provider_modes(&value))
        .unwrap_or_default();
    // **A failure is not cached.** `list_providers` failing — the daemon
    // restarting mid-keystroke is the ordinary way — yields an empty map,
    // and caching that for two minutes would strip every mode entry out of
    // the `⌘K` menu long after Paseo came back. Configuration is worth
    // caching; the absence of it is not.
    if !found.is_empty() {
        *MODES.lock().unwrap() = Some((Instant::now(), found.clone()));
    }
    found
}

/// Reads `{"providers": [{id, modes: [{id, label}]}]}`.
pub fn parse_provider_modes(value: &serde_json::Value) -> ProviderModes {
    let mut found: ProviderModes = HashMap::new();
    let Some(providers) = value.get("providers").and_then(serde_json::Value::as_array) else {
        return found;
    };
    for provider in providers {
        let Some(id) = provider.get("id").and_then(serde_json::Value::as_str) else { continue };
        let modes: Vec<(String, String)> = provider
            .get("modes")
            .and_then(serde_json::Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|mode| {
                        let mode_id = mode.get("id")?.as_str()?.to_string();
                        let label = mode
                            .get("label")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(&mode_id)
                            .to_string();
                        Some((mode_id, label))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // A provider with no modes contributes no menu entries at all,
        // rather than an empty submenu.
        if !modes.is_empty() {
            found.insert(id.to_string(), modes);
        }
    }
    found
}

/// The arguments for `send_agent_prompt`.
///
/// Split out from the call so the one property that matters can be tested
/// without a network round trip or a real agent: **`background` is always
/// `true`.** Paseo derives it as `Boolean(callerAgentId)`
/// (`paseo-tools.ts:1881`) and neko is a top-level caller with no agent id,
/// so the default is `false` — and `false` makes the tool `await
/// waitForAgentWithTimeout` (`:1906`), holding the request until the agent
/// finishes, which can be minutes. Read out of Paseo's own source rather
/// than discovered by hanging.
pub fn send_prompt_arguments(agent_id: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({ "agentId": agent_id, "prompt": prompt, "background": true })
}

/// The `agent` mode's list: every agent, with the keyboard pointed at the
/// thing you actually came to do — say something else to it.
///
/// Separate from [`AgentsProvider`] rather than a flag on it, because Enter
/// means something different here. In the root list an agent row opens the
/// session in Paseo; here it *sends a prompt*, and one provider cannot have
/// two meanings for the primary action.
pub struct AgentControlProvider {
    root: Option<PathBuf>,
}

impl AgentControlProvider {
    pub fn new() -> Self {
        Self { root: agents_root() }
    }

    pub fn with_root(root: PathBuf) -> Self {
        Self { root: Some(root) }
    }

    fn client(&self) -> Result<crate::mcp::McpClient, ProviderError> {
        crate::mcp::McpClient::discover().map_err(|e| ProviderError(e.to_string()))
    }
}

impl Default for AgentControlProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for AgentControlProvider {
    fn id(&self) -> &'static str {
        "agent-control"
    }

    fn section_label(&self) -> &'static str {
        "Agents"
    }

    /// **The query is ignored on purpose** — it is the prompt to send, not a
    /// filter. Filtering on it would make the list shrink as you described
    /// the task, and the row you were aiming at would move out from under
    /// the selection mid-sentence. Same rule `new_agent` follows.
    fn search(&self, _query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let Some(root) = &self.root else { return Vec::new() };
        let names = read_workspace_names(root);
        let modes = self.client().map(|c| provider_modes(&c)).unwrap_or_default();

        let mut agents: Vec<PaseoAgent> =
            read_agents(root).into_iter().filter(|a| !a.is_closed()).collect();
        // Running first, then most recent — the same order the grid uses, and
        // for the same reason: what is happening now outranks what happened
        // last.
        agents.sort_by(|a, b| {
            b.is_running()
                .cmp(&a.is_running())
                .then_with(|| b.activity_at().unwrap_or("").cmp(a.activity_at().unwrap_or("")))
        });

        let count = agents.len();
        agents
            .iter()
            .enumerate()
            .map(|(rank, agent)| {
                let title = agent.display_title(&names);
                let subtitle = agent.subtitle(&names);
                // **Every agent, whatever is typed.** The query is the prompt
                // to send, not a filter — the same rule `new_agent` follows,
                // and for the same reason: filtering on it would make the
                // list shrink as you described the task.
                let score = (count - rank) as f32;
                let running = agent.is_running();
                let mut actions: Vec<ItemAction> = modes
                    .get(agent.provider.as_deref().unwrap_or("claude"))
                    .map(|list| {
                        list.iter()
                            .map(|(mode_id, label)| ItemAction {
                                id: format!("mode:{mode_id}"),
                                label: format!("Set mode: {label}"),
                                destructive: false,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                actions.extend(agent_actions(running));
                Candidate {
                    score,
                    item: SearchItem {
                        id: agent.id.clone(),
                        kind: "agent-control".to_string(),
                        title,
                        subtitle,
                        icon: Icon::Glyph(if running { Glyph::AgentLive } else { Glyph::Agent }),
                        section_label: "Agents".to_string(),
                        action_label: "Send  \u{21b5}".to_string(),
                        badge: running.then(|| "LIVE".to_string()),
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions,
                        source: agent.provider_mark(),
                        meter: None,
                        keeps_open: false,
                        preview: None,
                    },
                }
            })
            // An agent with no id cannot be prompted, cancelled or archived,
            // so a row for one could only ever fail.
            .filter(|c| !c.item.id.is_empty())
            .collect()
    }

    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        Err(ProviderError("type what to send first".to_string()))
    }

    /// Enter sends the search field's contents to the agent.
    ///
    /// **`background: true`, and it is not optional.** Paseo derives that
    /// flag as `Boolean(callerAgentId)`, and neko is a top-level caller with
    /// no agent id — so the default is `false`, which makes the tool *wait
    /// for the agent to finish*. A prompt sent from the panel would hold a
    /// daemon request thread for as long as the agent worked, which can be
    /// minutes. Read out of `paseo-tools.ts` rather than discovered by
    /// hanging.
    fn activate_with_query(&self, id: &str, query: &str) -> Result<(), ProviderError> {
        let prompt = query.trim();
        if prompt.is_empty() {
            return Err(ProviderError("type what to send first".to_string()));
        }
        self.client()?
            .call("send_agent_prompt", send_prompt_arguments(id, prompt))
            .map(|_| ())
            .map_err(|e| ProviderError(e.to_string()))
    }

    fn perform_action(&self, id: &str, action: &str) -> Result<(), ProviderError> {
        if let Some(mode_id) = action.strip_prefix("mode:") {
            return self
                .client()?
                .call("set_agent_mode", serde_json::json!({ "agentId": id, "modeId": mode_id }))
                .map(|_| ())
                .map_err(|e| ProviderError(e.to_string()));
        }
        let tool = match action {
            "cancel" => "cancel_agent",
            "archive" => "archive_agent",
            other => return Err(ProviderError(format!("no such action: {other}"))),
        };
        self.client()?
            .call(tool, serde_json::json!({ "agentId": id }))
            .map(|_| ())
            .map_err(|e| ProviderError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_agent(root: &Path, workspace: &str, id: &str, body: &str) {
        let dir = root.join(workspace);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), body).unwrap();
    }

    fn fixture_root() -> (tempfile::TempDir, AgentsProvider) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        write_agent(
            &root,
            "neko",
            "run-1",
            r#"{"id":"run-1","title":"hi","provider":"claude","cwd":"/tmp/neko",
                "lastStatus":"running","lastActivityAt":"2026-08-23T01:48:18.912Z"}"#,
        );
        write_agent(
            &root,
            "other",
            "idle-1",
            r#"{"id":"idle-1","title":"fix the parser","provider":"codex","cwd":"/tmp/parser",
                "lastStatus":"idle","updatedAt":"2026-08-22T09:00:00.000Z"}"#,
        );
        write_agent(
            &root,
            "other",
            "closed-1",
            r#"{"id":"closed-1","title":"old work","provider":"claude","lastStatus":"closed"}"#,
        );
        write_agent(&root, "other", "internal-1", r#"{"id":"internal-1","lastStatus":"running","internal":true}"#);
        let provider = AgentsProvider::with_root(root);
        (dir, provider)
    }

    #[test]
    fn an_empty_query_returns_running_agents_first_then_the_most_recent() {
        let (_dir, provider) = fixture_root();
        let found = provider.search("", 0);
        // The grid's own list: live first, then what was worked on last.
        // Closed agents are still never included.
        assert_eq!(
            found.iter().map(|c| c.item.id.as_str()).collect::<Vec<_>>(),
            vec!["run-1", "idle-1"]
        );
        assert_eq!(found[0].item.badge.as_deref(), Some("LIVE"));
        assert!(found[0].score > found[1].score, "allocate orders by score, so the order must survive as one");
    }

    #[test]
    fn an_empty_query_never_returns_more_than_the_grid_can_show() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..9 {
            write_agent(
                dir.path(),
                "w",
                &format!("a{i}"),
                &format!(
                    r#"{{"id":"a{i}","title":"agent {i}","lastStatus":"idle","updatedAt":"2026-08-2{i}T00:00:00.000Z"}}"#
                ),
            );
        }
        let provider = AgentsProvider::with_root(dir.path().to_path_buf());
        let found = provider.search("", 0);
        assert_eq!(found.len(), GRID_CAPACITY);
        // Most recent first, so the cap keeps the useful end of the list.
        assert_eq!(found[0].item.id, "a8");
        assert_eq!(found[GRID_CAPACITY - 1].item.id, "a5");
    }

    #[test]
    fn a_running_agent_still_leads_even_when_an_idle_one_was_touched_more_recently() {
        let dir = tempfile::tempdir().unwrap();
        write_agent(
            dir.path(),
            "w",
            "old-running",
            r#"{"id":"old-running","title":"busy","lastStatus":"running","updatedAt":"2020-01-01T00:00:00.000Z"}"#,
        );
        write_agent(
            dir.path(),
            "w",
            "fresh-idle",
            r#"{"id":"fresh-idle","title":"just closed","lastStatus":"idle","updatedAt":"2030-01-01T00:00:00.000Z"}"#,
        );
        let provider = AgentsProvider::with_root(dir.path().to_path_buf());
        let found = provider.search("", 0);
        assert_eq!(found[0].item.id, "old-running", "running outranks recency, not the other way round");
    }

    #[test]
    fn a_closed_agent_is_never_returned_even_by_an_exact_query() {
        let (_dir, provider) = fixture_root();
        assert!(provider.search("old work", 0).iter().all(|c| c.item.id != "closed-1"));
    }

    #[test]
    fn paseos_own_internal_agents_are_never_shown() {
        let (_dir, provider) = fixture_root();
        assert!(provider.search("", 0).iter().all(|c| c.item.id != "internal-1"));
        assert!(provider.search("agent", 0).iter().all(|c| c.item.id != "internal-1"));
    }

    #[test]
    fn a_running_agent_outranks_an_idle_one_that_matches_the_query_just_as_well() {
        let (_dir, provider) = fixture_root();
        // Both match "agent" only through the shared alias, so the ordering
        // is decided purely by whether one of them is live.
        let found = provider.search("agent", 0);
        let top = found.iter().max_by(|a, b| a.score.partial_cmp(&b.score).unwrap()).unwrap();
        assert_eq!(top.item.id, "run-1");
    }

    #[test]
    fn an_agent_is_findable_by_its_working_directory_not_only_by_its_title() {
        let (_dir, provider) = fixture_root();
        // "neko" appears only in the running agent's cwd, never in its title.
        let found = provider.search("neko", 0);
        assert!(found.iter().any(|c| c.item.id == "run-1"), "cwd must be searchable");
    }

    #[test]
    fn idle_agents_are_excluded_by_default_and_included_once_the_setting_is_on() {
        let (_dir, provider) = fixture_root();
        // `with_root` carries no `Db`, so the documented defaults apply:
        // agents on, idle off.
        assert!(
            provider.search("parser", 0).is_empty(),
            "an idle agent must not match by default — they outnumber live ones heavily"
        );

        let dir = tempfile::tempdir().unwrap();
        let db = crate::Db::open_in_memory().unwrap();
        set_include_idle(&db, true).unwrap();
        write_agent(
            dir.path(),
            "w",
            "idle-2",
            r#"{"id":"idle-2","title":"fix the parser","lastStatus":"idle","cwd":"/tmp/parser"}"#,
        );
        let with_idle =
            AgentsProvider { root: Some(dir.path().to_path_buf()), db: Some(Arc::new(Mutex::new(db))) };
        assert_eq!(with_idle.search("parser", 0).len(), 1, "turning the setting on must include them");
    }

    #[test]
    fn turning_agents_off_returns_nothing_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::Db::open_in_memory().unwrap();
        set_agents_enabled(&db, false).unwrap();
        write_agent(dir.path(), "w", "a", r#"{"id":"a","title":"hi","lastStatus":"running"}"#);
        let provider =
            AgentsProvider { root: Some(dir.path().to_path_buf()), db: Some(Arc::new(Mutex::new(db))) };
        assert!(provider.search("", 0).is_empty());
        assert!(provider.search("hi", 0).is_empty());
    }

    #[test]
    fn a_malformed_document_is_skipped_without_losing_the_rest_of_the_list() {
        let (dir, provider) = fixture_root();
        let before = provider.search("", 0).len();
        write_agent(dir.path(), "broken", "bad", "{ this is not json");
        assert_eq!(provider.search("", 0).len(), before, "one unreadable file must not shrink the list");
    }

    #[test]
    fn an_untitled_agent_falls_back_to_its_working_directorys_name() {
        let dir = tempfile::tempdir().unwrap();
        write_agent(
            dir.path(),
            "w",
            "a",
            r#"{"id":"a","provider":"claude","cwd":"/Users/someone/Documents/triage-fe","lastStatus":"running"}"#,
        );
        let provider = AgentsProvider::with_root(dir.path().to_path_buf());
        assert_eq!(provider.search("", 0)[0].item.title, "triage-fe");
    }

    #[test]
    fn a_multi_line_pasted_title_is_reduced_to_its_first_line() {
        let dir = tempfile::tempdir().unwrap();
        write_agent(
            dir.path(),
            "w",
            "a",
            r#"{"id":"a","title":"first line\nsecond line\nthird","lastStatus":"running"}"#,
        );
        let provider = AgentsProvider::with_root(dir.path().to_path_buf());
        assert_eq!(provider.search("", 0)[0].item.title, "first line", "a row is one line tall");
    }

    #[test]
    fn the_badge_names_the_tool_and_the_line_under_the_title_is_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        write_agent(
            dir.path(),
            "w",
            "a",
            r#"{"id":"a","title":"x","provider":"claude","cwd":"/tmp/w","lastStatus":"running",
                "config":{"model":"claude-opus-5[1m]"}}"#,
        );
        let provider = AgentsProvider::with_root(dir.path().to_path_buf());
        let item = provider.search("", 0).remove(0).item;
        assert_eq!(item.source.as_deref(), Some("claude"), "the badge names the tool, not the model");
        assert_eq!(item.subtitle.as_deref(), Some("/tmp/w"), "the line under the title is the workspace");
    }

    #[test]
    fn the_badge_falls_back_to_the_models_family_when_no_provider_is_recorded() {
        let dir = tempfile::tempdir().unwrap();
        write_agent(
            dir.path(),
            "w",
            "a",
            r#"{"id":"a","title":"x","lastStatus":"running","config":{"model":"gpt-5-codex"}}"#,
        );
        let provider = AgentsProvider::with_root(dir.path().to_path_buf());
        assert_eq!(provider.search("", 0)[0].item.source.as_deref(), Some("gpt"));
    }

    #[test]
    fn an_agent_needing_attention_says_so_instead_of_showing_a_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        write_agent(
            dir.path(),
            "w",
            "a",
            r#"{"id":"a","title":"blocked","lastStatus":"running","requiresAttention":true,
                "lastActivityAt":"2026-08-23T01:48:18.912Z"}"#,
        );
        let provider = AgentsProvider::with_root(dir.path().to_path_buf());
        assert_eq!(provider.search("", 0)[0].item.accessory.as_deref(), Some("Needs you"));
    }

    #[test]
    fn the_deep_link_puts_h_in_the_host_where_paseos_parser_requires_it() {
        let link = deep_link("srv_abc", "agent-1");
        assert_eq!(link, "paseo://h/srv_abc/agent/agent-1");
        // The exact conditions `parseAgentDeepLink` applies: `h` must be the
        // host, and the path must be exactly three segments with "agent" in
        // the middle. A single-slash URL fails the first and the third.
        let rest = link.strip_prefix("paseo://h/").expect("`h` must be the host, not a path segment");
        let segments: Vec<&str> = rest.split('/').collect();
        assert_eq!(segments, vec!["srv_abc", "agent", "agent-1"]);
    }

    #[test]
    fn the_server_id_is_read_from_disk_and_trimmed() {
        // Paseo writes the file with a trailing newline; an untrimmed id
        // would produce a URL with a newline in the path and fail silently.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".paseo");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("server-id"), "srv_Rj6twNQn7Qcc\n").unwrap();
        // `server_id` reads `$HOME`, so the assertion is on the parsing rule
        // it applies rather than on this process's real home.
        let raw = std::fs::read_to_string(path.join("server-id")).unwrap();
        assert_eq!(raw.trim(), "srv_Rj6twNQn7Qcc");
    }

    #[test]
    fn a_missing_paseo_directory_is_an_empty_list_not_an_error() {
        let provider = AgentsProvider::with_root(PathBuf::from("/definitely/not/here"));
        assert!(provider.search("", 0).is_empty());
        assert!(provider.search("claude", 0).is_empty());
    }

    #[test]
    fn a_bare_link_keeps_the_end_that_names_it() {
        // Every one of these is a real title from this machine. Truncated
        // raw they all read "https://github.co…" — the whole tile spent on
        // the two parts every such link shares.
        assert_eq!(
            title_from_prompt("https://github.com/HealthifyMe/athena/pull/4501"),
            "github.com/\u{2026}/pull/4501"
        );
        // Figma puts a 22-character file key in the middle of the path; it
        // is the least informative run of characters in the URL.
        assert_eq!(
            title_from_prompt(
                "https://www.figma.com/design/ihcBLzmf4sBM0KJCMo8ryO/Care-Comms"
            ),
            "figma.com/design/Care-Comms"
        );
        assert_eq!(title_from_prompt("https://example.com/"), "example.com");
        assert_eq!(title_from_prompt("https://example.com/a/b?x=1#frag"), "example.com/a/b");
    }

    #[test]
    fn a_prompt_that_merely_mentions_a_link_is_left_alone() {
        // Rewriting the link inside a sentence would be rewriting what was
        // typed. Only a prompt that is *nothing but* a URL is compacted.
        let raw = "have a look at https://github.com/a/b/pull/1 when you get a chance";
        assert_eq!(title_from_prompt(raw), raw);
        assert_eq!(compact_url("ftp://example.com/x"), None);
    }

    #[test]
    fn leading_decoration_comes_off_the_front_of_the_tile() {
        // A real closed agent on this machine opened with a quoted block.
        assert_eq!(
            title_from_prompt("\u{258e} Heads-up: one small change needed"),
            "Heads-up: one small change needed"
        );
        assert_eq!(title_from_prompt("> quoted request"), "quoted request");
        assert_eq!(title_from_prompt("## Fix the parser"), "Fix the parser");
        assert_eq!(title_from_prompt("- do the thing"), "do the thing");
    }

    #[test]
    fn a_pasted_task_is_cut_to_one_line_with_its_gaps_closed() {
        // A title is one row tall, and a pasted prompt's own spacing must
        // not render as a title with a hole in the middle of it.
        assert_eq!(
            title_from_prompt("rebase   the\tbranch\n\nthen run the tests"),
            "rebase the branch"
        );
        assert_eq!(title_from_prompt("   \n  "), "");
    }

    #[test]
    fn an_opaque_key_is_dropped_but_a_real_word_or_a_number_is_not() {
        assert!(is_opaque_id("ihcBLzmf4sBM0KJCMo8ryO"));
        // A long word with no digits is a name, not a key.
        assert!(!is_opaque_id("documentation-index"));
        assert!(!is_opaque_id("internationalization"));
        // A PR number is short and is the whole point of the link.
        assert!(!is_opaque_id("4501"));
    }


    /// Lays out a real `~/.paseo` shape: `agents/` and its **sibling**
    /// `projects/`, which is the relationship `read_workspace_names` has to
    /// walk up to find.
    fn write_paseo_home(home: &Path) -> PathBuf {
        std::fs::create_dir_all(home.join("projects")).unwrap();
        std::fs::write(
            home.join("projects/projects.json"),
            r#"[{"projectId":"remote:github.com/Care-Connect-AI/triage-fe",
                 "displayName":"Care-Connect-AI/triage-fe"}]"#,
        )
        .unwrap();
        std::fs::write(
            home.join("projects/workspaces.json"),
            r#"[{"workspaceId":"wks_1","displayName":"feat/doctors-maps",
                 "projectId":"remote:github.com/Care-Connect-AI/triage-fe"},
                {"workspaceId":"wks_2","displayName":"main"}]"#,
        )
        .unwrap();
        home.join("agents")
    }

    #[test]
    fn a_tile_is_named_by_its_branch_and_repository_not_by_its_first_prompt() {
        // Three of this machine's real agents share the prompt "what's the
        // update on the agents tasks"; none of them share a branch.
        let dir = tempfile::tempdir().unwrap();
        let root = write_paseo_home(dir.path());
        write_agent(
            &root,
            "triage",
            "a1",
            r#"{"id":"a1","title":"https://www.figma.com/design/ihcBLzmf4sBM0KJCMo8ryO/Care",
                "workspaceId":"wks_1","cwd":"/Users/x/Documents/tcc/triage-fe",
                "lastStatus":"running"}"#,
        );
        let provider = AgentsProvider::with_root(root);
        let items: Vec<_> = provider.search("", 0).into_iter().map(|c| c.item).collect();
        assert_eq!(items[0].title, "feat/doctors-maps");
        assert_eq!(items[0].subtitle.as_deref(), Some("Care-Connect-AI/triage-fe"));
    }

    #[test]
    fn a_workspace_with_no_project_still_names_the_tile() {
        let dir = tempfile::tempdir().unwrap();
        let root = write_paseo_home(dir.path());
        write_agent(
            &root,
            "neko",
            "a2",
            r#"{"id":"a2","title":"hi","workspaceId":"wks_2",
                "cwd":"/Users/x/Documents/neko","lastStatus":"running"}"#,
        );
        let provider = AgentsProvider::with_root(root);
        let items: Vec<_> = provider.search("", 0).into_iter().map(|c| c.item).collect();
        assert_eq!(items[0].title, "main");
        // No project for this workspace, so the path is still better than
        // nothing. Asserted by suffix because `tildify` reads the real
        // `$HOME`, which is not this fixture's `/Users/x`.
        assert!(items[0].subtitle.as_deref().unwrap().ends_with("Documents/neko"));
    }

    #[test]
    fn an_unresolvable_workspace_falls_back_to_the_directory_then_the_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let root = write_paseo_home(dir.path());
        write_agent(
            &root,
            "gone",
            "a3",
            r#"{"id":"a3","title":"fix the parser","workspaceId":"wks_missing",
                "cwd":"/Users/x/Documents/parser","lastStatus":"running"}"#,
        );
        write_agent(&root, "gone", "a4", r#"{"id":"a4","title":"fix the parser","lastStatus":"running"}"#);
        let provider = AgentsProvider::with_root(root);
        let titles: Vec<_> = provider.search("", 0).into_iter().map(|c| c.item.title).collect();
        assert!(titles.contains(&"parser".to_string()), "{titles:?}");
        assert!(titles.contains(&"fix the parser".to_string()), "{titles:?}");
    }

    #[test]
    fn the_prompt_is_still_searchable_even_though_it_is_never_shown() {
        // It is how a person remembers what they asked for, even though it
        // does not distinguish two sessions on screen.
        let dir = tempfile::tempdir().unwrap();
        let root = write_paseo_home(dir.path());
        write_agent(
            &root,
            "triage",
            "a1",
            r#"{"id":"a1","title":"testimonials carousel","workspaceId":"wks_1",
                "cwd":"/Users/x/Documents/tcc/triage-fe","lastStatus":"running"}"#,
        );
        let provider = AgentsProvider::with_root(root);
        let found = provider.search("testimonials", 0);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].item.title, "feat/doctors-maps");
    }


    #[test]
    fn a_second_line_that_would_repeat_the_title_carries_the_prompt_instead() {
        // Paseo names a `kind: "directory"` workspace after its folder and
        // gives it a project of the same name, so both of this machine's
        // `~/Documents/hme` agents resolved to "hme" over "hme" — the
        // lookup succeeding and still saying nothing.
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::create_dir_all(home.join("projects")).unwrap();
        std::fs::write(
            home.join("projects/projects.json"),
            r#"[{"projectId":"/x/hme","displayName":"hme"}]"#,
        )
        .unwrap();
        std::fs::write(
            home.join("projects/workspaces.json"),
            r#"[{"workspaceId":"/x/hme","displayName":"hme","projectId":"/x/hme"}]"#,
        )
        .unwrap();
        let root = home.join("agents");
        write_agent(
            &root,
            "hme",
            "a1",
            r#"{"id":"a1","title":"https://github.com/HealthifyMe/athena/pull/4501",
                "workspaceId":"/x/hme","cwd":"/x/hme","lastStatus":"running"}"#,
        );
        write_agent(
            &root,
            "hme",
            "a2",
            r#"{"id":"a2","title":"So we've been working on testimonials",
                "workspaceId":"/x/hme","cwd":"/x/hme","lastStatus":"running"}"#,
        );
        let provider = AgentsProvider::with_root(root);
        let mut seen: Vec<(String, String)> = provider
            .search("", 0)
            .into_iter()
            .map(|c| (c.item.title, c.item.subtitle.unwrap_or_default()))
            .collect();
        seen.sort();
        assert_eq!(seen, vec![
            ("hme".to_string(), "So we've been working on testimonials".to_string()),
            ("hme".to_string(), "github.com/\u{2026}/pull/4501".to_string()),
        ]);
    }

    #[test]
    fn a_real_repository_name_still_beats_the_prompt() {
        // The fallback is scoped to the case that has nothing to say. Where
        // Paseo knows the repository, that is what the line is for.
        let dir = tempfile::tempdir().unwrap();
        let root = write_paseo_home(dir.path());
        write_agent(
            &root,
            "triage",
            "a1",
            r#"{"id":"a1","title":"fix the carousel","workspaceId":"wks_1",
                "cwd":"/x/triage-fe","lastStatus":"running"}"#,
        );
        let provider = AgentsProvider::with_root(root);
        let item = provider.search("", 0).remove(0).item;
        assert_eq!(item.title, "feat/doctors-maps");
        assert_eq!(item.subtitle.as_deref(), Some("Care-Connect-AI/triage-fe"));
    }


    #[test]
    fn a_prompt_is_always_sent_in_the_background() {
        // The default is `false` for a top-level caller like neko, and
        // `false` makes Paseo wait for the agent to *finish* before
        // answering — a panel keystroke holding a request thread for
        // minutes. This is the assertion that keeps that from creeping back.
        let args = send_prompt_arguments("agent-1", "run the tests");
        assert_eq!(args["background"], serde_json::json!(true));
        assert_eq!(args["agentId"], serde_json::json!("agent-1"));
        assert_eq!(args["prompt"], serde_json::json!("run the tests"));
    }

    #[test]
    fn sending_nothing_is_refused_before_it_reaches_the_daemon() {
        // The query *is* the prompt here, so an empty one is a mis-keyed
        // Enter rather than a request.
        let provider = AgentControlProvider::with_root(PathBuf::from("/nonexistent"));
        assert!(provider.activate_with_query("a", "   ").is_err());
        assert!(provider.activate("a").is_err());
    }

    #[test]
    fn the_modes_offered_come_from_the_provider_that_owns_them() {
        // Claude and Codex genuinely offer different session modes, so a
        // compiled-in list would be wrong for whichever one it was not
        // written against.
        let value = serde_json::json!({"providers": [
            {"id": "claude", "modes": [
                {"id": "plan", "label": "Plan Mode"},
                {"id": "bypassPermissions", "label": "Bypass"}
            ]},
            {"id": "codex", "modes": [{"id": "auto"}]},
            {"id": "no-modes", "modes": []}
        ]});
        let parsed = parse_provider_modes(&value);
        assert_eq!(parsed["claude"], vec![
            ("plan".to_string(), "Plan Mode".to_string()),
            ("bypassPermissions".to_string(), "Bypass".to_string()),
        ]);
        // A mode with no label falls back to its id rather than rendering
        // an empty menu entry.
        assert_eq!(parsed["codex"], vec![("auto".to_string(), "auto".to_string())]);
        // A provider with no modes contributes no menu at all.
        assert!(!parsed.contains_key("no-modes"));
    }


    #[test]
    fn an_empty_provider_mode_map_is_never_cached() {
        // The daemon restarting mid-keystroke yields an empty map, and
        // caching that for two minutes would strip every mode entry out of
        // the ⌘K menu long after Paseo came back.
        *MODES.lock().unwrap() = None;
        let empty = parse_provider_modes(&serde_json::json!({"providers": []}));
        assert!(empty.is_empty());
        // The guard lives in `provider_modes`; this pins the property it
        // protects — an empty parse must stay empty rather than becoming a
        // cached answer.
        assert!(MODES.lock().unwrap().is_none(), "nothing was cached from a failure");
    }

}

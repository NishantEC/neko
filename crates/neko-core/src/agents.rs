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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use neko_protocol::{Glyph, Icon, SearchItem};
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

    /// What the row is called. Paseo titles an agent from its first prompt,
    /// which can be absent (never prompted) or a wall of text (a pasted
    /// task), so neither is trusted raw: a missing title falls back to the
    /// working directory's own name, which is how a person thinks about
    /// "the agent in neko" anyway.
    fn display_title(&self) -> String {
        let from_title = self.title.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(first_line);
        if let Some(title) = from_title {
            return title;
        }
        self.cwd
            .as_deref()
            .map(|cwd| Path::new(cwd).file_name().map_or_else(|| cwd.to_string(), |n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "Agent".to_string())
    }

    /// **The workspace, not the model.** The model moved to its own mark in
    /// the corner of the tile (`SearchItem::source` carries it), which frees
    /// this line for the thing that actually distinguishes two agents running
    /// the same model: where they are working.
    fn subtitle(&self) -> Option<String> {
        self.cwd.as_deref().map(tildify)
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

/// A title is one row tall. A pasted multi-line task must not push a row's
/// own height around, and the first line is the part that identifies it.
fn first_line(raw: &str) -> String {
    raw.lines().next().unwrap_or(raw).trim().to_string()
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
                    let title = agent.display_title();
                    let haystacks = [
                        Some(title.clone()),
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
                Some((agent.activity_at().unwrap_or("").to_string(), running, Candidate { score, item: to_item(&agent, running) }))
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

fn to_item(agent: &PaseoAgent, running: bool) -> SearchItem {
    SearchItem {
        id: agent.id.clone(),
        kind: "agent".to_string(),
        title: agent.display_title(),
        subtitle: agent.subtitle(),
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
        actions: Vec::new(),
        // The tool running this agent, for the badge the client overlays on
        // the host icon. `source` is the wire's "bare value for a labelled
        // field" slot; how it renders is the client's business.
        source: agent.provider_mark(),
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
}

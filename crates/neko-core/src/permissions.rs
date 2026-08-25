//! The permission inbox — agents that are blocked waiting for you.
//!
//! **L2 of `docs/plan-agent-control-plane.md`, and the feature that justifies
//! the plan.** An agent that hits something it is not allowed to do stops and
//! waits. Until now the only way to notice was to switch to Paseo and look;
//! an agent could sit blocked for an hour on a question you would have
//! answered in a second.
//!
//! A launcher's whole job is the shortest path from "something needs me" to
//! "handled". This is that path: the rows are at the top of an empty panel,
//! Enter approves, `⌘K` denies.
//!
//! ## Why this answers an empty root query when themes and preferences do not
//!
//! `Provider::answers_empty_root_query` exists because most providers have
//! nothing useful to say before you have typed. This one is the opposite of
//! that: a blocked agent is the single most time-sensitive thing this app
//! knows about, it is self-limiting (a machine has a handful of agents, not
//! hundreds), and it is *news* — the exact quality "Agents: what is running
//! right now" already uses to justify its own place there.
//!
//! ## Freshness beats cheapness here
//!
//! Every other daemon-backed provider caches for tens of seconds. This one
//! caches for [`CACHE_TTL`] — long enough that typing does not spawn a `curl`
//! per keystroke, short enough that a permission answered in Paseo's own
//! window disappears from neko almost immediately. Approving something that
//! was already handled elsewhere is the one failure this provider must not
//! have, and a short window is most of the defence; `respond_to_permission`
//! failing on an unknown request id is the rest.

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::mcp::{McpClient, McpError};
use crate::provider::{Provider, ProviderError};
use crate::search::Candidate;

/// See the module comment: short on purpose.
const CACHE_TTL: Duration = Duration::from_millis(1500);

/// Blocked agents outrank everything else in the root list, by a margin no
/// ordinary match can close.
///
/// This is a deliberate exception to "providers compete on `fuzzy_score`".
/// The scale that ranks an app against a file has nothing to say about "an
/// agent is stopped, waiting for you" — that is not a better match, it is a
/// different kind of thing, and it is the only row in this app with a person
/// on the other end of it.
const ATTENTION_BONUS: f32 = 1_000.0;

/// One agent, stopped, waiting.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub agent_id: String,
    pub request_id: String,
    /// What is being asked for — a tool name like `Bash`, or a title the
    /// provider wrote.
    pub title: String,
    /// The fuller sentence, when there is one.
    pub detail: Option<String>,
    /// `tool`, `plan`, `question`, `mode`, `other`.
    pub kind: String,
}

impl Pending {
    /// What the row says it will do. `"Bash"` on its own is a tool name, not
    /// a sentence, so it gets one — and a request that already carries a
    /// title keeps it, because the provider wrote that for a person.
    pub fn headline(&self) -> String {
        match self.kind.as_str() {
            "tool" => format!("Run {}", self.title),
            "plan" => format!("Approve plan: {}", self.title),
            "question" => self.title.clone(),
            _ => self.title.clone(),
        }
    }
}

/// What the inbox has to say, including the reasons it might have nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Inbox {
    Pending(Vec<Pending>),
    /// Nobody is blocked. Renders as no rows at all — an empty inbox is the
    /// normal state and does not deserve a row saying so.
    Clear,
    /// Paseo is not running. Also renders as nothing: neko is a launcher
    /// first, and an app that is not running is not an error to report on
    /// every keystroke.
    Unavailable,
}

static CACHE: Mutex<Option<(Instant, Inbox)>> = Mutex::new(None);

pub fn inbox(client: &McpClient) -> Inbox {
    if let Some((at, cached)) = CACHE.lock().unwrap().as_ref()
        && at.elapsed() < CACHE_TTL
    {
        return cached.clone();
    }
    let fresh = fetch(client);
    *CACHE.lock().unwrap() = Some((Instant::now(), fresh.clone()));
    fresh
}

/// Drops the cache so the next [`inbox`] really refetches — called the
/// instant a response is sent, so the row disappears without waiting out the
/// TTL.
/// Whether [`inbox`] can answer right now without touching the network.
///
/// This is what makes a blocked agent appear in the panel's **first** frame
/// rather than its second: `defers_for` exists to keep a socket round trip
/// off the fast path, and a warm cache has no round trip to keep off it.
/// With `neko-daemon`'s poller running the cache is warm essentially always,
/// so the deferred path is really only the cold-start case.
pub fn is_warm() -> bool {
    CACHE.lock().unwrap().as_ref().is_some_and(|(at, _)| at.elapsed() < CACHE_TTL)
}

/// How many agents are waiting, without fetching. `None` when nothing is
/// cached yet — distinct from `Some(0)`, which is a real, current "nobody".
pub fn cached_count() -> Option<usize> {
    CACHE.lock().unwrap().as_ref().and_then(|(at, inbox)| {
        if at.elapsed() >= CACHE_TTL {
            return None;
        }
        Some(match inbox {
            Inbox::Pending(pending) => pending.len(),
            Inbox::Clear | Inbox::Unavailable => 0,
        })
    })
}

/// Refetches unconditionally, ignoring the cache, and returns the new count.
/// The poller's entry point — [`inbox`] would mostly return its own warm
/// cache and never refresh anything.
pub fn refresh(client: &McpClient) -> usize {
    let fresh = fetch(client);
    let count = match &fresh {
        Inbox::Pending(pending) => pending.len(),
        Inbox::Clear | Inbox::Unavailable => 0,
    };
    *CACHE.lock().unwrap() = Some((Instant::now(), fresh));
    count
}

pub fn invalidate() {
    *CACHE.lock().unwrap() = None;
}

fn fetch(client: &McpClient) -> Inbox {
    match client.call("list_pending_permissions", json!({})) {
        Ok(value) => parse_inbox(&value),
        Err(McpError::NotRunning) => Inbox::Unavailable,
        // A daemon that answered with a failure is still a daemon that is
        // there; treating it as "clear" is the honest reading — neko knows
        // of nothing blocked — and it keeps a transient daemon error from
        // putting an error row above every search result in the app.
        Err(_) => Inbox::Clear,
    }
}

/// Reads `{"permissions": [{agentId, status, request: {...}}]}`.
///
/// Every field but the two ids is optional in practice: `title` and
/// `description` are provider-written and frequently absent, and a row with
/// no name at all falls back to its kind rather than rendering blank — the
/// same rule `apps::bundle_display_name` follows for a nameless bundle.
pub fn parse_inbox(value: &Value) -> Inbox {
    let Some(entries) = value.get("permissions").and_then(Value::as_array) else {
        return Inbox::Clear;
    };
    let pending: Vec<Pending> = entries
        .iter()
        .filter_map(|entry| {
            let request = entry.get("request")?;
            let agent_id = entry.get("agentId")?.as_str()?.to_string();
            let request_id = request.get("id")?.as_str()?.to_string();
            let kind =
                request.get("kind").and_then(Value::as_str).unwrap_or("other").to_string();
            let title = ["title", "name"]
                .iter()
                .find_map(|key| request.get(*key)?.as_str().filter(|s| !s.is_empty()))
                .map(str::to_string)
                .unwrap_or_else(|| kind.clone());
            Some(Pending {
                agent_id,
                request_id,
                title,
                detail: request
                    .get("description")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                kind,
            })
        })
        .collect();
    if pending.is_empty() { Inbox::Clear } else { Inbox::Pending(pending) }
}

/// `"<agentId>:<requestId>"` — the row's own id, and what `activate` takes
/// apart again.
///
/// Both halves are needed to answer, and `SearchItem::id` is one string.
/// Joined on `:` because neither half contains one (both are uuids), and
/// split on the *first* so a future id that does contain one still resolves
/// its agent correctly.
pub fn row_id(pending: &Pending) -> String {
    format!("{}:{}", pending.agent_id, pending.request_id)
}

pub fn split_row_id(id: &str) -> Option<(&str, &str)> {
    let (agent, request) = id.split_once(':')?;
    (!agent.is_empty() && !request.is_empty()).then_some((agent, request))
}

/// The daemon reads `list_pending_permissions` and answers one of them.
pub struct PermissionsProvider {
    /// Whether this provider is allowed to reach the daemon at all.
    ///
    /// Injected rather than always-on because `AppState::with_test_providers`
    /// is the one constructor the whole daemon test suite goes through, and a
    /// provider that opens a socket there would put a real network round trip
    /// inside every hermetic test — the same reason `FileProvider::empty()`
    /// and `SettingsProvider::with_panes` exist beside their real
    /// counterparts.
    live: bool,
}

impl PermissionsProvider {
    pub fn new() -> Self {
        Self { live: true }
    }

    /// A provider that never reaches the daemon and always answers with
    /// nothing — for tests, which must not depend on whether Paseo happens
    /// to be running on the machine running the suite.
    pub fn disabled() -> Self {
        Self { live: false }
    }

    /// Rediscovered per search rather than resolved once, so starting Paseo
    /// does not need a neko restart to be noticed.
    fn client(&self) -> Option<McpClient> {
        self.live.then(McpClient::discover).and_then(Result::ok)
    }
}

impl Default for PermissionsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for PermissionsProvider {
    fn id(&self) -> &'static str {
        "permission"
    }

    fn section_label(&self) -> &'static str {
        "Needs you"
    }

    /// **Yes** — see the module comment. This is the one row in the app with
    /// a person waiting on the other end of it.
    fn answers_empty_root_query(&self) -> bool {
        true
    }

    /// **Always deferred.** Answering the inbox costs a `curl` to the
    /// daemon, and the fast provider group exists precisely so that nothing
    /// which touches a socket delays first paint (`AGENTS.md`, "Two-phase
    /// search"). Unlike `files::FileProvider`, which defers only once a
    /// query is long enough to really run `mdfind`, this defers for every
    /// query including the empty one — the cost is the round trip, not the
    /// query, so there is no cheap case to exempt.
    ///
    /// The consequence is visible and correct: on a cold panel the inbox
    /// rows land a few milliseconds after the apps do, appended below them
    /// by `panel::merge_late_results` rather than reordering what is already
    /// on screen.
    fn defers_for(&self, _query: &str) -> bool {
        // Gated twice, and both gates are about not paying for a frame
        // that buys nothing. A provider that will not make a request has
        // nothing to defer *for*; and a **warm cache** has no round trip to
        // keep off the fast path, so with the daemon's poller running the
        // rows land in the first frame — which is the whole point of L3.
        // Discovery is one small file read, free next to the `mdfind` this
        // same search is about to run.
        !is_warm() && self.client().is_some()
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let Some(client) = self.client() else { return Vec::new() };
        let Inbox::Pending(pending) = inbox(&client) else { return Vec::new() };

        let trimmed = query.trim();
        pending
            .iter()
            .filter_map(|p| {
                let headline = p.headline();
                // Typed queries still have to match, or "needs you" would
                // pin itself above every search result in the app forever.
                // An empty query matches everything, which is the point.
                let score = if trimmed.is_empty() {
                    ATTENTION_BONUS
                } else {
                    let best = [Some(headline.clone()), p.detail.clone(), Some("needs you".into())]
                        .into_iter()
                        .flatten()
                        .filter_map(|hay| crate::search::fuzzy_score(trimmed, &hay))
                        .fold(None, |best: Option<f32>, s| Some(best.map_or(s, |b| b.max(s))))?;
                    best + ATTENTION_BONUS
                };
                Some(Candidate {
                    score,
                    item: SearchItem {
                        id: row_id(p),
                        kind: "permission".to_string(),
                        title: headline,
                        subtitle: p.detail.clone(),
                        // **`Agent`, not `AgentLive`.** A blocked agent is
                        // stopped — that is the entire reason the row
                        // exists — and the live glyph carries a presence dot
                        // that would say the opposite.
                        icon: Icon::Glyph(Glyph::Agent),
                        section_label: "Needs you".to_string(),
                        action_label: "Approve  \u{21b5}".to_string(),
                        badge: Some(p.kind.to_uppercase()),
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        // Deny is deliberately not the primary action and
                        // deliberately not `destructive`: denying is a normal
                        // answer, not a mistake to guard against, and arming
                        // it behind a second Enter would make the safer reply
                        // the slower one.
                        actions: vec![ItemAction {
                            id: "deny".to_string(),
                            label: "Deny".to_string(),
                            destructive: false,
                        }],
                        source: None,
                        meter: None,
                        keeps_open: false,
                        preview_markdown: false,
                        preview: None,
                    },
                })
            })
            .collect()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        self.respond(id, json!({ "behavior": "allow" }))
    }

    fn perform_action(&self, id: &str, action: &str) -> Result<(), ProviderError> {
        match action {
            "deny" => self.respond(id, json!({ "behavior": "deny" })),
            other => Err(ProviderError(format!("no action '{other}' on this row"))),
        }
    }
}

impl PermissionsProvider {
    fn respond(&self, id: &str, response: Value) -> Result<(), ProviderError> {
        let (agent_id, request_id) = split_row_id(id)
            .ok_or_else(|| ProviderError("that permission row is malformed".to_string()))?;
        let client = self
            .client()
            // One wording for one condition: `McpError::NotRunning` already
            // says this, and a second phrasing of the same fact is the
            // "Can't reach" / "Couldn't reach" split all over again.
            .ok_or_else(|| ProviderError(McpError::NotRunning.to_string()))?;
        let result = client.call(
            "respond_to_permission",
            json!({ "agentId": agent_id, "requestId": request_id, "response": response }),
        );
        // Whatever happened, what neko believes about the inbox is now stale
        // — including on failure, where the most likely cause is that this
        // request was already answered somewhere else.
        invalidate();
        result.map(|_| ()).map_err(|e| ProviderError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped exactly like `paseo-tools.ts`'s own `outputSchema` for this
    /// tool: `{agentId, status, request: AgentPermissionRequestPayload}`.
    fn fixture() -> Value {
        json!({"permissions": [
            {
                "agentId": "agent-1",
                "status": "waiting",
                "request": {
                    "id": "req-1",
                    "provider": "claude",
                    "name": "Bash",
                    "kind": "tool",
                    "description": "rm -rf build/"
                }
            },
            {
                "agentId": "agent-2",
                "status": "waiting",
                "request": {"id": "req-2", "provider": "claude", "name": "ExitPlanMode",
                            "kind": "plan", "title": "Rewrite the parser"}
            }
        ]})
    }

    #[test]
    fn a_pending_request_becomes_a_row_a_person_can_read() {
        let Inbox::Pending(pending) = parse_inbox(&fixture()) else { panic!("expected pending") };
        assert_eq!(pending.len(), 2);
        // "Bash" alone is a tool name, not a sentence.
        assert_eq!(pending[0].headline(), "Run Bash");
        assert_eq!(pending[0].detail.as_deref(), Some("rm -rf build/"));
        // A title the provider wrote for a person is kept as written.
        assert_eq!(pending[1].headline(), "Approve plan: Rewrite the parser");
    }

    #[test]
    fn an_empty_inbox_is_clear_rather_than_an_empty_list_of_rows() {
        // The normal state, and it must produce no rows at all — not a row
        // saying there is nothing.
        assert_eq!(parse_inbox(&json!({"permissions": []})), Inbox::Clear);
        assert_eq!(parse_inbox(&json!({})), Inbox::Clear);
    }

    #[test]
    fn a_request_with_no_name_at_all_still_renders() {
        // Every field but the two ids is provider-written and often absent.
        let value = json!({"permissions": [
            {"agentId": "a", "status": "waiting", "request": {"id": "r", "kind": "question"}}
        ]});
        let Inbox::Pending(pending) = parse_inbox(&value) else { panic!("expected pending") };
        assert_eq!(pending[0].title, "question");
        assert!(pending[0].detail.is_none());
    }

    #[test]
    fn an_entry_missing_an_id_is_skipped_rather_than_answered_wrongly() {
        // Both ids are needed to reply, and replying to the wrong request is
        // worse than not showing the row.
        let value = json!({"permissions": [
            {"agentId": "a", "request": {"kind": "tool", "name": "Bash"}},
            {"request": {"id": "r", "kind": "tool", "name": "Bash"}}
        ]});
        assert_eq!(parse_inbox(&value), Inbox::Clear);
    }

    #[test]
    fn a_row_id_carries_both_halves_and_survives_the_round_trip() {
        let p = Pending {
            agent_id: "5c9b-uuid".into(),
            request_id: "req-77".into(),
            title: "Bash".into(),
            detail: None,
            kind: "tool".into(),
        };
        let id = row_id(&p);
        assert_eq!(split_row_id(&id), Some(("5c9b-uuid", "req-77")));
        // A malformed row must never resolve to a half-specified response.
        assert_eq!(split_row_id("no-colon"), None);
        assert_eq!(split_row_id(":req"), None);
        assert_eq!(split_row_id("agent:"), None);
    }

    #[test]
    fn a_second_colon_belongs_to_the_request_not_the_agent() {
        assert_eq!(split_row_id("agent:req:extra"), Some(("agent", "req:extra")));
    }
}

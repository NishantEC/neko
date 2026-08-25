//! Reading an agent's conversation inside neko.
//!
//! Until now an agent row's Enter opened Paseo. That is the right thing when
//! you want to *work* with an agent, and the wrong thing when you only want
//! to know what it has been doing — switching apps to read three lines is
//! most of the cost of not having asked.
//!
//! ## This provider is about one agent, not a list of them
//!
//! Every other mode is one provider's own list, so scoping the search to the
//! provider was the whole of "which mode am I in". A conversation is a list
//! *about* something, and the provider cannot know which agent unless it is
//! told — so `ActiveMode::subject` carries the agent id and `run_search`
//! sends it as the scoped query. `search`'s `query` argument here is an
//! **agent id**, where a filtering provider would receive what was typed.
//!
//! **The consequence, stated rather than discovered later: typing in this
//! mode does nothing.** The query slot is spoken for. That is a real limit
//! and the honest place for it is here, not in a comment on the caller —
//! searching *within* a conversation would need a second query channel the
//! protocol does not have.
//!
//! ## Why the daemon's summary rather than the raw transcript
//!
//! Claude Code writes its own transcript to
//! `~/.claude/projects/<cwd>/<session>.jsonl`, and this machine's is **13 MB**
//! for one session. Paseo's `get_agent_activity` returns a curated summary of
//! the same thing — tool calls reduced to `[Write] path`, the agent's own
//! prose kept — which is what a person reading a panel actually wants, and it
//! arrives over a channel neko already speaks.

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::mcp::{McpClient, McpError};
use crate::provider::{Provider, ProviderError};
use crate::search::Candidate;

/// How many timeline entries to ask for.
///
/// A running agent can have a thousand; this is a panel, not a log viewer.
/// The most recent are the ones worth reading — `get_agent_activity` returns
/// newest-first and says how many it left out.
const ACTIVITY_LIMIT: usize = 12;

/// How long a fetched conversation stays good.
///
/// Longer than the terminal capture's two seconds: a terminal is a live
/// screen, a conversation is a record, and an agent that says something new
/// while you are reading has not invalidated what you were reading. Short
/// enough that re-entering the mode shows a fresh answer.
const CACHE_TTL: Duration = Duration::from_secs(10);

/// One thing the agent did or said.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// `"[Write] /path/to/file"` or the first line of what it said.
    pub headline: String,
    /// The rest, when there is more than one line of it.
    pub body: Option<String>,
}

static CACHE: Mutex<Option<(Instant, String, Vec<Entry>)>> = Mutex::new(None);

/// Splits the daemon's `content` blob into readable entries.
///
/// The shape is observed, not documented: a `Showing N of M activities` header
/// line, then alternating `[Tool] argument` lines and free prose, blank-line
/// separated. Anything that does not match is kept verbatim as its own entry
/// rather than dropped — a conversation view that silently omits what the
/// agent said would be worse than one that shows a line it did not parse.
pub fn parse_activity(content: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    for block in content.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        // The daemon's own count line is about the fetch, not the agent.
        if block.starts_with("Showing ") && block.contains(" activities") {
            continue;
        }
        let mut lines = block.lines();
        let Some(first) = lines.next() else { continue };
        let rest: Vec<&str> = lines.collect();
        entries.push(Entry {
            headline: first.trim().to_string(),
            body: (!rest.is_empty()).then(|| rest.join("\n").trim().to_string()),
        });
    }
    entries
}

/// `"[Write] /very/long/path"` → `("Write", "/very/long/path")`.
///
/// Split so the tool name can lead the row and the argument can be the
/// subtitle, which is the difference between a scannable list and a wall of
/// bracketed text. `None` for prose, which is most of what is worth reading.
pub fn split_tool(headline: &str) -> Option<(&str, &str)> {
    let rest = headline.strip_prefix('[')?;
    let (tool, argument) = rest.split_once(']')?;
    (!tool.is_empty()).then(|| (tool, argument.trim()))
}

fn fetch(client: &McpClient, agent_id: &str) -> Result<Vec<Entry>, McpError> {
    let value =
        client.call("get_agent_activity", json!({"agentId": agent_id, "limit": ACTIVITY_LIMIT}))?;
    let content = value.get("content").and_then(Value::as_str).unwrap_or_default();
    Ok(parse_activity(content))
}

/// The `conversation` mode's list.
pub struct ConversationProvider {
    live: bool,
}

impl ConversationProvider {
    pub fn new() -> Self {
        Self { live: true }
    }

    /// Never reaches the daemon — see `permissions::PermissionsProvider::disabled`.
    pub fn disabled() -> Self {
        Self { live: false }
    }
}

impl Default for ConversationProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for ConversationProvider {
    fn id(&self) -> &'static str {
        "conversation"
    }

    fn section_label(&self) -> &'static str {
        "Conversation"
    }

    /// `query` is an **agent id** — see the module comment.
    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let agent_id = query.trim();
        if agent_id.is_empty() || !self.live {
            return Vec::new();
        }
        let entries = {
            let cached = CACHE
                .lock()
                .unwrap()
                .as_ref()
                .filter(|(at, id, _)| at.elapsed() < CACHE_TTL && id == agent_id)
                .map(|(_, _, entries)| entries.clone());
            match cached {
                Some(entries) => entries,
                None => {
                    let Ok(client) = McpClient::discover() else { return Vec::new() };
                    let fresh = fetch(&client, agent_id).unwrap_or_default();
                    *CACHE.lock().unwrap() =
                        Some((Instant::now(), agent_id.to_string(), fresh.clone()));
                    fresh
                }
            }
        };

        let count = entries.len();
        entries
            .iter()
            .enumerate()
            .map(|(rank, entry)| {
                let (title, subtitle, badge) = match split_tool(&entry.headline) {
                    Some((tool, argument)) => (
                        argument.to_string(),
                        entry.body.clone(),
                        Some(tool.to_uppercase()),
                    ),
                    // Prose: the agent's own words lead, and the rest of the
                    // paragraph follows.
                    None => (entry.headline.clone(), entry.body.clone(), None),
                };
                Candidate {
                    // Newest first, preserved as descending scores because
                    // `allocate` ranks by score and has no reason to know
                    // this arrived in order.
                    score: (count - rank) as f32,
                    item: SearchItem {
                        // The index, because two identical shell commands are
                        // genuinely two different entries and
                        // `resolve_selection` follows `(kind, id)`.
                        id: format!("{agent_id}#{rank}"),
                        kind: "conversation".to_string(),
                        title: if title.is_empty() { entry.headline.clone() } else { title },
                        subtitle,
                        icon: Icon::Glyph(Glyph::Text),
                        section_label: "Conversation".to_string(),
                        // Nothing here is an action. Enter on a line of a
                        // transcript has no meaning, and inventing one — jump
                        // to the file, re-run the command — would be guessing
                        // at intent on a read-only surface.
                        action_label: "Reading".to_string(),
                        badge,
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions: vec![ItemAction {
                            id: "open-in-paseo".to_string(),
                            label: "Open in Paseo".to_string(),
                            destructive: false,
                        }],
                        source: None,
                        meter: None,
                        keeps_open: true,
                        // The full text, for the detail pane, so a long reply
                        // is readable rather than truncated into a row.
                        preview: Some(match &entry.body {
                            Some(body) => format!("{}\n\n{body}", entry.headline),
                            None => entry.headline.clone(),
                        }),
                    },
                }
            })
            .collect()
    }

    /// Enter does nothing, deliberately — a transcript line is something to
    /// read, not something to run.
    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        Ok(())
    }

    fn perform_action(&self, id: &str, action: &str) -> Result<(), ProviderError> {
        match action {
            "open-in-paseo" => {
                let agent_id = id.split('#').next().unwrap_or(id);
                crate::agents::open_agent_in_paseo(agent_id)
            }
            other => Err(ProviderError(format!("no action '{other}' on this row"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim from a live `get_agent_activity` on this machine, trimmed.
    const REAL: &str = "Showing 6 of 1046 activities (limited to 6)\n\n\
        [Write] /Users/nish/Documents/hme/granth/data/drive/protab.md\n\
        All 60 documents now written. Verifying completion.\n\n\
        [Shell] pnpm ingest-drive --limit 0 2>&1 | tail -3\n\n\
        Drive KB ingestion is done: **60/60 documents**.\n\n\
        Not yet done: wiring the `node:` field into the granth UI.";

    #[test]
    fn the_daemons_own_count_line_is_not_part_of_the_conversation() {
        // "Showing 6 of 1046 activities" is about the fetch, not the agent,
        // and it would otherwise be the first thing a person read.
        let entries = parse_activity(REAL);
        assert!(!entries.iter().any(|e| e.headline.starts_with("Showing ")));
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn a_tool_call_splits_into_a_name_and_what_it_acted_on() {
        // The difference between a scannable list and a wall of bracketed
        // text: the path leads the row, the tool becomes a badge.
        assert_eq!(
            split_tool("[Write] /Users/nish/a.md"),
            Some(("Write", "/Users/nish/a.md"))
        );
        assert_eq!(split_tool("[Shell] git status"), Some(("Shell", "git status")));
        // Prose is most of what is worth reading, and is not a tool call.
        assert_eq!(split_tool("Drive KB ingestion is done."), None);
        assert_eq!(split_tool("[] nothing"), None);
    }

    #[test]
    fn a_multi_line_entry_keeps_its_body_for_the_detail_pane() {
        let entries = parse_activity(REAL);
        assert_eq!(entries[0].headline, "[Write] /Users/nish/Documents/hme/granth/data/drive/protab.md");
        assert_eq!(entries[0].body.as_deref(), Some("All 60 documents now written. Verifying completion."));
        // A single-line entry has no body rather than an empty one.
        assert!(entries[1].body.is_none());
    }

    #[test]
    fn an_unparseable_line_is_kept_rather_than_dropped() {
        // A conversation view that silently omits what the agent said is
        // worse than one showing a line it did not understand.
        let entries = parse_activity("just some text with no shape at all");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].headline, "just some text with no shape at all");
    }

    #[test]
    fn an_empty_or_countless_blob_produces_nothing_rather_than_a_blank_row() {
        assert!(parse_activity("").is_empty());
        assert!(parse_activity("Showing 0 of 0 activities").is_empty());
        assert!(parse_activity("\n\n   \n\n").is_empty());
    }

    #[test]
    fn a_row_id_carries_the_agent_and_survives_being_taken_apart() {
        // `perform_action` has to get back to the agent to open it in Paseo,
        // and the index is what keeps two identical shell commands distinct.
        let id = "628f0922-4faf#3";
        assert_eq!(id.split('#').next(), Some("628f0922-4faf"));
    }
}

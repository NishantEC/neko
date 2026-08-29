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
//! ## The transcript, not the activity feed — a reversal, and why
//!
//! The first version of this read Paseo's `get_agent_activity`, reasoning
//! that a curated summary beats a 13 MB raw transcript. That was right about
//! the size and wrong about the shape: **the activity feed has no user
//! turns** (verified against a live agent — 659 activities, all of them the
//! agent's own prose and tool calls), and a conversation with one voice is
//! not a conversation. The captain asked for a chat like Paseo's own agent
//! view, and a chat needs both speakers.
//!
//! Both speakers exist on disk. A claude agent's Paseo document carries
//! `persistence.sessionId` and `cwd`, which is exactly the address of Claude
//! Code's own session transcript —
//! `~/.claude/projects/<munged-cwd>/<sessionId>.jsonl` — chronological, with
//! real roles. The 13 MB problem is answered by reading the **tail**: the
//! last [`TAIL_BYTES`] of the file, parsed forward, keeping the last
//! [`TURN_LIMIT`] turns. No subprocess, no MCP round trip, no daemon that
//! has to be running.
//!
//! The activity feed stays as the **fallback** — a non-claude agent, or a
//! transcript file that is not where the document says — because one voice
//! is still better than an empty pane, and the fallback marks itself by
//! having no user turns rather than by an error.

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
/// Two seconds now, down from ten: the moment the mode grew a composer this
/// stopped being a record and became a live exchange — the client polls
/// while the mode is open so replies stream in, and ten seconds of staleness
/// reads as the agent ignoring you. The cost is bounded by what this
/// protects: one ~4 MB tail read per expiry, only while a conversation is
/// actually on screen.
const CACHE_TTL: Duration = Duration::from_secs(2);

/// One thing the agent did or said.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// `"[Write] /path/to/file"` or the first line of what it said.
    pub headline: String,
    /// The rest, when there is more than one line of it.
    pub body: Option<String>,
}

/// How many turns of the transcript to show.
///
/// A chat pane, not a log viewer — the recent exchange is what you came to
/// read, and `updateCount` on a real agent here is four digits.
const TURN_LIMIT: usize = 40;

/// How much of the transcript's tail to read.
///
/// Sized by measurement, not taste: 3 MB of a real 32 MB session held 3 user
/// turns and 19 agent messages, because tool results and pasted images make
/// single lines enormous. 4 MB comfortably covers [`TURN_LIMIT`] turns of
/// real traffic while keeping the read trivial.
const TAIL_BYTES: u64 = 4 * 1024 * 1024;

/// How much of a tool's output survives into the chat.
///
/// Enough to answer "what did that command actually say", far short of a
/// build log — the transcript window itself is the bound on how much anyone
/// is reading here.
const RESULT_LIMIT_CHARS: usize = 1500;

/// One fetched conversation: when, whose, its turns, and whether the agent
/// was running at read time.
struct CachedConversation {
    at: Instant,
    agent_id: String,
    turns: Vec<Turn>,
    running: bool,
}

static CACHE: Mutex<Option<CachedConversation>> = Mutex::new(None);

/// One voice's turn in the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub speaker: Speaker,
    pub text: String,
    /// The tool's name, for `Speaker::Tool` turns; the badge on the row.
    pub tool: Option<String>,
    /// What the tool answered, when the window held its `tool_result` —
    /// joined by `tool_use_id`, truncated at [`RESULT_LIMIT_CHARS`]. Rides
    /// the row's `preview`; the client renders it on demand, because a chat
    /// where every `ls` prints its output is a terminal, not a chat.
    pub result: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    User,
    Agent,
    Tool,
}

impl Speaker {
    /// The wire value `SearchItem::speaker` carries.
    pub fn wire(self) -> &'static str {
        match self {
            Speaker::User => "user",
            Speaker::Agent => "agent",
            Speaker::Tool => "tool",
        }
    }
}

/// Where a claude agent's real transcript lives, resolved from its Paseo
/// document.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionRef {
    provider: String,
    session_id: String,
    cwd: String,
    /// `lastStatus == "running"` at read time — what drives the typing
    /// indicator. Read from the same document, so it can never be fresher
    /// than the turns beside it, which is the correct kind of stale.
    running: bool,
}

/// The one agent document, found by filename under `~/.paseo/agents/*/`.
fn session_ref(agent_id: &str) -> Option<SessionRef> {
    let home = std::env::var_os("HOME")?;
    let root = std::path::PathBuf::from(home).join(".paseo/agents");
    let workspaces = std::fs::read_dir(root).ok()?;
    for workspace in workspaces.flatten() {
        let doc_path = workspace.path().join(format!("{agent_id}.json"));
        let Ok(raw) = std::fs::read_to_string(&doc_path) else { continue };
        let Ok(doc) = serde_json::from_str::<Value>(&raw) else { continue };
        let provider = doc.get("provider").and_then(Value::as_str)?.to_string();
        let cwd = doc.get("cwd").and_then(Value::as_str)?.to_string();
        let session_id = doc
            .get("persistence")
            .and_then(|p| p.get("sessionId"))
            .and_then(Value::as_str)?
            .to_string();
        let running = doc.get("lastStatus").and_then(Value::as_str) == Some("running");
        return Some(SessionRef { provider, session_id, cwd, running });
    }
    None
}

/// Claude Code's project-directory encoding of a working directory: every
/// character outside `[A-Za-z0-9]` becomes `-`, leading slash included —
/// `/Users/nish/Documents/neko` → `-Users-nish-Documents-neko`, and a dotted
/// path like `/Users/nish/.openclaw/workspace` →
/// `-Users-nish--openclaw-workspace` (both verified against the real
/// directory listing, not inferred).
pub fn munge_cwd(cwd: &str) -> String {
    cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

fn transcript_path(session: &SessionRef) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        std::path::PathBuf::from(home)
            .join(".claude/projects")
            .join(munge_cwd(&session.cwd))
            .join(format!("{}.jsonl", session.session_id)),
    )
}

/// The last `TAIL_BYTES` of the transcript, as a string starting at a line
/// boundary.
fn read_tail(path: &std::path::Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;
    if start > 0 {
        // Sought into the middle of a line; drop the partial one.
        if let Some(nl) = buf.find('\n') {
            buf.drain(..=nl);
        }
    }
    Some(buf)
}

/// Whether a user line's text is harness noise rather than something the
/// captain typed — command caveats, slash-command echoes, injected reminders.
fn is_user_noise(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<local-command")
        || t.starts_with("<command-name")
        || t.starts_with("<system-reminder")
        || t.starts_with("Caveat:")
}

/// One line describing a tool call: the name, then the most human of its
/// arguments. The argument keys are Claude Code's own tool vocabulary,
/// probed in preference order — a `description` reads better than a raw
/// command, a `command` better than nothing.
fn tool_line(input: &Value) -> Option<String> {
    for key in ["description", "command", "file_path", "prompt", "pattern", "query", "url"] {
        if let Some(v) = input.get(key).and_then(Value::as_str) {
            let mut line = v.trim().replace('\n', " ");
            if line.chars().count() > 90 {
                line = line.chars().take(90).collect::<String>() + "\u{2026}";
            }
            return Some(line);
        }
    }
    None
}

/// Parses the transcript tail into turns, keeping the last [`TURN_LIMIT`].
///
/// Pure over the string, so the extraction rules — the ones that decide what
/// counts as the captain speaking — are pinned without a 32 MB fixture:
/// a `user` line whose content carries a `tool_result` block is a tool
/// answer, not a human turn; sidechain and meta lines belong to subagents
/// and the harness; an image block becomes an `[image]` marker rather than
/// vanishing.
pub fn parse_transcript_tail(tail: &str) -> Vec<Turn> {
    // **First pass: the answers.** Tool results arrive as later `user` lines
    // joined by `tool_use_id`, so attaching them to their calls needs the
    // whole window before any turn is built.
    let mut results: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in tail.lines() {
        let Ok(doc) = serde_json::from_str::<Value>(line.trim()) else { continue };
        let Some(Value::Array(blocks)) = doc.get("message").and_then(|m| m.get("content")) else {
            continue;
        };
        for block in blocks {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else { continue };
            let text = match block.get("content") {
                Some(Value::String(text)) => text.clone(),
                Some(Value::Array(parts)) => parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => continue,
            };
            let mut text = text.trim().to_string();
            if text.chars().count() > RESULT_LIMIT_CHARS {
                text = text.chars().take(RESULT_LIMIT_CHARS).collect::<String>() + "\u{2026}";
            }
            if !text.is_empty() {
                results.insert(id.to_string(), text);
            }
        }
    }

    let mut turns = Vec::new();
    // Contiguous sidechain traffic — a Task subagent working — collapses to
    // one chip counting its steps. The uuid chains that would attribute each
    // step to its exact Task call are real further work; a count in the right
    // *place* (sidechain lines sit next to the Task call that spawned them)
    // is honest without it, where a wrong attribution would not be.
    let mut sidechain_steps: usize = 0;
    let flush_sidechain = |turns: &mut Vec<Turn>, steps: &mut usize| {
        if *steps > 0 {
            turns.push(Turn {
                speaker: Speaker::Tool,
                text: format!(
                    "{} step{}",
                    steps,
                    if *steps == 1 { "" } else { "s" }
                ),
                tool: Some("Subagent".to_string()),
                result: None,
            });
            *steps = 0;
        }
    };
    for line in tail.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(doc) = serde_json::from_str::<Value>(line) else { continue };
        if doc.get("isSidechain").and_then(Value::as_bool).unwrap_or(false) {
            // Only the subagent's own moves count as steps; its incoming
            // tool results would double every one.
            if doc.get("type").and_then(Value::as_str) == Some("assistant") {
                sidechain_steps += 1;
            }
            continue;
        }
        if doc.get("isMeta").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let Some(message) = doc.get("message") else { continue };
        match doc.get("type").and_then(Value::as_str) {
            Some("user") => {
                let text = match message.get("content") {
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Array(blocks)) => {
                        if blocks.iter().any(|b| {
                            b.get("type").and_then(Value::as_str) == Some("tool_result")
                        }) {
                            continue;
                        }
                        let mut text = String::new();
                        for block in blocks {
                            match block.get("type").and_then(Value::as_str) {
                                Some("text") => {
                                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                                        if !text.is_empty() {
                                            text.push('\n');
                                        }
                                        text.push_str(t);
                                    }
                                }
                                Some("image") => {
                                    if !text.is_empty() {
                                        text.push('\n');
                                    }
                                    text.push_str("[image]");
                                }
                                _ => {}
                            }
                        }
                        text
                    }
                    _ => continue,
                };
                let text = text.trim().to_string();
                if text.is_empty() || is_user_noise(&text) {
                    continue;
                }
                flush_sidechain(&mut turns, &mut sidechain_steps);
                turns.push(Turn { speaker: Speaker::User, text, tool: None, result: None });
            }
            Some("assistant") => {
                let Some(Value::Array(blocks)) = message.get("content") else { continue };
                for block in blocks {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let text = block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .trim()
                                .to_string();
                            if !text.is_empty() {
                                flush_sidechain(&mut turns, &mut sidechain_steps);
                                turns.push(Turn {
                                    speaker: Speaker::Agent,
                                    text,
                                    tool: None,
                                    result: None,
                                });
                            }
                        }
                        Some("tool_use") => {
                            flush_sidechain(&mut turns, &mut sidechain_steps);
                            let name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("tool")
                                .to_string();
                            let text = block
                                .get("input")
                                .and_then(tool_line)
                                .unwrap_or_default();
                            let result = block
                                .get("id")
                                .and_then(Value::as_str)
                                .and_then(|id| results.get(id).cloned());
                            turns.push(Turn {
                                speaker: Speaker::Tool,
                                text,
                                tool: Some(name),
                                result,
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    flush_sidechain(&mut turns, &mut sidechain_steps);
    if turns.len() > TURN_LIMIT {
        turns.drain(..turns.len() - TURN_LIMIT);
    }
    turns
}

/// The activity-feed fallback's entries, as one-voice turns.
fn turns_from_entries(entries: Vec<Entry>) -> Vec<Turn> {
    entries
        .into_iter()
        // The feed arrives newest-first; a chat reads oldest-first.
        .rev()
        .map(|entry| match split_tool(&entry.headline) {
            Some((tool, argument)) => Turn {
                speaker: Speaker::Tool,
                text: argument.to_string(),
                tool: Some(tool.to_string()),
                result: None,
            },
            None => Turn {
                speaker: Speaker::Agent,
                text: match &entry.body {
                    Some(body) => format!("{}\n\n{body}", entry.headline),
                    None => entry.headline.clone(),
                },
                tool: None,
                result: None,
            },
        })
        .collect()
}

/// Every turn for `agent_id`: the real transcript when the agent is claude
/// and the file is where its document says, the activity feed otherwise.
fn fetch_turns(agent_id: &str) -> (Vec<Turn>, bool) {
    let session = session_ref(agent_id);
    let running = session.as_ref().is_some_and(|s| s.running);
    if let Some(session) = &session
        && session.provider == "claude"
        && let Some(path) = transcript_path(session)
        && let Some(tail) = read_tail(&path)
    {
        let turns = parse_transcript_tail(&tail);
        if !turns.is_empty() {
            return (turns, running);
        }
    }
    let Ok(client) = McpClient::discover() else { return (Vec::new(), running) };
    (turns_from_entries(fetch(&client, agent_id).unwrap_or_default()), running)
}


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
        let (turns, running) = {
            let cached = CACHE
                .lock()
                .unwrap()
                .as_ref()
                .filter(|c| c.at.elapsed() < CACHE_TTL && c.agent_id == agent_id)
                .map(|c| (c.turns.clone(), c.running));
            match cached {
                Some(hit) => hit,
                None => {
                    let (fresh, running) = fetch_turns(agent_id);
                    *CACHE.lock().unwrap() = Some(CachedConversation {
                        at: Instant::now(),
                        agent_id: agent_id.to_string(),
                        turns: fresh.clone(),
                        running,
                    });
                    (fresh, running)
                }
            }
        };

        let count = turns.len();
        turns
            .iter()
            .enumerate()
            .map(|(rank, turn)| {
                let first_line =
                    turn.text.lines().next().unwrap_or_default().trim().to_string();
                Candidate {
                    // **Chronological, oldest first** — a chat reads downward
                    // into the present. The daemon's scoped branch sorts by
                    // score descending, so the oldest turn takes the highest.
                    score: (count - rank) as f32,
                    item: SearchItem {
                        // The index, because two identical shell commands are
                        // genuinely two different turns and
                        // `resolve_selection` follows `(kind, id)`.
                        id: format!("{agent_id}#{rank}"),
                        kind: "conversation".to_string(),
                        title: if first_line.is_empty() {
                            turn.tool.clone().unwrap_or_else(|| "\u{2026}".to_string())
                        } else {
                            first_line
                        },
                        subtitle: None,
                        icon: Icon::Glyph(Glyph::Text),
                        section_label: "Conversation".to_string(),
                        // Nothing here is an action. Enter on a turn of a
                        // transcript has no meaning, and inventing one would
                        // be guessing at intent on a read-only surface.
                        action_label: "Reading".to_string(),
                        badge: turn.tool.as_ref().map(|t| t.to_uppercase()),
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
                        // A turn's own words render as markdown; a tool call
                        // is a typed line, not prose.
                        preview_markdown: turn.speaker != Speaker::Tool,
                        speaker: Some(turn.speaker.wire().to_string()),
                        // A tool turn's preview is what the tool *answered* —
                        // shown on demand — never its argument, which the
                        // title already carries. `None` marks a chip with
                        // nothing to expand.
                        preview: match turn.speaker {
                            Speaker::Tool => turn.result.clone(),
                            _ => Some(turn.text.clone()),
                        },
                    },
                }
            })
            .chain(running.then(|| {
                // **The typing indicator, as a row.** The provider states
                // "the agent is composing" through the same vocabulary every
                // other fact travels in; what a composing agent looks like is
                // entirely the client's. Scored below every real turn so it
                // is always last, exactly where a typing bubble sits.
                Candidate {
                    score: 0.5,
                    item: SearchItem {
                        id: format!("{agent_id}#working"),
                        kind: "conversation".to_string(),
                        title: "\u{2026}".to_string(),
                        subtitle: None,
                        icon: Icon::Glyph(Glyph::Text),
                        section_label: "Conversation".to_string(),
                        action_label: "Reading".to_string(),
                        badge: None,
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        // Nothing to do to a typing bubble.
                        actions: Vec::new(),
                        source: None,
                        meter: None,
                        keeps_open: true,
                        preview_markdown: false,
                        speaker: Some("working".to_string()),
                        preview: None,
                    },
                }
            }))
            .collect()
    }

    /// Enter does nothing, deliberately — a transcript line is something to
    /// read, not something to run.
    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        Ok(())
    }

    /// The composer's send: `id` is the **agent** (the mode's subject, no
    /// `#rank`), `query` is what was typed.
    ///
    /// A turn row's own Enter still arrives here too — with its `agent#rank`
    /// id — and stays a no-op, because a transcript line is something to
    /// read. The two are distinguished by the id's shape, which is the same
    /// convention `perform_action` already reads it by.
    fn activate_with_query(&self, id: &str, query: &str) -> Result<(), ProviderError> {
        if id.contains('#') {
            return Ok(());
        }
        let prompt = query.trim();
        if prompt.is_empty() {
            return Err(ProviderError("type a message first".to_string()));
        }
        let client = McpClient::discover()
            .map_err(|e| ProviderError(e.to_string()))?;
        client
            .call("send_agent_prompt", crate::agents::send_prompt_arguments(id, prompt))
            .map_err(|e| ProviderError(e.to_string()))?;
        // The sent message lands in the transcript the moment the harness
        // writes it; a cache serving the pre-send read for another two
        // seconds would make the send look swallowed.
        *CACHE.lock().unwrap() = None;
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

    #[test]
    fn a_tools_answer_is_joined_to_its_call_by_id() {
        let tail = jsonl(&[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"call-1","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"call-1","content":"src\nlib.rs"}]}}"#,
        ]);
        let turns = parse_transcript_tail(&tail);
        assert_eq!(turns.len(), 1, "the result attaches, it does not add a turn");
        assert_eq!(turns[0].result.as_deref(), Some("src\nlib.rs"));
    }

    #[test]
    fn a_huge_tool_answer_is_truncated_not_carried_whole() {
        let big = "x".repeat(RESULT_LIMIT_CHARS * 3);
        let tail = jsonl(&[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"c","name":"Bash","input":{"command":"cat log"}}]}}"#,
            &format!(r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"c","content":"{big}"}}]}}}}"#),
        ]);
        let turns = parse_transcript_tail(&tail);
        let result = turns[0].result.as_ref().unwrap();
        assert!(result.chars().count() <= RESULT_LIMIT_CHARS + 1, "bounded, ellipsis included");
    }

    #[test]
    fn a_sidechain_run_collapses_to_one_counted_chip_in_its_place() {
        // A Task subagent's own traffic sits between the Task call and the
        // agent's next words. One chip counting its moves is honest; per-step
        // attribution needs the uuid chains and is deliberately not guessed.
        let tail = jsonl(&[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":"Task","input":{"description":"audit the call sites"}}]}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"step"}]}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"step"}]}}"#,
            r#"{"type":"user","isSidechain":true,"message":{"content":[{"type":"tool_result","tool_use_id":"x","content":"noise"}]}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"step"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"done"}]}}"#,
        ]);
        let turns = parse_transcript_tail(&tail);
        let shape: Vec<(&str, Option<&str>)> =
            turns.iter().map(|t| (t.text.as_str(), t.tool.as_deref())).collect();
        assert_eq!(
            shape,
            vec![
                ("audit the call sites", Some("Task")),
                ("3 steps", Some("Subagent")),
                ("done", None),
            ],
            "the subagent's incoming results are not counted as its steps"
        );
    }


    /// Real line shapes from a live session file, values swapped for fixtures.
    fn jsonl(lines: &[&str]) -> String {
        lines.join("\n")
    }

    #[test]
    fn a_transcript_becomes_both_voices_in_order() {
        let tail = jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"make the logo bigger"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done - it is **2x** now."},{"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"ship it"}]}}"#,
        ]);
        let turns = parse_transcript_tail(&tail);
        let shape: Vec<(Speaker, &str)> =
            turns.iter().map(|t| (t.speaker, t.text.as_str())).collect();
        assert_eq!(
            shape,
            vec![
                (Speaker::User, "make the logo bigger"),
                (Speaker::Agent, "Done - it is **2x** now."),
                (Speaker::Tool, "cargo test"),
                (Speaker::User, "ship it"),
            ]
        );
        assert_eq!(turns[2].tool.as_deref(), Some("Bash"));
    }

    #[test]
    fn a_tool_result_is_not_the_captain_speaking() {
        // Claude Code files tool results as `user` lines; rendering one as a
        // user bubble would put the output of `ls` in the captain's mouth.
        let tail = jsonl(&[
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"x","content":"src lib.rs"}]}}"#,
        ]);
        assert!(parse_transcript_tail(&tail).is_empty());
    }

    #[test]
    fn sidechain_meta_and_harness_noise_stay_out_of_the_chat() {
        let tail = jsonl(&[
            r#"{"type":"user","isSidechain":true,"message":{"content":"subagent chatter"}}"#,
            r#"{"type":"user","isMeta":true,"message":{"content":"meta line"}}"#,
            r#"{"type":"user","message":{"content":"<local-command-caveat>...</local-command-caveat>"}}"#,
            r#"{"type":"user","message":{"content":"Caveat: the messages below were generated..."}}"#,
            r#"{"type":"user","message":{"content":"a real question"}}"#,
        ]);
        let turns = parse_transcript_tail(&tail);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].text, "a real question");
    }

    #[test]
    fn an_image_leaves_a_marker_rather_than_vanishing() {
        // "look at this [image]" with the image dropped silently would read
        // as the captain pointing at nothing.
        let tail = jsonl(&[
            r#"{"type":"user","message":{"content":[{"type":"text","text":"look at this"},{"type":"image","source":{}}]}}"#,
        ]);
        let turns = parse_transcript_tail(&tail);
        assert_eq!(turns[0].text, "look at this\n[image]");
    }

    #[test]
    fn only_the_last_turn_limit_turns_survive() {
        let lines: Vec<String> = (0..60)
            .map(|i| format!(r#"{{"type":"user","message":{{"content":"turn {i}"}}}}"#))
            .collect();
        let tail = lines.join("\n");
        let turns = parse_transcript_tail(&tail);
        assert_eq!(turns.len(), TURN_LIMIT);
        assert_eq!(turns.last().unwrap().text, "turn 59", "the newest survive");
    }

    #[test]
    fn the_cwd_munge_matches_claude_codes_own_directory_names() {
        // Both verified against the real ~/.claude/projects listing.
        assert_eq!(munge_cwd("/Users/nish/Documents/neko"), "-Users-nish-Documents-neko");
        assert_eq!(munge_cwd("/Users/nish/.openclaw/workspace"), "-Users-nish--openclaw-workspace");
    }

    #[test]
    fn a_tool_call_line_prefers_the_most_human_argument() {
        let input = serde_json::json!({"command": "cargo test -p neko", "description": "Run the tests"});
        assert_eq!(tool_line(&input).as_deref(), Some("Run the tests"));
        let bare = serde_json::json!({"file_path": "/a/b.rs"});
        assert_eq!(tool_line(&bare).as_deref(), Some("/a/b.rs"));
    }

    #[test]
    fn the_activity_fallback_reads_oldest_first_like_the_transcript() {
        // The feed arrives newest-first; the chat reads downward into the
        // present, so the fallback must flip it.
        let entries = vec![
            Entry { headline: "newest prose".into(), body: None },
            Entry { headline: "[Shell] older command".into(), body: None },
        ];
        let turns = turns_from_entries(entries);
        assert_eq!(turns[0].speaker, Speaker::Tool);
        assert_eq!(turns[1].text, "newest prose");
    }


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

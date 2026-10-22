//! Neko's chat in the AI SDK's vocabulary (`docs/design/chat-on-ai-sdk.md`).
//!
//! A message is a list of `UIMessage` *parts*; a reply in flight is a stream of
//! `UIMessageChunk`s. This module is the whole of that vocabulary on the Rust
//! side and nothing else knows it:
//!
//! * [`apply_chunk`] folds one chunk into a part list (the same reduction the
//!   SDK's client runs, so a replayed message equals the streamed one);
//! * [`ReplyStream`] is the one place a model's prose is searched for the
//!   `neko-*` blocks it may write, turning a complete valid block into a typed
//!   `data-neko-*` part (or, for proposals, into [`Proposals`]);
//! * the hub (`begin`/`emit`/`attach`/`finish`) holds the replies in flight:
//!   their parts, and the subscribers (browser windows) following them.
//!
//! Streaming deltas touch memory and the subscribers only. The database sees a
//! reply's parts at most every [`PERSIST_EVERY`] (deferred) and at every state
//! change that matters, so a long reply never rewrites the chat row per token.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// Text and reasoning one message may hold; later deltas are dropped.
pub const MAX_TEXT_BYTES: usize = 192 * 1024;
/// Characters of a tool's output kept on the part.
pub const TOOL_OUTPUT_CHARS: usize = 6000;
/// Characters of any one string inside a tool's input kept on the part.
pub const TOOL_INPUT_STRING_CHARS: usize = 2000;
/// The most one stored message may take (serialised parts).
pub const MAX_STORED_BYTES: usize = 96 * 1024;
/// How often the parts of a reply in flight are written (deferred).
pub const PERSIST_EVERY: Duration = Duration::from_secs(2);

/// `data-neko-*` kinds drawn in the reply.
pub const DISPLAY_KINDS: [&str; 6] = ["chart", "choices", "form", "plan", "files", "sources"];
/// Blocks that carry what the reply proposes; consumed, never drawn.
pub const PROPOSAL_KINDS: [&str; 5] = ["ticket", "remember", "watch", "action", "used-memory"];

fn bound(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit { return text.to_owned(); }
    let mut out: String = text.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A tool's input with every long string shortened, so a `Write` of a whole
/// file does not live in the chat row.
pub fn bounded_input(value: &Value) -> Value {
    match value {
        Value::String(s) => Value::String(bound(s, TOOL_INPUT_STRING_CHARS)),
        Value::Array(items) => Value::Array(items.iter().take(64).map(bounded_input).collect()),
        Value::Object(map) => Value::Object(map.iter().take(64).map(|(k, v)| (k.clone(), bounded_input(v))).collect()),
        other => other.clone(),
    }
}

pub fn bounded_output(text: &str) -> String { bound(text, TOOL_OUTPUT_CHARS) }

// ---------------------------------------------------------------------------
// Chunk constructors
// ---------------------------------------------------------------------------

pub fn text_chunks(id: &str, text: &str) -> Vec<Value> {
    vec![json!({"type": "text-start", "id": id}), json!({"type": "text-delta", "id": id, "delta": text}), json!({"type": "text-end", "id": id})]
}

pub fn tool_input(call_id: &str, name: &str, input: &Value) -> Value {
    json!({"type": "tool-input-available", "toolCallId": call_id, "toolName": name, "input": bounded_input(input), "dynamic": true})
}

pub fn tool_output(call_id: &str, output: Value) -> Value {
    json!({"type": "tool-output-available", "toolCallId": call_id, "output": output, "dynamic": true})
}

pub fn tool_error(call_id: &str, text: &str) -> Value {
    json!({"type": "tool-output-error", "toolCallId": call_id, "errorText": bound(text, 400), "dynamic": true})
}

// ---------------------------------------------------------------------------
// Folding chunks into parts
// ---------------------------------------------------------------------------

fn last_streaming<'a>(parts: &'a mut [Value], kind: &str) -> Option<&'a mut Value> {
    parts.iter_mut().rev().find(|p| p["type"] == kind && p["state"] == "streaming")
}

fn tool_part<'a>(parts: &'a mut [Value], call_id: &str) -> Option<&'a mut Value> {
    parts.iter_mut().find(|p| p["type"] == "dynamic-tool" && p["toolCallId"] == call_id)
}

/// Folds one chunk into a message's parts. Chunks that are not parts (`start`,
/// `finish`, `abort`, `error`, metadata) change nothing here.
pub fn apply_chunk(parts: &mut Vec<Value>, chunk: &Value) {
    let Some(kind) = chunk["type"].as_str() else { return };
    let call = chunk["toolCallId"].as_str().unwrap_or("");
    match kind {
        "start-step" => parts.push(json!({"type": "step-start"})),
        "text-start" => parts.push(json!({"type": "text", "text": "", "state": "streaming"})),
        "reasoning-start" => parts.push(json!({"type": "reasoning", "text": "", "state": "streaming"})),
        "text-delta" | "reasoning-delta" => {
            let which = if kind == "text-delta" { "text" } else { "reasoning" };
            if let Some(part) = last_streaming(parts, which) {
                let mut text = part["text"].as_str().unwrap_or("").to_owned();
                text.push_str(chunk["delta"].as_str().unwrap_or(""));
                part["text"] = Value::String(text);
            }
        }
        "text-end" | "reasoning-end" => {
            let which = if kind == "text-end" { "text" } else { "reasoning" };
            if let Some(part) = last_streaming(parts, which) { part["state"] = json!("done"); }
        }
        "tool-input-start" => parts.push(json!({"type": "dynamic-tool", "toolName": chunk["toolName"], "toolCallId": call, "state": "input-streaming", "input": null})),
        "tool-input-available" => {
            if let Some(part) = tool_part(parts, call) {
                part["state"] = json!("input-available");
                part["input"] = chunk["input"].clone();
            } else {
                parts.push(json!({"type": "dynamic-tool", "toolName": chunk["toolName"], "toolCallId": call, "state": "input-available", "input": chunk["input"]}));
            }
            for key in ["title", "toolMetadata"] {
                if let Some(value) = chunk.get(key).filter(|v| !v.is_null()) {
                    if let Some(part) = tool_part(parts, call) { part[key] = value.clone(); }
                }
            }
        }
        "tool-approval-request" => {
            if let Some(part) = tool_part(parts, call) {
                part["state"] = json!("approval-requested");
                let mut approval = json!({"id": chunk["approvalId"]});
                if let Some(reason) = chunk.get("reason").filter(|r| r.is_string()) { approval["requestReason"] = reason.clone(); }
                part["approval"] = approval;
            }
        }
        "tool-approval-response" => {
            let id = chunk["approvalId"].as_str().unwrap_or("");
            if let Some(part) = parts.iter_mut().find(|p| p["type"] == "dynamic-tool" && p["approval"]["id"] == id) {
                part["state"] = json!("approval-responded");
                part["approval"]["approved"] = chunk["approved"].clone();
            }
        }
        "tool-output-available" => {
            if let Some(part) = tool_part(parts, call) {
                part["state"] = json!("output-available");
                part["output"] = chunk["output"].clone();
            }
        }
        "tool-output-error" => {
            if let Some(part) = tool_part(parts, call) {
                part["state"] = json!("output-error");
                part["errorText"] = chunk["errorText"].clone();
            }
        }
        "tool-output-denied" => {
            if let Some(part) = tool_part(parts, call) { part["state"] = json!("output-denied"); }
        }
        "file" => parts.push(json!({"type": "file", "url": chunk["url"], "mediaType": chunk["mediaType"]})),
        k if k.starts_with("data-") => {
            let id = chunk.get("id").and_then(Value::as_str);
            if let Some(id) = id {
                if let Some(part) = parts.iter_mut().find(|p| p["type"] == kind && p["id"] == id) {
                    part["data"] = chunk["data"].clone();
                    return;
                }
            }
            let mut part = json!({"type": kind, "data": chunk["data"]});
            if let Some(id) = id { part["id"] = json!(id); }
            parts.push(part);
        }
        _ => {}
    }
}

/// The chunks that rebuild `parts` on a client that has none of them. A text or
/// reasoning part still streaming keeps the id its live deltas will carry.
pub fn replay_chunks(parts: &[Value], open_text: Option<&str>, open_reasoning: Option<&str>) -> Vec<Value> {
    let mut out = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let kind = part["type"].as_str().unwrap_or("");
        match kind {
            "step-start" => out.push(json!({"type": "start-step"})),
            "text" | "reasoning" => {
                let streaming = part["state"] == "streaming";
                let id = match (streaming, kind) {
                    (true, "text") => open_text.map(str::to_owned),
                    (true, _) => open_reasoning.map(str::to_owned),
                    _ => None,
                }.unwrap_or_else(|| format!("r{index}"));
                out.push(json!({"type": format!("{kind}-start"), "id": id}));
                if part["text"].as_str().is_some_and(|t| !t.is_empty()) {
                    out.push(json!({"type": format!("{kind}-delta"), "id": id, "delta": part["text"]}));
                }
                if !streaming { out.push(json!({"type": format!("{kind}-end"), "id": id})); }
            }
            "dynamic-tool" => {
                let call = part["toolCallId"].as_str().unwrap_or("");
                let name = part["toolName"].as_str().unwrap_or("");
                match part["state"].as_str().unwrap_or("") {
                    "input-streaming" => out.push(json!({"type": "tool-input-start", "toolCallId": call, "toolName": name, "dynamic": true})),
                    state => {
                        let mut input = json!({"type": "tool-input-available", "toolCallId": call, "toolName": name, "input": part["input"], "dynamic": true});
                        for key in ["title", "toolMetadata"] {
                            if let Some(value) = part.get(key) { input[key] = value.clone(); }
                        }
                        out.push(input);
                        let approval = &part["approval"];
                        match state {
                            "approval-requested" => out.push(json!({"type": "tool-approval-request", "toolCallId": call, "approvalId": approval["id"]})),
                            "approval-responded" => {
                                out.push(json!({"type": "tool-approval-request", "toolCallId": call, "approvalId": approval["id"]}));
                                out.push(json!({"type": "tool-approval-response", "approvalId": approval["id"], "approved": approval["approved"]}));
                            }
                            "output-available" => out.push(json!({"type": "tool-output-available", "toolCallId": call, "output": part["output"], "dynamic": true})),
                            "output-error" => out.push(json!({"type": "tool-output-error", "toolCallId": call, "errorText": part["errorText"], "dynamic": true})),
                            "output-denied" => {
                                out.push(json!({"type": "tool-approval-request", "toolCallId": call, "approvalId": approval["id"]}));
                                out.push(json!({"type": "tool-approval-response", "approvalId": approval["id"], "approved": false}));
                                out.push(json!({"type": "tool-output-denied", "toolCallId": call}));
                            }
                            _ => {}
                        }
                    }
                }
            }
            "file" => out.push(json!({"type": "file", "url": part["url"], "mediaType": part["mediaType"]})),
            k if k.starts_with("data-") => {
                let mut chunk = json!({"type": k, "data": part["data"]});
                if let Some(id) = part.get("id") { chunk["id"] = id.clone(); }
                out.push(chunk);
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The blocks a model may write in its prose
// ---------------------------------------------------------------------------

/// What a reply proposed, in order, as the JSON the model wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Proposals {
    pub blocks: Vec<(String, Value)>,
}

const MARKER: &str = "```neko-";

/// Reads a model's text as it streams. Prose passes through; a *complete* block
/// ```` ```neko-<kind> ```` whose body is valid JSON becomes a `data-neko-<kind>`
/// chunk (display kinds) or a [`Proposals`] entry (proposal kinds). A block that
/// is still open is held back, so half a fence never reaches the screen; one
/// that never closes, or is not valid JSON, stays what the model wrote.
#[derive(Debug, Default)]
pub struct ReplyStream {
    raw: String,
    emitted: usize,
    open: Option<String>,
    counter: u32,
    pub proposals: Proposals,
}

impl ReplyStream {
    pub fn new() -> Self { Self::default() }

    /// The id the currently open text part carries (live deltas must match it).
    pub fn open_id(&self) -> Option<&str> { self.open.as_deref() }

    /// A new upstream text block starts.
    pub fn begin(&mut self) -> Vec<Value> {
        let mut out = self.drain(true);
        self.raw.clear();
        self.emitted = 0;
        out.extend(self.close());
        out
    }

    pub fn delta(&mut self, text: &str) -> Vec<Value> {
        self.raw.push_str(text);
        self.drain(false)
    }

    /// The upstream text block ended: release what was held.
    pub fn end(&mut self) -> Vec<Value> {
        let mut out = self.drain(true);
        out.extend(self.close());
        self.raw.clear();
        self.emitted = 0;
        out
    }

    /// A whole text at once (a runtime that does not stream).
    pub fn whole(&mut self, text: &str) -> Vec<Value> {
        let mut out = self.begin();
        out.extend(self.delta(text));
        out.extend(self.end());
        out
    }

    fn close(&mut self) -> Vec<Value> {
        match self.open.take() {
            Some(id) => vec![json!({"type": "text-end", "id": id})],
            None => Vec::new(),
        }
    }

    fn text(&mut self, text: &str, out: &mut Vec<Value>) {
        let mut text = text;
        if self.open.is_none() {
            text = text.trim_start();
            if text.is_empty() { return; }
            self.counter += 1;
            let id = format!("t{}", self.counter);
            out.push(json!({"type": "text-start", "id": id}));
            self.open = Some(id);
        }
        if text.is_empty() { return; }
        out.push(json!({"type": "text-delta", "id": self.open.clone().unwrap_or_default(), "delta": text}));
    }

    fn drain(&mut self, last: bool) -> Vec<Value> {
        let mut out = Vec::new();
        loop {
            let region = &self.raw[self.emitted..];
            let Some(at) = region.find(MARKER) else {
                // Hold a trailing piece that may still become the marker, and
                // trailing whitespace (it belongs to whatever comes next).
                let mut end = region.len();
                if !last {
                    for len in (1..MARKER.len()).rev() {
                        if len <= region.len() && region.is_char_boundary(region.len() - len) && MARKER.as_bytes()[..len] == region.as_bytes()[region.len() - len..] {
                            end = region.len() - len;
                            break;
                        }
                    }
                }
                let safe = region[..end].trim_end().to_owned();
                self.emitted += safe.len();
                if last { self.emitted = self.raw.len(); }
                self.text(&safe, &mut out);
                return out;
            };
            // Prose before the block.
            let before = region[..at].trim_end().to_owned();
            let from = self.emitted + at;
            let block = &self.raw[from..];
            // Header runs to the end of the line: "```neko-choices" or with inline JSON.
            let Some(newline) = block.find('\n') else {
                if last { let all = block.to_owned(); self.text(&before, &mut out); self.text(&format!("\n\n{all}"), &mut out); self.emitted = self.raw.len(); } else {
                    // Emit the prose before it; hold the open header.
                    self.text(&before, &mut out);
                    self.emitted = from;
                }
                return out;
            };
            let header = block[3..newline].trim().to_owned();
            let Some(close) = block[newline..].find("\n```") else {
                if last {
                    let all = block.to_owned();
                    self.text(&before, &mut out);
                    self.text(&format!("\n\n{all}"), &mut out);
                    self.emitted = self.raw.len();
                } else {
                    self.text(&before, &mut out);
                    self.emitted = from;
                }
                return out;
            };
            let (kind, inline) = header.split_once(char::is_whitespace).map_or((header.as_str(), ""), |(k, rest)| (k, rest.trim()));
            let kind = kind.strip_prefix("neko-").unwrap_or(kind).to_owned();
            let body_start = newline + 1;
            let body_end = newline + close;
            let body = format!("{inline}\n{}", if body_start <= body_end { &block[body_start..body_end] } else { "" });
            let fence_end = newline + close + 4;
            let whole = block[..fence_end].to_owned();
            let value = serde_json::from_str::<Value>(body.trim()).ok().filter(Value::is_object);
            self.text(&before, &mut out);
            self.emitted = from + fence_end;
            match value {
                Some(value) if DISPLAY_KINDS.contains(&kind.as_str()) => {
                    out.extend(self.close());
                    out.push(json!({"type": format!("data-neko-{kind}"), "data": value}));
                }
                Some(value) if PROPOSAL_KINDS.contains(&kind.as_str()) => self.proposals.blocks.push((kind, value)),
                _ if PROPOSAL_KINDS.contains(&kind.as_str()) => {} // an invalid proposal is dropped, never shown
                _ => self.text(&format!("\n\n{whole}"), &mut out),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The hub: replies in flight
// ---------------------------------------------------------------------------

struct Live {
    parts: Vec<Value>,
    stream: ReplyStream,
    subscribers: Vec<Sender<Value>>,
    started_ms: i64,
    open_reasoning: Option<String>,
    text_bytes: usize,
    persisted: Instant,
    dirty: bool,
}

fn hub() -> MutexGuard<'static, HashMap<String, Live>> {
    static HUB: OnceLock<Mutex<HashMap<String, Live>>> = OnceLock::new();
    HUB.get_or_init(Mutex::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn start_chunk(id: &str, started_ms: i64, at_ms: i64) -> Value {
    json!({"type": "start", "messageId": id, "messageMetadata": {"atMs": at_ms, "startedMs": started_ms, "state": "pending"}})
}

/// A reply starts. Anything it already holds (parts written before the stream)
/// is kept.
pub fn begin(id: &str, parts: Vec<Value>, started_ms: i64) {
    hub().insert(id.to_owned(), Live {
        parts, stream: ReplyStream::new(), subscribers: Vec::new(), started_ms,
        open_reasoning: None, text_bytes: 0, persisted: Instant::now(), dirty: false,
    });
}

pub fn is_live(id: &str) -> bool { hub().contains_key(id) }

/// The parts of a reply in flight, fresher than what the database holds.
pub fn live_parts(id: &str) -> Option<Vec<Value>> { hub().get(id).map(|l| l.parts.clone()) }

fn publish(live: &mut Live, chunk: Value) {
    apply_chunk(&mut live.parts, &chunk);
    live.dirty = true;
    live.subscribers.retain(|s| s.send(chunk.clone()).is_ok());
}

/// Feeds one agent chunk to a reply in flight (text goes through the block
/// reader). Returns the parts to write when a write is due.
pub fn emit(id: &str, chunk: &Value) -> Option<Vec<Value>> {
    let mut hub = hub();
    let live = hub.get_mut(id)?;
    let kind = chunk["type"].as_str().unwrap_or("");
    match kind {
        "text-start" => for c in live.stream.begin() { publish(live, c); },
        "text-delta" => {
            let delta = chunk["delta"].as_str().unwrap_or("");
            if live.text_bytes >= MAX_TEXT_BYTES { return None; }
            live.text_bytes += delta.len();
            for c in live.stream.delta(delta) { publish(live, c); }
        }
        "text-end" => for c in live.stream.end() { publish(live, c); },
        "reasoning-start" => {
            let reasoning_id = chunk["id"].as_str().unwrap_or("reasoning").to_owned();
            live.open_reasoning = Some(reasoning_id);
            publish(live, chunk.clone());
        }
        "reasoning-delta" => {
            if live.text_bytes >= MAX_TEXT_BYTES { return None; }
            live.text_bytes += chunk["delta"].as_str().map_or(0, str::len);
            publish(live, chunk.clone());
        }
        "reasoning-end" => { live.open_reasoning = None; publish(live, chunk.clone()); }
        _ => publish(live, chunk.clone()),
    }
    if live.dirty && live.persisted.elapsed() >= PERSIST_EVERY {
        live.persisted = Instant::now();
        live.dirty = false;
        return Some(live.parts.clone());
    }
    None
}

/// A runtime that never streamed: the answer becomes the reply's text.
pub fn emit_whole_text(id: &str, text: &str) {
    let mut hub = hub();
    let Some(live) = hub.get_mut(id) else { return };
    for c in live.stream.whole(text) { publish(live, c); }
}

/// Whether the reply holds any text yet.
pub fn has_text(id: &str) -> bool {
    hub().get(id).is_some_and(|l| l.parts.iter().any(|p| p["type"] == "text"))
}

/// Takes what a reply proposed.
pub fn take_proposals(id: &str) -> Proposals {
    hub().get_mut(id).map(|l| std::mem::take(&mut l.stream.proposals)).unwrap_or_default()
}

/// A follower: the chunks that rebuild the reply so far, then every later one.
/// The receiver closes when the reply finishes.
pub struct Feed {
    pub replay: Vec<Value>,
    pub live: Receiver<Value>,
}

pub fn attach(id: &str, at_ms: i64) -> Option<Feed> {
    let mut hub = hub();
    let live = hub.get_mut(id)?;
    let (sender, receiver) = channel();
    let mut replay = vec![start_chunk(id, live.started_ms, at_ms)];
    replay.extend(replay_chunks(&live.parts, live.stream.open_id(), live.open_reasoning.as_deref()));
    live.subscribers.push(sender);
    Some(Feed { replay, live: receiver })
}

/// The reply is over: followers get the closing chunk and the hub forgets it.
/// Returns the final parts (open text closed) for the caller to store.
pub fn finish(id: &str, state: &str, at_ms: i64) -> Option<Vec<Value>> {
    let mut hub = hub();
    let mut live = hub.remove(id)?;
    for c in live.stream.end() { publish(&mut live, c); }
    for part in &mut live.parts {
        if matches!(part["type"].as_str(), Some("text" | "reasoning")) && part["state"] == "streaming" { part["state"] = json!("done"); }
    }
    let closing = json!({"type": "finish", "messageMetadata": {"atMs": at_ms, "startedMs": live.started_ms, "state": state}});
    for s in &live.subscribers { let _ = s.send(closing.clone()); }
    Some(live.parts)
}

/// `publish_to` for several chunks; whether the reply was in flight.
pub fn publish_all(id: &str, chunks: &[Value]) -> bool {
    let mut hub = hub();
    let Some(live) = hub.get_mut(id) else { return false };
    for chunk in chunks { publish(live, chunk.clone()); }
    true
}

/// Keeps a stored message under [`MAX_STORED_BYTES`]: tool inputs and outputs
/// go first (the call and its outcome stay), then the longest text is cut.
pub fn trim_parts(parts: &mut [Value]) {
    let size = |parts: &[Value]| parts.iter().map(|p| p.to_string().len()).sum::<usize>();
    if size(parts) <= MAX_STORED_BYTES { return; }
    for part in parts.iter_mut().filter(|p| p["type"] == "dynamic-tool") {
        if part.get("output").is_some_and(|o| !o.is_null()) { part["output"] = json!({"trimmed": true}); }
        if let Some(input) = part.get_mut("input") { if input.to_string().len() > 400 { *input = json!({"trimmed": true}); } }
    }
    while size(parts) > MAX_STORED_BYTES {
        let Some(longest) = parts.iter_mut().filter(|p| p["type"] == "text").max_by_key(|p| p["text"].as_str().map_or(0, str::len)) else { break };
        let text = longest["text"].as_str().unwrap_or("");
        if text.len() < 1024 { break; }
        longest["text"] = Value::String(bound(text, text.chars().count() / 2));
    }
}

/// Sends a chunk to followers and folds it into the reply (an approval answered,
/// a call finished), when the reply is in flight. Returns whether it was.
pub fn publish_to(id: &str, chunk: Value) -> bool {
    let mut hub = hub();
    let Some(live) = hub.get_mut(id) else { return false };
    publish(live, chunk);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(deltas: &[&str]) -> (Vec<Value>, Proposals) {
        let mut stream = ReplyStream::new();
        let mut chunks = stream.begin();
        for d in deltas { chunks.extend(stream.delta(d)); }
        chunks.extend(stream.end());
        let mut parts = Vec::new();
        for c in &chunks { apply_chunk(&mut parts, c); }
        (parts, stream.proposals)
    }

    #[test]
    fn plain_prose_is_one_text_part_however_it_is_split() {
        let (parts, proposals) = run(&["Hel", "lo ", "there.\n"]);
        assert_eq!(parts, vec![json!({"type": "text", "text": "Hello there.", "state": "done"})]);
        assert!(proposals.blocks.is_empty());
    }

    #[test]
    fn a_complete_block_becomes_a_typed_part_between_two_texts() {
        let (parts, _) = run(&["Pick one.\n\n```neko-choices\n{\"question\":\"Which?\",", "\"options\":[{\"label\":\"A\"}]}\n```\n\nThen I wait."]);
        assert_eq!(parts.len(), 3, "{parts:?}");
        assert_eq!(parts[0]["text"], "Pick one.");
        assert_eq!(parts[1]["type"], "data-neko-choices");
        assert_eq!(parts[1]["data"]["question"], "Which?");
        assert_eq!(parts[2]["text"], "Then I wait.");
    }

    #[test]
    fn json_on_the_header_line_is_understood() {
        let (parts, _) = run(&["```neko-chart {\"title\":\"x\",\"bars\":[]}\n```"]);
        assert_eq!(parts, vec![json!({"type": "data-neko-chart", "data": {"title": "x", "bars": []}})]);
    }

    #[test]
    fn half_a_fence_is_never_shown_while_it_streams() {
        let mut stream = ReplyStream::new();
        let mut chunks = stream.begin();
        for piece in ["Look", " at ``", "`neko-ch", "oices\n{\"question\":\"Q"] { chunks.extend(stream.delta(piece)); }
        let mut parts = Vec::new();
        for c in &chunks { apply_chunk(&mut parts, c); }
        assert_eq!(parts[0]["text"], "Look at", "the marker and the open block are held back: {parts:?}");
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn invalid_display_blocks_stay_text_and_invalid_proposals_vanish() {
        let (parts, proposals) = run(&["Hi.\n```neko-choices\n{not json}\n```\nBye.\n```neko-ticket\n{broken\n```"]);
        let text: String = parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("|");
        assert!(text.contains("{not json}"), "{text}");
        assert!(!text.contains("broken"), "{text}");
        assert!(proposals.blocks.is_empty());
        assert!(parts.iter().all(|p| p["type"] == "text"));
    }

    #[test]
    fn an_unclosed_block_at_the_end_is_released_as_text() {
        let (parts, _) = run(&["Done.\n```neko-plan\n{\"steps\":[\"a\"]"]);
        let text = parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("");
        assert!(text.contains("```neko-plan"), "{text}");
    }

    #[test]
    fn proposals_are_collected_in_order_and_not_drawn() {
        let (parts, proposals) = run(&["Opening it.\n\n```neko-ticket\n{\"title\":\"A\",\"goal\":\"do a\"}\n```\n```neko-remember\n{\"text\":\"Likes tabs\"}\n```"]);
        assert_eq!(parts, vec![json!({"type": "text", "text": "Opening it.", "state": "done"})]);
        assert_eq!(proposals.blocks.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["ticket", "remember"]);
        assert_eq!(proposals.blocks[0].1["title"], "A");
    }

    #[test]
    fn tool_parts_follow_the_sdk_states() {
        let mut parts = Vec::new();
        for chunk in [
            json!({"type": "tool-input-start", "toolCallId": "c1", "toolName": "Bash"}),
            tool_input("c1", "Bash", &json!({"command": "ls"})),
            json!({"type": "tool-approval-request", "toolCallId": "c1", "approvalId": "a1"}),
            json!({"type": "tool-approval-response", "approvalId": "a1", "approved": true}),
            tool_output("c1", json!({"exit_code": 0, "output": "x"})),
        ] {
            apply_chunk(&mut parts, &chunk);
        }
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["state"], "output-available");
        assert_eq!(parts[0]["input"]["command"], "ls");
        assert_eq!(parts[0]["approval"], json!({"id": "a1", "approved": true}));
    }

    #[test]
    fn replaying_the_parts_rebuilds_the_same_parts() {
        let (mut parts, _) = run(&["First.\n\n```neko-files\n{\"files\":[{\"path\":\"/a\"}]}\n```\n\nSecond."]);
        for chunk in [
            tool_input("c1", "Edit", &json!({"path": "/a"})),
            tool_error("c1", "no"),
            json!({"type": "reasoning-start", "id": "x"}),
            json!({"type": "reasoning-delta", "id": "x", "delta": "hm"}),
            json!({"type": "reasoning-end", "id": "x"}),
        ] {
            apply_chunk(&mut parts, &chunk);
        }
        let mut again = Vec::new();
        for c in replay_chunks(&parts, None, None) { apply_chunk(&mut again, &c); }
        assert_eq!(again, parts);
    }

    #[test]
    fn a_follower_gets_the_reply_so_far_then_the_rest() {
        begin("m-follow", Vec::new(), 1000);
        for c in [json!({"type": "text-start", "id": "a"}), json!({"type": "text-delta", "id": "a", "delta": "One "})] {
            emit("m-follow", &c);
        }
        let feed = attach("m-follow", 2000).expect("a reply in flight");
        let mut seen = Vec::new();
        for c in &feed.replay { apply_chunk(&mut seen, c); }
        assert_eq!(feed.replay[0]["type"], "start");
        assert_eq!(feed.replay[0]["messageId"], "m-follow");
        assert_eq!(seen[0]["text"], "One");
        emit("m-follow", &json!({"type": "text-delta", "id": "a", "delta": "two."}));
        emit("m-follow", &json!({"type": "text-end", "id": "a"}));
        let parts = finish("m-follow", "done", 3000).unwrap();
        assert_eq!(parts[0]["text"], "One two.");
        let mut ids = Vec::new();
        while let Ok(c) = feed.live.try_recv() { ids.push(c["type"].as_str().unwrap().to_owned()); }
        assert_eq!(ids.last().map(String::as_str), Some("finish"));
        assert!(attach("m-follow", 4000).is_none(), "nothing to follow once it has finished");
    }

    #[test]
    fn writes_are_due_at_most_every_interval() {
        begin("m-due", Vec::new(), 0);
        let mut due = 0;
        emit("m-due", &json!({"type": "text-start", "id": "a"}));
        for _ in 0..500 {
            if emit("m-due", &json!({"type": "text-delta", "id": "a", "delta": "x "})).is_some() { due += 1; }
        }
        finish("m-due", "done", 1);
        assert_eq!(due, 0, "500 tokens inside {PERSIST_EVERY:?} ask for no write");
    }
}

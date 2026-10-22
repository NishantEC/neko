//! What an agent CLI reports, said as AI SDK UI message chunks.
//!
//! Claude Code (`--include-partial-messages`) reports tokens, thinking and
//! tool calls; Codex `exec --json` reports each finished item; OpenCode
//! reports finished parts. Each is mapped here, and only here, onto
//! `UIMessageChunk`s (`ui_message` folds them into parts). A chunk travels the
//! runner's ordinary `on_event(String)` channel under [`STREAM_EVENT`], a
//! control prefix no agent prose can collide with.
//!
//! The scoped Neko bridge's own tools are not reported: the bridge records
//! them, with their approval, in the chat's ledger.
use crate::ui_message::{self, bounded_output};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Prefix of an event whose rest is one chunk as JSON.
pub const STREAM_EVENT: &str = "\u{1}chunk ";

pub fn emit(on_event: &mut dyn FnMut(String), chunk: Value) {
    on_event(format!("{STREAM_EVENT}{chunk}"));
}

/// The chunk inside a [`STREAM_EVENT`] event, if it is one.
pub fn chunk_of(event: &str) -> Option<Value> {
    serde_json::from_str(event.strip_prefix(STREAM_EVENT)?).ok()
}

fn is_bridge_tool(name: &str) -> bool {
    name.starts_with("mcp__neko__")
}

/// A new assistant message of a run: a step.
fn step(on_event: &mut dyn FnMut(String)) {
    emit(on_event, json!({"type": "start-step"}));
}

fn whole_text(on_event: &mut dyn FnMut(String), id: &str, text: &str) {
    for chunk in ui_message::text_chunks(id, text) { emit(on_event, chunk); }
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// One `item.started` / `item.completed` of `codex exec --json`.
pub fn codex_item(item: &Value, completed: bool, on_event: &mut dyn FnMut(String)) {
    let id = item["id"].as_str().unwrap_or("item");
    match item["type"].as_str() {
        Some("agent_message") if completed => {
            if let Some(text) = item["text"].as_str() {
                step(on_event);
                whole_text(on_event, &format!("m-{id}"), text);
            }
        }
        Some("reasoning") if completed => {
            if let Some(text) = item["text"].as_str().filter(|t| !t.trim().is_empty()) {
                emit(on_event, json!({"type": "reasoning-start", "id": format!("r-{id}")}));
                emit(on_event, json!({"type": "reasoning-delta", "id": format!("r-{id}"), "delta": text}));
                emit(on_event, json!({"type": "reasoning-end", "id": format!("r-{id}")}));
            }
        }
        Some("command_execution") => {
            if !completed {
                emit(on_event, ui_message::tool_input(id, "Bash", &json!({"command": item["command"]})));
            } else {
                let output = item["aggregated_output"].as_str().unwrap_or("");
                emit(on_event, ui_message::tool_input(id, "Bash", &json!({"command": item["command"]})));
                emit(on_event, ui_message::tool_output(id, json!({"exit_code": item["exit_code"], "output": bounded_output(output)})));
            }
        }
        Some("file_change") if completed => {
            emit(on_event, ui_message::tool_input(id, "Edit", &json!({"changes": item["changes"]})));
            emit(on_event, ui_message::tool_output(id, json!({"status": item["status"]})));
        }
        Some("mcp_tool_call") if item["server"] != "neko" => {
            let name = format!("{}.{}", item["server"].as_str().unwrap_or("mcp"), item["tool"].as_str().unwrap_or("tool"));
            emit(on_event, ui_message::tool_input(id, &name, &item["arguments"]));
            if completed {
                if item["status"] == "failed" {
                    emit(on_event, ui_message::tool_error(id, item["error"]["message"].as_str().unwrap_or("The tool failed.")));
                } else {
                    emit(on_event, ui_message::tool_output(id, json!({"status": item["status"]})));
                }
            }
        }
        Some("web_search") if completed => {
            emit(on_event, ui_message::tool_input(id, "WebSearch", &json!({"query": item["query"]})));
            emit(on_event, ui_message::tool_output(id, json!({"status": "done"})));
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Block {
    Text(String),
    Reasoning(String),
    Tool { id: String, name: String, json: String },
    Skipped,
}

/// Claude Code's stream, tracked across lines.
#[derive(Debug, Default)]
pub struct ClaudeChunks {
    blocks: HashMap<u64, Block>,
    /// Tool calls already announced by the token stream.
    announced: HashSet<String>,
    /// Calls that are the bridge's, never shown.
    bridge: HashSet<String>,
    /// Messages whose text the token stream already carried.
    streamed: HashSet<String>,
    message: u32,
}

impl ClaudeChunks {
    /// A `stream_event` of the main session.
    pub fn stream_event(&mut self, event: &Value, on_event: &mut dyn FnMut(String)) {
        match event["type"].as_str() {
            Some("message_start") => {
                self.message += 1;
                if let Some(id) = event["message"]["id"].as_str() { self.streamed.insert(id.to_owned()); }
                self.blocks.clear();
                step(on_event);
            }
            Some("content_block_start") => {
                let index = event["index"].as_u64().unwrap_or(0);
                let block = &event["content_block"];
                let id = format!("m{}b{index}", self.message);
                let entry = match block["type"].as_str() {
                    Some("text") => {
                        emit(on_event, json!({"type": "text-start", "id": id}));
                        Block::Text(id)
                    }
                    Some("thinking") => {
                        emit(on_event, json!({"type": "reasoning-start", "id": id}));
                        Block::Reasoning(id)
                    }
                    Some("tool_use") => {
                        let name = block["name"].as_str().unwrap_or("tool").to_owned();
                        let call = block["id"].as_str().unwrap_or(&id).to_owned();
                        if is_bridge_tool(&name) {
                            self.bridge.insert(call);
                            Block::Skipped
                        } else {
                            emit(on_event, json!({"type": "tool-input-start", "toolCallId": call, "toolName": name, "dynamic": true}));
                            Block::Tool { id: call, name, json: String::new() }
                        }
                    }
                    _ => Block::Skipped,
                };
                self.blocks.insert(index, entry);
            }
            Some("content_block_delta") => {
                let index = event["index"].as_u64().unwrap_or(0);
                let delta = &event["delta"];
                match (self.blocks.get_mut(&index), delta["type"].as_str()) {
                    (Some(Block::Text(id)), Some("text_delta")) => {
                        if let Some(text) = delta["text"].as_str().filter(|t| !t.is_empty()) {
                            emit(on_event, json!({"type": "text-delta", "id": id, "delta": text}));
                        }
                    }
                    (Some(Block::Reasoning(id)), Some("thinking_delta")) => {
                        if let Some(text) = delta["thinking"].as_str().filter(|t| !t.is_empty()) {
                            emit(on_event, json!({"type": "reasoning-delta", "id": id, "delta": text}));
                        }
                    }
                    (Some(Block::Tool { json, .. }), Some("input_json_delta")) => {
                        if json.len() < 64 * 1024 { json.push_str(delta["partial_json"].as_str().unwrap_or("")); }
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                let index = event["index"].as_u64().unwrap_or(0);
                match self.blocks.remove(&index) {
                    Some(Block::Text(id)) => emit(on_event, json!({"type": "text-end", "id": id})),
                    Some(Block::Reasoning(id)) => emit(on_event, json!({"type": "reasoning-end", "id": id})),
                    Some(Block::Tool { id, name, json }) => {
                        let input = serde_json::from_str::<Value>(&json).unwrap_or_else(|_| json!({}));
                        self.announced.insert(id.clone());
                        emit(on_event, ui_message::tool_input(&id, &name, &input));
                    }
                    _ => {}
                }
            }
            Some("message_stop") => emit(on_event, json!({"type": "finish-step"})),
            _ => {}
        }
    }

    /// A whole `assistant` message: what the token stream did not carry (a run
    /// without partial messages) is reported now.
    pub fn assistant(&mut self, value: &Value, on_event: &mut dyn FnMut(String)) {
        if !value["parent_tool_use_id"].is_null() { return; }
        let message_id = value["message"]["id"].as_str().unwrap_or("");
        let streamed = self.streamed.contains(message_id);
        let mut stepped = false;
        for (n, block) in value["message"]["content"].as_array().into_iter().flatten().enumerate() {
            match block["type"].as_str() {
                Some("text") if !streamed => {
                    if let Some(text) = block["text"].as_str() {
                        if !std::mem::replace(&mut stepped, true) { step(on_event); }
                        whole_text(on_event, &format!("a{message_id}{n}"), text);
                    }
                }
                Some("thinking") if !streamed => {
                    if let Some(text) = block["thinking"].as_str().filter(|t| !t.trim().is_empty()) {
                        if !std::mem::replace(&mut stepped, true) { step(on_event); }
                        let id = format!("a{message_id}{n}");
                        emit(on_event, json!({"type": "reasoning-start", "id": id}));
                        emit(on_event, json!({"type": "reasoning-delta", "id": id, "delta": text}));
                        emit(on_event, json!({"type": "reasoning-end", "id": id}));
                    }
                }
                Some("tool_use") => {
                    let name = block["name"].as_str().unwrap_or("tool");
                    let call = block["id"].as_str().unwrap_or("");
                    if is_bridge_tool(name) { self.bridge.insert(call.to_owned()); continue; }
                    if self.announced.insert(call.to_owned()) {
                        if !std::mem::replace(&mut stepped, true) && !streamed { step(on_event); }
                        emit(on_event, ui_message::tool_input(call, name, &block["input"]));
                    }
                }
                _ => {}
            }
        }
    }

    /// A tool's result (a `tool_result` block of a `user` line). `bash` is the
    /// exit code and body already read from a command's output.
    pub fn tool_result(&mut self, block: &Value, bash: Option<(i64, String)>, on_event: &mut dyn FnMut(String)) {
        let call = block["tool_use_id"].as_str().unwrap_or("");
        if self.bridge.contains(call) || !self.announced.contains(call) { return; }
        let text = match &block["content"] {
            Value::String(s) => s.clone(),
            Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
            _ => String::new(),
        };
        if let Some((code, body)) = bash {
            emit(on_event, ui_message::tool_output(call, json!({"exit_code": code, "output": bounded_output(&body)})));
        } else if block["is_error"].as_bool() == Some(true) {
            emit(on_event, ui_message::tool_error(call, &text));
        } else {
            emit(on_event, ui_message::tool_output(call, json!({"output": bounded_output(&text)})));
        }
    }
}

// ---------------------------------------------------------------------------
// OpenCode
// ---------------------------------------------------------------------------

/// A finished `text` part.
pub fn opencode_text(part: &Value, on_event: &mut dyn FnMut(String)) {
    if let (Some(id), Some(text)) = (part["id"].as_str(), part["text"].as_str()) {
        step(on_event);
        whole_text(on_event, &format!("o-{id}"), text);
    }
}

/// A finished `tool_use` part.
pub fn opencode_tool(part: &Value, on_event: &mut dyn FnMut(String)) {
    let state = &part["state"];
    if !matches!(state["status"].as_str(), Some("completed" | "error")) { return; }
    let id = part["callID"].as_str().or_else(|| part["id"].as_str()).unwrap_or("call");
    let name = part["tool"].as_str().unwrap_or("tool");
    if name.starts_with("neko_") { return; }
    emit(on_event, ui_message::tool_input(id, name, &state["input"]));
    if state["status"] == "error" {
        emit(on_event, ui_message::tool_error(id, state["error"].as_str().unwrap_or("The tool failed.")));
    } else {
        let exit = state["metadata"]["exit"].clone();
        emit(on_event, ui_message::tool_output(id, json!({"exit_code": exit, "output": bounded_output(state["output"].as_str().unwrap_or(""))})));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold(events: &[String]) -> Vec<Value> {
        let mut parts = Vec::new();
        for event in events {
            if let Some(chunk) = chunk_of(event) { ui_message::apply_chunk(&mut parts, &chunk); }
        }
        parts
    }

    /// A recorded Claude Code run: thinking, text, a Bash call, its result, an
    /// MCP call of the bridge (not shown), a second message, a subagent's tokens.
    fn recorded_claude() -> Vec<Value> {
        let lines = [
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Check the repo."}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"text"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Let me "}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"look."}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"Bash"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"command\":"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"\"git status\"}"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":2}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":3,"content_block":{"type":"tool_use","id":"toolu_b","name":"mcp__neko__call"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":3}}"#,
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
            r#"{"type":"stream_event","parent_tool_use_id":"sub1","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"subagent"}}}"#,
        ];
        lines.iter().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    #[test]
    fn claude_tokens_become_thinking_prose_and_a_command() {
        let mut chunks = ClaudeChunks::default();
        let mut events = Vec::new();
        for line in recorded_claude() {
            if line["parent_tool_use_id"].is_null() { chunks.stream_event(&line["event"], &mut |e| events.push(e)); }
        }
        let assistant: Value = serde_json::from_str(r#"{"type":"assistant","message":{"id":"msg1","content":[{"type":"text","text":"Let me look."},{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"git status"}}]}}"#).unwrap();
        chunks.assistant(&assistant, &mut |e| events.push(e));
        let result: Value = serde_json::from_str(r#"{"type":"tool_result","tool_use_id":"toolu_1","content":"On branch main"}"#).unwrap();
        chunks.tool_result(&result, Some((0, "On branch main".into())), &mut |e| events.push(e));
        let bridge: Value = serde_json::from_str(r#"{"type":"tool_result","tool_use_id":"toolu_b","content":"x"}"#).unwrap();
        chunks.tool_result(&bridge, None, &mut |e| events.push(e));
        let parts = fold(&events);
        let kinds: Vec<_> = parts.iter().map(|p| p["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["step-start", "reasoning", "text", "dynamic-tool"], "{parts:?}");
        assert_eq!(parts[1]["text"], "Check the repo.");
        assert_eq!(parts[2]["text"], "Let me look.", "the assistant line must not repeat what the tokens carried");
        assert_eq!(parts[3]["toolName"], "Bash");
        assert_eq!(parts[3]["input"]["command"], "git status");
        assert_eq!(parts[3]["state"], "output-available");
        assert_eq!(parts[3]["output"]["exit_code"], 0);
        assert!(events.iter().all(|e| !e.contains("subagent") && !e.contains("toolu_b")), "subagent tokens and bridge calls are not shown");
    }

    #[test]
    fn a_claude_run_without_partial_messages_still_reports_text_and_tools() {
        let mut chunks = ClaudeChunks::default();
        let mut events = Vec::new();
        let assistant: Value = serde_json::from_str(r#"{"type":"assistant","message":{"id":"m9","content":[{"type":"text","text":"Reading."},{"type":"tool_use","id":"t9","name":"Read","input":{"file_path":"/a"}}]}}"#).unwrap();
        chunks.assistant(&assistant, &mut |e| events.push(e));
        let failed: Value = serde_json::from_str(r#"{"type":"tool_result","tool_use_id":"t9","is_error":true,"content":"no such file"}"#).unwrap();
        chunks.tool_result(&failed, None, &mut |e| events.push(e));
        let parts = fold(&events);
        assert_eq!(parts[1]["text"], "Reading.");
        assert_eq!(parts[2]["toolName"], "Read");
        assert_eq!(parts[2]["state"], "output-error");
        assert_eq!(parts[2]["errorText"], "no such file");
    }

    #[test]
    fn codex_items_become_prose_commands_edits_and_reasoning() {
        let mut events = Vec::new();
        let items = [
            (r#"{"id":"i0","type":"reasoning","text":"Plan it."}"#, true),
            (r#"{"id":"i1","type":"command_execution","command":"/bin/zsh -lc 'ls'","status":"in_progress"}"#, false),
            (r#"{"id":"i1","type":"command_execution","command":"/bin/zsh -lc 'ls'","aggregated_output":"a\nb","exit_code":1,"status":"failed"}"#, true),
            (r#"{"id":"i2","type":"file_change","changes":[{"path":"/a.rs","kind":"update"}],"status":"completed"}"#, true),
            (r#"{"id":"i3","type":"mcp_tool_call","server":"neko","tool":"call","arguments":{}}"#, true),
            (r#"{"id":"i4","type":"agent_message","text":"All done."}"#, true),
        ];
        for (json, completed) in items {
            codex_item(&serde_json::from_str(json).unwrap(), completed, &mut |e| events.push(e));
        }
        let parts = fold(&events);
        let kinds: Vec<_> = parts.iter().map(|p| p["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["reasoning", "dynamic-tool", "dynamic-tool", "step-start", "text"], "{parts:?}");
        assert_eq!(parts[1]["toolName"], "Bash");
        assert_eq!(parts[1]["output"]["exit_code"], 1);
        assert_eq!(parts[2]["toolName"], "Edit");
        assert_eq!(parts[4]["text"], "All done.");
    }

    #[test]
    fn a_chunk_event_round_trips_and_prose_cannot_be_mistaken_for_one() {
        let mut events = Vec::new();
        emit(&mut |e| events.push(e), json!({"type": "start-step"}));
        assert_eq!(chunk_of(&events[0]), Some(json!({"type": "start-step"})));
        assert_eq!(chunk_of("chunk {\"type\":\"x\"}"), None);
    }
}

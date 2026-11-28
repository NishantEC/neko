//! Claude Code's stream-json, line by line, into Neko's events (docs/knowledge/runtimes.md).
//!
//! Pure: a [`Mapper`] is fed one stdout line at a time and says what it means. No I/O, no clock. What the CLI sends, as read from
//! its documented SDK protocol (version 2.1):
//!
//! - `system` / `init`: the conversation's `session_id` and `model`.
//! - `stream_event` (with `--include-partial-messages`): the API's raw stream: `message_start`, `content_block_start`,
//!   `content_block_delta` (`text_delta`, `thinking_delta`), `content_block_stop`.
//! - `assistant`: whole content blocks (text, thinking, `tool_use` with its complete input), one or more per message.
//! - `user`: `tool_result` blocks for the tool calls.
//! - `result`: the turn's end, with `usage`, `total_cost_usd`, `modelUsage`, `is_error` and the final words or `errors`.
//! - `rate_limit_event`: the account's limit; `rejected` with `resetsAt` when it is reached.
//! - `control_request` / `can_use_tool`: a permission request (with `--permission-prompt-tool stdio`), answered with a
//!   `control_response`. `control_cancel_request` takes one back.
//!
//! Words are streamed from `stream_event` deltas; a message whose deltas never came (an older CLI, or no partial messages) gives
//! its words whole from the `assistant` event instead, so nothing is shown twice and nothing is lost.
//!
//! Thinking: Claude Code 2.1.331 sends thinking blocks with empty text and only a `signature` (the model's thoughts are not shown
//! to it either: its own transcript holds the same empty blocks). An empty delta, a `signature_delta` and a `redacted_thinking`
//! block say nothing a person can read, so they make no event at all.
//!
//! A shell command that ran and exited non-zero comes back as a `tool_result` with `is_error` and text that begins
//! "Exit code N". That step finished: it is `ok`, with its exit code. A tool that could not run (denied, an error, cut off) is not.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::runtime::{Action, AgentEvent, Failure, Reported, shorten};

/// What one line means.
#[derive(Clone, Debug, PartialEq)]
pub enum Said {
    Event(AgentEvent),
    /// A permission request to answer: the CLI's request id, the tool's name, its input (to send back on allow), its tool-use id.
    Ask { request: String, tool: String, input: Value, tool_use: Option<String> },
    /// A control request Neko does not serve: answer it with an error so the CLI does not wait.
    Unsupported { request: String, subtype: String },
    /// The CLI consumed one of the `user` messages Neko wrote to its stdin (`--replay-user-messages` echoes it): it is read, in this
    /// turn or the next. Never drawn: Neko stored the person's words when they were sent.
    Consumed,
    /// The turn ended.
    TurnEnded { ok: bool },
}

/// The longest tool input and output kept for the fold.
pub const MAX_INPUT: usize = 2_000;
pub const MAX_OUTPUT: usize = 4_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Text,
    Thinking,
}

/// The state between lines.
#[derive(Default)]
pub struct Mapper {
    started: bool,
    /// Messages whose words arrived as deltas.
    streamed: HashSet<String>,
    current: Option<String>,
    /// The block open in the stream, by its index.
    open: HashMap<u64, Open>,
    tools: HashMap<String, String>,
    /// The context the last top-level model call held, from its message's usage.
    context: Option<u64>,
    pub session: Option<String>,
    pub model: Option<String>,
    /// The CLI said the account's limit was reached; when it resets, in words.
    pub limit: Option<Option<String>>,
    /// The failure the last turn reported.
    pub failure: Option<Failure>,
    pub turns: u32,
    /// Lines that were not JSON (kept short, for a crash's reason).
    pub noise: Vec<String>,
}

fn s(v: &Value) -> Option<&str> {
    v.as_str()
}

impl Mapper {
    pub fn new() -> Mapper {
        Mapper::default()
    }

    /// One line of the CLI's stdout. `workdir` makes paths short.
    pub fn feed(&mut self, line: &str, workdir: &str) -> Vec<Said> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            if self.noise.len() < 20 {
                self.noise.push(shorten(line, 300));
            }
            return Vec::new();
        };
        let mut out = Vec::new();
        match s(&v["type"]).unwrap_or("") {
            // Claude Code's own news (hooks starting and answering, its state, its commands, notices): never shown. Two matter.
            "system" if s(&v["subtype"]) == Some("thinking_tokens") => out.push(Said::Event(AgentEvent::ThinkingStarted)),
            "system" if s(&v["tag"]) == Some("hook_no_verdict") || !v["hook_no_verdict"].is_null() => {
                out.push(Said::Event(AgentEvent::Unchecked { tool_use: s(&v["hook_no_verdict"]["tool_use_id"]).map(str::to_owned) }))
            }
            "system" => self.system(&v, &mut out),
            // A subagent's own stream stays inside its tool call.
            "stream_event" if v["parent_tool_use_id"].is_null() => self.stream(&v["event"], &mut out),
            "assistant" => self.assistant(&v, workdir, &mut out),
            "user" => self.user(&v, &mut out),
            "result" => self.result(&v, &mut out),
            "rate_limit_event" => {
                let info = &v["rate_limit_info"];
                if let Some(status) = s(&info["status"]) {
                    out.push(Said::Event(AgentEvent::RateLimit { status: status.to_owned(), resets_at: info["resetsAt"].as_i64() }));
                }
                if s(&info["status"]) == Some("rejected") {
                    self.limit = Some(info["resetsAt"].as_i64().map(reset_words));
                }
            }
            "control_request" => {
                let request = s(&v["request_id"]).unwrap_or("").to_owned();
                let r = &v["request"];
                match s(&r["subtype"]).unwrap_or("") {
                    "can_use_tool" => out.push(Said::Ask {
                        request,
                        tool: s(&r["tool_name"]).unwrap_or("tool").to_owned(),
                        input: r["input"].clone(),
                        tool_use: s(&r["tool_use_id"]).map(str::to_owned),
                    }),
                    other => out.push(Said::Unsupported { request, subtype: other.to_owned() }),
                }
            }
            "control_cancel_request" => {
                if let Some(id) = s(&v["request_id"]) {
                    out.push(Said::Event(AgentEvent::PermissionWithdrawn { id: id.to_owned() }));
                }
            }
            _ => {}
        }
        out
    }

    fn start(&mut self, out: &mut Vec<Said>) {
        if !self.started {
            self.started = true;
            out.push(Said::Event(AgentEvent::Started { session: self.session.clone(), model: self.model.clone() }));
        }
    }

    fn system(&mut self, v: &Value, out: &mut Vec<Said>) {
        if s(&v["subtype"]) == Some("init") {
            if let Some(id) = s(&v["session_id"]) {
                self.session = Some(id.to_owned());
            }
            if let Some(m) = s(&v["model"]) {
                self.model = Some(m.to_owned());
            }
            self.start(out);
        }
    }

    fn close_open(&mut self, out: &mut Vec<Said>) {
        let mut open: Vec<_> = self.open.drain().collect();
        open.sort_by_key(|(i, _)| *i);
        for (_, o) in open {
            out.push(Said::Event(match o {
                Open::Text => AgentEvent::TextDone,
                Open::Thinking => AgentEvent::ThinkingDone,
            }));
        }
    }

    fn stream(&mut self, e: &Value, out: &mut Vec<Said>) {
        match s(&e["type"]).unwrap_or("") {
            "message_start" => {
                self.close_open(out);
                self.current = s(&e["message"]["id"]).map(str::to_owned);
                out.push(Said::Event(AgentEvent::StepBegan));
            }
            "content_block_start" => {
                let i = e["index"].as_u64().unwrap_or(0);
                match s(&e["content_block"]["type"]) {
                    Some("text") => {
                        self.open.insert(i, Open::Text);
                    }
                    Some("thinking") => {
                        self.open.insert(i, Open::Thinking);
                        out.push(Said::Event(AgentEvent::ThinkingStarted));
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                let d = &e["delta"];
                let (event, words) = match s(&d["type"]) {
                    Some("text_delta") => (AgentEvent::TextDelta(s(&d["text"]).unwrap_or("").to_owned()), true),
                    Some("thinking_delta") => (AgentEvent::ThinkingDelta(s(&d["thinking"]).unwrap_or("").to_owned()), true),
                    _ => (AgentEvent::TextDone, false),
                };
                if words {
                    if let Some(m) = &self.current {
                        self.streamed.insert(m.clone());
                    }
                    // An empty piece (Claude Code's thinking comes with none) opens nothing.
                    let empty = matches!(&event, AgentEvent::TextDelta(t) | AgentEvent::ThinkingDelta(t) if t.is_empty());
                    if !empty {
                        self.start(out);
                        out.push(Said::Event(event));
                    }
                }
            }
            "content_block_stop" => {
                let i = e["index"].as_u64().unwrap_or(0);
                if let Some(o) = self.open.remove(&i) {
                    out.push(Said::Event(match o {
                        Open::Text => AgentEvent::TextDone,
                        Open::Thinking => AgentEvent::ThinkingDone,
                    }));
                }
            }
            "message_stop" => {
                self.close_open(out);
                out.push(Said::Event(AgentEvent::StepEnded));
            }
            _ => {}
        }
    }

    fn assistant(&mut self, v: &Value, workdir: &str, out: &mut Vec<Said>) {
        let m = &v["message"];
        let streamed = s(&m["id"]).is_some_and(|id| self.streamed.contains(id));
        if self.model.is_none() {
            self.model = s(&m["model"]).map(str::to_owned);
        }
        // A subagent's own messages (parent_tool_use_id set) stay inside its tool call.
        if !v["parent_tool_use_id"].is_null() {
            return;
        }
        let u = &m["usage"];
        if u.is_object() {
            let held = ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"].iter().filter_map(|k| u[*k].as_u64()).sum::<u64>();
            if held > 0 {
                self.context = Some(held);
            }
        }
        for block in m["content"].as_array().into_iter().flatten() {
            match s(&block["type"]).unwrap_or("") {
                "text" if !streamed => {
                    let t = s(&block["text"]).unwrap_or("");
                    if !t.is_empty() {
                        self.start(out);
                        out.push(Said::Event(AgentEvent::TextDelta(t.to_owned())));
                        out.push(Said::Event(AgentEvent::TextDone));
                    }
                }
                "thinking" if !streamed => {
                    let t = s(&block["thinking"]).unwrap_or("");
                    if !t.is_empty() {
                        self.start(out);
                        out.push(Said::Event(AgentEvent::ThinkingDelta(t.to_owned())));
                        out.push(Said::Event(AgentEvent::ThinkingDone));
                    }
                }
                "tool_use" => {
                    self.close_open(out);
                    self.start(out);
                    let id = s(&block["id"]).unwrap_or("").to_owned();
                    let name = s(&block["name"]).unwrap_or("tool").to_owned();
                    let input = &block["input"];
                    self.tools.insert(id.clone(), name.clone());
                    out.push(Said::Event(AgentEvent::ToolStarted { id, summary: summary(&name, input, workdir), input: input_words(&name, input), name }));
                }
                _ => {}
            }
        }
    }

    fn user(&mut self, v: &Value, out: &mut Vec<Said>) {
        if !v["parent_tool_use_id"].is_null() {
            return;
        }
        // The echo of a message from stdin (`--replay-user-messages`): counted, never a part. Ordinary tool results carry no flag.
        if v["isReplay"].as_bool() == Some(true) {
            out.push(Said::Consumed);
            return;
        }
        for block in v["message"]["content"].as_array().into_iter().flatten() {
            if s(&block["type"]) != Some("tool_result") {
                continue;
            }
            let Some(id) = s(&block["tool_use_id"]) else { continue };
            let Some(name) = self.tools.get(id) else { continue };
            let output = match &block["content"] {
                Value::String(t) => t.clone(),
                Value::Array(parts) => parts.iter().filter_map(|p| s(&p["text"])).collect::<Vec<_>>().join("\n"),
                _ => String::new(),
            };
            let error = block["is_error"].as_bool() == Some(true);
            // A command that ran and ended non-zero finished; only a tool that could not run failed.
            let exit = if error && name == "Bash" { exit_code(&output) } else { None };
            out.push(Said::Event(AgentEvent::ToolFinished { id: id.to_owned(), ok: !error || exit.is_some(), output: shorten(&output, MAX_OUTPUT), exit }));
        }
    }

    fn result(&mut self, v: &Value, out: &mut Vec<Said>) {
        self.close_open(out);
        self.start(out);
        self.turns += 1;
        if let Some(id) = s(&v["session_id"]) {
            self.session = Some(id.to_owned());
        }
        let u = &v["usage"];
        let model = v["modelUsage"].as_object().and_then(|m| {
            // The model that did the most work names the run.
            m.iter().max_by_key(|(_, mu)| mu["outputTokens"].as_u64().unwrap_or(0)).map(|(k, _)| k.clone())
        });
        let window = v["modelUsage"].as_object().and_then(|m| m.values().filter_map(|mu| mu["contextWindow"].as_u64()).max());
        let reported = Reported {
            input_tokens: u["input_tokens"].as_u64().unwrap_or(0) + u["cache_creation_input_tokens"].as_u64().unwrap_or(0),
            output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
            cached_tokens: u["cache_read_input_tokens"].as_u64().unwrap_or(0),
            cost_usd: v["total_cost_usd"].as_f64(),
            model: model.or_else(|| self.model.clone()),
            context_tokens: self.context,
            context_window: window,
        };
        if reported != Reported::default() {
            out.push(Said::Event(AgentEvent::Usage(reported)));
        }
        let words = s(&v["result"]).map(str::to_owned).unwrap_or_else(|| v["errors"].as_array().map(|e| e.iter().filter_map(s).collect::<Vec<_>>().join("; ")).unwrap_or_default());
        let failed = v["is_error"].as_bool() == Some(true) || s(&v["subtype"]).is_some_and(|t| t != "success");
        self.failure = classify(&words, failed, self.limit.clone());
        out.push(Said::TurnEnded { ok: self.failure.is_none() });
    }
}

/// The exit code a shell tool's result begins with ("Exit code 1"), when it does.
pub fn exit_code(output: &str) -> Option<i32> {
    let rest = output.trim_start().strip_prefix("Exit code ")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
    digits.parse().ok().filter(|c| *c != 0)
}

/// Claude Code's own bookkeeping tools: nothing on the Mac changes by them.
pub const INTERNAL_TOOLS: &[&str] = &[
    "TodoWrite", "TodoRead", "Task", "Agent", "TaskCreate", "TaskUpdate", "TaskList", "TaskGet", "TaskOutput", "TaskStop", "BashOutput", "KillShell", "KillBash", "ToolSearch", "AskUserQuestion",
    "ExitPlanMode", "EnterPlanMode", "Skill", "SlashCommand", "ListMcpResourcesTool", "ReadMcpResourceTool",
];

/// What a turn's words and flags say went wrong, if anything.
pub fn classify(words: &str, failed: bool, limit: Option<Option<String>>) -> Option<Failure> {
    let lower = words.to_lowercase();
    if lower.contains("usage limit") || lower.contains("hit your limit") || lower.contains("limit reached") || (failed && limit.is_some()) {
        let resets = limit.flatten().or_else(|| words.split('|').nth(1).and_then(|t| t.trim().parse::<i64>().ok()).map(reset_words)).or_else(|| {
            lower.find("resets").map(|i| words[i..].trim().trim_end_matches('.').to_owned())
        });
        return Some(Failure::UsageLimit { resets });
    }
    if !failed {
        return None;
    }
    if lower.contains("/login") || lower.contains("not logged in") || lower.contains("invalid api key") || lower.contains("oauth token") {
        return Some(Failure::NotSignedIn);
    }
    Some(Failure::Reported(if words.trim().is_empty() { "Claude Code said the run failed and gave no reason.".to_owned() } else { shorten(words.trim(), 600) }))
}

/// When a limit resets, from the CLI's Unix time, in the Mac's local time ("at 15:00", or "on 27 Nov at 15:00").
pub fn reset_words(unix_s: i64) -> String {
    // SAFETY: localtime_r fills the zeroed tm from a valid time_t.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = unix_s as libc::time_t;
    unsafe { libc::localtime_r(&t, &mut tm) };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    if (unix_s - now).abs() < 20 * 3600 {
        format!("at {:02}:{:02}", tm.tm_hour, tm.tm_min)
    } else {
        format!("on {} {} at {:02}:{:02}", tm.tm_mday, MONTHS[tm.tm_mon.clamp(0, 11) as usize], tm.tm_hour, tm.tm_min)
    }
}

fn field<'a>(input: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| s(&input[*k]))
}

/// `path` relative to `workdir` when it is inside it.
fn short_path(path: &str, workdir: &str) -> String {
    let w = workdir.trim_end_matches('/');
    match path.strip_prefix(w).and_then(|r| r.strip_prefix('/')) {
        Some(rest) if !rest.is_empty() => rest.to_owned(),
        _ => path.to_owned(),
    }
}

fn first_line(t: &str, max: usize) -> String {
    let l = t.lines().next().unwrap_or("").trim();
    if l.chars().count() > max { format!("{}…", l.chars().take(max).collect::<String>()) } else { l.to_owned() }
}

/// A few words on what a call does, after its name.
pub fn summary(name: &str, input: &Value, workdir: &str) -> String {
    match name {
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "NotebookRead" => field(input, &["file_path", "notebook_path", "path"]).map(|p| short_path(p, workdir)).unwrap_or_default(),
        "Bash" => first_line(field(input, &["command"]).unwrap_or(""), 80),
        "Glob" | "Grep" => field(input, &["pattern"]).map(|p| first_line(p, 60)).unwrap_or_default(),
        "LS" => field(input, &["path"]).map(|p| short_path(p, workdir)).unwrap_or_default(),
        "WebFetch" => field(input, &["url"]).unwrap_or("").to_owned(),
        "WebSearch" => field(input, &["query"]).map(|q| first_line(q, 60)).unwrap_or_default(),
        "Task" | "Agent" => field(input, &["description"]).map(|d| first_line(d, 60)).unwrap_or_default(),
        "TodoWrite" => "the plan".to_owned(),
        _ => input.as_object().and_then(|o| o.values().find_map(s)).map(|v| first_line(v, 60)).unwrap_or_default(),
    }
}

/// A call's input for its fold: a command as itself, anything else as indented JSON. Shortened.
pub fn input_words(name: &str, input: &Value) -> String {
    if name == "Bash"
        && let Some(c) = s(&input["command"])
    {
        return shorten(c, MAX_INPUT);
    }
    shorten(&serde_json::to_string_pretty(input).unwrap_or_default(), MAX_INPUT)
}

/// What a tool wants to do, for Neko's policy. Tools Neko does not know are [`Action::Other`], and so always ask.
pub fn action(tool: &str, input: &Value, workdir: &str) -> Action {
    let path = || field(input, &["file_path", "notebook_path", "path"]).unwrap_or(workdir).to_owned();
    match tool {
        "Read" | "Glob" | "Grep" | "LS" | "NotebookRead" => Action::Read { path: path() },
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => Action::Edit { path: path() },
        "Bash" => Action::Command { command: field(input, &["command"]).unwrap_or("").to_owned() },
        "WebFetch" => Action::Network { target: field(input, &["url"]).unwrap_or("").to_owned() },
        "WebSearch" => Action::Network { target: format!("a web search: {}", field(input, &["query"]).unwrap_or("")) },
        t if INTERNAL_TOOLS.contains(&t) => Action::Internal { tool: t.to_owned() },
        // Neko's own tools (docs/knowledge/subagents.md) only record work in Neko's own state, so they go ahead in every mode.
        t if t.starts_with("mcp__neko__") => Action::Internal { tool: t.to_owned() },
        t if neko_protocol::plugins::mcp_tool(t).is_some() => {
            let (server, tool) = neko_protocol::plugins::mcp_tool(t).unwrap_or_default();
            Action::Mcp { server: server.to_owned(), tool: tool.to_owned(), input: shorten(&input.to_string(), 400) }
        }
        other => Action::Other { summary: format!("{other} {}", summary(other, input, workdir)).trim().to_owned() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(m: &mut Mapper, lines: &[&str]) -> Vec<Said> {
        lines.iter().flat_map(|l| m.feed(l, "/w")).collect()
    }

    fn events(said: &[Said]) -> Vec<AgentEvent> {
        said.iter().filter_map(|s| if let Said::Event(e) = s { Some(e.clone()) } else { None }).collect()
    }

    #[test]
    fn streamed_words_are_not_repeated_by_the_whole_message() {
        let mut m = Mapper::new();
        let said = feed(&mut m, &[
            r#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-x"}"#,
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"m1"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            r#"{"type":"assistant","message":{"id":"m1","model":"claude-x","content":[{"type":"text","text":"Hello"}]},"parent_tool_use_id":null}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"Hello","session_id":"s1","total_cost_usd":0.01,"usage":{"input_tokens":3,"cache_creation_input_tokens":2,"cache_read_input_tokens":7,"output_tokens":4},"modelUsage":{"claude-x":{"outputTokens":4}}}"#,
        ]);
        assert_eq!(
            events(&said),
            vec![
                AgentEvent::Started { session: Some("s1".into()), model: Some("claude-x".into()) },
                AgentEvent::StepBegan,
                AgentEvent::TextDelta("Hel".into()),
                AgentEvent::TextDelta("lo".into()),
                AgentEvent::TextDone,
                AgentEvent::Usage(Reported { input_tokens: 5, output_tokens: 4, cached_tokens: 7, cost_usd: Some(0.01), model: Some("claude-x".into()), context_tokens: None, context_window: None }),
            ]
        );
        assert_eq!(said.last(), Some(&Said::TurnEnded { ok: true }));
    }

    #[test]
    fn whole_messages_give_their_words_when_nothing_streamed() {
        let mut m = Mapper::new();
        let said = feed(&mut m, &[r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"thinking","thinking":"Hm"},{"type":"text","text":"Yes"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls -la\necho"}}]},"parent_tool_use_id":null}"#]);
        let e = events(&said);
        assert!(matches!(&e[0], AgentEvent::Started { .. }));
        assert_eq!(e[1..5], [AgentEvent::ThinkingDelta("Hm".into()), AgentEvent::ThinkingDone, AgentEvent::TextDelta("Yes".into()), AgentEvent::TextDone]);
        assert_eq!(e[5], AgentEvent::ToolStarted { id: "t1".into(), name: "Bash".into(), summary: "ls -la".into(), input: "ls -la\necho".into() });
    }

    #[test]
    fn tool_results_finish_their_calls() {
        let mut m = Mapper::new();
        feed(&mut m, &[r#"{"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/src/a.rs"}}]},"parent_tool_use_id":null}"#]);
        let said = feed(&mut m, &[r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":[{"type":"text","text":"no such file"}]}]},"parent_tool_use_id":null}"#]);
        assert_eq!(events(&said), vec![AgentEvent::ToolFinished { id: "t1".into(), ok: false, output: "no such file".into(), exit: None }]);
    }

    #[test]
    fn a_replayed_user_message_is_counted_and_never_drawn() {
        let mut m = Mapper::new();
        feed(&mut m, &[r#"{"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"sleep 1"}}]},"parent_tool_use_id":null}"#]);
        // An ordinary tool result carries no flag and still finishes its call; the echo of the person's words is only counted.
        let said = feed(&mut m, &[
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"ok"}]},"parent_tool_use_id":null}"#,
            r#"{"type":"user","isReplay":true,"message":{"role":"user","content":[{"type":"text","text":"Also: end with meow."}]},"parent_tool_use_id":null,"session_id":"s"}"#,
            r#"{"type":"user","isReplay":true,"message":{"role":"user","content":"a plain string message"},"parent_tool_use_id":null}"#,
        ]);
        assert_eq!(events(&said), vec![AgentEvent::ToolFinished { id: "t1".into(), ok: true, output: "ok".into(), exit: None }]);
        assert_eq!(said.iter().filter(|s| matches!(s, Said::Consumed)).count(), 2);
        assert_eq!(said.len(), 3, "nothing else comes of a replay: {said:?}");
        // A flag that is false, or a subagent's own line, is not the person's message.
        let said = feed(&mut m, &[
            r#"{"type":"user","isReplay":false,"message":{"content":[{"type":"text","text":"x"}]},"parent_tool_use_id":null}"#,
            r#"{"type":"user","isReplay":true,"message":{"content":[{"type":"text","text":"x"}]},"parent_tool_use_id":"toolu_sub"}"#,
        ]);
        assert!(said.is_empty(), "{said:?}");
    }

    #[test]
    fn a_command_that_exits_non_zero_finished_and_says_its_code() {
        let mut m = Mapper::new();
        feed(&mut m, &[r#"{"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":"b1","name":"Bash","input":{"command":"git diff --quiet"}},{"type":"tool_use","id":"b2","name":"Bash","input":{"command":"cargo test"}}]},"parent_tool_use_id":null}"#]);
        let said = feed(&mut m, &[
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b1","is_error":true,"content":"Exit code 1\nfiles differ"}]},"parent_tool_use_id":null}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b2","is_error":true,"content":"The person denied this."}]},"parent_tool_use_id":null}"#,
        ]);
        assert_eq!(events(&said), vec![
            AgentEvent::ToolFinished { id: "b1".into(), ok: true, output: "Exit code 1\nfiles differ".into(), exit: Some(1) },
            AgentEvent::ToolFinished { id: "b2".into(), ok: false, output: "The person denied this.".into(), exit: None },
        ]);
        assert_eq!(exit_code("Exit code 127"), Some(127));
        assert_eq!(exit_code("Exit code 0"), None);
        assert_eq!(exit_code("exit 1"), None);
    }

    #[test]
    fn a_model_call_begins_and_ends_only_for_the_agents_own_messages() {
        // How a steered message is known to be read (docs/knowledge/composer.md): the calls' edges, never a subagent's.
        let mut m = Mapper::new();
        let said = feed(&mut m, &[
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"m1"}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"message_stop"},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"sub1"}},"parent_tool_use_id":"toolu_1"}"#,
            r#"{"type":"stream_event","event":{"type":"message_stop"},"parent_tool_use_id":"toolu_1"}"#,
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"m2"}},"parent_tool_use_id":null}"#,
        ]);
        assert_eq!(events(&said), vec![AgentEvent::StepBegan, AgentEvent::StepEnded, AgentEvent::StepBegan]);
    }

    #[test]
    fn empty_thinking_makes_no_event_and_signatures_are_ignored() {
        // The shape Claude Code 2.1.331 sends: a thinking block, an empty delta, a signature, the whole block with no text.
        let mut m = Mapper::new();
        let said = feed(&mut m, &[
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"m1"}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"EqQBCkYIBxgCKkA"}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0},"parent_tool_use_id":null}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"thinking","thinking":"","signature":"EqQBCkYIBxgCKkA"},{"type":"redacted_thinking","data":"x"}]},"parent_tool_use_id":null}"#,
        ]);
        assert!(!events(&said).iter().any(|e| matches!(e, AgentEvent::ThinkingDelta(_) | AgentEvent::Started { .. })), "nothing to show: {said:?}");
        assert_eq!(events(&said), vec![AgentEvent::StepBegan, AgentEvent::ThinkingStarted, AgentEvent::ThinkingDone], "only that a call began and it thought");
        // A thought with words still comes through.
        let said = feed(&mut m, &[r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"thinking","thinking":"Check the tests first."}]},"parent_tool_use_id":null}"#]);
        assert!(events(&said).contains(&AgentEvent::ThinkingDelta("Check the tests first.".into())));
    }

    #[test]
    fn claude_codes_own_news_is_quiet_and_a_hook_with_no_verdict_is_heard() {
        let mut m = Mapper::new();
        let said = feed(&mut m, &[
            r#"{"type":"system","subtype":"hook_started","hook_name":"SessionStart"}"#,
            r#"{"type":"system","subtype":"hook_response","stdout":"","stderr":"sh: node: command not found","exit_code":127}"#,
            r#"{"type":"system","subtype":"session_state_changed","state":"running"}"#,
            r#"{"type":"system","subtype":"status","status":"requesting"}"#,
            r#"{"type":"system","subtype":"commands_changed","commands":["a","b"]}"#,
            r#"{"type":"system","subtype":"informational","level":"info","content":"Something to note"}"#,
        ]);
        assert!(said.is_empty(), "{said:?}");
        assert!(m.noise.is_empty());
        // The account's limit news is the daemon's (it holds back what it starts by itself); the transcript hears nothing of it.
        let said = feed(&mut m, &[r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}"#, r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1900000000}}"#]);
        assert_eq!(events(&said), vec![AgentEvent::RateLimit { status: "allowed".into(), resets_at: None }, AgentEvent::RateLimit { status: "rejected".into(), resets_at: Some(1_900_000_000) }]);
        let said = feed(&mut m, &[
            r#"{"type":"system","subtype":"thinking_tokens","estimated_tokens":120,"estimated_tokens_delta":40}"#,
            r#"{"type":"system","subtype":"informational","level":"warning","tag":"hook_no_verdict","content":"A hook gave no usable answer, so it didn't check this action.","hook_no_verdict":{"hook_event":"PreToolUse","tool_use_id":"toolu_1"}}"#,
        ]);
        assert_eq!(events(&said), vec![AgentEvent::ThinkingStarted, AgentEvent::Unchecked { tool_use: Some("toolu_1".into()) }]);
    }

    #[test]
    fn the_context_held_is_the_last_calls_input_and_the_window_comes_from_the_result() {
        let mut m = Mapper::new();
        feed(&mut m, &[
            r#"{"type":"assistant","message":{"id":"m1","content":[],"usage":{"input_tokens":4,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200,"output_tokens":9}},"parent_tool_use_id":null}"#,
            r#"{"type":"assistant","message":{"id":"m2","content":[],"usage":{"input_tokens":6,"cache_read_input_tokens":1200,"cache_creation_input_tokens":300,"output_tokens":9}},"parent_tool_use_id":null}"#,
            r#"{"type":"assistant","message":{"id":"s1","content":[],"usage":{"input_tokens":99999}},"parent_tool_use_id":"toolu_sub"}"#,
        ]);
        let said = feed(&mut m, &[r#"{"type":"result","subtype":"success","is_error":false,"result":"ok","usage":{"input_tokens":10,"output_tokens":18},"modelUsage":{"claude-x":{"outputTokens":18,"contextWindow":200000}}}"#]);
        let Some(AgentEvent::Usage(r)) = events(&said).into_iter().find(|e| matches!(e, AgentEvent::Usage(_))) else { panic!("no usage") };
        assert_eq!((r.context_tokens, r.context_window), (Some(1506), Some(200_000)), "a subagent's calls are not the conversation's");
    }

    #[test]
    fn permission_requests_and_withdrawals() {
        let mut m = Mapper::new();
        let said = feed(&mut m, &[
            r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"rm -rf build"},"tool_use_id":"t9"}}"#,
            r#"{"type":"control_cancel_request","request_id":"r1"}"#,
            r#"{"type":"control_request","request_id":"r2","request":{"subtype":"hook_callback"}}"#,
        ]);
        assert!(matches!(&said[0], Said::Ask { request, tool, tool_use, .. } if request == "r1" && tool == "Bash" && tool_use.as_deref() == Some("t9")));
        assert_eq!(said[1], Said::Event(AgentEvent::PermissionWithdrawn { id: "r1".into() }));
        assert_eq!(said[2], Said::Unsupported { request: "r2".into(), subtype: "hook_callback".into() });
    }

    #[test]
    fn failures_are_read_from_the_result() {
        assert_eq!(classify("Not logged in · Please run /login", true, None), Some(Failure::NotSignedIn));
        assert!(matches!(classify("Claude AI usage limit reached|1700000000", false, None), Some(Failure::UsageLimit { resets: Some(_) })));
        assert_eq!(classify("anything", true, Some(Some("at 15:00".into()))), Some(Failure::UsageLimit { resets: Some("at 15:00".into()) }));
        assert_eq!(classify("All done", false, None), None);
        assert!(matches!(classify("API Error: 500", true, None), Some(Failure::Reported(w)) if w == "API Error: 500"));
    }

    #[test]
    fn actions_for_the_policy() {
        let v = |j: &str| serde_json::from_str::<Value>(j).unwrap();
        assert_eq!(action("Read", &v(r#"{"file_path":"/w/a"}"#), "/w"), Action::Read { path: "/w/a".into() });
        assert_eq!(action("Grep", &v(r#"{"pattern":"x"}"#), "/w"), Action::Read { path: "/w".into() });
        assert_eq!(action("Write", &v(r#"{"file_path":"/etc/hosts"}"#), "/w"), Action::Edit { path: "/etc/hosts".into() });
        assert_eq!(action("Bash", &v(r#"{"command":"cargo test"}"#), "/w"), Action::Command { command: "cargo test".into() });
        assert_eq!(action("WebFetch", &v(r#"{"url":"https://example.com"}"#), "/w"), Action::Network { target: "https://example.com".into() });
        assert_eq!(action("mcp__x__y", &v(r#"{"q":"a"}"#), "/w"), Action::Mcp { server: "x".into(), tool: "y".into(), input: r#"{"q":"a"}"#.into() });
        assert!(matches!(action("mcp__x", &v("{}"), "/w"), Action::Other { .. }), "not a server's tool");
        assert_eq!(action("TodoWrite", &v("{}"), "/w"), Action::Internal { tool: "TodoWrite".into() });
    }
}

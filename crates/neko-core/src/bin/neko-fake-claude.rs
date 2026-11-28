//! `neko-fake-claude`: a stand-in for the `claude` CLI, for tests and scenes only. It never calls a model.
//!
//! It takes the same arguments Neko passes and speaks the same streaming JSON (docs/knowledge/runtimes.md), playing a script picked
//! by a token in the person's words, `[fake:NAME]` or `[fake:NAME:ARG]` (else `NEKO_FAKE_SCRIPT`, else `plain`):
//!
//! | Script | What it does |
//! | --- | --- |
//! | `plain` | A short answer, streamed, with usage and a cost. |
//! | `long:SECS` | Streams words for SECS seconds (default 300), twenty pieces a second. |
//! | `thinking` | Thinks, then answers. |
//! | `tools` | Reads a file (no question), edits one (asks; allowed, it really changes `README.md` and adds `NOTES.md`), lists a folder, then answers. |
//! | `permission` | Asks to run `cargo test` and waits; answers by what Neko said. |
//! | `destructive` | Asks to run `rm -rf build`. |
//! | `outside` | Asks to read `/etc/hosts`. |
//! | `slow` | A word every 200 ms for ten minutes (to be stopped). |
//! | `work:SECS[:step=MS][:deaf][:separate]` | SECS seconds (default 6) of steps, each its own message, MS apart (default 250). A user message that arrives meanwhile is **folded into the running turn**, as the real CLI does: it is read at the next step ("Reading your message: …") and the one `result` at the end covers both. With `separate` it is not: the turn ends with its own `result`, and the waiting message gets a turn and a `result` of its own, as a CLI that does not fold would. A `control_request` `interrupt` ends the turn at once with the CLI's interrupted result; `deaf` does not hear it. |
//! | `spawn:PIDFILE` | Starts a grandchild in its own process group (`sleep 600`), writes its pid to PIDFILE, answers, ends. |
//! | `spawn-stay:PIDFILE` | The same, then keeps streaming until stopped. |
//! | `crash` | Says something on stderr and exits with code 3. |
//! | `limit` | The account's usage limit is reached. |
//! | `signin` | Not signed in. |
//! | `question` | Answers with a question for the person (the task is then theirs to answer). |
//! | `stall` | Starts, then says nothing. |
//! | `hook-shell` | Runs `cargo test` (asking, unless the person's settings allow `Bash`), then says whether it ran and why not. |
//! | `hook-twice` | Runs `cargo test`, then `cargo build`. |
//! | `hook-read` | Reads `README.md` in its folder (Claude Code never asks about that). |
//! | `hook-destructive` | Runs `git reset --hard HEAD~1`. |
//! | `real` | The shapes a real run showed: empty thinking, several words between tools, a long absolute path, a check that exits 1. |
//! | `plugin-tool:SERVER:TOOL` | Calls the tool `mcp__SERVER__TOOL` (asking, as Claude Code would about an MCP tool), then says whether it ran. When `--mcp-config` did not give a server called SERVER there is no such tool: the call fails at once, asking nothing. |
//! | `mcp` (or `delegate`, or `steps`) | Plays the lines after the tag through the MCP servers `--mcp-config` gave, really started (stdio, with this process's environment): `mcp SERVER TOOL {json}` calls `mcp__SERVER__TOOL` (asking first, as Claude Code does) and reads the server's answer, `poll N` calls `mcp__neko__subagents` N times, `stay N` keeps streaming N seconds, `write NAME TEXT` writes a file in its folder, `sh COMMAND` runs a shell command there, `tools LABEL` records the tools Neko's server offered (`{"neko_tools": "a,b", "label": LABEL}` in the record). Steps are one per line, or several on one line apart by ` ; ` (what stands before the tag is the person's words). `steps` is the same script for a prompt that sits inside another script's step (a subagent's), where a second `mcp` tag would cut that step: its steps must be on lines of their own. Then it ends its turn. A message that begins `<neko-wake` is answered by saying which reports it read; with `NEKO_FAKE_WAKE_SH` set it first runs that shell command in its folder for each report that names a branch (`{branch}` is replaced), as a thread that merges would. A `kind="question"` wake (a kitten asks) is handled as `NEKO_FAKE_ON_QUESTION` says, rules apart by `|`, by the kitten's number: `1=answer:TEXT` answers it with `message_subagent`, `1=up` passes it up (`ask_person` in a thread, `ask_parent` in a kitten); with no rule it is read and left. |
//! | `hook-no-verdict` | Says a hook gave no usable answer for `cargo test`, then runs it anyway, as Claude Code does (it fails open). |
//!
//! `NEKO_FAKE_RECORD` names a file it appends what it was given to, one JSON line each: `{"argv": [...]}` when it starts, then
//! `{"mcp_config": {"path": ..., "content": ...}}` for each file named by `--mcp-config` (read now, so a test sees exactly what the run
//! was given even though Neko removes the file when the run ends), then `{"stdin": <line>}` for every line it reads. Tests read it to
//! see exactly what reached the CLI.
//!
//! With `--replay-user-messages` (and only then) it echoes each user message from stdin on stdout when it consumes it, as the real
//! CLI does: `{"type":"user","isReplay":true,"message":{...}}`. A message waiting for a turn of its own is echoed when that turn
//! starts; one folded into a running turn, when the turn reads it. Ordinary tool results (no `isReplay`) are not echoes.
//!
//! With `--resume ID` it continues conversation ID and says so. `--version` and `auth status --json` answer like the real CLI
//! (`NEKO_FAKE_AUTH=out` makes it signed out). It also serves as the run guard in tests (`--neko-run-guard`). With `--model NAME`
//! it reports NAME as its model (else `claude-fake-1`).
//!
//! Like Claude Code it reads the person's own rules from `$HOME/.claude/settings.json` (`permissions.allow`: a tool named there
//! is never asked about), and it runs the `PreToolUse` hooks given with `--settings` before every tool call, as the hooks
//! documentation says: exec form (`command` and `args`, no shell), the hook's JSON on stdin, `timeout` in seconds, exit 0 with
//! `hookSpecificOutput.permissionDecision` (`allow` skips the ask, `deny` blocks with `permissionDecisionReason`), exit 2 blocks,
//! anything else is a failure that blocks only with `onFailure: "block"`; without it Claude Code says `hook_no_verdict` and runs
//! the call. A denied call's result reads "PreToolUse:<tool> hook error: <reason>". `NEKO_FAKE_HOOK_LOG` names a file each hook
//! input is appended to. Around the work it sends the news a real run sends (hooks of the person's own starting and failing, its
//! state, its commands, thinking token counts), with placeholder content.

use std::io::{BufRead, Write};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

fn out(v: Value) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{v}");
    let _ = o.flush();
}

/// The model it reports: `--model NAME` when given, else `claude-fake-1`.
fn model() -> String {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == "--model").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "claude-fake-1".to_owned())
}

fn now_s() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// One `PreToolUse` hook from `--settings`.
#[derive(Clone)]
struct Hook {
    matcher: String,
    command: String,
    args: Vec<String>,
    timeout: Duration,
    block_on_failure: bool,
}

/// What the hooks decided for one call.
enum Hooked {
    Allow,
    Deny(String),
    Nothing,
}

/// The `PreToolUse` hooks in a `--settings` JSON string.
fn hooks_from(settings: Option<&str>) -> Vec<Hook> {
    let Some(v) = settings.and_then(|s| serde_json::from_str::<Value>(s).ok()) else { return Vec::new() };
    let mut out = Vec::new();
    for group in v["hooks"]["PreToolUse"].as_array().into_iter().flatten() {
        let matcher = group["matcher"].as_str().unwrap_or("").to_owned();
        for h in group["hooks"].as_array().into_iter().flatten().filter(|h| h["type"] == "command") {
            out.push(Hook {
                matcher: matcher.clone(),
                command: h["command"].as_str().unwrap_or("").to_owned(),
                args: h["args"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
                timeout: Duration::from_secs(h["timeout"].as_u64().unwrap_or(600)),
                block_on_failure: h["onFailure"] == "block",
            });
        }
    }
    out
}

/// The tools the person's own settings allow outright.
fn allowed_by_settings() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else { return Vec::new() };
    let p = std::path::Path::new(&home).join(".claude/settings.json");
    let Some(v) = std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { return Vec::new() };
    v["permissions"]["allow"].as_array().into_iter().flatten().filter_map(|r| r.as_str()).filter(|r| !r.contains('(')).map(str::to_owned).collect()
}

/// Claude Code's word that a hook gave no verdict and the call runs unchecked.
fn no_verdict(tool_use: &str) {
    out(json!({"type":"system","subtype":"informational","level":"warning","tag":"hook_no_verdict","content":"A hook gave no usable answer, so it didn't check this action.","hook_no_verdict":{"hook_event":"PreToolUse","tool_use_id":tool_use}}));
}

/// Runs one hook as the documentation says, and reads its decision.
fn run_hook(h: &Hook, input: &Value) -> Hooked {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let failed = |why: String| if h.block_on_failure { Hooked::Deny(why) } else { Hooked::Nothing };
    let Ok(mut child) = Command::new(&h.command).args(&h.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() else {
        if !h.block_on_failure {
            no_verdict(input["tool_use_id"].as_str().unwrap_or(""));
        }
        return failed(format!("PreToolUse hook error: [{}]: failed", h.command));
    };
    if let Some(mut i) = child.stdin.take() {
        let _ = writeln!(i, "{input}");
    }
    let until = Instant::now() + h.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let Some(status) = status else { return failed(format!("PreToolUse hook error: [{}]: timed out", h.command)) };
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut stdout);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut stderr);
    }
    match status.code() {
        Some(2) => Hooked::Deny(stderr.trim().to_owned()),
        Some(0) if stdout.trim().is_empty() => Hooked::Nothing,
        Some(0) => match serde_json::from_str::<Value>(stdout.trim()) {
            Ok(v) => {
                let o = &v["hookSpecificOutput"];
                let why = o["permissionDecisionReason"].as_str().unwrap_or("").to_owned();
                match o["permissionDecision"].as_str() {
                    Some("allow") => Hooked::Allow,
                    Some("deny") => Hooked::Deny(why),
                    _ => Hooked::Nothing,
                }
            }
            Err(_) => failed("PreToolUse hook error: not JSON".into()),
        },
        _ => failed(format!("PreToolUse hook error: [{}]: failed", h.command)),
    }
}

/// An MCP server from `--mcp-config`, started as Claude Code starts it: a child process speaking JSON-RPC lines on its stdio.
struct McpServer {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    lines: std::io::BufReader<std::process::ChildStdout>,
    next: u64,
    tools: Vec<String>,
}

impl McpServer {
    fn start(spec: &Value) -> Option<McpServer> {
        use std::process::{Command, Stdio};
        let mut c = Command::new(spec["command"].as_str()?);
        c.args(spec["args"].as_array().into_iter().flatten().filter_map(|a| a.as_str())).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        for (k, v) in spec["env"].as_object().into_iter().flatten() {
            if let Some(v) = v.as_str() {
                c.env(k, v);
            }
        }
        let mut child = c.spawn().ok()?;
        let stdin = child.stdin.take()?;
        let lines = std::io::BufReader::new(child.stdout.take()?);
        let mut s = McpServer { child, stdin, lines, next: 1, tools: Vec::new() };
        // As the real CLI does: a probe a server may refuse, the handshake, then the tools.
        let _ = s.request("server/discover", json!({}));
        let init = s.request("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{"roots":{"listChanged":true}},"clientInfo":{"name":"claude-code","version":"9.9.9"}}))?;
        init.get("result")?;
        s.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        let list = s.request("tools/list", json!({}))?;
        s.tools = list["result"]["tools"].as_array().into_iter().flatten().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect();
        Some(s)
    }

    fn send(&mut self, v: &Value) {
        let _ = writeln!(self.stdin, "{v}");
        let _ = self.stdin.flush();
    }

    /// One request and its answer (answers to other ids are skipped).
    fn request(&mut self, method: &str, params: Value) -> Option<Value> {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        loop {
            let mut line = String::new();
            if self.lines.read_line(&mut line).ok()? == 0 {
                return None;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if v["id"] == json!(id) {
                return Some(v);
            }
        }
    }

    /// A tool call: what the server answered, and whether it was an error.
    fn call(&mut self, tool: &str, args: &Value, tool_use_id: &str) -> (String, bool) {
        if !self.tools.iter().any(|t| t == tool) {
            return (format!("Error: No such tool available: {tool}"), true);
        }
        match self.request("tools/call", json!({"name": tool, "arguments": args, "_meta": {"claudecode/toolUseId": tool_use_id, "progressToken": 2}})) {
            Some(v) if v.get("error").is_some() => (format!("MCP error: {}", v["error"]["message"].as_str().unwrap_or("")), true),
            Some(v) => (v["result"]["content"].as_array().into_iter().flatten().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("
"), v["result"]["isError"].as_bool().unwrap_or(false)),
            None => ("MCP server closed".to_owned(), true),
        }
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Fake {
    hooks: Vec<Hook>,
    allowed: Vec<String>,
    session: String,
    resumed: bool,
    cwd: String,
    input: Receiver<Value>,
    /// Lines that arrived while a script waited for something else.
    queued: Vec<Value>,
    /// Words the person sent while a script was working: the next step reads them (`work`).
    steered: Vec<String>,
    /// `--replay-user-messages` was given: echo each user message when it is consumed.
    replay: bool,
    n: u32,
    /// The names of the MCP servers `--mcp-config` gave.
    mcp: Vec<String>,
    /// What `--mcp-config` said about each, and the ones started so far.
    mcp_specs: std::collections::HashMap<String, Value>,
    servers: std::collections::HashMap<String, McpServer>,
}

impl Fake {
    fn id(&mut self, p: &str) -> String {
        self.n += 1;
        format!("{p}_{}_{}", std::process::id(), self.n)
    }

    fn message(&mut self) -> String {
        self.id("msg")
    }

    /// Streams `text` in pieces, then the whole message.
    fn say(&mut self, text: &str, pieces: usize, gap: Duration) {
        let id = self.message();
        out(json!({"type":"stream_event","event":{"type":"message_start","message":{"id":id}},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"parent_tool_use_id":null,"session_id":self.session}));
        let chars: Vec<char> = text.chars().collect();
        let size = chars.len().div_ceil(pieces.max(1)).max(1);
        for c in chars.chunks(size) {
            out(json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":c.iter().collect::<String>()}},"parent_tool_use_id":null,"session_id":self.session}));
            std::thread::sleep(gap);
        }
        out(json!({"type":"stream_event","event":{"type":"content_block_stop","index":0},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"assistant","message":{"id":id,"model":model(),"role":"assistant","content":[{"type":"text","text":text}],"usage":{"input_tokens":8,"cache_read_input_tokens":15_200,"cache_creation_input_tokens":1_100,"output_tokens":120}},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"stream_event","event":{"type":"message_stop"},"parent_tool_use_id":null,"session_id":self.session}));
    }

    fn think(&mut self, text: &str) {
        let id = self.message();
        out(json!({"type":"stream_event","event":{"type":"message_start","message":{"id":id}},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}},"parent_tool_use_id":null,"session_id":self.session}));
        for w in text.split_inclusive(' ') {
            out(json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":w}},"parent_tool_use_id":null,"session_id":self.session}));
        }
        out(json!({"type":"stream_event","event":{"type":"content_block_stop","index":0},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"assistant","message":{"id":id,"model":model(),"content":[{"type":"thinking","thinking":text}]},"parent_tool_use_id":null,"session_id":self.session}));
    }

    /// A tool call; with `ask`, Claude Code would ask about it (unless the person's settings allow the tool or a hook allowed
    /// it), waiting for Neko's answer. Returns whether it ran.
    fn tool(&mut self, name: &str, input: Value, ask: bool, result: &str) -> bool {
        self.tool_why(name, input, ask, result, false).0
    }

    /// The same, and why it did not run. With `fails`, it ran and exited 1, as a shell check does.
    fn tool_why(&mut self, name: &str, input: Value, ask: bool, result: &str, fails: bool) -> (bool, String) {
        let (tool_id, ran, why) = self.gate(name, input, ask);
        let (content, error) = match (ran, fails) {
            (false, _) if why.is_empty() => ("The person denied this.".to_owned(), true),
            (false, _) => (format!("PreToolUse:{name} hook error: {why}"), true),
            (true, true) => (format!("Exit code 1\n{result}"), true),
            (true, false) => (result.to_owned(), false),
        };
        out(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":tool_id,"is_error":error,"content":content}]},"parent_tool_use_id":null,"session_id":self.session}));
        (ran, why)
    }

    /// Announces a tool call and settles whether it runs: the hooks, then the person's own rules, then Neko's answer. Returns the
    /// call's id, whether it runs and, when not, why.
    fn gate(&mut self, name: &str, input: Value, ask: bool) -> (String, bool, String) {
        let tool_id = self.id("toolu");
        let msg = self.message();
        out(json!({"type":"assistant","message":{"id":msg,"model":model(),"content":[{"type":"tool_use","id":tool_id,"name":name,"input":input}],"usage":{"input_tokens":6,"cache_read_input_tokens":14_000,"cache_creation_input_tokens":800,"output_tokens":40}},"parent_tool_use_id":null,"session_id":self.session}));
        // The hooks run first; a deny blocks, an allow skips the ask.
        // Exactly the keys Claude Code 2.1.331 sends.
        let hook_input = json!({"cwd":self.cwd,"effort":"medium","hook_event_name":"PreToolUse","permission_mode":"default","prompt_id":"prompt-fake-1","session_id":self.session,"tool_input":input,"tool_name":name,"tool_use_id":tool_id,"transcript_path":"/tmp/fake-transcript.jsonl"});
        let mut decided = Hooked::Nothing;
        for h in self.hooks.clone().iter().filter(|h| h.matcher.is_empty() || h.matcher == "*" || h.matcher.split('|').any(|m| m == name)) {
            if let Ok(log) = std::env::var("NEKO_FAKE_HOOK_LOG") {
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log) {
                    let _ = writeln!(f, "{hook_input}");
                }
            }
            match run_hook(h, &hook_input) {
                Hooked::Deny(why) => {
                    decided = Hooked::Deny(why);
                    break;
                }
                Hooked::Allow => decided = Hooked::Allow,
                Hooked::Nothing => {}
            }
        }
        let (ran, why) = match decided {
            Hooked::Deny(why) => (false, why),
            Hooked::Allow => (true, String::new()),
            Hooked::Nothing if !ask || self.allowed.iter().any(|a| a == name) => (true, String::new()),
            Hooked::Nothing => {
                let req = self.id("req");
                out(json!({"type":"control_request","request_id":req,"request":{"subtype":"can_use_tool","tool_name":name,"input":input,"tool_use_id":tool_id}}));
                if self.wait_answer(&req) { (true, String::new()) } else { (false, "The person denied this.".to_owned()) }
            }
        };
        (tool_id, ran, why)
    }

    /// A call to an MCP server's tool, the way Claude Code makes it: announced, asked about, then really sent to the server over its
    /// stdio, and the server's own words are the result. Returns what the server said (None when the call did not run).
    fn mcp_call(&mut self, server: &str, tool: &str, args: Value) -> Option<String> {
        let name = format!("mcp__{server}__{tool}");
        let (tool_id, ran, why) = self.gate(&name, args.clone(), true);
        let (content, error, said) = if !ran {
            (if why.is_empty() { "The person denied this.".to_owned() } else { format!("PreToolUse:{name} hook error: {why}") }, true, None)
        } else if let Some(srv) = self.server(server) {
            let began = Instant::now();
            let (text, is_error) = srv.call(tool, &args, &tool_id);
            keep(&json!({"mcp_call": {"server": server, "tool": tool, "ms": began.elapsed().as_millis() as u64, "error": is_error, "text": text}}));
            (text.clone(), is_error, Some(text))
        } else {
            (format!("Error: No such tool available: {name}"), true, None)
        };
        out(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":tool_id,"is_error":error,"content":content}]},"parent_tool_use_id":null,"session_id":self.session}));
        said
    }

    /// The server `name` from `--mcp-config`, started now if it was not (stdio only), its handshake done.
    fn server(&mut self, name: &str) -> Option<&mut McpServer> {
        if !self.servers.contains_key(name) {
            let spec = self.mcp_specs.get(name)?.clone();
            let s = McpServer::start(&spec)?;
            self.servers.insert(name.to_owned(), s);
        }
        self.servers.get_mut(name)
    }

    /// Thinking as Claude Code 2.1.331 sends it: a block with no words, only a signature.
    fn think_empty(&mut self) {
        let id = self.message();
        out(json!({"type":"stream_event","event":{"type":"message_start","message":{"id":id}},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"system","subtype":"thinking_tokens","estimated_tokens":40,"estimated_tokens_delta":40,"session_id":self.session}));
        out(json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"","estimated_tokens":40}},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"system","subtype":"thinking_tokens","estimated_tokens":95,"estimated_tokens_delta":55,"session_id":self.session}));
        out(json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"c2lnbmF0dXJl"}},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"stream_event","event":{"type":"content_block_stop","index":0},"parent_tool_use_id":null,"session_id":self.session}));
        out(json!({"type":"assistant","message":{"id":id,"model":model(),"content":[{"type":"thinking","thinking":"","signature":"c2lnbmF0dXJl"}]},"parent_tool_use_id":null,"session_id":self.session}));
    }

    fn wait_answer(&mut self, req: &str) -> bool {
        loop {
            let Ok(v) = self.input.recv() else { std::process::exit(0) };
            if v["type"] == "control_response" && v["response"]["request_id"] == req {
                return v["response"]["response"]["behavior"] == "allow";
            }
            self.queued.push(v);
        }
    }

    /// A user message was consumed: with `--replay-user-messages` the CLI says so on stdout, then goes on.
    fn consumed(&self, words: &str) {
        if self.replay {
            out(json!({"type":"user","isReplay":true,"message":{"role":"user","content":[{"type":"text","text":words}]},"parent_tool_use_id":null,"session_id":self.session}));
        }
    }

    fn result(&mut self, words: &str, error: bool) {
        out(json!({
            "type":"result","subtype":"success","is_error":error,"duration_ms":1200,"duration_api_ms":1000,"num_turns":1,"result":words,
            "session_id":self.session,"total_cost_usd":0.0123,
            "usage":{"input_tokens":1200,"cache_creation_input_tokens":0,"cache_read_input_tokens":5000,"output_tokens":340},
            "modelUsage":{ model(): {"inputTokens":1200,"outputTokens":340,"contextWindow":200000}},"permission_denials":[]
        }));
    }

    /// Reads what the person sent while a script works, without waiting: words are kept for the next step (`steered`), an interrupt
    /// is answered and returns true (a `deaf` fake says nothing and goes on, like a CLI that does not take it).
    fn listen(&mut self, fold: bool, deaf: bool) -> bool {
        while let Ok(v) = self.input.try_recv() {
            match v["type"].as_str() {
                Some("control_request") if v["request"]["subtype"] == "interrupt" => {
                    if !deaf {
                        out(json!({"type":"control_response","response":{"subtype":"success","request_id":v["request_id"],"response":{}}}));
                        return true;
                    }
                }
                Some("control_request") => out(json!({"type":"control_response","response":{"subtype":"success","request_id":v["request_id"],"response":{}}})),
                Some("user") if fold => self.steered.push(match &v["message"]["content"] {
                    Value::String(s) => s.clone(),
                    Value::Array(a) => a.iter().filter_map(|p| p["text"].as_str()).collect(),
                    _ => String::new(),
                }),
                _ => self.queued.push(v),
            }
        }
        false
    }

    /// The turn ends the way the CLI ends one the person interrupted: an error result, the conversation intact.
    fn interrupted(&mut self) {
        out(json!({
            "type":"result","subtype":"error_during_execution","is_error":true,"duration_ms":900,"num_turns":1,"result":"[Request interrupted by user]",
            "session_id":self.session,"total_cost_usd":0.004,"usage":{"input_tokens":900,"output_tokens":60},
            "modelUsage":{ model(): {"inputTokens":900,"outputTokens":60,"contextWindow":200000}},"permission_denials":[]
        }));
    }

    fn play(&mut self, words: &str) {
        let (script, arg) = if words.trim_start().starts_with("<neko-wake") { ("woken".to_owned(), String::new()) } else { pick(words) };
        let cwd = self.cwd.clone();
        match script.as_str() {
            "long" => {
                let secs: u64 = arg.parse().unwrap_or(300);
                let until = Instant::now() + Duration::from_secs(secs);
                let id = self.message();
                out(json!({"type":"stream_event","event":{"type":"message_start","message":{"id":id}},"parent_tool_use_id":null}));
                out(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"parent_tool_use_id":null}));
                let mut i = 0u64;
                while Instant::now() < until {
                    i += 1;
                    if self.listen(false, false) {
                        return self.interrupted();
                    }
                    let piece = if i % 12 == 0 { format!("word{i}.\n\n") } else { format!("word{i} ") };
                    out(json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":piece}},"parent_tool_use_id":null}));
                    std::thread::sleep(Duration::from_millis(50));
                }
                out(json!({"type":"stream_event","event":{"type":"content_block_stop","index":0},"parent_tool_use_id":null}));
                out(json!({"type":"assistant","message":{"id":id,"content":[{"type":"text","text":"(long)"}]},"parent_tool_use_id":null}));
                self.result("(long)", false);
            }
            "work" => {
                // `work:SECS` (default 6): a step every quarter second, each its own message. A message the person sends meanwhile is
                // folded into the turn, as the real CLI does: read at the next step ("Reading your message: …") and echoed then
                // (`--replay-user-messages`); the one result at the end covers both. `work:SECS:separate` leaves it waiting instead: it
                // gets a turn and a result of its own, after this one's (the main loop plays it). An interrupt ends the turn at once;
                // `work:SECS:deaf` does not hear it.
                // `work:SECS:step=MS` makes the quiet between steps MS long (250 by default): time to see what waits.
                let mut parts = arg.split(':');
                let secs = parts.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(6);
                let (mut deaf, mut separate, mut step_ms) = (false, false, 250u64);
                for p in parts {
                    match p.strip_prefix("step=") {
                        Some(ms) => step_ms = ms.parse().unwrap_or(250),
                        None => {
                            deaf = deaf || p == "deaf";
                            separate = separate || p == "separate";
                        }
                    }
                }
                let until = Instant::now() + Duration::from_secs(secs);
                let mut i = 0u32;
                while Instant::now() < until {
                    i += 1;
                    if self.listen(!separate, deaf) {
                        return self.interrupted();
                    }
                    let taken = std::mem::take(&mut self.steered);
                    for w in &taken {
                        self.consumed(w);
                    }
                    let text = if taken.is_empty() { format!("Step {i} of the work. ") } else { format!("Reading your message: {} ", taken.join(" | ")) };
                    self.say(&text, 1, Duration::from_millis(0));
                    for _ in 0..(step_ms / 50).max(1) {
                        std::thread::sleep(Duration::from_millis(50));
                        if self.listen(!separate, deaf) {
                            return self.interrupted();
                        }
                    }
                }
                // What came in during the last quarter second is read now (folded) or waits for its own turn (separate).
                let taken = std::mem::take(&mut self.steered);
                for w in &taken {
                    self.consumed(w);
                }
                let text = if taken.is_empty() { "The work is finished.".to_owned() } else { format!("Reading your message: {} The work is finished.", taken.join(" | ")) };
                self.say(&text, 1, Duration::from_millis(0));
                self.result(&text, false);
            }
            "thinking" => {
                self.think("The person wants a short answer. I will check the folder first and keep it brief.");
                self.say("Here is a short answer after some thought.", 4, Duration::from_millis(20));
                self.result("Here is a short answer after some thought.", false);
            }
            "tools" => {
                self.tool("Read", json!({"file_path": format!("{cwd}/README.md")}), false, "# Acme\nA small project.");
                if self.tool("Edit", json!({"file_path": format!("{cwd}/README.md"), "old_string":"small","new_string":"tiny"}), true, "The file was edited.") {
                    // Really change the folder, so the run card's counts come from git.
                    let readme = format!("{cwd}/README.md");
                    if let Ok(t) = std::fs::read_to_string(&readme) {
                        let _ = std::fs::write(&readme, t.replace("small", "tiny"));
                    }
                    let _ = std::fs::write(format!("{cwd}/NOTES.md"), "Notes\nfrom the fake\n");
                }
                self.tool("Glob", json!({"pattern":"src/**/*.rs"}), false, "src/main.rs\nsrc/lib.rs");
                self.say("I read the README, changed one word and listed the sources.", 3, Duration::from_millis(20));
                self.result("Done.", false);
            }
            "edit" => {
                // Every kind of change undo must take back: an edit, a new file in a new folder, a deletion, a rename, a binary.
                let p = |rel: &str| format!("{cwd}/{rel}");
                if let Ok(t) = std::fs::read_to_string(p("README.md")) {
                    let _ = std::fs::write(p("README.md"), t.replace("small", "tidy"));
                }
                let _ = std::fs::create_dir_all(p("src/retry"));
                let _ = std::fs::write(p("src/retry/mod.rs"), "pub fn attempts() -> u32 {\n    3\n}\n");
                let _ = std::fs::remove_file(p("OLD.md"));
                let _ = std::fs::rename(p("docs/guide.md"), p("docs/start.md"));
                let _ = std::fs::write(p("logo.png"), [0x89, b'P', b'N', b'G', 7, 7, 7, 7]);
                self.tool("Edit", json!({"file_path": p("README.md"), "old_string":"small","new_string":"tidy"}), false, "The file was edited.");
                self.tool("Write", json!({"file_path": p("src/retry/mod.rs")}), false, "Wrote the file.");
                self.say("I edited the README, added the retry module, removed OLD.md and moved the guide.", 3, Duration::from_millis(10));
                self.result("Done.", false);
            }
            "commit" => {
                let _ = std::fs::write(format!("{cwd}/CHANGELOG.md"), "# Changes\n\n- Retry failed pages\n");
                let git = |args: &[&str]| std::process::Command::new("git").arg("-C").arg(&cwd).args(["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "-c", "commit.gpgsign=false"]).args(args).status();
                let _ = git(&["add", "CHANGELOG.md"]);
                let _ = git(&["commit", "--quiet", "-m", "Add a changelog"]);
                self.tool("Bash", json!({"command":"git commit -m 'Add a changelog'"}), false, "1 file changed");
                self.say("I committed a changelog.", 2, Duration::from_millis(10));
                self.result("Done.", false);
            }
            "many" => {
                let n: usize = arg.parse().unwrap_or(50);
                for i in 0..n {
                    let _ = std::fs::create_dir_all(format!("{cwd}/many"));
                    let _ = std::fs::write(format!("{cwd}/many/{i}.txt"), format!("file {i}\n"));
                }
                self.say(&format!("I wrote {n} files."), 2, Duration::from_millis(10));
                self.result("Done.", false);
            }
            "permission" => {
                let ran = self.tool("Bash", json!({"command":"cargo test","description":"Run the tests"}), true, "test result: ok. 12 passed");
                self.say(if ran { "The tests pass." } else { "Okay, I did not run the tests." }, 2, Duration::from_millis(20));
                self.result("Done.", false);
            }
            "destructive" => {
                let ran = self.tool("Bash", json!({"command":"rm -rf build"}), true, "");
                self.say(if ran { "I removed the build folder." } else { "I left the build folder alone." }, 2, Duration::from_millis(20));
                self.result("Done.", false);
            }
            "outside" => {
                let ran = self.tool("Read", json!({"file_path":"/etc/hosts"}), true, "127.0.0.1 localhost");
                self.say(if ran { "I read it." } else { "I did not read it." }, 2, Duration::from_millis(20));
                self.result("Done.", false);
            }
            "slow" => {
                let id = self.message();
                out(json!({"type":"stream_event","event":{"type":"message_start","message":{"id":id}},"parent_tool_use_id":null}));
                out(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"parent_tool_use_id":null}));
                for i in 0..3000 {
                    if self.listen(false, false) {
                        return self.interrupted();
                    }
                    out(json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":format!("slow{i} ")}},"parent_tool_use_id":null}));
                    std::thread::sleep(Duration::from_millis(200));
                }
                self.result("(slow)", false);
            }
            "spawn" | "spawn-stay" => {
                spawn_grandchild(&arg);
                // An agent's command runs a while; the guard looks every 100 ms.
                std::thread::sleep(Duration::from_millis(300));
                self.tool("Bash", json!({"command":"sleep 600 &"}), false, "started");
                if script == "spawn-stay" {
                    for i in 0..3000 {
                        self.say(&format!("still here {i}"), 1, Duration::from_millis(0));
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
                self.say("I started a background job.", 2, Duration::from_millis(10));
                self.result("Done.", false);
            }
            "hook-shell" => {
                let (ran, why) = self.tool_why("Bash", json!({"command":"cargo test","description":"Run the tests"}), true, "test result: ok. 12 passed", false);
                let words = if ran { "The tests pass.".to_owned() } else { format!("I did not run the tests. Claude Code said: {why}") };
                self.say(&words, 2, Duration::from_millis(10));
                self.result(&words, false);
            }
            "hook-twice" => {
                self.tool("Bash", json!({"command":"cargo test"}), true, "test result: ok");
                self.tool("Bash", json!({"command":"cargo build --release"}), true, "Finished");
                self.say("Tested and built.", 2, Duration::from_millis(10));
                self.result("Done.", false);
            }
            "hook-read" => {
                let ran = self.tool("Read", json!({"file_path": format!("{cwd}/README.md")}), false, "# Acme");
                self.say(if ran { "I read the README." } else { "I could not read the README." }, 2, Duration::from_millis(10));
                self.result("Done.", false);
            }
            "hook-destructive" => {
                let (ran, why) = self.tool_why("Bash", json!({"command":"git reset --hard HEAD~1"}), true, "HEAD is now at 1a2b3c4", false);
                let words = if ran { "I reset the branch.".to_owned() } else { format!("I left the branch alone. Claude Code said: {why}") };
                self.say(&words, 2, Duration::from_millis(10));
                self.result(&words, false);
            }
            "plugin-tool" => {
                let (server, tool) = arg.split_once(':').unwrap_or(("acme-docs", "search"));
                let name = format!("mcp__{server}__{tool}");
                let words = if self.mcp.iter().any(|m| m == server) {
                    let (ran, why) = self.tool_why(&name, json!({"query":"retry policy"}), true, "3 results: Retry policy, Backoff, Limits", false);
                    if ran { format!("I used {tool} from {server}.") } else { format!("I did not use {tool}. Claude Code said: {why}") }
                } else {
                    // Claude Code has no such tool: nothing asks, and the call fails.
                    let tool_id = self.id("toolu");
                    let msg = self.message();
                    out(json!({"type":"assistant","message":{"id":msg,"model":model(),"content":[{"type":"tool_use","id":tool_id,"name":name,"input":{"query":"retry policy"}}]},"parent_tool_use_id":null,"session_id":self.session}));
                    out(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":tool_id,"is_error":true,"content":format!("Error: No such tool available: {name}")}]},"parent_tool_use_id":null,"session_id":self.session}));
                    format!("{server} was not available to me.")
                };
                self.say(&words, 2, Duration::from_millis(10));
                self.result(&words, false);
            }
            "mcp" | "delegate" | "steps" => {
                let mut reports = Vec::new();
                let mut stay = 0u64;
                for line in steps_of(words) {
                    let line = line.as_str();
                    if let Some(rest) = line.strip_prefix("mcp ") {
                        let mut p = rest.splitn(3, ' ');
                        let (Some(server), Some(tool)) = (p.next(), p.next()) else { continue };
                        let args = serde_json::from_str::<Value>(p.next().unwrap_or("{}")).unwrap_or(json!({}));
                        reports.push(self.mcp_call(server, tool, args).unwrap_or_else(|| "(did not run)".to_owned()));
                    } else if let Some(n) = line.strip_prefix("poll ") {
                        let n: u32 = n.trim().parse().unwrap_or(10);
                        let mut slowest = 0u128;
                        for _ in 0..n {
                            let began = Instant::now();
                            self.mcp_call("neko", "subagents", json!({}));
                            slowest = slowest.max(began.elapsed().as_millis());
                        }
                        reports.push(format!("polled {n} times, slowest {slowest} ms"));
                    } else if let Some(n) = line.strip_prefix("stay ") {
                        stay = n.trim().parse().unwrap_or(0);
                    } else if let Some(rest) = line.strip_prefix("write ") {
                        // `write NAME TEXT`: a file in the folder it works in, as an agent's edit would make it.
                        let (name, text) = rest.split_once(' ').unwrap_or((rest, ""));
                        let path = format!("{}/{name}", self.cwd);
                        let _ = std::fs::write(&path, format!("{text}\n"));
                        self.tool("Write", json!({"file_path": path, "content": text}), false, "Wrote the file.");
                        reports.push(format!("wrote {name}"));
                    } else if let Some(cmd) = line.strip_prefix("sh ") {
                        // `sh COMMAND`: a shell command in the folder it works in (git add, commit, merge), its output is the result.
                        let done = std::process::Command::new("/bin/sh").arg("-c").arg(cmd).current_dir(&self.cwd).stdin(std::process::Stdio::null()).output();
                        let text = done.map(|o| format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))).unwrap_or_default();
                        self.tool("Bash", json!({"command": cmd}), false, text.trim());
                        reports.push(format!("ran {cmd}"));
                    } else if line == "tools" || line.starts_with("tools ") {
                        // The tools Neko's own server offered this run, as the CLI would list them.
                        let names = self.server("neko").map(|s| s.tools.join(",")).unwrap_or_default();
                        keep(&json!({"neko_tools": names, "label": line.trim_start_matches("tools").trim()}));
                        reports.push(format!("tools {names}"));
                    }
                }
                // `stay N` keeps working up to N seconds, and ends early when a message of the person's (or Neko's: a report) arrives, so
                // a test waits on that event and not on a guess of how long a loaded machine takes.
                let until = Instant::now() + Duration::from_secs(stay);
                while Instant::now() < until {
                    self.say("working on my own part ", 1, Duration::from_millis(0));
                    std::thread::sleep(Duration::from_millis(300));
                    self.listen(false, false);
                    if self.queued.iter().any(|v| v["type"] == "user") {
                        break;
                    }
                }
                let text = format!("Done with my calls: {} call(s). {}", reports.len(), reports.last().cloned().unwrap_or_default());
                self.say(&text, 3, Duration::from_millis(10));
                self.result(&text, false);
            }
            "woken" => {
                let n = words.matches("<neko-wake").count();
                let mut seen = Vec::new();
                for chunk in words.split("<neko-wake").skip(1) {
                    let attr = |k: &str| chunk.split(&format!("{k}=\"")).nth(1).and_then(|r| r.split('"').next()).unwrap_or("?").to_owned();
                    let first = chunk.split("<summary>").nth(1).and_then(|s| s.lines().next()).unwrap_or("").trim().to_owned();
                    if chunk.starts_with(" kind=\"question\"") {
                        // A kitten's question: `NEKO_FAKE_ON_QUESTION` says what this agent does with it, by the kitten's number.
                        let id = attr("id");
                        let q = chunk.split("<question>").nth(1).and_then(|r| r.split("</question>").next()).unwrap_or("").replace("&lt;", "<").replace("&amp;", "&");
                        seen.push(format!("Subagent {id} asked: {q}"));
                        let rule = std::env::var("NEKO_FAKE_ON_QUESTION").unwrap_or_default();
                        let mine = rule.split('|').find_map(|r| r.split_once('=').filter(|(k, _)| k.trim() == id).map(|(_, v)| v.trim().to_owned()));
                        match mine.as_deref() {
                            Some(v) if v.starts_with("answer:") => {
                                let text = v.trim_start_matches("answer:");
                                seen.push(self.mcp_call("neko", "message_subagent", json!({"id": id, "text": text})).unwrap_or_default());
                            }
                            Some("up") => {
                                // A thread asks the person; a kitten asks its own parent.
                                let tool = if self.server("neko").is_some_and(|s| s.tools.iter().any(|t| t == "ask_person")) { "ask_person" } else { "ask_parent" };
                                seen.push(self.mcp_call("neko", tool, json!({"question": q, "subagent": id})).unwrap_or_default());
                            }
                            _ => {}
                        }
                        continue;
                    }
                    seen.push(format!("Subagent {} {}: {}", attr("id"), attr("status"), first));
                    // `NEKO_FAKE_WAKE_SH`: what a thread that merges does with each branch a report names (`{branch}` is replaced).
                    if let (Ok(template), Some(branch)) = (std::env::var("NEKO_FAKE_WAKE_SH"), chunk.split("<branch>").nth(1).and_then(|r| r.split("</branch>").next())) {
                        let cmd = template.replace("{branch}", branch);
                        let done = std::process::Command::new("/bin/sh").arg("-c").arg(&cmd).current_dir(&self.cwd).stdin(std::process::Stdio::null()).output();
                        let text = done.map(|o| format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))).unwrap_or_default();
                        self.tool("Bash", json!({"command": cmd}), false, text.trim());
                    }
                }
                let text = format!("I read {n} report(s). {}", seen.join(" | "));
                self.say(&text, 3, Duration::from_millis(10));
                self.result(&text, false);
            }
            "hook-no-verdict" => {
                let tool_id = self.id("toolu");
                let msg = self.message();
                out(json!({"type":"assistant","message":{"id":msg,"model":model(),"content":[{"type":"tool_use","id":tool_id,"name":"Bash","input":{"command":"cargo test","description":"Run the tests"}}]},"parent_tool_use_id":null,"session_id":self.session}));
                no_verdict(&tool_id);
                out(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":tool_id,"is_error":false,"content":"test result: ok"}]},"parent_tool_use_id":null,"session_id":self.session}));
                std::thread::sleep(Duration::from_millis(300));
                self.say("The tests pass.", 2, Duration::from_millis(10));
                self.result("The tests pass.", false);
            }
            "real" => {
                self.think_empty();
                self.tool("Read", json!({"file_path":"/Users/example/Documents/acme-api/docs/guides/getting-started/installing-on-a-new-mac.md"}), false, "# Installing\nSteps.");
                self.think_empty();
                self.say("The guide is current. I will check the branches next.", 3, Duration::from_millis(5));
                self.tool("Bash", json!({"command":"git status --short && git branch --list 'neko/*' --format='%(refname:short) %(upstream:track)' && git log --oneline -5 --decorate"}), false, "?? notes.txt\nneko/x7k2m9qa [ahead 1]");
                self.say("One branch is ahead. Checking whether anything is left to commit.", 3, Duration::from_millis(5));
                self.tool_why("Bash", json!({"command":"git diff --quiet && git diff --cached --quiet"}), false, "", true);
                self.think_empty();
                self.say("There are uncommitted changes, so I left them as they are.\n\n- `notes.txt` is new and untracked\n- `neko/x7k2m9qa` is one commit ahead", 4, Duration::from_millis(5));
                self.result("There are uncommitted changes.", false);
            }
            "question" => {
                let text = "I found the button in four places.\n\nShould I rename all four to **Preferences**, or keep Settings?";
                self.say(text, 4, Duration::from_millis(10));
                self.result(text, false);
            }
            "crash" => {
                eprintln!("fatal: the fake fell over");
                std::process::exit(3);
            }
            "limit" => {
                let resets = now_s() + 3600;
                out(json!({"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":resets,"rateLimitType":"five_hour"},"session_id":self.session}));
                self.result(&format!("Claude AI usage limit reached|{resets}"), true);
            }
            "signin" => {
                self.result("Not logged in · Please run /login", true);
                std::process::exit(1);
            }
            "stall" => loop {
                std::thread::sleep(Duration::from_secs(60));
            },
            _ => {
                let text = if self.resumed {
                    format!("I remember our conversation {}. You said: {}", self.session, words.trim())
                } else {
                    "Hello from the fake **Claude Code**. Nothing was changed.".to_owned()
                };
                self.say(&text, 5, Duration::from_millis(15));
                self.result(&text, false);
            }
        }
    }
}

/// The script named in the words: `[fake:NAME]` or `[fake:NAME:ARG]`.
fn pick(words: &str) -> (String, String) {
    let tag = words.find("[fake:").and_then(|i| words[i + 6..].find(']').map(|j| words[i + 6..i + 6 + j].to_owned())).or_else(|| std::env::var("NEKO_FAKE_SCRIPT").ok()).unwrap_or_else(|| "plain".into());
    match tag.split_once(':') {
        Some((a, b)) => (a.to_owned(), b.to_owned()),
        None => (tag, String::new()),
    }
}

/// The steps of a `delegate` script: one per line, and also several on one line apart by ` ; ` (a composer takes one line). A step may
/// follow the script's own tag (`[fake:delegate] mcp neko start_subagent {...}`); text after the tag in a later step is left alone.
fn steps_of(words: &str) -> Vec<String> {
    words
        .lines()
        .flat_map(|l| l.split(" ; "))
        .map(|p| {
            // The script's own tag ends the person's words in front of it: what follows the tag is a step.
            let at = ["[fake:delegate]", "[fake:mcp]"].iter().filter_map(|t| p.find(t).map(|i| i + t.len())).min();
            at.map_or(p, |i| &p[i..]).trim().to_owned()
        })
        .collect()
}

/// A grandchild in its own process group, as an agent's shell command would be, its pid written to `pidfile`.
fn spawn_grandchild(pidfile: &str) {
    use std::os::unix::process::CommandExt;
    let script = format!("/bin/sleep 600 & echo $! > '{pidfile}.tmp' && mv '{pidfile}.tmp' '{pidfile}'; wait");
    let _ = std::process::Command::new("/bin/sh").arg("-c").arg(script).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).process_group(0).spawn();
    let until = Instant::now() + Duration::from_secs(5);
    while !std::path::Path::new(pidfile).exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Appends `v` to the file `NEKO_FAKE_RECORD` names, if any.
fn keep(v: &Value) {
    if let Some(r) = std::env::var_os("NEKO_FAKE_RECORD").filter(|r| !r.is_empty())
        && let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(r)
    {
        // One write per line: `writeln!` on a File writes each formatted piece separately, and several fakes (a parent and its
        // kittens) append to the same record at once, so their pieces interleaved and broke lines. A single append write is not split.
        let line = format!("{v}\n");
        let _ = f.write_all(line.as_bytes());
    }
}

fn main() {
    neko_core::guard::run_if_requested();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        println!("9.9.9 (Claude Code)");
        return;
    }
    if args.first().map(String::as_str) == Some("auth") {
        if std::env::var("NEKO_FAKE_AUTH").as_deref() == Ok("out") {
            println!(r#"{{"loggedIn":false,"authMethod":"none","apiProvider":"firstParty"}}"#);
            std::process::exit(1);
        }
        println!(r#"{{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","subscriptionType":"max"}}"#);
        return;
    }
    keep(&json!({ "argv": args }));
    // The MCP config files given, read now: the run's own copy goes when the run ends.
    let mut mcp = Vec::new();
    let mut mcp_specs = std::collections::HashMap::new();
    for (i, a) in args.iter().enumerate() {
        if a == "--mcp-config"
            && let Some(path) = args.get(i + 1)
        {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let content = serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
            if let Some(m) = content["mcpServers"].as_object() {
                mcp.extend(m.keys().cloned());
                mcp_specs.extend(m.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            keep(&json!({ "mcp_config": { "path": path, "content": content } }));
        }
    }
    let resume = args.iter().position(|a| a == "--resume").and_then(|i| args.get(i + 1)).cloned();
    let settings = args.iter().position(|a| a == "--settings").and_then(|i| args.get(i + 1)).cloned();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                keep(&json!({ "stdin": v }));
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });
    let cwd = std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    let session = resume.clone().unwrap_or_else(|| format!("fake-{}-{}", std::process::id(), now_s()));
    let mut fake = Fake { hooks: hooks_from(settings.as_deref()), allowed: allowed_by_settings(), session: session.clone(), resumed: resume.is_some(), cwd: cwd.clone(), input: rx, queued: Vec::new(), steered: Vec::new(), replay: args.iter().any(|a| a == "--replay-user-messages"), n: 0, mcp, mcp_specs, servers: Default::default() };
    let mut started = false;
    loop {
        let v = if fake.queued.is_empty() {
            match fake.input.recv() {
                Ok(v) => v,
                Err(_) => break,
            }
        } else {
            fake.queued.remove(0)
        };
        match v["type"].as_str() {
            Some("control_request") => {
                out(json!({"type":"control_response","response":{"subtype":"success","request_id":v["request_id"],"response":{}}}));
            }
            Some("user") => {
                if !started {
                    started = true;
                    // The news a real run sends first; none of it is for a person. The person's own hook fails under a short PATH.
                    out(json!({"type":"system","subtype":"hook_started","hook_name":"SessionStart:startup","session_id":session}));
                    out(json!({"type":"system","subtype":"hook_response","hook_name":"SessionStart:startup","stdout":"","stderr":"sh: acme-hook: command not found","exit_code":127,"session_id":session}));
                    let tools: Vec<String> = ["Read", "Edit", "Bash", "Glob"].iter().map(|t| (*t).to_owned()).chain((0..300).map(|i| format!("mcp__acme__tool_{i}"))).collect();
                    let mode = if args.iter().any(|a| a == "--permission-mode") { "default" } else { "auto" };
                    out(json!({"type":"system","subtype":"init","session_id":session,"model":model(),"cwd":cwd,"tools":tools,"permissionMode":mode,"apiKeySource":"none","claude_code_version":"9.9.9"}));
                    out(json!({"type":"system","subtype":"commands_changed","commands":(0..500).map(|i| format!("/command-{i}")).collect::<Vec<_>>(),"session_id":session}));
                    out(json!({"type":"system","subtype":"session_state_changed","state":"running","session_id":session}));
                    out(json!({"type":"system","subtype":"status","status":"requesting","session_id":session}));
                    out(json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"five_hour"},"session_id":session}));
                }
                let words: String = match &v["message"]["content"] {
                    Value::String(s) => s.clone(),
                    Value::Array(a) => a.iter().filter_map(|p| p["text"].as_str()).collect(),
                    _ => String::new(),
                };
                // This message is consumed now: the CLI starts a turn on it.
                fake.consumed(&words);
                fake.play(&words);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::steps_of;

    #[test]
    fn a_delegate_script_reads_lines_and_semicolons_alike() {
        let one = "Check two things [fake:delegate] mcp neko start_subagent {\"title\":\"A\",\"prompt\":\"look [fake:long:2]\"} ; mcp neko start_subagent {\"title\":\"B\"} ; stay 1";
        let steps = steps_of(one);
        assert_eq!(steps.len(), 3, "{steps:?}");
        assert_eq!(steps[0], "mcp neko start_subagent {\"title\":\"A\",\"prompt\":\"look [fake:long:2]\"}", "the words before the tag are not a step");
        assert_eq!(steps[1], "mcp neko start_subagent {\"title\":\"B\"}");
        assert_eq!(steps[2], "stay 1");
        // The newline form still works.
        let lines = steps_of("[fake:delegate]\nmcp neko start_subagent {\"title\":\"A\"}\npoll 3");
        assert_eq!(lines, ["", "mcp neko start_subagent {\"title\":\"A\"}", "poll 3"]);
    }
}

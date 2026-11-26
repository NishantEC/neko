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
//! | `hook-no-verdict` | Says a hook gave no usable answer for `cargo test`, then runs it anyway, as Claude Code does (it fails open). |
//!
//! `NEKO_FAKE_RECORD` names a file it appends what it was given to, one JSON line each: `{"argv": [...]}` when it starts, then
//! `{"stdin": <line>}` for every line it reads. Tests read it to see exactly what reached the CLI.
//!
//! With `--resume ID` it continues conversation ID and says so. `--version` and `auth status --json` answer like the real CLI
//! (`NEKO_FAKE_AUTH=out` makes it signed out). It also serves as the run guard in tests (`--neko-run-guard`).
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

struct Fake {
    hooks: Vec<Hook>,
    allowed: Vec<String>,
    session: String,
    resumed: bool,
    cwd: String,
    input: Receiver<Value>,
    /// Lines that arrived while a script waited for something else.
    queued: Vec<Value>,
    n: u32,
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
        out(json!({"type":"assistant","message":{"id":id,"model":"claude-fake-1","role":"assistant","content":[{"type":"text","text":text}],"usage":{"input_tokens":8,"cache_read_input_tokens":15_200,"cache_creation_input_tokens":1_100,"output_tokens":120}},"parent_tool_use_id":null,"session_id":self.session}));
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
        out(json!({"type":"assistant","message":{"id":id,"model":"claude-fake-1","content":[{"type":"thinking","thinking":text}]},"parent_tool_use_id":null,"session_id":self.session}));
    }

    /// A tool call; with `ask`, Claude Code would ask about it (unless the person's settings allow the tool or a hook allowed
    /// it), waiting for Neko's answer. Returns whether it ran.
    fn tool(&mut self, name: &str, input: Value, ask: bool, result: &str) -> bool {
        self.tool_why(name, input, ask, result, false).0
    }

    /// The same, and why it did not run. With `fails`, it ran and exited 1, as a shell check does.
    fn tool_why(&mut self, name: &str, input: Value, ask: bool, result: &str, fails: bool) -> (bool, String) {
        let tool_id = self.id("toolu");
        let msg = self.message();
        out(json!({"type":"assistant","message":{"id":msg,"model":"claude-fake-1","content":[{"type":"tool_use","id":tool_id,"name":name,"input":input}],"usage":{"input_tokens":6,"cache_read_input_tokens":14_000,"cache_creation_input_tokens":800,"output_tokens":40}},"parent_tool_use_id":null,"session_id":self.session}));
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
        let (content, error) = match (ran, fails) {
            (false, _) if why.is_empty() => ("The person denied this.".to_owned(), true),
            (false, _) => (format!("PreToolUse:{name} hook error: {why}"), true),
            (true, true) => (format!("Exit code 1\n{result}"), true),
            (true, false) => (result.to_owned(), false),
        };
        out(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":tool_id,"is_error":error,"content":content}]},"parent_tool_use_id":null,"session_id":self.session}));
        (ran, why)
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
        out(json!({"type":"assistant","message":{"id":id,"model":"claude-fake-1","content":[{"type":"thinking","thinking":"","signature":"c2lnbmF0dXJl"}]},"parent_tool_use_id":null,"session_id":self.session}));
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

    fn result(&mut self, words: &str, error: bool) {
        out(json!({
            "type":"result","subtype":"success","is_error":error,"duration_ms":1200,"duration_api_ms":1000,"num_turns":1,"result":words,
            "session_id":self.session,"total_cost_usd":0.0123,
            "usage":{"input_tokens":1200,"cache_creation_input_tokens":0,"cache_read_input_tokens":5000,"output_tokens":340},
            "modelUsage":{"claude-fake-1":{"inputTokens":1200,"outputTokens":340,"contextWindow":200000}},"permission_denials":[]
        }));
    }

    fn play(&mut self, words: &str) {
        let (script, arg) = pick(words);
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
                    let piece = if i % 12 == 0 { format!("word{i}.\n\n") } else { format!("word{i} ") };
                    out(json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":piece}},"parent_tool_use_id":null}));
                    std::thread::sleep(Duration::from_millis(50));
                }
                out(json!({"type":"stream_event","event":{"type":"content_block_stop","index":0},"parent_tool_use_id":null}));
                out(json!({"type":"assistant","message":{"id":id,"content":[{"type":"text","text":"(long)"}]},"parent_tool_use_id":null}));
                self.result("(long)", false);
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
            "hook-no-verdict" => {
                let tool_id = self.id("toolu");
                let msg = self.message();
                out(json!({"type":"assistant","message":{"id":msg,"model":"claude-fake-1","content":[{"type":"tool_use","id":tool_id,"name":"Bash","input":{"command":"cargo test","description":"Run the tests"}}]},"parent_tool_use_id":null,"session_id":self.session}));
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
        let _ = writeln!(f, "{v}");
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
    let mut fake = Fake { hooks: hooks_from(settings.as_deref()), allowed: allowed_by_settings(), session: session.clone(), resumed: resume.is_some(), cwd: cwd.clone(), input: rx, queued: Vec::new(), n: 0 };
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
                    out(json!({"type":"system","subtype":"init","session_id":session,"model":"claude-fake-1","cwd":cwd,"tools":tools,"permissionMode":mode,"apiKeySource":"none","claude_code_version":"9.9.9"}));
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
                fake.play(&words);
            }
            _ => {}
        }
    }
}

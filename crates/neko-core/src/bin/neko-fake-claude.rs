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
//! | `stall` | Starts, then says nothing. |
//!
//! With `--resume ID` it continues conversation ID and says so. `--version` and `auth status --json` answer like the real CLI
//! (`NEKO_FAKE_AUTH=out` makes it signed out). It also serves as the run guard in tests (`--neko-run-guard`).

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

struct Fake {
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
        out(json!({"type":"assistant","message":{"id":id,"model":"claude-fake-1","role":"assistant","content":[{"type":"text","text":text}]},"parent_tool_use_id":null,"session_id":self.session}));
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

    /// A tool call; with `ask`, a permission request first, waiting for Neko's answer. Returns whether it ran.
    fn tool(&mut self, name: &str, input: Value, ask: bool, result: &str) -> bool {
        let tool_id = self.id("toolu");
        let msg = self.message();
        out(json!({"type":"assistant","message":{"id":msg,"model":"claude-fake-1","content":[{"type":"tool_use","id":tool_id,"name":name,"input":input}]},"parent_tool_use_id":null,"session_id":self.session}));
        let allowed = !ask || {
            let req = self.id("req");
            out(json!({"type":"control_request","request_id":req,"request":{"subtype":"can_use_tool","tool_name":name,"input":input,"tool_use_id":tool_id}}));
            self.wait_answer(&req)
        };
        let content = if allowed { result.to_owned() } else { "The person denied this.".to_owned() };
        out(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":tool_id,"is_error":!allowed,"content":content}]},"parent_tool_use_id":null,"session_id":self.session}));
        allowed
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
            "modelUsage":{"claude-fake-1":{"inputTokens":1200,"outputTokens":340}},"permission_denials":[]
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
    let resume = args.iter().position(|a| a == "--resume").and_then(|i| args.get(i + 1)).cloned();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });
    let cwd = std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    let session = resume.clone().unwrap_or_else(|| format!("fake-{}-{}", std::process::id(), now_s()));
    let mut fake = Fake { session: session.clone(), resumed: resume.is_some(), cwd: cwd.clone(), input: rx, queued: Vec::new(), n: 0 };
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
                    out(json!({"type":"system","subtype":"init","session_id":session,"model":"claude-fake-1","cwd":cwd,"tools":["Read","Edit","Bash","Glob"],"permissionMode":"default","apiKeySource":"none","claude_code_version":"9.9.9"}));
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

//! The Claude Code runtime (docs/knowledge/runtimes.md): the `claude` CLI already installed and signed in on this Mac, run headless
//! with streaming JSON in and out, under the run guard. Neko never passes, reads or stores a credential: the CLI uses its own login.
//!
//! The command line:
//!
//! ```text
//! claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages
//!        --permission-prompt-tool stdio [--permission-mode default] [--settings <hook>] [--resume <session>]
//! ```
//!
//! The person's words go in as a `user` message on stdin. With `--permission-prompt-tool stdio`, every step Claude Code would ask
//! about arrives as a `can_use_tool` control request on stdout and waits for Neko's `control_response`; Neko's policy decides it or
//! asks the person in the task. Once every message sent has its `result`, stdin closes and the CLI ends. A follow-up after that is a
//! new run with `--resume` and the conversation id the first one reported.
//!
//! The permission mode (docs/knowledge/permissions.md): following Claude Code's own rules, Neko passes no `--permission-mode` and
//! no settings, so the person's own default mode and rules apply as in the terminal. Asking before risky steps, it passes
//! `--permission-mode default` and, for that run only, a `PreToolUse` hook in `--settings` whose command is the daemon's own binary
//! as `--hook` ([`hook_settings`]). Claude Code adds those hooks to the person's own; it never writes them to a file. Full access
//! passes `--permission-mode default` and answers every ask with yes.

pub mod stream;

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::find;
use crate::guard::{self, Stopper};
use crate::runtime::{AgentEvent, Answer, Control, ControlError, Detection, Failure, HOOK_PORT_ENV, HOOK_SECRET_ENV, Login, Outcome, Runtime, StartSpec, Started, shorten};
use neko_protocol::PermissionMode;

pub mod rules;
use stream::{Mapper, Said};

/// How long `--version` and `auth status` may take.
const PROBE_LIMIT: Duration = Duration::from_secs(10);

/// Claude Code, found on this Mac.
pub struct ClaudeRuntime {
    /// Use this binary instead of looking for `claude` (tests point it at a fake).
    pub binary: Option<PathBuf>,
    /// The binary that acts as the run guard (the daemon's own).
    pub guard: PathBuf,
    /// More environment for the CLI (tests give the fake a HOME of its own).
    pub env: Vec<(String, String)>,
}

impl ClaudeRuntime {
    pub fn new(binary: Option<PathBuf>, guard: PathBuf) -> ClaudeRuntime {
        ClaudeRuntime { binary, guard, env: Vec::new() }
    }

    pub fn with_env(mut self, key: &str, value: &str) -> ClaudeRuntime {
        self.env.push((key.to_owned(), value.to_owned()));
        self
    }

    fn path(&self) -> Option<PathBuf> {
        find::find("claude", self.binary.as_deref())
    }
}

/// How long Claude Code waits for the hook helper, in seconds (its `timeout` field): a day. The person may answer late; the
/// daemon denies before then.
pub const HOOK_TIMEOUT_S: u64 = 86_400;

/// The settings JSON Neko passes with `--settings` when it asks before risky steps: one `PreToolUse` hook for every tool, run in
/// exec form (no shell) as `<helper> --hook`, waiting up to a day, and blocking the call if the helper fails or times out.
pub fn hook_settings(helper: &Path) -> String {
    json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{
                    "type": "command",
                    "command": helper.to_string_lossy(),
                    "args": ["--hook"],
                    "timeout": HOOK_TIMEOUT_S,
                    "onFailure": "block"
                }]
            }]
        }
    })
    .to_string()
}

/// The arguments for a run (after the program). `hook` is the hook helper's path, in "Ask before risky steps".
pub fn arguments(resume: Option<&str>, mode: PermissionMode, hook: Option<&Path>) -> Vec<String> {
    let mut a: Vec<String> = ["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--permission-prompt-tool", "stdio"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    // Following Claude Code's own rules, the person's own default mode applies, as in the terminal.
    if mode != PermissionMode::Follow {
        a.push("--permission-mode".into());
        a.push("default".into());
    }
    if mode == PermissionMode::Ask
        && let Some(h) = hook
    {
        a.push("--settings".into());
        a.push(hook_settings(h));
    }
    if let Some(id) = resume {
        a.push("--resume".into());
        a.push(id.to_owned());
    }
    a
}

/// Reads `claude auth status --json`: whether it is signed in and with which login. Never its account details.
pub fn read_auth(out: &[u8]) -> (Option<bool>, Option<Login>) {
    let Ok(v) = serde_json::from_slice::<Value>(out) else { return (None, None) };
    let signed_in = v["loggedIn"].as_bool();
    let login = match (v["authMethod"].as_str(), v["apiProvider"].as_str()) {
        (_, Some(p)) if p != "firstParty" => Some(Login::Cloud(match p {
            "bedrock" => "Amazon Bedrock".to_owned(),
            "vertex" => "Google Vertex AI".to_owned(),
            other => other.to_owned(),
        })),
        (Some("claude.ai"), _) => Some(Login::Subscription(v["subscriptionType"].as_str().filter(|s| !s.is_empty()).map(str::to_owned))),
        (Some(m), _) if m.contains("api") || m.contains("console") => Some(Login::ApiKey),
        (Some(m), _) if !m.is_empty() && m != "none" => Some(Login::Other(m.to_owned())),
        _ => None,
    };
    (signed_in, login.filter(|_| signed_in != Some(false)))
}

impl Runtime for ClaudeRuntime {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn sign_in_line(&self) -> &'static str {
        "claude auth login"
    }

    fn install_line(&self) -> &'static str {
        "curl -fsSL https://claude.ai/install.sh | bash"
    }

    fn detect(&self) -> Detection {
        let Some(path) = self.path() else { return Detection::default() };
        let probe = |args: &[&str]| {
            let mut c = Command::new(&path);
            c.args(args).stdin(Stdio::null()).stderr(Stdio::null()).env("PATH", find::child_path(&path));
            find::run_for(c, PROBE_LIMIT)
        };
        let version = probe(&["--version"]).and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(|l| l.trim().trim_end_matches("(Claude Code)").trim().to_owned())).filter(|v| !v.is_empty());
        let (signed_in, login) = probe(&["auth", "status", "--json"]).map(|o| read_auth(&o.stdout)).unwrap_or((None, None));
        Detection { found: true, path: Some(path), version, signed_in, login }
    }

    fn start(&self, spec: StartSpec) -> Result<Started, Failure> {
        let path = self.path().ok_or(Failure::NotInstalled)?;
        if !spec.workdir.is_dir() {
            return Err(Failure::FolderGone);
        }
        // Claude Code runs a call unchecked when its hook cannot run: asking before risky steps, no helper, no run.
        if spec.mode == PermissionMode::Ask && !spec.hook.as_ref().is_some_and(|h| find::executable(&h.helper)) {
            return Err(Failure::HookMissing);
        }
        let mut env = vec![("PATH".to_owned(), find::child_path(&path))];
        env.extend(self.env.iter().cloned());
        // The hook helper finds the daemon through these; Claude Code's hooks inherit its environment.
        let hook = spec.hook.as_ref().filter(|_| spec.mode == PermissionMode::Ask);
        if let Some(h) = hook {
            env.push((HOOK_PORT_ENV.to_owned(), h.port.to_string()));
            env.push((HOOK_SECRET_ENV.to_owned(), h.secret.clone()));
        }
        let args = arguments(spec.resume.as_deref(), spec.mode, hook.map(|h| h.helper.as_path()));
        let guarded = guard::spawn(&self.guard, &path, &args, &spec.workdir, &env, |c| {
            c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        })
        .map_err(|e| Failure::Crashed { code: None, words: format!("Neko could not start Claude Code: {e}") })?;
        let guard::Guarded { mut child, stopper } = guarded;
        let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            stopper.stop();
            let _ = child.wait();
            return Err(Failure::Crashed { code: None, words: "Claude Code started without its pipes.".into() });
        };
        let session = Arc::new(Session { stdin: Mutex::new(Some(stdin)), stopper, stopped: AtomicBool::new(false), sent: AtomicU32::new(0), asks: Mutex::new(Default::default()) });
        let (tx, rx) = mpsc::channel();
        // The first line asks the CLI to set up its control channel, as the Agent SDK does; then the words.
        let _ = session.write(&json!({"type": "control_request", "request_id": "neko-init", "request": {"subtype": "initialize", "hooks": {}}}));
        if session.steer(&spec.words).is_err() {
            session.stop();
        }
        let tail = Arc::new(Mutex::new(Vec::<u8>::new()));
        let tail_w = tail.clone();
        let _ = std::thread::Builder::new().name("neko-claude-stderr".into()).spawn(move || {
            let mut r = BufReader::new(stderr);
            let mut buf = [0u8; 4096];
            while let Ok(n) = r.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let mut t = tail_w.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                t.extend_from_slice(&buf[..n]);
                let len = t.len();
                if len > 8192 {
                    t.drain(..len - 8192);
                }
            }
        });
        let s = session.clone();
        let workdir = spec.workdir.to_string_lossy().into_owned();
        let _ = std::thread::Builder::new().name("neko-claude-stdout".into()).spawn(move || {
            let mut mapper = Mapper::new();
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                for said in mapper.feed(&line, &workdir) {
                    s.said(said, &workdir, &tx);
                }
            }
            s.close_stdin();
            let status = child.wait().ok();
            // Let the stderr reader take the last words.
            std::thread::sleep(Duration::from_millis(30));
            let tail = String::from_utf8_lossy(&tail.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).into_owned();
            let outcome = outcome(&mapper, s.stopped.load(Ordering::SeqCst), status.and_then(|st| st.code()), status.is_some_and(|st| st.success()), &tail);
            let _ = tx.send(AgentEvent::Finished(outcome));
        });
        let control: Arc<dyn Control> = session;
        Ok(Started { control, events: rx })
    }
}

/// How a run ended, from what the stream said, whether it was stopped, and how the process ended.
fn outcome(m: &Mapper, stopped: bool, code: Option<i32>, success: bool, stderr: &str) -> Outcome {
    if stopped {
        return Outcome::Stopped;
    }
    if let Some(f) = &m.failure {
        return Outcome::Failed(f.clone());
    }
    if m.turns > 0 && (success || code.is_none()) {
        return Outcome::Done;
    }
    let lower = stderr.to_lowercase();
    if lower.contains("/login") || lower.contains("not logged in") || lower.contains("invalid api key") {
        return Outcome::Failed(Failure::NotSignedIn);
    }
    if let Some(Some(f)) = (m.limit.is_some()).then(|| stream::classify("usage limit", true, m.limit.clone())) {
        return Outcome::Failed(f);
    }
    if m.turns > 0 {
        return Outcome::Done;
    }
    let words = stderr.lines().rev().find(|l| !l.trim().is_empty()).map(str::trim).or_else(|| m.noise.last().map(String::as_str)).unwrap_or("").to_owned();
    Outcome::Failed(Failure::Crashed { code, words: shorten(&words, 400) })
}

/// The running CLI.
struct Session {
    stdin: Mutex<Option<ChildStdin>>,
    stopper: Stopper,
    stopped: AtomicBool,
    /// User messages sent; when every one has its result, stdin closes and the CLI ends.
    sent: AtomicU32,
    /// Requests waiting for an answer: their input and tool-use id.
    asks: Mutex<std::collections::HashMap<String, (Value, Option<String>)>>,
}

impl Session {
    fn write(&self, v: &Value) -> Result<(), ControlError> {
        let mut g = self.stdin.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(w) = g.as_mut() else { return Err(ControlError("Claude Code has finished this turn.".into())) };
        let mut line = v.to_string();
        line.push('\n');
        w.write_all(line.as_bytes()).and_then(|()| w.flush()).map_err(|e| ControlError(format!("Claude Code is not listening: {e}")))
    }

    fn close_stdin(&self) {
        self.stdin.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    }

    fn said(&self, said: Said, workdir: &str, tx: &Sender<AgentEvent>) {
        match said {
            Said::Event(e) => {
                if let AgentEvent::PermissionWithdrawn { id } = &e {
                    self.asks.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(id);
                }
                let _ = tx.send(e);
            }
            Said::Ask { request, tool, input, tool_use } => {
                let action = stream::action(&tool, &input, workdir);
                self.asks.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(request.clone(), (input, tool_use.clone()));
                let _ = tx.send(AgentEvent::PermissionAsked { id: request, tool, action, tool_use });
            }
            Said::Unsupported { request, subtype } => {
                let _ = self.write(&json!({"type": "control_response", "response": {"subtype": "error", "request_id": request, "error": format!("Neko does not handle {subtype}")}}));
            }
            Said::TurnEnded { .. } => {
                // Every message sent has had its answer: the CLI may end.
                if self.sent.fetch_sub(1, Ordering::SeqCst) <= 1 {
                    self.sent.store(0, Ordering::SeqCst);
                    self.close_stdin();
                }
            }
        }
    }
}

impl Control for Session {
    fn steer(&self, words: &str) -> Result<(), ControlError> {
        self.write(&json!({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": words}]}, "parent_tool_use_id": null, "session_id": ""}))?;
        self.sent.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn answer(&self, id: &str, answer: Answer) -> Result<(), ControlError> {
        let Some((input, tool_use)) = self.asks.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(id) else {
            return Err(ControlError("Claude Code is no longer waiting for that answer.".into()));
        };
        let mut response = match answer {
            Answer::Allow => json!({"behavior": "allow", "updatedInput": input}),
            Answer::Deny => json!({"behavior": "deny", "message": "The person said no to this step in Neko. Do not try it another way; say what you would have done instead."}),
        };
        if let Some(t) = tool_use {
            response["toolUseID"] = Value::String(t);
        }
        self.write(&json!({"type": "control_response", "response": {"subtype": "success", "request_id": id, "response": response}}))
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.stopper.stop();
    }
}

/// Where the fake CLI for tests is, when a test asks: beside the running test binary's own target folder.
pub fn fake_cli_beside(exe: &Path) -> Option<PathBuf> {
    let mut dir = exe.parent()?;
    for _ in 0..3 {
        let p = dir.join("neko-fake-claude");
        if find::executable(&p) {
            return Some(p);
        }
        dir = dir.parent()?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_follows_the_mode_and_resumes_when_asked() {
        let helper = Path::new("/Applications/Neko.app/Contents/MacOS/neko-daemon");
        let follow = arguments(None, PermissionMode::Follow, Some(helper));
        assert!(follow.windows(2).any(|w| w == ["--permission-prompt-tool", "stdio"]));
        assert!(!follow.iter().any(|x| x == "--permission-mode" || x == "--settings"), "Claude Code's own rules: Neko adds nothing");
        let ask = arguments(None, PermissionMode::Ask, Some(helper));
        assert!(ask.windows(2).any(|w| w == ["--permission-mode", "default"]));
        let at = ask.iter().position(|x| x == "--settings").expect("the hook is passed");
        let v: Value = serde_json::from_str(&ask[at + 1]).unwrap();
        let h = &v["hooks"]["PreToolUse"][0];
        assert_eq!(h["matcher"], "*");
        assert_eq!(h["hooks"][0], json!({"type": "command", "command": helper.to_string_lossy(), "args": ["--hook"], "timeout": 86_400, "onFailure": "block"}));
        let full = arguments(None, PermissionMode::Full, Some(helper));
        assert!(!full.iter().any(|x| x == "--settings"));
        for a in [&follow, &ask, &full] {
            assert!(!a.iter().any(|x| x.contains("dangerously") || x.contains("bypass") || x == "--setting-sources"), "never skips permissions, never drops the person's settings");
        }
        assert!(arguments(Some("abc"), PermissionMode::Ask, None).ends_with(&["--resume".to_owned(), "abc".to_owned()]));
    }

    #[test]
    fn auth_status_says_signed_in_and_which_login_and_nothing_else() {
        let (s, l) = read_auth(br#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","email":"asha@example.com","subscriptionType":"max"}"#);
        assert_eq!((s, l), (Some(true), Some(Login::Subscription(Some("max".into())))));
        assert_eq!(read_auth(br#"{"loggedIn":false,"authMethod":"none"}"#), (Some(false), None));
        assert_eq!(read_auth(br#"{"loggedIn":true,"authMethod":"api_key","apiProvider":"firstParty"}"#).1, Some(Login::ApiKey));
        assert_eq!(read_auth(br#"{"loggedIn":true,"apiProvider":"bedrock"}"#).1, Some(Login::Cloud("Amazon Bedrock".into())));
        assert_eq!(read_auth(b"not json"), (None, None));
    }
}

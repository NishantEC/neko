//! MCP as Neko needs it (docs/knowledge/plugins.md): what a plugin's server is launched with, the config a run hands Claude Code, and
//! a minimal client that starts a server and asks it who it is and which tools it has.
//!
//! The client speaks JSON-RPC 2.0, one message per line, over a program's stdin and stdout: `initialize`, the `initialized`
//! notification, then `tools/list` (every page). It starts the program under the run guard (`guard.rs`), so nothing it started
//! outlives the check, whatever happens: an answer, a deadline, a crash. Servers reached by a web address are not checked (they are
//! handed to Claude Code as they are): that would need a TLS client, and Neko takes no such dependency for this.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use neko_protocol::plugins::{MAX_TOOLS, Plugin, expand};
use serde_json::{Value, json};

use crate::guard;
use crate::plugins::{Stored, value_text};

/// The protocol version Neko says it speaks. A server answers with the one it speaks; Neko does not insist on this one.
pub const PROTOCOL: &str = "2025-06-18";

/// A program to run for a plugin, with every `${…}` filled in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// Where a plugin's server is reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Stdio(Launch),
    Remote { url: String, headers: BTreeMap<String, String>, sse: bool },
}

/// A plugin's target with its settings put in: what is set (else the default) and, for secrets, `secrets` (setting key to value, read
/// from the Keychain by the caller). `${plugin.dir}` is the plugin's folder.
pub fn target(p: &Plugin, stored: &BTreeMap<String, Stored>, secrets: &BTreeMap<String, String>) -> Target {
    let dir = p.folder.as_deref();
    let value = |key: &str| -> String { p.manifest.settings.iter().find(|f| f.key == key).map(|f| value_text(f, stored.get(key), secrets.get(key).map(String::as_str))).unwrap_or_default() };
    let fill = |s: &String| expand(s, dir, &value);
    let m = &p.manifest.mcp;
    match &m.url {
        Some(u) => Target::Remote { url: fill(u), headers: m.headers.iter().map(|(k, v)| (k.clone(), fill(v))).collect(), sse: m.transport.as_deref() == Some("sse") },
        None => Target::Stdio(Launch { program: m.command.as_ref().map(&fill).unwrap_or_default(), args: m.args.iter().map(&fill).collect(), env: m.env.iter().map(|(k, v)| (k.clone(), fill(v))).collect() }),
    }
}

/// The file Claude Code reads with `--mcp-config`: each server by the plugin's name, so its tools are `mcp__<name>__<tool>`.
pub fn claude_config(servers: &[(String, Target)]) -> Value {
    let mut map = serde_json::Map::new();
    for (name, t) in servers {
        let entry = match t {
            Target::Stdio(l) => {
                let mut e = json!({ "type": "stdio", "command": l.program, "args": l.args });
                if !l.env.is_empty() {
                    e["env"] = json!(l.env);
                }
                e
            }
            Target::Remote { url, headers, sse } => {
                let mut e = json!({ "type": if *sse { "sse" } else { "http" }, "url": url });
                if !headers.is_empty() {
                    e["headers"] = json!(headers);
                }
                e
            }
        };
        map.insert(name.clone(), entry);
    }
    json!({ "mcpServers": Value::Object(map) })
}

/// What a server said about itself.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Found {
    pub server: Option<String>,
    pub version: Option<String>,
    pub tools: Vec<String>,
}

/// The PATH a plugin's program is started with: this process's, the folders installers use and the login shell's (read once; it may
/// take a few seconds the first time).
pub fn path() -> String {
    let mut parts: Vec<PathBuf> = crate::find::common_dirs();
    if let Some(p) = crate::find::login_shell_path() {
        parts.extend(std::env::split_paths(p));
    }
    let mut seen = std::collections::HashSet::new();
    parts.retain(|p| p.is_absolute() && seen.insert(p.clone()));
    std::env::join_paths(parts).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The longest line read from a server. A longer one is not MCP.
const MAX_LINE: usize = 4 * 1024 * 1024;

enum Heard {
    Line(String),
    /// A line longer than [`MAX_LINE`].
    Huge,
    End,
}

/// Starts `launch` under the guard in `cwd` (with `path` as its PATH and nothing else of this environment) and asks it who it is and
/// which tools it has, within `limit` in all. Whatever it started is killed when this returns.
pub fn check(guard_exe: &Path, launch: &Launch, cwd: &Path, path: &str, limit: Duration) -> Result<Found, String> {
    let until = Instant::now() + limit;
    let shown = launch.program.rsplit('/').next().unwrap_or(&launch.program).to_owned();
    let mut env: Vec<(String, String)> = vec![("PATH".into(), path.to_owned())];
    for k in ["HOME", "USER", "LANG", "TMPDIR"] {
        if let Ok(v) = std::env::var(k) {
            env.push((k.to_owned(), v));
        }
    }
    env.extend(launch.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    // A command given by name is found on the PATH the server is given, not this process's.
    let program = resolve(&launch.program, path);
    let guarded = guard::spawn(guard_exe, &program, &launch.args, cwd, &env, |c| {
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    })
    .map_err(|e| format!("Neko could not start {shown}: {e}."))?;
    let guard::Guarded { mut child, stopper } = guarded;
    let (Some(mut stdin), Some(mut stdout), Some(mut stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
        stopper.stop();
        return Err(format!("Neko could not start {shown}."));
    };
    let (tx, rx) = mpsc::channel::<Heard>();
    std::thread::Builder::new().name("neko-mcp-out".into()).spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    while let Some(at) = buf.iter().position(|b| *b == b'\n') {
                        let line: Vec<u8> = buf.drain(..=at).collect();
                        if tx.send(Heard::Line(String::from_utf8_lossy(&line).trim_end().to_owned())).is_err() {
                            return;
                        }
                    }
                    if buf.len() > MAX_LINE {
                        let _ = tx.send(Heard::Huge);
                        return;
                    }
                }
            }
        }
        let _ = tx.send(Heard::End);
    }).map_err(|e| e.to_string())?;
    let tail = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let tail_w = tail.clone();
    let _ = std::thread::Builder::new().name("neko-mcp-err".into()).spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = stderr.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let mut t = tail_w.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            t.extend_from_slice(&chunk[..n]);
            let len = t.len();
            if len > 4096 {
                t.drain(..len - 4096);
            }
        }
    });
    let outcome = converse(&mut stdin, &rx, until, limit, &shown, &tail, &mut child);
    drop(stdin);
    // Whatever happened, nothing it started outlives this: the guardian kills the tree when this end closes. A thread reaps it, so
    // the answer does not wait for the guardian's last look.
    stopper.stop();
    let _ = std::thread::Builder::new().name("neko-mcp-reap".into()).spawn(move || {
        let _ = child.wait();
    });
    outcome
}

/// The program as an absolute path when `name` is found in `path`, else as given.
fn resolve(name: &str, path: &str) -> PathBuf {
    if name.contains('/') {
        return PathBuf::from(name);
    }
    std::env::split_paths(path).map(|d| d.join(name)).find(|p| crate::find::executable(p)).unwrap_or_else(|| PathBuf::from(name))
}

fn send(stdin: &mut impl Write, v: &Value) -> Result<(), ()> {
    let mut line = v.to_string();
    line.push('\n');
    stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).map_err(|_| ())
}

/// Words for what the server printed to stderr last: its final non-empty line, shortened.
fn said(tail: &std::sync::Mutex<Vec<u8>>) -> Option<String> {
    let t = String::from_utf8_lossy(&tail.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).into_owned();
    // The guardian says "Neko's run guard: …" when it cannot start the program.
    t.lines().rev().map(str::trim).find(|l| !l.is_empty()).map(|l| crate::runtime::shorten(l, 200))
}

/// One conversation with a server: what it said and how long is left.
struct Talk<'a> {
    rx: &'a mpsc::Receiver<Heard>,
    until: Instant,
    limit: Duration,
    shown: &'a str,
    tail: &'a std::sync::Mutex<Vec<u8>>,
    child: &'a mut std::process::Child,
    /// The last line it printed that was not MCP.
    noise: Option<String>,
}

impl Talk<'_> {
    fn ended(&mut self) -> String {
        ended(self.shown, self.tail, self.child, self.noise.as_deref())
    }

    /// Sends one message; a server that has closed its input has ended.
    fn say(&mut self, stdin: &mut impl Write, v: &Value) -> Result<(), String> {
        send(stdin, v).map_err(|()| self.ended())
    }

    /// Waits for the response with `id`, answering what the server asks meanwhile.
    fn wait_for(&mut self, stdin: &mut impl Write, id: i64) -> Result<Value, String> {
        loop {
            let heard = match self.rx.recv_timeout(self.until.saturating_duration_since(Instant::now())) {
                Ok(h) => h,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let secs = self.limit.as_secs().max(1);
                    let mut why = format!("{} did not answer within {secs} second{}.", self.shown, if secs == 1 { "" } else { "s" });
                    if let Some(n) = &self.noise {
                        why.push_str(&format!(" It printed something that is not MCP: “{n}”."));
                    }
                    return Err(why);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => Heard::End,
            };
            match heard {
                Heard::Huge => return Err(format!("{} sent a line far too long to be MCP.", self.shown)),
                Heard::End => return Err(self.ended()),
                Heard::Line(l) if l.trim().is_empty() => {}
                Heard::Line(l) => match serde_json::from_str::<Value>(&l) {
                    Ok(v) if v.get("jsonrpc").and_then(Value::as_str) == Some("2.0") => {
                        if v.get("method").is_some() && v.get("id").is_some() {
                            // A request from the server: answer ping, refuse anything else (Neko offers no roots or sampling).
                            let reply = if v["method"] == "ping" { json!({"jsonrpc":"2.0","id":v["id"],"result":{}}) } else { json!({"jsonrpc":"2.0","id":v["id"],"error":{"code":-32601,"message":"Method not found"}}) };
                            let _ = send(stdin, &reply);
                        } else if v.get("id").and_then(Value::as_i64) == Some(id) {
                            return Ok(v);
                        }
                    }
                    _ => self.noise = Some(crate::runtime::shorten(l.trim(), 80)),
                },
            }
        }
    }
}

fn converse(stdin: &mut impl Write, rx: &mpsc::Receiver<Heard>, until: Instant, limit: Duration, shown: &str, tail: &std::sync::Mutex<Vec<u8>>, child: &mut std::process::Child) -> Result<Found, String> {
    let mut t = Talk { rx, until, limit, shown, tail, child, noise: None };
    t.say(stdin, &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"neko","version":env!("CARGO_PKG_VERSION")}}}))?;
    let init = t.wait_for(stdin, 1)?;
    if let Some(e) = init.get("error") {
        return Err(format!("{shown} refused to start MCP: {}", e["message"].as_str().unwrap_or("no reason given")));
    }
    let result = init.get("result").filter(|r| r.is_object()).ok_or_else(|| format!("{shown} answered, but not the way an MCP server does."))?;
    let mut found = Found { server: result["serverInfo"]["name"].as_str().map(str::to_owned), version: result["serverInfo"]["version"].as_str().map(str::to_owned), tools: Vec::new() };
    if result["capabilities"].get("tools").is_none() {
        return Ok(found);
    }
    t.say(stdin, &json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    let mut cursor: Option<String> = None;
    for page in 0..20 {
        let id = 2 + page;
        let params = cursor.as_ref().map_or_else(|| json!({}), |c| json!({ "cursor": c }));
        t.say(stdin, &json!({"jsonrpc":"2.0","id":id,"method":"tools/list","params":params}))?;
        let reply = t.wait_for(stdin, id)?;
        if let Some(e) = reply.get("error") {
            // A server that says "method not found" has no tools to list.
            if e["code"].as_i64() == Some(-32601) {
                return Ok(found);
            }
            return Err(format!("{shown} could not list its tools: {}", e["message"].as_str().unwrap_or("no reason given")));
        }
        let tools = reply["result"]["tools"].as_array().ok_or_else(|| format!("{shown} answered tools/list, but not with a list of tools."))?;
        found.tools.extend(tools.iter().filter_map(|x| x["name"].as_str().map(str::to_owned)));
        found.tools.truncate(MAX_TOOLS);
        match reply["result"]["nextCursor"].as_str() {
            Some(c) if !c.is_empty() && found.tools.len() < MAX_TOOLS => cursor = Some(c.to_owned()),
            _ => break,
        }
    }
    Ok(found)
}

/// Words for a server that ended before it answered: how, and what it said last.
fn ended(shown: &str, tail: &std::sync::Mutex<Vec<u8>>, child: &mut std::process::Child, noise: Option<&str>) -> String {
    // The exit status may come a moment after the pipe closes.
    let until = Instant::now() + Duration::from_millis(500);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(10)),
            _ => break None,
        }
    };
    std::thread::sleep(Duration::from_millis(30));
    let said = said(tail);
    if let Some(w) = said.as_deref().filter(|w| w.contains("run guard: cannot start")) {
        return format!("Neko could not start {shown}. Is it installed? ({})", w.rsplit(": ").next().unwrap_or(w));
    }
    let mut why = match status.and_then(|s| s.code()) {
        Some(code) => format!("{shown} stopped before it answered (exit code {code})."),
        None => format!("{shown} stopped before it answered."),
    };
    if let Some(w) = said {
        why.push_str(&format!(" It said: “{w}”."));
    } else if let Some(n) = noise {
        why.push_str(&format!(" It printed something that is not MCP: “{n}”."));
    }
    why
}

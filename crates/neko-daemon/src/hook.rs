//! "Ask before risky steps", enforced (docs/knowledge/permissions.md): Claude Code runs `neko-daemon --hook` before every tool
//! call of a run Neko started in that mode (a `PreToolUse` hook passed with `--settings` for that run only). The helper reads the
//! hook's JSON on stdin, asks the daemon over loopback with the run's own secret, waits for the answer and prints the hook's
//! decision. The daemon applies Neko's policy: a safe step is allowed at once; a risky one becomes the same stored request the
//! Task view shows, and the helper waits for the person.
//!
//! The contract, from Claude Code's hooks documentation (checked against 2.1.331, see runtimes.md): the input has `tool_name`,
//! `tool_input` and `tool_use_id`; the output, on exit 0, is
//! `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"|"deny","permissionDecisionReason":"…"}}`;
//! printing nothing leaves the call to Claude Code's own permission flow.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use neko_core::conductor::{EXPIRED, HOOK, RUN_ENDED};
use neko_core::runtime::{Action, AgentEvent, Answer, HOOK_PORT_ENV, HOOK_SECRET_ENV};
use serde_json::{Value, json};

use crate::http::{self, HOOK_PATH};
use crate::runs;
use crate::server::{Shared, lock};

/// The hook's decision, as the helper prints it.
pub fn decision(answer: Answer, why: &str) -> String {
    let d = if answer == Answer::Allow { "allow" } else { "deny" };
    json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": d, "permissionDecisionReason": why}}).to_string()
}

/// How long the daemon keeps a hook's request open: a day less a minute (Claude Code waits a day), or `NEKO_HOOK_EXPIRE_MS`.
fn expiry() -> Duration {
    Duration::from_millis(std::env::var("NEKO_HOOK_EXPIRE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(86_340_000))
}

/// Whether the helper has hung up (Claude Code ended it).
fn gone(stream: &TcpStream) -> bool {
    if stream.set_nonblocking(true).is_err() {
        return true;
    }
    let mut b = [0u8; 1];
    let closed = match stream.peek(&mut b) {
        Ok(n) => n == 0,
        Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
    };
    let _ = stream.set_nonblocking(false);
    closed
}

/// One hook question on a connection whose head asked `POST /v1/hook`.
pub fn serve(shared: &Arc<Shared>, mut stream: TcpStream, secret: &str, length: usize) {
    let mut body = vec![0u8; length];
    if stream.read_exact(&mut body).is_err() {
        return;
    }
    let Some(run) = runs::by_secret(shared, secret) else {
        let _ = http::refuse(&mut stream, &http::Refused::Unauthorized);
        return;
    };
    let _ = stream.set_read_timeout(None);
    let reply = ask(shared, &stream, &run, &body);
    if let Some(text) = reply {
        let _ = http::respond(&mut stream, "200 OK", "application/json", &text);
    }
}

/// Decides one call, waiting for the person when the policy asks. None when the helper hung up first.
fn ask(shared: &Arc<Shared>, stream: &TcpStream, run: &runs::HookRun, body: &[u8]) -> Option<String> {
    let Ok(v) = serde_json::from_slice::<Value>(body) else { return Some(decision(Answer::Deny, "Neko could not read this step, so it did not run.")) };
    let tool = v["tool_name"].as_str().unwrap_or("tool").to_owned();
    let input = v["tool_input"].clone();
    let tool_use = v["tool_use_id"].as_str().map(str::to_owned);
    let workdir = lock(&run.book).workdir().to_owned();
    let action = neko_core::claude::stream::action(&tool, &input, &workdir);
    // Claude Code's own bookkeeping is left to Claude Code: no decision.
    if matches!(action, Action::Internal { .. }) {
        return Some(String::new());
    }
    let request = format!("{HOOK}{}", tool_use.clone().unwrap_or_else(|| format!("call-{}", runs::next_hook())));
    let (tx, rx) = mpsc::channel();
    lock(&shared.runs.hooks).insert(request.clone(), tx);
    {
        let mut b = lock(&run.book);
        let mut core = lock(&shared.core);
        let now = core.now();
        match b.on(&mut core, AgentEvent::PermissionAsked { id: request.clone(), tool, action, tool_use }, now) {
            Ok(outs) => runs::push(shared, &run.task, outs, run.control.as_ref()),
            Err(_) => {
                lock(&shared.runs.hooks).remove(&request);
                return Some(decision(Answer::Deny, "Neko could not write to its store, so the step did not run."));
            }
        }
    }
    let until = Instant::now() + expiry();
    let withdraw = |shared: &Arc<Shared>| {
        lock(&shared.runs.hooks).remove(&request);
        let mut b = lock(&run.book);
        let mut core = lock(&shared.core);
        let now = core.now();
        if let Ok(outs) = b.on(&mut core, AgentEvent::PermissionWithdrawn { id: request.clone() }, now) {
            runs::push(shared, &run.task, outs, run.control.as_ref());
        }
    };
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok((answer, why)) => {
                lock(&shared.runs.hooks).remove(&request);
                return Some(decision(answer, &why));
            }
            Err(RecvTimeoutError::Disconnected) => return Some(decision(Answer::Deny, RUN_ENDED)),
            Err(RecvTimeoutError::Timeout) => {
                if Instant::now() >= until {
                    withdraw(shared);
                    return Some(decision(Answer::Deny, EXPIRED));
                }
                if gone(stream) {
                    withdraw(shared);
                    return None;
                }
            }
        }
    }
}

/// `neko-daemon --hook`: Claude Code's `PreToolUse` hook for a run Neko started. Prints the decision and exits 0; when the daemon
/// cannot be reached or does not know the run, the step is denied (and the hook's `onFailure: "block"` covers a crash).
pub fn helper() -> ExitCode {
    // Claude Code runs the call when its hook gives no verdict, so every path prints one: a panic too.
    match std::panic::catch_unwind(helper_inner) {
        Ok(code) => code,
        Err(_) => {
            println!("{}", decision(Answer::Deny, "Neko's check failed, so the step did not run."));
            ExitCode::SUCCESS
        }
    }
}

fn helper_inner() -> ExitCode {
    let deny = |why: &str| {
        println!("{}", decision(Answer::Deny, why));
        ExitCode::SUCCESS
    };
    let mut input = Vec::new();
    if std::io::stdin().read_to_end(&mut input).is_err() {
        return deny("Neko could not read this step, so it did not run.");
    }
    let (Some(port), Some(secret)) = (std::env::var(HOOK_PORT_ENV).ok().and_then(|p| p.parse::<u16>().ok()), std::env::var(HOOK_SECRET_ENV).ok().filter(|s| !s.is_empty())) else {
        return deny("Neko's hook ran outside a Neko run, so the step did not run.");
    };
    let Ok(mut s) = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(5)) else {
        return deny("Neko could not be reached, so the step did not run.");
    };
    let head = format!("POST {HOOK_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {secret}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", input.len());
    if s.write_all(head.as_bytes()).and_then(|()| s.write_all(&input)).and_then(|()| s.flush()).is_err() {
        return deny("Neko could not be reached, so the step did not run.");
    }
    // The daemon answers within a day; past that (and a little), the helper says no itself.
    let _ = s.set_read_timeout(Some(Duration::from_secs(neko_core::claude::HOOK_TIMEOUT_S - 10)));
    let mut reply = Vec::new();
    if s.read_to_end(&mut reply).is_err() && reply.is_empty() {
        return deny("Neko did not answer in time, so the step did not run.");
    }
    let text = String::from_utf8_lossy(&reply);
    let status = text.split_whitespace().nth(1).unwrap_or("");
    let body = text.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    match status {
        "200" => {
            if !body.trim().is_empty() {
                println!("{}", body.trim());
            }
            ExitCode::SUCCESS
        }
        "401" => deny("Neko does not know this run, so the step did not run."),
        _ => deny("Neko could not answer, so the step did not run."),
    }
}

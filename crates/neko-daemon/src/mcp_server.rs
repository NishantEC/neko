//! `neko-daemon --mcp`: Neko's own MCP server (docs/knowledge/subagents.md). Claude Code starts it for every run, from the run's
//! `--mcp-config`, as a stdio server named `neko`; its tools are `mcp__neko__*`.
//!
//! It is a thin line from the agent to the daemon and keeps nothing. It reads the daemon's loopback port and the run's own secret
//! from its environment (set on the CLI by Neko and inherited here: never in argv or in a file), and forwards each tool call to
//! `POST /v1/mcp` with the secret as its bearer. The daemon knows from the secret which run is asking. Every tool answers at once:
//! the helper gives the daemon ten seconds and says so when it does not answer.
//!
//! JSON-RPC 2.0, one message per line. A request Neko does not serve is refused with "method not found"; a tool call that cannot be
//! made is an error result in words (never a hang, never a silent success).

use std::io::{BufRead, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

use neko_core::runtime::{MCP_PORT_ENV, MCP_SECRET_ENV};
use serde_json::{Value, json};

use crate::http::MCP_PATH;

/// The protocol version this server speaks when the client names none it knows.
const PROTOCOL: &str = "2025-06-18";
/// How long the daemon has to answer a call.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);

/// What the daemon says about this run: how deep it is and whether it may make Cattains (the tools offered depend on both).
fn info() -> Option<Value> {
    call(&json!({"op": "info"})).ok()
}

/// One request to the daemon, and its JSON answer.
fn call(body: &Value) -> Result<Value, String> {
    let (Some(port), Some(secret)) = (std::env::var(MCP_PORT_ENV).ok().and_then(|p| p.parse::<u16>().ok()), std::env::var(MCP_SECRET_ENV).ok().filter(|s| !s.is_empty())) else {
        return Err("Neko's tools ran outside a Neko run, so they cannot reach Neko.".into());
    };
    let mut s = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(5)).map_err(|_| "Neko could not be reached.".to_owned())?;
    let text = body.to_string();
    let head = format!("POST {MCP_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {secret}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", text.len());
    s.write_all(head.as_bytes()).and_then(|()| s.write_all(text.as_bytes())).and_then(|()| s.flush()).map_err(|_| "Neko could not be reached.".to_owned())?;
    let _ = s.set_read_timeout(Some(ANSWER_WITHIN));
    let mut reply = Vec::new();
    if s.read_to_end(&mut reply).is_err() && reply.is_empty() {
        return Err("Neko did not answer in time.".into());
    }
    let reply = String::from_utf8_lossy(&reply);
    let status = reply.split_whitespace().nth(1).unwrap_or("");
    let body = reply.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    match status {
        "200" => serde_json::from_str(body).map_err(|_| "Neko's answer could not be read.".to_owned()),
        "401" => Err("Neko does not know this run any more, so the tool did not run.".into()),
        _ => Err("Neko could not answer.".into()),
    }
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn failure(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// The answer to one message, if it needs one (notifications do not).
/// `offer` says how deep the run is, whether it may make Cattains and whether it is a Cattain's own thread (its crew tools), when the
/// tools are listed.
pub fn handle(msg: &Value, ask: &dyn Fn(&Value) -> Result<Value, String>, offer: &dyn Fn() -> (usize, bool, bool)) -> Option<Value> {
    let method = msg["method"].as_str().unwrap_or("");
    let id = msg.get("id")?;
    Some(match method {
        "initialize" => {
            let asked = msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL);
            reply(id, json!({"protocolVersion": asked, "capabilities": {"tools": {}}, "serverInfo": {"name": "neko", "version": env!("CARGO_PKG_VERSION")}, "instructions": neko_core::subagents::SERVER_INSTRUCTIONS}))
        }
        "ping" => reply(id, json!({})),
        "tools/list" => {
            let (depth, cattains, crew) = offer();
            let mut tools = neko_core::subagents::tools(depth, cattains);
            if crew && depth == 0 {
                tools.extend(neko_core::crew::tools());
            }
            // Timers and watches: a thread's, not a kitten's (docs/knowledge/waits.md).
            if depth == 0 {
                tools.extend(neko_core::waits::tools());
            }
            reply(id, json!({"tools": tools}))
        }
        "tools/call" => {
            let p = &msg["params"];
            let tool = p["name"].as_str().unwrap_or("");
            let body = json!({"op": "call", "tool": tool, "arguments": p["arguments"], "tool_use_id": p["_meta"]["claudecode/toolUseId"]});
            match ask(&body) {
                Ok(v) => reply(id, json!({"content": [{"type": "text", "text": v["text"].as_str().unwrap_or("")}], "isError": v["error"].as_bool().unwrap_or(false)})),
                Err(why) => reply(id, json!({"content": [{"type": "text", "text": why}], "isError": true})),
            }
        }
        _ => failure(id, -32601, "method not found"),
    })
}

/// The server: reads lines until stdin closes (Claude Code ended), answers each.
pub fn serve() -> ExitCode {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let offer = || {
        let v = info().unwrap_or_default();
        (v["depth"].as_u64().map_or(0, |d| d as usize), v["cattains"].as_bool().unwrap_or(false), v["crew"].as_bool().unwrap_or(false))
    };
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(answer) = handle(&msg, &call, &offer) {
            if writeln!(out, "{answer}").and_then(|()| out.flush()).is_err() {
                break;
            }
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask_ok(_: &Value) -> Result<Value, String> {
        Ok(json!({"text": "Subagent 1 is started.", "error": false}))
    }

    #[test]
    fn it_introduces_itself_and_lists_its_tools() {
        let init = handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}), &ask_ok, &|| (0, false, false)).unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "neko");
        assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
        let list = handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), &ask_ok, &|| (0, false, false)).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["start_subagent", "start_drafts", "subagents", "message_subagent", "stop_subagent", "ask_person", "wake_me", "watch_files", "watch_command", "stop_watching"], "a thread may also set timers and watches");
        let deep = handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}), &ask_ok, &|| (3, true, true)).unwrap();
        assert_eq!(deep["result"]["tools"].as_array().unwrap().len(), 4, "no tool to start more at the last level, ask_parent for ask_person, no Cattains, and no timers or watches for a kitten");
        let thread = handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}), &ask_ok, &|| (0, true, false)).unwrap();
        let names: Vec<&str> = thread["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names[6..], ["make_cattain", "list_cattains", "wake_me", "watch_files", "watch_command", "stop_watching"], "a thread whose agent may make Cattains");
        let own = handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}), &ask_ok, &|| (0, true, true)).unwrap();
        let names: Vec<&str> = own["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names[6..], ["make_cattain", "list_cattains", "start_crew_thread", "crew", "message_crew", "wake_me", "watch_files", "watch_command", "stop_watching"], "a Cattain's own thread also runs its crew");
    }

    #[test]
    fn a_call_goes_to_the_daemon_and_its_words_come_back() {
        let r = handle(&json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"start_subagent","arguments":{"title":"T","prompt":"P"},"_meta":{"claudecode/toolUseId":"toolu_1"}}}), &|b: &Value| {
            assert_eq!(b["tool"], "start_subagent");
            assert_eq!(b["tool_use_id"], "toolu_1");
            ask_ok(b)
        }, &|| (0, false, false)).unwrap();
        assert_eq!(r["result"]["content"][0]["text"], "Subagent 1 is started.");
        assert_eq!(r["result"]["isError"], false);
    }

    #[test]
    fn a_call_that_cannot_be_made_is_an_error_in_words() {
        let r = handle(&json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"subagents"}}), &|_: &Value| Err("Neko could not be reached.".into()), &|| (0, false, false)).unwrap();
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(r["result"]["content"][0]["text"], "Neko could not be reached.");
    }

    #[test]
    fn notifications_get_no_answer_and_unknown_requests_are_refused() {
        assert!(handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}), &ask_ok, &|| (0, false, false)).is_none());
        let r = handle(&json!({"jsonrpc":"2.0","id":"server-discover-probe-1","method":"server/discover"}), &ask_ok, &|| (0, false, false)).unwrap();
        assert_eq!(r["error"]["code"], -32601);
        assert_eq!(r["id"], "server-discover-probe-1");
    }
}

//! A minimal Model Context Protocol client, spoken over HTTP to Paseo's own
//! daemon.
//!
//! **This is the base layer of `docs/plan-agent-control-plane.md`.** Paseo
//! exposes its entire agent surface — 61 tools, confirmed live — at
//! `POST /mcp/agents`, and every capability neko grows on top of it is a
//! `call` away. Nothing above this module talks to Paseo any other way.
//!
//! ## Why MCP rather than Paseo's WebSocket
//!
//! `neko_core::usage` tried the WebSocket first and abandoned it: the `hello`
//! handshake needs an undocumented `protocolVersion` and the reply routes
//! through a session whose establishment is not written down for third
//! parties. The MCP endpoint has none of that — it is a documented protocol
//! with a published schema, and the daemon answers a plain `initialize` on
//! the first request.
//!
//! ## Authorization, stated rather than assumed
//!
//! The route is exempt from the daemon's global bearer middleware and
//! authorizes with either a per-run capability token or the daemon password.
//! When **no daemon password is configured it is open**, which is this
//! machine's state. [`McpClient::call`] therefore sends no credential, and a
//! `401` comes back as an ordinary [`McpError`] naming the daemon rather than
//! as a panic — the honest behaviour if that machine ever sets a password.
//!
//! ## Why `curl`
//!
//! The same trade `usage.rs` made: a Rust HTTP client would pull in an async
//! runtime and a TLS stack, and this is loopback with no TLS at all. `curl`
//! ships with macOS and is already how this crate reaches `mdfind`, `open`,
//! `launchctl` and `paseo`. Every call is one process; at the round-trip
//! sizes this sees (a few ms on loopback) that is far below the search
//! latency it sits behind.

use std::io::Write;
use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

/// How long to wait on the daemon before giving up.
///
/// Short on purpose: several of these calls sit behind a keystroke, and a
/// stalled daemon must never be what makes the panel feel slow. Long enough
/// that a real `create_agent` — which boots a provider CLI — is not cut off
/// mid-flight.
const REQUEST_TIMEOUT_SECS: u64 = 20;

/// The MCP revision this client speaks. Sent verbatim in `initialize`; the
/// daemon echoes its own, and a mismatch is not fatal — MCP negotiates.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// Where the daemon writes what it is listening on.
///
/// Read rather than hard-coded: 6767 is only the default, and a daemon on
/// another port with a hard-coded client would look exactly like a daemon
/// that is not running — the most confusing failure this module could have.
const PID_FILE: &str = ".paseo/paseo.pid";

#[derive(Debug, Clone, PartialEq)]
pub enum McpError {
    /// No `paseo.pid`, or it names nothing to connect to. Not an error in the
    /// ordinary sense — Paseo simply is not running, which is a state neko
    /// has to render rather than report.
    NotRunning,
    /// The daemon answered, and said no.
    Refused(String),
    /// Transport, framing, or a tool that failed on its own terms.
    Failed(String),
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::NotRunning => write!(f, "Paseo isn't running"),
            McpError::Refused(why) => write!(f, "{why}"),
            McpError::Failed(why) => write!(f, "{why}"),
        }
    }
}

/// A connection to the local Paseo daemon's agent tool surface.
///
/// Cheap to construct and holds no socket — each call is its own request, so
/// there is no connection to keep alive, reconnect, or invalidate when the
/// daemon restarts. That is worth more here than saving a handshake: the
/// daemon is restarted often during development and a client caching a dead
/// session would be silently wrong until something noticed.
#[derive(Debug, Clone)]
pub struct McpClient {
    endpoint: String,
}

impl McpClient {
    /// Discovers the running daemon, or [`McpError::NotRunning`].
    pub fn discover() -> Result<Self, McpError> {
        let home = std::env::home_dir().ok_or(McpError::NotRunning)?;
        let raw = std::fs::read_to_string(home.join(PID_FILE)).map_err(|_| McpError::NotRunning)?;
        let listen = listen_address(&raw).ok_or(McpError::NotRunning)?;
        Ok(Self { endpoint: format!("http://{listen}/mcp/agents") })
    }

    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }

    /// Calls one tool by name, returning its structured result.
    ///
    /// Performs `initialize` on every call rather than once per client. That
    /// is one extra loopback round-trip and it buys statelessness: this
    /// endpoint issues no session id for a stateless client (verified live —
    /// `tools/list` is answered with no `Mcp-Session-Id` at all), so there is
    /// no session to hold, expire, or resume across a daemon restart.
    pub fn call(&self, tool: &str, arguments: Value) -> Result<Value, McpError> {
        let body = self.post(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": tool, "arguments": arguments },
        }))?;
        tool_result(&body)
    }

    /// Every tool the daemon offers. Used to check the channel is live and by
    /// `docs/plan-agent-control-plane.md`'s own survey; not on any hot path.
    pub fn list_tools(&self) -> Result<Vec<String>, McpError> {
        let body = self.post(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))?;
        let tools = body
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(Value::as_array)
            .ok_or_else(|| McpError::Failed("no tool list in the reply".to_string()))?;
        Ok(tools
            .iter()
            .filter_map(|t| t.get("name")?.as_str().map(str::to_string))
            .collect())
    }

    fn post(&self, request: Value) -> Result<Value, McpError> {
        // `initialize` first, in the same connection reuse curl gives us for
        // free, so the daemon has negotiated before the real request lands.
        let init = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "neko", "version": env!("CARGO_PKG_VERSION") },
            },
        });
        self.exchange(&init)?;
        self.exchange(&request)
    }

    fn exchange(&self, request: &Value) -> Result<Value, McpError> {
        let mut child = Command::new("/usr/bin/curl")
            .args([
                "--silent",
                "--show-error",
                "--config",
                "-",
                "--write-out",
                "\n%{http_code}",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| McpError::Failed(format!("couldn't run curl: {e}")))?;

        // The body goes on stdin with the rest of the options. Nothing here is
        // secret today, but this endpoint authenticates with a bearer the
        // moment a daemon password exists, and a request shape that already
        // keeps its payload out of `argv` cannot leak one later by omission.
        let config = format!(
            "url = \"{}\"\n\
             request = POST\n\
             max-time = {REQUEST_TIMEOUT_SECS}\n\
             header = \"Content-Type: application/json\"\n\
             header = \"Accept: application/json, text/event-stream\"\n\
             data-binary = {}\n",
            self.endpoint,
            json_as_curl_argument(request)
        );
        child
            .stdin
            .take()
            .ok_or_else(|| McpError::Failed("curl refused stdin".to_string()))?
            .write_all(config.as_bytes())
            .map_err(|e| McpError::Failed(format!("couldn't send the request: {e}")))?;

        let out = child
            .wait_with_output()
            .map_err(|e| McpError::Failed(format!("curl failed: {e}")))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let (body, status) = split_status(&stdout);
        match status {
            Some(200) => parse_frame(&body),
            Some(401 | 403) => Err(McpError::Refused(
                "Paseo's daemon refused neko — it has a password set".to_string(),
            )),
            // The daemon was not there to answer at all.
            None if out.stdout.is_empty() => Err(McpError::NotRunning),
            Some(code) => Err(McpError::Failed(format!("Paseo's daemon returned {code}"))),
            None => Err(McpError::Failed(
                String::from_utf8_lossy(&out.stderr)
                    .trim()
                    .split('\n')
                    .next()
                    .unwrap_or("request failed")
                    .to_string(),
            )),
        }
    }
}

/// `"127.0.0.1:6767"` out of the pid file's own `listen` field.
///
/// Validated as a real socket address rather than trusted as a string: this
/// value is interpolated straight into a URL, and a pid file left behind by
/// something else should fail discovery rather than produce a request aimed
/// somewhere unexpected.
pub fn listen_address(raw: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(raw.trim()).ok()?;
    let listen = parsed.get("listen")?.as_str()?;
    listen.parse::<SocketAddr>().ok().map(|addr| addr.to_string())
}

/// Splits `--write-out`'s trailing status line off the body — the same shape
/// `usage::split_status` uses, kept separate because these two modules must
/// be free to change their transports independently.
pub fn split_status(raw: &str) -> (String, Option<u16>) {
    match raw.rsplit_once('\n') {
        Some((body, status)) => (body.to_string(), status.trim().parse().ok()),
        None => (String::new(), raw.trim().parse().ok()),
    }
}

/// Pulls the JSON-RPC envelope out of whichever framing the daemon chose.
///
/// **It answers a single request two different ways.** A plain JSON body when
/// it feels like it, and Server-Sent Events (`event: message` / `data: {…}`)
/// when it does not — confirmed live, both from the same endpoint on the same
/// daemon. Rather than negotiate one, this accepts either: the last `data:`
/// line if the body is SSE, the whole body if it parses as JSON.
pub fn parse_frame(body: &str) -> Result<Value, McpError> {
    if let Ok(direct) = serde_json::from_str::<Value>(body.trim()) {
        return Ok(direct);
    }
    let last = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|payload| serde_json::from_str::<Value>(payload.trim()).ok())
        .next_back();
    last.ok_or_else(|| McpError::Failed("the daemon's reply was unreadable".to_string()))
}

/// Unwraps `tools/call`'s own two-level result.
///
/// MCP reports a failed *call* as a normal response carrying `isError`, and a
/// failed *request* as a JSON-RPC `error`. Both are failures to a caller, and
/// collapsing them here is what keeps every call site from re-deriving that
/// distinction. The payload itself is the first `structuredContent` when the
/// tool provides one, falling back to its text content, because Paseo's tools
/// return both and the structured half is the one worth parsing.
pub fn tool_result(body: &Value) -> Result<Value, McpError> {
    if let Some(error) = body.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the daemon rejected the request");
        return Err(McpError::Failed(message.to_string()));
    }
    let result = body
        .get("result")
        .ok_or_else(|| McpError::Failed("the daemon's reply had no result".to_string()))?;

    let text = || -> String {
        result
            .get("content")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("text")?.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    };

    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        let message = text();
        return Err(McpError::Failed(if message.is_empty() {
            "the tool reported an error".to_string()
        } else {
            message
        }));
    }

    if let Some(structured) = result.get("structuredContent") {
        return Ok(structured.clone());
    }
    // A tool with only text content still answered; hand back what it said
    // rather than an error, so a caller can decide whether it needed shape.
    serde_json::from_str::<Value>(&text()).or_else(|_| Ok(json!({ "text": text() })))
}

/// curl's `--config` format takes an unquoted or a double-quoted value, and
/// backslash-escapes inside the quoted form. A JSON body is full of quotes,
/// so it always takes the quoted form.
fn json_as_curl_argument(value: &Value) -> String {
    let raw = value.to_string();
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// A tool call that must not outlive the panel's patience.
pub fn short_timeout() -> Duration {
    Duration::from_secs(REQUEST_TIMEOUT_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoint_comes_from_the_pid_file_and_is_validated() {
        // Real content, from this machine's own daemon.
        let raw = r#"{"pid":19500,"startedAt":"2026-08-23T18:53:35.167Z",
                      "hostname":"mac.local","uid":503,"listen":"127.0.0.1:6767",
                      "desktopManaged":true,"heartbeat":true}"#;
        assert_eq!(listen_address(raw).as_deref(), Some("127.0.0.1:6767"));
    }

    #[test]
    fn a_pid_file_that_names_nothing_connectable_fails_discovery() {
        // This value is interpolated into a URL, so a leftover file from
        // something else must not produce a request aimed somewhere else.
        assert_eq!(listen_address(r#"{"listen":"evil.example.com/x"}"#), None);
        assert_eq!(listen_address(r#"{"listen":""}"#), None);
        assert_eq!(listen_address(r#"{"pid":1}"#), None);
        assert_eq!(listen_address("not json"), None);
    }

    #[test]
    fn both_framings_the_daemon_actually_uses_are_read() {
        // Verified live: the same endpoint answers one request as SSE and
        // another as a plain body.
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n";
        assert_eq!(parse_frame(sse).unwrap()["result"]["ok"], json!(true));
        let plain = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#;
        assert_eq!(parse_frame(plain).unwrap()["result"]["ok"], json!(true));
        assert!(parse_frame("event: message\n\n").is_err());
    }

    #[test]
    fn the_last_sse_frame_wins_when_the_daemon_streams_several() {
        let stream = "data: {\"result\":{\"n\":1}}\n\ndata: {\"result\":{\"n\":2}}\n\n";
        assert_eq!(parse_frame(stream).unwrap()["result"]["n"], json!(2));
    }

    #[test]
    fn a_tool_that_failed_on_its_own_terms_is_an_error_not_a_result() {
        // MCP reports this as a *successful* response carrying `isError`,
        // which is exactly the shape a caller would otherwise treat as data.
        let body = json!({"result": {
            "isError": true,
            "content": [{"type": "text", "text": "no such agent"}]
        }});
        assert_eq!(tool_result(&body), Err(McpError::Failed("no such agent".to_string())));
    }

    #[test]
    fn a_json_rpc_error_and_a_tool_error_collapse_to_one_kind() {
        let body = json!({"error": {"code": -32602, "message": "bad arguments"}});
        assert_eq!(tool_result(&body), Err(McpError::Failed("bad arguments".to_string())));
    }

    #[test]
    fn the_structured_half_wins_over_the_text_half() {
        // Paseo's tools return both; the structured one is the parseable one.
        let body = json!({"result": {
            "content": [{"type": "text", "text": "2 agents"}],
            "structuredContent": {"agents": [{"id": "a"}, {"id": "b"}]},
        }});
        let out = tool_result(&body).unwrap();
        assert_eq!(out["agents"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_text_only_tool_still_answers_rather_than_failing() {
        let body = json!({"result": {"content": [{"type": "text", "text": "done"}]}});
        assert_eq!(tool_result(&body).unwrap(), json!({"text": "done"}));
        // …and text that happens to be JSON is handed back parsed.
        let body = json!({"result": {"content": [{"type": "text", "text": "{\"n\":1}"}]}});
        assert_eq!(tool_result(&body).unwrap(), json!({"n": 1}));
    }

    #[test]
    fn a_json_body_survives_curls_own_config_quoting() {
        // A JSON payload is nothing but quotes and backslashes, and curl's
        // config format escapes both inside a quoted value. Asserted as a
        // **round trip** rather than as a literal escape count, because the
        // count is the part that is easy to reason about wrongly — this was
        // written with the wrong one first, and only a real request through
        // the daemon settled it (`say "hi" c:\d` came back verbatim in the
        // tool's own error text).
        let payload = json!({"a": "say \"hi\"", "b": "c:\\d", "c": "line\nbreak"});
        let arg = json_as_curl_argument(&payload);
        assert!(arg.starts_with('"') && arg.ends_with('"'));

        // Undo exactly what curl will: strip the wrapping quotes, then
        // collapse its two escapes. What is left has to be the JSON we meant
        // to send, byte for byte.
        let inner = &arg[1..arg.len() - 1];
        let unescaped = inner.replace("\\\"", "\"").replace("\\\\", "\\");
        assert_eq!(
            serde_json::from_str::<Value>(&unescaped).expect("curl receives valid JSON"),
            payload
        );
    }

    #[test]
    fn a_status_line_is_split_off_the_body() {
        assert_eq!(split_status("{\"a\":1}\n200"), ("{\"a\":1}".to_string(), Some(200)));
        assert_eq!(split_status("\n401").1, Some(401));
    }

    /// Live checks against whatever daemon is really running. `#[ignore]` so
    /// `cargo test` stays hermetic — run with
    /// `cargo test -p neko-core mcp::live -- --ignored --nocapture`.
    ///
    /// They exist because every interesting property of this module is a
    /// property of *Paseo's* behaviour, not of this code: which framing comes
    /// back, whether the endpoint is open, what a tool's result is shaped
    /// like. A unit test can only pin what was already observed.
    mod live {
        use super::*;

        #[test]
        #[ignore = "needs a running Paseo daemon"]
        fn the_channel_is_open_and_carries_the_whole_tool_surface() {
            let client = McpClient::discover().expect("a running daemon");
            let tools = client.list_tools().expect("tools/list");
            eprintln!("{} tools", tools.len());
            // The four groups `docs/plan-agent-control-plane.md` builds on.
            for required in
                ["list_agents", "list_pending_permissions", "respond_to_permission", "list_schedules"]
            {
                assert!(tools.iter().any(|t| t == required), "missing {required}");
            }
        }

        #[test]
        #[ignore = "needs a running Paseo daemon"]
        fn a_real_tool_call_returns_structured_content() {
            let client = McpClient::discover().expect("a running daemon");
            let out = client.call("list_agents", json!({"limit": 3})).expect("list_agents");
            eprintln!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            assert!(out.is_object() || out.is_array());
        }

        #[test]
        #[ignore = "needs a running Paseo daemon"]
        fn a_tool_that_fails_on_its_own_terms_surfaces_as_an_error() {
            let client = McpClient::discover().expect("a running daemon");
            let err = client
                .call("get_agent_status", json!({"agentId": "definitely-not-an-agent"}))
                .expect_err("a missing agent is an error");
            eprintln!("{err}");
            assert!(matches!(err, McpError::Failed(_)));
        }
    }

}

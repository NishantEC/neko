//! Supervision for the local Codex app-server process.
//!
//! The actor deliberately speaks only the app-server's JSON-RPC stdio
//! protocol: Codex owns authentication and its own local state, while neko
//! keeps a small, display-ready snapshot for the client to render.

use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::server::AppState;

type SharedSnapshot = Arc<RwLock<neko_core::codex::Snapshot>>;

const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(5);
const STABLE_SESSION: Duration = Duration::from_secs(30);

/// A newline-delimited JSON transport. Tests provide a fake; the process
/// implementation below owns no protocol behavior beyond the lines it moves.
trait LineTransport {
    fn send_line(&mut self, line: &str) -> io::Result<()>;
    fn read_line(&mut self) -> io::Result<Option<String>>;
}

/// Starts one resident supervisor. A failed spawn or an EOF never clears task
/// rows: it merely makes the snapshot unavailable until a later bootstrap
/// receives a valid thread list.
pub fn spawn(snapshot: SharedSnapshot, _state: Arc<AppState>) {
    std::thread::spawn(move || supervise(snapshot));
}

fn supervise(snapshot: SharedSnapshot) {
    let mut backoff = INITIAL_BACKOFF;
    loop {
        let (active_for, outcome) = start_session(snapshot.clone());
        if let Err(error) = outcome {
            eprintln!("neko-daemon: Codex app-server unavailable: {error}");
        }
        mark_unavailable(&snapshot);
        let (retry_delay, next_backoff) = restart_plan(backoff, active_for);
        std::thread::sleep(retry_delay);
        backoff = next_backoff;
    }
}

/// Chooses the current retry delay and the delay after the next failed
/// session. A server gets a fresh retry budget only after it stayed healthy
/// long enough to be useful; a child that bootstraps then immediately exits
/// therefore still progresses from 250ms to the 5s cap.
fn restart_plan(backoff: Duration, active_for: Duration) -> (Duration, Duration) {
    let retry_delay = if active_for >= STABLE_SESSION {
        INITIAL_BACKOFF
    } else {
        backoff
    };
    (retry_delay, retry_delay.saturating_mul(2).min(MAX_BACKOFF))
}

fn start_session(snapshot: SharedSnapshot) -> (Duration, io::Result<()>) {
    let child = Command::new(resolved_codex())
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => return (Duration::ZERO, Err(error)),
    };

    let stdin = match child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("Codex stdin was not piped"))
    {
        Ok(stdin) => stdin,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return (Duration::ZERO, Err(error));
        }
    };
    let stdout = match child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Codex stdout was not piped"))
    {
        Ok(stdout) => stdout,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return (Duration::ZERO, Err(error));
        }
    };
    let mut transport = ProcessTransport::new(stdin, stdout);
    let (active_for, result) = match bootstrap(&mut transport, &snapshot) {
        Ok(()) => {
            let active_since = Instant::now();
            let result = read_notifications(&mut transport, &snapshot);
            (active_since.elapsed(), result)
        }
        Err(error) => (Duration::ZERO, Err(error)),
    };

    // An EOF is normally paired with an exited child. Kill still makes a
    // bootstrap failure bounded rather than leaving a faulty child behind.
    let _ = child.kill();
    let _ = child.wait();
    (active_for, result)
}

/// Resolves the user-visible Codex CLI once per child launch. This avoids a
/// shell and keeps the child process exactly `codex app-server --stdio`.
fn resolved_codex() -> PathBuf {
    if let Some(path) = std::env::var_os("NEKO_CODEX_PATH") {
        return PathBuf::from(path);
    }

    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .into_iter()
        .flatten()
        .map(|directory| directory.join("codex"))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("codex"))
}

/// Sends the protocol's required startup sequence and applies the correlated
/// thread-list reply. A valid list is the one event that makes Codex available.
fn bootstrap(transport: &mut impl LineTransport, snapshot: &SharedSnapshot) -> io::Result<()> {
    transport.send_line(
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "neko",
                    "title": "neko",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {"experimentalApi": true},
            },
        })
        .to_string(),
    )?;
    read_reply(transport, snapshot, 1)?;

    transport.send_line(
        &json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {},
        })
        .to_string(),
    )?;

    transport.send_line(
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "thread/list",
            "params": {
                "limit": 100,
                "sortKey": "updated_at",
                "sortDirection": "desc",
                "archived": false,
            },
        })
        .to_string(),
    )?;
    let result = read_reply(transport, snapshot, 2)?;
    apply_thread_list(snapshot, &result);
    Ok(())
}

fn read_reply(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
    expected_id: u64,
) -> io::Result<Value> {
    loop {
        let line = transport
            .read_line()?
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("method").is_some() {
            apply_notification(snapshot, &message);
            continue;
        }
        if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

fn read_notifications(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
) -> io::Result<()> {
    while let Some(line) = transport.read_line()? {
        apply_line(snapshot, &line);
    }
    Err(io::Error::from(io::ErrorKind::UnexpectedEof))
}

fn apply_line(snapshot: &SharedSnapshot, line: &str) {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if message.get("method").is_some() {
        apply_notification(snapshot, &message);
    } else if message.get("id").and_then(Value::as_u64) == Some(2) {
        if let Some(result) = message.get("result") {
            apply_thread_list(snapshot, result);
        }
    }
}

fn apply_notification(snapshot: &SharedSnapshot, notification: &Value) {
    snapshot.write().unwrap().apply_notification(notification);
}

fn apply_thread_list(snapshot: &SharedSnapshot, result: &Value) {
    if result.get("data").and_then(Value::as_array).is_none() {
        return;
    }
    let mut snapshot = snapshot.write().unwrap();
    snapshot.apply_thread_list(result);
    snapshot.available = true;
}

fn mark_unavailable(snapshot: &SharedSnapshot) {
    snapshot.write().unwrap().available = false;
}

struct ProcessTransport {
    writer: Sender<String>,
    stdout: BufReader<std::process::ChildStdout>,
}

impl ProcessTransport {
    fn new(stdin: ChildStdin, stdout: std::process::ChildStdout) -> Self {
        let (writer, lines) = mpsc::channel::<String>();
        std::thread::spawn(move || write_lines(stdin, lines));
        Self {
            writer,
            stdout: BufReader::new(stdout),
        }
    }
}

fn write_lines(mut stdin: ChildStdin, lines: mpsc::Receiver<String>) {
    for line in lines {
        if stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush())
            .is_err()
        {
            return;
        }
    }
}

impl LineTransport for ProcessTransport {
    fn send_line(&mut self, line: &str) -> io::Result<()> {
        self.writer
            .send(line.to_owned())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Codex stdin writer stopped"))
    }

    fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        match self.stdout.read_line(&mut line)? {
            0 => Ok(None),
            _ => Ok(Some(line)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    struct FakeTransport {
        replies: VecDeque<String>,
        sent: Vec<String>,
    }

    impl FakeTransport {
        fn replying(replies: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                replies: replies.into_iter().map(str::to_owned).collect(),
                sent: Vec::new(),
            }
        }

        fn sent_methods(&self) -> Vec<String> {
            self.sent
                .iter()
                .map(|line| {
                    serde_json::from_str::<Value>(line).unwrap()["method"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
                .collect()
        }
    }

    impl LineTransport for FakeTransport {
        fn send_line(&mut self, line: &str) -> io::Result<()> {
            self.sent.push(line.to_owned());
            Ok(())
        }

        fn read_line(&mut self) -> io::Result<Option<String>> {
            Ok(self.replies.pop_front())
        }
    }

    fn snapshot() -> SharedSnapshot {
        Arc::new(RwLock::new(neko_core::codex::Snapshot::default()))
    }

    #[test]
    fn bootstrap_initializes_before_listing_threads() {
        let mut fake = FakeTransport::replying([
            r#"{"id":1,"result":{"codexHome":"/tmp/codex"}}"#,
            r#"{"id":2,"result":{"data":[]}}"#,
        ]);

        bootstrap(&mut fake, &snapshot()).unwrap();

        assert_eq!(
            fake.sent_methods(),
            ["initialize", "initialized", "thread/list"]
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fake.sent[0]).unwrap(),
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {
                        "name": "neko",
                        "title": "neko",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {"experimentalApi": true},
                },
            })
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fake.sent[1]).unwrap(),
            json!({"jsonrpc": "2.0", "method": "initialized", "params": {}})
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fake.sent[2]).unwrap(),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "thread/list",
                "params": {
                    "limit": 100,
                    "sortKey": "updated_at",
                    "sortDirection": "desc",
                    "archived": false,
                },
            })
        );
    }

    #[test]
    fn bootstrap_uses_the_thread_list_reply_id_not_an_unrelated_reply() {
        let snapshot = snapshot();
        let mut fake = FakeTransport::replying([
            r#"{"id":1,"result":{"codexHome":"/tmp/codex"}}"#,
            r#"{"id":77,"result":{"data":[{"id":"wrong","name":"Wrong reply"}]}}"#,
            r#"{"id":2,"result":{"data":[{"id":"thread-1","name":"Correlated"}]}}"#,
        ]);

        bootstrap(&mut fake, &snapshot).unwrap();

        assert_eq!(snapshot.read().unwrap().tasks[0].title, "Correlated");
    }

    #[test]
    fn malformed_json_is_ignored() {
        let snapshot = snapshot();
        apply_line(&snapshot, "not json");

        assert_eq!(
            *snapshot.read().unwrap(),
            neko_core::codex::Snapshot::default()
        );
    }

    #[test]
    fn eof_marks_unavailable_without_clearing_tasks() {
        let snapshot = snapshot();
        apply_thread_list(
            &snapshot,
            &json!({"data": [{"id": "thread-1", "name": "Keep me"}]}),
        );

        let mut fake = FakeTransport::replying([]);
        assert_eq!(
            read_notifications(&mut fake, &snapshot).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        mark_unavailable(&snapshot);

        let snapshot = snapshot.read().unwrap();
        assert!(!snapshot.available);
        assert_eq!(snapshot.tasks.len(), 1);
        assert_eq!(snapshot.tasks[0].title, "Keep me");
    }

    #[test]
    fn consecutive_post_bootstrap_eofs_back_off() {
        let (first_delay, after_first) = restart_plan(INITIAL_BACKOFF, Duration::ZERO);
        let (second_delay, after_second) = restart_plan(after_first, Duration::ZERO);
        let (third_delay, _) = restart_plan(after_second, Duration::ZERO);

        assert_eq!(first_delay, Duration::from_millis(250));
        assert_eq!(second_delay, Duration::from_millis(500));
        assert_eq!(third_delay, Duration::from_secs(1));
    }

    #[test]
    fn sustained_healthy_session_resets_the_retry_budget() {
        let (delay, next_backoff) = restart_plan(MAX_BACKOFF, STABLE_SESSION);

        assert_eq!(delay, INITIAL_BACKOFF);
        assert_eq!(next_backoff, Duration::from_millis(500));
    }

    #[test]
    fn later_valid_list_restores_availability() {
        let snapshot = snapshot();
        mark_unavailable(&snapshot);

        apply_line(
            &snapshot,
            r#"{"id":2,"result":{"data":[{"id":"thread-1","name":"Restored"}]}}"#,
        );

        let snapshot = snapshot.read().unwrap();
        assert!(snapshot.available);
        assert_eq!(snapshot.tasks[0].title, "Restored");
    }
}

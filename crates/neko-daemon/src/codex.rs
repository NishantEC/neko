//! Supervision for the local Codex app-server process.
//!
//! The actor deliberately speaks only the app-server's JSON-RPC stdio
//! protocol: Codex owns authentication and its own local state, while neko
//! keeps a small, display-ready snapshot for the client to render.

use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::server::AppState;

type SharedSnapshot = Arc<RwLock<neko_core::codex::Snapshot>>;

const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(5);
const STABLE_SESSION: Duration = Duration::from_secs(30);
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(5);

/// A newline-delimited JSON transport. Tests provide a fake; the process
/// implementation below owns no protocol behavior beyond the lines it moves.
trait LineTransport {
    fn send_line(&mut self, line: &str, timeout: Duration) -> io::Result<()>;
    fn read_line(&mut self, timeout: Option<Duration>) -> io::Result<Option<String>>;
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
    bootstrap_with_timeout(transport, snapshot, BOOTSTRAP_TIMEOUT)
}

fn bootstrap_with_timeout(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
    timeout: Duration,
) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    let result = bootstrap_inner(transport, snapshot, deadline);
    if result.is_err() {
        mark_unavailable(snapshot);
    }
    result
}

fn bootstrap_inner(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
    deadline: Instant,
) -> io::Result<()> {
    send_json(
        transport,
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
        }),
        deadline,
    )?;
    read_reply(transport, snapshot, 1, deadline)?;

    send_json(
        transport,
        json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {},
        }),
        deadline,
    )?;

    send_json(
        transport,
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
        }),
        deadline,
    )?;
    let result = read_reply(transport, snapshot, 2, deadline)?;
    if result.get("data").and_then(Value::as_array).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Codex thread/list result has no data array",
        ));
    }
    apply_thread_list(snapshot, &result);
    Ok(())
}

fn send_json(
    transport: &mut impl LineTransport,
    message: Value,
    deadline: Instant,
) -> io::Result<()> {
    transport.send_line(&message.to_string(), remaining(deadline)?)
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
}

fn read_reply(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
    expected_id: u64,
    deadline: Instant,
) -> io::Result<Value> {
    loop {
        let line = transport
            .read_line(Some(remaining(deadline)?))?
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("method").is_some() {
            apply_notification(snapshot, &message);
            continue;
        }
        if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
            if let Some(error) = message.get("error") {
                let detail = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown JSON-RPC error");
                return Err(io::Error::other(format!("Codex JSON-RPC error: {detail}")));
            }
            return message.get("result").cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Codex JSON-RPC reply has neither result nor error",
                )
            });
        }
    }
}

fn read_notifications(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
) -> io::Result<()> {
    while let Some(line) = transport.read_line(None)? {
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
    writer: Sender<WriteRequest>,
    reader: Receiver<io::Result<Option<String>>>,
}

struct WriteRequest {
    line: String,
    completed: Sender<io::Result<()>>,
}

impl ProcessTransport {
    fn new(stdin: ChildStdin, stdout: std::process::ChildStdout) -> Self {
        let (writer, requests) = mpsc::channel();
        std::thread::spawn(move || write_lines(stdin, requests));
        let (lines, reader) = mpsc::channel();
        std::thread::spawn(move || read_lines(stdout, lines));
        Self { writer, reader }
    }
}

fn write_lines(mut stdin: impl Write, requests: Receiver<WriteRequest>) {
    for request in requests {
        let result = stdin
            .write_all(request.line.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush());
        let failed = result.is_err();
        let _ = request.completed.send(result);
        if failed {
            return;
        }
    }
}

fn read_lines(stdout: std::process::ChildStdout, lines: Sender<io::Result<Option<String>>>) {
    let mut stdout = BufReader::new(stdout);
    loop {
        let mut line = String::new();
        match stdout.read_line(&mut line) {
            Ok(0) => {
                let _ = lines.send(Ok(None));
                return;
            }
            Ok(_) => {
                if lines.send(Ok(Some(line))).is_err() {
                    return;
                }
            }
            Err(error) => {
                let _ = lines.send(Err(error));
                return;
            }
        }
    }
}

fn send_to_writer(writer: &Sender<WriteRequest>, line: &str, timeout: Duration) -> io::Result<()> {
    let (completed, receipt) = mpsc::channel();
    writer
        .send(WriteRequest {
            line: line.to_owned(),
            completed,
        })
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Codex stdin writer stopped"))?;
    receipt.recv_timeout(timeout).map_err(|error| match error {
        mpsc::RecvTimeoutError::Timeout => io::Error::from(io::ErrorKind::TimedOut),
        mpsc::RecvTimeoutError::Disconnected => {
            io::Error::new(io::ErrorKind::BrokenPipe, "Codex stdin writer stopped")
        }
    })?
}

impl LineTransport for ProcessTransport {
    fn send_line(&mut self, line: &str, timeout: Duration) -> io::Result<()> {
        send_to_writer(&self.writer, line, timeout)
    }

    fn read_line(&mut self, timeout: Option<Duration>) -> io::Result<Option<String>> {
        match timeout {
            Some(timeout) => self
                .reader
                .recv_timeout(timeout)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => io::Error::from(io::ErrorKind::TimedOut),
                    mpsc::RecvTimeoutError::Disconnected => {
                        io::Error::new(io::ErrorKind::BrokenPipe, "Codex stdout reader stopped")
                    }
                })?,
            None => self.reader.recv().map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "Codex stdout reader stopped")
            })?,
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
        write_failures: VecDeque<io::ErrorKind>,
        stalled: bool,
    }

    impl FakeTransport {
        fn replying(replies: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                replies: replies.into_iter().map(str::to_owned).collect(),
                sent: Vec::new(),
                write_failures: VecDeque::new(),
                stalled: false,
            }
        }

        fn failing_writes(error: io::ErrorKind) -> Self {
            Self {
                replies: VecDeque::new(),
                sent: Vec::new(),
                write_failures: [error].into(),
                stalled: false,
            }
        }

        fn stalled() -> Self {
            Self {
                replies: VecDeque::new(),
                sent: Vec::new(),
                write_failures: VecDeque::new(),
                stalled: true,
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
        fn send_line(&mut self, line: &str, _timeout: Duration) -> io::Result<()> {
            if let Some(error) = self.write_failures.pop_front() {
                return Err(io::Error::from(error));
            }
            self.sent.push(line.to_owned());
            Ok(())
        }

        fn read_line(&mut self, _timeout: Option<Duration>) -> io::Result<Option<String>> {
            if self.stalled {
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
            Ok(self.replies.pop_front())
        }
    }

    struct FailingWriter;

    impl std::io::Write for FailingWriter {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
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
    fn initialize_error_stops_bootstrap_and_marks_snapshot_unavailable() {
        let snapshot = snapshot();
        snapshot.write().unwrap().available = true;
        let mut fake =
            FakeTransport::replying([r#"{"id":1,"error":{"code":-32000,"message":"not ready"}}"#]);

        assert!(bootstrap(&mut fake, &snapshot).is_err());

        assert_eq!(fake.sent_methods(), ["initialize"]);
        assert!(!snapshot.read().unwrap().available);
    }

    #[test]
    fn thread_list_error_stops_bootstrap_and_marks_snapshot_unavailable() {
        let snapshot = snapshot();
        snapshot.write().unwrap().available = true;
        let mut fake = FakeTransport::replying([
            r#"{"id":1,"result":{"codexHome":"/tmp/codex"}}"#,
            r#"{"id":2,"error":{"code":-32000,"message":"thread list failed"}}"#,
        ]);

        assert!(bootstrap(&mut fake, &snapshot).is_err());

        assert!(!snapshot.read().unwrap().available);
    }

    #[test]
    fn invalid_thread_list_stops_bootstrap_and_marks_snapshot_unavailable() {
        let snapshot = snapshot();
        snapshot.write().unwrap().available = true;
        let mut fake = FakeTransport::replying([
            r#"{"id":1,"result":{"codexHome":"/tmp/codex"}}"#,
            r#"{"id":2,"result":{"data":{}}}"#,
        ]);

        assert!(bootstrap(&mut fake, &snapshot).is_err());

        assert!(!snapshot.read().unwrap().available);
    }

    #[test]
    fn failed_writer_completion_stops_bootstrap_and_marks_snapshot_unavailable() {
        let snapshot = snapshot();
        snapshot.write().unwrap().available = true;
        let mut fake = FakeTransport::failing_writes(io::ErrorKind::BrokenPipe);

        let error = bootstrap(&mut fake, &snapshot).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert!(!snapshot.read().unwrap().available);
    }

    #[test]
    fn stalled_bootstrap_reply_times_out_without_sleeping() {
        let snapshot = snapshot();
        snapshot.write().unwrap().available = true;
        let mut fake = FakeTransport::stalled();

        let error =
            bootstrap_with_timeout(&mut fake, &snapshot, Duration::from_secs(1)).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(!snapshot.read().unwrap().available);
    }

    #[test]
    fn writer_acknowledges_its_io_failure_to_the_requester() {
        let (writer, requests) = mpsc::channel();
        std::thread::spawn(move || write_lines(FailingWriter, requests));

        let error = send_to_writer(&writer, "{}", Duration::from_secs(1)).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
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

//! Supervision for the local Codex app-server process.
//!
//! The actor deliberately speaks only the app-server's JSON-RPC stdio
//! protocol: Codex owns authentication and its own local state, while neko
//! keeps a small, display-ready snapshot for the client to render.

use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::server::AppState;

type SharedSnapshot = Arc<RwLock<neko_core::codex::Snapshot>>;

const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(5);
const STABLE_SESSION: Duration = Duration::from_secs(30);
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_QUEUE_CAPACITY: usize = 16;

/// The only route from a palette click to the supervised stdio actor. It is
/// deliberately a request/reply channel: callers get an honest unavailable
/// error instead of pretending a decision was delivered when no actor owns
/// the app-server stdin.
pub struct ControlHandle {
    snapshot: SharedSnapshot,
    sender: RwLock<Option<SyncSender<ControlRequest>>>,
}

struct ControlRequest {
    /// `Some` means this is the one explicit task-view activation that may
    /// ask Codex for bounded history. `None` is an approval response.
    task_id: Option<String>,
    thread_id: String,
    request_id: Value,
    approve: bool,
    /// A new task is the only control operation that does not name an
    /// existing thread or approval. It still carries an already-validated,
    /// explicit directory — never an inherited daemon cwd.
    start_task: Option<neko_core::codex::StartTask>,
    completion: Arc<ControlCompletion>,
}

/// One exact decision moves from pending to actor-owned writing under this
/// mutex. A timeout may cancel only pending work; once writing owns it, the
/// caller waits for its confirmed outcome instead of returning a false timeout.
struct ControlCompletion {
    phase: Mutex<ControlPhase>,
    changed: Condvar,
}

enum ControlPhase {
    Pending,
    Writing,
    Finished(Result<(), String>),
    Cancelled,
}

impl ControlCompletion {
    fn new() -> Self {
        Self {
            phase: Mutex::new(ControlPhase::Pending),
            changed: Condvar::new(),
        }
    }

    fn claim_write(&self) -> bool {
        let mut phase = self
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*phase, ControlPhase::Pending) {
            *phase = ControlPhase::Writing;
            true
        } else {
            false
        }
    }

    fn finish(&self, result: Result<(), neko_core::provider::ProviderError>) {
        let mut phase = self
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*phase, ControlPhase::Writing) {
            *phase = ControlPhase::Finished(result.map_err(|error| error.0));
            self.changed.notify_all();
        }
    }

    fn wait_for_outcome(
        &self,
        timeout: Duration,
    ) -> Result<(), neko_core::provider::ProviderError> {
        let deadline = Instant::now() + timeout;
        let mut phase = self
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            match &*phase {
                ControlPhase::Finished(result) => {
                    return result.clone().map_err(neko_core::provider::ProviderError);
                }
                ControlPhase::Cancelled => {
                    return Err(neko_core::provider::ProviderError(
                        "Codex did not accept the approval in time".to_string(),
                    ));
                }
                ControlPhase::Writing => {
                    phase = self.changed.wait(phase).unwrap();
                }
                ControlPhase::Pending => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let (next, timed_out) = self.changed.wait_timeout(phase, remaining).unwrap();
                    phase = next;
                    if timed_out.timed_out() && matches!(*phase, ControlPhase::Pending) {
                        *phase = ControlPhase::Cancelled;
                        self.changed.notify_all();
                        return Err(neko_core::provider::ProviderError(
                            "Codex did not accept the approval in time".to_string(),
                        ));
                    }
                }
            }
        }
    }
}

impl ControlHandle {
    pub fn new(snapshot: SharedSnapshot) -> Self {
        Self {
            snapshot,
            sender: RwLock::new(None),
        }
    }

    fn attach(&self, sender: SyncSender<ControlRequest>) {
        *self.sender.write().unwrap() = Some(sender);
    }

    fn detach(&self) {
        *self.sender.write().unwrap() = None;
    }
}

impl neko_core::codex::CodexControl for ControlHandle {
    fn resolve_approval(
        &self,
        thread_id: &str,
        request_id: &Value,
        approve: bool,
    ) -> Result<(), neko_core::provider::ProviderError> {
        self.resolve_approval_with_timeout(thread_id, request_id, approve, CONTROL_TIMEOUT)
    }

    fn open_task(&self, thread_id: &str) -> Result<(), neko_core::provider::ProviderError> {
        if !self
            .snapshot
            .read()
            .unwrap()
            .tasks
            .iter()
            .any(|task| task.id == thread_id)
        {
            return Err(neko_core::provider::ProviderError(
                "Codex task is no longer available".to_string(),
            ));
        }
        let sender = self.sender.read().unwrap().clone().ok_or_else(|| {
            neko_core::provider::ProviderError("Codex is unavailable".to_string())
        })?;
        let completion = Arc::new(ControlCompletion::new());
        sender
            .try_send(ControlRequest {
                task_id: Some(thread_id.to_owned()),
                thread_id: String::new(),
                request_id: Value::Null,
                approve: false,
                start_task: None,
                completion: completion.clone(),
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => neko_core::provider::ProviderError(
                    "Codex is busy processing another request".to_string(),
                ),
                TrySendError::Disconnected(_) => {
                    neko_core::provider::ProviderError("Codex is unavailable".to_string())
                }
            })?;
        completion.wait_for_outcome(CONTROL_TIMEOUT)
    }

    fn start_task(
        &self,
        start: neko_core::codex::StartTask,
    ) -> Result<(), neko_core::provider::ProviderError> {
        self.start_task_with_timeout(start)
    }
}

impl ControlHandle {
    fn resolve_approval_with_timeout(
        &self,
        thread_id: &str,
        request_id: &Value,
        approve: bool,
        timeout: Duration,
    ) -> Result<(), neko_core::provider::ProviderError> {
        if !self
            .snapshot
            .read()
            .unwrap()
            .can_resolve_id(thread_id, request_id)
        {
            return Err(neko_core::provider::ProviderError(
                "approval was already resolved".to_string(),
            ));
        }
        let sender = self.sender.read().unwrap().clone().ok_or_else(|| {
            neko_core::provider::ProviderError("Codex is unavailable".to_string())
        })?;
        let completion = Arc::new(ControlCompletion::new());
        sender
            .try_send(ControlRequest {
                task_id: None,
                thread_id: thread_id.to_owned(),
                request_id: request_id.clone(),
                approve,
                start_task: None,
                completion: completion.clone(),
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => neko_core::provider::ProviderError(
                    "Codex is busy processing another approval".to_string(),
                ),
                TrySendError::Disconnected(_) => {
                    neko_core::provider::ProviderError("Codex is unavailable".to_string())
                }
            })?;
        completion.wait_for_outcome(timeout)
    }

    fn start_task_with_timeout(
        &self,
        start: neko_core::codex::StartTask,
    ) -> Result<(), neko_core::provider::ProviderError> {
        // Re-validate at the actor boundary: the provider's row is a
        // keystroke old and a directory can disappear between selection and
        // the first JSON-RPC write. No request is sent until this passes.
        let start = neko_core::codex::StartTask::new(&start.prompt, Some(start.cwd))
            .map_err(|message| neko_core::provider::ProviderError(message.to_string()))?;
        let sender = self.sender.read().unwrap().clone().ok_or_else(|| {
            neko_core::provider::ProviderError("Codex is unavailable".to_string())
        })?;
        let completion = Arc::new(ControlCompletion::new());
        sender
            .try_send(ControlRequest {
                task_id: None,
                thread_id: String::new(),
                request_id: Value::Null,
                approve: false,
                start_task: Some(start),
                completion: completion.clone(),
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => neko_core::provider::ProviderError(
                    "Codex is busy processing another request".to_string(),
                ),
                TrySendError::Disconnected(_) => {
                    neko_core::provider::ProviderError("Codex is unavailable".to_string())
                }
            })?;
        completion.wait_for_outcome(CONTROL_TIMEOUT)
    }
}

/// A newline-delimited JSON transport. Tests provide a fake; the process
/// implementation below owns no protocol behavior beyond the lines it moves.
trait LineTransport {
    fn send_line(&mut self, line: &str, timeout: Duration) -> io::Result<()>;
    fn read_line(&mut self, timeout: Option<Duration>) -> io::Result<Option<String>>;
}

/// Starts one resident supervisor. A failed spawn or an EOF never clears task
/// rows: it merely makes the snapshot unavailable until a later bootstrap
/// receives a valid thread list.
pub fn spawn(snapshot: SharedSnapshot, state: Arc<AppState>) {
    let control = state.codex_control.clone();
    std::thread::spawn(move || supervise(snapshot, control, state));
}

fn supervise(snapshot: SharedSnapshot, control: Arc<ControlHandle>, state: Arc<AppState>) {
    let mut backoff = INITIAL_BACKOFF;
    loop {
        let (active_for, outcome) = start_session(snapshot.clone(), control.clone(), state.clone());
        if let Err(error) = outcome {
            eprintln!("neko-daemon: Codex app-server unavailable: {error}");
        }
        mark_unavailable(&snapshot);
        state.set_codex_attention(0);
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

fn start_session(
    snapshot: SharedSnapshot,
    control: Arc<ControlHandle>,
    state: Arc<AppState>,
) -> (Duration, io::Result<()>) {
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
    let (controls, control_requests) = mpsc::sync_channel(CONTROL_QUEUE_CAPACITY);
    control.attach(controls);
    let (active_for, result) = match bootstrap(&mut transport, &snapshot) {
        Ok(()) => {
            state.set_codex_attention(snapshot.read().unwrap().approvals.len());
            let active_since = Instant::now();
            let result = read_notifications_with_controls(
                &mut transport,
                &snapshot,
                &control_requests,
                &state,
            );
            (active_since.elapsed(), result)
        }
        Err(error) => (Duration::ZERO, Err(error)),
    };
    control.detach();

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
    let initialized = read_reply(transport, snapshot, 1, deadline)?;
    snapshot
        .write()
        .unwrap()
        .set_history_available(experimental_history_supported(&initialized));

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

fn experimental_history_supported(result: &Value) -> bool {
    result
        .get("capabilities")
        .and_then(|capabilities| capabilities.get("experimentalApi"))
        .and_then(Value::as_bool)
        == Some(true)
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

fn read_notifications_with_controls(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
    controls: &Receiver<ControlRequest>,
    state: &AppState,
) -> io::Result<()> {
    // Bootstrap owns ids 1 and 2. Every later task view gets a fresh id, so
    // a reply that arrives after a timed-out view cannot be mistaken for the
    // next selected task's history.
    let mut next_request_id = 3_u64;
    loop {
        while let Ok(request) = controls.try_recv() {
            if !request.completion.claim_write() {
                continue;
            }
            let response = match (&request.start_task, &request.task_id) {
                (Some(start), _) => {
                    let thread_request_id = next_request_id;
                    let turn_request_id = next_request_id.saturating_add(1);
                    next_request_id = turn_request_id.saturating_add(1);
                    start_task(
                        transport,
                        snapshot,
                        start,
                        thread_request_id,
                        turn_request_id,
                    )
                }
                (None, Some(thread_id)) => {
                    let request_id = next_request_id;
                    next_request_id = next_request_id.saturating_add(1);
                    request_task_history(transport, snapshot, thread_id, request_id)
                }
                (None, None) => respond_to_approval(transport, snapshot, &request),
            };
            match response {
                Ok(result) => {
                    if result.is_ok() {
                        state.set_codex_attention(snapshot.read().unwrap().approvals.len());
                    }
                    request.completion.finish(result);
                }
                Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                    // `send_to_writer` only returns this timeout after it
                    // atomically canceled a still-pending writer entry, so
                    // no bytes were sent and this approval remains safe to
                    // retry on the same live session.
                    request
                        .completion
                        .finish(Err(neko_core::provider::ProviderError(
                            "Codex did not accept the approval in time".to_string(),
                        )));
                }
                Err(error) => {
                    // A failed stdin write means this session cannot honestly
                    // remain available. Clear its controls now; returning the
                    // error sends the supervisor through its restart path.
                    mark_unavailable(snapshot);
                    state.set_codex_attention(0);
                    request
                        .completion
                        .finish(Err(neko_core::provider::ProviderError(
                            "Codex is unavailable".to_string(),
                        )));
                    return Err(error);
                }
            }
        }
        match transport.read_line(Some(Duration::from_millis(50))) {
            Ok(Some(line)) => {
                apply_line(snapshot, &line);
                state.set_codex_attention(snapshot.read().unwrap().approvals.len());
            }
            Ok(None) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
            Err(error) if error.kind() == io::ErrorKind::TimedOut => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Creates the explicit Codex thread before sending its first text turn. The
/// two calls are deliberately sequential: a turn cannot be associated with a
/// guessed thread id, and neither call creates a worktree or chooses a cwd.
fn start_task(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
    start: &neko_core::codex::StartTask,
    thread_request_id: u64,
    turn_request_id: u64,
) -> io::Result<Result<(), neko_core::provider::ProviderError>> {
    let start = match neko_core::codex::StartTask::new(&start.prompt, Some(start.cwd.clone())) {
        Ok(start) => start,
        Err(message) => return Ok(Err(neko_core::provider::ProviderError(message.to_string()))),
    };
    let deadline = Instant::now() + CONTROL_TIMEOUT;
    send_json(
        transport,
        json!({
            "jsonrpc": "2.0",
            "id": thread_request_id,
            "method": "thread/start",
            "params": {"cwd": start.cwd},
        }),
        deadline,
    )?;
    let thread = match read_start_reply(transport, snapshot, thread_request_id, deadline) {
        Ok(thread) => thread,
        Err(StartReplyError::Rpc(error)) => return Ok(Err(error)),
        Err(StartReplyError::OutcomeUnknown(error)) => return Ok(Err(error)),
        Err(StartReplyError::Transport(error)) => return Err(error),
    };
    let Some(thread_id) = thread
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
    else {
        return Ok(Err(neko_core::provider::ProviderError(
            "Codex did not provide the new task id".to_string(),
        )));
    };
    send_json(
        transport,
        json!({
            "jsonrpc": "2.0",
            "id": turn_request_id,
            "method": "turn/start",
            "params": {
                "threadId": thread_id,
                "input": [{"type": "text", "text": start.prompt}],
            },
        }),
        deadline,
    )?;
    match read_start_reply(transport, snapshot, turn_request_id, deadline) {
        Ok(_) => {}
        Err(StartReplyError::Rpc(error)) => return Ok(Err(error)),
        Err(StartReplyError::OutcomeUnknown(error)) => return Ok(Err(error)),
        Err(StartReplyError::Transport(error)) => return Err(error),
    }
    Ok(Ok(()))
}

/// Start commands need a different error boundary from bootstrap: a valid
/// JSON-RPC `error` is Codex rejecting this one task, not evidence the stdio
/// session has died. Once a start command was successfully written, a timeout
/// or EOF cannot prove whether Codex created the task, so it is deliberately
/// surfaced as an inline unknown outcome rather than retried or restarted.
/// Other transport/parse failures retain the bootstrap restart behavior.
enum StartReplyError {
    Rpc(neko_core::provider::ProviderError),
    OutcomeUnknown(neko_core::provider::ProviderError),
    Transport(io::Error),
}

fn unknown_start_outcome() -> StartReplyError {
    StartReplyError::OutcomeUnknown(neko_core::provider::ProviderError(
        "Codex task start outcome is unknown; it may have started. Check Codex before trying again."
            .to_string(),
    ))
}

fn read_start_reply(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
    expected_id: u64,
    deadline: Instant,
) -> Result<Value, StartReplyError> {
    loop {
        let timeout = remaining(deadline).map_err(|_| unknown_start_outcome())?;
        let line = transport
            .read_line(Some(timeout))
            .map_err(|error| match error.kind() {
                io::ErrorKind::TimedOut
                | io::ErrorKind::UnexpectedEof
                | io::ErrorKind::BrokenPipe => unknown_start_outcome(),
                _ => StartReplyError::Transport(error),
            })?
            .ok_or_else(unknown_start_outcome)?;
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
                return Err(StartReplyError::Rpc(neko_core::provider::ProviderError(
                    format!("Codex: {detail}"),
                )));
            }
            return message.get("result").cloned().ok_or_else(|| {
                StartReplyError::Transport(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Codex JSON-RPC reply has neither result nor error",
                ))
            });
        }
    }
}

/// The only history request. It is called solely by a `CodexTaskProvider`
/// activation after the task id was checked against the live snapshot; never
/// from a root or scoped search.
fn request_task_history(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
    thread_id: &str,
    request_id: u64,
) -> io::Result<Result<(), neko_core::provider::ProviderError>> {
    let snapshot_now = snapshot.read().unwrap();
    if !snapshot_now.tasks.iter().any(|task| task.id == thread_id) {
        return Ok(Err(neko_core::provider::ProviderError(
            "Codex task is no longer available".to_string(),
        )));
    }
    if !snapshot_now.history_available {
        return Ok(Ok(()));
    }
    drop(snapshot_now);
    let deadline = Instant::now() + CONTROL_TIMEOUT;
    send_json(
        transport,
        json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "thread/turns/list",
            "params": {"threadId": thread_id, "limit": 40},
        }),
        deadline,
    )?;
    let result = read_reply(transport, snapshot, request_id, deadline)?;
    if result.get("data").and_then(Value::as_array).is_none() {
        return Ok(Err(neko_core::provider::ProviderError(
            "Codex did not provide task history".to_string(),
        )));
    }
    snapshot
        .write()
        .unwrap()
        .apply_turn_list(thread_id, &result);
    Ok(Ok(()))
}

fn respond_to_approval(
    transport: &mut impl LineTransport,
    snapshot: &SharedSnapshot,
    request: &ControlRequest,
) -> io::Result<Result<(), neko_core::provider::ProviderError>> {
    let (request_id, kind, permissions) = {
        let snapshot = snapshot.read().unwrap();
        let Some(approval) = snapshot.approvals.iter().find(|approval| {
            approval.thread_id == request.thread_id && approval.request_id == request.request_id
        }) else {
            return Ok(Err(neko_core::provider::ProviderError(
                "approval was already resolved".to_string(),
            )));
        };
        (
            approval.request_id.clone(),
            approval.kind.clone(),
            approval.permissions.clone(),
        )
    };
    let result = match kind {
        neko_core::codex::ApprovalKind::Permissions => {
            let permissions = if request.approve {
                let Some(permissions) = permissions else {
                    return Ok(Err(neko_core::provider::ProviderError(
                        "Codex permission request is missing its permission profile".to_string(),
                    )));
                };
                permissions
            } else {
                json!({})
            };
            // `item/permissions/requestApproval` does not accept the
            // command/file-change `decision` enum. Accept grants exactly
            // the profile the server requested for this turn; decline grants
            // an empty profile, which is the documented no-permission reply.
            json!({"permissions": permissions, "scope": "turn"})
        }
        _ => json!({"decision": if request.approve { "accept" } else { "decline" }}),
    };
    transport.send_line(
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result,
        })
        .to_string(),
        CONTROL_TIMEOUT,
    )?;
    snapshot
        .write()
        .unwrap()
        .resolve_id(&request.thread_id, &request.request_id);
    Ok(Ok(()))
}

fn apply_line(snapshot: &SharedSnapshot, line: &str) {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if message.get("method").is_some() {
        apply_notification(snapshot, &message);
    } else if message.get("id").and_then(Value::as_u64) == Some(2)
        && let Some(result) = message.get("result")
    {
        apply_thread_list(snapshot, result);
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
    let mut snapshot = snapshot.write().unwrap();
    snapshot.available = false;
    snapshot.clear_approvals();
}

struct ProcessTransport {
    writer: Sender<WriteRequest>,
    reader: Receiver<io::Result<Option<String>>>,
}

struct WriteRequest {
    line: String,
    gate: Arc<WriteGate>,
    completed: Sender<io::Result<()>>,
}

/// Keeps a timed receipt from returning while its writer queue entry can
/// still take ownership of stdin. Cancellation and writer ownership use the
/// same mutex, so a timed-out entry is either removed before any bytes are
/// written or waited through to its real completion.
struct WriteGate(Mutex<WritePhase>);

enum WritePhase {
    Pending,
    Writing,
    Cancelled,
}

impl WriteGate {
    fn new() -> Self {
        Self(Mutex::new(WritePhase::Pending))
    }

    fn claim_write(&self) -> bool {
        let mut phase = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*phase, WritePhase::Pending) {
            *phase = WritePhase::Writing;
            true
        } else {
            false
        }
    }

    fn cancel_if_pending(&self) -> bool {
        let mut phase = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*phase, WritePhase::Pending) {
            *phase = WritePhase::Cancelled;
            true
        } else {
            false
        }
    }
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
        if !request.gate.claim_write() {
            let _ = request.completed.send(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Codex stdin write was cancelled",
            )));
            continue;
        }
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
    let gate = Arc::new(WriteGate::new());
    writer
        .send(WriteRequest {
            line: line.to_owned(),
            gate: gate.clone(),
            completed,
        })
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Codex stdin writer stopped"))?;
    match receipt.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) if gate.cancel_if_pending() => {
            Err(io::Error::from(io::ErrorKind::TimedOut))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => receipt
            .recv()
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Codex stdin writer stopped"))?,
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "Codex stdin writer stopped",
        )),
    }
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
            match self.replies.pop_front() {
                Some(reply) if reply == "__timeout__" => {
                    Err(io::Error::from(io::ErrorKind::TimedOut))
                }
                reply => Ok(reply),
            }
        }
    }

    struct BlockingTransport {
        entered_write: Sender<()>,
        release_write: Receiver<()>,
        sent: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl LineTransport for BlockingTransport {
        fn send_line(&mut self, line: &str, _timeout: Duration) -> io::Result<()> {
            self.entered_write.send(()).unwrap();
            self.release_write.recv().unwrap();
            self.sent
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(line.to_owned());
            Ok(())
        }

        fn read_line(&mut self, _timeout: Option<Duration>) -> io::Result<Option<String>> {
            Ok(None)
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

    struct BlockingWriter {
        entered_write: Option<Sender<()>>,
        release_write: Receiver<()>,
        bytes: Arc<std::sync::Mutex<Vec<u8>>>,
    }

    impl std::io::Write for BlockingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(entered_write) = self.entered_write.take() {
                entered_write.send(()).unwrap();
                self.release_write.recv().unwrap();
            }
            self.bytes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn snapshot() -> SharedSnapshot {
        Arc::new(RwLock::new(neko_core::codex::Snapshot::default()))
    }

    fn state() -> AppState {
        AppState::new(neko_core::db::Db::open_in_memory().unwrap(), Vec::new())
    }

    #[test]
    fn bootstrap_initializes_before_listing_threads() {
        let mut fake = FakeTransport::replying([
            r#"{"id":1,"result":{"codexHome":"/tmp/codex","capabilities":{"experimentalApi":true}}}"#,
            r#"{"id":2,"result":{"data":[]}}"#,
        ]);

        let snapshot = snapshot();
        bootstrap(&mut fake, &snapshot).unwrap();
        assert!(snapshot.read().unwrap().history_available);

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
    fn starting_a_task_sends_the_selected_cwd_then_uses_its_thread_for_the_first_turn() {
        let cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let start = neko_core::codex::StartTask::new("Fix ranking", Some(cwd.clone()))
            .expect("the fixture directory exists");
        let mut fake = FakeTransport::replying([
            r#"{"id":3,"result":{"thread":{"id":"thr-new"}}}"#,
            r#"{"id":4,"result":{}}"#,
        ]);

        start_task(&mut fake, &snapshot(), &start, 3, 4)
            .unwrap()
            .unwrap();

        assert_eq!(
            serde_json::from_str::<Value>(&fake.sent[0]).unwrap(),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "thread/start",
                "params": {"cwd": cwd},
            })
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fake.sent[1]).unwrap(),
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "turn/start",
                "params": {
                    "threadId": "thr-new",
                    "input": [{"type": "text", "text": "Fix ranking"}],
                },
            })
        );
    }

    #[test]
    fn start_command_json_rpc_errors_are_inline_and_never_send_an_unrelated_turn() {
        let cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let start = neko_core::codex::StartTask::new("Fix ranking", Some(cwd))
            .expect("the fixture directory exists");

        for (replies, expected_sent, expected_error) in [
            (
                vec![r#"{"id":3,"error":{"message":"project is unavailable"}}"#],
                1,
                "Codex: project is unavailable",
            ),
            (
                vec![
                    r#"{"id":3,"result":{"thread":{"id":"thr-new"}}}"#,
                    r#"{"id":4,"error":{"message":"turn was rejected"}}"#,
                ],
                2,
                "Codex: turn was rejected",
            ),
        ] {
            let snapshot = snapshot();
            snapshot.write().unwrap().available = true;
            let mut fake = FakeTransport::replying(replies);

            let error = start_task(&mut fake, &snapshot, &start, 3, 4)
                .expect("an application error is not a transport failure")
                .unwrap_err();

            assert_eq!(error.to_string(), expected_error);
            assert_eq!(fake.sent.len(), expected_sent);
            assert!(
                snapshot.read().unwrap().available,
                "the actor session stays live"
            );
        }
    }

    #[test]
    fn start_reply_loss_after_a_write_is_unknown_and_never_retries_the_non_idempotent_command() {
        let cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let start = neko_core::codex::StartTask::new("Fix ranking", Some(cwd))
            .expect("the fixture directory exists");

        for (replies, expected_methods) in [
            (vec!["__timeout__"], vec!["thread/start"]),
            (
                vec![
                    r#"{"id":3,"result":{"thread":{"id":"thr-new"}}}"#,
                    "__timeout__",
                ],
                vec!["thread/start", "turn/start"],
            ),
        ] {
            let snapshot = snapshot();
            snapshot.write().unwrap().available = true;
            let mut fake = FakeTransport::replying(replies);

            let error = start_task(&mut fake, &snapshot, &start, 3, 4)
                .expect("a post-write unknown outcome keeps the session available")
                .unwrap_err();

            assert!(error.to_string().contains("outcome is unknown"));
            assert_eq!(fake.sent_methods(), expected_methods);
            assert!(snapshot.read().unwrap().available);
        }
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
    fn explicit_task_open_requests_only_that_tasks_bounded_turn_summaries() {
        let snapshot = snapshot();
        {
            let mut state = snapshot.write().unwrap();
            state.replace_tasks(vec![neko_core::codex::Task {
                id: "thr-1".into(),
                title: "Only this task".into(),
                opening_prompt: None,
                cwd: None,
                provider: None,
                updated_at: 0,
                status: neko_core::codex::TaskStatus::Idle,
            }]);
            state.set_history_available(true);
        }
        let mut fake = FakeTransport::replying([
            r#"{"id":3,"result":{"data":[{"id":"turn-1","summary":"Ran tests","status":"completed"}]}}"#,
        ]);

        assert!(
            request_task_history(&mut fake, &snapshot, "thr-1", 3)
                .unwrap()
                .is_ok()
        );
        let request: Value = serde_json::from_str(&fake.sent[0]).unwrap();
        assert_eq!(request["method"], "thread/turns/list");
        assert_eq!(request["params"], json!({"threadId":"thr-1","limit":40}));
        assert_eq!(
            snapshot.read().unwrap().activity["thr-1"][0].summary,
            "Ran tests"
        );
    }

    #[test]
    fn a_late_turn_reply_is_ignored_before_the_next_selected_tasks_reply() {
        let snapshot = snapshot();
        {
            let mut state = snapshot.write().unwrap();
            state.replace_tasks(vec![
                neko_core::codex::Task {
                    id: "thr-a".into(),
                    title: "A".into(),
                    opening_prompt: None,
                    cwd: None,
                    provider: None,
                    updated_at: 0,
                    status: neko_core::codex::TaskStatus::Idle,
                },
                neko_core::codex::Task {
                    id: "thr-b".into(),
                    title: "B".into(),
                    opening_prompt: None,
                    cwd: None,
                    provider: None,
                    updated_at: 0,
                    status: neko_core::codex::TaskStatus::Idle,
                },
            ]);
            state.set_history_available(true);
        }
        let mut fake = FakeTransport::replying([
            "__timeout__",
            r#"{"id":3,"result":{"data":[{"id":"late","summary":"A late reply"}]}}"#,
            r#"{"id":4,"result":{"data":[{"id":"current","summary":"B current reply"}]}}"#,
        ]);

        assert!(request_task_history(&mut fake, &snapshot, "thr-a", 3).is_err());
        assert!(
            request_task_history(&mut fake, &snapshot, "thr-b", 4)
                .unwrap()
                .is_ok()
        );
        assert_eq!(
            snapshot.read().unwrap().activity["thr-b"][0].summary,
            "B current reply"
        );
        let ids: Vec<u64> = fake
            .sent
            .iter()
            .map(|line| {
                serde_json::from_str::<Value>(line).unwrap()["id"]
                    .as_u64()
                    .unwrap()
            })
            .collect();
        assert_eq!(ids, [3, 4]);
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
    fn a_timed_out_writer_request_never_flushes_later() {
        let (writer, requests) = mpsc::channel();
        let (entered_write, writer_entered) = mpsc::channel();
        let (release_write, writer_release) = mpsc::channel();
        let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let written = bytes.clone();
        let writer_thread = std::thread::spawn(move || {
            write_lines(
                BlockingWriter {
                    entered_write: Some(entered_write),
                    release_write: writer_release,
                    bytes: written,
                },
                requests,
            )
        });

        let sender = writer.clone();
        let requester = std::thread::spawn(move || {
            send_to_writer(&sender, "approval", Duration::from_millis(10))
        });
        writer_entered.recv().unwrap();
        std::thread::sleep(Duration::from_millis(30));
        release_write.send(()).unwrap();
        let result = requester.join().unwrap();
        drop(writer);
        writer_thread.join().unwrap();
        assert!(
            result.is_ok()
                || bytes
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_empty(),
            "a writer timeout must cancel before bytes are owned, or wait for the written result"
        );
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
    fn a_stale_approval_control_request_writes_no_newer_decision() {
        let snapshot = snapshot();
        snapshot
            .write()
            .unwrap()
            .apply_notification(&json!({
                "jsonrpc": "2.0",
                "id": "new-request",
                "method": "item/commandExecution/requestApproval",
                "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1, "command": "pwd"},
            }));
        let request = ControlRequest {
            task_id: None,
            thread_id: "thr".to_string(),
            request_id: json!("old-request"),
            approve: true,
            start_task: None,
            completion: Arc::new(ControlCompletion::new()),
        };
        let mut fake = FakeTransport::replying([]);

        let error = respond_to_approval(&mut fake, &snapshot, &request)
            .unwrap()
            .unwrap_err();

        assert_eq!(error.to_string(), "approval was already resolved");
        assert!(
            fake.sent.is_empty(),
            "a stale click must not answer the newer request"
        );
        assert!(snapshot.read().unwrap().can_resolve("thr", "new-request"));
    }

    #[test]
    fn timed_out_control_is_cancelled_before_a_later_actor_turn_can_write() {
        let snapshot = Arc::new(RwLock::new(neko_core::codex::Snapshot::with_approval(
            "thr", "request",
        )));
        let handle = ControlHandle::new(snapshot.clone());
        let (sender, pending) = mpsc::sync_channel(CONTROL_QUEUE_CAPACITY);
        handle.attach(sender);

        let error = handle
            .resolve_approval_with_timeout("thr", &json!("request"), true, Duration::ZERO)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Codex did not accept the approval in time"
        );

        let (actor_sender, actor_controls) = mpsc::channel();
        actor_sender.send(pending.recv().unwrap()).unwrap();
        let mut fake = FakeTransport::replying([]);
        let error =
            read_notifications_with_controls(&mut fake, &snapshot, &actor_controls, &state())
                .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(
            fake.sent.is_empty(),
            "a timed-out click must never write later"
        );
        assert!(snapshot.read().unwrap().can_resolve("thr", "request"));
    }

    #[test]
    fn actor_pickup_never_returns_timeout_then_writes_an_approval() {
        let snapshot = Arc::new(RwLock::new(neko_core::codex::Snapshot::with_approval(
            "thr", "request",
        )));
        let handle = Arc::new(ControlHandle::new(snapshot.clone()));
        let (sender, pending) = mpsc::sync_channel(CONTROL_QUEUE_CAPACITY);
        handle.attach(sender);
        let (caller_result, caller_waiting) = mpsc::channel();
        let caller = std::thread::spawn(move || {
            caller_result
                .send(handle.resolve_approval_with_timeout(
                    "thr",
                    &json!("request"),
                    true,
                    Duration::from_millis(10),
                ))
                .unwrap();
        });

        let request = pending.recv().unwrap();
        let (entered_write, writer_entered) = mpsc::channel();
        let (release_write, writer_release) = mpsc::channel();
        let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
        let actor_snapshot = snapshot.clone();
        let actor_sent = sent.clone();
        let actor = std::thread::spawn(move || {
            let mut transport = BlockingTransport {
                entered_write,
                release_write: writer_release,
                sent: actor_sent,
            };
            assert!(request.completion.claim_write());
            let result = respond_to_approval(&mut transport, &actor_snapshot, &request).unwrap();
            request.completion.finish(result);
        });

        writer_entered.recv().unwrap();
        // Let the caller's short deadline elapse while the actor owns the
        // blocked write. The fixed handoff must keep it waiting for an ack.
        std::thread::sleep(Duration::from_millis(30));
        release_write.send(()).unwrap();
        let caller_result = caller_waiting.recv().unwrap();
        actor.join().unwrap();
        caller.join().unwrap();

        assert!(
            caller_result.is_ok()
                || (sent
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_empty()
                    && snapshot.read().unwrap().can_resolve("thr", "request")),
            "a caller must not receive failure while the actor later writes its approval"
        );
    }

    #[test]
    fn approval_write_failure_marks_the_session_unavailable_and_returns_for_restart() {
        let state = state();
        let snapshot = state.codex.clone();
        snapshot.write().unwrap().apply_notification(&json!({
            "id": "request",
            "method": "item/fileChange/requestApproval",
            "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1}
        }));
        let (sender, controls) = mpsc::channel();
        let completion = Arc::new(ControlCompletion::new());
        sender
            .send(ControlRequest {
                task_id: None,
                thread_id: "thr".to_string(),
                request_id: json!("request"),
                approve: true,
                start_task: None,
                completion: completion.clone(),
            })
            .unwrap();

        let error = read_notifications_with_controls(
            &mut FakeTransport::failing_writes(io::ErrorKind::BrokenPipe),
            &snapshot,
            &controls,
            &state,
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert!(!snapshot.read().unwrap().available);
        assert!(snapshot.read().unwrap().approvals.is_empty());
        assert_eq!(
            completion
                .wait_for_outcome(Duration::ZERO)
                .unwrap_err()
                .to_string(),
            "Codex is unavailable"
        );
    }

    #[test]
    fn a_cancelled_writer_timeout_keeps_the_approval_pending() {
        let state = state();
        let snapshot = state.codex.clone();
        snapshot.write().unwrap().apply_notification(&json!({
            "id": "request",
            "method": "item/fileChange/requestApproval",
            "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1}
        }));
        let (sender, controls) = mpsc::channel();
        let completion = Arc::new(ControlCompletion::new());
        sender
            .send(ControlRequest {
                task_id: None,
                thread_id: "thr".to_string(),
                request_id: json!("request"),
                approve: true,
                start_task: None,
                completion: completion.clone(),
            })
            .unwrap();

        let error = read_notifications_with_controls(
            &mut FakeTransport::failing_writes(io::ErrorKind::TimedOut),
            &snapshot,
            &controls,
            &state,
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(snapshot.read().unwrap().can_resolve("thr", "request"));
        assert_eq!(
            completion
                .wait_for_outcome(Duration::ZERO)
                .unwrap_err()
                .to_string(),
            "Codex did not accept the approval in time"
        );
    }

    #[test]
    fn an_explicit_approval_writes_the_matching_json_rpc_response() {
        let snapshot = snapshot();
        snapshot
            .write()
            .unwrap()
            .apply_notification(&json!({
                "jsonrpc": "2.0",
                "id": 42,
                "method": "item/fileChange/requestApproval",
                "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1, "reason": "edit config"},
            }));
        snapshot.write().unwrap().apply_notification(&json!({
            "jsonrpc": "2.0",
            "id": "42",
            "method": "item/fileChange/requestApproval",
            "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1},
        }));
        let request = ControlRequest {
            task_id: None,
            thread_id: "thr".to_string(),
            request_id: json!(42),
            approve: true,
            start_task: None,
            completion: Arc::new(ControlCompletion::new()),
        };
        let mut fake = FakeTransport::replying([]);

        respond_to_approval(&mut fake, &snapshot, &request)
            .unwrap()
            .unwrap();

        assert_eq!(
            serde_json::from_str::<Value>(&fake.sent[0]).unwrap(),
            json!({"jsonrpc": "2.0", "id": 42, "result": {"decision": "accept"}})
        );
        assert!(!snapshot.read().unwrap().can_resolve_id("thr", &json!(42)));
        assert!(snapshot.read().unwrap().can_resolve_id("thr", &json!("42")));
    }

    #[test]
    fn permission_approval_responses_send_permissions_not_a_decision() {
        let permission_request = |id: &str, approve: bool| {
            let snapshot = snapshot();
            snapshot.write().unwrap().apply_notification(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "item/permissions/requestApproval",
                "params": {
                    "threadId": "thr",
                    "itemId": "item",
                    "turnId": "turn",
                    "startedAtMs": 1,
                    "cwd": "/work/neko",
                    "permissions": {"network": {"enabled": true}},
                },
            }));
            let request = ControlRequest {
                task_id: None,
                thread_id: "thr".to_string(),
                request_id: json!(id),
                approve,
                start_task: None,
                completion: Arc::new(ControlCompletion::new()),
            };
            let mut fake = FakeTransport::replying([]);
            respond_to_approval(&mut fake, &snapshot, &request)
                .unwrap()
                .unwrap();
            serde_json::from_str::<Value>(&fake.sent[0]).unwrap()
        };

        assert_eq!(
            permission_request("permission-accept", true),
            json!({
                "jsonrpc": "2.0",
                "id": "permission-accept",
                "result": {"permissions": {"network": {"enabled": true}}, "scope": "turn"},
            })
        );
        assert_eq!(
            permission_request("permission-decline", false),
            json!({
                "jsonrpc": "2.0",
                "id": "permission-decline",
                "result": {"permissions": {}, "scope": "turn"},
            })
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

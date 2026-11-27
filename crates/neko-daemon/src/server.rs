//! The daemon's life: claim the folder, open the store, listen on loopback, serve each screen on its own thread, push changes,
//! and shut down cleanly on a signal.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use neko_core::runtime::Runtime;
use neko_core::{Core, NewTaskError, RandomIds, SystemClock};
use neko_protocol::pump::{self, Flow, Out, Wake};
use neko_protocol::{Change, ClientMessage, DaemonInfo, ErrorCode, NeedsItem, PROTOCOL_VERSION, Reply, Request, ServerMessage, TaskId, TaskStatus, files};
use tungstenite::WebSocket;
use tungstenite::protocol::Role;

use crate::runs::{self, Runs};
use crate::{files as dfiles, http};

pub enum StartError {
    /// Another daemon holds the folder's lock.
    InUse,
    Other(String),
}

impl<E: std::fmt::Display> From<E> for StartError {
    fn from(e: E) -> Self {
        StartError::Other(e.to_string())
    }
}

/// One connected screen.
struct Conn {
    tx: Sender<Out>,
    wake: Wake,
    subscribed: bool,
    /// The task whose words this screen wants as they stream.
    watching: Option<TaskId>,
}

/// What every connection's thread, and every run's, shares.
pub struct Shared {
    pub core: Mutex<Core>,
    conns: Mutex<HashMap<u64, Conn>>,
    token: String,
    pub port: u16,
    pub stopping: AtomicBool,
    pub runs: Runs,
    /// "Needs you": the list screens were last told, and whether a change may have altered it.
    needs: NeedsWatch,
    /// The rounds scheduler's clock and thread (docs/knowledge/rounds.md).
    pub rounds: crate::rounds::Hub,
    /// The briefing screens were last told (docs/knowledge/briefing.md).
    pub briefing_last: Mutex<Option<neko_protocol::Briefing>>,
    /// Plugins: where their secrets are kept and which are being checked (docs/knowledge/plugins.md).
    pub plugins: crate::plugins::Hub,
    /// Subagents: the limits, the queue and the wakes (docs/knowledge/subagents.md).
    pub subagents: crate::subagents::Hub,
}

/// Keeps "Needs you" (docs/knowledge/needs-you.md) pushed: a change to a task's state, a request or a task's removal marks it, and
/// its own thread derives the list again from the store and pushes it whole when it differs.
struct NeedsWatch {
    dirty: Mutex<bool>,
    wake: Condvar,
    last: Mutex<Vec<NeedsItem>>,
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    /// Sends `msg` to one connection.
    pub fn send_to(&self, id: u64, msg: &ServerMessage) {
        if let Some(c) = lock(&self.conns).get(&id) {
            let _ = c.tx.send(Out::Text(msg.to_json()));
            c.wake.wake();
        }
    }

    /// Whether any screen is subscribed (the folder watcher runs only then).
    pub fn screens(&self) -> bool {
        lock(&self.conns).values().any(|c| c.subscribed)
    }

    /// Pushes a change to every subscribed connection. Called with the core locked, so events leave in revision order.
    pub fn broadcast(&self, msg: &ServerMessage) {
        if let ServerMessage::Event { change: Change::TaskUpdated { task }, .. } = msg
            && task.round.is_some()
        {
            // A round's task changed: the scheduler looks at how it ended (and at the three-failures brake).
            self.rounds.poke();
        }
        if let ServerMessage::Event {
            change:
                Change::TaskCreated { .. }
                | Change::TaskUpdated { .. }
                | Change::PermissionChanged { .. }
                | Change::TaskRemoved { .. }
                | Change::CatUpdated { .. }
                | Change::RunChanged { .. }
                | Change::RoundUpdated { .. }
                | Change::RoundRemoved { .. }
                | Change::BriefingSettingsChanged { .. }
                | Change::PluginAdded { .. }
                | Change::PluginUpdated { .. }
                | Change::PluginRemoved { .. },
            ..
        } = msg
        {
            *lock(&self.needs.dirty) = true;
            self.needs.wake.notify_one();
        }
        let text = msg.to_json();
        for c in lock(&self.conns).values().filter(|c| c.subscribed) {
            let _ = c.tx.send(Out::Text(text.clone()));
            c.wake.wake();
        }
    }

    /// Pushes streaming words to the subscribed connections watching `task`.
    pub fn stream(&self, task: &TaskId, msg: &ServerMessage) {
        let mut text = None;
        for c in lock(&self.conns).values().filter(|c| c.subscribed && c.watching.as_ref() == Some(task)) {
            let t = text.get_or_insert_with(|| msg.to_json());
            let _ = c.tx.send(Out::Text(t.clone()));
            c.wake.wake();
        }
    }
}

/// The "Needs you" thread: after each marked change (a few at once are taken together), the list again, pushed when it changed.
fn watch_needs(shared: Arc<Shared>) {
    loop {
        {
            let mut dirty = lock(&shared.needs.dirty);
            while !*dirty {
                dirty = shared.needs.wake.wait(dirty).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
        *lock(&shared.needs.dirty) = false;
        // The core stays locked while the list is pushed, so it leaves in order with the changes around it.
        let core = lock(&shared.core);
        if let Ok(items) = neko_core::needs::derive(&core.store) {
            let mut last = lock(&shared.needs.last);
            if *last != items {
                *last = items.clone();
                let revision = core.store.revision().unwrap_or(0);
                shared.broadcast(&ServerMessage::Event { revision, change: Change::NeedsChanged { items } });
            }
        }
        // The briefing is made of the same records (and more), so it is looked at after the same changes.
        crate::briefing::push_if_changed(&shared, &core);
    }
}

/// Blocks SIGINT, SIGTERM and SIGHUP in this thread (and every thread it starts), and returns the set for `sigwait`.
fn block_signals() -> libc::sigset_t {
    // SAFETY: a zeroed sigset_t is filled by sigemptyset before use; pthread_sigmask only changes this thread's mask.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for s in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::sigaddset(&mut set, s);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        set
    }
}

/// Runs the daemon for `dir` until a signal asks it to stop.
pub fn run(dir: &Path, port: u16) -> Result<(), StartError> {
    let signals = block_signals();
    dfiles::ensure_dir(dir)?;
    let lock_file = OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(dir.join(files::LOCK))?;
    match lock_file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Err(StartError::InUse),
        Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
    }
    let token = dfiles::token(dir)?;
    // A run's plugin config a killed daemon left behind goes (this daemon holds the folder's lock).
    crate::plugins::sweep(dir);
    let mut core = Core::open(&dir.join(files::STORE), Box::new(SystemClock), Box::new(RandomIds))?;
    // Runs the last daemon left running were cut off: they are interrupted, and say so.
    runs::recover(&mut core);
    // Subagents the last daemon cut off leave their reports, so their threads are woken (docs/knowledge/subagents.md).
    crate::subagents::recover(&mut core);
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    let port = listener.local_addr()?.port();
    // The run guard is this binary (`--neko-run-guard`). `NEKO_CLAUDE_BIN` points at another `claude` (tests use the fake).
    let guard = std::env::current_exe()?;
    let claude_bin = std::env::var_os("NEKO_CLAUDE_BIN").filter(|b| !b.is_empty()).map(std::path::PathBuf::from);
    let runtimes: Vec<Arc<dyn Runtime>> = vec![Arc::new(neko_core::claude::ClaudeRuntime::new(claude_bin, guard))];
    let runs = Runs::new(runtimes, dir.to_path_buf());
    let first = neko_core::needs::derive(&core.store).unwrap_or_default();
    let needs = NeedsWatch { dirty: Mutex::new(false), wake: Condvar::new(), last: Mutex::new(first) };
    let shared = Arc::new(Shared { core: Mutex::new(core), conns: Mutex::new(HashMap::new()), token, port, stopping: AtomicBool::new(false), runs, needs, rounds: crate::rounds::Hub::new(), briefing_last: Mutex::new(None), plugins: crate::plugins::Hub::new(), subagents: crate::subagents::Hub::new() });
    let for_needs = shared.clone();
    std::thread::Builder::new().name("neko-needs".into()).spawn(move || watch_needs(for_needs))?;
    crate::folders::spawn(shared.clone())?;
    crate::rounds::spawn(shared.clone())?;
    crate::plugins::spawn(shared.clone())?;
    crate::subagents::spawn(shared.clone())?;
    // Find the agent CLIs once now, off the main thread, so the first task and Settings do not wait for it.
    let warm = shared.clone();
    let _ = std::thread::Builder::new().name("neko-detect".into()).spawn(move || {
        warm.runs.detections(Duration::from_secs(0));
    });
    dfiles::write_info(dir, &DaemonInfo { pid: std::process::id(), port, protocol: PROTOCOL_VERSION, version: env!("CARGO_PKG_VERSION").to_owned() })?;
    eprintln!("neko-daemon {}: listening on 127.0.0.1:{port} for {}", env!("CARGO_PKG_VERSION"), dir.display());

    // The signal thread: the first signal stops the daemon.
    let for_signals = shared.clone();
    std::thread::Builder::new().name("neko-signals".into()).spawn(move || {
        let mut got = 0;
        // SAFETY: waits on the set blocked above; `got` receives the signal number.
        unsafe { libc::sigwait(&signals, &mut got) };
        stop(&for_signals);
    })?;

    let ids = AtomicU64::new(1);
    let mut workers = Vec::new();
    for stream in listener.incoming() {
        if shared.stopping.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else { continue };
        let id = ids.fetch_add(1, Ordering::Relaxed);
        let shared = shared.clone();
        if let Ok(h) = std::thread::Builder::new().name(format!("neko-conn-{id}")).spawn(move || serve(stream, id, &shared)) {
            workers.push(h);
        }
        workers.retain(|h| !h.is_finished());
    }
    // Each connection ends when its socket closes, but one may be busy with something that never ends (a file read macOS holds
    // for its privacy prompt, for one): wait a moment for them, then go on; leaving `main` ends whatever is left.
    let until = std::time::Instant::now() + Duration::from_secs(1);
    while workers.iter().any(|h| !h.is_finished()) && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    for h in workers.into_iter().filter(|h| h.is_finished()) {
        let _ = h.join();
    }
    // A run that is finishing writes its last rows now: wait a moment for it (a long run goes on; a restart marks it interrupted).
    let until = std::time::Instant::now() + Duration::from_secs(2);
    while shared.runs.threads.load(Ordering::SeqCst) > 0 && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    // Runs still marked running were cut off by this stop: interrupted, with nothing lost.
    crate::runs::interrupted_at_quit(&mut lock(&shared.core));
    if let Err(e) = lock(&shared.core).store.checkpoint() {
        eprintln!("neko-daemon: checkpoint at shutdown failed: {e}");
    }
    dfiles::remove_info(dir);
    eprintln!("neko-daemon: stopped");
    drop(lock_file);
    Ok(())
}

/// Closes every connection and wakes the accept loop so it can end.
fn stop(shared: &Shared) {
    shared.stopping.store(true, Ordering::SeqCst);
    // Pending requests never hold the stop up: runs end through their guards and write their words (docs/knowledge/run-guard.md).
    crate::runs::stop_all(shared);
    crate::folders::stopping();
    crate::rounds::stopping(shared);
    crate::plugins::stopping();
    crate::subagents::stopping(shared);
    *lock(&shared.needs.dirty) = true;
    shared.needs.wake.notify_one();
    for c in lock(&shared.conns).values() {
        let _ = c.tx.send(Out::Close);
        c.wake.wake();
    }
    let _ = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, shared.port)), Duration::from_secs(1));
}

/// One connection: its request head, then the hello answer or a WebSocket session.
fn serve(mut stream: TcpStream, id: u64, shared: &Arc<Shared>) {
    if shared.stopping.load(Ordering::SeqCst) {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_nodelay(true);
    let Ok(head) = http::read_head(&mut stream) else { return };
    match http::check(&head, shared.port, &shared.token) {
        Err(why) => {
            let _ = http::refuse(&mut stream, &why);
        }
        Ok(http::Asked::Hello) => {
            let _ = http::hello(&mut stream);
        }
        Ok(http::Asked::Hook { secret, length }) => crate::hook::serve(shared, stream, &secret, length),
        Ok(http::Asked::Mcp { secret, length }) => serve_mcp(shared, stream, &secret, length),
        Ok(http::Asked::Upgrade(key)) => {
            if http::accept_upgrade(&mut stream, &key).is_ok() && stream.set_read_timeout(None).is_ok() && stream.set_nonblocking(true).is_ok() {
                session(stream, id, shared);
            }
        }
    }
}

/// One call from a run's own MCP server (`POST /v1/mcp`): the run is found by its secret, the tool answers at once.
fn serve_mcp(shared: &Arc<Shared>, mut stream: TcpStream, secret: &str, length: usize) {
    use std::io::Read;
    let mut body = vec![0u8; length];
    if stream.read_exact(&mut body).is_err() {
        return;
    }
    let Some(run) = runs::by_mcp_secret(shared, secret) else {
        let _ = http::refuse(&mut stream, &http::Refused::Unauthorized);
        return;
    };
    let reply = match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(v) => crate::subagents::handle(shared, &run, &v),
        Err(_) => serde_json::json!({"text": "Neko could not read that call.", "error": true}),
    };
    let _ = http::respond(&mut stream, "200 OK", "application/json", &reply.to_string());
}

fn session(stream: TcpStream, id: u64, shared: &Arc<Shared>) {
    let Ok((wake, wake_rx)) = pump::wake_pair() else { return };
    let (tx, rx) = mpsc::channel();
    lock(&shared.conns).insert(id, Conn { tx: tx.clone(), wake: wake.clone(), subscribed: false, watching: None });
    if shared.stopping.load(Ordering::SeqCst) {
        let _ = tx.send(Out::Close);
    }
    let fd = stream.as_raw_fd();
    let mut ws = WebSocket::from_raw_socket(stream, Role::Server, None);
    let mut said_hello = false;
    let _ = pump::run(&mut ws, fd, &wake_rx, &rx, |text| {
        // A transcript is sent from inside the run's lock, so no streamed word falls between it and the words after it.
        if said_hello
            && let Ok(ClientMessage { id: reply_id, request: Request::GetTranscript { task } }) = serde_json::from_str::<ClientMessage>(&text)
        {
            crate::runs::send_transcript(shared, id, reply_id, &task);
            return Flow::Continue;
        }
        // A plugin request that may wait on the Keychain, a folder or a server is answered on a thread of its own, so this connection (and
        // a stop) never waits for it.
        if said_hello
            && let Ok(ClientMessage { id: reply_id, request }) = serde_json::from_str::<ClientMessage>(&text)
            && crate::plugins::is_slow(&request)
        {
            crate::plugins::spawn_slow(shared, id, reply_id, request);
            return Flow::Continue;
        }
        let (reply_id, reply, flow) = answer(&text, id, &mut said_hello, shared);
        shared.send_to(id, &ServerMessage::Reply { id: reply_id, reply });
        flow
    });
    lock(&shared.conns).remove(&id);
}

/// A refused change to a workspace, as a reply.
fn workspace_error(e: neko_core::WorkspaceError) -> Reply {
    use neko_core::WorkspaceError as W;
    let code = match &e {
        W::NotFound => ErrorCode::NotFound,
        W::Folder(_) => ErrorCode::InvalidFolder,
        W::Refused(_) => ErrorCode::Refused,
        W::Store(_) => ErrorCode::Internal,
    };
    Reply::error(code, e.to_string())
}

/// A workspace changed: every screen hears it, and the asker gets the row. Called with the core locked, so events leave in order.
fn one_workspace(shared: &Shared, done: Result<(neko_protocol::Workspace, u64), neko_core::WorkspaceError>) -> Reply {
    match done {
        Ok((workspace, revision)) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::WorkspaceUpdated { workspace: workspace.clone() } });
            Reply::Workspace { workspace, existed: false }
        }
        Err(e) => workspace_error(e),
    }
}

/// A refused change to a cat, as a reply.
fn cat_error(e: neko_core::CatError) -> Reply {
    use neko_core::CatError as C;
    let code = match &e {
        C::NotFound => ErrorCode::NotFound,
        C::InvalidName(_) => ErrorCode::InvalidName,
        C::Refused(_) => ErrorCode::Refused,
        C::Store(_) => ErrorCode::Internal,
    };
    Reply::error(code, e.to_string())
}

/// A cat changed: every screen hears it whole (and "Needs you" again, since its items carry the cat's name), and the asker gets
/// it. The core stays locked while the event leaves, so events leave in order.
fn one_cat(shared: &Shared, change: impl FnOnce(&mut Core) -> Result<(neko_protocol::Cat, u64), neko_core::CatError>) -> Reply {
    let mut core = lock(&shared.core);
    match change(&mut core) {
        Ok((cat, revision)) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::CatUpdated { cat: cat.clone() } });
            Reply::Cat { cat }
        }
        Err(e) => cat_error(e),
    }
}

/// The person opened a thread (`read`), or asked for it to be unread: the task, whole, to every screen, when that changed anything.
fn mark(shared: &Shared, task: &neko_protocol::TaskId, read: bool) -> Reply {
    let internal = |e: neko_core::StoreError| Reply::error(ErrorCode::Internal, e.to_string());
    let mut core = lock(&shared.core);
    match core.store.get_task(task) {
        Ok(Some(t)) if t.unread() != read => Reply::Task { task: t },
        Ok(Some(t)) => {
            let now = core.now();
            match core.store.set_read(&t.id, read, now) {
                Ok((task, revision)) => {
                    shared.broadcast(&ServerMessage::Event { revision, change: Change::TaskUpdated { task: task.clone() } });
                    Reply::Task { task }
                }
                Err(e) => internal(e),
            }
        }
        Ok(None) => Reply::error(ErrorCode::NotFound, "There is no such thread."),
        Err(e) => internal(e),
    }
}

/// The reply to one message, the id it answers, and whether the connection goes on.
fn answer(text: &str, conn: u64, said_hello: &mut bool, shared: &Arc<Shared>) -> (u64, Reply, Flow) {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            let id = serde_json::from_str::<serde_json::Value>(text).ok().and_then(|v| v.get("id").and_then(serde_json::Value::as_u64)).unwrap_or(0);
            return (id, Reply::error(ErrorCode::BadRequest, format!("not a request this daemon knows: {e}")), Flow::Continue);
        }
    };
    let id = msg.id;
    let internal = |e: neko_core::StoreError| Reply::error(ErrorCode::Internal, e.to_string());
    let reply = match msg.request {
        Request::Hello { protocol, .. } if protocol != PROTOCOL_VERSION => {
            let words = format!("This screen speaks protocol {protocol}; the daemon speaks {PROTOCOL_VERSION}. Update Neko.");
            return (id, Reply::error(ErrorCode::ProtocolMismatch, words), Flow::Stop);
        }
        Request::Hello { .. } => {
            *said_hello = true;
            Reply::Hello { daemon: env!("CARGO_PKG_VERSION").to_owned(), protocol: PROTOCOL_VERSION, workspace: neko_core::DEFAULT_WORKSPACE.to_owned() }
        }
        _ if !*said_hello => Reply::error(ErrorCode::HelloFirst, "Say hello first."),
        Request::ListTasks => match lock(&shared.core).store.all_tasks() {
            Ok(tasks) => Reply::Tasks { tasks },
            Err(e) => internal(e),
        },
        Request::GetTask { task } => match lock(&shared.core).store.get_task(&task) {
            Ok(Some(task)) => Reply::Task { task },
            Ok(None) => Reply::error(ErrorCode::NotFound, "There is no such thread."),
            Err(e) => internal(e),
        },
        Request::CreateTask { words, workspace, cat } => {
            let made = {
                let mut core = lock(&shared.core);
                let made = core.create_task_for(&words, workspace.as_deref(), cat.as_deref());
                if let Ok((task, revision)) = &made {
                    shared.broadcast(&ServerMessage::Event { revision: *revision, change: Change::TaskCreated { task: task.clone() } });
                }
                made
            };
            match made {
                Ok((task, _)) => {
                    // The agent starts by itself; how it goes arrives as events.
                    let _ = runs::start(shared, task.clone(), None);
                    Reply::Task { task }
                }
                Err(e @ (NewTaskError::Empty | NewTaskError::TooLong)) => Reply::error(ErrorCode::InvalidWords, e.to_string()),
                Err(e @ NewTaskError::NoWorkspace) => Reply::error(ErrorCode::NotFound, e.to_string()),
                Err(e @ (NewTaskError::Removed(_) | NewTaskError::FolderMissing(_))) => Reply::error(ErrorCode::Refused, e.to_string()),
                Err(NewTaskError::Cat(e)) => cat_error(e),
                Err(e) => Reply::error(ErrorCode::Internal, e.to_string()),
            }
        }
        Request::RemoveTask { task } => runs::remove_task(shared, &task),
        Request::SetTaskCat { task, cat } => {
            if lock(&shared.runs.active).contains_key(&task) {
                return (id, Reply::error(ErrorCode::Refused, "This thread has started, so its cat stays."), Flow::Continue);
            }
            let mut core = lock(&shared.core);
            match core.set_task_cat(&task, &cat) {
                Ok((task, revision)) => {
                    shared.broadcast(&ServerMessage::Event { revision, change: Change::TaskUpdated { task: task.clone() } });
                    Reply::Task { task }
                }
                Err(e) => cat_error(e),
            }
        }
        Request::ListCats => match lock(&shared.core).cats() {
            Ok(cats) => Reply::Cats { cats },
            Err(e) => internal(e),
        },
        Request::CreateCat { name, face, purpose, workspace } => {
            let mut core = lock(&shared.core);
            match core.create_cat(&name, face.as_deref(), purpose.as_deref(), workspace.as_deref()) {
                Ok((cat, revision)) => {
                    shared.broadcast(&ServerMessage::Event { revision, change: Change::CatAdded { cat: cat.clone() } });
                    Reply::Cat { cat }
                }
                Err(e) => cat_error(e),
            }
        }
        Request::RenameCat { cat, name } => one_cat(shared, |c| c.rename_cat(&cat, &name)),
        Request::SetCatFace { cat, face } => one_cat(shared, |c| c.set_cat_face(&cat, &face)),
        Request::SetCatPurpose { cat, purpose } => one_cat(shared, |c| c.set_cat_purpose(&cat, &purpose)),
        Request::SetCatWorkspace { cat, workspace } => one_cat(shared, |c| c.set_cat_workspace(&cat, workspace.as_deref())),
        Request::SaveCatRules { cat, rules } => one_cat(shared, |c| c.save_cat_rules(&cat, &rules)),
        Request::RestoreCatRules { cat } => one_cat(shared, |c| c.restore_cat_rules(&cat)),
        Request::RetireCat { cat } => one_cat(shared, |c| c.retire_cat(&cat)),
        Request::BringBackCat { cat } => one_cat(shared, |c| c.bring_back_cat(&cat)),
        Request::ListRounds => crate::rounds::list(shared),
        Request::CreateRound { name, words, cat, workspace, schedule, on } => crate::rounds::create(shared, &name, &words, &cat, workspace.as_deref(), &schedule, on),
        Request::UpdateRound { round, name, words, cat, workspace, schedule } => crate::rounds::update(shared, &round, &name, &words, &cat, workspace.as_deref(), &schedule),
        Request::SetRoundOn { round, on } => crate::rounds::set_on(shared, &round, on),
        Request::RemoveRound { round } => crate::rounds::remove(shared, &round),
        Request::RunRoundNow { round } => crate::rounds::run_now(shared, &round),
        Request::SetRoundsPaused { paused } => crate::rounds::set_paused(shared, paused),
        Request::RoundHistory { round } => crate::rounds::history(shared, &round),
        Request::DebugClock { offset_ms } => crate::rounds::debug_clock(shared, offset_ms),
        Request::ListWorkspaces => match lock(&shared.core).workspaces() {
            Ok(workspaces) => Reply::Workspaces { workspaces },
            Err(e) => internal(e),
        },
        Request::AddWorkspace { folder } => {
            let mut core = lock(&shared.core);
            match core.add_workspace(&folder) {
                Ok(added) => {
                    if let Some(revision) = added.revision {
                        let change = if added.restored { Change::WorkspaceUpdated { workspace: added.workspace.clone() } } else { Change::WorkspaceAdded { workspace: added.workspace.clone() } };
                        shared.broadcast(&ServerMessage::Event { revision, change });
                    }
                    Reply::Workspace { workspace: added.workspace, existed: added.existed }
                }
                Err(e) => workspace_error(e),
            }
        }
        Request::RenameWorkspace { workspace, name } => {
            let mut core = lock(&shared.core);
            let done = core.rename_workspace(&workspace, &name);
            one_workspace(shared, done)
        }
        Request::RemoveWorkspace { workspace } => {
            let mut core = lock(&shared.core);
            let done = core.remove_workspace(&workspace);
            one_workspace(shared, done)
        }
        Request::LocateWorkspace { workspace, folder } => {
            let mut core = lock(&shared.core);
            let done = core.locate_workspace(&workspace, &folder);
            if let Ok((w, _)) = &done
                && let Some(f) = &w.folder
            {
                // Worktrees of its tasks find the moved repository again.
                let trees: Vec<std::path::PathBuf> = core.store.list_tasks(&workspace).unwrap_or_default().iter().map(|t| neko_core::workplace::worktree_root(&shared.runs.data_dir, &t.id)).collect();
                neko_core::workplace::repair(std::path::Path::new(f), &trees);
            }
            if done.is_ok() {
                // Its tasks' folder followed it: screens get them again.
                if let Ok(tasks) = core.store.list_tasks(&workspace) {
                    let revision = core.store.revision().unwrap_or(0);
                    for task in tasks {
                        shared.broadcast(&ServerMessage::Event { revision, change: Change::TaskUpdated { task } });
                    }
                }
            }
            one_workspace(shared, done)
        }
        Request::MoveWorkspace { workspace, to } => {
            let mut core = lock(&shared.core);
            match core.move_workspace(&workspace, to as usize) {
                Ok((changed, revision)) => {
                    for workspace in changed {
                        shared.broadcast(&ServerMessage::Event { revision, change: Change::WorkspaceUpdated { workspace } });
                    }
                    Reply::Ok
                }
                Err(e) => workspace_error(e),
            }
        }
        Request::GetTranscript { task } => {
            let core = lock(&shared.core);
            let s = &core.store;
            match (s.get_task(&task), s.runs_of(&task), s.parts_of(&task), s.permissions_of(&task)) {
                (Ok(Some(task)), Ok(runs), Ok(parts), Ok(permissions)) => Reply::Transcript { transcript: neko_protocol::Transcript { task, runs, parts, permissions } },
                (Ok(None), ..) => Reply::error(ErrorCode::NotFound, "There is no such thread."),
                (Err(e), ..) | (_, Err(e), ..) | (_, _, Err(e), _) | (.., Err(e)) => internal(e),
            }
        }
        Request::FollowUp { task, words, answering } => {
            if words.len() > neko_protocol::MAX_WORDS_BYTES {
                return (id, Reply::error(ErrorCode::InvalidWords, "That is too long to send. Put the long part in a file."), Flow::Continue);
            }
            let found = lock(&shared.core).store.get_task(&task);
            match found {
                // A subagent takes no messages yet: it is read, not talked to (docs/knowledge/subagents.md).
                Ok(Some(t)) if t.subagent.is_some() => Reply::error(ErrorCode::Refused, "A subagent takes no messages yet. Say it in the thread."),
                Ok(Some(t)) if answering.is_some() => {
                    // An answer to a question: only while the task still waits on that run's question (another screen, or a
                    // notification, may have answered it).
                    let newest = lock(&shared.core).store.runs_of(&t.id).ok().and_then(|mut r| r.pop()).map(|r| r.id);
                    if t.status != TaskStatus::Asked || newest != answering {
                        Reply::error(ErrorCode::AlreadyAnswered, "That was already answered.")
                    } else if words.trim().is_empty() {
                        Reply::error(ErrorCode::InvalidWords, "Type an answer first.")
                    } else {
                        match runs::start(shared, t, Some(words)) {
                            Ok(()) => Reply::Ok,
                            // Another answer started a run first.
                            Err(Reply::Error { code: ErrorCode::Busy, .. }) => Reply::error(ErrorCode::AlreadyAnswered, "That was already answered."),
                            Err(r) => r,
                        }
                    }
                }
                Ok(Some(t)) => runs::follow_up(shared, t, words),
                Ok(None) => Reply::error(ErrorCode::NotFound, "There is no such thread."),
                Err(e) => internal(e),
            }
        }
        Request::StopTask { task } => runs::stop(shared, &task),
        Request::MarkRead { task } => mark(shared, &task, true),
        Request::MarkUnread { task } => mark(shared, &task, false),
        Request::AnswerPermission { permission, decision } => runs::answer(shared, &permission, decision),
        Request::WatchTask { task } => {
            if let Some(c) = lock(&shared.conns).get_mut(&conn) {
                c.watching = task;
            }
            Reply::Ok
        }
        Request::ListRuntimes => {
            // The newest task's chosen folder: its own Claude Code settings count in the line Settings shows.
            let folder = lock(&shared.core).store.all_tasks().ok().and_then(|t| t.into_iter().find_map(|t| t.folder));
            let runtimes = shared.runs.infos(Duration::from_secs(10), folder.as_deref().map(std::path::Path::new));
            match lock(&shared.core).store.settings() {
                Ok(settings) => Reply::Runtimes { runtimes, settings },
                Err(e) => internal(e),
            }
        }
        Request::SetSettings { settings } => {
            let known = shared.runs.runtimes.iter().any(|r| r.id() == settings.runtime);
            if !known {
                return (id, Reply::error(ErrorCode::BadRequest, format!("Neko cannot run {} yet.", settings.runtime)), Flow::Continue);
            }
            let mut core = lock(&shared.core);
            let now = core.now();
            match core.store.set_settings(&settings, now) {
                Ok(revision) => {
                    shared.broadcast(&ServerMessage::Event { revision, change: Change::SettingsChanged { settings: settings.clone() } });
                    Reply::Settings { settings }
                }
                Err(e) => internal(e),
            }
        }
        Request::ListNeeds => match neko_core::needs::derive(&lock(&shared.core).store) {
            Ok(items) => Reply::Needs { items },
            Err(e) => internal(e),
        },
        r @ (Request::ListPlugins
        | Request::UpdatePluginManifest { .. }
        | Request::ApprovePlugin { .. }
        | Request::DeclinePlugin { .. }
        | Request::SetPluginEnabled { .. }
        | Request::SetPluginAccess { .. }) => crate::plugins::quick(shared, &r).unwrap_or_else(|| Reply::error(ErrorCode::BadRequest, "Not a plugin request.")),
        // Answered on their own threads (`session`); never reached here.
        Request::AddMcpJson { .. } | Request::AddPluginFolder { .. } | Request::SetPluginSetting { .. } | Request::TestPlugin { .. } | Request::RemovePlugin { .. } => Reply::error(ErrorCode::Internal, "A plugin request reached the wrong place."),
        Request::GetBriefing { from_ms, to_ms } => crate::briefing::get(shared, from_ms, to_ms),
        Request::MarkBriefingRead => crate::briefing::mark_read(shared),
        Request::GetBriefingSettings => crate::briefing::settings(shared),
        Request::SetBriefingSettings { settings } => crate::briefing::set_settings(shared, settings),
        Request::GetDiff { task, run } => crate::undo::diff(shared, &task, run),
        Request::GetDiffFile { task, run, path, full } => crate::undo::diff_file(shared, &task, run, &path, full),
        Request::UndoRun { task, run, anyway } => crate::undo::undo_run(shared, &task, &run, anyway, false),
        Request::RedoRun { task, run, anyway } => crate::undo::undo_run(shared, &task, &run, anyway, true),
        Request::UndoStorage => crate::undo::storage(shared),
        Request::ClearUndo => crate::undo::clear(shared),
        Request::DayUsage { from_ms } => match lock(&shared.core).store.day_usage(from_ms) {
            Ok(usage) => Reply::DayUsage { usage },
            Err(e) => internal(e),
        },
        Request::Subscribe => {
            // The core stays locked while the connection is marked, so no change falls between the revision and the first event.
            let core = lock(&shared.core);
            match core.store.revision() {
                Ok(revision) => {
                    if let Some(c) = lock(&shared.conns).get_mut(&conn) {
                        c.subscribed = true;
                    }
                    crate::folders::screen_subscribed();
                    // A morning notification that waited for a screen is due now.
                    shared.rounds.poke();
                    crate::plugins::poke();
                    Reply::Subscribed { revision }
                }
                Err(e) => internal(e),
            }
        }
    };
    (id, reply, Flow::Continue)
}

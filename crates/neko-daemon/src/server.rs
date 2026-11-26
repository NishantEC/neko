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
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use neko_core::{Core, NewTaskError, RandomIds, SystemClock};
use neko_protocol::pump::{self, Flow, Out, Wake};
use neko_protocol::{Change, ClientMessage, DaemonInfo, ErrorCode, PROTOCOL_VERSION, Reply, Request, ServerMessage, files};
use tungstenite::WebSocket;
use tungstenite::protocol::Role;

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
}

/// What every connection's thread shares.
struct Shared {
    core: Mutex<Core>,
    conns: Mutex<HashMap<u64, Conn>>,
    token: String,
    port: u16,
    stopping: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    /// Sends `msg` to one connection.
    fn send_to(&self, id: u64, msg: &ServerMessage) {
        if let Some(c) = lock(&self.conns).get(&id) {
            let _ = c.tx.send(Out::Text(msg.to_json()));
            c.wake.wake();
        }
    }

    /// Pushes a change to every subscribed connection. Called with the core locked, so events leave in revision order.
    fn broadcast(&self, msg: &ServerMessage) {
        let text = msg.to_json();
        for c in lock(&self.conns).values().filter(|c| c.subscribed) {
            let _ = c.tx.send(Out::Text(text.clone()));
            c.wake.wake();
        }
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
    let core = Core::open(&dir.join(files::STORE), Box::new(SystemClock), Box::new(RandomIds))?;
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    let port = listener.local_addr()?.port();
    let shared = Arc::new(Shared { core: Mutex::new(core), conns: Mutex::new(HashMap::new()), token, port, stopping: AtomicBool::new(false) });
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
    for h in workers {
        let _ = h.join();
    }
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
    for c in lock(&shared.conns).values() {
        let _ = c.tx.send(Out::Close);
        c.wake.wake();
    }
    let _ = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, shared.port)), Duration::from_secs(1));
}

/// One connection: its request head, then the hello answer or a WebSocket session.
fn serve(mut stream: TcpStream, id: u64, shared: &Shared) {
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
        Ok(http::Asked::Upgrade(key)) => {
            if http::accept_upgrade(&mut stream, &key).is_ok() && stream.set_read_timeout(None).is_ok() && stream.set_nonblocking(true).is_ok() {
                session(stream, id, shared);
            }
        }
    }
}

fn session(stream: TcpStream, id: u64, shared: &Shared) {
    let Ok((wake, wake_rx)) = pump::wake_pair() else { return };
    let (tx, rx) = mpsc::channel();
    lock(&shared.conns).insert(id, Conn { tx: tx.clone(), wake: wake.clone(), subscribed: false });
    if shared.stopping.load(Ordering::SeqCst) {
        let _ = tx.send(Out::Close);
    }
    let fd = stream.as_raw_fd();
    let mut ws = WebSocket::from_raw_socket(stream, Role::Server, None);
    let mut said_hello = false;
    let _ = pump::run(&mut ws, fd, &wake_rx, &rx, |text| {
        let (reply_id, reply, flow) = answer(&text, id, &mut said_hello, shared);
        shared.send_to(id, &ServerMessage::Reply { id: reply_id, reply });
        flow
    });
    lock(&shared.conns).remove(&id);
}

/// The reply to one message, the id it answers, and whether the connection goes on.
fn answer(text: &str, conn: u64, said_hello: &mut bool, shared: &Shared) -> (u64, Reply, Flow) {
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
        Request::ListTasks => match lock(&shared.core).store.list_tasks(neko_core::DEFAULT_WORKSPACE) {
            Ok(tasks) => Reply::Tasks { tasks },
            Err(e) => internal(e),
        },
        Request::GetTask { task } => match lock(&shared.core).store.get_task(&task) {
            Ok(Some(task)) => Reply::Task { task },
            Ok(None) => Reply::error(ErrorCode::NotFound, "There is no such task."),
            Err(e) => internal(e),
        },
        Request::CreateTask { words } => {
            let mut core = lock(&shared.core);
            match core.create_task(&words) {
                Ok((task, revision)) => {
                    shared.broadcast(&ServerMessage::Event { revision, change: Change::TaskCreated { task: task.clone() } });
                    Reply::Task { task }
                }
                Err(e @ (NewTaskError::Empty | NewTaskError::TooLong)) => Reply::error(ErrorCode::InvalidWords, e.to_string()),
                Err(e) => Reply::error(ErrorCode::Internal, e.to_string()),
            }
        }
        Request::Subscribe => {
            // The core stays locked while the connection is marked, so no change falls between the revision and the first event.
            let core = lock(&shared.core);
            match core.store.revision() {
                Ok(revision) => {
                    if let Some(c) = lock(&shared.conns).get_mut(&conn) {
                        c.subscribed = true;
                    }
                    Reply::Subscribed { revision }
                }
                Err(e) => internal(e),
            }
        }
    };
    (id, reply, Flow::Continue)
}

//! The app's link to the daemon (docs/knowledge/app-shell.md).
//!
//! One supervisor thread finds the daemon of the data folder (`daemon.json` and `token`), starts it beside the app when none is
//! running, connects, and reconnects whenever the connection ends (the daemon restarted, crashed or was updated). What arrives is
//! put in an inbox the window drains on its own thread; the window never blocks on the network.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use neko_protocol::client::{Link, LinkEvent, find};
use neko_protocol::{Reply, Request, ServerMessage};

/// What the window reads from the link.
#[derive(Debug)]
pub enum Incoming {
    /// Connected and greeted: the daemon's version.
    Connected(String),
    Message(ServerMessage),
    /// The connection ended or could not be made; the words say why. The link keeps trying.
    Lost(String),
}

/// The longest wait between two tries.
const MAX_DELAY: Duration = Duration::from_secs(2);
/// The least time between two daemons started by this link.
const START_GAP: Duration = Duration::from_secs(3);

struct Shared {
    inbox: Mutex<VecDeque<Incoming>>,
    link: Mutex<Option<Link>>,
    wake: Box<dyn Fn() + Send + Sync>,
    stop: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    fn push(&self, i: Incoming) {
        lock(&self.inbox).push_back(i);
        (self.wake)();
    }
}

/// The app's end of the link. Dropping it ends the link (the daemon keeps running).
pub struct Conn {
    shared: Arc<Shared>,
}

/// The daemon binary: `NEKO_DAEMON_BIN`, else `neko-daemon` beside the app's own executable (inside `Neko.app`, `Contents/MacOS`).
pub fn daemon_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NEKO_DAEMON_BIN").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let beside = std::env::current_exe().ok()?.parent()?.join("neko-daemon");
    beside.is_file().then_some(beside)
}

/// Starts the daemon for `dir`, detached from the app (its own process group, its output in `daemon.log`). A thread waits for it,
/// so it never lingers as a zombie if it ends while the app runs.
fn start_daemon(bin: &Path, dir: &Path) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    std::fs::create_dir_all(dir)?;
    let log = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("daemon.log"))?;
    let mut child = Command::new(bin).arg("--data-dir").arg(dir).stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log).process_group(0).spawn()?;
    std::thread::Builder::new().name("neko-daemon-reaper".into()).spawn(move || {
        let _ = child.wait();
    })?;
    Ok(())
}

impl Conn {
    /// Starts the link for the daemon of `dir`. `wake` is called (on the link's thread) whenever the inbox has something new.
    /// With `daemon` the link starts that binary when no daemon answers.
    pub fn start(dir: PathBuf, daemon: Option<PathBuf>, wake: impl Fn() + Send + Sync + 'static) -> Conn {
        let shared = Arc::new(Shared { inbox: Mutex::new(VecDeque::new()), link: Mutex::new(None), wake: Box::new(wake), stop: AtomicBool::new(false) });
        let s = shared.clone();
        let _ = std::thread::Builder::new().name("neko-link-supervisor".into()).spawn(move || supervise(&s, &dir, daemon.as_deref()));
        Conn { shared }
    }

    /// Sends a request now; None while not connected.
    pub fn send(&self, request: Request) -> Option<u64> {
        lock(&self.shared.link).as_ref().map(|l| l.send(request))
    }

    /// Everything that arrived since the last call.
    pub fn drain(&self) -> Vec<Incoming> {
        lock(&self.shared.inbox).drain(..).collect()
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(l) = lock(&self.shared.link).take() {
            l.close();
        }
    }
}

fn supervise(shared: &Arc<Shared>, dir: &Path, daemon: Option<&Path>) {
    let mut delay = Duration::from_millis(50);
    let mut started: Option<Instant> = None;
    let mut said_lost = false;
    while !shared.stop.load(Ordering::SeqCst) {
        let attempt = find(dir).and_then(|(info, token)| {
            let (closed_tx, closed_rx) = mpsc::channel::<String>();
            let to_inbox = shared.clone();
            let link = Link::open(info.port, &token, "neko-app", move |e| match e {
                LinkEvent::Message(m) => to_inbox.push(Incoming::Message(m)),
                LinkEvent::Closed(why) => {
                    let _ = closed_tx.send(why);
                }
            })?;
            Ok((link, closed_rx))
        });
        match attempt {
            Ok((link, closed)) => {
                let daemon_version = match &link.hello {
                    Reply::Hello { daemon, .. } => daemon.clone(),
                    _ => String::new(),
                };
                *lock(&shared.link) = Some(link);
                shared.push(Incoming::Connected(daemon_version));
                delay = Duration::from_millis(50);
                let why = closed.recv().unwrap_or_else(|_| "the connection ended".to_owned());
                lock(&shared.link).take();
                if shared.stop.load(Ordering::SeqCst) {
                    return;
                }
                shared.push(Incoming::Lost(why));
                said_lost = true;
            }
            Err(e) => {
                // No daemon answers: start one, at most every few seconds (one that is still starting is given time).
                if let Some(bin) = daemon
                    && started.is_none_or(|at| at.elapsed() >= START_GAP)
                {
                    started = Some(Instant::now());
                    if let Err(why) = start_daemon(bin, dir) {
                        shared.push(Incoming::Lost(format!("Neko's daemon could not be started: {why}")));
                        said_lost = true;
                    }
                } else if daemon.is_none() && !said_lost {
                    shared.push(Incoming::Lost(format!("Neko's daemon is not running ({e})")));
                    said_lost = true;
                }
            }
        }
        std::thread::sleep(delay);
        delay = (delay * 2).min(MAX_DELAY);
    }
}

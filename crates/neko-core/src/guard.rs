//! The run guard (docs/knowledge/run-guard.md): nothing an agent starts outlives its run.
//!
//! An agent CLI runs under a guardian: a small process (the daemon's own binary started with [`GUARD_ARG`]) that starts the CLI in
//! a process group of its own, records every descendant while it runs, and kills them all when the run ends, when it is stopped, or
//! when the daemon dies. The daemon holds one end of a socket pair; the guardian holds the other as fd 3. Closing it (a stop, or
//! the daemon's death, even by SIGKILL) tells the guardian to end the run.
//!
//! Why each piece, from the earlier project:
//! - Agent CLIs run each shell command in a process group of its own, so killing the CLI's group never reached a test runner it
//!   started (one ran for seven days). Descendants are recorded while their ancestry is intact, with their start time, so a pid
//!   reused by someone else is never touched.
//! - A process forking at the instant its group is signalled can escape one signal, so groups are signalled several times.
//! - The guardian catches SIGTERM, SIGINT and SIGHUP and cleans up first; dying on the default action left the child running.

use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The first argument that makes a binary act as a guardian: `<binary> --neko-run-guard <program> [args…]`.
pub const GUARD_ARG: &str = "--neko-run-guard";
/// The guardian's end of the socket.
const CONTROL_FD: RawFd = 3;
/// How often the guardian looks at the run (and records its descendants).
const LOOK_EVERY_MS: i32 = 100;
/// How many times a group is signalled, and how far apart.
const KILL_PASSES: u32 = 4;
const KILL_GAP: Duration = Duration::from_millis(15);
/// The most descendants recorded for one run.
const MAX_DESCENDANTS: usize = 4096;

/// Call first in `main`, before anything else starts: in a guardian it runs the guard and exits; otherwise it returns at once.
pub fn run_if_requested() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(GUARD_ARG)) {
        return;
    }
    let code = match args.next() {
        None => {
            eprintln!("Neko's run guard: no program to run");
            125
        }
        Some(program) => {
            let mut cmd = Command::new(program);
            cmd.args(args);
            match guard(cmd) {
                Ok(status) => status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(1)),
                Err(e) => {
                    eprintln!("Neko's run guard: {e}");
                    // The same code a shell gives a program it cannot find or run.
                    127
                }
            }
        }
    };
    std::process::exit(code);
}

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_stop_signal(_: libc::c_int) {
    STOP.store(true, Ordering::Release);
}

/// `(process group, start seconds, start microseconds)` of a live process, so a reused pid is never mistaken for it.
type Identity = (libc::pid_t, u64, u64);

fn identity(pid: libc::pid_t) -> Option<Identity> {
    // SAFETY: a zeroed proc_bsdinfo of exactly `size` bytes, filled by the kernel.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let filled = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    (filled == size).then_some((info.pbi_pgid as libc::pid_t, info.pbi_start_tvsec, info.pbi_start_tvusec))
}

fn children(parent: libc::pid_t) -> Vec<libc::pid_t> {
    let mut buf = vec![0 as libc::pid_t; 512];
    let bytes = (buf.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
    // SAFETY: the buffer holds `bytes` bytes; entries left zero are unused.
    let n = unsafe { libc::proc_listchildpids(parent, buf.as_mut_ptr().cast(), bytes) };
    if n <= 0 {
        return Vec::new();
    }
    buf.into_iter().filter(|p| *p > 0).collect()
}

/// Every process below `root`, recorded with its identity while the tree is intact.
#[derive(Default)]
pub struct Descendants {
    seen: HashMap<libc::pid_t, Identity>,
}

impl Descendants {
    /// Walks the tree under `root` and records what is there now.
    pub fn record(&mut self, root: libc::pid_t) {
        let mut queue = vec![root];
        while let Some(parent) = queue.pop() {
            for pid in children(parent) {
                if self.seen.len() >= MAX_DESCENDANTS {
                    return;
                }
                if let Some(id) = identity(pid) {
                    self.seen.insert(pid, id);
                    queue.push(pid);
                }
            }
        }
    }

    /// Kills every recorded process that is still the same process, and the groups they lead. True when none is left.
    pub fn kill(&self, own_group: libc::pid_t) -> bool {
        let ours: HashSet<libc::pid_t> = self.seen.keys().copied().collect();
        let mut left = false;
        for (&pid, &id) in &self.seen {
            if identity(pid) != Some(id) {
                continue;
            }
            left = true;
            // SAFETY: signals to a process verified just above, and to a group led by one of this run's own descendants.
            unsafe {
                if id.0 != own_group && ours.contains(&id.0) {
                    libc::kill(-id.0, libc::SIGKILL);
                }
                libc::kill(pid, libc::SIGKILL);
            }
        }
        !left
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// Signals the group `pgid` several times, until it is empty.
fn kill_group(pgid: libc::pid_t) {
    for pass in 0..KILL_PASSES {
        // SAFETY: a negative pid targets only the group the guardian made for its child.
        let sent = unsafe { libc::kill(-pgid, libc::SIGKILL) };
        if sent == -1 && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            break;
        }
        if pass + 1 < KILL_PASSES {
            std::thread::sleep(KILL_GAP);
        }
    }
}

/// Whether the child has exited, without reaping it (so its pid, the group's id, cannot be reused while the group is killed).
fn exited(pid: libc::pid_t) -> bool {
    // SAFETY: a zeroed siginfo, filled by waitid for our own child only.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
    r == 0 && unsafe { info.si_pid() } != 0
}

fn guard(mut cmd: Command) -> Result<ExitStatus, String> {
    // SAFETY: the guardian protocol hands over one socket at fd 3; it is checked before use.
    if unsafe { libc::fcntl(CONTROL_FD, libc::F_GETFD) } == -1 {
        return Err("started without its control socket".into());
    }
    let mut control = unsafe { UnixStream::from_raw_fd(CONTROL_FD) };
    control.peer_addr().map_err(|e| format!("fd 3 is not a socket: {e}"))?;
    // The agent must not inherit the control socket.
    unsafe { libc::fcntl(CONTROL_FD, libc::F_SETFD, libc::FD_CLOEXEC) };
    control.set_nonblocking(true).map_err(|e| e.to_string())?;
    for s in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
        unsafe { libc::signal(s, on_stop_signal as extern "C" fn(libc::c_int) as libc::sighandler_t) };
    }
    let parent_gone = |c: &mut UnixStream| match c.read(&mut [0u8; 16]) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => !matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted),
    };
    if parent_gone(&mut control) {
        return Err("the daemon ended before the run started".into());
    }
    cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit()).process_group(0);
    let mut child = cmd.spawn().map_err(|e| format!("cannot start {:?}: {e}", cmd.get_program()))?;
    let pid = child.id() as libc::pid_t;
    // SAFETY: getpgrp has no preconditions.
    let own_group = unsafe { libc::getpgrp() };
    let mut seen = Descendants::default();
    loop {
        seen.record(pid);
        let stop = STOP.load(Ordering::Acquire) || parent_gone(&mut control);
        if stop || exited(pid) {
            // One last look while the tree is still whole, then the group, then everyone recorded, until none is left.
            seen.record(pid);
            kill_group(pid);
            let until = Instant::now() + Duration::from_secs(2);
            while !seen.kill(own_group) && Instant::now() < until {
                std::thread::sleep(KILL_GAP);
            }
            let status = child.wait().map_err(|e| e.to_string())?;
            return Ok(if stop && status.success() { ExitStatus::from_raw(libc::SIGKILL) } else { status });
        }
        let mut fds = libc::pollfd { fd: control.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd on a descriptor this process owns. Wakes when the daemon's end closes, or to look again.
        unsafe { libc::poll(&mut fds, 1, LOOK_EVERY_MS) };
    }
}

/// Ends a guarded run when told to, or when dropped.
pub struct Stopper(std::sync::Mutex<Option<UnixStream>>);

impl Stopper {
    /// Ends the run: the guardian kills the agent and everything it started, then exits. Returns at once; harmless twice.
    pub fn stop(&self) {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    }
}

/// A run under its guardian, as the daemon holds it: the guardian process (its stdio is the agent's), and what stops it.
pub struct Guarded {
    pub child: Child,
    pub stopper: Stopper,
}

/// Starts `program args` under a guardian (`guard_exe` with [`GUARD_ARG`]), in `cwd`, with `env` added. `stdio` sets the pipes;
/// they reach the agent itself.
pub fn spawn(guard_exe: &Path, program: &Path, args: &[String], cwd: &Path, env: &[(String, String)], stdio: impl FnOnce(&mut Command)) -> io::Result<Guarded> {
    let (ours, theirs) = UnixStream::pair()?;
    let theirs_fd = theirs.as_raw_fd();
    let mut cmd = Command::new(guard_exe);
    cmd.arg(GUARD_ARG).arg(program).args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    stdio(&mut cmd);
    // SAFETY: between fork and exec only async-signal-safe dup2 and fcntl. `theirs` stays open in this process through spawn.
    unsafe {
        cmd.pre_exec(move || {
            if libc::dup2(theirs_fd, CONTROL_FD) == -1 || libc::fcntl(CONTROL_FD, libc::F_SETFD, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    // Our end must not leak into any child (or the guardian would never see us go).
    // SAFETY: plain fcntl on a descriptor we own.
    unsafe { libc::fcntl(ours.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    let child = cmd.spawn()?;
    drop(theirs);
    Ok(Guarded { child, stopper: Stopper(std::sync::Mutex::new(Some(ours))) })
}

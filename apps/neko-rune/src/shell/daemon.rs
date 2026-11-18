//! Daemon supervision: a port of `NativeLifecycle.swift` (`startDaemon`, `launchDaemonIfNeeded`, `DaemonRecoveryBackoff`).
//!
//! * The daemon owns singleton detection: a second one exits by itself. This watch **never kills a daemon, and never spawns one while
//!   one answers** (`Ping`); it does not rewrite `HOME` (development isolation stays in `NEKO_DATA_DIR`).
//! * It asks `Ping` every 5 s. An answer resets the backoff. No answer launches the daemon unless the child this watch started is still
//!   running, and the next look waits `min(60, 2^failures)` s, failures counting 1 to 6 (2, 4, 8, 16, 32, 60, 60 ...).
//!   Only a health read is repeated: nothing is replayed.
//! * The binary: `NEKO_DAEMON_PATH` if set, else the sibling `neko-daemon` in the folder of this executable (Swift's
//!   `Contents/MacOS/neko-daemon`). No executable there (a development run): one log line, and nothing is ever spawned.
//! * The child's output goes nowhere (as Swift's); the environment is inherited.
//!
//! The decisions are in [`Supervisor`] over the [`Io`] trait, so they are tested without a process or a socket; [`spawn_watch`] is the thread.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::link::shell::Ping;

/// The first delay, and the one after every answer.
pub const HEALTHY_EVERY: Duration = Duration::from_secs(5);
/// The daemon's file name.
pub const BINARY: &str = "neko-daemon";

/// Swift's `DaemonRecoveryBackoff`: failures 1 to 6, `min(60, 2^failures)` seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Backoff {
    failures: u32,
}

impl Backoff {
    pub fn failed(&mut self) -> Duration {
        self.failures = (self.failures + 1).min(6);
        Duration::from_secs(2u64.pow(self.failures).min(60))
    }

    pub fn recovered(&mut self) {
        self.failures = 0;
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }
}

/// What the supervisor needs from the world.
pub trait Io {
    fn ping(&mut self) -> Ping;
    /// Whether the child this watch started is still running.
    fn child_running(&mut self) -> bool;
    /// Starts the daemon binary; None when there is none to start (nothing happened), or the error.
    fn spawn(&mut self) -> Spawn;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spawn {
    Started,
    /// No executable to start (a development run).
    NoBinary,
    Failed(String),
}

/// What one look did, for the log and the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Did {
    /// The daemon answered; nothing to do.
    Healthy,
    /// Nothing answered and a daemon was started.
    Launched,
    /// Nothing answered, but the daemon this watch started still runs (it may be starting).
    Waiting,
    /// Something answered that was not `Pong`.
    Odd,
    /// Nothing answered and there is no binary to start.
    NoBinary,
}

#[derive(Debug, Default)]
pub struct Supervisor {
    backoff: Backoff,
    /// "No sibling daemon" is logged once.
    said_no_binary: bool,
}

impl Supervisor {
    /// The first look, at start: starts a daemon when none answers (Swift starts one at once; this asks first, so a daemon that is already
    /// there is never raced). Counts no failure. Returns what it did and how long until the next look.
    pub fn start(&mut self, io: &mut impl Io) -> (Did, Duration) {
        match io.ping() {
            Ping::Pong => (Did::Healthy, HEALTHY_EVERY),
            Ping::Other => (Did::Odd, HEALTHY_EVERY),
            Ping::Down => (self.launch_if_needed(io), HEALTHY_EVERY),
        }
    }

    /// One look after a wait: Swift's loop body.
    pub fn look(&mut self, io: &mut impl Io) -> (Did, Duration) {
        match io.ping() {
            Ping::Pong => {
                self.backoff.recovered();
                (Did::Healthy, HEALTHY_EVERY)
            }
            // An answer that is not Pong: back off, launch nothing (Swift).
            Ping::Other => (Did::Odd, self.backoff.failed()),
            Ping::Down => {
                let did = self.launch_if_needed(io);
                (did, self.backoff.failed())
            }
        }
    }

    fn launch_if_needed(&mut self, io: &mut impl Io) -> Did {
        if io.child_running() {
            return Did::Waiting;
        }
        match io.spawn() {
            Spawn::Started => Did::Launched,
            Spawn::NoBinary => {
                if !self.said_no_binary {
                    self.said_no_binary = true;
                    eprintln!("neko-rune: --app: no neko-daemon next to this executable (a development run?), so the daemon is not watched");
                }
                Did::NoBinary
            }
            Spawn::Failed(why) => {
                eprintln!("neko-rune: --app: could not start the daemon: {why}");
                Did::NoBinary
            }
        }
    }

    pub fn failures(&self) -> u32 {
        self.backoff.failures()
    }
}

/// The daemon binary to start: the override, else the sibling of `exe`. Only an executable file counts.
pub fn binary_path(override_path: Option<&str>, exe: Option<&Path>, is_executable: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let candidate = match override_path.filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => exe?.parent()?.join(BINARY),
    };
    is_executable(&candidate).then_some(candidate)
}

fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The real world: the socket, the binary, the one child.
pub struct Real {
    pub socket: PathBuf,
    pub child: Option<std::process::Child>,
}

impl Io for Real {
    fn ping(&mut self) -> Ping {
        crate::link::shell::ping(&self.socket)
    }

    fn child_running(&mut self) -> bool {
        match &mut self.child {
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                // Exited (reaped now) or not askable: no child to wait for.
                _ => {
                    self.child = None;
                    false
                }
            },
            None => false,
        }
    }

    fn spawn(&mut self) -> Spawn {
        let override_path = std::env::var("NEKO_DAEMON_PATH").ok();
        let exe = std::env::current_exe().ok();
        let Some(path) = binary_path(override_path.as_deref(), exe.as_deref(), is_executable_file) else { return Spawn::NoBinary };
        let started = std::process::Command::new(&path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
        match started {
            Ok(child) => {
                self.child = Some(child);
                Spawn::Started
            }
            Err(e) => Spawn::Failed(e.to_string()),
        }
    }
}

/// One turn of the watch. `active` is whether Rune holds the daemon duty right now (the Swift app may have taken it back). Inactive: nothing is
/// asked and nothing is started, and the daemon this watch started is left running. The first active turn after that (and the very first) is a
/// `start` (ask, start none that answers), later ones a `look`. None while inactive.
pub fn watch_step(active: bool, was_active: &mut bool, supervisor: &mut Supervisor, io: &mut impl Io) -> Option<(Did, Duration)> {
    if !active {
        *was_active = false;
        return None;
    }
    let step = if *was_active { supervisor.look(io) } else { supervisor.start(io) };
    *was_active = true;
    Some(step)
}

/// Starts the watch on a thread of its own. It runs for the life of the process; `active` says whether Rune holds the duty now.
pub fn spawn_watch(socket: PathBuf, active: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new().name("neko-daemon-watch".into()).spawn(move || {
        let mut io = Real { socket, child: None };
        let mut supervisor = Supervisor::default();
        let mut was_active = false;
        loop {
            let delay = match watch_step(active.load(Ordering::SeqCst), &mut was_active, &mut supervisor, &mut io) {
                Some((did, next)) => {
                    if did != Did::Healthy {
                        log(did);
                    }
                    next
                }
                None => HEALTHY_EVERY,
            };
            std::thread::sleep(delay);
        }
    });
}

fn log(did: Did) {
    match did {
        Did::Launched => eprintln!("neko-rune: --app: started the daemon"),
        Did::Odd => eprintln!("neko-rune: --app: the daemon answered something other than Pong"),
        Did::Waiting | Did::Healthy | Did::NoBinary => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted world: pings answer from a list, spawns are counted.
    struct Fake {
        pings: Vec<Ping>,
        child: bool,
        spawn: Spawn,
        spawned: usize,
        asked: usize,
    }

    impl Fake {
        fn new(pings: &[Ping]) -> Fake {
            Fake { pings: pings.to_vec(), child: false, spawn: Spawn::Started, spawned: 0, asked: 0 }
        }
    }

    impl Io for Fake {
        fn ping(&mut self) -> Ping {
            let at = self.asked.min(self.pings.len() - 1);
            self.asked += 1;
            self.pings[at]
        }
        fn child_running(&mut self) -> bool {
            self.child
        }
        fn spawn(&mut self) -> Spawn {
            self.spawned += 1;
            if self.spawn == Spawn::Started {
                self.child = true;
            }
            self.spawn.clone()
        }
    }

    #[test]
    fn the_backoff_is_swifts_two_to_the_failures_capped_at_sixty() {
        let mut b = Backoff::default();
        let seq: Vec<u64> = (0..8).map(|_| b.failed().as_secs()).collect();
        assert_eq!(seq, [2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(b.failures(), 6, "failures stop counting at 6");
        b.recovered();
        assert_eq!(b.failures(), 0);
        assert_eq!(b.failed().as_secs(), 2);
    }

    #[test]
    fn a_watch_handed_back_asks_nothing_and_starts_nothing_until_it_holds_the_duty_again() {
        let mut io = Fake::new(&[Ping::Down]);
        let mut s = Supervisor::default();
        let mut was = false;
        let first = watch_step(true, &mut was, &mut s, &mut io).expect("active: a first look");
        assert_eq!(first.0, Did::Launched);
        assert_eq!(io.spawned, 1);
        // The Swift app started: the duty is handed back. Nothing is asked, nothing is started, the child is left alone.
        let asked = io.asked;
        for _ in 0..4 {
            assert!(watch_step(false, &mut was, &mut s, &mut io).is_none());
        }
        assert_eq!((io.asked, io.spawned), (asked, 1), "no ping, no spawn while handed back");
        assert!(io.child, "the daemon it started is never killed");
        // It quit: the duty is Rune's again, and the first look asks before it starts anything.
        io.pings = vec![Ping::Pong];
        io.asked = 0;
        let back = watch_step(true, &mut was, &mut s, &mut io).expect("active again");
        assert_eq!(back, (Did::Healthy, HEALTHY_EVERY));
        assert_eq!(io.spawned, 1, "a daemon that answers is not started over");
    }

    #[test]
    fn a_daemon_that_answers_is_never_spawned_over() {
        let mut io = Fake::new(&[Ping::Pong]);
        let mut s = Supervisor::default();
        assert_eq!(s.start(&mut io), (Did::Healthy, HEALTHY_EVERY));
        for _ in 0..5 {
            assert_eq!(s.look(&mut io), (Did::Healthy, HEALTHY_EVERY));
        }
        assert_eq!(io.spawned, 0);
    }

    #[test]
    fn no_sibling_binary_means_nothing_is_ever_spawned_and_no_backoff_surprise() {
        let mut io = Fake::new(&[Ping::Down]);
        io.spawn = Spawn::NoBinary;
        let mut s = Supervisor::default();
        assert_eq!(s.start(&mut io).0, Did::NoBinary);
        assert_eq!(s.look(&mut io).0, Did::NoBinary);
        assert_eq!(io.spawned, 2, "it asks each time, and each time there is nothing to start");
        assert!(!io.child);
    }

    #[test]
    fn a_dead_daemon_is_started_once_and_then_waited_for_with_swifts_backoff() {
        let mut io = Fake::new(&[Ping::Down]);
        let mut s = Supervisor::default();
        assert_eq!(s.start(&mut io), (Did::Launched, HEALTHY_EVERY), "the start counts no failure");
        assert_eq!(s.failures(), 0);
        // The child runs but does not answer yet: no second one, delays grow.
        let waits: Vec<(Did, u64)> = (0..4).map(|_| { let (d, w) = s.look(&mut io); (d, w.as_secs()) }).collect();
        assert_eq!(waits, [(Did::Waiting, 2), (Did::Waiting, 4), (Did::Waiting, 8), (Did::Waiting, 16)]);
        assert_eq!(io.spawned, 1);
        // The child died: it is started again, once.
        io.child = false;
        assert_eq!(s.look(&mut io).0, Did::Launched);
        assert_eq!(io.spawned, 2);
        // Then it answers: the backoff resets.
        io.pings = vec![Ping::Pong];
        io.asked = 0;
        assert_eq!(s.look(&mut io), (Did::Healthy, HEALTHY_EVERY));
        assert_eq!(s.failures(), 0);
    }

    #[test]
    fn an_answer_that_is_not_pong_backs_off_and_launches_nothing() {
        let mut io = Fake::new(&[Ping::Other]);
        let mut s = Supervisor::default();
        assert_eq!(s.start(&mut io).0, Did::Odd);
        assert_eq!(s.look(&mut io), (Did::Odd, Duration::from_secs(2)));
        assert_eq!(s.look(&mut io), (Did::Odd, Duration::from_secs(4)));
        assert_eq!(io.spawned, 0);
    }

    #[test]
    fn the_binary_is_the_override_or_the_sibling_and_only_when_executable() {
        let exe = Path::new("/Applications/Neko.app/Contents/MacOS/neko-rune");
        let yes = |_: &Path| true;
        let no = |_: &Path| false;
        assert_eq!(binary_path(None, Some(exe), yes), Some(PathBuf::from("/Applications/Neko.app/Contents/MacOS/neko-daemon")));
        assert_eq!(binary_path(Some("/opt/x/neko-daemon"), Some(exe), yes), Some(PathBuf::from("/opt/x/neko-daemon")), "the override wins");
        assert_eq!(binary_path(Some(""), Some(exe), yes), Some(PathBuf::from("/Applications/Neko.app/Contents/MacOS/neko-daemon")), "an empty override is none");
        assert_eq!(binary_path(None, Some(exe), no), None, "a development run has no sibling");
        assert_eq!(binary_path(None, None, yes), None);
    }

    #[test]
    fn a_real_unreachable_socket_is_down() {
        let mut io = Real { socket: std::env::temp_dir().join("neko-daemon-watch-test-none.sock"), child: None };
        assert_eq!(io.ping(), Ping::Down);
        assert!(!io.child_running());
    }
}

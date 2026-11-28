//! What the daemon tests share: a real daemon on a temporary data folder, always with the fake CLI as its `claude` (never the
//! real one: no test spends anything), stopped by its PID when the test ends, pass or fail.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use neko_protocol::client::{Client, find};

/// The fake `claude`, built beside the daemon (scripts/check.sh builds both).
pub fn fake_cli() -> PathBuf {
    let p = Path::new(env!("CARGO_BIN_EXE_neko-daemon")).with_file_name("neko-fake-claude");
    assert!(p.is_file(), "build the fake CLI first: cargo build -p neko-core --bins ({} is missing)", p.display());
    p
}

pub fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    // The test's name keeps parallel tests apart; the time keeps runs apart.
    let dir = std::env::temp_dir().join(format!("neko-daemon-test-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A daemon this test started, stopped (by its PID) and its folder removed on drop.
pub struct Daemon {
    pub child: Child,
    pub dir: PathBuf,
    pub keep_dir: bool,
}

pub fn spawn(dir: &Path) -> Child {
    spawn_with(dir, &[])
}

/// The HOME a test's daemon (and so the fake CLI) sees: a folder of its own beside the data folder, so the person's own Claude
/// Code settings never reach a test. A test writes the settings it wants there.
pub fn home_of(dir: &Path) -> PathBuf {
    let h = dir.join("home");
    std::fs::create_dir_all(h.join(".claude")).unwrap();
    h
}

/// The daemon with the fake CLI and `env` (short run limits, a signed-out fake, …).
pub fn spawn_with(dir: &Path, env: &[(&str, &str)]) -> Child {
    let mut c = Command::new(env!("CARGO_BIN_EXE_neko-daemon"));
    c.arg("--data-dir").arg(dir).env("NEKO_CLAUDE_BIN", fake_cli()).env("HOME", home_of(dir)).env("NEKO_HEAT", "ok").env_remove("NEKO_FAKE_AUTH").env_remove("NEKO_FAKE_SCRIPT").env_remove("NEKO_HOOK_EXPIRE_MS");
    for (k, v) in env {
        c.env(k, v);
    }
    c.stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap()
}

impl Daemon {
    pub fn start(name: &str) -> Daemon {
        Daemon::start_in(temp_dir(name))
    }

    pub fn start_in(dir: PathBuf) -> Daemon {
        Daemon::start_with(dir, &[])
    }

    pub fn start_with(dir: PathBuf, env: &[(&str, &str)]) -> Daemon {
        let child = spawn_with(&dir, env);
        let d = Daemon { child, dir, keep_dir: false };
        d.wait_ready();
        d
    }

    pub fn wait_ready(&self) {
        let until = Instant::now() + Duration::from_secs(20);
        while Instant::now() < until {
            if let Ok((info, _)) = find(&self.dir)
                && info.pid == self.child.id()
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the daemon did not start");
    }

    pub fn port(&self) -> u16 {
        find(&self.dir).unwrap().0.port
    }

    pub fn token(&self) -> String {
        find(&self.dir).unwrap().1
    }

    pub fn client(&self) -> Client {
        Client::connect(self.port(), &self.token(), "test").unwrap()
    }

    pub fn signal(&self, sig: i32) {
        // SAFETY: signals the child this test spawned, by its PID.
        unsafe { libc::kill(self.child.id() as i32, sig) };
    }

    /// Sends SIGTERM and waits for the exit status.
    pub fn stop(&mut self) -> std::process::ExitStatus {
        self.signal(libc::SIGTERM);
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < until, "the daemon did not stop on SIGTERM");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if !self.keep_dir {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}


/// A task that has a cat. A thread the person starts has none (protocol 14), so one comes the way such a task still does: from a
/// round, run now. The round's words are the task's words.
pub fn task_of_cat(c: &mut Client, words: &str, cat: &str) -> neko_protocol::Task {
    use neko_protocol::{Reply, Request, Schedule};
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let round = match c.request(Request::CreateRound { name: format!("Round {n}"), words: words.into(), cat: Some(cat.into()), standing: false, workspace: None, schedule: Schedule::Every { hours: 24 }, on: false }).unwrap() {
        Reply::Round { round } => round,
        r => panic!("create_round: {r:?}"),
    };
    match c.request(Request::RunRoundNow { round: round.id }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("run_round_now: {r:?}"),
    }
}

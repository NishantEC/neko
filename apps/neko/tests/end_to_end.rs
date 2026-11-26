//! The whole slice, headless, against a real daemon on a temporary data folder and the fake CLI: the app starts the daemon, the
//! person types a task and presses Return, its agent asks and is allowed, its answer streams in; a second task is stopped with
//! Escape; the daemon is stopped, the app starts it again and the transcripts are still there.
//!
//! Needs the daemon binary: `NEKO_DAEMON_BIN` (scripts/check.sh builds it and sets it). Every daemon started here is stopped by the
//! PID it wrote, pass or fail; the real data folder is never touched.

mod support;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko_app::state::{App, Daemon};
use neko_protocol::TaskStatus;
use rune::{Key, Mods, Runtime};
use support::*;

/// The data folder and every daemon that ran in it, stopped and removed on drop.
struct Folder {
    dir: PathBuf,
    pids: Vec<i32>,
}

impl Folder {
    fn pid(&self) -> Option<i32> {
        neko_protocol::client::find(&self.dir).ok().map(|(i, _)| i.pid as i32)
    }
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

impl Drop for Folder {
    fn drop(&mut self) {
        for &pid in &self.pids {
            if alive(pid) {
                // SAFETY: a daemon this test's app started, by the PID it wrote.
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn until(rt: &mut Runtime<App>, what: &str, ok: impl Fn(&Runtime<App>) -> bool) {
    let end = Instant::now() + Duration::from_secs(20);
    while !ok(rt) {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
        rt.frame(0.0);
    }
    rt.frame(0.0);
}

fn shown(rt: &Runtime<App>, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.text.unwrap_or_default())
}

/// Whether some text on screen contains `words`.
fn on_screen(rt: &Runtime<App>, words: &str) -> bool {
    rt.elements().into_iter().filter_map(|e| e.text).any(|t| t.contains(words))
}

fn type_and_send(rt: &mut Runtime<App>, input: &str, words: &str) {
    click(rt, input);
    press(rt, Key::Text(words.into()), Mods::default());
    press(rt, Key::Enter, Mods::default());
}

fn status(rt: &Runtime<App>) -> Option<TaskStatus> {
    rt.state.open_task().map(|t| t.status)
}

#[test]
fn a_task_streams_asks_stops_and_its_transcript_survives_a_restart() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    assert!(bin.is_file(), "{} is not a file", bin.display());
    // Never the real `claude`: the daemon this test's app starts inherits this, and runs the fake CLI built beside it.
    let fake = bin.with_file_name("neko-fake-claude");
    assert!(fake.is_file(), "build the fake CLI first: cargo build -p neko-core --bins ({} is missing)", fake.display());
    // SAFETY: set before any thread of this test reads the environment; the test binary runs this one test that starts a daemon.
    unsafe { std::env::set_var("NEKO_CLAUDE_BIN", &fake) };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-e2e-{}-{nanos}", std::process::id())), pids: Vec::new() };
    let dir: &Path = &folder.dir.clone();
    // A HOME of the test's own: the person's Claude Code settings never reach the fake.
    let home = dir.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    // SAFETY: as above, before any thread reads the environment.
    unsafe { std::env::set_var("HOME", &home) };

    let mut rt = runtime((1200.0, 800.0), false, |_| {});
    let conn = neko_app::link::Conn::start(dir.to_path_buf(), Some(bin), || {});
    rt.update(move |s, _| {
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to start the daemon and connect", |rt| rt.state.daemon == Daemon::Connected);
    folder.pids.extend(folder.pid());

    // Type a task; it opens and its agent asks before running a command.
    let words = "Run the tests [fake:permission]";
    type_and_send(&mut rt, "composer-input", words);
    until(&mut rt, "the task to open", |rt| rt.state.open_task().is_some());
    let task = rt.state.open_task().unwrap().clone();
    assert_eq!(task.words, words, "the daemon kept the exact words");
    until(&mut rt, "the request to appear in the task", |rt| rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("perm-") && k.ends_with("-allow"))));
    assert_eq!(status(&rt), Some(TaskStatus::NeedsYou));
    let allow = rt.elements().into_iter().find_map(|e| e.key.filter(|k| k.starts_with("perm-") && k.ends_with("-allow"))).unwrap();
    click(&mut rt, &allow);
    until(&mut rt, "the run to finish after Allow once", |rt| status(rt) == Some(TaskStatus::Done));
    until(&mut rt, "the answer on the page", |rt| on_screen(rt, "The tests pass."));
    assert!(rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("run-") && k.ends_with("-facts"))), "the run card");
    assert_eq!(shown(&rt, "task-words").as_deref(), Some(words), "the task's words are the first message");

    // A second task streams; Escape stops it.
    press(&mut rt, Key::Command('n'), Mods { cmd: true, ..Mods::default() });
    type_and_send(&mut rt, "composer-input", "Go slowly [fake:slow]");
    until(&mut rt, "the second task to open and stream", |rt| rt.state.open_task().is_some_and(|t| t.id != task.id) && on_screen(rt, "slow1"));
    let second = rt.state.open_task().unwrap().id.clone();
    assert!(rt.state.open_is_working());
    press(&mut rt, Key::Escape, Mods::default());
    until(&mut rt, "Escape to stop it", |rt| status(rt) == Some(TaskStatus::Stopped));

    // Stop the daemon; the app starts a new one, and both transcripts are still there.
    let first = folder.pid().expect("the daemon wrote its PID");
    // SAFETY: the daemon this test's app started, by its PID.
    unsafe { libc::kill(first, libc::SIGTERM) };
    let end = Instant::now() + Duration::from_secs(10);
    while alive(first) {
        assert!(Instant::now() < end, "the daemon did not stop");
        std::thread::sleep(Duration::from_millis(20));
    }
    until(&mut rt, "the app to see the daemon go", |rt| matches!(rt.state.daemon, Daemon::Lost(_)));
    rt.update(|s, _| {
        s.tasks.clear();
        s.books.clear();
    });
    until(&mut rt, "a new daemon and the transcript back from the store", |rt| rt.state.daemon == Daemon::Connected && rt.state.books.get(&second).is_some_and(|b| b.loaded));
    let second_pid = folder.pid().expect("a new daemon");
    folder.pids.push(second_pid);
    assert_ne!(first, second_pid, "a new daemon process");
    let id = task.id.clone();
    rt.update(move |s, _| s.open_task_id(&id));
    until(&mut rt, "the first task's transcript", |rt| rt.state.books.get(&task.id).is_some_and(|b| b.loaded) && on_screen(rt, "The tests pass."));
    assert_eq!(rt.state.window_title(), "Run the tests [fake:permission]", "the window's title follows the task");

    rt.update(|s, _| s.conn = None);
    // SAFETY: as above.
    unsafe { libc::kill(second_pid, libc::SIGTERM) };
    let end = Instant::now() + Duration::from_secs(10);
    while alive(second_pid) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(second_pid), "the second daemon stopped cleanly");
}

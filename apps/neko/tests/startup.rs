//! The window opens at once: nothing before the first frame waits for the daemon. With a daemon that takes 5 seconds to come up,
//! the first frame is drawn well within a second, the composer takes a task, the task is kept and sent once the daemon answers.
//! A daemon that never comes up gets a calm line with Retry.
//!
//! Needs the daemon binary: `NEKO_DAEMON_BIN` (scripts/check.sh builds it and sets it), and the fake CLI beside it, which the daemon
//! runs instead of `claude`. Every daemon started here is stopped by the PID it wrote, pass or fail.

mod support;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{Duration, Instant};

use neko_app::state::{App, Daemon};
use rune::{Env, Key, Mods, Runtime};
use support::*;

static ENV: Once = Once::new();

/// The real daemon, and a temporary HOME of the test's own; never the real `claude` (the fake CLI is set explicitly).
fn daemon_bin() -> PathBuf {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    assert!(bin.is_file(), "{} is not a file", bin.display());
    let fake = bin.with_file_name("neko-fake-claude");
    assert!(fake.is_file(), "build the fake CLI first: cargo build -p neko-core --bins ({} is missing)", fake.display());
    ENV.call_once(|| {
        // One fixed folder that every run reuses (the tests share it, so no single test may remove it).
        let home = std::env::temp_dir().join("neko-startup-home");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        // SAFETY: set once, before this binary's tests start any daemon or read these.
        unsafe {
            std::env::set_var("NEKO_CLAUDE_BIN", &fake);
            std::env::set_var("HOME", &home);
        }
    });
    bin
}

/// A temporary folder, and the daemon that ran in it stopped by its PID, on drop.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Folder {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("neko-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Folder(dir)
    }
}

/// The PID a daemon wrote in `daemon.json` (read directly: a daemon of another protocol version is still found), else the slow
/// script's, written before it waits (the same process once it runs the daemon).
fn pid_in(dir: &Path) -> Option<i32> {
    let json = std::fs::read_to_string(dir.join("data/daemon.json")).ok();
    let from_json = json.and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok()).and_then(|v| v.get("pid").and_then(|p| p.as_i64())).map(|p| p as i32);
    from_json.or_else(|| std::fs::read_to_string(dir.join("slow.pid")).ok().and_then(|p| p.trim().parse().ok()))
}

impl Drop for Folder {
    fn drop(&mut self) {
        // A few times: if a test ends without letting go of its link, the link may start the daemon again and make the folder again.
        for _ in 0..4 {
            if let Some(pid) = pid_in(&self.0) {
                // SAFETY: a process this test's app started, by the PID it wrote.
                unsafe { libc::kill(pid, libc::SIGTERM) };
                let end = Instant::now() + Duration::from_secs(10);
                // SAFETY: signal 0 only asks whether the process exists.
                while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < end {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            let _ = std::fs::remove_dir_all(&self.0);
            std::thread::sleep(Duration::from_millis(700));
        }
    }
}

/// A daemon that takes `secs` to start: a script that waits, then becomes the real daemon (same PID).
fn slow_daemon(dir: &Path, real: &Path, secs: u32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("slow-daemon.sh");
    std::fs::write(&script, format!("#!/bin/sh\necho $$ > '{}'\nsleep {secs}\nexec '{}' \"$@\"\n", dir.join("slow.pid").display(), real.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

fn starting() -> App {
    App { daemon: Daemon::Starting, ..neko_app::headless_state() }
}

fn has(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of(rt: &Runtime<App>, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text)
}

#[test]
fn the_first_frame_does_not_wait_for_a_slow_daemon_and_a_task_typed_meanwhile_is_sent() {
    let real = daemon_bin();
    let folder = Folder::new("startup");
    let slow = slow_daemon(&folder.0, &real, 5);

    let launched = Instant::now();
    let mut rt = neko_app::test_app().init(starting).headless_runtime((1200.0, 800.0), Env::new(false, 2.0));
    rt.frame(0.0);
    let first_frame = launched.elapsed();
    // What a window does after its first frame (`start_link`): start the link, and hand it to the window. It must return at once.
    let begun = Instant::now();
    let conn = neko_app::begin_link(folder.0.join("data"), Some(slow), || {});
    let start_link = begun.elapsed();
    rt.update(move |s, _| s.link_started(conn));
    settle(&mut rt);
    eprintln!("startup: first frame {first_frame:?}, start_link {start_link:?}");
    assert!(first_frame < Duration::from_millis(1000), "the first frame took {first_frame:?}");
    assert!(start_link < Duration::from_millis(100), "starting the link blocked for {start_link:?}");

    // The window shows what it can by itself: the sidebar's quiet placeholder, Home and its composer.
    assert_eq!(rt.state.daemon, Daemon::Starting);
    assert_eq!(text_of(&rt, "connection").as_deref(), Some("Starting Neko…"));
    assert_eq!(text_of(&rt, "ws-default-none").as_deref(), Some("Loading…"), "not \"No tasks yet\" before the list arrives");
    click(&mut rt, "composer-input");
    press(&mut rt, Key::Text("Tidy the notes [fake:plain]".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.queued, "Return before the daemon answers keeps the task to send");
    assert_eq!(rt.state.draft.text, "Tidy the notes [fake:plain]", "the words stay in sight");

    // Every frame stays quick while the daemon starts; then the kept task is sent and opens.
    let end = Instant::now() + Duration::from_secs(30);
    let mut slowest = Duration::ZERO;
    while rt.state.open_task().is_none() {
        assert!(Instant::now() < end, "the kept task never opened (daemon {:?})", rt.state.daemon);
        let f = Instant::now();
        rt.frame(0.02);
        slowest = slowest.max(f.elapsed());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(slowest < Duration::from_millis(250), "a frame took {slowest:?} while the daemon started");
    assert!(launched.elapsed() >= Duration::from_secs(5), "the daemon really took its 5 seconds");
    assert_eq!(rt.state.open_task().unwrap().words, "Tidy the notes [fake:plain]");
    assert!(!rt.state.queued && rt.state.draft.text.is_empty());
    assert!(rt.state.tasks_loaded);
    // Let go of the link now (a runtime's state is not dropped with it), so it starts nothing more.
    rt.state.conn = None;
}

#[test]
fn a_daemon_that_does_not_start_gets_a_calm_line_with_retry() {
    daemon_bin();
    let folder = Folder::new("startup-never");
    let mut rt = neko_app::test_app().init(|| App { slow_after: Duration::from_millis(400), ..starting() }).headless_runtime((1200.0, 800.0), Env::new(false, 2.0));
    rt.frame(0.0);
    // A "daemon" that exits at once and never answers.
    let conn = neko_app::begin_link(folder.0.join("data"), Some(PathBuf::from("/usr/bin/false")), || {});
    rt.update(move |s, _| s.link_started(conn));
    settle(&mut rt);
    assert!(!has(&rt, "daemon-slow"), "not at once: a daemon gets a few seconds");
    std::thread::sleep(Duration::from_millis(500));
    settle(&mut rt);
    assert!(has(&rt, "daemon-slow") && has(&rt, "daemon-retry"), "after the wait, a calm line with Retry");
    assert_eq!(text_of(&rt, "connection").as_deref(), Some("Neko has not started"));
    click(&mut rt, "daemon-retry");
    settle(&mut rt);
    assert!(rt.state.waiting_since.is_some_and(|at| at.elapsed() < Duration::from_millis(400)), "Retry starts the wait over");
    assert!(!has(&rt, "daemon-slow"), "and the line goes until the wait is long again");
    // Let go of the link now (a runtime's state is not dropped with it), so it starts nothing more.
    rt.state.conn = None;
}

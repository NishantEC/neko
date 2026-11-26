//! The whole slice, headless, against a real daemon on a temporary data folder: the app starts the daemon, the person types and
//! presses Return, the task is stored and shown; the daemon is stopped, the app starts it again and the task is still there.
//!
//! Needs the daemon binary: `NEKO_DAEMON_BIN` (scripts/check.sh builds it and sets it). Every daemon started here is stopped by the
//! PID it wrote, pass or fail; the real data folder is never touched.

mod support;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko_app::state::{App, Daemon};
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

#[test]
fn type_return_stored_shown_and_still_there_after_a_restart() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    assert!(bin.is_file(), "{} is not a file", bin.display());
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-e2e-{}-{nanos}", std::process::id())), pids: Vec::new() };
    let dir: &Path = &folder.dir.clone();

    let mut rt = runtime((1200.0, 800.0), false, |_| {});
    let conn = neko_app::link::Conn::start(dir.to_path_buf(), Some(bin), || {});
    rt.update(move |s, _| {
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to start the daemon and connect", |rt| rt.state.daemon == Daemon::Connected);
    folder.pids.extend(folder.pid());

    click(&mut rt, "composer-input");
    let words = "Write the release notes\nand send them to Lee";
    press(&mut rt, Key::Text("Write the release notes".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods { shift: true, ..Mods::default() });
    press(&mut rt, Key::Text("and send them to Lee".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "the task to open", |rt| rt.state.open_task().is_some());
    let task = rt.state.open_task().unwrap().clone();
    assert_eq!(task.words, words, "the daemon kept the exact words");
    settle(&mut rt);
    assert_eq!(shown(&rt, "task-words").as_deref(), Some(words), "the task view shows the words");
    assert!(shown(&rt, &format!("task-{}", task.id.0)).is_some(), "it is in the sidebar");
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("task-status")), "with its quiet status line");
    assert_eq!(rt.state.draft.text, "", "the composer is empty again");

    // Stop the daemon; the app notices, starts a new one and reconnects.
    let first = folder.pid().expect("the daemon wrote its PID");
    // SAFETY: the daemon this test's app started, by its PID.
    unsafe { libc::kill(first, libc::SIGTERM) };
    let end = Instant::now() + Duration::from_secs(10);
    while alive(first) {
        assert!(Instant::now() < end, "the daemon did not stop");
        std::thread::sleep(Duration::from_millis(20));
    }
    until(&mut rt, "the app to see the daemon go", |rt| matches!(rt.state.daemon, Daemon::Lost(_)));
    rt.update(|s, _| s.tasks.clear());
    until(&mut rt, "a new daemon and the task back from the store", |rt| rt.state.daemon == Daemon::Connected && !rt.state.tasks.is_empty());
    let second = folder.pid().expect("a new daemon");
    folder.pids.push(second);
    assert_ne!(first, second, "a new daemon process");
    assert_eq!(rt.state.tasks, vec![task.clone()], "the task survived the restart");
    settle(&mut rt);
    assert_eq!(shown(&rt, "task-words").as_deref(), Some(words), "and is still open in the window");

    rt.update(|s, _| s.conn = None);
    // SAFETY: as above.
    unsafe { libc::kill(second, libc::SIGTERM) };
    let end = Instant::now() + Duration::from_secs(10);
    while alive(second) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(second), "the second daemon stopped cleanly");
}

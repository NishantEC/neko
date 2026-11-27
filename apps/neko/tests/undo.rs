//! Undo end to end, headless, against a real daemon on a temporary data folder and the fake CLI editing real files: a run in a
//! plain folder, Review shows its files, Undo takes them back, Redo puts them back, Undo again, and the follow-up tells the agent.
//!
//! Needs the daemon binary: `NEKO_DAEMON_BIN` (scripts/check.sh builds it and sets it). The daemon started here is stopped by the
//! PID it wrote, pass or fail; the real data folder is never touched.

mod support;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko_app::state::{App, Daemon};
use neko_protocol::{Request, TaskStatus, UndoState};
use rune::{Key, Mods, Runtime};
use support::*;

struct Folder {
    dir: PathBuf,
    pid: Option<i32>,
}

impl Drop for Folder {
    fn drop(&mut self) {
        if let Some(pid) = self.pid
            // SAFETY: signal 0 only asks whether the process exists.
            && unsafe { libc::kill(pid, 0) } == 0
        {
            // SAFETY: the daemon this test's app started, by the PID it wrote.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The frame clock: real seconds since the test began, so what fades in has faded in before it is pressed.
fn now() -> f64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn until(rt: &mut Runtime<App>, what: &str, ok: impl Fn(&Runtime<App>) -> bool) {
    let end = Instant::now() + Duration::from_secs(30);
    while !ok(rt) {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
        rt.frame(now());
    }
    rt.frame(now());
}

/// Lets what just appeared finish appearing, then clicks it.
fn press_on(rt: &mut Runtime<App>, key: &str) {
    let end = Instant::now() + Duration::from_millis(600);
    while Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
        rt.frame(now());
    }
    click(rt, key);
}

fn has(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn on_screen(rt: &Runtime<App>, words: &str) -> bool {
    rt.elements().into_iter().filter_map(|e| e.text).any(|t| t.contains(words))
}

fn state(rt: &Runtime<App>) -> Option<UndoState> {
    rt.state.book().and_then(|b| b.runs.first()).and_then(|r| r.undo.as_ref()).map(|u| u.state)
}

fn files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    v.sort();
    v
}

#[test]
fn review_undo_redo_and_the_follow_up_tells_the_agent() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    let fake = bin.with_file_name("neko-fake-claude");
    assert!(fake.is_file(), "build the fake CLI first: cargo build -p neko-core --bins ({} is missing)", fake.display());
    // SAFETY: set before any thread of this test reads the environment; this test binary has this one test.
    unsafe { std::env::set_var("NEKO_CLAUDE_BIN", &fake) };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-undo-e2e-{}-{nanos}", std::process::id())), pid: None };
    let dir = folder.dir.clone();
    let home = dir.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    // SAFETY: as above.
    unsafe { std::env::set_var("HOME", &home) };
    // The person's plain folder, a workspace of its own.
    let notes = dir.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(notes.join("plan.md"), "# Plan\n").unwrap();
    let notes = notes.canonicalize().unwrap();

    let mut rt = runtime((1200.0, 800.0), false, |_| {});
    let conn = neko_app::link::Conn::start(dir.join("data"), Some(bin), || {});
    rt.update(move |s, _| {
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to connect", |rt| rt.state.daemon == Daemon::Connected);
    folder.pid = neko_protocol::client::find(&dir.join("data")).ok().map(|(i, _)| i.pid as i32);
    let n = notes.to_string_lossy().into_owned();
    rt.update(move |s, _| s.add_workspace(n, true));
    until(&mut rt, "the project to be chosen", |rt| rt.state.chosen().is_some_and(|w| w.folder.is_some()));

    click(&mut rt, "composer-input");
    press(&mut rt, Key::Text("Write three files [fake:many:3]".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "the run to finish with its checkpoints", |rt| rt.state.open_task().is_some_and(|t| t.status == TaskStatus::Done) && state(rt) == Some(UndoState::Applied));
    assert_eq!(files(&notes), ["many", "plan.md"]);
    let run = rt.state.book().unwrap().runs[0].id.clone();

    // Review: the run's three files.
    press_on(&mut rt, &format!("run-{}-review", run.0));
    until(&mut rt, "the review's files", |rt| has(rt, "diff-2") && !has(rt, "diff-3"));
    assert!(on_screen(&rt, "miso's changes in this run"), "the heading names the cat whose changes they are");
    press_on(&mut rt, "diff-0-trigger");
    until(&mut rt, "a file's lines", |rt| on_screen(rt, "file 0"));
    press(&mut rt, Key::Escape, Mods::default());
    until(&mut rt, "the conversation again", |rt| rt.state.review.is_none() && has(rt, &format!("run-{}-undo", run.0)));

    // Undo, Redo, Undo.
    press_on(&mut rt, &format!("run-{}-undo", run.0));
    until(&mut rt, "the run undone", |rt| state(rt) == Some(UndoState::Undone));
    assert_eq!(files(&notes), ["plan.md"], "the files are as they were before the run");
    assert!(on_screen(&rt, "Undone: the files are as they were before this run."));
    press_on(&mut rt, &format!("run-{}-redo", run.0));
    until(&mut rt, "the run redone", |rt| state(rt) == Some(UndoState::Applied));
    assert_eq!(files(&notes), ["many", "plan.md"]);
    assert_eq!(std::fs::read_to_string(notes.join("many/2.txt")).unwrap(), "file 2\n");
    press_on(&mut rt, &format!("run-{}-undo", run.0));
    until(&mut rt, "the run undone again", |rt| state(rt) == Some(UndoState::Undone));

    // The follow-up: the agent hears first that its changes were undone.
    let task = rt.state.open_task().unwrap().id.clone();
    rt.update(move |s, _| {
        s.send(Request::FollowUp { task, words: "Carry on".into(), answering: None });
    });
    until(&mut rt, "the agent's answer", |rt| on_screen(rt, "the person undid the changes of your last run"));
    until(&mut rt, "the redo dropped", |rt| matches!(state(rt), Some(UndoState::Dropped | UndoState::Removed)));
    assert!(on_screen(&rt, "can no longer be redone"));
    assert!(!notes.join(".git").exists(), "nothing of Neko's was written into the folder");
}

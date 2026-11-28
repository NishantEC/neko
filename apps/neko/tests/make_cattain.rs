//! A Cattain made by asking, from end to end, headless, against a real daemon on a temporary data folder and the fake CLI (which calls
//! Neko's `make_cattain` through the real MCP hop): the thread shows the note with Open, the toast says "Made yuzu" with Undo, Open
//! takes the person to yuzu's own thread on who it is, Settings ▸ Cattains lists it with where it was made and the switch, and Undo
//! (⌘Z) retires it.
//!
//! Needs the daemon binary: `NEKO_DAEMON_BIN` (scripts/check.sh builds it and sets it). The daemon started here is stopped by the
//! PID it wrote, pass or fail; the real data folder is never touched.

mod support;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use neko_app::state::{App, Daemon};
use neko_protocol::TaskStatus;
use rune::{Key, Mods, Runtime};
use support::*;

struct Folder {
    dir: PathBuf,
    pid: Option<i32>,
}

impl Drop for Folder {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            // SAFETY: the daemon this test's app started, by the PID it wrote.
            unsafe { libc::kill(pid, libc::SIGKILL) };
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

fn has_key(rt: &Runtime<App>, f: impl Fn(&str) -> bool) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref().is_some_and(&f))
}

fn toast_titles(rt: &Runtime<App>) -> Vec<String> {
    rt.state.toasts.items().map(|(_, spec)| spec.title.clone()).collect()
}

#[test]
fn a_cattain_made_by_asking_shows_its_note_and_toast_opens_on_who_it_is_and_undo_retires_it() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    let fake = bin.with_file_name("neko-fake-claude");
    assert!(fake.is_file(), "build the fake CLI first ({} is missing)", fake.display());
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-make-cattain-e2e-{}-{nanos}", std::process::id())), pid: None };
    let dir = folder.dir.clone();
    let home = dir.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    // SAFETY: set before any thread of this test reads the environment; this binary runs one test. Never the real `claude`.
    unsafe {
        std::env::set_var("NEKO_CLAUDE_BIN", &fake);
        std::env::set_var("HOME", &home);
    }
    let mut rt = runtime((1300.0, 800.0), false, |_| {});
    let conn = neko_app::link::Conn::start(dir.clone(), Some(bin), || {});
    rt.update(move |s, _| {
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to connect and list the starter cats", |rt| rt.state.daemon == Daemon::Connected && rt.state.cats.len() == 3);
    folder.pid = neko_protocol::client::find(&dir).ok().map(|(i, _)| i.pid as i32);

    // The person asks a thread; its agent makes yuzu through Neko's own tool.
    rt.update(|s, _| s.go_new_thread());
    for _ in 0..4 {
        rt.frame(1.0);
    }
    click(&mut rt, "composer-input");
    press(&mut rt, Key::Text("Make a Cattain for the iOS work [fake:mcp] mcp neko make_cattain {\"purpose\":\"iOS work\"}".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "the thread to finish", |rt| rt.state.open_task().is_some_and(|t| t.status == TaskStatus::Done));
    let thread = rt.state.open_task().unwrap().id.clone();
    until(&mut rt, "yuzu in the house and the note in the thread", |rt| rt.state.cats.iter().any(|c| c.name == "yuzu") && has_key(rt, |k| k.starts_with("cattain-made-") && k.ends_with("-open")));
    let yuzu = rt.state.cats.iter().find(|c| c.name == "yuzu").unwrap().clone();
    assert_eq!(yuzu.made_in.as_ref(), Some(&thread));
    let note = rt.elements().into_iter().find(|e| e.role == "note" && e.label.as_deref().is_some_and(|l| l.starts_with("Made a Cattain"))).expect("the note");
    assert_eq!(note.label.as_deref(), Some("Made a Cattain: yuzu \u{b7} iOS work"));
    assert_eq!(toast_titles(&rt), ["Made yuzu"], "the toast, once");

    // Open: yuzu's own thread, on who it is.
    let open = rt.elements().into_iter().find(|e| e.key.as_deref().is_some_and(|k| k.starts_with("cattain-made-") && k.ends_with("-open"))).unwrap().key.unwrap();
    click(&mut rt, &open);
    until(&mut rt, "yuzu's own thread, on who it is", |rt| rt.state.open_cattain_cat().is_some_and(|c| c.name == "yuzu") && has_key(rt, |k| k == "cattain-intro") && has_key(rt, |k| k == "cattain-intro-purpose"));

    // Settings ▸ Cattains lists it, says where it was made, and has the switch.
    rt.update(|s, _| {
        s.settings_open = true;
        s.settings_page = 3;
    });
    settle(&mut rt);
    let facts = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("settings-cat-{}-facts", yuzu.id))).expect("yuzu in Settings");
    assert!(facts.text.as_deref().is_some_and(|t| t.contains("made in \u{201c}Make a Cattain for the iOS work")), "{:?}", facts.text);
    assert!(has_key(&rt, |k| k == "settings-agents-make-cattains"));

    // Undo (⌘Z with the toast showing): retired, and the toast is gone.
    assert!(rt.state.undo_toast(), "the newest toast has Undo");
    let id = yuzu.id.clone();
    until(&mut rt, "yuzu retired", move |rt| rt.state.cat(&id).is_some_and(|c| !c.is_active()));
    assert!(toast_titles(&rt).is_empty());

    rt.update(|s, _| s.conn = None);
    if let Some(pid) = folder.pid.take() {
        // SAFETY: as above.
        unsafe { libc::kill(pid, libc::SIGTERM) };
        let end = Instant::now() + Duration::from_secs(10);
        // SAFETY: signal 0 only asks whether it exists.
        while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

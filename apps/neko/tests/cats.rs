//! Cattains from end to end, headless, against a real daemon on a temporary data folder and the fake CLI (which records what it was
//! given): make one, find its own thread open on its Profile, save its RULES.md and see it reach its run with its turn led by its face
//! and what it wrote in its folder reach the app; save CREW.md, start a thread in its crew from its Overview and see CREW.md reach that
//! run and nothing of RULES.md.
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

fn type_in(rt: &mut Runtime<App>, input: &str, words: &str) {
    click(rt, input);
    press(rt, Key::Text(words.into()), Mods::default());
}

/// The blocks the fake CLI was given with `--append-system-prompt`, one per run.
fn blocks(record: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(record)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| {
            let a: Vec<String> = v["argv"].as_array()?.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect();
            a.iter().any(|x| x == "-p").then(|| a.windows(2).filter(|w| w[0] == "--append-system-prompt").map(|w| w[1].clone()).collect::<Vec<_>>().join("|"))
        })
        .collect()
}

#[test]
fn a_cattain_has_its_own_thread_its_files_and_a_crew_that_reads_crew_md() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    let fake = bin.with_file_name("neko-fake-claude");
    assert!(fake.is_file(), "build the fake CLI first ({} is missing)", fake.display());
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-cats-e2e-{}-{nanos}", std::process::id())), pid: None };
    let dir = folder.dir.clone();
    let home = dir.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let record = dir.join("fake-record.jsonl");
    // SAFETY: set before any thread of this test reads the environment; this binary runs one test. Never the real `claude`.
    unsafe {
        std::env::set_var("NEKO_CLAUDE_BIN", &fake);
        std::env::set_var("HOME", &home);
        std::env::set_var("NEKO_FAKE_RECORD", &record);
    }
    let mut rt = runtime((1300.0, 800.0), false, |_| {});
    let conn = neko_app::link::Conn::start(dir.clone(), Some(bin), || {});
    rt.update(move |s, _| {
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to start the daemon, connect and list the starter cats", |rt| rt.state.daemon == Daemon::Connected && rt.state.cats.len() == 3 && rt.state.cats[0].id == "cat_miso");
    folder.pid = neko_protocol::client::find(&dir).ok().map(|(i, _)| i.pid as i32);
    assert!(!has_key(&rt, |k| k == "cat-cat_tofu" || k == "cattains-section" || k == "new-cat"), "no Cattain is at work: the sidebar lists none and has no New cat…");

    // New Cattain… (Settings ▸ Cattains is where they are made): name it, and its own thread opens on its Profile.
    rt.update(|s, _| {
        s.settings_open = true;
        s.settings_page = neko_app::settings_view::Page::Cattains;
    });
    settle(&mut rt);
    assert!(has_key(&rt, |k| k == "settings-cat-cat_tofu"), "Settings lists every Cattain");
    click(&mut rt, "settings-new-cat");
    type_in(&mut rt, "cat-name-input", "pip");
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "pip's own thread on its Profile", |rt| rt.state.open_cattain_cat().is_some_and(|c| c.name == "pip") && has_key(rt, |k| k == "cat-profile"));
    let pip = rt.state.open_cattain_cat().unwrap().id.clone();
    let own = rt.state.open_task().unwrap().clone();
    assert!(own.cattain && !rt.state.thread_order().contains(&own.id), "its own thread is not listed under Threads");
    let band = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("band-title")).expect("the band's title");
    assert_eq!(band.text.as_deref(), Some("pip"), "the band leads with its name");
    assert!(has_key(&rt, |k| k == "band-face"), "and its face");

    // RULES.md, in the Files tab: typed, unsaved, saved.
    let p = pip.clone();
    rt.update(move |s, _| s.open_cattain_file(&p, "RULES.md"));
    until(&mut rt, "RULES.md in the editor", |rt| has_key(rt, |k| k == "cattain-file-input"));
    type_in(&mut rt, "cattain-file-input", "PIP-RULE: answer in one short paragraph.");
    assert!(has_key(&rt, |k| k == "cattain-file-unsaved"), "unsaved changes say so");
    click(&mut rt, "cattain-file-save");
    let p = pip.clone();
    until(&mut rt, "RULES.md saved", move |rt| rt.state.cattains.files.get(&(p.clone(), "RULES.md".to_owned())).is_some_and(|f| f.text == "PIP-RULE: answer in one short paragraph." && f.previous));
    assert_eq!(std::fs::read_to_string(dir.join("cattains").join(&pip).join("RULES.md")).unwrap(), "PIP-RULE: answer in one short paragraph.", "on disk");

    // Writing to pip: its run is handed RULES.md, and its turn is led by its face.
    type_in(&mut rt, "reply-input", "What is open? [fake:cattain-notes]");
    press(&mut rt, Key::Enter, Mods::default());
    let o = own.id.clone();
    until(&mut rt, "pip's answer", move |rt| rt.state.tasks.iter().any(|t| t.id == o && t.status == TaskStatus::Done));
    let o = own.id.clone();
    until(&mut rt, "its turn, led by its face", move |rt| rt.state.books.get(&o).is_some_and(|b| b.runs.len() == 1) && has_key(rt, |k| k.starts_with("turn-") && k.ends_with("-who")));
    let got = blocks(&record);
    assert_eq!(got.len(), 1);
    assert!(got[0].starts_with("<neko-cattain>\nYou are pip, a Cattain in Neko") && got[0].contains("Your standing rules (RULES.md):\nPIP-RULE: answer in one short paragraph.\n</neko-cattain>"), "{}", got[0]);
    // What it wrote in its folder reaches the Digest tab.
    let p = pip.clone();
    until(&mut rt, "the digest it wrote", move |rt| rt.state.cattains.files.get(&(p.clone(), "DIGEST.md".to_owned())).is_some_and(|f| f.text.contains("Merge acme#12?")));

    // CREW.md, then a thread in its crew, started from its Overview.
    let p = pip.clone();
    rt.update(move |s, _| s.choose_file(&p, "CREW.md"));
    until(&mut rt, "CREW.md in the editor", |rt| rt.elements().iter().any(|e| e.key.as_deref() == Some("cattain-file-body") && e.label.as_deref() == Some("CREW.md")));
    type_in(&mut rt, "cattain-file-input", "PIP-CREW: open a PR; never merge.");
    click(&mut rt, "cattain-file-save");
    let p = pip.clone();
    until(&mut rt, "CREW.md saved", move |rt| rt.state.cattains.files.get(&(p.clone(), "CREW.md".to_owned())).is_some_and(|f| f.text.starts_with("PIP-CREW")));
    rt.update(|s, _| s.select_tab(0));
    until(&mut rt, "the Overview's crew", |rt| has_key(rt, |k| k == "cattain-new-crew"));
    click(&mut rt, "cattain-new-crew");
    until(&mut rt, "New thread, in pip's crew", |rt| has_key(rt, |k| k == "new-thread-crew"));
    // Let the page's fade finish (headless frames otherwise stand still in time).
    for _ in 0..4 {
        rt.frame(1.0);
    }
    type_in(&mut rt, "composer-input", "Tidy the README [fake:plain]");
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "the crew thread to finish", |rt| rt.state.open_task().is_some_and(|t| !t.cattain && t.status == TaskStatus::Done));
    let crew = rt.state.open_task().unwrap().clone();
    assert_eq!(crew.crew_of(), Some(pip.as_str()), "the thread is in pip's crew");
    let band = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("task-cat")).expect("its crew in the band");
    assert_eq!(band.label.as_deref(), Some("In pip's crew"));
    assert!(!has_key(&rt, |k| k.starts_with("turn-") && k.ends_with("-who")), "a crew thread's turns have no face");
    let got = blocks(&record);
    assert_eq!(got.len(), 2);
    assert!(got[1].starts_with("<neko-crew>\nThis thread is in pip's crew.") && got[1].contains("PIP-CREW: open a PR; never merge.\n</neko-crew>") && !got[1].contains("PIP-RULE"), "{}", got[1]);

    // The sidebar's Cattains row opens pip's own thread again.
    rt.update(|s, _| s.go_new_thread());
    let p = pip.clone();
    rt.update(move |s, _| s.open_cattain(&p));
    settle(&mut rt);
    assert_eq!(rt.state.open.as_ref(), Some(&own.id));

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

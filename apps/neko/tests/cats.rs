//! Cats from end to end, headless, against a real daemon on a temporary data folder and the fake CLI (which records what it was
//! given): make a cat, give it standing rules, give it a task, see its name on the task and its rules in what the CLI received;
//! edit the rules, follow up, and see the quiet line and the new rules reach the CLI.
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
fn a_cat_with_rules_does_a_task_and_its_edited_rules_reach_the_next_run() {
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
    let mut rt = runtime((1200.0, 800.0), false, |_| {});
    let conn = neko_app::link::Conn::start(dir.clone(), Some(bin), || {});
    rt.update(move |s, _| {
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to start the daemon, connect and list the starter cats", |rt| rt.state.daemon == Daemon::Connected && rt.state.cats.len() == 3 && rt.state.cats[0].id == "cat_miso");
    folder.pid = neko_protocol::client::find(&dir).ok().map(|(i, _)| i.pid as i32);
    assert!(has_key(&rt, |k| k == "cat-cat_tofu"), "the sidebar lists the cats");

    // New cat…: name it, and its page opens.
    click(&mut rt, "new-cat");
    type_in(&mut rt, "cat-name-input", "pip");
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "pip's page", |rt| rt.state.page_cat().is_some_and(|c| c.name == "pip"));
    let pip = rt.state.page_cat().unwrap().id.clone();
    // Standing rules, saved.
    type_in(&mut rt, "cat-rules-input", "Answer in one short paragraph.");
    assert!(has_key(&rt, |k| k == "cat-unsaved"), "unsaved changes say so");
    click(&mut rt, "cat-save");
    until(&mut rt, "the rules saved", |rt| rt.state.page_cat().is_some_and(|c| c.rules == "Answer in one short paragraph."));

    // A task for pip, picked in the composer.
    press(&mut rt, Key::Command('n'), Mods { cmd: true, ..Mods::default() });
    let p = pip.clone();
    rt.update(move |s, _| s.pick_cat(Some(p.clone())));
    type_in(&mut rt, "composer-input", "Tidy the notes [fake:plain]");
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "the task to finish", |rt| rt.state.open_task().is_some_and(|t| t.status == TaskStatus::Done));
    let task = rt.state.open_task().unwrap().clone();
    assert_eq!(task.cat, pip, "the task is pip's");
    let band = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("task-cat")).expect("the cat in the task's band");
    assert_eq!(band.label.as_deref(), Some("Cat: pip"));
    assert!(has_key(&rt, |k| k.starts_with("turn-") && k.ends_with("-cat")), "the turn is led by the cat");
    let got = blocks(&record);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0], "<neko-cat>\nYou are pip, one of the person's cats in Neko.\n\nYour standing rules, from the person:\nAnswer in one short paragraph.\n</neko-cat>");

    // Edit the rules from the task's band, then follow up.
    click(&mut rt, "task-cat");
    until(&mut rt, "pip's page again", |rt| rt.state.open_cat.as_deref() == Some(pip.as_str()));
    // Let the task view's exit finish (headless frames otherwise stand still in time), then type at the end of the rules.
    for _ in 0..4 {
        rt.frame(1.0);
    }
    type_in(&mut rt, "cat-rules-input", " Use British spelling.");
    click(&mut rt, "cat-save");
    until(&mut rt, "the new rules saved", |rt| rt.state.page_cat().is_some_and(|c| c.rules.ends_with("Use British spelling.")));
    assert!(rt.state.page_cat().unwrap().previous_rules.as_deref() == Some("Answer in one short paragraph."), "one previous version kept");
    click(&mut rt, &format!("task-{}", task.id.0));
    for _ in 0..4 {
        rt.frame(1.0);
    }
    type_in(&mut rt, "reply-input", "Once more [fake:plain]");
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "the follow-up to finish with the quiet line", |rt| rt.state.books.get(&task.id).is_some_and(|b| b.runs.len() == 2 && b.runs[1].status == neko_protocol::RunStatus::Done) && has_key(rt, |k| k.starts_with("rules-changed-")));
    let got = blocks(&record);
    assert_eq!(got.len(), 2, "one block per run");
    assert!(got[1].ends_with("Answer in one short paragraph. Use British spelling.\n</neko-cat>"), "{}", got[1]);

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

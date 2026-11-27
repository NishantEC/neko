//! Threads in the core (docs/knowledge/threads.md): when a thread was last opened, what it last said, and what it is doing, derived
//! in one place. Time is handed in; nothing waits.

use neko_core::conductor::{Out, RunBook, Who};
use neko_core::runtime::{Failure, Outcome};
use neko_core::store::SCHEMA_VERSION;
use neko_core::{Core, FixedClock, Place, SequentialIds, Store};
use neko_protocol::{Change, Pricing, TaskId, TaskStatus, ThreadStatus};

struct Gone(std::path::PathBuf);
impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir() -> Gone {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("neko-threads-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(d.join("w")).unwrap();
    Gone(d)
}

fn open(d: &Gone) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn who() -> Who {
    Who { id: "claude".into(), name: "Claude Code".into(), sign_in: "claude auth login".into(), install: "install".into(), pricing: Pricing::ApiEquivalent }
}

fn begin(core: &mut Core, d: &Gone, task: &TaskId) -> RunBook {
    let place = Place { workdir: d.0.join("w").to_string_lossy().into_owned(), branch: None, base_commit: None, repo: None, plain: false };
    RunBook::begin(core, task, who(), place, neko_protocol::PermissionMode::Follow, None, None, None).unwrap().0
}

fn say(core: &mut Core, b: &mut RunBook, words: &str) -> Vec<Out> {
    use neko_core::runtime::AgentEvent;
    let now = core.now();
    let mut out = b.on(core, AgentEvent::TextDelta(words.into()), now).unwrap();
    let now = core.now();
    out.extend(b.on(core, AgentEvent::TextDone, now).unwrap());
    out
}

fn finish(core: &mut Core, b: &mut RunBook, o: Outcome) {
    let now = core.now();
    b.finish(core, o, None, now).unwrap();
}

fn task(core: &Core, id: &TaskId) -> neko_protocol::Task {
    core.store.get_task(id).unwrap().unwrap()
}

#[test]
fn a_task_the_person_made_is_read_at_birth() {
    let d = dir();
    let mut core = open(&d);
    let (t, _) = core.create_task("Rename the label").unwrap();
    assert!(!task(&core, &t.id).unread());
    assert_eq!(task(&core, &t.id).thread_status, ThreadStatus::Idle);
    assert!(!task(&core, &t.id).needs_you);
}

#[test]
fn the_agents_words_make_a_thread_unread_and_its_last_line_what_the_row_says() {
    let d = dir();
    let mut core = open(&d);
    let (t, _) = core.create_task("Fix the build").unwrap();
    let mut b = begin(&mut core, &d, &t.id);
    assert_eq!(task(&core, &t.id).thread_status, ThreadStatus::Working);
    assert!(!task(&core, &t.id).unread(), "starting a run is the person's own doing");
    let out = say(&mut core, &mut b, "Looking at it.\n\nRenamed the label in **three** places.");
    let t1 = task(&core, &t.id);
    assert_eq!(t1.latest_line, "Renamed the label in three places.");
    assert!(t1.unread());
    assert!(out.iter().any(|o| matches!(o, Out::Change(_, Change::TaskUpdated { task }) if task.latest_line == "Renamed the label in three places.")), "screens hear the new line");
    finish(&mut core, &mut b, Outcome::Done);
    let t2 = task(&core, &t.id);
    assert_eq!((t2.status, t2.thread_status), (TaskStatus::Done, ThreadStatus::Idle));
    assert!(t2.unread(), "it finished while the person was elsewhere");
}

#[test]
fn opening_a_thread_reads_it_and_it_can_be_made_unread_again() {
    let d = dir();
    let mut core = open(&d);
    let (t, _) = core.create_task("Fix the build").unwrap();
    let mut b = begin(&mut core, &d, &t.id);
    say(&mut core, &mut b, "Done.");
    finish(&mut core, &mut b, Outcome::Done);
    assert!(task(&core, &t.id).unread());
    let now = core.now();
    let (read, _) = core.store.set_read(&t.id, true, now).unwrap();
    assert!(!read.unread());
    let (again, _) = core.store.set_read(&t.id, false, now).unwrap();
    assert!(again.unread());
    assert!(again.last_read_ms < again.activity_ms);
}

#[test]
fn a_failed_run_shows_failed_until_the_thread_is_opened_and_says_why() {
    let d = dir();
    let mut core = open(&d);
    let (t, _) = core.create_task("Fix the build").unwrap();
    let mut b = begin(&mut core, &d, &t.id);
    finish(&mut core, &mut b, Outcome::Failed(Failure::Crashed { code: Some(3), words: "fatal".into() }));
    let f = task(&core, &t.id);
    assert_eq!((f.status, f.thread_status), (TaskStatus::Failed, ThreadStatus::Failed));
    assert!(!f.needs_you, "a crash is not something to act on");
    assert!(f.failure_line.contains("stopped unexpectedly"), "{}", f.failure_line);
    let now = core.now();
    let (read, _) = core.store.set_read(&t.id, true, now).unwrap();
    assert_eq!(read.thread_status, ThreadStatus::Idle);
}

#[test]
fn a_failure_the_person_can_act_on_raises_the_needs_you_flag_the_list_agrees() {
    let d = dir();
    let mut core = open(&d);
    let (t, _) = core.create_task("Fix the build").unwrap();
    let mut b = begin(&mut core, &d, &t.id);
    finish(&mut core, &mut b, Outcome::Failed(Failure::NotSignedIn));
    let f = task(&core, &t.id);
    assert!(f.needs_you);
    let items = neko_core::needs::derive(&core.store).unwrap();
    assert!(items.iter().any(|i| i.task == t.id), "{items:?}");
}

#[test]
fn a_run_that_ends_with_a_question_needs_the_person_and_a_stop_does_not_make_it_unread() {
    let d = dir();
    let mut core = open(&d);
    let (t, _) = core.create_task("Rename it").unwrap();
    let mut b = begin(&mut core, &d, &t.id);
    say(&mut core, &mut b, "Which label should I rename?");
    finish(&mut core, &mut b, Outcome::Done);
    let q = task(&core, &t.id);
    assert_eq!((q.status, q.needs_you, q.thread_status), (TaskStatus::Asked, true, ThreadStatus::Idle));
    let (u, _) = core.create_task("Other").unwrap();
    let mut b = begin(&mut core, &d, &u.id);
    finish(&mut core, &mut b, Outcome::Stopped);
    assert!(!task(&core, &u.id).unread(), "the person stopped it themselves");
}

#[test]
fn a_store_from_before_threads_opens_with_every_task_read_and_its_last_line_kept() {
    let d = dir();
    let path = d.0.join("neko.sqlite3");
    {
        let mut core = open(&d);
        let (t, _) = core.create_task("Fix the build").unwrap();
        let mut b = begin(&mut core, &d, &t.id);
        say(&mut core, &mut b, "All fixed.");
        finish(&mut core, &mut b, Outcome::Done);
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("ALTER TABLE tasks DROP COLUMN last_read_ms; ALTER TABLE tasks DROP COLUMN activity_ms; ALTER TABLE tasks DROP COLUMN latest_line; PRAGMA user_version = 8;").unwrap();
    }
    assert_eq!(Store::open_at(&path, 8).unwrap().version().unwrap(), 8);
    let core = open(&d);
    assert_eq!(core.store.version().unwrap(), SCHEMA_VERSION);
    let all = core.store.all_tasks().unwrap();
    assert_eq!(all.len(), 1);
    assert!(!all[0].unread());
    assert_eq!(all[0].latest_line, "All fixed.");
    assert!(all[0].activity_ms >= all[0].created_ms);
}

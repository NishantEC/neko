//! A Cattain runs its crew (docs/knowledge/cats.md, "The crew"; version 23 of the protocol, 19 of the store): a crew thread its agent
//! starts carries its brief as its words, the title it gave and the Cattain that started it, in the Cattain's project unless another is
//! named; a retired Cattain starts none; at most eight are unfinished at once; a crew wake is kept once per run and counted per hour;
//! a request of a crew thread is found until it has told its Cattain; the migration from version 18.

use neko_core::crew::{CrewAsk, LIVE_LIMIT};
use neko_core::store::SCHEMA_VERSION;
use neko_core::subagents::WakeRow;
use neko_core::{Core, FixedClock, SequentialIds, Store};
use neko_protocol::{TaskId, TaskStatus};

struct Gone(std::path::PathBuf);
impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir() -> Gone {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("neko-crew-test-{}-{n}-{nanos}", std::process::id()));
    std::fs::create_dir_all(d.join("acme-web")).unwrap();
    Gone(d)
}

fn open(d: &Gone) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn ask<'a>(title: &'a str, brief: &'a str) -> CrewAsk<'a> {
    CrewAsk { title, brief, ..CrewAsk::default() }
}

#[test]
fn a_crew_thread_carries_the_brief_the_title_and_who_started_it_in_the_cattains_project() {
    let d = dir();
    let mut c = open(&d);
    let w = c.add_workspace(&d.0.join("acme-web").to_string_lossy()).unwrap().workspace;
    c.set_cat_workspace("cat_miso", Some(&w.id)).unwrap();
    let (t, _) = c.create_crew_thread("cat_miso", &ask("  Fix   the flaky\ntest ", "Fix the flaky login test. Run it ten times.")).unwrap();
    assert_eq!((t.title.as_str(), t.words.as_str()), ("Fix the flaky test", "Fix the flaky login test. Run it ten times."));
    assert_eq!((t.cat.as_deref(), t.started_by.as_deref(), t.crew_of(), t.status), (Some("cat_miso"), Some("cat_miso"), Some("cat_miso"), TaskStatus::New));
    assert_eq!((t.workspace.as_str(), t.in_folder), (w.id.as_str(), false), "the Cattain's project, in a worktree of its own");
    assert!(t.folder.is_some());
    // It is an ordinary thread, read back the same; it waits for room until it runs.
    assert_eq!(c.store.get_task(&t.id).unwrap().unwrap().started_by.as_deref(), Some("cat_miso"));
    assert_eq!(c.store.queued_crew().unwrap().iter().map(|x| x.id.clone()).collect::<Vec<_>>(), [t.id.clone()]);
    assert_eq!(c.store.crew_of("cat_miso").unwrap().len(), 1);
    // A project named by its folder's last part; an unknown one is refused with the projects there are; No project for a Cattain with none.
    let (named, _) = c.create_crew_thread("cat_mochi", &CrewAsk { project: Some("acme-web"), in_folder: true, ..ask("Docs", "Tidy the docs.") }).unwrap();
    assert_eq!((named.workspace.as_str(), named.in_folder), (w.id.as_str(), true));
    let e = c.create_crew_thread("cat_mochi", &CrewAsk { project: Some("acme-ios"), ..ask("X", "Y") }).unwrap_err();
    assert!(e.contains("no project called acme-ios") && e.contains("acme-web") && e.contains("use your own project"), "{e}");
    let (none, _) = c.create_crew_thread("cat_tofu", &ask("X", "Y")).unwrap();
    assert_eq!(none.workspace, neko_core::DEFAULT_WORKSPACE);
}

#[test]
fn a_retired_cattain_starts_none_and_no_more_than_eight_are_unfinished() {
    let d = dir();
    let mut c = open(&d);
    assert!(c.create_crew_thread("cat_miso", &ask("", "Y")).unwrap_err().contains("needs a title"));
    assert!(c.create_crew_thread("cat_miso", &ask("X", "  ")).unwrap_err().contains("needs a brief"));
    for i in 0..LIVE_LIMIT {
        c.create_crew_thread("cat_miso", &ask(&format!("T{i}"), "Do it.")).unwrap();
    }
    let e = c.create_crew_thread("cat_miso", &ask("One more", "Do it.")).unwrap_err();
    assert!(e.starts_with("8 threads of your crew have not finished") && e.contains("Neko wakes you"), "{e}");
    // One ends: there is room for one more.
    let first = c.store.crew_of("cat_miso").unwrap().pop().unwrap();
    let now = c.now();
    c.store.set_task_status(&first.id, TaskStatus::Done, now).unwrap();
    c.create_crew_thread("cat_miso", &ask("One more", "Do it.")).unwrap();
    c.retire_cat("cat_tofu").unwrap();
    assert!(c.create_crew_thread("cat_tofu", &ask("X", "Y")).unwrap_err().starts_with("tofu is retired and takes no new crew"));
}

fn crew_wake(c: &mut Core, thread: &TaskId, crew: &TaskId, run: Option<&str>) -> WakeRow {
    WakeRow {
        id: c.new_id("wake"),
        task: thread.clone(),
        subagent: crew.clone(),
        number: String::new(),
        title: "Fix login".into(),
        status: "done".into(),
        summary: "Fixed.".into(),
        files: vec![],
        branch: None,
        created_ms: c.now(),
        run: run.map(|r| neko_protocol::RunId(r.into())),
        question: None,
        chain: vec![],
        crew: true,
        about: None,
    }
}

#[test]
fn a_crew_wake_is_kept_once_per_run_read_back_as_crew_and_counted_per_hour() {
    let d = dir();
    let mut c = open(&d);
    let (own, _, _) = c.cattain_thread("cat_miso").unwrap();
    let (t, _) = c.create_crew_thread("cat_miso", &ask("Fix login", "Fix it.")).unwrap();
    let w = crew_wake(&mut c, &own.id, &t.id, Some("run_1"));
    let now = c.now();
    c.store.insert_wake(&w, "pending", now).unwrap();
    assert!(c.store.crew_wake_for_run(&t.id, &neko_protocol::RunId("run_1".into())).unwrap());
    assert!(!c.store.crew_wake_for_run(&t.id, &neko_protocol::RunId("run_2".into())).unwrap());
    let pending = c.store.pending_wakes().unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].crew && pending[0].number.is_empty() && pending[0].subagent == t.id && pending[0].title == "Fix login");
    assert!(c.store.has_pending_wake(&own.id).unwrap(), "the Cattain's thread has news waiting");
    assert_eq!(c.store.crew_wakes_since(&own.id, 0).unwrap(), 0);
    let now = c.now();
    c.store.settle_wakes(&[w.id.clone()], "delivered", now).unwrap();
    let second = crew_wake(&mut c, &own.id, &t.id, Some("run_2"));
    let third = crew_wake(&mut c, &own.id, &t.id, Some("run_3"));
    let now = c.now();
    c.store.insert_wake(&second, "pending", now).unwrap();
    c.store.insert_wake(&third, "pending", now).unwrap();
    let later = c.now();
    c.store.settle_wakes(&[second.id, third.id], "delivered", later).unwrap();
    assert_eq!(c.store.crew_wakes_since(&own.id, 0).unwrap(), 2, "two wakes, the second carrying two crew ends");
    assert_eq!(c.store.crew_wakes_since(&own.id, later + 1).unwrap(), 0, "none in the hour after");
}

#[test]
fn a_version_18_store_opens_with_no_thread_started_by_a_cattain_and_every_wake_a_kittens() {
    let d = dir();
    let path = d.0.join("neko.sqlite3");
    {
        let s = Store::open_at(&path, 18).unwrap();
        s.connection().execute("INSERT OR IGNORE INTO workspaces (id, name, created_ms, position) VALUES ('default', 'General', 1, 0)", []).unwrap();
        s.connection()
            .execute("INSERT INTO tasks (id, workspace_id, words, title, status, created_ms, last_read_ms, activity_ms, latest_line) VALUES ('task_old', 'default', 'Old', 'Old', 'done', 1, 1, 1, '')", [])
            .unwrap();
        s.connection().execute("INSERT INTO wakes (id, task_id, subagent_id, status, summary, files, state, created_ms) VALUES ('wake_old', 'task_old', 'task_old', 'done', 'x', '[]', 'delivered', 1)", []).unwrap();
    }
    let c = open(&d);
    assert_eq!((c.store.version().unwrap(), SCHEMA_VERSION), (19, 19));
    assert_eq!(c.store.get_task(&TaskId("task_old".into())).unwrap().unwrap().started_by, None);
    let kind: String = c.store.connection().query_row("SELECT kind FROM wakes WHERE id = 'wake_old'", [], |r| r.get(0)).unwrap();
    assert_eq!(kind, "subagent");
    assert!(c.store.crew_requests().unwrap().is_empty());
}

//! Subagents in the store (docs/knowledge/subagents.md): numbering, depth and the live limit, the reports that wake a thread and how
//! they settle, what "Needs you" says about a subagent's request, and that a store from before them opens with nothing changed.

mod support;
use neko_core::conductor::{RunBook, Who};
use neko_core::needs::derive;
use neko_core::runtime::{Action, AgentEvent, Outcome};
use neko_core::store::SCHEMA_VERSION;
use neko_core::subagents::{self, MAX_DEPTH, MAX_LIVE_PER_THREAD, WakeRow};
use neko_core::{Core, FixedClock, Place, SequentialIds};
use neko_protocol::{Need, PermissionMode, Pricing, Task, TaskId, TaskStatus};
use support::Temp;

fn open(d: &Temp) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn number(t: &Task) -> String {
    t.subagent.as_ref().unwrap().number.clone()
}

#[test]
fn subagents_are_numbered_under_their_parent_and_belong_to_the_thread_at_the_top() {
    let d = Temp::new("sub-numbers");
    let mut core = open(&d);
    let (thread, _) = core.create_task("Fix login").unwrap();
    let (a, _) = core.create_subagent(&thread.id, "Check links", "Look at the links.").unwrap();
    let (b, _) = core.create_subagent(&thread.id, "Check images", "Look at the images.").unwrap();
    assert_eq!((number(&a), number(&b)), ("1".to_owned(), "2".to_owned()));
    assert_eq!(a.status, TaskStatus::New, "not started: queued");
    assert_eq!((a.words.as_str(), a.title.as_str()), ("Look at the links.", "Check links"), "the prompt is its words");
    assert_eq!((a.workspace.as_str(), a.cat.as_deref()), (thread.workspace.as_str(), thread.cat.as_deref()));
    let (c, _) = core.create_subagent(&a.id, "Read one file", "Read it.").unwrap();
    let (e, _) = core.create_subagent(&a.id, "Read another", "Read it.").unwrap();
    assert_eq!((number(&c), number(&e)), ("1.1".to_owned(), "1.2".to_owned()), "a dot for each level down");
    let of = c.subagent.as_ref().unwrap();
    assert_eq!((&of.parent, &of.root), (&a.id, &thread.id));
    assert_eq!(core.store.children_of(&thread.id).unwrap().len(), 2);
    assert_eq!(core.store.subagents_of(&thread.id).unwrap().len(), 4, "every level, under the thread");
    assert_eq!(core.store.queued_subagents().unwrap().len(), 4);
    assert!(thread.is_thread() && !a.is_thread());
    assert_eq!(a.subagent_name().as_deref(), Some("Kitten 1"));
    // They survive a reopen as they were.
    drop(core);
    let core = open(&d);
    assert_eq!(core.store.get_task(&e.id).unwrap().unwrap(), e);
}

#[test]
fn a_subagent_at_the_last_level_may_not_start_more_and_a_thread_has_a_ceiling() {
    let d = Temp::new("sub-limits");
    let mut core = open(&d);
    let (thread, _) = core.create_task("Fix login").unwrap();
    let mut parent = thread.id.clone();
    for _ in 0..MAX_DEPTH {
        parent = core.create_subagent(&parent, "Deeper", "Go deeper.").unwrap().0.id;
    }
    let why = core.create_subagent(&parent, "Too deep", "Go deeper.").unwrap_err();
    assert!(why.contains("1.1.1") && why.contains("Do the work yourself"), "{why}");
    let (top, _) = core.create_task("Another").unwrap();
    for i in 0..MAX_LIVE_PER_THREAD {
        core.create_subagent(&top.id, &format!("Part {i}"), "Do it.").unwrap();
    }
    assert!(core.create_subagent(&top.id, "One more", "Do it.").unwrap_err().contains("have not finished"));
    // An empty title or prompt, and a thread that is not there, are refused in words.
    assert!(core.create_subagent(&thread.id, "  ", "Do it.").unwrap_err().contains("title"));
    assert!(core.create_subagent(&thread.id, "Title", "  ").unwrap_err().contains("prompt"));
    assert!(core.create_subagent(&TaskId("task_nobody".into()), "T", "P").unwrap_err().contains("gone"));
}

fn wake(core: &mut Core, thread: &TaskId, sub: &Task, status: &str, summary: &str) -> WakeRow {
    WakeRow { id: core.new_id("wake"), task: thread.clone(), subagent: sub.id.clone(), number: number(sub), title: sub.title.clone(), status: status.into(), summary: summary.into(), files: vec!["src/a.rs +1 -0".into()], branch: Some("neko/x".into()), created_ms: core.now() }
}

#[test]
fn a_report_waits_until_it_is_read_and_a_subagent_that_ended_without_one_is_found() {
    let d = Temp::new("sub-wakes");
    let mut core = open(&d);
    let (thread, _) = core.create_task("Fix login").unwrap();
    let (a, _) = core.create_subagent(&thread.id, "Check links", "Look.").unwrap();
    let (b, _) = core.create_subagent(&thread.id, "Check images", "Look.").unwrap();
    for t in [&a, &b] {
        core.store.set_task_status(&t.id, TaskStatus::Done, 5).unwrap();
    }
    assert_eq!(core.store.ended_without_wake().unwrap().len(), 2, "ended, no report yet");
    let w = wake(&mut core, &thread.id, &a, "done", "Fixed two links.");
    core.store.insert_wake(&w, "pending", 6).unwrap();
    assert_eq!(core.store.ended_without_wake().unwrap().iter().map(number).collect::<Vec<_>>(), ["2"], "one has its report");
    assert!(core.store.has_pending_wake(&thread.id).unwrap());
    let pending = core.store.pending_wakes().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!((pending[0].number.as_str(), pending[0].title.as_str(), pending[0].branch.as_deref()), ("1", "Check links", Some("neko/x")));
    assert_eq!(pending[0].files, ["src/a.rs +1 -0"]);
    core.store.settle_wakes(&[w.id.clone()], "delivered", 7).unwrap();
    assert!(!core.store.has_pending_wake(&thread.id).unwrap() && core.store.pending_wakes().unwrap().is_empty());
    // Stopping a thread discards what waited.
    let w2 = wake(&mut core, &thread.id, &b, "done", "Fixed two images.");
    core.store.insert_wake(&w2, "pending", 8).unwrap();
    core.store.discard_wakes_of(&thread.id, 9).unwrap();
    assert!(core.store.pending_wakes().unwrap().is_empty());
    assert!(core.store.ended_without_wake().unwrap().is_empty(), "a discarded report still counts as made");
    // Removing the thread's task removes its reports.
    core.store.delete_wakes_of_task(&thread.id).unwrap();
}

#[test]
fn what_a_subagent_reports_is_its_last_words_in_its_newest_run() {
    let d = Temp::new("sub-final");
    let mut core = open(&d);
    let (thread, _) = core.create_task("Fix login").unwrap();
    let (a, _) = core.create_subagent(&thread.id, "Check links", "Look.").unwrap();
    assert_eq!(core.store.final_words(&a.id).unwrap(), "", "nothing said yet");
    let place = Place { workdir: d.0.to_string_lossy().into_owned(), ..Place::default() };
    let who = Who { id: "claude".into(), name: "Claude Code".into(), sign_in: String::new(), install: String::new(), pricing: Pricing::ApiEquivalent };
    let (mut book, _) = RunBook::begin(&mut core, &a.id, who, place, PermissionMode::Follow, None, None, None).unwrap();
    for words in ["First I looked. ", "Then I fixed two links."] {
        let now = core.now();
        book.on(&mut core, AgentEvent::TextDelta(words.into()), now).unwrap();
        let now = core.now();
        book.on(&mut core, AgentEvent::TextDone, now).unwrap();
    }
    let now = core.now();
    book.finish(&mut core, Outcome::Done, None, now).unwrap();
    assert_eq!(core.store.final_words(&a.id).unwrap(), "Then I fixed two links.");
}

#[test]
fn a_subagents_request_is_shown_under_its_threads_title_and_says_which_subagent() {
    let d = Temp::new("sub-needs");
    let mut core = open(&d);
    let (thread, _) = core.create_task("Fix login").unwrap();
    let (a, _) = core.create_subagent(&thread.id, "Run the tests", "Run them.").unwrap();
    let place = Place { workdir: d.0.to_string_lossy().into_owned(), ..Place::default() };
    let who = Who { id: "claude".into(), name: "Claude Code".into(), sign_in: String::new(), install: String::new(), pricing: Pricing::ApiEquivalent };
    let (mut book, _) = RunBook::begin(&mut core, &a.id, who, place, PermissionMode::Ask, None, None, None).unwrap();
    let now = core.now();
    book.on(&mut core, AgentEvent::PermissionAsked { id: "req1".into(), tool: "Bash".into(), action: Action::Command { command: "cargo test".into() }, tool_use: None }, now).unwrap();
    let items = derive(&core.store).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!((items[0].task.clone(), items[0].subagent.as_deref(), items[0].title.as_str()), (a.id.clone(), Some("1"), "Fix login"));
    assert!(matches!(items[0].need, Need::Permission { .. }));
    // Its question or failure is its thread's to read, not the person's to answer: only requests wait for them.
    core.store.set_task_status(&a.id, TaskStatus::Asked, 9).unwrap();
    core.store.set_task_status(&a.id, TaskStatus::Failed, 10).unwrap();
    assert_eq!(derive(&core.store).unwrap().iter().filter(|i| !matches!(i.need, Need::Permission { .. })).count(), 0);
}

#[test]
fn a_plugin_may_not_take_the_name_of_nekos_own_server() {
    let d = Temp::new("sub-name");
    let mut core = open(&d);
    let found = neko_protocol::plugins::parse_mcp_json(r#"{"mcpServers": {"neko": {"command": "node", "args": ["x.js"]}}}"#, None).unwrap();
    let why = core.add_plugins(found, None).unwrap_err().to_string();
    assert!(why.contains("Neko's own"), "{why}");
}

#[test]
fn the_migration_from_store_nine_adds_subagents_and_changes_nothing_else() {
    let d = Temp::new("sub-migrate");
    let path = d.0.join("neko.sqlite3");
    {
        let mut core = open(&d);
        core.create_task("Rename the sidebar label").unwrap();
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "DROP INDEX tasks_by_activity; DROP TRIGGER find_titles_insert; DROP TRIGGER find_titles_update; DROP TRIGGER find_titles_delete; DROP TRIGGER find_words_insert; DROP TRIGGER find_words_update; DROP TRIGGER find_words_delete; DROP TABLE find_titles; DROP TABLE find_words; ALTER TABLE tasks DROP COLUMN pinned_ms; ALTER TABLE tasks DROP COLUMN settled_ms; ALTER TABLE tasks DROP COLUMN archived_ms; ALTER TABLE tasks DROP COLUMN in_folder; ALTER TABLE tasks DROP COLUMN from_branch; DROP TABLE queued; DROP TABLE wakes; DROP INDEX tasks_by_parent; DROP INDEX tasks_by_root; \
             ALTER TABLE tasks DROP COLUMN parent_id; ALTER TABLE tasks DROP COLUMN sub_number; ALTER TABLE tasks DROP COLUMN root_id; PRAGMA user_version = 9;",
        )
        .unwrap();
    }
    let core = open(&d);
    assert_eq!(core.store.version().unwrap(), SCHEMA_VERSION);
    let tasks = core.store.all_tasks().unwrap();
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].subagent.is_none(), "an old task is a thread");
    assert!(core.store.pending_wakes().unwrap().is_empty());
}

#[test]
fn the_texts_the_agent_reads_do_not_change_between_runs_and_name_the_way_out() {
    assert_eq!(subagents::SERVER, "neko");
    let p = subagents::PARAGRAPH;
    for must in ["mcp__neko__start_subagent", "end your turn", "Do not poll or wait", "<neko-wake>", "cannot approve a subagent's steps", "Most work needs none"] {
        assert!(p.contains(must), "{must}");
    }
    assert!(subagents::instructions(None, Some(("1.2", "Fix login"))).contains("You are Subagent 1.2 of \"Fix login\""));
}

//! The Threads section of the sidebar (docs/knowledge/threads.md): what each row says in words, the empty and ungrouped shapes, View,
//! opening a thread reading it, and the order that keeps a working row in its place.

mod support;
use neko_protocol::{Change, Task, TaskId, ThreadStatus};
use support::*;

fn scene(name: &str) -> fn(&mut neko_app::state::App) {
    scenes().iter().find(|s| s.0 == name).unwrap().1
}

fn texts(rt: &rune::Runtime<neko_app::state::App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn keys(rt: &rune::Runtime<neko_app::state::App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.key).collect()
}

/// The accessibility description of the row of the thread whose title starts so.
fn described(rt: &rune::Runtime<neko_app::state::App>, title: &str) -> String {
    let all = nodes(&rt.accessibility_json());
    all.iter().find(|(_, n)| str_of(n, "role") == "list-box-option" && str_of(n, "name").starts_with(title)).map(|(_, n)| str_of(n, "description").to_owned()).unwrap_or_else(|| panic!("no row for {title}"))
}

#[test]
fn each_row_says_its_state_its_line_and_its_age_in_words() {
    let rt = runtime((1200.0, 800.0), false, scene("sparse-status"));
    let working = described(&rt, "Draft the release notes");
    assert!(working.contains("working"), "{working}");
    let failed = described(&rt, "Sync the changelog");
    assert!(failed.contains("failed") && failed.contains("Failed: Claude Code is not signed in."), "{failed}");
    let asks = described(&rt, "Find out why the nightly build");
    assert!(asks.contains("needs you") && asks.contains("Needs you: Run a command"), "{asks}");
    let unread = described(&rt, "What changed this week");
    assert!(unread.contains("unread"), "a finished thread nobody opened: {unread}");
    // The needs-you thread is first, then the rest by last activity.
    let t = texts(&rt);
    let from = t.iter().position(|x| x == "Threads").unwrap();
    let at = |s: &str| t.iter().skip(from).position(|x| x == s).unwrap_or_else(|| panic!("{s} in {t:?}"));
    assert!(at("Find out why the nightly build is slow") < at("Draft the release notes for 0.2") && at("Draft the release notes for 0.2") < at("Sync the changelog with the docs"), "{t:?}");
}

#[test]
fn no_row_is_led_by_a_cats_face() {
    let rt = runtime((1200.0, 800.0), false, scene("workspaces"));
    let k = keys(&rt);
    assert!(k.iter().any(|x| x.starts_with("thread-task_") && x.ends_with("-mark")), "a status mark leads each row");
    assert!(!k.iter().any(|x| x.starts_with("thread-task_") && x.ends_with("-face")), "no portrait on a thread row");
}

#[test]
fn with_no_thread_the_section_says_so_and_no_project_is_not_listed() {
    let rt = runtime((1200.0, 800.0), false, scene("sparse-empty"));
    assert!(texts(&rt).iter().any(|t| t == "Threads you start appear here"));
    assert!(!keys(&rt).iter().any(|k| k == "ws-default"), "an empty No project is not a group");
    assert!(keys(&rt).iter().any(|k| k == "ws-ws_acme"), "the project the person added is");
}

#[test]
fn view_groups_by_project_or_gives_one_list_naming_each_projects() {
    let mut rt = runtime((1200.0, 800.0), false, scene("workspaces"));
    assert!(keys(&rt).iter().any(|k| k == "ws-ws_acme"));
    click(&mut rt, "threads-view-button");
    assert!(rt.state.menu_open.as_deref() == Some("threads-view"));
    settle(&mut rt);
    for _ in 0..6 {
        rt.frame(0.1);
    }
    click(&mut rt, "threads-view-item3");
    assert_eq!(rt.state.grouping, neko_app::threads::Grouping::None);
    let k = keys(&rt);
    assert!(!k.iter().any(|x| x.starts_with("ws-ws_") && x.ends_with("-section")), "no group headings: {k:?}");
    assert!(k.iter().any(|x| x == "all-list"), "the one list");
    assert!(texts(&rt).iter().any(|t| t == "acme-api"), "each row names its project");
}

#[test]
fn the_ungrouped_scene_is_one_list() {
    let rt = runtime((1200.0, 800.0), true, scene("threads-ungrouped"));
    assert!(keys(&rt).iter().any(|x| x == "all-list"));
}

#[test]
fn opening_a_thread_reads_it_at_once() {
    let mut rt = runtime((1200.0, 800.0), false, scene("workspaces"));
    let id = rt.state.tasks.iter().find(|t| t.title.starts_with("Find out why the nightly build")).map(|t| t.id.clone()).unwrap();
    assert!(rt.state.tasks.iter().find(|t| t.id == id).unwrap().unread());
    assert!(described(&rt, "Find out why the nightly build").contains("unread"));
    click(&mut rt, &format!("thread-{}", id.0));
    assert!(!rt.state.tasks.iter().find(|t| t.id == id).unwrap().unread(), "opened, so read");
    assert!(!described(&rt, "Find out why the nightly build").contains("unread"));
}

#[test]
fn a_working_thread_keeps_its_place_while_it_streams_and_moves_when_it_ends() {
    let mut a = neko_app::state::App::default();
    let mk = |id: &str, activity: i64| {
        let mut t = Task::blank(TaskId(id.into()), "default", "w", id, 1, "cat_miso");
        t.activity_ms = activity;
        t.last_read_ms = activity;
        t
    };
    a.change(Change::TaskCreated { task: mk("older", 10) });
    a.change(Change::TaskCreated { task: mk("working", 20) });
    a.change(Change::TaskCreated { task: mk("newest", 30) });
    let order = |a: &neko_app::state::App| a.ordered(a.tasks.iter().collect()).iter().map(|t| t.id.0.clone()).collect::<Vec<_>>();
    assert_eq!(order(&a), ["newest", "working", "older"]);
    let mut w = mk("working", 20);
    w.thread_status = ThreadStatus::Working;
    w.status = neko_protocol::TaskStatus::Working;
    a.change(Change::TaskUpdated { task: w.clone() });
    // Another thread finishes after the working one began; the working one streams on past it.
    a.change(Change::TaskUpdated { task: mk("newest", 40) });
    w.activity_ms = 50;
    w.latest_line = "Reading the notes.".into();
    a.change(Change::TaskUpdated { task: w.clone() });
    assert_eq!(order(&a), ["newest", "working", "older"], "its words did not move its row");
    w.status = neko_protocol::TaskStatus::Done;
    w.thread_status = ThreadStatus::Idle;
    w.activity_ms = 60;
    a.change(Change::TaskUpdated { task: w });
    assert_eq!(order(&a), ["working", "newest", "older"], "the work ended: it takes the time it ended");
}

#[test]
fn what_arrives_in_the_open_thread_is_read_only_while_neko_is_in_front() {
    let mut a = neko_app::state::App::default();
    let mk = |activity: i64| {
        let mut t = Task::blank(TaskId("t".into()), "default", "w", "t", 1, "cat_miso");
        t.last_read_ms = 5;
        t.activity_ms = activity;
        t
    };
    a.change(Change::TaskCreated { task: mk(5) });
    a.open_task_id(&TaskId("t".into()));
    a.front = false;
    a.change(Change::TaskUpdated { task: mk(9) });
    assert!(a.tasks[0].unread(), "Neko is behind another app: the person did not see it");
    a.front = true;
    a.change(Change::TaskUpdated { task: mk(12) });
    assert!(!a.tasks[0].unread(), "in front, with the thread in view: read as it arrives");
}

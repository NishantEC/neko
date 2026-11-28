//! "Needs you" in the real daemon, with the fake CLI as `claude`: the list and its pushes for each kind of item, answering from the
//! list (a request, a question, and a stale answer refused), and the list after a restart. Every wait has a deadline.

mod support;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(30);

fn watcher(d: &Daemon) -> Client {
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    c
}

/// The pushed lists until one satisfies `done` (the deadline fails the test).
fn needs_until(c: &mut Client, what: &str, mut done: impl FnMut(&[NeedsItem]) -> bool) -> Vec<NeedsItem> {
    let end = Instant::now() + T;
    loop {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        if let Some(ServerMessage::Event { change: Change::NeedsChanged { items }, .. }) = c.next_event(Duration::from_millis(200)).unwrap()
            && done(&items)
        {
            return items;
        }
    }
}

fn list(c: &mut Client) -> Vec<NeedsItem> {
    match c.request(Request::ListNeeds).unwrap() {
        Reply::Needs { items } => items,
        r => panic!("list_needs: {r:?}"),
    }
}

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { crew: None, words: words.into(), workspace: None, in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

fn code(r: Reply) -> Option<ErrorCode> {
    match r {
        Reply::Error { code, .. } => Some(code),
        _ => None,
    }
}

#[test]
fn a_request_is_listed_pushed_and_answered_once() {
    let d = Daemon::start("needs-permission");
    let mut c = watcher(&d);
    assert!(list(&mut c).is_empty(), "a new install needs nothing");
    let task = create(&mut c, "Run the tests [fake:permission]");
    let items = needs_until(&mut c, "the request", |i| !i.is_empty());
    assert_eq!(items.len(), 1);
    let Need::Permission { permission } = &items[0].need else { panic!("{items:?}") };
    assert_eq!((items[0].task.clone(), items[0].title.as_str(), items[0].workspace.as_str()), (task.id.clone(), "Run the tests [fake:permission]", GENERAL));
    assert_eq!(list(&mut c), items, "the list asked for is the list pushed");
    // Answered from the list: the same request the task's own control sends.
    assert_eq!(c.request(Request::AnswerPermission { permission: permission.id.clone(), decision: Decision::AllowOnce }).unwrap(), Reply::Ok);
    needs_until(&mut c, "the request to leave the list", <[NeedsItem]>::is_empty);
    assert_eq!(code(c.request(Request::AnswerPermission { permission: permission.id.clone(), decision: Decision::Deny }).unwrap()), Some(ErrorCode::AlreadyAnswered), "a second answer is refused");
}

#[test]
fn a_question_is_answered_from_the_list_once() {
    let d = Daemon::start("needs-question");
    let mut c = watcher(&d);
    let task = create(&mut c, "Rename the button [fake:question]");
    let items = needs_until(&mut c, "the question", |i| !i.is_empty());
    let Need::Question { run, question, .. } = items[0].need.clone() else { panic!("{items:?}") };
    assert_eq!(question, "Should I rename all four to Preferences, or keep Settings?");
    let answer = |c: &mut Client, run: &RunId| c.request(Request::FollowUp { task: task.id.clone(), words: "All four.".into(), answering: Some(run.clone()), mode: None }).unwrap();
    assert_eq!(code(answer(&mut c, &RunId("run_another".into()))), Some(ErrorCode::AlreadyAnswered), "an answer to another run's question is stale");
    assert_eq!(answer(&mut c, &run), Reply::Ok);
    assert_eq!(code(answer(&mut c, &run)), Some(ErrorCode::AlreadyAnswered), "a second answer is refused, not sent to the agent");
    needs_until(&mut c, "the question to leave the list", <[NeedsItem]>::is_empty);
    let t = match c.request(Request::GetTranscript { task: task.id.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("{r:?}"),
    };
    let said: Vec<&str> = t.parts.iter().filter_map(|p| if let PartBody::Person { text } = &p.body { Some(text.as_str()) } else { None }).collect();
    assert_eq!(said, ["All four."], "the answer went once, as a follow-up");
}

#[test]
fn a_failure_to_act_on_is_listed_and_survives_a_restart() {
    let dir = temp_dir("needs-restart");
    let mut d = Daemon::start_in(dir.clone());
    d.keep_dir = true;
    let mut c = watcher(&d);
    let signin = create(&mut c, "Hi [fake:signin]");
    let crash = create(&mut c, "Hi [fake:crash]");
    let asked = create(&mut c, "Hi [fake:question]");
    let items = needs_until(&mut c, "a failure and a question", |i| i.len() == 2);
    assert!(items.iter().all(|i| i.task != crash.id), "a crash is a notice in the task, not something to act on: {items:?}");
    let failure = items.iter().find(|i| i.task == signin.id).expect("the sign-in failure");
    assert_eq!(failure.need, Need::Failure {
        failure: FailureKind::NotSignedIn,
        text: "Claude Code is not signed in.".into(),
        next: Some("Run `claude auth login` in Terminal, then send a follow-up.".into()),
        command: Some("claude auth login".into()),
    });
    assert!(items.iter().any(|i| i.task == asked.id));
    drop(c);
    assert!(d.stop().success());
    drop(d);
    let d = Daemon::start_in(dir.clone());
    let mut c = watcher(&d);
    assert_eq!(list(&mut c), items, "the same list after a restart");
    drop(c);
    drop(d);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_gone_before_the_run_is_listed() {
    let d = Daemon::start("needs-folder");
    let mut c = watcher(&d);
    let outside = temp_dir("needs-gone-folder");
    let notes = outside.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let ws = match c.request(Request::AddWorkspace { folder: notes.to_string_lossy().into_owned() }).unwrap() {
        Reply::Workspace { workspace, .. } => workspace.id,
        r => panic!("{r:?}"),
    };
    let task = match c.request(Request::CreateTask { crew: None, words: "Hi [fake:plain]".into(), workspace: Some(ws.clone()), in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("{r:?}"),
    };
    let end = Instant::now() + T;
    while !matches!(c.request(Request::GetTask { task: task.id.clone() }).unwrap(), Reply::Task { task } if task.status == TaskStatus::Done) {
        assert!(Instant::now() < end, "the first run to finish");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::fs::remove_dir_all(&outside).unwrap();
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "Again".into(), answering: None, mode: None }).unwrap(), Reply::Ok);
    let items = needs_until(&mut c, "the missing folder", |i| !i.is_empty());
    assert_eq!((items[0].workspace.as_str(), &items[0].task), (ws.as_str(), &task.id));
    assert!(matches!(&items[0].need, Need::Failure { failure: FailureKind::FolderMissing, command: None, .. }), "{items:?}");
}

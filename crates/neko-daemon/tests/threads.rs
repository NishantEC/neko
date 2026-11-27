//! Threads in the real daemon, with the fake CLI as `claude`: what a row's second line says as the words stream, the thread status and
//! the Needs you flag on the task, and `mark_read` / `mark_unread` and their push. Every wait has a deadline.

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

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { words: words.into(), workspace: None, cat: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

/// The pushed versions of `id` until one satisfies `done` (the deadline fails the test).
fn task_until(c: &mut Client, id: &TaskId, what: &str, mut done: impl FnMut(&Task) -> bool) -> Task {
    let end = Instant::now() + T;
    loop {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        if let Some(ServerMessage::Event { change: Change::TaskUpdated { task }, .. }) = c.next_event(Duration::from_millis(200)).unwrap()
            && task.id == *id
            && done(&task)
        {
            return task;
        }
    }
}

fn get(c: &mut Client, id: &TaskId) -> Task {
    match c.request(Request::GetTask { task: id.clone() }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("get: {r:?}"),
    }
}

fn mark(c: &mut Client, id: &TaskId, read: bool) -> Reply {
    c.request(if read { Request::MarkRead { task: id.clone() } } else { Request::MarkUnread { task: id.clone() } }).unwrap()
}

#[test]
fn a_finished_thread_is_unread_says_its_last_line_and_is_read_when_opened() {
    let d = Daemon::start("threads-read");
    let mut c = watcher(&d);
    let made = create(&mut c, "Fix the build");
    assert!(!made.unread(), "the person made it and is looking at it");
    let done = task_until(&mut c, &made.id, "it to finish", |t| t.status == TaskStatus::Done);
    assert_eq!((done.thread_status, done.needs_you), (ThreadStatus::Idle, false));
    assert!(done.unread(), "it finished; nobody has opened it since");
    assert!(!done.latest_line.is_empty(), "the agent's last line is kept: {done:?}");
    let Reply::Task { task: read } = mark(&mut c, &made.id, true) else { panic!() };
    assert!(!read.unread());
    let pushed = task_until(&mut c, &made.id, "the read to be pushed", |t| !t.unread());
    assert_eq!(pushed.last_read_ms, read.last_read_ms);
    // Reading it again changes nothing and pushes nothing.
    let Reply::Task { task: again } = mark(&mut c, &made.id, true) else { panic!() };
    assert_eq!(again.last_read_ms, read.last_read_ms);
    let Reply::Task { task: unread } = mark(&mut c, &made.id, false) else { panic!() };
    assert!(unread.unread());
    assert!(get(&mut c, &made.id).unread());
    assert!(matches!(mark(&mut c, &TaskId("task_nope".into()), true), Reply::Error { code: ErrorCode::NotFound, .. }));
}

#[test]
fn a_request_in_a_thread_is_working_with_the_needs_you_flag_and_a_failure_is_failed_until_opened() {
    let d = Daemon::start("threads-status");
    let mut c = watcher(&d);
    let asking = create(&mut c, "Run the tests [fake:permission]");
    let t = task_until(&mut c, &asking.id, "the request", |t| t.status == TaskStatus::NeedsYou);
    assert_eq!((t.thread_status, t.needs_you), (ThreadStatus::Working, true), "the agent waits, still working, and the person is needed");
    let crashed = create(&mut c, "Hi [fake:crash]");
    let f = task_until(&mut c, &crashed.id, "the crash", |t| t.status == TaskStatus::Failed);
    assert_eq!((f.thread_status, f.needs_you), (ThreadStatus::Failed, false));
    assert!(!f.failure_line.is_empty());
    mark(&mut c, &crashed.id, true);
    assert_eq!(get(&mut c, &crashed.id).thread_status, ThreadStatus::Idle, "opened since it failed");
    let signin = create(&mut c, "Hi [fake:signin]");
    let s = task_until(&mut c, &signin.id, "the sign-in failure", |t| t.status == TaskStatus::Failed);
    assert!(s.needs_you, "a failure to act on is on the Needs you list, so it is flagged");
}

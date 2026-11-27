//! Pin, settle and archive in the real daemon, with the fake CLI as `claude` (docs/knowledge/threads.md): the requests and their
//! refusals, every screen hearing the change, a message waking an archived thread, a settled one coming back, the Needs you list
//! leaving an archived thread out, and the week's housekeeping, moved by the debug clock. Every wait has a deadline.

mod support;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(30);
const DAY_MIN: i64 = 24 * 60;

fn daemon(name: &str) -> Daemon {
    Daemon::start_with(temp_dir(name), &[("NEKO_DEBUG_CLOCK", "1")])
}

fn screen(d: &Daemon) -> Client {
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    c
}

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { words: words.into(), workspace: None, in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

/// The pushed versions of `id` until one satisfies `done` (the deadline fails the test).
fn heard(c: &mut Client, id: &TaskId, what: &str, mut done: impl FnMut(&Task) -> bool) -> Task {
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

fn finished(c: &mut Client, words: &str) -> Task {
    let t = create(c, words);
    heard(c, &t.id, "it to finish", |t| t.status == TaskStatus::Done)
}

fn get(c: &mut Client, id: &TaskId) -> Task {
    match c.request(Request::GetTask { task: id.clone() }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("get: {r:?}"),
    }
}

fn put(c: &mut Client, r: Request) -> Task {
    match c.request(r).unwrap() {
        Reply::Task { task } => task,
        r => panic!("expected a task: {r:?}"),
    }
}

fn pin(c: &mut Client, id: &TaskId, on: bool) -> Task {
    put(c, Request::PinTask { task: id.clone(), pinned: on })
}

fn settle(c: &mut Client, id: &TaskId, on: bool) -> Task {
    put(c, Request::SettleTask { task: id.clone(), settled: on })
}

fn archive(c: &mut Client, id: &TaskId, on: bool) -> Task {
    put(c, Request::ArchiveTask { task: id.clone(), archived: on })
}

fn code(r: Reply) -> Option<ErrorCode> {
    match r {
        Reply::Error { code, .. } => Some(code),
        _ => None,
    }
}

#[test]
fn a_thread_is_pinned_settled_and_archived_and_every_screen_hears_it() {
    let d = daemon("shelf-requests");
    let mut c = screen(&d);
    let mut other = screen(&d);
    let t = finished(&mut c, "Fix the build [fake:plain]");
    assert!(pin(&mut c, &t.id, true).pinned);
    let said = heard(&mut other, &t.id, "the pin", |t| t.pinned);
    assert!(!said.settled && !said.archived);
    // Settling a pinned thread unpins it; pinning a settled one brings it back.
    let s = settle(&mut c, &t.id, true);
    assert_eq!((s.pinned, s.settled), (false, true));
    heard(&mut other, &t.id, "the settle", |t| t.settled && !t.pinned);
    let p = pin(&mut c, &t.id, true);
    assert_eq!((p.pinned, p.settled), (true, false));
    // Doing what is already so changes nothing.
    assert!(pin(&mut c, &t.id, true).pinned);
    let a = archive(&mut c, &t.id, true);
    assert!(a.archived && a.pinned, "archiving keeps the pin, so restoring is exact");
    // The list holds it still, flagged: the screen decides what to show. Restoring needs no run.
    match c.request(Request::ListTasks).unwrap() {
        Reply::Tasks { tasks } => assert!(tasks.iter().any(|x| x.id == t.id && x.archived)),
        r => panic!("{r:?}"),
    }
    let back = archive(&mut c, &t.id, false);
    assert_eq!((back.archived, back.pinned, back.status), (false, true, TaskStatus::Done));
    assert_eq!(get(&mut c, &t.id).pinned, true);
    assert_eq!(code(c.request(Request::PinTask { task: TaskId("task_nope".into()), pinned: true }).unwrap()), Some(ErrorCode::NotFound));
}

#[test]
fn a_thread_that_works_cannot_be_settled_or_archived_but_may_be_pinned() {
    let d = daemon("shelf-working");
    let mut c = screen(&d);
    let t = create(&mut c, "Take your time [fake:slow]");
    heard(&mut c, &t.id, "it to work", |t| t.thread_status == ThreadStatus::Working);
    assert_eq!(code(c.request(Request::SettleTask { task: t.id.clone(), settled: true }).unwrap()), Some(ErrorCode::Refused));
    assert_eq!(code(c.request(Request::ArchiveTask { task: t.id.clone(), archived: true }).unwrap()), Some(ErrorCode::Refused));
    assert!(pin(&mut c, &t.id, true).pinned);
    assert!(!get(&mut c, &t.id).settled && !get(&mut c, &t.id).archived);
    assert_eq!(c.request(Request::StopTask { task: t.id.clone() }).unwrap(), Reply::Ok);
    heard(&mut c, &t.id, "it to stop", |t| t.status == TaskStatus::Stopped);
    // Stopped, it may be archived.
    assert!(archive(&mut c, &t.id, true).archived);
}

#[test]
fn a_message_wakes_an_archived_thread_and_brings_a_settled_one_back() {
    let d = daemon("shelf-wake");
    let mut c = screen(&d);
    let t = finished(&mut c, "Fix the build [fake:plain]");
    assert!(settle(&mut c, &t.id, true).settled);
    let Reply::Ok = c.request(Request::FollowUp { task: t.id.clone(), words: "And the tests [fake:plain]".into(), answering: None }).unwrap() else { panic!("follow-up") };
    let woken = heard(&mut c, &t.id, "the run to bring it back", |t| !t.settled);
    assert!(!woken.archived);
    heard(&mut c, &t.id, "it to finish again", |t| t.status == TaskStatus::Done);

    assert!(archive(&mut c, &t.id, true).archived);
    let Reply::Ok = c.request(Request::FollowUp { task: t.id.clone(), words: "One more thing [fake:plain]".into(), answering: None }).unwrap() else { panic!("follow-up") };
    let woken = heard(&mut c, &t.id, "the message to wake it", |t| !t.archived);
    assert_eq!(woken.status, TaskStatus::Working, "it runs");
    heard(&mut c, &t.id, "it to finish", |t| t.status == TaskStatus::Done);
}

#[test]
fn an_archived_thread_is_not_waiting_for_the_person() {
    let d = daemon("shelf-needs");
    let mut c = screen(&d);
    let t = create(&mut c, "Rename the button [fake:question]");
    heard(&mut c, &t.id, "its question", |t| t.status == TaskStatus::Asked);
    let items = |c: &mut Client| match c.request(Request::ListNeeds).unwrap() {
        Reply::Needs { items } => items,
        r => panic!("{r:?}"),
    };
    let until = |c: &mut Client, what: &str, n: usize| {
        let end = Instant::now() + T;
        while items(c).len() != n {
            assert!(Instant::now() < end, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    until(&mut c, "the question to be listed", 1);
    assert!(get(&mut c, &t.id).needs_you);
    assert!(archive(&mut c, &t.id, true).archived);
    until(&mut c, "the archived thread to leave the list", 0);
    assert!(!get(&mut c, &t.id).needs_you, "its flag goes with it");
    archive(&mut c, &t.id, false);
    until(&mut c, "the restored question to be the person's again", 1);
    assert!(get(&mut c, &t.id).needs_you);
}

#[test]
fn the_housekeeping_settles_a_thread_left_alone_for_a_week_and_leaves_the_rest() {
    let d = daemon("shelf-auto");
    let mut c = screen(&d);
    let idle = finished(&mut c, "Idle [fake:plain]");
    let pinned = finished(&mut c, "Pinned [fake:plain]");
    let asking = create(&mut c, "Run the tests [fake:permission]");
    heard(&mut c, &asking.id, "the request", |t| t.status == TaskStatus::NeedsYou);
    pin(&mut c, &pinned.id, true);
    // Six days on: nothing yet. The settle, if it came, would be pushed.
    assert_eq!(c.request(Request::DebugClock { offset_ms: Some(6 * DAY_MIN * 60_000) }).unwrap(), Reply::Ok);
    std::thread::sleep(Duration::from_millis(600));
    assert!(!get(&mut c, &idle.id).settled, "six days is not a week");
    // Eight days on: the idle one is settled and pushed; the pinned one and the one that needs the person are not.
    assert_eq!(c.request(Request::DebugClock { offset_ms: Some(8 * DAY_MIN * 60_000) }).unwrap(), Reply::Ok);
    let s = heard(&mut c, &idle.id, "the housekeeping to settle it", |t| t.settled);
    assert!(!s.pinned && !s.archived);
    assert!(!get(&mut c, &pinned.id).settled, "a pinned thread is never settled by itself");
    let a = get(&mut c, &asking.id);
    assert!(!a.settled && a.needs_you, "a thread that needs the person is left alone");
}

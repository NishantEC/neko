//! A clean stop (SIGTERM) never waits on the person and loses nothing they saw: with a request pending and a run streaming, the
//! daemon exits within a second, nothing it started outlives it, the streamed words are in the store after a restart, and the runs
//! it cut off are interrupted with a notice that says nothing was lost. Real processes, bounded deadlines.

mod support;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(30);

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { words: words.into(), workspace: None, cat: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

fn transcript(c: &mut Client, task: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: task.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

/// Every process below `root`.
fn below(root: u32) -> Vec<i32> {
    let out = std::process::Command::new("ps").args(["-axo", "pid=,ppid="]).output().unwrap();
    let rows: Vec<(i32, i32)> = String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| {
        let mut w = l.split_whitespace();
        Some((w.next()?.parse().ok()?, w.next()?.parse().ok()?))
    }).collect();
    let mut found = Vec::new();
    let mut frontier = vec![root as i32];
    while let Some(p) = frontier.pop() {
        for &(pid, ppid) in &rows {
            if ppid == p && !found.contains(&pid) {
                found.push(pid);
                frontier.push(pid);
            }
        }
    }
    found
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[test]
fn sigterm_with_a_request_pending_and_words_streaming_stops_within_a_second_and_keeps_every_word() {
    let dir = temp_dir("shutdown");
    let mut d = Daemon::start_in(dir.clone());
    d.keep_dir = true;
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    let s = Settings { runtime: "claude".into(), permission_mode: PermissionMode::Ask, own_mcp: false };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });

    // One task waits on the person; another streams words.
    let asking = create(&mut c, "Run the tests [fake:hook-shell]");
    let streaming = create(&mut c, "Count slowly [fake:slow]");
    assert!(matches!(c.request(Request::WatchTask { task: Some(streaming.id.clone()) }).unwrap(), Reply::Ok));
    let end = Instant::now() + T;
    let (mut pending, mut seen) = (false, String::new());
    while !(pending && seen.contains("slow3")) {
        assert!(Instant::now() < end, "timed out: pending {pending}, words {seen:?}");
        match c.next_event(Duration::from_millis(100)).unwrap() {
            Some(ServerMessage::Event { change: Change::PermissionChanged { permission }, .. }) if permission.task == asking.id && permission.state == PermissionState::Pending => pending = true,
            Some(ServerMessage::Event { change: Change::PartDelta { task, text, .. }, .. }) if task == streaming.id => seen.push_str(&text),
            _ => {}
        }
    }
    let started = below(d.child.id());
    assert!(!started.is_empty(), "the runs' guardians and CLIs are running");
    drop(c);

    // SIGTERM: out within a second, and nothing it started is left.
    let t = Instant::now();
    d.signal(libc::SIGTERM);
    loop {
        if d.child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(1), "the daemon took over a second to stop with a request pending");
        std::thread::sleep(Duration::from_millis(10));
    }
    let gone_by = Instant::now() + Duration::from_secs(3);
    while started.iter().any(|&p| alive(p)) && Instant::now() < gone_by {
        std::thread::sleep(Duration::from_millis(20));
    }
    let left: Vec<i32> = started.iter().copied().filter(|&p| alive(p)).collect();
    for &p in &left {
        // SAFETY: a process this test's daemon started, by its PID (cleanup only; the assert below fails the test).
        unsafe { libc::kill(p, libc::SIGKILL) };
    }
    assert!(left.is_empty(), "outlived the daemon: {left:?}");

    // After a restart: every word streamed before the stop is in the store, and both runs say they were cut off by a quit.
    let d2 = Daemon::start_in(dir);
    let mut c = d2.client();
    let t = transcript(&mut c, &streaming.id);
    let stored: String = t.parts.iter().filter_map(|p| if let PartBody::Agent { text, .. } = &p.body { Some(text.as_str()) } else { None }).collect();
    let shown = seen.trim_end();
    assert!(stored.contains(shown), "the store lost words the person saw: stored {stored:?}, seen {shown:?}");
    assert_eq!(t.runs.last().unwrap().status, RunStatus::Interrupted);
    for id in [&streaming.id, &asking.id] {
        let t = transcript(&mut c, id);
        assert!(t.parts.iter().any(|p| matches!(&p.body, PartBody::Notice { text, .. } if text.contains("Neko was quit while this was running"))), "the notice says it was a quit");
        assert!(t.permissions.iter().all(|p| p.state != PermissionState::Pending), "no request is left pending");
    }
}

/// A connection busy with something that never ends must not hold the stop. Seen on a Mac: Settings asked for the runtimes, the
/// daemon read a Claude Code settings file inside a protected folder, macOS held that read for its privacy prompt, and the
/// daemon then ignored SIGTERM for an hour because the stop waited for that connection. Here the never-ending read is a settings
/// file that is a FIFO with no writer.
#[test]
fn sigterm_while_a_connection_is_stuck_in_a_read_still_stops_within_two_seconds() {
    let dir = temp_dir("shutdown-stuck");
    let fifo = home_of(&dir).join(".claude/settings.json");
    let path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    // SAFETY: makes a FIFO at a path inside this test's own temporary folder.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0, "mkfifo");
    let mut d = Daemon::start_in(dir);
    let c = d.client();
    // Settings' list reads that file: the request never gets its answer.
    let asking = std::thread::spawn(move || {
        let mut c = c;
        let _ = c.request(Request::ListRuntimes);
    });
    std::thread::sleep(Duration::from_millis(500));
    assert!(!asking.is_finished(), "the read is stuck, as the privacy prompt held it");

    let t = Instant::now();
    d.signal(libc::SIGTERM);
    loop {
        if d.child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(2), "the daemon waited on a stuck connection instead of stopping");
        std::thread::sleep(Duration::from_millis(10));
    }
}

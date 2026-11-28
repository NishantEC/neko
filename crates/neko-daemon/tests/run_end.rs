//! When a run ends (docs/knowledge/runtimes.md, "When a run ends"; composer.md), in the real daemon with the fake CLI as `claude`: a
//! CLI keeps running a moment after the `result` that ended its run (the real one ends its MCP servers first), and words sent in that
//! moment cannot reach it. They are held as a queued message and start the next run as soon as this one is released, whatever mode
//! they were sent in; and a CLI that does not exit at all is ended through the run guard after a grace, its run done, so a finished
//! run never holds the thread. Every wait has a deadline.

mod support;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use serde_json::Value;
use support::*;

const T: Duration = Duration::from_secs(60);

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + T;
    while !ok() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn rig(name: &str, env: &[(&str, &str)]) -> (Daemon, PathBuf) {
    let dir = temp_dir(name);
    let record = dir.join("record.jsonl");
    let r = record.to_string_lossy().into_owned();
    let mut all = vec![("NEKO_FAKE_RECORD", r.as_str())];
    all.extend_from_slice(env);
    (Daemon::start_with(dir, &all), record)
}

fn client(d: &Daemon) -> Client {
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    c
}

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { crew: None, words: words.into(), workspace: None, in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

fn transcript(c: &mut Client, id: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: id.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn follow_up(c: &mut Client, id: &TaskId, words: &str, mode: SendMode) -> Reply {
    c.request(Request::FollowUp { task: id.clone(), words: words.into(), answering: None, mode: Some(mode) }).unwrap()
}

/// Waits for the moment after the run's `result` (its usage is in) while the run is still going: the CLI is exiting.
fn result_in(c: &mut Client, id: &TaskId) -> Instant {
    let end = Instant::now() + T;
    loop {
        assert!(Instant::now() < end, "no result");
        if let Some(ServerMessage::Event { change: Change::RunChanged { run }, .. }) = c.next_event(Duration::from_millis(50)).unwrap()
            && &run.task == id
            && run.usage.is_some()
        {
            assert_eq!(run.status, RunStatus::Running, "the run is still going while the CLI exits");
            return Instant::now();
        }
    }
}

fn recorded(record: &Path) -> Vec<Value> {
    std::fs::read_to_string(record).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// The words of every `user` line the fake CLIs read, in order.
fn user_lines(record: &Path) -> Vec<String> {
    recorded(record).iter().filter(|v| v["stdin"]["type"] == "user").filter_map(|v| v["stdin"]["message"]["content"][0]["text"].as_str().map(str::to_owned)).collect()
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[test]
fn words_sent_while_the_cli_exits_after_its_result_wait_and_start_the_next_run_in_every_mode() {
    let (d, record) = rig("run-end-window", &[("NEKO_FAKE_LINGER_MS", "1500")]);
    let mut c = client(&d);
    for mode in [SendMode::Steer, SendMode::Interrupt, SendMode::Queue] {
        let t = create(&mut c, &format!("First ask ({mode:?}) [fake:plain]"));
        result_in(&mut c, &t.id);
        let words = format!("And again ({mode:?}) [fake:plain]");
        // Before, a steer here was refused with "Claude Code has finished this turn." and the words went nowhere.
        assert_eq!(follow_up(&mut c, &t.id, &words, mode), Reply::Ok, "{mode:?}: the words are taken");
        let tr = transcript(&mut c, &t.id);
        assert_eq!(tr.runs.len(), 1, "{mode:?}: no run started yet");
        assert_eq!(tr.queued.iter().map(|q| (q.words.as_str(), q.kind)).collect::<Vec<_>>(), [(words.as_str(), QueuedKind::Queued)], "{mode:?}: held as a queued message");
        until("the next run to take the words and end", || {
            let tr = transcript(&mut c, &t.id);
            tr.task.status == TaskStatus::Done && tr.runs.len() == 2 && tr.runs.iter().all(|r| r.status == RunStatus::Done) && tr.queued.is_empty()
        });
        let tr = transcript(&mut c, &t.id);
        assert!(tr.parts.iter().any(|p| p.run.as_ref() == Some(&tr.runs[1].id) && matches!(&p.body, PartBody::Person { text } if text == &words)), "{mode:?}: the words open the next run");
        assert_eq!(tr.runs[0].reason, None, "{mode:?}: the first run ended as it was, not interrupted or stopped");
        assert_eq!(user_lines(&record).iter().filter(|w| *w == &words).count(), 1, "{mode:?}: the words reached a CLI once");
    }
}

#[test]
fn a_cli_that_does_not_exit_after_its_result_is_ended_after_the_grace_and_the_words_sent_meanwhile_go_on() {
    let (d, record) = rig("run-end-grace", &[("NEKO_FAKE_LINGER_MS", "120000"), ("NEKO_RUN_END_GRACE_MS", "1500")]);
    let mut c = client(&d);
    let t = create(&mut c, "Make a Cattain for the iOS work [fake:plain]");
    let at = result_in(&mut c, &t.id);
    assert_eq!(follow_up(&mut c, &t.id, "Make it again [fake:plain]", SendMode::Steer), Reply::Ok, "the words are taken");
    let first_done = {
        let mut c2 = client(&d);
        until("the first run to end", || transcript(&mut c2, &t.id).runs.first().is_some_and(|r| r.status != RunStatus::Running));
        at.elapsed()
    };
    assert!(first_done >= Duration::from_millis(1200) && first_done < Duration::from_secs(10), "released about the grace after its result ({first_done:?})");
    let tr = transcript(&mut c, &t.id);
    assert_eq!((tr.runs[0].status, tr.runs[0].reason.as_deref()), (RunStatus::Done, None), "its result said done: done, not stopped or failed");
    assert!(tr.parts.iter().any(|p| p.run.as_ref() == Some(&tr.runs[0].id) && matches!(&p.body, PartBody::Agent { text, .. } if text.starts_with("Hello from the fake"))), "its answer is kept");
    // The words sent meanwhile started the next run, which ends the same way.
    until("the next run to take the words and end", || {
        let tr = transcript(&mut c, &t.id);
        tr.task.status == TaskStatus::Done && tr.runs.len() == 2 && tr.runs.iter().all(|r| r.status == RunStatus::Done)
    });
    assert_eq!(user_lines(&record).iter().filter(|w| w.as_str() == "Make it again [fake:plain]").count(), 1);
    // Nothing the runs started is left running.
    let pids: Vec<i32> = recorded(&record).iter().filter_map(|v| v["pid"].as_i64()).map(|p| p as i32).collect();
    assert_eq!(pids.len(), 2, "both CLIs lingered: {pids:?}");
    until("the lingering CLIs to be gone", || pids.iter().all(|p| !alive(*p)));
}

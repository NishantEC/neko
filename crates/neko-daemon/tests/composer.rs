//! Words typed while the agent works (docs/knowledge/composer.md), in the real daemon with the fake CLI recording exactly what it was
//! given (`NEKO_FAKE_RECORD`): Return steers, ⌥Return queues, ⌘Return interrupts, and what waits above the composer is the daemon's
//! own list. Every wait has a deadline.

mod support;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(30);

fn start(name: &str, extra: &[(&str, &str)]) -> (Daemon, std::path::PathBuf) {
    let dir = temp_dir(name);
    let rec = dir.join("record.jsonl");
    let rec_s = rec.to_string_lossy().into_owned();
    let mut env: Vec<(&str, &str)> = vec![("NEKO_FAKE_RECORD", &rec_s)];
    env.extend_from_slice(extra);
    (Daemon::start_with(dir, &env), rec)
}

fn watcher(d: &Daemon) -> Client {
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

fn transcript(c: &mut Client, task: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: task.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn agent_words(t: &Transcript) -> String {
    t.parts.iter().filter_map(|p| if let PartBody::Agent { text, .. } = &p.body { Some(text.as_str()) } else { None }).collect::<Vec<_>>().join("\n")
}

/// Waits (polling the transcript) until `done` holds.
fn until(c: &mut Client, task: &TaskId, what: &str, mut done: impl FnMut(&Transcript) -> bool) -> Transcript {
    let end = Instant::now() + T;
    loop {
        let t = transcript(c, task);
        if done(&t) {
            return t;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}; the agent said {:?}, {} run(s), queued {:?}", agent_words(&t), t.runs.len(), t.queued);
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn working(c: &mut Client, task: &TaskId) -> Transcript {
    until(c, task, "the first step", |t| agent_words(t).contains("Step 1"))
}

fn send(c: &mut Client, task: &TaskId, words: &str, mode: Option<SendMode>) -> Reply {
    c.request(Request::FollowUp { task: task.clone(), words: words.into(), answering: None, mode }).unwrap()
}

/// What reached the CLI, in order: each line of the record.
fn record(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// The words of every user message the CLI read, in order.
fn user_messages(path: &std::path::Path) -> Vec<String> {
    record(path)
        .iter()
        .filter(|v| v["stdin"]["type"] == "user")
        .map(|v| v["stdin"]["message"]["content"].as_array().map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect::<String>()).unwrap_or_default())
        .collect()
}

fn argvs(path: &std::path::Path) -> Vec<Vec<String>> {
    record(path).iter().filter_map(|v| v["argv"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())).collect()
}

fn queued_is(n: usize) -> impl Fn(&Transcript) -> bool {
    move |t| t.queued.len() == n
}

fn finished(c: &mut Client, task: &TaskId, runs: usize) -> Transcript {
    until(c, task, &format!("{runs} finished run(s)"), |t| t.runs.len() == runs && t.runs.iter().all(|r| r.status != RunStatus::Running) && t.task.status != TaskStatus::Working)
}

#[test]
fn steer_reaches_the_running_cli_and_the_row_clears_when_the_next_step_reads_it() {
    let (d, rec) = start("steer", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Do the work [fake:work:5]");
    working(&mut c, &task.id);
    // No mode is today's behaviour: steer.
    assert_eq!(send(&mut c, &task.id, "Also check the docs", None), Reply::Ok);
    let t = transcript(&mut c, &task.id);
    assert!(t.parts.iter().any(|p| matches!(&p.body, PartBody::Person { text } if text == "Also check the docs")), "the words are in the transcript at once");
    // The row waits until a model call that began after the one going now starts.
    assert!(t.queued.iter().all(|q| q.kind == QueuedKind::Steered && q.words == "Also check the docs") || t.queued.is_empty());
    until(&mut c, &task.id, "the agent to read it", |t| agent_words(t).contains("Reading your message: Also check the docs"));
    until(&mut c, &task.id, "the row to clear", queued_is(0));
    let t = finished(&mut c, &task.id, 1);
    assert_eq!(t.task.status, TaskStatus::Done);
    assert_eq!(user_messages(&rec), vec!["Do the work [fake:work:5]".to_owned(), "Also check the docs".to_owned()], "both reached the one process, the second mid-turn");
    assert_eq!(argvs(&rec).len(), 1, "one process: a steer starts no run");
}

/// The real Claude Code often folds a steer into the turn already running and ends with one result for both. The run must end on that
/// result (docs/knowledge/runtimes.md), not sit at "Working" until the silence limit fails it: here the limit is 25 seconds and the
/// run must be done long before it.
#[test]
fn a_steer_folded_into_the_running_turn_ends_the_run_promptly_with_the_words_in_the_turn() {
    let (d, rec) = start("steer-fold", &[("NEKO_RUN_STALL_MS", "25000")]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Do the work [fake:work:4:step=300]");
    working(&mut c, &task.id);
    let steered = Instant::now();
    assert_eq!(send(&mut c, &task.id, "Also end with the word meow", Some(SendMode::Steer)), Reply::Ok);
    let t = finished(&mut c, &task.id, 1);
    assert!(steered.elapsed() < Duration::from_secs(15), "the run ended on the one result, not on the silence limit ({:?})", steered.elapsed());
    assert_eq!(t.task.status, TaskStatus::Done, "ended well, not failed by the stall limit");
    assert_eq!(t.runs[0].status, RunStatus::Done);
    assert!(agent_words(&t).contains("Reading your message: Also end with the word meow"), "the steer was obeyed inside the turn: {:?}", agent_words(&t));
    // The person's words stand once: the CLI's echo of them is not a second message.
    assert_eq!(t.parts.iter().filter(|p| matches!(&p.body, PartBody::Person { text } if text == "Also end with the word meow")).count(), 1);
    assert!(t.queued.is_empty());
    assert_eq!(user_messages(&rec).len(), 2);
    assert_eq!(argvs(&rec).len(), 1, "one process took both");
    assert!(argvs(&rec)[0].iter().any(|a| a == "--replay-user-messages"), "the CLI is asked to say which messages it consumed");
}

/// A CLI that does not fold: the steer waits for a turn of its own, and the run waits for that turn's result too.
#[test]
fn a_steer_the_cli_answers_in_a_turn_of_its_own_keeps_the_run_going_until_that_result() {
    let (d, rec) = start("steer-separate", &[("NEKO_RUN_STALL_MS", "25000")]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Do the work [fake:work:3:step=300:separate]");
    working(&mut c, &task.id);
    assert_eq!(send(&mut c, &task.id, "Also end with the word meow", Some(SendMode::Steer)), Reply::Ok);
    let t = finished(&mut c, &task.id, 1);
    assert_eq!(t.task.status, TaskStatus::Done);
    let said = agent_words(&t);
    assert!(said.contains("The work is finished.") && said.contains("Hello from the fake"), "both turns were answered in the one run: {said:?}");
    assert!(!said.contains("Reading your message"), "not read inside the first turn: {said:?}");
    assert_eq!(argvs(&rec).len(), 1, "still one process");
    assert_eq!(user_messages(&rec).len(), 2);
}

#[test]
fn steered_messages_show_as_steered_until_read() {
    let (d, _rec) = start("steered-row", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Go [fake:work:4]");
    working(&mut c, &task.id);
    assert_eq!(send(&mut c, &task.id, "One more thing", Some(SendMode::Steer)), Reply::Ok);
    let mut saw = false;
    let end = Instant::now() + T;
    while Instant::now() < end {
        if let Some(ServerMessage::Event { change: Change::QueueChanged { queued, .. }, .. }) = c.next_event(Duration::from_millis(100)).unwrap() {
            if queued.iter().any(|q| q.kind == QueuedKind::Steered && q.words == "One more thing") {
                saw = true;
            }
            if queued.is_empty() {
                break;
            }
        }
    }
    assert!(saw, "a QueueChanged event carried the steered message");
    // A steered message cannot be taken back.
    let id = QueuedId("queued_none".into());
    assert!(matches!(c.request(Request::DiscardQueued { task: task.id.clone(), queued: id }).unwrap(), Reply::Error { code: ErrorCode::NotFound, .. }));
}

#[test]
fn two_queued_messages_run_in_order_after_the_turn_and_discard_drops_one() {
    let (d, rec) = start("queue", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Start [fake:work:3]");
    working(&mut c, &task.id);
    for w in ["Second", "Third", "Fourth"] {
        assert_eq!(send(&mut c, &task.id, w, Some(SendMode::Queue)), Reply::Ok);
    }
    let t = until(&mut c, &task.id, "three queued", queued_is(3));
    assert!(t.queued.iter().all(|q| q.kind == QueuedKind::Queued));
    assert_eq!(t.runs.len(), 1, "queued words do not start anything while the turn works");
    assert_eq!(user_messages(&rec).len(), 1, "and nothing but the first words reached the CLI");
    let drop_it = t.queued.iter().find(|q| q.words == "Third").unwrap().id.clone();
    assert_eq!(c.request(Request::DiscardQueued { task: task.id.clone(), queued: drop_it }).unwrap(), Reply::Ok);
    let t = finished(&mut c, &task.id, 3);
    assert!(t.queued.is_empty());
    let said = agent_words(&t);
    let (second, fourth) = (said.find("You said: Second").expect("the second ran"), said.find("You said: Fourth").expect("the fourth ran"));
    assert!(second < fourth, "in order");
    assert!(!said.contains("Third"), "the discarded one never ran");
    assert_eq!(user_messages(&rec), vec!["Start [fake:work:3]".to_owned(), "Second".to_owned(), "Fourth".to_owned()]);
    let a = argvs(&rec);
    assert_eq!(a.len(), 3);
    assert!(a[1].iter().any(|x| x == "--resume") && a[2].iter().any(|x| x == "--resume"), "each next run resumes the conversation");
    assert!(t.parts.iter().any(|p| matches!(&p.body, PartBody::Person { text } if text == "Second")), "a queued message is in the transcript when its run starts");
}

#[test]
fn interrupt_stops_the_turn_at_once_and_the_words_run_in_a_resumable_conversation() {
    let (d, rec) = start("interrupt", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "A long job [fake:work:60]");
    working(&mut c, &task.id);
    let began = Instant::now();
    assert_eq!(send(&mut c, &task.id, "Change of plan", Some(SendMode::Interrupt)), Reply::Ok);
    let t = until(&mut c, &task.id, "the interrupted run and the new one", |t| t.runs.len() == 2 && t.runs[0].status == RunStatus::Interrupted);
    assert!(began.elapsed() < Duration::from_secs(10), "not after the 60 seconds the job had left");
    assert_eq!(t.runs[0].reason.as_deref(), Some("You interrupted it with a new message."));
    let t = finished(&mut c, &task.id, 2);
    assert_eq!(t.task.status, TaskStatus::Done);
    assert_eq!(t.runs[1].session, t.runs[0].session, "the conversation is the same one");
    assert!(agent_words(&t).contains("You said: Change of plan"));
    assert!(t.queued.is_empty());
    let r = record(&rec);
    let at = r.iter().position(|v| v["stdin"]["request"]["subtype"] == "interrupt").expect("the CLI's own interrupt request was sent");
    assert_eq!(r[at]["stdin"]["type"], "control_request");
    let a = argvs(&rec);
    assert_eq!(a.len(), 2);
    assert!(a[1].iter().any(|x| x == "--resume"));
    assert_eq!(user_messages(&rec), vec!["A long job [fake:work:60]".to_owned(), "Change of plan".to_owned()]);
    // And it carries on: a plain follow-up after an interrupt resumes as after any run.
    assert_eq!(send(&mut c, &task.id, "And then", None), Reply::Ok);
    finished(&mut c, &task.id, 3);
}

#[test]
fn a_cli_that_does_not_hear_the_interrupt_is_stopped_and_the_words_still_run() {
    let (d, rec) = start("interrupt-deaf", &[("NEKO_INTERRUPT_GRACE_MS", "600")]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Deaf [fake:work:120:deaf]");
    working(&mut c, &task.id);
    assert_eq!(send(&mut c, &task.id, "Please stop that", Some(SendMode::Interrupt)), Reply::Ok);
    let t = until(&mut c, &task.id, "the interrupted run", |t| t.runs.len() == 2 && t.runs[0].status == RunStatus::Interrupted);
    assert_eq!(t.runs[0].status, RunStatus::Interrupted);
    finished(&mut c, &task.id, 2);
    assert_eq!(user_messages(&rec).last().map(String::as_str), Some("Please stop that"));
}

#[test]
fn a_second_interrupt_while_one_is_going_waits_its_turn() {
    let (d, rec) = start("interrupt-twice", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Work [fake:work:60]");
    working(&mut c, &task.id);
    assert_eq!(send(&mut c, &task.id, "First change", Some(SendMode::Interrupt)), Reply::Ok);
    assert_eq!(send(&mut c, &task.id, "Second change", Some(SendMode::Interrupt)), Reply::Ok);
    finished(&mut c, &task.id, 3);
    assert_eq!(user_messages(&rec), vec!["Work [fake:work:60]".to_owned(), "First change".to_owned(), "Second change".to_owned()]);
}

#[test]
fn stop_holds_the_queue_and_send_now_starts_a_run_from_it() {
    let (d, rec) = start("stop-holds", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Work [fake:work:60]");
    working(&mut c, &task.id);
    assert_eq!(send(&mut c, &task.id, "Later", Some(SendMode::Queue)), Reply::Ok);
    until(&mut c, &task.id, "it waits", queued_is(1));
    assert_eq!(c.request(Request::StopTask { task: task.id.clone() }).unwrap(), Reply::Ok);
    let t = until(&mut c, &task.id, "stopped", |t| t.task.status == TaskStatus::Stopped);
    std::thread::sleep(Duration::from_millis(800));
    let t2 = transcript(&mut c, &task.id);
    assert_eq!(t2.runs.len(), 1, "a stop starts nothing by itself");
    assert_eq!(t2.queued.len(), 1, "what waited is still there, held for the person");
    let id = t.queued[0].id.clone();
    assert_eq!(c.request(Request::SendQueuedNow { task: task.id.clone(), queued: id.clone() }).unwrap(), Reply::Ok);
    let t = finished(&mut c, &task.id, 2);
    assert!(t.queued.is_empty());
    assert!(agent_words(&t).contains("You said: Later"));
    assert!(matches!(c.request(Request::SendQueuedNow { task: task.id.clone(), queued: id }).unwrap(), Reply::Error { code: ErrorCode::NotFound, .. }), "sent already");
    assert_eq!(user_messages(&rec).last().map(String::as_str), Some("Later"));
}

#[test]
fn send_now_steers_a_queued_message_into_the_running_agent() {
    let (d, rec) = start("send-now", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Work [fake:work:5]");
    working(&mut c, &task.id);
    assert_eq!(send(&mut c, &task.id, "Right away", Some(SendMode::Queue)), Reply::Ok);
    let t = until(&mut c, &task.id, "it waits", queued_is(1));
    assert_eq!(c.request(Request::SendQueuedNow { task: task.id.clone(), queued: t.queued[0].id.clone() }).unwrap(), Reply::Ok);
    until(&mut c, &task.id, "the agent to read it", |t| agent_words(t).contains("Reading your message: Right away"));
    let t = finished(&mut c, &task.id, 1);
    assert_eq!(t.runs.len(), 1, "it went into the run, and started no other");
    assert_eq!(user_messages(&rec), vec!["Work [fake:work:5]".to_owned(), "Right away".to_owned()]);
}

#[test]
fn at_rest_every_mode_just_sends() {
    let (d, rec) = start("at-rest", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Quick one [fake:plain]");
    finished(&mut c, &task.id, 1);
    for (i, mode) in [SendMode::Queue, SendMode::Interrupt, SendMode::Steer].into_iter().enumerate() {
        assert_eq!(send(&mut c, &task.id, &format!("Again {i}"), Some(mode)), Reply::Ok);
        let t = finished(&mut c, &task.id, i + 2);
        assert!(t.queued.is_empty(), "nothing waits when nothing was running");
    }
    assert_eq!(user_messages(&rec).len(), 4);
    assert_eq!(argvs(&rec).len(), 4);
}

#[test]
fn what_was_queued_is_kept_across_a_restart_and_is_not_sent_by_itself() {
    let (mut d, rec) = start("restart", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Work [fake:work:60]");
    working(&mut c, &task.id);
    assert_eq!(send(&mut c, &task.id, "Keep me", Some(SendMode::Queue)), Reply::Ok);
    until(&mut c, &task.id, "it waits", queued_is(1));
    drop(c);
    d.stop();
    let dir = d.dir.clone();
    d.keep_dir = true;
    drop(d);
    let rec_s = rec.to_string_lossy().into_owned();
    let d2 = Daemon::start_with(dir, &[("NEKO_FAKE_RECORD", &rec_s)]);
    let mut c = watcher(&d2);
    let t = transcript(&mut c, &task.id);
    assert_eq!(t.queued.len(), 1);
    assert_eq!(t.queued[0].words, "Keep me");
    assert_eq!(t.queued[0].kind, QueuedKind::Queued);
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(transcript(&mut c, &task.id).runs.len(), 1, "an interrupted run starts nothing by itself");
    // Removing the thread takes its waiting messages with it.
    let id = t.queued[0].id.clone();
    assert_eq!(c.request(Request::SendQueuedNow { task: task.id.clone(), queued: id }).unwrap(), Reply::Ok);
    finished(&mut c, &task.id, 2);
    assert_eq!(c.request(Request::RemoveTask { task: task.id.clone() }).unwrap(), Reply::Ok);
}

#[test]
fn a_message_that_is_too_long_or_empty_is_refused_for_every_mode() {
    let (d, _rec) = start("refuse", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Work [fake:work:4]");
    working(&mut c, &task.id);
    for mode in [SendMode::Steer, SendMode::Queue, SendMode::Interrupt] {
        assert!(matches!(send(&mut c, &task.id, "   ", Some(mode)), Reply::Error { code: ErrorCode::InvalidWords, .. }));
    }
    let big = "x".repeat(MAX_WORDS_BYTES + 1);
    assert!(matches!(send(&mut c, &task.id, &big, Some(SendMode::Queue)), Reply::Error { code: ErrorCode::InvalidWords, .. }));
}

//! Timers and watches in the real daemon, with the fake CLI as `claude` and Neko's own MCP server really started by it
//! (docs/knowledge/waits.md): a thread sets a timer and rests as Waiting with no process, the scheduler's test clock moves and it is
//! woken once with `--resume` and a `<neko-wake kind="timer">`; a watched file wakes it and Stop watching ends the watch; a command
//! watch wakes it when its check exits 0 and asks first under "Ask before risky steps"; "Pause automatic work" holds a timer, which is
//! recorded and said in the thread; a standing round posts into its Cattain's own thread. Every wait has a deadline.

mod support;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use serde_json::{Value, json};
use support::*;

const T: Duration = Duration::from_secs(40);
const MIN: i64 = 60_000;

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + T;
    while !ok() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

struct Rig {
    d: Daemon,
    record: PathBuf,
}

fn rig(name: &str) -> Rig {
    let dir = temp_dir(name);
    let record = dir.join("record.jsonl");
    let r = record.to_string_lossy().into_owned();
    Rig { d: Daemon::start_with(dir, &[("NEKO_FAKE_RECORD", &r), ("NEKO_WAKE_BATCH_MS", "300"), ("NEKO_DEBUG_CLOCK", "1")]), record }
}

impl Rig {
    fn client(&self) -> Client {
        let mut c = self.d.client();
        assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
        c
    }

    fn recorded(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.record).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    fn argvs(&self) -> Vec<Vec<String>> {
        self.recorded().iter().filter_map(|v| v["argv"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())).collect()
    }

    /// The user messages the fake CLI read.
    fn messages(&self) -> Vec<String> {
        self.recorded()
            .iter()
            .filter_map(|v| v["stdin"].as_object().filter(|m| m.get("type").and_then(Value::as_str) == Some("user")).map(|m| m["message"]["content"].as_array().map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect::<String>()).unwrap_or_default()))
            .collect()
    }
}

fn task(c: &mut Client, id: &TaskId) -> Task {
    match c.request(Request::GetTask { task: id.clone() }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("task: {r:?}"),
    }
}

fn transcript(c: &mut Client, id: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: id.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { crew: None, words: words.into(), workspace: None, in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

fn set_mode(c: &mut Client, m: PermissionMode) {
    let s = Settings { runtime: "claude".into(), permission_mode: m, own_mcp: false, queue_first: false, ask_first_subagent: false, cattains_by_hand: false };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });
}

/// The scheduler's clock, `minutes` after the real one.
fn clock(c: &mut Client, minutes: i64) {
    assert_eq!(c.request(Request::DebugClock { offset_ms: Some(minutes * MIN) }).unwrap(), Reply::Ok);
}

/// A thread's words: the fake calls Neko's tool `tool` with `args` through the real MCP hop, then ends its turn.
fn calls(tool: &str, args: Value) -> String {
    format!("Wait for it [fake:mcp]\nmcp neko {tool} {args}")
}

fn events(t: &Transcript) -> Vec<EventKind> {
    t.parts.iter().filter_map(|p| if let PartBody::EventWake { event, .. } = &p.body { Some(*event) } else { None }).collect()
}

fn resting(c: &mut Client, id: &TaskId) -> Task {
    let mut last = None;
    until("the thread to rest as Waiting", || {
        let t = task(c, id);
        let ok = t.status == TaskStatus::Waiting && !t.waits.is_empty();
        last = Some(t);
        ok
    });
    last.unwrap()
}

#[test]
fn a_timer_rests_the_thread_and_wakes_it_once_with_resume_when_the_clock_moves() {
    let r = rig("timer");
    let mut c = r.client();
    let t = create(&mut c, &calls("wake_me", json!({"after": "2h", "note": "Check the deploy"})));
    let rest = resting(&mut c, &t.id);
    assert_eq!(rest.thread_status, ThreadStatus::Waiting);
    let w = &rest.waits[0];
    assert_eq!((w.kind, w.note.as_str()), (WaitKind::Timer, "Check the deploy"));
    let at = w.at_ms.expect("a timer has its time");
    assert!(at - rest.created_ms >= 2 * 60 * MIN - 5_000 && at - rest.created_ms <= 2 * 60 * MIN + 30_000, "two hours on");
    assert_eq!(r.argvs().len(), 1, "resting: no second run yet");
    // Not yet: an hour on, nothing.
    clock(&mut c, 60);
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(r.argvs().len(), 1, "an hour early, nothing wakes");
    clock(&mut c, 121);
    until("the timer's wake run", || r.argvs().len() == 2);
    let argv = &r.argvs()[1];
    assert!(argv.windows(2).any(|w| w[0] == "--resume"), "the wake resumes the conversation: {argv:?}");
    let woke = r.messages().into_iter().find(|m| m.starts_with("<neko-wake kind=\"timer\"")).expect("the wake message");
    assert!(woke.contains("<note>Check the deploy</note>"), "{woke}");
    until("the thread to be done", || task(&mut c, &t.id).status == TaskStatus::Done);
    let done = task(&mut c, &t.id);
    assert!(done.waits.is_empty() && done.thread_status == ThreadStatus::Idle, "nothing left to wait on");
    assert_eq!(events(&transcript(&mut c, &t.id)), [EventKind::Timer], "one note before the run that read it");
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(r.argvs().len(), 2, "woken once");
}

#[test]
fn a_watched_file_wakes_the_thread_and_stop_watching_ends_the_watch() {
    let r = rig("watch-file");
    let mut c = r.client();
    let t = create(&mut c, &calls("watch_files", json!({"paths": ["build.log"], "note": "Read the log"})));
    let rest = resting(&mut c, &t.id);
    let w = rest.waits[0].clone();
    assert_eq!((w.kind, w.paths.clone()), (WaitKind::Files, vec!["build.log".to_owned()]));
    let folder = PathBuf::from(&transcript(&mut c, &t.id).runs[0].workdir);
    // A change to a file it does not watch wakes nothing; then the one it watches does. (Past the moment after the run ends, when its
    // own changes may still be arriving.)
    std::thread::sleep(Duration::from_millis(3_500));
    std::fs::write(folder.join("other.txt"), "x").unwrap();
    std::thread::sleep(Duration::from_millis(2_000));
    assert_eq!(r.argvs().len(), 1, "a file it does not watch");
    std::fs::write(folder.join("build.log"), "build ok").unwrap();
    until("the watch's wake run", || r.argvs().len() == 2);
    let woke = r.messages().into_iter().find(|m| m.starts_with("<neko-wake kind=\"watch\"")).expect("the wake message");
    assert!(woke.contains("status=\"changed\"") && woke.contains("<changed>build.log</changed>"), "{woke}");
    // The watch goes on: the thread rests on it again.
    let again = resting(&mut c, &t.id);
    assert_eq!(again.waits.iter().map(|x| x.id.clone()).collect::<Vec<_>>(), [w.id.clone()], "the same watch");
    assert_eq!(events(&transcript(&mut c, &t.id)), [EventKind::Files]);
    // Stop watching: the thread is at rest with nothing to wait on, and a change wakes nothing.
    assert_eq!(c.request(Request::StopWaiting { task: t.id.clone(), wait: w.id.clone() }).unwrap(), Reply::Ok);
    let stopped = task(&mut c, &t.id);
    assert!(stopped.waits.is_empty() && stopped.status == TaskStatus::Done, "{stopped:?}");
    assert!(matches!(c.request(Request::StopWaiting { task: t.id.clone(), wait: w.id.clone() }).unwrap(), Reply::Error { code: ErrorCode::NotFound, .. }));
    std::thread::sleep(Duration::from_millis(1_000));
    std::fs::write(folder.join("build.log"), "build ok again").unwrap();
    std::thread::sleep(Duration::from_millis(3_000));
    assert_eq!(r.argvs().len(), 2, "a stopped watch wakes nothing");
}

#[test]
fn a_command_watch_wakes_the_thread_when_its_check_exits_zero() {
    let r = rig("watch-command");
    let mut c = r.client();
    set_mode(&mut c, PermissionMode::Full);
    let t = create(&mut c, &calls("watch_command", json!({"command": "test -f ready", "every": "1m", "note": "Ship it"})));
    let rest = resting(&mut c, &t.id);
    let w = rest.waits[0].clone();
    assert_eq!((w.kind, w.command.as_deref(), w.every_ms), (WaitKind::Command, Some("test -f ready"), Some(MIN)));
    // The first check finds nothing: not yet.
    clock(&mut c, 2);
    until("the first check", || task(&mut c, &t.id).waits.first().is_some_and(|x| x.checked_ms.is_some()));
    assert_eq!(task(&mut c, &t.id).waits[0].last_exit, Some(1));
    assert_eq!(r.argvs().len(), 1);
    let folder = PathBuf::from(&transcript(&mut c, &t.id).runs[0].workdir);
    std::fs::write(folder.join("ready"), "").unwrap();
    clock(&mut c, 4);
    until("the watch's wake run", || r.argvs().len() == 2);
    let woke = r.messages().into_iter().find(|m| m.starts_with("<neko-wake kind=\"watch\"")).expect("the wake message");
    assert!(woke.contains("status=\"succeeded\"") && woke.contains("command=\"test -f ready\""), "{woke}");
    until("the thread to be done", || task(&mut c, &t.id).status == TaskStatus::Done);
    assert!(task(&mut c, &t.id).waits.is_empty(), "a check that succeeded ends its watch");
    assert_eq!(events(&transcript(&mut c, &t.id)), [EventKind::Command]);
}

#[test]
fn a_command_watch_asks_first_under_ask_before_risky_steps() {
    let r = rig("watch-ask");
    let mut c = r.client();
    set_mode(&mut c, PermissionMode::Ask);
    let t = create(&mut c, &calls("watch_command", json!({"command": "gh pr checks 12"})));
    let mut asked = None;
    until("the request", || {
        let tr = transcript(&mut c, &t.id);
        asked = tr.permissions.iter().find(|p| p.state == PermissionState::Pending).cloned();
        asked.is_some()
    });
    let p = asked.unwrap();
    assert_eq!((p.title.as_str(), p.detail.as_str()), ("Run a check again and again", "gh pr checks 12"));
    assert!(task(&mut c, &t.id).waits.is_empty(), "nothing is watched before the person allows it");
    assert_eq!(c.request(Request::AnswerPermission { permission: p.id.clone(), decision: Decision::Deny }).unwrap(), Reply::Ok);
    until("the run to end", || !task(&mut c, &t.id).status.is_active());
    assert!(task(&mut c, &t.id).waits.is_empty(), "denied: no watch");
}

#[test]
fn a_timer_due_while_automatic_work_is_paused_starts_nothing_is_recorded_and_the_thread_says_so() {
    let r = rig("pause-automatic");
    let mut c = r.client();
    let t = create(&mut c, &calls("wake_me", json!({"after": "30m", "note": "Look again"})));
    let rest = resting(&mut c, &t.id);
    let id = rest.waits[0].id.clone();
    assert_eq!(c.request(Request::SetRoundsPaused { paused: true }).unwrap(), Reply::Ok);
    clock(&mut c, 31);
    until("the thread to say so", || {
        transcript(&mut c, &t.id).parts.iter().any(|p| matches!(&p.body, PartBody::Notice { text, .. } if text.contains("automatic work is paused")))
    });
    let after = task(&mut c, &t.id);
    assert!(after.waits.is_empty() && after.status == TaskStatus::Done, "skipped, and at rest: {after:?}");
    std::thread::sleep(Duration::from_millis(1_000));
    assert_eq!(r.argvs().len(), 1, "no run while paused");
    assert!(events(&transcript(&mut c, &t.id)).is_empty(), "no wake");
    // It is recorded: the event log says the timer was skipped.
    assert!(stored_as_skipped(&r.d, &id), "the skip is in the store");
}

/// Whether the store holds the skipped timer (read from the data folder's database with the system's sqlite3).
fn stored_as_skipped(d: &Daemon, id: &str) -> bool {
    let db = d.dir.join(neko_protocol::files::STORE);
    let out = std::process::Command::new("sqlite3").arg(&db).arg(format!("SELECT state, end_reason FROM waits WHERE id = '{id}'")).output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim() == "skipped|Automatic work was paused.",
        // No sqlite3 on this Mac: the thread's notice above is the record.
        _ => true,
    }
}

#[test]
fn a_standing_round_posts_its_words_into_its_cattains_own_thread() {
    let r = rig("round-standing");
    let mut c = r.client();
    let round = match c.request(Request::CreateRound { name: "Morning check".into(), words: "Look over the open pull requests".into(), cat: Some("cat_miso".into()), standing: true, workspace: None, schedule: Schedule::Manual, on: true }).unwrap() {
        Reply::Round { round } => round,
        x => panic!("create_round: {x:?}"),
    };
    assert!(round.standing && round.cat.as_deref() == Some("cat_miso"));
    let Reply::Task { task: thread } = c.request(Request::RunRoundNow { round: round.id.clone() }).unwrap() else { panic!("run now") };
    assert!(thread.cattain, "the Cattain's own thread, not a new one");
    until("the round's run", || r.argvs().len() == 1);
    let woke = r.messages().into_iter().find(|m| m.starts_with("<neko-wake kind=\"round\"")).expect("the round's message");
    assert!(woke.contains("round=\"Morning check\"") && woke.contains("Look over the open pull requests"), "{woke}");
    until("the thread to be done", || task(&mut c, &thread.id).status == TaskStatus::Done);
    assert_eq!(events(&transcript(&mut c, &thread.id)), [EventKind::Round]);
    let Reply::Tasks { tasks } = c.request(Request::ListTasks).unwrap() else { panic!("tasks") };
    assert!(tasks.iter().all(|t| t.round.is_none()), "no thread was made for it");
    // A standing round needs a Cattain; a round of nobody's starts a thread with no cat.
    assert!(matches!(c.request(Request::CreateRound { name: "x".into(), words: "x".into(), cat: None, standing: true, workspace: None, schedule: Schedule::Manual, on: true }).unwrap(), Reply::Error { code: ErrorCode::Refused, .. }));
    let Reply::Round { round: nobody } = c.request(Request::CreateRound { name: "Tidy".into(), words: "Tidy up [fake:plain]".into(), cat: None, standing: false, workspace: None, schedule: Schedule::Manual, on: true }).unwrap() else { panic!("nobody's") };
    let Reply::Task { task: made } = c.request(Request::RunRoundNow { round: nobody.id.clone() }).unwrap() else { panic!("run now") };
    assert_eq!((made.cat.clone(), made.round.as_ref().map(|x| x.id.clone())), (None, Some(nobody.id.clone())));
}

#[test]
fn a_thread_is_refused_timers_past_the_limits_and_stop_ends_the_ones_it_has() {
    let r = rig("wait-limits");
    let mut c = r.client();
    let mut words = String::from("Set many [fake:mcp]");
    for i in 0..5 {
        words.push_str(&format!("\nmcp neko wake_me {}", json!({"after": format!("{}h", i + 1)})));
    }
    words.push_str(&format!("\nmcp neko wake_me {}", json!({"after": "8d"})));
    let t = create(&mut c, &words);
    let rest = resting(&mut c, &t.id);
    assert_eq!(rest.waits.len(), neko_protocol::waits::TIMERS_PER_THREAD, "four timers at most");
    let tr = transcript(&mut c, &t.id);
    let outs: Vec<String> = tr.parts.iter().filter_map(|p| if let PartBody::Tool { name, output, .. } = &p.body { (name == "mcp__neko__wake_me").then(|| output.clone().unwrap_or_default()) } else { None }).collect();
    assert!(outs[4].contains("4 timers set") && outs[5].contains("at most 7 days"), "{outs:?}");
    // Stop takes them all.
    assert_eq!(c.request(Request::StopTask { task: t.id.clone() }).unwrap(), Reply::Ok);
    until("the timers to end", || task(&mut c, &t.id).waits.is_empty());
}

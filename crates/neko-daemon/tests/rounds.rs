//! Rounds in the real daemon, with the fake CLI as `claude` (docs/knowledge/rounds.md). Time is handed in through the debug clock
//! (`NEKO_DEBUG_CLOCK=1`: a debug build only), so a test makes a round due without waiting. Every wait has a deadline.

mod support;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::rounds::{RunEnd, RunKind};
use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(30);
const MIN: i64 = 60_000;

fn daemon(name: &str) -> Daemon {
    Daemon::start_with(temp_dir(name), &[("NEKO_DEBUG_CLOCK", "1")])
}

fn screen(d: &Daemon) -> Client {
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    c
}

fn wait_for<R>(what: &str, mut f: impl FnMut() -> Option<R>) -> R {
    let end = Instant::now() + T;
    loop {
        if let Some(r) = f() {
            return r;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn hourly(c: &mut Client, name: &str, words: &str) -> Round {
    match c.request(Request::CreateRound { name: name.into(), words: words.into(), cat: "cat_mochi".into(), workspace: None, schedule: Schedule::Every { hours: 1 }, on: true }).unwrap() {
        Reply::Round { round } => round,
        r => panic!("create_round: {r:?}"),
    }
}

fn round(c: &mut Client, id: &str) -> Round {
    match c.request(Request::ListRounds).unwrap() {
        Reply::Rounds { rounds, .. } => rounds.into_iter().find(|r| r.id == id).expect("the round is listed"),
        r => panic!("list_rounds: {r:?}"),
    }
}

fn history(c: &mut Client, id: &str) -> Vec<RoundRun> {
    match c.request(Request::RoundHistory { round: id.into() }).unwrap() {
        Reply::RoundRuns { runs } => runs,
        r => panic!("round_history: {r:?}"),
    }
}

fn tasks_of(c: &mut Client, id: &str) -> Vec<Task> {
    match c.request(Request::ListTasks).unwrap() {
        Reply::Tasks { tasks } => tasks.into_iter().filter(|t| t.round.as_ref().is_some_and(|r| r.id == id)).collect(),
        r => panic!("list_tasks: {r:?}"),
    }
}

/// The scheduler's clock, `minutes` after the real one.
fn clock(c: &mut Client, minutes: i64) {
    assert_eq!(c.request(Request::DebugClock { offset_ms: Some(minutes * MIN) }).unwrap(), Reply::Ok);
}

fn code(r: Reply) -> Option<ErrorCode> {
    match r {
        Reply::Error { code, .. } => Some(code),
        _ => None,
    }
}

fn mode(c: &mut Client, m: PermissionMode) {
    assert!(matches!(c.request(Request::SetSettings { settings: Settings { runtime: "claude".into(), permission_mode: m, own_mcp: false } }).unwrap(), Reply::Settings { .. }));
}

#[test]
fn a_round_is_made_listed_changed_switched_and_removed_and_every_screen_hears_it() {
    let d = daemon("rounds-crud");
    let mut c = screen(&d);
    let mut other = screen(&d);
    let r = hourly(&mut c, "Docs check", "Check the docs [fake:plain]");
    assert_eq!((r.status, r.on, r.workspace.as_str()), (RoundStatus::Scheduled, true, GENERAL));
    assert!(r.next_due_ms.unwrap() > r.created_ms, "the daemon works out the next time");
    let heard = wait_for("round_added", || match other.next_event(Duration::from_millis(100)).unwrap() {
        Some(ServerMessage::Event { change: Change::RoundAdded { round }, .. }) => Some(round),
        _ => None,
    });
    assert_eq!(heard, r);
    // Edit.
    let Reply::Round { round: edited } = c
        .request(Request::UpdateRound { round: r.id.clone(), name: "Docs links".into(), words: "Check links".into(), cat: "cat_miso".into(), workspace: None, schedule: Schedule::Weekdays { days: vec![schedule::Weekday::Mon], at: "09:00".into() } })
        .unwrap()
    else {
        panic!("update")
    };
    assert_eq!((edited.name.as_str(), edited.cat.as_str(), edited.schedule.is_automatic()), ("Docs links", "cat_miso", true));
    // Off and on again.
    let Reply::Round { round: off } = c.request(Request::SetRoundOn { round: r.id.clone(), on: false }).unwrap() else { panic!("off") };
    assert_eq!((off.on, off.status, off.next_due_ms), (false, RoundStatus::Off, None));
    let Reply::Round { round: on } = c.request(Request::SetRoundOn { round: r.id.clone(), on: true }).unwrap() else { panic!("on") };
    assert_eq!(on.status, RoundStatus::Scheduled);
    // Refusals.
    assert_eq!(code(c.request(Request::CreateRound { name: " ".into(), words: "x".into(), cat: "cat_miso".into(), workspace: None, schedule: Schedule::Manual, on: true }).unwrap()), Some(ErrorCode::InvalidName));
    assert_eq!(code(c.request(Request::CreateRound { name: "A".into(), words: "".into(), cat: "cat_miso".into(), workspace: None, schedule: Schedule::Manual, on: true }).unwrap()), Some(ErrorCode::InvalidWords));
    assert_eq!(code(c.request(Request::CreateRound { name: "A".into(), words: "x".into(), cat: "cat_miso".into(), workspace: None, schedule: Schedule::Every { hours: 99 }, on: true }).unwrap()), Some(ErrorCode::Refused));
    assert_eq!(code(c.request(Request::SetRoundOn { round: "round_nope".into(), on: true }).unwrap()), Some(ErrorCode::NotFound));
    // Remove.
    assert_eq!(c.request(Request::RemoveRound { round: r.id.clone() }).unwrap(), Reply::Ok);
    wait_for("round_removed", || match other.next_event(Duration::from_millis(100)).unwrap() {
        Some(ServerMessage::Event { change: Change::RoundRemoved { round }, .. }) if round == r.id => Some(()),
        _ => None,
    });
    assert!(matches!(c.request(Request::ListRounds).unwrap(), Reply::Rounds { rounds, .. } if rounds.is_empty()));
}

#[test]
fn run_now_makes_an_ordinary_task_that_says_where_it_came_from_and_the_history_shows_how_it_ended() {
    let d = daemon("rounds-run-now");
    let mut c = screen(&d);
    let r = c.request(Request::CreateRound { name: "Docs check".into(), words: "Check the docs [fake:plain]".into(), cat: "cat_mochi".into(), workspace: None, schedule: Schedule::Manual, on: true }).unwrap();
    let Reply::Round { round: r } = r else { panic!("{r:?}") };
    assert_eq!(r.status, RoundStatus::Manual);
    let Reply::Task { task } = c.request(Request::RunRoundNow { round: r.id.clone() }).unwrap() else { panic!("run now") };
    assert_eq!((task.title.as_str(), task.cat.as_deref().unwrap_or_default(), task.workspace.as_str()), ("Docs check", "cat_mochi", GENERAL));
    assert_eq!(task.round.clone().map(|r| (r.id, r.name, r.due_ms)), Some((r.id.clone(), "Docs check".into(), None)));
    let done = wait_for("the task to finish", || tasks_of(&mut c, &r.id).into_iter().find(|t| t.status == TaskStatus::Done));
    let Reply::Transcript { transcript } = c.request(Request::GetTranscript { task: done.id.clone() }).unwrap() else { panic!("transcript") };
    assert_eq!(transcript.runs.len(), 1, "an ordinary run, with checkpoints and a cat's rules like any other");
    assert_eq!(transcript.runs[0].cat.as_deref(), Some("cat_mochi"));
    let h = wait_for("the history line to close", || Some(history(&mut c, &r.id)).filter(|h| h.first().is_some_and(|l| l.end.is_some())));
    assert_eq!((h[0].kind, h[0].due_ms, h[0].end, h[0].task.clone()), (RunKind::Started, None, Some(RunEnd::Done), Some(done.id.clone())));
    assert_eq!(round(&mut c, &r.id).last.map(|l| l.end), Some(Some(RunEnd::Done)));
    // Removing the round keeps the task.
    assert_eq!(c.request(Request::RemoveRound { round: r.id.clone() }).unwrap(), Reply::Ok);
    assert!(matches!(c.request(Request::GetTask { task: done.id.clone() }).unwrap(), Reply::Task { task } if task.round.is_some()));
}

#[test]
fn a_round_becomes_due_and_starts_by_itself_and_an_unattended_step_that_would_ask_waits_for_the_person() {
    let d = daemon("rounds-due");
    let mut c = screen(&d);
    mode(&mut c, PermissionMode::Ask);
    let r = hourly(&mut c, "Run tests", "Run the tests [fake:hook-shell]");
    clock(&mut c, 61);
    let task = wait_for("the task a round made", || tasks_of(&mut c, &r.id).into_iter().next());
    let due = task.round.as_ref().unwrap().due_ms.unwrap();
    assert!(due > r.created_ms && due <= r.created_ms + 61 * MIN + 5_000, "due an hour after it was made");
    // The same step waits for the person, as it does in a task the person made: the permission mode is the one in Settings.
    let attended = match c.request(Request::CreateTask { words: "Run the tests [fake:hook-shell]".into(), workspace: None, in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("{r:?}"),
    };
    let asked = wait_for("both to ask", || match c.request(Request::ListNeeds).unwrap() {
        Reply::Needs { items } if items.len() == 2 => Some(items),
        _ => None,
    });
    let perm = |t: &TaskId| {
        asked
            .iter()
            .find_map(|i| match &i.need {
                Need::Permission { permission } if &i.task == t => Some(permission.clone()),
                _ => None,
            })
            .expect("a request for this task")
    };
    let (unattended, by_hand) = (perm(&task.id), perm(&attended.id));
    assert_eq!((unattended.state, unattended.tool.as_str(), unattended.title.as_str(), unattended.destructive), (PermissionState::Pending, by_hand.tool.as_str(), by_hand.title.as_str(), by_hand.destructive));
    assert_eq!(asked.iter().find(|i| i.task == task.id).unwrap().cat_name, "mochi");
    assert_eq!(asked.iter().find(|i| i.task == attended.id).unwrap().cat_name, "", "a thread the person started has no cat");
    // Nothing answers it for the person: it is still pending a moment later, and the task still waits.
    std::thread::sleep(Duration::from_millis(1500));
    assert!(matches!(c.request(Request::GetTask { task: task.id.clone() }).unwrap(), Reply::Task { task } if task.status == TaskStatus::NeedsYou));
    assert!(matches!(c.request(Request::ListNeeds).unwrap(), Reply::Needs { items } if items.len() == 2));
    // The next time comes while the first still waits: skipped, with the reason.
    clock(&mut c, 121);
    let h = wait_for("the skipped line", || Some(history(&mut c, &r.id)).filter(|h| h.len() == 2));
    assert_eq!(h[0].kind, RunKind::Skipped);
    assert!(h[0].reason.as_deref().unwrap().contains("waiting for you"), "{:?}", h[0].reason);
    assert_eq!(tasks_of(&mut c, &r.id).len(), 1, "no second task");
    // Pause all holds it: the next time passes as a skip, and nothing is run when it is let go.
    assert_eq!(c.request(Request::SetRoundsPaused { paused: true }).unwrap(), Reply::Ok);
    assert_eq!(round(&mut c, &r.id).status, RoundStatus::Paused);
    // The person answers (no): the task ends.
    assert_eq!(c.request(Request::AnswerPermission { permission: unattended.id.clone(), decision: Decision::Deny }).unwrap(), Reply::Ok);
    wait_for("the task to end", || tasks_of(&mut c, &r.id).into_iter().find(|t| t.status == TaskStatus::Done));
    clock(&mut c, 181);
    let h = wait_for("the paused skip", || Some(history(&mut c, &r.id)).filter(|h| h.len() == 3));
    assert_eq!((h[0].kind, h[0].reason.as_deref()), (RunKind::Skipped, Some("Every round was paused.")));
    assert_eq!(tasks_of(&mut c, &r.id).len(), 1);
    assert_eq!(c.request(Request::SetRoundsPaused { paused: false }).unwrap(), Reply::Ok);
    clock(&mut c, 241);
    wait_for("the next run, once let go", || Some(tasks_of(&mut c, &r.id)).filter(|t| t.len() == 2));
}

#[test]
fn three_failed_runs_in_a_row_turn_the_round_off_and_it_says_so_in_the_task() {
    let d = daemon("rounds-brake");
    let mut c = screen(&d);
    let r = hourly(&mut c, "Flaky", "Do it [fake:crash]");
    for n in 1..=3 {
        clock(&mut c, 61 * n);
        wait_for("a failed run", || Some(tasks_of(&mut c, &r.id)).filter(|t| t.len() == n as usize && t.iter().all(|t| t.status == TaskStatus::Failed)));
    }
    let braked = wait_for("the brake", || Some(round(&mut c, &r.id)).filter(|r| r.status == RoundStatus::Braked));
    assert!(!braked.on);
    assert!(braked.note.as_deref().unwrap().contains("3 failed runs in a row"), "{:?}", braked.note);
    let last = tasks_of(&mut c, &r.id).into_iter().max_by_key(|t| t.created_ms).unwrap();
    let Reply::Transcript { transcript } = c.request(Request::GetTranscript { task: last.id }).unwrap() else { panic!("transcript") };
    assert!(transcript.parts.iter().any(|p| matches!(&p.body, PartBody::Notice { text, .. } if text.contains("turned itself off after 3 failed runs in a row"))));
    // It runs no more, even when time passes.
    clock(&mut c, 61 * 6);
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(tasks_of(&mut c, &r.id).len(), 3);
}

#[test]
fn a_restart_does_not_run_a_time_twice_or_lose_the_rounds_or_the_pause() {
    let mut d = daemon("rounds-restart");
    let dir = d.dir.clone();
    d.keep_dir = true;
    let id = {
        let mut c = screen(&d);
        let r = hourly(&mut c, "Docs check", "Check [fake:plain]");
        clock(&mut c, 61);
        wait_for("the first run", || tasks_of(&mut c, &r.id).into_iter().find(|t| t.status == TaskStatus::Done));
        wait_for("its history line to close", || Some(()).filter(|_| history(&mut c, &r.id).first().is_some_and(|l| l.end == Some(RunEnd::Done))));
        assert_eq!(c.request(Request::SetRoundsPaused { paused: true }).unwrap(), Reply::Ok);
        r.id
    };
    assert!(d.stop().success());
    drop(d);
    // The same folder, a new daemon: the round, its history and the pause are as they were.
    let mut d = Daemon::start_with(dir.clone(), &[("NEKO_DEBUG_CLOCK", "1")]);
    d.keep_dir = false;
    let mut c = screen(&d);
    assert!(matches!(c.request(Request::ListRounds).unwrap(), Reply::Rounds { paused: true, rounds } if rounds.len() == 1));
    assert_eq!(history(&mut c, &id).len(), 1);
    assert_eq!(c.request(Request::SetRoundsPaused { paused: false }).unwrap(), Reply::Ok);
    // The first hour's time is not run again at 61 minutes; the second hour's is, at 121 (not lost).
    clock(&mut c, 61);
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(tasks_of(&mut c, &id).len(), 1, "a due time already dealt with is not run twice");
    clock(&mut c, 121);
    wait_for("the next time", || Some(tasks_of(&mut c, &id)).filter(|t| t.len() == 2));
}

#[test]
fn the_debug_clock_is_refused_unless_the_daemon_was_started_with_it() {
    let d = Daemon::start("rounds-no-debug-clock");
    let mut c = d.client();
    assert_eq!(code(c.request(Request::DebugClock { offset_ms: Some(3_600_000) }).unwrap()), Some(ErrorCode::Refused));
    // And a round is not made due by it.
    let Reply::Round { round } = c.request(Request::CreateRound { name: "A".into(), words: "x [fake:plain]".into(), cat: "cat_miso".into(), workspace: None, schedule: Schedule::Every { hours: 1 }, on: true }).unwrap() else { panic!("create") };
    std::thread::sleep(Duration::from_millis(500));
    assert!(tasks_of(&mut c, &round.id).is_empty());
}

#[test]
fn a_retired_cats_round_says_so_and_a_round_cannot_run_now_while_its_task_works() {
    let d = daemon("rounds-brakes");
    let mut c = screen(&d);
    let Reply::Round { round: r } =
        c.request(Request::CreateRound { name: "Slow".into(), words: "Go slowly [fake:slow]".into(), cat: "cat_tofu".into(), workspace: None, schedule: Schedule::Manual, on: true }).unwrap()
    else {
        panic!("create")
    };
    let Reply::Task { task } = c.request(Request::RunRoundNow { round: r.id.clone() }).unwrap() else { panic!("run now") };
    assert_eq!(code(c.request(Request::RunRoundNow { round: r.id.clone() }).unwrap()), Some(ErrorCode::Busy), "its last task is still working");
    assert_eq!(c.request(Request::StopTask { task: task.id.clone() }).unwrap(), Reply::Ok);
    wait_for("the task to stop", || tasks_of(&mut c, &r.id).into_iter().find(|t| t.status == TaskStatus::Stopped));
    assert!(matches!(c.request(Request::RetireCat { cat: "cat_tofu".into() }).unwrap(), Reply::Cat { .. }));
    let view = wait_for("the round to say so", || Some(round(&mut c, &r.id)).filter(|r| r.status == RoundStatus::CatRetired));
    assert_eq!(view.note.as_deref(), Some("tofu is retired."));
    assert_eq!(code(c.request(Request::RunRoundNow { round: r.id.clone() }).unwrap()), Some(ErrorCode::Refused));
}

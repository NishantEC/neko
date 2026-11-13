//! Drives goals end to end with a stubbed model, shell, clock, GitHub and
//! process table. No real model, shell check or GitHub call happens here.
use super::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};

struct Stub {
    now: AtomicI64,
    checks: Mutex<VecDeque<i64>>,
    judges: Mutex<VecDeque<Result<String, String>>>,
    judged: AtomicUsize,
    checked: AtomicUsize,
    held: AtomicBool,
    pr: Mutex<Option<String>>,
    pid_alive: AtomicBool,
    artifact: AtomicBool,
}

impl Stub {
    fn new() -> Self {
        Self { now: AtomicI64::new(store::now_ms()), checks: Default::default(), judges: Default::default(), judged: AtomicUsize::new(0), checked: AtomicUsize::new(0), held: AtomicBool::new(false), pr: Default::default(), pid_alive: AtomicBool::new(true), artifact: AtomicBool::new(false) }
    }
    fn check(self, codes: &[i64]) -> Self {
        self.checks.lock().unwrap().extend(codes.iter().copied());
        self
    }
    fn say(&self, verdict: &str) {
        self.judges.lock().unwrap().push_back(Ok(format!("{{\"verdict\":\"{verdict}\",\"reason\":\"because\",\"done\":\"did a step\",\"next\":\"the next step\"}}")));
    }
    fn say_json(&self, json: &str) {
        self.judges.lock().unwrap().push_back(Ok(json.to_owned()));
    }
    fn advance(&self, ms: i64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Env for Stub {
    fn now_ms(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }
    fn run_check(&self, _: &std::path::Path, command: &str) -> CheckResult {
        self.checked.fetch_add(1, Ordering::SeqCst);
        let code = self.checks.lock().unwrap().pop_front().expect("a scripted check result");
        CheckResult { command: command.into(), exit_code: Some(code), tail: format!("check output, exit {code}"), at_ms: self.now_ms(), source: "neko".into() }
    }
    fn judge(&self, _: &str) -> Result<String, String> {
        self.judged.fetch_add(1, Ordering::SeqCst);
        self.judges.lock().unwrap().pop_front().expect("a scripted judge answer")
    }
    fn machine_held(&self) -> bool {
        self.held.load(Ordering::SeqCst)
    }
    fn pr_state(&self, _: &str) -> Option<String> {
        self.pr.lock().unwrap().clone()
    }
    fn pid_alive(&self, _: u32) -> bool {
        self.pid_alive.load(Ordering::SeqCst)
    }
    fn exists(&self, _: &std::path::Path, _: &str) -> bool {
        self.artifact.load(Ordering::SeqCst)
    }
}

fn fixture(status: TaskStatus) -> (Controller, Arc<Mutex<Db>>) {
    let controller = super::super::tests::controller_with_task(status);
    let db = controller.db.clone();
    (controller, db)
}

fn goal_on_t(controller: &Controller, check: &str, max_turns: Option<u32>) -> String {
    controller.goal_command(GoalCommand::Create { workspace_id: "w".into(), objective: "Make the parser handle nested quotes".into(), done_check: check.into(), constraints: "Do not touch the lexer".into(), max_turns, ticket_id: Some("t".into()), conversation_id: None }).unwrap();
    let guard = controller.db.lock().unwrap();
    core::load_all(&guard).unwrap().pop().unwrap().id
}

fn goal(db: &Arc<Mutex<Db>>, id: &str) -> Goal {
    core::load(&db.lock().unwrap(), id).unwrap().unwrap()
}

fn task(db: &Arc<Mutex<Db>>) -> Task {
    store::load(&db.lock().unwrap()).unwrap().tasks.remove(0)
}

/// The ticket's turn ended the way a run ends.
fn end_turn(db: &Arc<Mutex<Db>>, status: TaskStatus, result: &str, failure: Option<&str>) {
    let db = db.lock().unwrap();
    let mut snapshot = store::load(&db).unwrap();
    let t = &mut snapshot.tasks[0];
    t.status = status;
    t.result = result.into();
    if let Some(failure) = failure {
        store::append_event(t, "supervisor", failure);
    }
    store::save(&db, &snapshot).unwrap();
}

fn open_items(db: &Arc<Mutex<Db>>) -> Vec<neko_protocol::routines::NeedsYou> {
    crate::routines::all_items(&db.lock().unwrap()).unwrap().into_iter().filter(|i| !i.done && i.link.as_deref().is_some_and(|l| l.starts_with("goal:"))).collect()
}

#[test]
fn continue_then_continue_then_done_with_a_passing_check() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new().check(&[101, 101, 0]);
    // The first turn is still running: nothing to do, no check, no judge.
    tick(&db, &env).unwrap();
    assert_eq!(env.checked.load(Ordering::SeqCst), 0);
    // Turn 1 ends with a failing check: CONTINUE, fed back to the same ticket.
    end_turn(&db, TaskStatus::ReadyForReview, "I fixed the parser.", None);
    env.say("CONTINUE");
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.status, g.turns, g.in_turn), (GoalStatus::Running, 2, true));
    assert_eq!(task(&db).status, TaskStatus::Queued, "the ticket carries the next turn");
    let note = task(&db).events.iter().find(|e| e.role == "note" && e.message.contains("Goal turn 2")).expect("the continuation reaches the agent").message.clone();
    assert!(note.contains("exit 101") && note.contains("check output") && note.contains("because"), "failure output and the judge's reason: {note}");
    // Turn 2 ends, still failing. The judge says DONE; the check says no.
    end_turn(&db, TaskStatus::ReadyForReview, "It is done, all tests pass.", None);
    env.say("DONE");
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.status, g.turns), (GoalStatus::Running, 3), "the agent's claim is not evidence: {}", g.reason);
    assert!(g.reason.contains("still fails"));
    // Turn 3: the check passes and the judge agrees.
    end_turn(&db, TaskStatus::ReadyForReview, "Fixed for real.", None);
    env.say("DONE");
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!(g.status, GoalStatus::Done);
    assert!(g.last_check.as_ref().unwrap().passed());
    assert_eq!(g.ledger.iter().map(|c| c.exit_code).collect::<Vec<_>>(), [Some(101), Some(101), Some(0)]);
    assert!(g.journal.iter().any(|j| j.note == "DONE" && j.next == "the next step"));
    // Finished goals are not ticked again.
    tick(&db, &env).unwrap();
    assert_eq!(env.judged.load(Ordering::SeqCst), 3);
}

#[test]
fn a_goal_with_no_check_needs_a_concrete_artifact_or_link() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "none", None);
    let env = Stub::new();
    end_turn(&db, TaskStatus::ReadyForReview, "Wrote the report.", None);
    env.say("DONE");
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Running, "done with no evidence is a continue");
    end_turn(&db, TaskStatus::ReadyForReview, "Wrote it to out/report.md", None);
    env.say_json(r#"{"verdict":"DONE","reason":"report written","evidence":"out/report.md"}"#);
    env.artifact.store(true, Ordering::SeqCst);
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Done);
    assert_eq!(env.checked.load(Ordering::SeqCst), 0, "no check to run");
}

#[test]
fn blocked_raises_one_inbox_question_with_a_default_and_waits_for_the_answer() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new().check(&[1]);
    end_turn(&db, TaskStatus::ReadyForReview, "I need the staging credentials.", None);
    env.say_json(r#"{"verdict":"BLOCKED","reason":"needs access","question":"Which staging account should it use?","default_answer":"The shared one"}"#);
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!(g.status, GoalStatus::Blocked);
    assert_eq!(g.blocked.as_ref().unwrap().default_answer, "The shared one");
    let items = open_items(&db);
    assert_eq!(items.len(), 1);
    assert!(items[0].title.contains("Which staging account") && items[0].detail.contains("The shared one") && items[0].link.as_deref() == Some(format!("goal:{id}").as_str()));
    // Waiting costs nothing: no more checks, judges or items.
    tick(&db, &env).unwrap();
    tick(&db, &env).unwrap();
    assert_eq!((env.judged.load(Ordering::SeqCst), env.checked.load(Ordering::SeqCst), open_items(&db).len()), (1, 1, 1));
    // An empty answer takes the default; the item closes and a turn follows.
    controller.goal_command(GoalCommand::Answer { id: id.clone(), text: String::new() }).unwrap();
    assert!(open_items(&db).is_empty());
    let g = goal(&db, &id);
    assert_eq!((g.status, g.in_turn), (GoalStatus::Running, false));
    assert!(g.reason.contains("The shared one"));
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.turns, g.in_turn), (2, true));
    assert!(task(&db).events.iter().any(|e| e.message.contains("You answered") && e.message.contains("The shared one")));
}

#[test]
fn wait_parks_without_spending_turns_until_the_time_the_pr_or_the_process() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "", None);
    let env = Stub::new();
    end_turn(&db, TaskStatus::ReadyForReview, "Waiting on CI.", None);
    env.say_json(r#"{"verdict":"WAIT","reason":"CI is running","wait_seconds":3600}"#);
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Waiting);
    for _ in 0..3 {
        tick(&db, &env).unwrap();
    }
    assert_eq!((goal(&db, &id).turns, env.judged.load(Ordering::SeqCst)), (1, 1), "no turn and no judge while waiting");
    env.advance(3_600_000 + 1_000);
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.status, g.in_turn), (GoalStatus::Running, false));
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.turns, g.in_turn), (2, true), "the next turn starts when the wait ends");
    assert!(task(&db).events.iter().any(|e| e.message.contains("The wait is over")));
    // A pull request state and a process exit end a wait too.
    end_turn(&db, TaskStatus::ReadyForReview, "Opened the PR.", None);
    env.say_json(r#"{"verdict":"WAIT","reason":"review","wait_pr":"acme/app#12","wait_pr_state":"merged"}"#);
    tick(&db, &env).unwrap();
    *env.pr.lock().unwrap() = Some("OPEN".into());
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Waiting);
    *env.pr.lock().unwrap() = Some("MERGED".into());
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Running);
    tick(&db, &env).unwrap();
    end_turn(&db, TaskStatus::ReadyForReview, "Running a long build.", None);
    env.say_json(r#"{"verdict":"WAIT","reason":"build","wait_pid":4242}"#);
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Waiting);
    env.pid_alive.store(false, Ordering::SeqCst);
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Running);
}

#[test]
fn the_turn_cap_pauses_with_state_raises_one_item_and_extend_resumes() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", Some(2));
    let env = Stub::new().check(&[1, 1]);
    end_turn(&db, TaskStatus::ReadyForReview, "Tried.", None);
    env.say("CONTINUE");
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).turns, 2);
    end_turn(&db, TaskStatus::ReadyForReview, "Tried again.", None);
    env.say("CONTINUE");
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!(g.status, GoalStatus::Paused);
    assert!(g.paused_reason.contains("2 turns") && g.last_check.is_some() && !g.journal.is_empty(), "paused with its state");
    let items = open_items(&db);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Extend by 5 turns?");
    tick(&db, &env).unwrap();
    assert_eq!(open_items(&db).len(), 1, "one item per pause");
    assert!(controller.goal_command(GoalCommand::Resume { id: id.clone() }).unwrap_err().contains("Extend"));
    controller.goal_command(GoalCommand::Extend { id: id.clone(), turns: 5 }).unwrap();
    assert!(open_items(&db).is_empty());
    let g = goal(&db, &id);
    assert_eq!((g.status, g.budget.max_turns), (GoalStatus::Running, 7));
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.turns, g.in_turn), (3, true));
}

#[test]
fn a_crash_mid_turn_resumes_from_the_journal_in_the_same_session_with_the_goal_in_the_prompt() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new().check(&[1]);
    // Turn 1 finished and was judged; turn 2 is running when the daemon dies.
    end_turn(&db, TaskStatus::ReadyForReview, "Half done.", None);
    env.say_json(r#"{"verdict":"CONTINUE","reason":"two tests fail","done":"wrote the parser","next":"fix the nested quote test","plan":"parser, then tests"}"#);
    tick(&db, &env).unwrap();
    {
        let guard = db.lock().unwrap();
        let mut snapshot = store::load(&guard).unwrap();
        snapshot.tasks[0].status = TaskStatus::Building;
        snapshot.task_sessions.insert("t".into(), "0199aaaa-bbbb-7ccc-8ddd-eeeeffff0000".into());
        store::save(&guard, &snapshot).unwrap();
    }
    // The restart: tickets are recovered first, then the goals resume.
    store::recover_interrupted(&db.lock().unwrap()).unwrap();
    assert_eq!(resume_sweep(&db, store::now_ms()).unwrap(), 1);
    let g = goal(&db, &id);
    let resumed = g.journal.last().unwrap();
    assert_eq!(resumed.note, "Resumed after a restart");
    assert_eq!(resumed.session_id.as_deref(), Some("0199aaaa-bbbb-7ccc-8ddd-eeeeffff0000"), "the same session is named for the resume");
    assert_eq!((g.status, g.in_turn, g.turns), (GoalStatus::Running, true, 2));
    // A fresh session (a runtime that cannot resume) is seeded with the goal, journal and ledger.
    let context = prompt_context(&db.lock().unwrap(), "t");
    for expected in ["Make the parser handle nested quotes", "Do not touch the lexer", "cargo test", "wrote the parser", "fix the nested quote test", "exit 1", "Turn 2 of at most 20"] {
        assert!(context.contains(expected), "the context names {expected}: {context}");
    }
    assert!(prompt_context(&db.lock().unwrap(), "other").is_empty());
    // The recovered ticket runs again and the goal carries on to a finish.
    end_turn(&db, TaskStatus::ReadyForReview, "Done.", None);
    env.say("DONE");
    env.checks.lock().unwrap().push_back(0);
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Done);
}

#[test]
fn a_ticket_that_failed_after_restarts_is_still_continued_by_its_goal() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new().check(&[1]);
    end_turn(&db, TaskStatus::Failed, "", Some("Interrupted by a daemon restart again after resuming twice. Inspect the worktree, then start it again."));
    env.say("CONTINUE");
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.status, g.turns), (GoalStatus::Running, 2));
    assert_eq!(task(&db).status, TaskStatus::Queued, "a failed ticket is queued again with the goal's next turn");
}

#[test]
fn a_stalled_run_is_resumed_once_without_a_judge_then_judged() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new().check(&[1]);
    let stalled = "Agent stopped: no output for 10 minutes while no command was running, so it looked stalled";
    end_turn(&db, TaskStatus::Failed, "", Some(stalled));
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.stall_resumes, g.turns, g.in_turn), (1, 1, true), "resumed, not counted as a new turn");
    assert_eq!(task(&db).status, TaskStatus::Queued);
    assert_eq!(env.judged.load(Ordering::SeqCst), 0, "no judge for a stall");
    assert!(g.journal.iter().any(|j| j.note.contains("Stalled")));
    // It stalls again: one resume only, so now the turn is judged like any failed turn.
    end_turn(&db, TaskStatus::Failed, "", Some(stalled));
    env.say("CONTINUE");
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.turns, env.judged.load(Ordering::SeqCst)), (2, 1));
    assert!(task(&db).events.iter().any(|e| e.message.contains("stopped with") && e.message.contains("looked stalled")));
}

#[test]
fn a_usage_limit_holds_until_the_reset_or_an_hour_then_resumes() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new();
    let reset = env.now_ms() / 1000 + 5 * 3600;
    end_turn(&db, TaskStatus::Failed, "", Some(&format!("Claude AI usage limit reached|{reset}")));
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!(g.status, GoalStatus::Held);
    assert_eq!(g.hold_until_ms, Some(reset * 1000 + 30_000));
    assert_eq!(env.judged.load(Ordering::SeqCst), 0, "no judge and no check while held");
    env.advance(4 * 3_600_000);
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Held, "not before the reset");
    env.advance(2 * 3_600_000);
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.status, g.turns, g.in_turn), (GoalStatus::Running, 1, true), "resumed without spending a turn");
    assert_eq!(task(&db).status, TaskStatus::Queued);
    // No time in the error: an hour.
    end_turn(&db, TaskStatus::Failed, "", Some("You've hit your limit"));
    let before = env.now_ms();
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).hold_until_ms, Some(before + 3_600_000));
}

#[test]
fn a_judge_that_fails_never_ends_the_goal_and_the_cap_still_bounds_it() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", Some(2));
    let env = Stub::new().check(&[1, 1]);
    end_turn(&db, TaskStatus::ReadyForReview, "x", None);
    env.judges.lock().unwrap().push_back(Err("judge offline".into()));
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.status, g.turns, g.judge_failures), (GoalStatus::Running, 2, 1), "fail open: a judge error is a continue");
    end_turn(&db, TaskStatus::ReadyForReview, "x", None);
    env.say_json("not json at all");
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Paused, "at the cap");
}

#[test]
fn a_held_machine_postpones_checks_and_new_turns() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new().check(&[0]);
    env.held.store(true, Ordering::SeqCst);
    end_turn(&db, TaskStatus::ReadyForReview, "x", None);
    tick(&db, &env).unwrap();
    assert_eq!((env.checked.load(Ordering::SeqCst), goal(&db, &id).status), (0, GoalStatus::Running));
    env.held.store(false, Ordering::SeqCst);
    env.say("DONE");
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Done);
}

#[test]
fn the_person_can_pause_resume_mark_done_and_stop_and_a_stale_tick_cannot_undo_it() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new().check(&[1, 1]);
    controller.goal_command(GoalCommand::Pause { id: id.clone() }).unwrap();
    end_turn(&db, TaskStatus::ReadyForReview, "x", None);
    tick(&db, &env).unwrap();
    assert_eq!((env.checked.load(Ordering::SeqCst), goal(&db, &id).status), (0, GoalStatus::Paused), "paused goals are not ticked");
    controller.goal_command(GoalCommand::Resume { id: id.clone() }).unwrap();
    env.say("CONTINUE");
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).turns, 2);
    controller.goal_command(GoalCommand::MarkDone { id: id.clone() }).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Done);
    // A tick that read the goal before the person finished it changes nothing.
    let stale = Goal { status: GoalStatus::Running, ..goal(&db, &id) };
    end_turn(&db, TaskStatus::ReadyForReview, "x", None);
    env.say("CONTINUE");
    step(&db, &env, stale).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Done);
    assert!(controller.goal_command(GoalCommand::Pause { id: id.clone() }).is_err());
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "", None);
    controller.goal_command(GoalCommand::Stop { id: id.clone() }).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Stopped);
    assert!(controller.goal_command(GoalCommand::Extend { id, turns: 5 }).is_err());
}

#[test]
fn a_ticket_waiting_for_plan_approval_parks_its_goal_and_a_cancelled_one_stops_it() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new();
    end_turn(&db, TaskStatus::AwaitingApproval, "", None);
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.status, g.wait.as_ref().is_some_and(|w| w.ticket)), (GoalStatus::Waiting, true));
    end_turn(&db, TaskStatus::Building, "", None);
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Running);
    end_turn(&db, TaskStatus::Cancelled, "", None);
    tick(&db, &env).unwrap();
    assert_eq!(goal(&db, &id).status, GoalStatus::Stopped);
}

#[test]
fn running_time_counts_only_while_running_and_the_clock_has_its_own_row() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "", None);
    let env = Stub::new();
    env.advance(2_000);
    tick(&db, &env).unwrap();
    env.advance(60_000_000);
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert!(g.wall_ms <= 2_000 + MAX_CLOCK_STEP_MS + 15_000, "a gap is not spent: {}", g.wall_ms);
    let row = db.lock().unwrap().get_setting(&format!("neko_goal_v1:{id}")).unwrap().unwrap();
    assert!(row.contains("\"wall_ms\":0"), "the clock is not in the goal's row: {row}");
    let writes = db.lock().unwrap().setting_writes();
    tick(&db, &env).unwrap();
    tick(&db, &env).unwrap();
    assert_eq!(db.lock().unwrap().setting_writes(), writes, "a tick writes the deferred clock row, never the goal's");
}

#[test]
fn the_evidence_gate_sends_code_back_twice_then_blocks_and_skips_prose() {
    let (_controller, db) = fixture(TaskStatus::Building);
    let db = db.lock().unwrap();
    let code = vec!["src/a.rs".to_owned()];
    let ok = format!("VERIFICATION_COMMAND {}", serde_json::json!({"command": "cargo test", "exit_code": 0, "output": "ok"}));
    let bad = format!("VERIFICATION_COMMAND {}", serde_json::json!({"command": "cargo test", "exit_code": 101, "output": "1 failed"}));
    let now = 1_000;
    assert!(matches!(evidence_gate(&db, "t", &["README.md".into()], &[], &[], 0, now).unwrap(), GateOutcome::Pass), "prose skips the gate");
    assert!(matches!(evidence_gate(&db, "t", &code, &[ok.clone()], &[], 0, now).unwrap(), GateOutcome::Pass));
    match evidence_gate(&db, "t", &code, &[bad.clone()], &[], 0, now).unwrap() {
        GateOutcome::Retry(text) => assert!(text.contains("1 failed") && text.contains("Run the repository's checks")),
        _ => panic!("first extra turn"),
    }
    assert!(matches!(evidence_gate(&db, "t", &code, &[], &[], 1, now).unwrap(), GateOutcome::Retry(_)));
    match evidence_gate(&db, "t", &code, &[bad], &[], 2, now).unwrap() {
        GateOutcome::Blocked(message) => assert!(message.starts_with(BLOCKED_PREFIX) && message.contains("1 failed")),
        _ => panic!("blocked after two extra turns"),
    }
    // The independent review's proven checks count, and everything lands in the ledger.
    assert!(matches!(evidence_gate(&db, "t", &code, &[], &[ok], 2, now).unwrap(), GateOutcome::Pass));
    assert!(core::log(&db, "t").unwrap().ledger.iter().any(|e| e.source == "review" && e.passed()));
}

#[test]
fn a_blocked_ticket_turns_into_one_question_with_a_default() {
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "cargo test", None);
    let env = Stub::new();
    end_turn(&db, TaskStatus::Failed, "", Some("Blocked: the checks are not passing after 2 extra turns. `cargo test` fails (exit 101): 1 failed"));
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!(g.status, GoalStatus::Blocked);
    assert!(g.blocked.unwrap().question.contains("1 failed"));
    assert_eq!((env.judged.load(Ordering::SeqCst), open_items(&db).len()), (0, 1));
}

#[test]
fn goals_attach_to_one_ticket_each_and_a_standalone_goal_makes_its_ticket() {
    let (controller, db) = fixture(TaskStatus::Building);
    goal_on_t(&controller, "", None);
    let again = controller.goal_command(GoalCommand::Create { workspace_id: "w".into(), objective: "Another".into(), done_check: String::new(), constraints: String::new(), max_turns: None, ticket_id: Some("t".into()), conversation_id: None });
    assert!(again.unwrap_err().contains("already has a goal"));
    assert!(controller.goal_command(GoalCommand::Create { workspace_id: "w".into(), objective: "  ".into(), done_check: String::new(), constraints: String::new(), max_turns: None, ticket_id: Some("t".into()), conversation_id: None }).is_err());
    // Standalone: a ticket is created in the workspace's repository.
    let repository = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| assert!(std::process::Command::new("git").args(args).current_dir(repository.path()).output().unwrap().status.success());
    git(&["init", "-q"]);
    {
        let guard = db.lock().unwrap();
        let mut snapshot = store::load(&guard).unwrap();
        snapshot.workspaces[0].repository = repository.path().to_string_lossy().into_owned();
        store::save(&guard, &snapshot).unwrap();
    }
    controller.goal_command(GoalCommand::Create { workspace_id: "w".into(), objective: "Keep the docs index current".into(), done_check: "none".into(), constraints: String::new(), max_turns: Some(5), ticket_id: None, conversation_id: Some("home:p:w".into()) }).unwrap();
    let snapshot = store::load(&db.lock().unwrap()).unwrap();
    assert_eq!(snapshot.tasks.len(), 2);
    let made = snapshot.tasks.last().unwrap();
    assert_eq!((made.status, made.request.as_deref()), (TaskStatus::Queued, Some("Keep the docs index current")));
    assert!(snapshot.start_when_planned.contains(&made.id), "the goal is the request for local work");
    let g = snapshot.goals.iter().find(|g| g.ticket_id.as_deref() == Some(made.id.as_str())).expect("goals reach readers through the snapshot");
    assert_eq!((g.budget.max_turns, g.conversation_id.as_deref(), g.done_check.as_str()), (5, Some("home:p:w"), ""));
}

#[test]
fn the_check_runner_reports_exit_code_output_and_ends_a_hung_check() {
    let dir = tempfile::tempdir().unwrap();
    let ok = run_shell(dir.path(), "echo built; echo warned 1>&2; exit 0", Duration::from_secs(20), 7);
    assert_eq!((ok.exit_code, ok.at_ms, ok.source.as_str()), (Some(0), 7, "neko"));
    assert!(ok.tail.contains("built") && ok.tail.contains("warned"), "stdout and stderr are both kept: {}", ok.tail);
    let failed = run_shell(dir.path(), "echo nope; exit 3", Duration::from_secs(20), 7);
    assert_eq!(failed.exit_code, Some(3));
    let hung = run_shell(dir.path(), "sleep 30", Duration::from_millis(300), 7);
    assert_eq!(hung.exit_code, None);
    assert!(hung.tail.contains("Ended after"));
    let local = run_shell(dir.path(), "pwd", Duration::from_secs(20), 7);
    assert!(local.tail.contains(dir.path().file_name().unwrap().to_str().unwrap()), "runs in the task's folder");
}

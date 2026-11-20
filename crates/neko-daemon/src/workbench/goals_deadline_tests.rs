//! The deadline default: a blocked goal takes its own default after a wait the person chose, only with a
//! checkpoint to undo to, and the person can take it back for seven days.
use super::tests::{fixture, goal, goal_on_t, Stub};
use super::*;
use super::super::run_checkpoints::tests::{a_run, fixture as git_fixture};

const HOUR: i64 = 3_600_000;

/// A goal on ticket "t", blocked on one question with this default, asked at the stub's now.
fn blocked(controller: &Controller, db: &Arc<Mutex<Db>>, env: &Stub, hours: Option<u32>, default: &str) -> String {
    let id = goal_on_t(controller, "", None);
    if hours.is_some() {
        controller.goal_command(GoalCommand::SetDefaultAfter { id: id.clone(), hours }).unwrap();
    }
    let guard = db.lock().unwrap();
    let mut g = core::load(&guard, &id).unwrap().unwrap();
    super::block_for_test(&guard, &mut g, "Which database?", default, env.now_ms()).unwrap();
    core::save(&guard, &g).unwrap();
    id
}

fn took(db: &Arc<Mutex<Db>>, id: &str) -> Vec<String> {
    let subject = goal(db, id).subject();
    let log = core::log(&db.lock().unwrap(), &subject).unwrap();
    log.journal.into_iter().map(|e| e.note).filter(|n| n.starts_with("Took the default")).collect()
}

fn open_questions(db: &Arc<Mutex<Db>>) -> usize {
    crate::routines::all_items(&db.lock().unwrap()).unwrap().into_iter().filter(|i| !i.done && i.link.as_deref().is_some_and(|l| l.starts_with("goal:"))).count()
}

#[test]
fn a_blocked_goal_takes_its_default_one_hour_after_it_asked_with_the_exact_journal_line() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(1), "the primary");
    assert_eq!(open_questions(&c.db), 1);
    // Before the deadline nothing happens.
    env.advance(HOUR - 1_000);
    tick(&c.db, &env).unwrap();
    assert_eq!(goal(&c.db, &id).status, GoalStatus::Blocked);
    env.advance(1_000);
    tick(&c.db, &env).unwrap();
    let g = goal(&c.db, &id);
    assert_eq!((g.status, g.in_turn, g.blocked.is_none()), (GoalStatus::Running, false, true));
    assert_eq!(took(&c.db, &id), ["Took the default after 1 h: the primary"]);
    assert!(g.reason.contains("the primary") && g.reason.contains("Which database?"));
    assert_eq!(open_questions(&c.db), 0, "its Inbox item closed");
    // Once only.
    tick(&c.db, &env).unwrap();
    assert_eq!(took(&c.db, &id).len(), 1);
}

#[test]
fn the_undo_target_is_the_next_runs_start_and_is_kept_seven_days() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    a_run(&c, &worktree, "three\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(2), "the primary");
    env.advance(2 * HOUR);
    tick(&c.db, &env).unwrap();
    let taken = goal(&c.db, &id).default_taken.expect("a target");
    assert_eq!(neko_core::checkpoints::row(&c.db, "t").unwrap().next_run, 2);
    assert_eq!(taken.undo_run, 3, "next_run + 1");
    assert_eq!((taken.after_hours, taken.answer.as_str(), taken.question.as_str()), (2, "the primary", "Which database?"));
    assert_eq!(taken.expires_ms, taken.at_ms + 7 * 24 * HOUR);
    assert_eq!(taken.at_ms, env.now_ms());
}

#[test]
fn no_deadline_no_default_and_a_generic_filler_are_never_answered() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let off = blocked(&c, &c.db, &env, None, "the primary");
    env.advance(1000 * HOUR);
    tick(&c.db, &env).unwrap();
    assert_eq!(goal(&c.db, &off).status, GoalStatus::Blocked, "off by default");
    // Another goal needs another ticket: close the first, then ask again with a deadline and a filler default.
    c.goal_command(GoalCommand::Stop { id: off }).unwrap();
    let env = Stub::new();
    let filler = blocked(&c, &c.db, &env, Some(1), "");
    let g = goal(&c.db, &filler);
    assert!(!g.blocked.as_ref().unwrap().real_default && g.blocked.as_ref().unwrap().default_answer == "Use your best judgement and keep going.");
    env.advance(1000 * HOUR);
    tick(&c.db, &env).unwrap();
    assert_eq!(goal(&c.db, &filler).status, GoalStatus::Blocked, "the filler never triggers an answer");
    assert!(took(&c.db, &filler).is_empty());
}

#[test]
fn with_no_checkpoint_nothing_is_answered_the_deadline_goes_off_and_one_line_says_why() {
    let (c, db) = fixture(TaskStatus::Building);
    let env = Stub::new();
    let id = blocked(&c, &db, &env, Some(1), "the primary");
    env.advance(HOUR);
    tick(&db, &env).unwrap();
    let g = goal(&db, &id);
    assert_eq!(g.status, GoalStatus::Blocked);
    assert!(g.blocked.as_ref().unwrap().deadline_off);
    let subject = g.subject();
    let lines = |db: &Arc<Mutex<Db>>| core::log(&db.lock().unwrap(), &subject).unwrap().journal.into_iter().filter(|e| e.note.starts_with("Waited 1 h for you.")).map(|e| e.note).collect::<Vec<_>>();
    assert_eq!(lines(&db), ["Waited 1 h for you. No checkpoint exists to undo to, so the default was not taken."]);
    env.advance(HOUR);
    tick(&db, &env).unwrap();
    assert_eq!(lines(&db).len(), 1, "not repeated on the next tick");
    assert_eq!(goal(&db, &id).status, GoalStatus::Blocked);
}

#[test]
fn a_persons_answer_arriving_first_wins_and_the_tick_then_does_nothing() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(1), "the primary");
    c.goal_command(GoalCommand::Answer { id: id.clone(), text: "the replica".into() }).unwrap();
    env.advance(5 * HOUR);
    tick(&c.db, &env).unwrap();
    let g = goal(&c.db, &id);
    assert!(g.default_taken.is_none() && took(&c.db, &id).is_empty());
    assert!(g.journal.iter().any(|e| e.note == "Answered: the replica"));
    // The race itself: the tick looked at the goal as blocked, the person answered, then the tick commits.
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(1), "the primary");
    let stale = goal(&c.db, &id);
    c.goal_command(GoalCommand::Answer { id: id.clone(), text: "the replica".into() }).unwrap();
    env.advance(HOUR);
    blocked_deadline(&c.db, &env, stale).unwrap();
    assert!(goal(&c.db, &id).default_taken.is_none() && took(&c.db, &id).is_empty(), "the stale deadline was dropped");
}

#[test]
fn undo_after_the_next_turn_ran_restores_the_folder_and_asks_the_question_again() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(1), "the primary");
    env.advance(HOUR);
    tick(&c.db, &env).unwrap();
    assert_eq!(goal(&c.db, &id).default_taken.as_ref().unwrap().undo_run, 2);
    // The turn that followed the answer ran and changed the folder.
    a_run(&c, &worktree, "three\n");
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "three\n");
    let snapshot = c.goal_command(GoalCommand::UndoDefault { goal: id.clone() }).unwrap();
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "two\n", "back to the start of that run");
    let g = goal(&c.db, &id);
    let b = g.blocked.as_ref().expect("the question is back");
    assert_eq!((g.status, g.in_turn, b.question.as_str(), b.default_answer.as_str()), (GoalStatus::Blocked, false, "Which database?", "the primary"));
    assert!(b.deadline_off && b.real_default && b.inbox_seq.is_some() && b.asked_ms > 0);
    assert!(g.default_taken.is_none() && g.paused_reason.is_empty());
    assert!(g.journal.iter().any(|e| e.note == "Undid the default answer: the primary"));
    assert_eq!(open_questions(&c.db), 1, "raised again");
    assert!(snapshot.tasks[0].events.iter().any(|e| e.message.starts_with("checkpoint: Undid run")), "the ticket's history says so");
    // The same question is never auto-answered again.
    env.advance(1000 * HOUR);
    tick(&c.db, &env).unwrap();
    assert_eq!(goal(&c.db, &id).status, GoalStatus::Blocked);
    assert_eq!(took(&c.db, &id).len(), 1);
}

#[test]
fn undo_before_any_turn_started_restores_only_the_goal() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(1), "the primary");
    env.advance(HOUR);
    tick(&c.db, &env).unwrap();
    std::fs::write(worktree.join("a.txt"), "unrelated edit\n").unwrap();
    c.goal_command(GoalCommand::UndoDefault { goal: id.clone() }).unwrap();
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "unrelated edit\n", "no run started, so no file is touched");
    let g = goal(&c.db, &id);
    assert_eq!(g.status, GoalStatus::Blocked);
    assert!(g.blocked.as_ref().unwrap().deadline_off && g.default_taken.is_none());
}

#[test]
fn undo_is_refused_with_the_checkpoint_code_when_the_ticket_is_busy_and_nothing_changes() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(1), "the primary");
    env.advance(HOUR);
    tick(&c.db, &env).unwrap();
    a_run(&c, &worktree, "three\n");
    c.active.lock().unwrap().push(Active { task_id: "t".into(), cancelled: Arc::new(AtomicBool::new(false)) });
    let before = goal(&c.db, &id);
    let error = c.goal_command(GoalCommand::UndoDefault { goal: id.clone() }).unwrap_err();
    assert!(error.starts_with("checkpoint:busy:"), "{error}");
    c.active.lock().unwrap().clear();
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "three\n", "the folder is untouched");
    let after = goal(&c.db, &id);
    assert_eq!((after.status, after.default_taken.clone(), after.paused_reason.clone()), (before.status, before.default_taken.clone(), String::new()), "the goal is not paused");
    // With the worker gone it goes ahead.
    c.goal_command(GoalCommand::UndoDefault { goal: id.clone() }).unwrap();
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "two\n");
}

#[test]
fn a_queued_next_turn_cannot_be_taken_back_by_a_goal_only_undo() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(1), "the primary");
    env.advance(HOUR);
    tick(&c.db, &env).unwrap();
    {
        let db = c.db.lock().unwrap();
        let mut g = core::load(&db, &id).unwrap().unwrap();
        g.in_turn = true;
        core::save(&db, &g).unwrap();
    }
    let error = c.goal_command(GoalCommand::UndoDefault { goal: id.clone() }).unwrap_err();
    assert!(error.starts_with("checkpoint:busy:"), "{error}");
    assert_eq!(goal(&c.db, &id).status, GoalStatus::Running);
}

#[test]
fn undo_is_refused_after_expiry_and_the_tick_clears_an_expired_target() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(1), "the primary");
    env.advance(HOUR);
    tick(&c.db, &env).unwrap();
    {
        let db = c.db.lock().unwrap();
        let mut g = core::load(&db, &id).unwrap().unwrap();
        g.default_taken.as_mut().unwrap().expires_ms = store::now_ms() - 1;
        core::save(&db, &g).unwrap();
    }
    let error = c.goal_command(GoalCommand::UndoDefault { goal: id.clone() }).unwrap_err();
    assert_eq!(error, "That undo is no longer available (kept 7 days)");
    // The tick's own clock is the stub's: move it past the target.
    env.advance(8 * 24 * HOUR);
    tick(&c.db, &env).unwrap();
    let g = goal(&c.db, &id);
    assert!(g.default_taken.is_none());
    assert!(g.journal.iter().any(|e| e.note == "The undo for the default answer expired"));
    assert_eq!(c.goal_command(GoalCommand::UndoDefault { goal: id }).unwrap_err(), "There is no default answer to undo");
}

#[test]
fn undo_on_a_finished_goal_is_refused() {
    let (c, _repo, worktree) = git_fixture(TaskStatus::ReadyForReview);
    a_run(&c, &worktree, "two\n");
    let env = Stub::new();
    let id = blocked(&c, &c.db, &env, Some(1), "the primary");
    env.advance(HOUR);
    tick(&c.db, &env).unwrap();
    c.goal_command(GoalCommand::MarkDone { id: id.clone() }).unwrap();
    assert_eq!(c.goal_command(GoalCommand::UndoDefault { goal: id.clone() }).unwrap_err(), "That goal already finished");
    // Its receipt still tells the story.
    assert_eq!(took(&c.db, &id), ["Took the default after 1 h: the primary"]);
}

#[test]
fn the_deadline_is_set_by_the_slash_command_the_create_command_and_set_default_after() {
    assert_eq!(
        slash("/goal fix it --check \"make test\" --default-after 8"),
        Some(Slash::Set { objective: "fix it".into(), done_check: "\"make test\"".into(), max_turns: None, default_after_hours: Some(8) })
    );
    assert_eq!(
        slash("/goal ship --turns 5 --default-after 3"),
        Some(Slash::Set { objective: "ship".into(), done_check: String::new(), max_turns: Some(5), default_after_hours: Some(3) })
    );
    assert_eq!(
        slash("/goal ship --default-after 3 --turns 5"),
        Some(Slash::Set { objective: "ship".into(), done_check: String::new(), max_turns: Some(5), default_after_hours: Some(3) })
    );
    let (c, _repo, _worktree) = git_fixture(TaskStatus::Completed);
    let db = c.db.clone();
    let Some(command) = slash("/goal fix it --check make-test --default-after 8") else { panic!("not a goal") };
    let (reply, tickets) = slash_reply(&db.lock().unwrap(), command, Some("w"));
    assert!(reply.starts_with("Goal set:"), "{reply}");
    assert_eq!(tickets.len(), 1);
    let stored = core::load_all(&db.lock().unwrap()).unwrap().pop().unwrap();
    assert_eq!(stored.default_after_hours, Some(8));
    // The Create command carries it too, and a bad number is refused before anything is made.
    let id = stored.id;
    assert_eq!(c.goal_command(GoalCommand::SetDefaultAfter { id: id.clone(), hours: Some(0) }).unwrap_err(), "Choose 1 to 168 hours, or turn it off");
    assert!(c.goal_command(GoalCommand::SetDefaultAfter { id: id.clone(), hours: Some(200) }).is_err());
    assert_eq!(goal(&db, &id).default_after_hours, Some(8), "a refusal changes nothing");
    c.goal_command(GoalCommand::SetDefaultAfter { id: id.clone(), hours: Some(168) }).unwrap();
    assert_eq!(goal(&db, &id).default_after_hours, Some(168));
    c.goal_command(GoalCommand::SetDefaultAfter { id: id.clone(), hours: None }).unwrap();
    assert_eq!(goal(&db, &id).default_after_hours, None, "None turns it off");
    let (c2, db2) = fixture(TaskStatus::Building);
    assert!(c2.goal_command(GoalCommand::Create { workspace_id: "w".into(), objective: "x".into(), done_check: String::new(), constraints: String::new(), max_turns: None, ticket_id: Some("t".into()), conversation_id: None, default_after_hours: Some(0) }).is_err());
    assert!(core::load_all(&db2.lock().unwrap()).unwrap().is_empty());
    c2.goal_command(GoalCommand::Create { workspace_id: "w".into(), objective: "x".into(), done_check: String::new(), constraints: String::new(), max_turns: None, ticket_id: Some("t".into()), conversation_id: None, default_after_hours: Some(24) }).unwrap();
    assert_eq!(core::load_all(&db2.lock().unwrap()).unwrap()[0].default_after_hours, Some(24));
}

#[test]
fn a_goal_a_model_proposed_ignores_the_deadline() {
    let (_c, db) = fixture(TaskStatus::Building);
    let guard = db.lock().unwrap();
    let made = create(&guard, &Spec { workspace_id: "w", objective: "Tidy the docs", done_check: "", constraints: "", max_turns: None, ticket_id: Some("t"), conversation_id: None, default_after_hours: Some(8), by_person: false }).unwrap();
    assert_eq!(made.default_after_hours, None);
    assert_eq!(core::load(&guard, &made.id).unwrap().unwrap().default_after_hours, None);
}

#[test]
fn goal_rows_from_before_deadlines_load_and_are_never_auto_answered() {
    let old: Goal = serde_json::from_str(r#"{"id":"g","workspace_id":"w","objective":"x","status":"Blocked","blocked":{"question":"q","default_answer":"a","inbox_seq":3}}"#).unwrap();
    assert_eq!((old.default_after_hours, old.default_taken.is_none()), (None, true));
    let b = old.blocked.as_ref().unwrap();
    assert_eq!((b.asked_ms, b.real_default, b.deadline_off), (0, false, false));
    // Even with a deadline set afterwards, a question asked before this build is left to the person.
    let with_deadline = Goal { default_after_hours: Some(1), ..old };
    assert!(!core::deadline_due(&with_deadline, i64::MAX / 2));
}

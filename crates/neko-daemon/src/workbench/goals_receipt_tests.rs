//! A finished goal's receipt: one markdown artifact, redacted, written after the goal is saved Done.
use super::tests::{end_turn, fixture, goal, goal_on_t, Stub};
use super::*;

/// Receipts go to a folder of this test's own; the guard keeps it alive.
fn receipts_in(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let root = dir.path().join("artifacts");
    TEST_RECEIPTS.with(|r| *r.borrow_mut() = Some(root.clone()));
    root
}

fn texts(root: &std::path::Path) -> Vec<(String, String)> {
    neko_core::artifacts::list(root, None)
        .into_iter()
        .map(|a| {
            let (file, _) = neko_core::artifacts::version_file(root, &a.id, 1).expect("version 1");
            (a.title.clone(), std::fs::read_to_string(file).unwrap())
        })
        .collect()
}

/// A goal on ticket "t" ruled DONE by the judge with a passing check.
fn done_by_the_judge(controller: &Controller, db: &Arc<Mutex<Db>>) -> String {
    let id = goal_on_t(controller, "cargo test", None);
    let env = Stub::new().check(&[0]);
    end_turn(db, TaskStatus::ReadyForReview, "Fixed.", None);
    env.say("DONE");
    tick(db, &env).unwrap();
    id
}

#[test]
fn a_goal_the_judge_rules_done_gets_one_receipt_linked_from_the_goal() {
    let dir = tempfile::tempdir().unwrap();
    let root = receipts_in(&dir);
    let (controller, db) = fixture(TaskStatus::Building);
    let id = done_by_the_judge(&controller, &db);
    let g = goal(&db, &id);
    assert_eq!(g.status, GoalStatus::Done);
    let receipt = g.receipt.clone().expect("the goal links its receipt");
    let all = texts(&root);
    assert_eq!(all.len(), 1);
    let (title, text) = &all[0];
    assert_eq!(title, "Receipt: Make the parser handle nested quotes");
    assert_eq!(neko_core::artifacts::list(&root, None)[0].id, receipt.artifact_id);
    let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(headings, ["## Objective", "## Checks", "## Changes", "## Journal", "## Cost and time"], "Pull request only when the ticket has one");
    assert!(text.contains("### `cargo test` exit 0 (") && text.contains("check output, exit 0"), "{text}");
    assert!(text.contains("No checkpoints were taken for this goal."), "{text}");
    assert!(text.contains("- turn 1: ") && text.contains("DONE"), "the journal is in it: {text}");
    // Another pass of the goal tick writes no second one.
    tick(&db, &Stub::new()).unwrap();
    assert_eq!(texts(&root).len(), 1);
    // The kept receipt is the task's artifact: the ticket links it.
    assert_eq!(neko_core::artifacts::list_for_task(&root, "t").len(), 1);
}

#[test]
fn mark_done_writes_a_receipt_too_and_a_second_call_writes_none() {
    let dir = tempfile::tempdir().unwrap();
    let root = receipts_in(&dir);
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "", None);
    controller.goal_command(GoalCommand::MarkDone { id: id.clone() }).unwrap();
    assert!(goal(&db, &id).receipt.is_some());
    let all = texts(&root);
    assert_eq!(all.len(), 1);
    assert!(all[0].1.contains("You marked it done.") && all[0].1.contains("No check was run."), "{}", all[0].1);
    controller.goal_command(GoalCommand::MarkDone { id: id.clone() }).unwrap();
    assert_eq!(texts(&root).len(), 1, "idempotent");
}

#[test]
fn the_checks_section_lists_every_ledger_entry_with_its_last_twenty_lines() {
    let dir = tempfile::tempdir().unwrap();
    let root = receipts_in(&dir);
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "make test", None);
    let subject = goal(&db, &id).subject();
    {
        let guard = db.lock().unwrap();
        let tail: String = (1..=30).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
        neko_core::goals::push_ledger(&guard, &subject, CheckResult { command: "make test".into(), exit_code: Some(2), tail, at_ms: 1, source: "neko".into() }).unwrap();
        neko_core::goals::push_ledger(&guard, &subject, CheckResult { command: "make lint".into(), exit_code: Some(0), tail: "clean".into(), at_ms: 2, source: "review".into() }).unwrap();
    }
    controller.goal_command(GoalCommand::MarkDone { id }).unwrap();
    let text = &texts(&root)[0].1;
    assert!(text.contains("### `make test` exit 2 (") && text.contains("### `make lint` exit 0 ("), "{text}");
    assert!(text.find("make test").unwrap() < text.find("make lint").unwrap(), "oldest first");
    assert!(text.contains("line 30") && text.contains("line 11") && !text.contains("line 10\n"), "the last twenty lines");
}

#[test]
fn the_changes_section_reads_the_whole_task_from_its_checkpoints() {
    let dir = tempfile::tempdir().unwrap();
    let root = receipts_in(&dir);
    let (c, _repo, worktree) = super::super::run_checkpoints::tests::fixture(TaskStatus::ReadyForReview);
    super::super::run_checkpoints::tests::a_run(&c, &worktree, "two\n");
    super::super::run_checkpoints::tests::a_run(&c, &worktree, "three\n");
    let id = goal_on_t(&c, "", None);
    c.goal_command(GoalCommand::MarkDone { id }).unwrap();
    let text = &texts(&root)[0].1;
    assert!(text.contains("Files changed: 1, +1 -1 (whole task: first run's start to last run's end)\n- a.txt\n"), "{text}");
    assert!(!text.contains("No checkpoints were taken"));
}

#[test]
fn cost_and_time_come_from_the_usage_ledger_of_this_goals_ticket_only() {
    let dir = tempfile::tempdir().unwrap();
    let root = receipts_in(&dir);
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "", None);
    let line = |key: &str, task: &str, cost: Option<u64>, tokens: Option<(u64, u64)>| neko_core::usage_ledger::UsageEvent {
        run_key: key.into(), at_ms: 10, provider: "claude".into(), model: "sonnet".into(), input_tokens: tokens.map(|t| t.0), output_tokens: tokens.map(|t| t.1), cost_micro_usd: cost,
        source: neko_core::usage_ledger::estimate_source(cost, tokens.map(|t| t.0), tokens.map(|t| t.1)), duration_ms: 1000, task_id: task.into(), kind: "ticket".into(),
    };
    {
        let guard = db.lock().unwrap();
        guard.record_usage_event(&line("a", "t", Some(1_000_000), Some((100, 20)))).unwrap();
        guard.record_usage_event(&line("b", "t", Some(500_000), Some((50, 5)))).unwrap();
        guard.record_usage_event(&line("c", "t", None, None)).unwrap();
        guard.record_usage_event(&line("other", "someone-else", Some(9_000_000), None)).unwrap();
    }
    controller.goal_command(GoalCommand::MarkDone { id }).unwrap();
    let text = &texts(&root)[0].1;
    assert!(text.contains("Cost: $1.50 reported across 3 runs (1 run reported no price)\n"), "{text}");
    assert!(text.contains("Input and output: 150 in, 25 out (reported runs only)\n"), "{text}");
}

#[test]
fn a_secret_planted_in_a_checks_output_never_reaches_the_stored_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = receipts_in(&dir);
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "make deploy-check", None);
    let subject = goal(&db, &id).subject();
    let github = format!("ghp_{}", "q".repeat(36));
    let tail = format!("deploying\nAWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\nAuthorization: Bearer abc.def.ghi\nuser password: hunter2\n{github}\nall done");
    neko_core::goals::push_ledger(&db.lock().unwrap(), &subject, CheckResult { command: "make deploy-check".into(), exit_code: Some(0), tail, at_ms: 1, source: "neko".into() }).unwrap();
    controller.goal_command(GoalCommand::MarkDone { id }).unwrap();
    let (_, text) = &texts(&root)[0];
    for secret in ["wJalrXUtnFEMI", "abc.def.ghi", "hunter2", &github, "AWS_SECRET_ACCESS_KEY"] {
        assert!(!text.contains(secret), "{secret} leaked: {text}");
    }
    assert!(text.contains("deploying") && text.contains("all done") && text.contains("[redacted]"));
    // Nor is it left in the folder it was written from.
    assert!(!std::env::temp_dir().join("neko-receipts").read_dir().map_or(false, |mut d| d.any(|e| e.is_ok_and(|e| std::fs::read_to_string(e.path()).is_ok_and(|t| t.contains("hunter2"))))));
}

#[test]
fn a_failing_save_leaves_the_goal_done_notes_it_and_the_tick_tries_three_times_in_all() {
    let dir = tempfile::tempdir().unwrap();
    // The receipts folder is a file, so nothing can be saved under it.
    let blocked = dir.path().join("artifacts");
    std::fs::write(&blocked, "in the way").unwrap();
    TEST_RECEIPTS.with(|r| *r.borrow_mut() = Some(blocked.clone()));
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "", None);
    controller.goal_command(GoalCommand::MarkDone { id: id.clone() }).unwrap();
    let g = goal(&db, &id);
    assert_eq!((g.status, g.receipt.is_none(), g.receipt_tries), (GoalStatus::Done, true, 1));
    let failures = |db: &Arc<Mutex<Db>>, id: &str| goal(db, id).journal.iter().filter(|e| e.note.starts_with("Could not write the receipt: ")).count();
    assert_eq!(failures(&db, &id), 1);
    let env = Stub::new();
    for _ in 0..6 {
        tick(&db, &env).unwrap();
    }
    let g = goal(&db, &id);
    assert_eq!((g.status, g.receipt_tries, failures(&db, &id)), (GoalStatus::Done, 3, 3), "three tries in all, then it leaves the goal alone");
    // A goal whose try has not run out is picked up when the folder works again.
    let (controller, db) = fixture(TaskStatus::Building);
    let id = goal_on_t(&controller, "", None);
    controller.goal_command(GoalCommand::MarkDone { id: id.clone() }).unwrap();
    assert_eq!(goal(&db, &id).receipt_tries, 1);
    let fixed = tempfile::tempdir().unwrap();
    let root = receipts_in(&fixed);
    tick(&db, &env).unwrap();
    assert!(goal(&db, &id).receipt.is_some());
    assert_eq!(texts(&root).len(), 1);
}

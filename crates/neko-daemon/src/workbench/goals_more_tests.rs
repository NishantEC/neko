//! Review fixes: evidence that is real, atomic creation, honest pauses, readers without clocks.
use super::*;

fn fixture(status: TaskStatus) -> (Controller, Arc<Mutex<Db>>) {
    let controller = super::super::tests::controller_with_task(status);
    let db = controller.db.clone();
    (controller, db)
}

fn create_on_t(controller: &Controller) -> Result<Snapshot, String> {
    controller.goal_command(GoalCommand::Create { workspace_id: "w".into(), objective: "Finish it".into(), done_check: String::new(), constraints: String::new(), max_turns: None, ticket_id: Some("t".into()), conversation_id: None })
}

#[test]
fn an_artifact_counts_only_when_this_goal_made_or_changed_it_inside_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "old").unwrap();
    std::fs::create_dir(dir.path().join("out")).unwrap();
    let before = store::now_ms();
    std::thread::sleep(Duration::from_millis(30));
    let start = store::now_ms();
    std::thread::sleep(Duration::from_millis(30));
    std::fs::write(dir.path().join("out/report.md"), "new").unwrap();
    assert!(artifact_since(dir.path(), "out/report.md", start), "a file written after the goal began");
    assert!(artifact_since(dir.path(), "out", start), "a folder with something new in it");
    assert!(!artifact_since(dir.path(), "README.md", start), "a file that was already there proves nothing");
    assert!(artifact_since(dir.path(), "README.md", before - 60_000), "unless the goal began before it was written");
    for bad in [".", "./", "", "nope.md", "../elsewhere", "/etc/hosts"] {
        assert!(!artifact_since(dir.path(), bad, 0), "{bad:?}");
    }
    // A link out of the folder is not inside it.
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("x.md"), "x").unwrap();
    std::os::unix::fs::symlink(outside.path().join("x.md"), dir.path().join("link.md")).unwrap();
    assert!(!artifact_since(dir.path(), "link.md", 0), "a symlink to outside is not an artifact");
}

#[test]
fn a_refused_goal_changes_nothing_about_the_ticket() {
    let (controller, db) = fixture(TaskStatus::Queued);
    create_on_t(&controller).unwrap();
    let before = store::load(&db.lock().unwrap()).unwrap();
    assert!(before.start_when_planned.contains("t"), "a goal the person sets asks for the work to start");
    // Take it back, then a second goal on the same ticket is refused with nothing changed.
    {
        let guard = db.lock().unwrap();
        let mut s = store::load(&guard).unwrap();
        s.start_when_planned.remove("t");
        store::save(&guard, &s).unwrap();
    }
    assert!(create_on_t(&controller).unwrap_err().contains("already has a goal"));
    let after = store::load(&db.lock().unwrap()).unwrap();
    assert!(!after.start_when_planned.contains("t"), "the refused goal did not touch start_when_planned");
    assert_eq!(after.goals.len(), 1);
}

#[test]
fn readers_get_no_clock_so_a_tick_never_changes_the_snapshot() {
    let (controller, db) = fixture(TaskStatus::Building);
    create_on_t(&controller).unwrap();
    {
        let guard = db.lock().unwrap();
        let mut goal = core::load_all(&guard).unwrap().remove(0);
        goal.wall_ms = 123_000;
        goal.last_tick_ms = 99;
        core::save_clock_only(&guard, &goal).unwrap();
    }
    let snapshot = store::load(&db.lock().unwrap()).unwrap();
    assert_eq!((snapshot.goals[0].wall_ms, snapshot.goals[0].last_tick_ms), (0, 0));
    assert_eq!(core::load_all(&db.lock().unwrap()).unwrap()[0].wall_ms, 123_000, "the loop still sees it");
}

#[test]
fn pausing_a_blocked_goal_and_resuming_it_brings_the_question_back_and_a_hold_is_not_extended() {
    let (controller, db) = fixture(TaskStatus::Building);
    create_on_t(&controller).unwrap();
    let id = core::load_all(&db.lock().unwrap()).unwrap()[0].id.clone();
    {
        let guard = db.lock().unwrap();
        let mut g = core::load(&guard, &id).unwrap().unwrap();
        g.status = GoalStatus::Blocked;
        g.blocked = Some(Blocked { question: "Which?".into(), default_answer: "A".into(), inbox_seq: None });
        core::save(&guard, &g).unwrap();
    }
    controller.goal_command(GoalCommand::Pause { id: id.clone() }).unwrap();
    controller.goal_command(GoalCommand::Resume { id: id.clone() }).unwrap();
    let g = core::load(&db.lock().unwrap(), &id).unwrap().unwrap();
    assert_eq!((g.status, g.blocked.is_some()), (GoalStatus::Blocked, true), "back to the question, not running with it hidden");
    {
        let guard = db.lock().unwrap();
        let mut g = core::load(&guard, &id).unwrap().unwrap();
        g.status = GoalStatus::Held;
        g.blocked = None;
        g.hold_until_ms = Some(5);
        core::save(&guard, &g).unwrap();
    }
    controller.goal_command(GoalCommand::Pause { id: id.clone() }).unwrap();
    controller.goal_command(GoalCommand::Resume { id: id.clone() }).unwrap();
    let g = core::load(&db.lock().unwrap(), &id).unwrap().unwrap();
    assert_eq!((g.status, g.hold_until_ms), (GoalStatus::Held, Some(5)), "the same hold, not a new hour");
}

#[test]
fn a_turn_the_person_started_is_adopted_not_raced() {
    let (controller, db) = fixture(TaskStatus::Building);
    create_on_t(&controller).unwrap();
    let id = core::load_all(&db.lock().unwrap()).unwrap()[0].id.clone();
    {
        let guard = db.lock().unwrap();
        let mut g = core::load(&guard, &id).unwrap().unwrap();
        g.in_turn = false;
        core::save(&guard, &g).unwrap();
    }
    // The ticket is working (the person replied on it): no second turn is issued, the run is the turn.
    struct Quiet;
    impl Env for Quiet {
        fn now_ms(&self) -> i64 { store::now_ms() }
        fn run_check(&self, _: &std::path::Path, _: &str) -> CheckResult { panic!("no check while the turn runs") }
        fn judge(&self, _: &str) -> Result<String, String> { panic!("no judge while the turn runs") }
        fn machine_held(&self) -> bool { false }
        fn pr_state(&self, _: &str) -> Option<String> { None }
        fn pid_alive(&self, _: u32) -> bool { true }
        fn url_ok(&self, _: &str) -> bool { false }
    }
    tick(&db, &Quiet).unwrap();
    let g = core::load(&db.lock().unwrap(), &id).unwrap().unwrap();
    assert!(g.in_turn, "adopted");
    assert_eq!(g.turns, 1);
}

#[test]
fn a_wait_on_a_pull_request_or_process_always_has_an_end() {
    let r = core::parse_ruling(r#"{"verdict":"WAIT","reason":"merge","wait_pr":"acme/app#12"}"#, 1_000).unwrap();
    assert_eq!(r.wait.unwrap().until_ms, Some(1_000 + core::MAX_WAIT_MS));
}

#[test]
fn a_goal_blocks_pruning_and_parking_while_it_is_unfinished() {
    let (controller, db) = fixture(TaskStatus::Building);
    assert!(!has_open_goal(&db.lock().unwrap(), "t"));
    create_on_t(&controller).unwrap();
    assert!(has_open_goal(&db.lock().unwrap(), "t"));
    let id = core::load_all(&db.lock().unwrap()).unwrap()[0].id.clone();
    controller.goal_command(GoalCommand::Stop { id }).unwrap();
    assert!(!has_open_goal(&db.lock().unwrap(), "t"));
}

#[test]
fn a_tickets_ledger_goes_when_the_ticket_does() {
    let (_controller, db) = fixture(TaskStatus::Completed);
    {
        let guard = db.lock().unwrap();
        core::push_ledger(&guard, "t", CheckResult { command: "cargo test".into(), exit_code: Some(0), tail: String::new(), at_ms: 1, source: "review".into() }).unwrap();
        assert_eq!(core::log(&guard, "t").unwrap().ledger.len(), 1);
    }
    store::apply(&db.lock().unwrap(), Command::DeleteTask { task_id: "t".into() }).unwrap();
    assert!(core::log(&db.lock().unwrap(), "t").unwrap().ledger.is_empty());
}

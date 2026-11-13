//! The chat's two ways to set a goal: `/goal <text>` and a `neko-goal` block.
use super::*;

fn workspace_with_repo() -> (Controller, Arc<Mutex<Db>>, String, tempfile::TempDir) {
    let repo = tempfile::tempdir().unwrap();
    assert!(std::process::Command::new("git").args(["init", "--quiet"]).arg(repo.path()).status().unwrap().success());
    let db = Arc::new(Mutex::new(Db::open_in_memory().unwrap()));
    let workspace = {
        let db = db.lock().unwrap();
        store::apply(&db, Command::SaveWorkspaceWithFolders {
            workspace: Workspace { id: String::new(), name: "Work".into(), repository: repo.path().to_string_lossy().into(), instructions: String::new(), away_enabled: false },
            folders: vec![repo.path().to_string_lossy().into()],
        }).unwrap().workspaces[0].id.clone()
    };
    (Controller::new(db.clone()), db, workspace, repo)
}

#[test]
fn slash_goal_reads_the_objective_the_check_and_the_turns() {
    assert_eq!(slash("/goal"), Some(Slash::List));
    assert_eq!(slash("/goalpost"), None);
    assert_eq!(slash("goal: x"), None);
    assert_eq!(
        slash("/goal Make the build green --check cargo test --turns 8"),
        Some(Slash::Set { objective: "Make the build green".into(), done_check: "cargo test".into(), max_turns: Some(8) })
    );
    assert_eq!(slash("  /goal Write the migration guide "), Some(Slash::Set { objective: "Write the migration guide".into(), done_check: String::new(), max_turns: None }));
}

#[test]
fn a_slash_goal_message_sets_a_goal_and_its_ticket_without_a_model_turn() {
    let (controller, db, workspace, _repo) = workspace_with_repo();
    let (snapshot, started) = controller.send_chat("/goal Fix the flaky test --check cargo test --turns 6".into(), Some(workspace.clone()), false, None).unwrap();
    assert!(started.is_none(), "no model turn started");
    let goal = snapshot.goals.first().expect("the goal reaches the snapshot");
    assert_eq!((goal.objective.as_str(), goal.done_check.as_str(), goal.budget.max_turns), ("Fix the flaky test", "cargo test", 6));
    assert!(goal.conversation_id.as_deref().is_some_and(|c| c.starts_with("home:")), "attached to this chat");
    let ticket = snapshot.tasks.iter().find(|t| Some(&t.id) == goal.ticket_id.as_ref()).expect("its ticket exists");
    assert_eq!(ticket.status, TaskStatus::Queued);
    let reply = snapshot.conversation.last().unwrap();
    assert!(!reply.pending && reply.text().contains("Goal set") && reply.text().contains("cargo test"), "the chat says so: {}", reply.text());
    assert_eq!(reply.ticket_ids, vec![ticket.id.clone()]);
    // A bare /goal lists what is running and explains itself.
    let (snapshot, _) = controller.send_chat("/goal".into(), Some(workspace), false, None).unwrap();
    let text = snapshot.conversation.last().unwrap().text();
    assert!(text.contains("--check") && text.contains("Fix the flaky test"), "{text}");
    drop(db);
}

#[test]
fn a_neko_goal_block_rides_the_ticket_the_reply_opened() {
    let (_controller, db, workspace, repo) = workspace_with_repo();
    let turn = neko_chat::begin_turn(&db.lock().unwrap(), "Keep going until the tests pass").unwrap();
    complete_chat_reply(&db, &AtomicBool::new(false), &turn, "Keep going until the tests pass", None, neko_chat::Reply {
        used_memory: vec![], actions: vec![], text: "On it.".into(),
        tickets: vec![neko_chat::ProposedTicket { title: "Green tests".into(), goal: "Make the tests pass".into(), workspace_id: Some(workspace.clone()), folder: Some(repo.path().to_string_lossy().into()) }],
        memories: vec![], responsibilities: vec![],
        goals: vec![neko_chat::ProposedGoal { objective: "Keep going until the tests pass".into(), done_check: "cargo test".into(), constraints: "No new dependencies".into(), max_turns: None, task_id: None, workspace_id: Some(workspace) }],
    }).unwrap();
    let state = store::load(&db.lock().unwrap()).unwrap();
    assert_eq!((state.tasks.len(), state.goals.len()), (1, 1), "one ticket carries the goal");
    assert_eq!(state.goals[0].ticket_id.as_deref(), Some(state.tasks[0].id.as_str()));
    assert_eq!(state.goals[0].constraints, "No new dependencies");
    let reply = state.conversation.iter().find(|m| m.id == turn).unwrap();
    assert!(reply.text().contains("Goal set") && reply.text().contains("cargo test"), "{}", reply.text());
    assert_eq!(reply.ticket_ids.len(), 1);
}

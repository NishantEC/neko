//! The orchestration tools and the durable queue, without a model: leases with
//! crew roles, idempotent calls, each delivery mode, restart persistence, the
//! wait timeout and the delegation cap.
use super::*;
use neko_core::command_queue as queue;
use neko_protocol::crew::*;
use serde_json::{json, Value};

fn fixture() -> (Arc<Controller>, Arc<Mutex<Db>>, String, tempfile::TempDir) {
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
    (Arc::new(Controller::new(db.clone())), db, workspace, repo)
}

fn no_artifacts() -> impl Fn(&str, Value) -> Result<String, String> {
    |_, _| Err("no artifacts in this test".into())
}

fn call(c: &Controller, lease: &crate::mcp_host::Lease, tool: &str, args: Value) -> Result<Value, String> {
    c.crew_call(lease.token(), tool, &args.to_string(), &no_artifacts()).map(|text| serde_json::from_str(&text).unwrap())
}

fn set_status(db: &Arc<Mutex<Db>>, id: &str, status: TaskStatus) {
    let db = db.lock().unwrap();
    let mut state = store::load(&db).unwrap();
    state.tasks.iter_mut().find(|t| t.id == id).unwrap().status = status;
    store::save(&db, &state).unwrap();
}

fn status_of(db: &Arc<Mutex<Db>>, id: &str) -> TaskStatus {
    store::load(&db.lock().unwrap()).unwrap().tasks.iter().find(|t| t.id == id).unwrap().status
}

fn make_task(c: &Controller, lease: &crate::mcp_host::Lease, title: &str) -> String {
    call(c, lease, "create_task", json!({"title": title, "request": format!("Please do {title}")})).unwrap()["task_id"].as_str().unwrap().to_owned()
}

fn hold(c: &Controller, id: &str) -> Arc<AtomicBool> {
    let cancelled = Arc::new(AtomicBool::new(false));
    c.active.lock().unwrap().push(Active { task_id: id.into(), cancelled: cancelled.clone() });
    cancelled
}

fn release(c: &Controller, id: &str) {
    c.active.lock().unwrap().retain(|a| a.task_id != id);
}

#[test]
fn delivery_rules_by_mode_and_state() {
    use DeliveryMode::*;
    use TaskStatus::*;
    assert_eq!(plan(Building, true, AfterTurn, false, false), Plan::Wait);
    assert_eq!(plan(Building, true, SteerNow, false, false), Plan::Wait, "no live input: the turn boundary");
    assert_eq!(plan(Building, true, SteerNow, true, false), Plan::Inject);
    assert_eq!(plan(Building, true, InterruptRestart, false, false), Plan::Reply);
    assert_eq!(plan(ReadyForReview, false, AfterTurn, false, false), Plan::Reply);
    assert_eq!(plan(Queued, false, AfterTurn, false, false), Plan::Note);
    assert_eq!(plan(Queued, false, AfterTurn, false, true), Plan::Reply, "joins the reply already waiting");
    assert_eq!(plan(Building, false, AfterTurn, false, false), Plan::Wait, "between phases, recovery owns it");
    assert!(matches!(plan(Cancelled, false, AfterTurn, false, false), Plan::Drop(_)));
}

#[test]
fn a_lease_carries_only_what_its_role_allows() {
    let (c, db, workspace, _repo) = fixture();
    let full = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let read_only = c.mcp.crew_lease("chat:t2", CrewRole::ReadOnly).unwrap();
    let id = make_task(&c, &full, "first");
    let own = c.mcp.crew_lease("run-3", CrewRole::Task { ticket_id: id.clone(), may_create: true }).unwrap();
    let listed = |lease: &crate::mcp_host::Lease| -> Vec<String> {
        call(&c, lease, "tools", json!({})).unwrap()["tools"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_owned()).collect()
    };
    assert_eq!(listed(&full).len(), TOOLS.len());
    assert_eq!(listed(&read_only), ["list_tasks", "task_status"]);
    assert_eq!(listed(&own), ["list_tasks", "create_task", "wait_for", "task_status"]);

    // Read-only chat looks, and cannot do anything else.
    assert_eq!(call(&c, &read_only, "list_tasks", json!({})).unwrap()["tasks"].as_array().unwrap().len(), 1);
    assert!(call(&c, &read_only, "task_status", json!({"task_id": id})).is_ok());
    for tool in ["create_task", "steer_task", "cancel_task", "wait_for", "schedule", "save_artifact"] {
        let error = call(&c, &read_only, tool, json!({"task_id": id, "title": "x", "request": "y", "message": "z"})).unwrap_err();
        assert!(error.contains("not available to this run"), "{tool}: {error}");
    }
    // A ticket's own run cannot steer or cancel, and its sub-tasks stay in its workspace and its own origin.
    for tool in ["steer_task", "cancel_task", "schedule"] {
        assert!(call(&c, &own, tool, json!({"task_id": id, "message": "m", "name": "n", "prompt": "p"})).unwrap_err().contains("not available"), "{tool}");
    }
    let sub = call(&c, &own, "create_task", json!({"title": "sub", "request": "a part of it"})).unwrap();
    assert_eq!(sub["workspace_id"], workspace);
    let sub_id = sub["task_id"].as_str().unwrap();
    assert_eq!(neko_core::crew::origin_of(&db.lock().unwrap(), sub_id).unwrap().as_deref(), Some(format!("ticket:{id}").as_str()));
    assert!(!store::load(&db.lock().unwrap()).unwrap().start_when_planned.contains(sub_id), "a sub-task waits for the person's approval to build");
    assert!(call(&c, &own, "create_task", json!({"title": "elsewhere", "request": "x", "workspace_id": "another"})).is_err());
    // A read-only ticket (may_create false) cannot open work.
    let looking = c.mcp.crew_lease("run-4", CrewRole::Task { ticket_id: id.clone(), may_create: false }).unwrap();
    assert!(call(&c, &looking, "create_task", json!({"title": "x", "request": "y"})).is_err());
    assert!(call(&c, &looking, "wait_for", json!({"task_id": id, "timeout_seconds": 0})).is_ok());
    // A dropped lease is no authority.
    let token = full.token().to_owned();
    drop(full);
    assert!(c.crew_call(&token, "list_tasks", "{}", &no_artifacts()).is_err());
}

#[test]
fn a_run_without_a_crew_role_has_no_crew_tools() {
    let (c, _db, _workspace, _repo) = fixture();
    let lease = c.mcp.lease("watch:x", "none", vec![], 0);
    // (A connection-only lease is refused for an unknown workspace; the point is that no role is ever invented.)
    assert!(lease.is_err() || c.crew_call(lease.as_ref().unwrap().token(), "list_tasks", "{}", &no_artifacts()).is_err());
    assert!(c.crew_call("not-a-token", "list_tasks", "{}", &no_artifacts()).is_err());
}

#[test]
fn a_retried_create_task_opens_one_task() {
    let (c, db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let args = json!({"title": "Fix login", "request": "The login page hangs"});
    let first = call(&c, &lease, "create_task", args.clone()).unwrap();
    let again = call(&c, &lease, "create_task", args).unwrap();
    assert_eq!(first["task_id"], again["task_id"]);
    assert_eq!(again["duplicate"], true);
    assert_eq!(store::load(&db.lock().unwrap()).unwrap().tasks.len(), 1);
    // Audited on the task, with the person's words kept as its request.
    let state = store::load(&db.lock().unwrap()).unwrap();
    assert!(state.tasks[0].events.iter().any(|e| e.role == "crew" && e.message.starts_with("Opened by chat")));
    assert_eq!(state.tasks[0].request.as_deref(), Some("The login page hangs"));
    assert!(state.start_when_planned.contains(&state.tasks[0].id), "the chat's own request builds without a plan approval");
    // A different key is a different task.
    call(&c, &lease, "create_task", json!({"title": "Fix login", "request": "The login page hangs", "idempotency_key": "second"})).unwrap();
    assert_eq!(store::load(&db.lock().unwrap()).unwrap().tasks.len(), 2);
}

#[test]
fn one_chat_may_have_eight_open_delegated_tasks_unless_raised() {
    let (c, db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let mut ids = Vec::new();
    for n in 0..8 {
        ids.push(make_task(&c, &lease, &format!("task {n}")));
    }
    let error = call(&c, &lease, "create_task", json!({"title": "ninth", "request": "one too many"})).unwrap_err();
    assert!(error.contains("8 delegated tasks are already open"), "{error}");
    assert_eq!(call(&c, &lease, "list_tasks", json!({"mine": true})).unwrap()["open_delegated"], 8);
    // Finishing one frees a slot; raising the limit frees more.
    set_status(&db, &ids[0], TaskStatus::Completed);
    make_task(&c, &lease, "ninth");
    assert!(call(&c, &lease, "create_task", json!({"title": "tenth", "request": "no room"})).is_err());
    c.command(Command::Crew(CrewCommand::SetDelegationCap { limit: 12 })).unwrap();
    make_task(&c, &lease, "tenth");
    assert!(c.command(Command::Crew(CrewCommand::SetDelegationCap { limit: 0 })).is_err());
    // A ticket's sub-tasks are counted on their own.
    let own = c.mcp.crew_lease("run-9", CrewRole::Task { ticket_id: ids[1].clone(), may_create: true }).unwrap();
    for n in 0..12 {
        call(&c, &own, "create_task", json!({"title": format!("part {n}"), "request": "part"})).unwrap();
    }
    assert!(call(&c, &own, "create_task", json!({"title": "part 13", "request": "part"})).is_err());
    // A burst only queues: nothing started by being created.
    assert!(store::load(&db.lock().unwrap()).unwrap().tasks.iter().all(|t| t.status == TaskStatus::Queued || t.status == TaskStatus::Completed));
}

#[test]
fn after_turn_waits_for_the_turn_then_arrives_as_a_reply() {
    let (c, db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let id = make_task(&c, &lease, "build");
    set_status(&db, &id, TaskStatus::Building);
    let worker = hold(&c, &id);
    let sent = call(&c, &lease, "steer_task", json!({"task_id": id, "message": "Use the compact layout"})).unwrap();
    assert_eq!(sent["state"], "queued");
    assert_eq!(sent["mode"], "after_turn");
    assert_eq!(c.deliver_queued().unwrap(), 0);
    assert!(!worker.load(Ordering::Acquire), "after_turn never interrupts");
    assert_eq!(queue::queued(&db.lock().unwrap()).unwrap().len(), 1);
    // The turn ends.
    release(&c, &id);
    set_status(&db, &id, TaskStatus::ReadyForReview);
    assert_eq!(c.deliver_queued().unwrap(), 1);
    let state = store::load(&db.lock().unwrap()).unwrap();
    assert_eq!(state.tasks.iter().find(|t| t.id == id).unwrap().status, TaskStatus::Queued, "the reply queued the next turn");
    assert_eq!(state.task_replies[&id].pending, ["Use the compact layout"]);
    assert_eq!(state.queued_messages[0].state, QueueState::Delivered);
    // Audited.
    assert!(state.tasks.iter().find(|t| t.id == id).unwrap().events.iter().any(|e| e.role == "crew" && e.message.contains("queued a message (after the turn)")));
}

#[test]
fn steer_now_goes_live_when_the_runtime_can_and_otherwise_waits_for_the_boundary() {
    let (c, db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let id = make_task(&c, &lease, "build");
    set_status(&db, &id, TaskStatus::Building);
    let worker = hold(&c, &id);
    let sent = call(&c, &lease, "steer_task", json!({"task_id": id, "message": "Skip the migration", "mode": "steer_now"})).unwrap();
    assert_eq!(sent["state"], "queued");
    assert!(sent["detail"].is_string());
    let waiting = queue::queued(&db.lock().unwrap()).unwrap();
    assert!(waiting[0].detail.contains("cannot take input mid-turn"), "the person is told why it is not instant");
    assert!(!worker.load(Ordering::Acquire), "steer_now never stops the turn");
    // A runtime that takes input mid-turn gets it at once.
    let injected = Mutex::new(Vec::new());
    assert_eq!(c.deliver_queued_with(&|task, text| { injected.lock().unwrap().push((task.to_owned(), text.to_owned())); true }).unwrap(), 1);
    assert_eq!(injected.into_inner().unwrap(), [(id.clone(), "Skip the migration".to_owned())]);
    let done = &store::load(&db.lock().unwrap()).unwrap().queued_messages[0];
    assert_eq!((done.state, done.detail.as_str()), (QueueState::Delivered, "Delivered into the live session."));
    assert!(!worker.load(Ordering::Acquire));
    assert_eq!(status_of(&db, &id), TaskStatus::Building, "nothing restarted");
}

#[test]
fn interrupt_restart_stops_the_turn_and_restarts_it_with_the_message() {
    let (c, db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let id = make_task(&c, &lease, "build");
    set_status(&db, &id, TaskStatus::Building);
    let worker = hold(&c, &id);
    let sent = call(&c, &lease, "steer_task", json!({"task_id": id, "message": "Stop. Do the API first.", "mode": "interrupt_restart"})).unwrap();
    assert_eq!(sent["state"], "delivered");
    assert!(worker.load(Ordering::Acquire), "the running turn was stopped");
    let state = store::load(&db.lock().unwrap()).unwrap();
    assert_eq!(state.tasks.iter().find(|t| t.id == id).unwrap().status, TaskStatus::Queued);
    assert_eq!(state.task_replies[&id].pending, ["Stop. Do the API first."]);
}

#[test]
fn a_task_that_has_not_started_takes_the_message_as_a_note_and_a_cancelled_one_drops_it() {
    let (c, db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let id = make_task(&c, &lease, "later");
    let sent = call(&c, &lease, "steer_task", json!({"task_id": id, "message": "Also add a test"})).unwrap();
    assert_eq!(sent["state"], "delivered");
    assert!(store::load(&db.lock().unwrap()).unwrap().tasks[0].events.iter().any(|e| e.message.contains("Also add a test")));
    // Cancelled before it was delivered: dropped, with a reason.
    let other = make_task(&c, &lease, "other");
    set_status(&db, &other, TaskStatus::Building);
    let _worker = hold(&c, &other);
    call(&c, &lease, "steer_task", json!({"task_id": other, "message": "wait for me"})).unwrap();
    release(&c, &other);
    call(&c, &lease, "cancel_task", json!({"task_id": other, "reason": "not needed"})).unwrap();
    c.deliver_queued().unwrap();
    let state = store::load(&db.lock().unwrap()).unwrap();
    let dropped = state.queued_messages.iter().find(|m| m.task_id == other).unwrap();
    assert_eq!(dropped.state, QueueState::Dropped);
    assert!(state.tasks.iter().find(|t| t.id == other).unwrap().events.iter().any(|e| e.role == "crew" && e.message.contains("cancelled the task: not needed")));
}

#[test]
fn a_retried_message_is_one_entry_and_is_never_delivered_twice() {
    let (c, db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let id = make_task(&c, &lease, "build");
    set_status(&db, &id, TaskStatus::Building);
    let _worker = hold(&c, &id);
    let enqueue = || c.command(Command::Crew(CrewCommand::Enqueue { task_id: id.clone(), command_id: Some("composer-1".into()), text: "Please hurry".into(), mode: DeliveryMode::AfterTurn, kind: QueueKind::Send })).unwrap();
    enqueue();
    enqueue();
    assert_eq!(store::load(&db.lock().unwrap()).unwrap().queued_messages.len(), 1);
    release(&c, &id);
    set_status(&db, &id, TaskStatus::ReadyForReview);
    c.deliver_queued().unwrap();
    enqueue(); // the web retried after it was delivered
    c.deliver_queued().unwrap();
    let state = store::load(&db.lock().unwrap()).unwrap();
    assert_eq!(state.task_replies[&id].pending, ["Please hurry"], "delivered once");
    assert_eq!(state.queued_messages.len(), 1);
    // The same tool call retried is one message too.
    set_status(&db, &id, TaskStatus::Building);
    let _worker = hold(&c, &id);
    let first = call(&c, &lease, "steer_task", json!({"task_id": id, "message": "and add docs"})).unwrap();
    let second = call(&c, &lease, "steer_task", json!({"task_id": id, "message": "and add docs"})).unwrap();
    assert_eq!(first["queue_id"], second["queue_id"]);
    assert_eq!(store::load(&db.lock().unwrap()).unwrap().queued_messages.iter().filter(|m| m.text == "and add docs").count(), 1);
}

#[test]
fn a_person_can_take_back_a_queued_message_once() {
    let (c, db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let id = make_task(&c, &lease, "build");
    set_status(&db, &id, TaskStatus::Building);
    let _worker = hold(&c, &id);
    c.command(Command::Crew(CrewCommand::Enqueue { task_id: id.clone(), command_id: Some("m1".into()), text: "hold on".into(), mode: DeliveryMode::AfterTurn, kind: QueueKind::Send })).unwrap();
    let after = c.command(Command::Crew(CrewCommand::CancelQueued { task_id: id.clone(), id: "m1".into() })).unwrap();
    assert_eq!(after.queued_messages[0].state, QueueState::Cancelled);
    assert!(c.command(Command::Crew(CrewCommand::CancelQueued { task_id: id.clone(), id: "m1".into() })).is_err());
    release(&c, &id);
    set_status(&db, &id, TaskStatus::ReadyForReview);
    assert_eq!(c.deliver_queued().unwrap(), 0, "a cancelled message is never delivered");
    assert!(c.command(Command::Crew(CrewCommand::Enqueue { task_id: "missing".into(), command_id: None, text: "x".into(), mode: DeliveryMode::AfterTurn, kind: QueueKind::Send })).is_err());
}

#[test]
fn queued_messages_survive_a_daemon_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("neko.db");
    let repo = tempfile::tempdir().unwrap();
    assert!(std::process::Command::new("git").args(["init", "--quiet"]).arg(repo.path()).status().unwrap().success());
    let (id, workspace);
    {
        let db = Arc::new(Mutex::new(Db::open(&path).unwrap()));
        workspace = store::apply(&db.lock().unwrap(), Command::SaveWorkspaceWithFolders {
            workspace: Workspace { id: String::new(), name: "Work".into(), repository: repo.path().to_string_lossy().into(), instructions: String::new(), away_enabled: false },
            folders: vec![repo.path().to_string_lossy().into()],
        }).unwrap().workspaces[0].id.clone();
        let c = Controller::new(db.clone());
        let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
        id = call(&c, &lease, "create_task", json!({"title": "build", "request": "x", "workspace_id": workspace})).unwrap()["task_id"].as_str().unwrap().to_owned();
        set_status(&db, &id, TaskStatus::Building);
        let _worker = hold(&c, &id);
        call(&c, &lease, "steer_task", json!({"task_id": id, "message": "survive me", "mode": "steer_now"})).unwrap();
        c.command(Command::Crew(CrewCommand::Enqueue { task_id: id.clone(), command_id: Some("w1".into()), text: "from the web".into(), mode: DeliveryMode::AfterTurn, kind: QueueKind::Send })).unwrap();
    }
    // The daemon restarted: a new Db, a new controller, no workers.
    let db = Arc::new(Mutex::new(Db::open(&path).unwrap()));
    let c = Controller::new(db.clone());
    let waiting = queue::queued(&db.lock().unwrap()).unwrap();
    assert_eq!(waiting.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["survive me", "from the web"]);
    assert_eq!(waiting[0].mode, DeliveryMode::SteerNow);
    set_status(&db, &id, TaskStatus::ReadyForReview);
    assert_eq!(c.deliver_queued().unwrap(), 2);
    let state = store::load(&db.lock().unwrap()).unwrap();
    assert_eq!(state.task_replies[&id].pending, ["survive me", "from the web"], "in the order they were sent");
    assert!(queue::queued(&db.lock().unwrap()).unwrap().is_empty());
}

#[test]
fn wait_for_returns_when_met_and_times_out_without_failing() {
    let (c, db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let id = make_task(&c, &lease, "build");
    set_status(&db, &id, TaskStatus::Building);
    let started = Instant::now();
    let waited = call(&c, &lease, "wait_for", json!({"task_id": id, "timeout_seconds": 0})).unwrap();
    assert_eq!((waited["met"].clone(), waited["timed_out"].clone(), waited["state"].clone()), (json!(false), json!(true), json!("Building")));
    // A short wait really waits, then reports the state: a timeout is not an error.
    let waited = call(&c, &lease, "wait_for", json!({"task_id": id, "timeout_seconds": 1.2})).unwrap();
    assert_eq!(waited["timed_out"], true);
    assert!(started.elapsed() >= Duration::from_millis(1100), "it waited");
    // Met the moment the state is reached, with the status to read.
    let finisher = {
        let db = db.clone();
        let id = id.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            set_status(&db, &id, TaskStatus::ReadyForReview);
        })
    };
    let met = call(&c, &lease, "wait_for", json!({"task_id": id, "timeout_seconds": 20})).unwrap();
    finisher.join().unwrap();
    assert_eq!(met["met"], true);
    assert_eq!(met["status"]["status"], "ReadyForReview");
    // A named state; and the cap on the wait is 30 minutes (a larger ask still returns at once when met).
    assert_eq!(call(&c, &lease, "wait_for", json!({"task_id": id, "until": "ready_for_review", "timeout_seconds": 999999})).unwrap()["met"], true);
    assert_eq!(call(&c, &lease, "wait_for", json!({"task_id": id, "until": "finished", "timeout_seconds": 0})).unwrap()["met"], false);
    assert!(call(&c, &lease, "wait_for", json!({"timeout_seconds": 1})).is_err(), "one of task_id or goal_id");
    // A run that ends stops its waits.
    let token = lease.token().to_owned();
    let ended = std::thread::spawn({
        let c = c.clone();
        let id = id.clone();
        move || c.crew_call(&token, "wait_for", &json!({"task_id": id, "until": "completed", "timeout_seconds": 600}).to_string(), &no_artifacts())
    });
    std::thread::sleep(Duration::from_millis(300));
    drop(lease);
    let result: Value = serde_json::from_str(&ended.join().unwrap().unwrap()).unwrap();
    assert_eq!(result["run_ended"], true);
}

#[test]
fn task_status_reports_state_summary_goal_and_evidence() {
    let (c, db, workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let made = call(&c, &lease, "create_task", json!({"title": "Make it green", "request": "Make the build green", "goal": {"done_check": "cargo test", "max_turns": 5}})).unwrap();
    let id = made["task_id"].as_str().unwrap().to_owned();
    assert!(made["goal_id"].is_string());
    {
        let db = db.lock().unwrap();
        let mut state = store::load(&db).unwrap();
        state.tasks[0].result = "Fixed the parser; two tests still fail.".into();
        store::save(&db, &state).unwrap();
        neko_core::goals::push_ledger(&db, &id, neko_protocol::goals::CheckResult { command: "cargo test".into(), exit_code: Some(101), tail: "2 failed".into(), at_ms: 1, source: "neko".into() }).unwrap();
    }
    let status = call(&c, &lease, "task_status", json!({"task_id": id})).unwrap();
    assert_eq!(status["last_turn_summary"], "Fixed the parser; two tests still fail.");
    assert_eq!(status["goal"]["status"], "Paused", "an agent-proposed goal waits for the person");
    assert_eq!(status["goal"]["evidence"][0]["passed"], false);
    assert_eq!(status["workspace_id"], workspace);
    assert!(call(&c, &lease, "task_status", json!({"task_id": "nope"})).is_err());
}

#[test]
fn schedule_makes_a_one_off_and_refuses_the_past() {
    let (c, _db, _workspace, _repo) = fixture();
    let lease = c.mcp.crew_lease("chat:t1", CrewRole::Full).unwrap();
    let made = call(&c, &lease, "schedule", json!({"name": "Check the deploy", "prompt": "Look at the deploy and report", "in_minutes": 30})).unwrap();
    assert!(made["schedule_id"].is_string());
    assert!(made["next_due_ms"].as_i64().unwrap() > store::now_ms());
    assert!(call(&c, &lease, "schedule", json!({"name": "Past", "prompt": "x", "at_unix_ms": 1000})).is_err());
    assert!(call(&c, &lease, "schedule", json!({"name": "None", "prompt": "x"})).is_err());
}

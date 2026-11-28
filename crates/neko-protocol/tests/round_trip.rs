//! Every message survives JSON both ways, and its JSON shape is pinned: a change here is a change to the contract's version.

use neko_protocol::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

fn task() -> Task {
    Task {
        id: TaskId("task_01HZX3J0000000000000000000".into()),
        workspace: "default".into(),
        words: "Rename the sidebar label\nand check ENG-123".into(),
        title: "Rename the sidebar label".into(),
        created_ms: 1_764_000_000_000,
        status: TaskStatus::New,
        folder: None,
        cat: Some("cat_miso".into()),
        in_folder: false,
        from_branch: None,
        round: None,
        last_read_ms: 1_764_000_000_000,
        activity_ms: 1_764_000_000_000,
        latest_line: String::new(),
        failure_line: String::new(),
        thread_status: ThreadStatus::Idle,
        needs_you: false,
        subagent: None,
        pinned: false,
        settled: false,
        archived: false,
        cattain: false,
        started_by: None,
    }
}

/// `value` encodes to exactly `json` and decodes back to itself.
fn pinned<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: T, json: &str) {
    let encoded = serde_json::to_string(&value).expect("encodes");
    assert_eq!(encoded, json, "the JSON shape of {value:?}");
    let back: T = serde_json::from_str(&encoded).expect("decodes");
    assert_eq!(back, value);
}

#[test]
fn requests_round_trip_with_their_ids() {
    pinned(ClientMessage { id: 0, request: Request::Hello { client: "neko-app".into(), protocol: 1 } }, r#"{"id":0,"type":"hello","client":"neko-app","protocol":1}"#);
    pinned(ClientMessage { id: 1, request: Request::ListTasks }, r#"{"id":1,"type":"list_tasks"}"#);
    pinned(ClientMessage { id: 2, request: Request::CreateTask { crew: None, words: "Fix the build".into(), workspace: None, in_folder: false, from_branch: None } }, r#"{"id":2,"type":"create_task","words":"Fix the build"}"#);
    pinned(
        ClientMessage { id: 2, request: Request::CreateTask { crew: None, words: "Fix the build".into(), workspace: Some("ws_1".into()), in_folder: false, from_branch: None } },
        r#"{"id":2,"type":"create_task","words":"Fix the build","workspace":"ws_1"}"#,
    );
    pinned(ClientMessage { id: 3, request: Request::GetTask { task: TaskId("task_1".into()) } }, r#"{"id":3,"type":"get_task","task":"task_1"}"#);
    pinned(ClientMessage { id: 4, request: Request::Subscribe }, r#"{"id":4,"type":"subscribe"}"#);
}

#[test]
fn a_thread_says_what_it_is_doing_and_can_be_marked_read() {
    pinned(ClientMessage { id: 6, request: Request::MarkRead { task: TaskId("task_1".into()) } }, r#"{"id":6,"type":"mark_read","task":"task_1"}"#);
    pinned(ClientMessage { id: 7, request: Request::MarkUnread { task: TaskId("task_1".into()) } }, r#"{"id":7,"type":"mark_unread","task":"task_1"}"#);
    let mut t = task();
    t.status = TaskStatus::Failed;
    t.thread_status = ThreadStatus::Failed;
    t.needs_you = true;
    t.latest_line = "Renamed the label.".into();
    t.failure_line = "Claude Code is not signed in.".into();
    t.activity_ms += 5;
    assert!(t.unread());
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains(r#""latest_line":"Renamed the label.","failure_line":"Claude Code is not signed in.","thread_status":"failed","needs_you":true"#), "{json}");
    let back: Task = serde_json::from_str(&json).unwrap();
    assert_eq!(back, t);
    // A task from a daemon that did not know them still reads: everything is as if the thread had been read.
    let old = r#"{"id":"task_1","workspace":"default","words":"w","title":"t","created_ms":1,"status":"new","cat":"c"}"#;
    let old: Task = serde_json::from_str(old).unwrap();
    assert_eq!((old.thread_status, old.needs_you, old.unread()), (ThreadStatus::Idle, false, false));
}

#[test]
fn replies_and_events_round_trip() {
    pinned(
        ServerMessage::Reply { id: 0, reply: Reply::Hello { daemon: "0.1.0".into(), protocol: 1, workspace: "default".into() } },
        r#"{"kind":"reply","id":0,"reply":{"type":"hello","daemon":"0.1.0","protocol":1,"workspace":"default"}}"#,
    );
    let t = r#"{"id":"task_01HZX3J0000000000000000000","workspace":"default","words":"Rename the sidebar label\nand check ENG-123","title":"Rename the sidebar label","created_ms":1764000000000,"status":"new","cat":"cat_miso","last_read_ms":1764000000000,"activity_ms":1764000000000,"thread_status":"idle"}"#;
    pinned(ServerMessage::Reply { id: 2, reply: Reply::Task { task: task() } }, &format!(r#"{{"kind":"reply","id":2,"reply":{{"type":"task","task":{t}}}}}"#));
    pinned(ServerMessage::Reply { id: 1, reply: Reply::Tasks { tasks: vec![task()] } }, &format!(r#"{{"kind":"reply","id":1,"reply":{{"type":"tasks","tasks":[{t}]}}}}"#));
    pinned(ServerMessage::Reply { id: 4, reply: Reply::Subscribed { revision: 7 } }, r#"{"kind":"reply","id":4,"reply":{"type":"subscribed","revision":7}}"#);
    pinned(
        ServerMessage::Reply { id: 5, reply: Reply::error(ErrorCode::InvalidWords, "Type a task first.") },
        r#"{"kind":"reply","id":5,"reply":{"type":"error","code":"invalid_words","message":"Type a task first."}}"#,
    );
    pinned(ServerMessage::Event { revision: 8, change: Change::TaskCreated { task: task() } }, &format!(r#"{{"kind":"event","revision":8,"change":{{"type":"task_created","task":{t}}}}}"#));
}

#[test]
fn every_error_code_has_a_stable_name() {
    for (code, name) in [
        (ErrorCode::ProtocolMismatch, "protocol_mismatch"),
        (ErrorCode::HelloFirst, "hello_first"),
        (ErrorCode::BadRequest, "bad_request"),
        (ErrorCode::InvalidWords, "invalid_words"),
        (ErrorCode::NotFound, "not_found"),
        (ErrorCode::AlreadyAnswered, "already_answered"),
        (ErrorCode::Busy, "busy"),
        (ErrorCode::Internal, "internal"),
        (ErrorCode::InvalidFolder, "invalid_folder"),
        (ErrorCode::Refused, "refused"),
        (ErrorCode::UnsavedWork, "unsaved_work"),
        (ErrorCode::InvalidName, "invalid_name"),
    ] {
        pinned(code, &format!("\"{name}\""));
    }
}

fn workspace() -> Workspace {
    Workspace { id: "ws_1".into(), name: "acme-api".into(), folder: Some("/Users/example/acme-api".into()), created_ms: 1_764_000_000_000, removed_ms: None, position: 1, missing: false }
}

#[test]
fn workspace_requests_round_trip() {
    pinned(ClientMessage { id: 1, request: Request::ListWorkspaces }, r#"{"id":1,"type":"list_workspaces"}"#);
    pinned(ClientMessage { id: 2, request: Request::AddWorkspace { folder: "/Users/example/acme-api".into() } }, r#"{"id":2,"type":"add_workspace","folder":"/Users/example/acme-api"}"#);
    pinned(
        ClientMessage { id: 3, request: Request::RenameWorkspace { workspace: "ws_1".into(), name: "Acme API".into() } },
        r#"{"id":3,"type":"rename_workspace","workspace":"ws_1","name":"Acme API"}"#,
    );
    pinned(ClientMessage { id: 4, request: Request::RemoveWorkspace { workspace: "ws_1".into() } }, r#"{"id":4,"type":"remove_workspace","workspace":"ws_1"}"#);
    pinned(
        ClientMessage { id: 5, request: Request::LocateWorkspace { workspace: "ws_1".into(), folder: "/Users/example/code/acme-api".into() } },
        r#"{"id":5,"type":"locate_workspace","workspace":"ws_1","folder":"/Users/example/code/acme-api"}"#,
    );
    pinned(ClientMessage { id: 6, request: Request::MoveWorkspace { workspace: "ws_1".into(), to: 0 } }, r#"{"id":6,"type":"move_workspace","workspace":"ws_1","to":0}"#);
    pinned(ClientMessage { id: 7, request: Request::RemoveTask { task: TaskId("task_1".into()) } }, r#"{"id":7,"type":"remove_task","task":"task_1"}"#);
}

#[test]
fn workspace_replies_and_events_round_trip() {
    let w = r#"{"id":"ws_1","name":"acme-api","folder":"/Users/example/acme-api","created_ms":1764000000000,"position":1}"#;
    pinned(ServerMessage::Reply { id: 1, reply: Reply::Workspaces { workspaces: vec![workspace()] } }, &format!(r#"{{"kind":"reply","id":1,"reply":{{"type":"workspaces","workspaces":[{w}]}}}}"#));
    pinned(ServerMessage::Reply { id: 2, reply: Reply::Workspace { workspace: workspace(), existed: false } }, &format!(r#"{{"kind":"reply","id":2,"reply":{{"type":"workspace","workspace":{w}}}}}"#));
    pinned(
        ServerMessage::Reply { id: 2, reply: Reply::Workspace { workspace: workspace(), existed: true } },
        &format!(r#"{{"kind":"reply","id":2,"reply":{{"type":"workspace","workspace":{w},"existed":true}}}}"#),
    );
    pinned(ServerMessage::Event { revision: 9, change: Change::WorkspaceAdded { workspace: workspace() } }, &format!(r#"{{"kind":"event","revision":9,"change":{{"type":"workspace_added","workspace":{w}}}}}"#));
    let gone = Workspace { removed_ms: Some(1_764_000_100_000), missing: true, ..workspace() };
    pinned(
        ServerMessage::Event { revision: 10, change: Change::WorkspaceUpdated { workspace: gone } },
        r#"{"kind":"event","revision":10,"change":{"type":"workspace_updated","workspace":{"id":"ws_1","name":"acme-api","folder":"/Users/example/acme-api","created_ms":1764000000000,"removed_ms":1764000100000,"position":1,"missing":true}}}"#,
    );
    let general = Workspace { id: GENERAL.into(), name: "General".into(), folder: None, created_ms: 1, removed_ms: None, position: 0, missing: false };
    pinned(general, r#"{"id":"default","name":"General","created_ms":1,"position":0}"#);
    pinned(ServerMessage::Event { revision: 11, change: Change::TaskRemoved { task: TaskId("task_1".into()) } }, r#"{"kind":"event","revision":11,"change":{"type":"task_removed","task":"task_1"}}"#);
    let mut t = task();
    t.workspace = "ws_1".into();
    t.folder = Some("/Users/example/acme-api".into());
    pinned(
        t,
        r#"{"id":"task_01HZX3J0000000000000000000","workspace":"ws_1","words":"Rename the sidebar label\nand check ENG-123","title":"Rename the sidebar label","created_ms":1764000000000,"status":"new","folder":"/Users/example/acme-api","cat":"cat_miso","last_read_ms":1764000000000,"activity_ms":1764000000000,"thread_status":"idle"}"#,
    );
}

#[test]
fn the_daemon_file_round_trips() {
    pinned(DaemonInfo { pid: 42, port: 50123, protocol: 1, version: "0.1.0".into() }, r#"{"pid":42,"port":50123,"protocol":1,"version":"0.1.0"}"#);
}

#[test]
fn an_unknown_request_is_refused_not_guessed() {
    assert!(serde_json::from_str::<ClientMessage>(r#"{"id":1,"type":"delete_everything"}"#).is_err());
    assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"list_tasks"}"#).is_err(), "a request without an id has nowhere to be answered");
}

#[test]
fn the_requests_that_run_a_task_round_trip() {
    let t = || TaskId("task_1".into());
    pinned(ClientMessage { id: 5, request: Request::GetTranscript { task: t() } }, r#"{"id":5,"type":"get_transcript","task":"task_1"}"#);
    pinned(ClientMessage { id: 6, request: Request::FollowUp { task: t(), words: "And the tests".into(), answering: None, mode: None } }, r#"{"id":6,"type":"follow_up","task":"task_1","words":"And the tests"}"#);
    pinned(ClientMessage { id: 7, request: Request::StopTask { task: t() } }, r#"{"id":7,"type":"stop_task","task":"task_1"}"#);
    pinned(
        ClientMessage { id: 8, request: Request::AnswerPermission { permission: PermissionId("perm_1".into()), decision: Decision::AlwaysAllow } },
        r#"{"id":8,"type":"answer_permission","permission":"perm_1","decision":"always_allow"}"#,
    );
    pinned(ClientMessage { id: 9, request: Request::WatchTask { task: Some(t()) } }, r#"{"id":9,"type":"watch_task","task":"task_1"}"#);
    pinned(ClientMessage { id: 9, request: Request::WatchTask { task: None } }, r#"{"id":9,"type":"watch_task"}"#);
    pinned(ClientMessage { id: 10, request: Request::ListRuntimes }, r#"{"id":10,"type":"list_runtimes"}"#);
    pinned(
        ClientMessage { id: 11, request: Request::SetSettings { settings: Settings { runtime: "claude".into(), permission_mode: PermissionMode::Full, own_mcp: false, queue_first: false, ask_first_subagent: false, cattains_by_hand: false } } },
        r#"{"id":11,"type":"set_settings","settings":{"runtime":"claude","permission_mode":"full"}}"#,
    );
    for (m, name) in [(PermissionMode::Follow, "follow"), (PermissionMode::Ask, "ask"), (PermissionMode::Full, "full")] {
        pinned(m, &format!("\"{name}\""));
    }
    assert_eq!(PermissionMode::default(), PermissionMode::Follow, "a new install follows Claude Code's own rules");
    pinned(ClientMessage { id: 12, request: Request::DayUsage { from_ms: 5 } }, r#"{"id":12,"type":"day_usage","from_ms":5}"#);
    for (d, name) in [(Decision::AllowOnce, "allow_once"), (Decision::AlwaysAllow, "always_allow"), (Decision::Deny, "deny")] {
        pinned(d, &format!("\"{name}\""));
    }
}

fn run() -> Run {
    Run {
        id: RunId("run_1".into()),
        task: TaskId("task_1".into()),
        runtime: "claude".into(),
        model: Some("claude-x".into()),
        status: RunStatus::Done,
        started_ms: 10,
        ended_ms: Some(20),
        session: Some("5f0c".into()),
        workdir: "/Users/example/acme-api".into(),
        branch: Some("neko/abcd1234".into()),
        reason: None,
        usage: Some(Usage { input_tokens: 12, output_tokens: 34, cached_tokens: 56, cost_usd: Some(0.25), model: Some("claude-x".into()), pricing: Pricing::ApiEquivalent, reported_by: "Claude Code".into(), context_tokens: Some(48_000), context_window: Some(200_000) }),
        changes: Some(Changes { files: 2, added: 10, removed: 3 }),
        undo: None,
        cat: None,
        rules_hash: None,
        ask: None,
    }
}

#[test]
fn runs_parts_and_permissions_round_trip() {
    pinned(
        run(),
        r#"{"id":"run_1","task":"task_1","runtime":"claude","model":"claude-x","status":"done","started_ms":10,"ended_ms":20,"session":"5f0c","workdir":"/Users/example/acme-api","branch":"neko/abcd1234","usage":{"input_tokens":12,"output_tokens":34,"cached_tokens":56,"cost_usd":0.25,"model":"claude-x","pricing":"api_equivalent","reported_by":"Claude Code","context_tokens":48000,"context_window":200000},"changes":{"files":2,"added":10,"removed":3}}"#,
    );
    let part = |seq, body| Part { id: PartId(format!("part_{seq}")), task: TaskId("task_1".into()), run: Some(RunId("run_1".into())), seq, at_ms: 1, ended_ms: None, body };
    pinned(part(1, PartBody::Person { text: "Hi".into() }), r#"{"id":"part_1","task":"task_1","run":"run_1","seq":1,"at_ms":1,"body":{"kind":"person","text":"Hi"}}"#);
    pinned(part(2, PartBody::Agent { text: "**Done**".into(), done: true }), r#"{"id":"part_2","task":"task_1","run":"run_1","seq":2,"at_ms":1,"body":{"kind":"agent","text":"**Done**","done":true}}"#);
    pinned(part(3, PartBody::Thinking { text: "Hm".into(), done: false }), r#"{"id":"part_3","task":"task_1","run":"run_1","seq":3,"at_ms":1,"body":{"kind":"thinking","text":"Hm","done":false}}"#);
    pinned(
        part(4, PartBody::Tool { call: "t1".into(), name: "Read".into(), summary: "src/main.rs".into(), input: "{}".into(), output: Some("fn main".into()), state: ToolState::Done, exit: None }),
        r#"{"id":"part_4","task":"task_1","run":"run_1","seq":4,"at_ms":1,"body":{"kind":"tool","call":"t1","name":"Read","summary":"src/main.rs","input":"{}","output":"fn main","state":"done"}}"#,
    );
    pinned(
        part(7, PartBody::Tool { call: "t2".into(), name: "Bash".into(), summary: "test -f a".into(), input: "test -f a".into(), output: Some("Exit code 1".into()), state: ToolState::Done, exit: Some(1) }),
        r#"{"id":"part_7","task":"task_1","run":"run_1","seq":7,"at_ms":1,"body":{"kind":"tool","call":"t2","name":"Bash","summary":"test -f a","input":"test -f a","output":"Exit code 1","state":"done","exit":1}}"#,
    );
    pinned(part(5, PartBody::Permission { permission: PermissionId("perm_1".into()) }), r#"{"id":"part_5","task":"task_1","run":"run_1","seq":5,"at_ms":1,"body":{"kind":"permission","permission":"perm_1"}}"#);
    pinned(
        part(6, PartBody::Notice { tone: Tone::Error, text: "Claude Code is not signed in.".into(), next: Some("Run claude".into()), failure: Some(FailureKind::NotSignedIn) }),
        r#"{"id":"part_6","task":"task_1","run":"run_1","seq":6,"at_ms":1,"body":{"kind":"notice","tone":"error","text":"Claude Code is not signed in.","next":"Run claude","failure":"not_signed_in"}}"#,
    );
    // A notice stored before version 6 has no failure kind: it still reads.
    let old: PartBody = serde_json::from_str(r#"{"kind":"notice","tone":"error","text":"It stopped.","next":"Try again"}"#).unwrap();
    assert_eq!(old, PartBody::Notice { tone: Tone::Error, text: "It stopped.".into(), next: Some("Try again".into()), failure: None });
    let perm = Permission {
        id: PermissionId("perm_1".into()),
        task: TaskId("task_1".into()),
        run: RunId("run_1".into()),
        tool: "Bash".into(),
        title: "Run a command".into(),
        why: "Commands can change anything on this Mac.".into(),
        detail: "cargo test".into(),
        destructive: false,
        state: PermissionState::Pending,
        asked_ms: 3,
        answered_ms: None,
    };
    pinned(
        perm.clone(),
        r#"{"id":"perm_1","task":"task_1","run":"run_1","tool":"Bash","title":"Run a command","why":"Commands can change anything on this Mac.","detail":"cargo test","destructive":false,"state":"pending","asked_ms":3}"#,
    );
    for (s, name) in [(PermissionState::Pending, "pending"), (PermissionState::Allowed, "allowed"), (PermissionState::AlwaysAllowed, "always_allowed"), (PermissionState::Denied, "denied"), (PermissionState::Expired, "expired")] {
        pinned(s, &format!("\"{name}\""));
    }
    for (s, name) in [(TaskStatus::Working, "working"), (TaskStatus::NeedsYou, "needs_you"), (TaskStatus::Done, "done"), (TaskStatus::Failed, "failed"), (TaskStatus::Stopped, "stopped"), (TaskStatus::Interrupted, "interrupted")] {
        pinned(s, &format!("\"{name}\""));
    }
    pinned(ServerMessage::Event { revision: 9, change: Change::PermissionChanged { permission: perm } }, &format!(r#"{{"kind":"event","revision":9,"change":{{"type":"permission_changed","permission":{}}}}}"#, r#"{"id":"perm_1","task":"task_1","run":"run_1","tool":"Bash","title":"Run a command","why":"Commands can change anything on this Mac.","detail":"cargo test","destructive":false,"state":"pending","asked_ms":3}"#));
    pinned(ServerMessage::Event { revision: 0, change: Change::Activity { task: TaskId("task_1".into()), thinking: true } }, r#"{"kind":"event","revision":0,"change":{"type":"activity","task":"task_1","thinking":true}}"#);
    pinned(ToolState::Denied, "\"denied\"");
    pinned(
        ServerMessage::Event { revision: 9, change: Change::PartDelta { task: TaskId("task_1".into()), part: PartId("part_2".into()), text: "more".into() } },
        r#"{"kind":"event","revision":9,"change":{"type":"part_delta","task":"task_1","part":"part_2","text":"more"}}"#,
    );
}

#[test]
fn transcripts_runtimes_and_usage_round_trip() {
    let transcript = Transcript { task: task(), runs: vec![run()], parts: Vec::new(), permissions: Vec::new(), queued: Vec::new() };
    let back: Reply = serde_json::from_str(&serde_json::to_string(&Reply::Transcript { transcript: transcript.clone() }).unwrap()).unwrap();
    assert_eq!(back, Reply::Transcript { transcript });
    pinned(
        RuntimeInfo { id: "codex".into(), name: "Codex".into(), found: false, version: None, signed_in: None, login: None, sign_in: "codex login".into(), install: "npm install -g @openai/codex".into(), supported: false, rules: None },
        r#"{"id":"codex","name":"Codex","found":false,"sign_in":"codex login","install":"npm install -g @openai/codex","supported":false}"#,
    );
    pinned(UsageTotal { runs: 2, input_tokens: 3, output_tokens: 4, cost_usd: Some(0.5), api_equivalent: true }, r#"{"runs":2,"input_tokens":3,"output_tokens":4,"cost_usd":0.5,"api_equivalent":true}"#);
    pinned(ServerMessage::Reply { id: 3, reply: Reply::Ok }, r#"{"kind":"reply","id":3,"reply":{"type":"ok"}}"#);
}

fn perm() -> Permission {
    Permission {
        id: PermissionId("perm_1".into()),
        task: TaskId("task_1".into()),
        run: RunId("run_1".into()),
        tool: "Bash".into(),
        title: "Run a command".into(),
        why: "Commands can change anything on this Mac.".into(),
        detail: "cargo test".into(),
        destructive: false,
        state: PermissionState::Pending,
        asked_ms: 3,
        answered_ms: None,
    }
}

#[test]
fn needs_you_round_trips() {
    pinned(ClientMessage { id: 1, request: Request::ListNeeds }, r#"{"id":1,"type":"list_needs"}"#);
    pinned(
        ClientMessage { id: 2, request: Request::FollowUp { task: TaskId("task_1".into()), words: "On main.".into(), answering: Some(RunId("run_1".into())), mode: None } },
        r#"{"id":2,"type":"follow_up","task":"task_1","words":"On main.","answering":"run_1"}"#,
    );
    let item = |id: &str, need: Need| NeedsItem { id: id.into(), task: TaskId("task_1".into()), workspace: "ws_1".into(), title: "Fix the build".into(), cat: "cat_tofu".into(), cat_name: "tofu".into(), since_ms: 5, need, subagent: None };
    let head = |id: &str| format!(r#"{{"id":"{id}","task":"task_1","workspace":"ws_1","title":"Fix the build","cat":"cat_tofu","cat_name":"tofu","since_ms":5,"need":"#);
    let p = r#"{"id":"perm_1","task":"task_1","run":"run_1","tool":"Bash","title":"Run a command","why":"Commands can change anything on this Mac.","detail":"cargo test","destructive":false,"state":"pending","asked_ms":3}"#;
    pinned(item("perm:perm_1", Need::Permission { permission: perm() }), &format!(r#"{}{{"kind":"permission","permission":{p}}}}}"#, head("perm:perm_1")));
    pinned(
        item("question:run_1", Need::Question { run: RunId("run_1".into()), question: "Which branch should it go on?".into(), kitten: None, via: vec![], unanswered: false }),
        &format!(r#"{}{{"kind":"question","run":"run_1","question":"Which branch should it go on?"}}}}"#, head("question:run_1")),
    );
    let failure = Need::Failure { failure: FailureKind::NotSignedIn, text: "Claude Code is not signed in.".into(), next: Some("Run `claude auth login` in Terminal.".into()), command: Some("claude auth login".into()) };
    pinned(
        item("failure:part_6", failure),
        &format!(r#"{}{{"kind":"failure","failure":"not_signed_in","text":"Claude Code is not signed in.","next":"Run `claude auth login` in Terminal.","command":"claude auth login"}}}}"#, head("failure:part_6")),
    );
    for (k, name) in [(FailureKind::NotInstalled, "not_installed"), (FailureKind::NotSignedIn, "not_signed_in"), (FailureKind::UsageLimit, "usage_limit"), (FailureKind::FolderMissing, "folder_missing")] {
        pinned(k, &format!("\"{name}\""));
    }
    pinned(ServerMessage::Reply { id: 1, reply: Reply::Needs { items: Vec::new() } }, r#"{"kind":"reply","id":1,"reply":{"type":"needs","items":[]}}"#);
    pinned(
        ServerMessage::Event { revision: 12, change: Change::NeedsChanged { items: vec![item("question:run_1", Need::Question { run: RunId("run_1".into()), question: "Shall I?".into(), kitten: None, via: vec![], unanswered: false })] } },
        &format!(r#"{{"kind":"event","revision":12,"change":{{"type":"needs_changed","items":[{}{{"kind":"question","run":"run_1","question":"Shall I?"}}}}]}}}}"#, head("question:run_1")),
    );
}

#[test]
fn undo_round_trips() {
    let t = || TaskId("task_1".into());
    let r = || RunId("run_1".into());
    pinned(ClientMessage { id: 1, request: Request::GetDiff { task: t(), run: Some(r()), source: DiffSource::Run } }, r#"{"id":1,"type":"get_diff","task":"task_1","run":"run_1"}"#);
    pinned(ClientMessage { id: 2, request: Request::GetDiff { task: t(), run: None, source: DiffSource::Run } }, r#"{"id":2,"type":"get_diff","task":"task_1"}"#);
    pinned(
        ClientMessage { id: 3, request: Request::GetDiffFile { task: t(), run: Some(r()), path: "src/lib.rs".into(), full: true, source: DiffSource::Run } },
        r#"{"id":3,"type":"get_diff_file","task":"task_1","run":"run_1","path":"src/lib.rs","full":true}"#,
    );
    pinned(ClientMessage { id: 4, request: Request::UndoRun { task: t(), run: r(), anyway: false } }, r#"{"id":4,"type":"undo_run","task":"task_1","run":"run_1"}"#);
    pinned(ClientMessage { id: 5, request: Request::RedoRun { task: t(), run: r(), anyway: true } }, r#"{"id":5,"type":"redo_run","task":"task_1","run":"run_1","anyway":true}"#);
    pinned(ClientMessage { id: 6, request: Request::UndoStorage }, r#"{"id":6,"type":"undo_storage"}"#);
    pinned(ClientMessage { id: 7, request: Request::ClearUndo }, r#"{"id":7,"type":"clear_undo"}"#);
    let file = DiffFile { path: "docs/start.md".into(), old_path: Some("docs/guide.md".into()), status: FileStatus::Renamed, added: 1, removed: 0, binary: false, old_size: None, new_size: None, old_mode: None, new_mode: None };
    let logo = DiffFile { path: "logo.png".into(), old_path: None, status: FileStatus::Modified, added: 0, removed: 0, binary: true, old_size: Some(6), new_size: Some(8), old_mode: Some("100644".into()), new_mode: Some("100755".into()) };
    pinned(
        Reply::Diff { diff: DiffSummary { task: t(), run: Some(r()), source: DiffSource::Run, root: "/Users/example/acme-api".into(), files: vec![file, logo], added: 1, removed: 0 } },
        r#"{"type":"diff","diff":{"task":"task_1","run":"run_1","root":"/Users/example/acme-api","files":[{"path":"docs/start.md","old_path":"docs/guide.md","status":"renamed","added":1,"removed":0},{"path":"logo.png","status":"modified","added":0,"removed":0,"binary":true,"old_size":6,"new_size":8,"old_mode":"100644","new_mode":"100755"}],"added":1,"removed":0}}"#,
    );
    pinned(ClientMessage { id: 20, request: Request::GetDiff { task: t(), run: None, source: DiffSource::Thread } }, r#"{"id":20,"type":"get_diff","task":"task_1","source":"thread"}"#);
    pinned(ClientMessage { id: 21, request: Request::GetDiff { task: t(), run: None, source: DiffSource::Uncommitted } }, r#"{"id":21,"type":"get_diff","task":"task_1","source":"uncommitted"}"#);
    pinned(
        ClientMessage { id: 22, request: Request::GetDiffFile { task: t(), run: None, path: "a.rs".into(), full: false, source: DiffSource::Uncommitted } },
        r#"{"id":22,"type":"get_diff_file","task":"task_1","path":"a.rs","source":"uncommitted"}"#,
    );
    pinned(ClientMessage { id: 23, request: Request::GetOverview { task: t() } }, r#"{"id":23,"type":"get_overview","task":"task_1"}"#);
    let changes = Changes { files: 1, added: 3, removed: 1 };
    pinned(
        Reply::Overview { overview: Overview { task: t(), folder: Some("/Users/example/acme-api".into()), branch: Some("neko/4f2a".into()), changes: None, uncommitted: Some(changes) } },
        r#"{"type":"overview","overview":{"task":"task_1","folder":"/Users/example/acme-api","branch":"neko/4f2a","uncommitted":{"files":1,"added":3,"removed":1}}}"#,
    );
    pinned(Reply::DiffFile { file: DiffPatch { path: "a.rs".into(), lines: vec!["@@ -1 +1 @@".into(), "-a".into(), "+b".into()], total: 3, cut: false } }, r#"{"type":"diff_file","file":{"path":"a.rs","lines":["@@ -1 +1 @@","-a","+b"],"total":3}}"#);
    pinned(Reply::ChangedSince { files: vec!["README.md".into()] }, r#"{"type":"changed_since","files":["README.md"]}"#);
    pinned(Reply::UndoStorage { storage: UndoStorage { bytes: 1200, checkpoints: 4, tasks: 2 } }, r#"{"type":"undo_storage","storage":{"bytes":1200,"checkpoints":4,"tasks":2}}"#);
    let mut with = run();
    with.undo = Some(RunUndo { state: UndoState::Unavailable, why: Some("the repository is in the middle of a merge".into()), changes: None, committed: false });
    let json = serde_json::to_string(&with).unwrap();
    assert!(json.ends_with(r#""undo":{"state":"unavailable","why":"the repository is in the middle of a merge"}}"#), "{json}");
    pinned(RunUndo { state: UndoState::Applied, why: None, changes: Some(Changes { files: 1, added: 2, removed: 3 }), committed: true }, r#"{"state":"applied","changes":{"files":1,"added":2,"removed":3},"committed":true}"#);
    for (s, name) in [(UndoState::Taking, "taking"), (UndoState::Undone, "undone"), (UndoState::Dropped, "dropped"), (UndoState::Removed, "removed")] {
        assert_eq!(serde_json::to_string(&s).unwrap(), format!("\"{name}\""));
    }
}

fn cat() -> Cat {
    Cat {
        id: "cat_miso".into(),
        name: "miso".into(),
        face: "ginger".into(),
        purpose: Some("Keeps the docs tidy.".into()),
        workspace: Some("ws_1".into()),
        thread: Some(TaskId("task_m".into())),
        model: Some("sonnet".into()),
        access: Some(PermissionMode::Ask),
        created_ms: 1,
        retired_ms: None,
        made_in: None,
    }
}

#[test]
fn cats_round_trip() {
    let c = || "cat_miso".to_owned();
    pinned(ClientMessage { id: 1, request: Request::ListCats }, r#"{"id":1,"type":"list_cats"}"#);
    pinned(
        ClientMessage { id: 2, request: Request::CreateCat { name: "Asha's helper".into(), face: Some("tabby".into()), purpose: None, workspace: Some("ws_1".into()) } },
        r#"{"id":2,"type":"create_cat","name":"Asha's helper","face":"tabby","workspace":"ws_1"}"#,
    );
    pinned(ClientMessage { id: 3, request: Request::CreateCat { name: "pip".into(), face: None, purpose: None, workspace: None } }, r#"{"id":3,"type":"create_cat","name":"pip"}"#);
    pinned(ClientMessage { id: 4, request: Request::RenameCat { cat: c(), name: "Miso".into() } }, r#"{"id":4,"type":"rename_cat","cat":"cat_miso","name":"Miso"}"#);
    pinned(ClientMessage { id: 5, request: Request::SetCatFace { cat: c(), face: "void".into() } }, r#"{"id":5,"type":"set_cat_face","cat":"cat_miso","face":"void"}"#);
    pinned(ClientMessage { id: 6, request: Request::SetCatPurpose { cat: c(), purpose: "Docs".into() } }, r#"{"id":6,"type":"set_cat_purpose","cat":"cat_miso","purpose":"Docs"}"#);
    pinned(ClientMessage { id: 7, request: Request::SetCatWorkspace { cat: c(), workspace: None } }, r#"{"id":7,"type":"set_cat_workspace","cat":"cat_miso"}"#);
    // A Cattain (version 19): its model and access, and its files read, saved and restored by name.
    pinned(ClientMessage { id: 8, request: Request::SetCatModel { cat: c(), model: Some("sonnet".into()) } }, r#"{"id":8,"type":"set_cat_model","cat":"cat_miso","model":"sonnet"}"#);
    pinned(ClientMessage { id: 9, request: Request::SetCatAccess { cat: c(), access: None } }, r#"{"id":9,"type":"set_cat_access","cat":"cat_miso"}"#);
    pinned(ClientMessage { id: 16, request: Request::SetCatAccess { cat: c(), access: Some(PermissionMode::Full) } }, r#"{"id":16,"type":"set_cat_access","cat":"cat_miso","access":"full"}"#);
    pinned(ClientMessage { id: 17, request: Request::GetCattainFile { cat: c(), name: "DIGEST.md".into() } }, r#"{"id":17,"type":"get_cattain_file","cat":"cat_miso","name":"DIGEST.md"}"#);
    pinned(ClientMessage { id: 18, request: Request::SaveCattainFile { cat: c(), name: "CREW.md".into(), text: "Open a PR.\n".into() } }, r#"{"id":18,"type":"save_cattain_file","cat":"cat_miso","name":"CREW.md","text":"Open a PR.\n"}"#);
    pinned(ClientMessage { id: 19, request: Request::RestoreCattainFile { cat: c(), name: "RULES.md".into() } }, r#"{"id":19,"type":"restore_cattain_file","cat":"cat_miso","name":"RULES.md"}"#);
    let file = CattainFile { cat: c(), name: "RULES.md".into(), text: "- Use British spelling.".into(), cut: false, previous: true, changed_ms: Some(9), path: "/Users/example/neko/cattains/cat_miso/RULES.md".into(), editable: true };
    let fj = r#"{"cat":"cat_miso","name":"RULES.md","text":"- Use British spelling.","previous":true,"changed_ms":9,"path":"/Users/example/neko/cattains/cat_miso/RULES.md","editable":true}"#;
    pinned(Reply::CattainFile { file: file.clone() }, &format!(r#"{{"type":"cattain_file","file":{fj}}}"#));
    pinned(Change::CattainFileChanged { file: CattainFile { name: "DIGEST.md".into(), previous: false, changed_ms: None, editable: false, text: String::new(), path: "/d".into(), ..file } }, r#"{"type":"cattain_file_changed","file":{"cat":"cat_miso","name":"DIGEST.md","text":"","path":"/d"}}"#);
    pinned(
        ClientMessage { id: 20, request: Request::CreateTask { crew: Some(c()), words: "Fix it".into(), workspace: None, in_folder: false, from_branch: None } },
        r#"{"id":20,"type":"create_task","words":"Fix it","crew":"cat_miso"}"#,
    );
    pinned(ClientMessage { id: 10, request: Request::RetireCat { cat: c() } }, r#"{"id":10,"type":"retire_cat","cat":"cat_miso"}"#);
    pinned(ClientMessage { id: 11, request: Request::BringBackCat { cat: c() } }, r#"{"id":11,"type":"bring_back_cat","cat":"cat_miso"}"#);
    // A new thread takes no cat (version 14): where it works is its own choice, and each is written only when it is made.
    pinned(
        ClientMessage { id: 13, request: Request::CreateTask { crew: None, words: "Fix it".into(), workspace: Some("ws_1".into()), in_folder: true, from_branch: Some("next".into()) } },
        r#"{"id":13,"type":"create_task","words":"Fix it","workspace":"ws_1","in_folder":true,"from_branch":"next"}"#,
    );
    pinned(ClientMessage { id: 14, request: Request::ListBranches { workspace: "ws_1".into() } }, r#"{"id":14,"type":"list_branches","workspace":"ws_1"}"#);
    pinned(
        ServerMessage::Reply { id: 14, reply: Reply::Branches { workspace: "ws_1".into(), git: true, current: Some("main".into()), branches: vec!["main".into(), "next".into()] } },
        r#"{"kind":"reply","id":14,"reply":{"type":"branches","workspace":"ws_1","git":true,"current":"main","branches":["main","next"]}}"#,
    );
    pinned(ServerMessage::Reply { id: 15, reply: Reply::Branches { workspace: "default".into(), git: false, current: None, branches: Vec::new() } }, r#"{"kind":"reply","id":15,"reply":{"type":"branches","workspace":"default","git":false}}"#);
    let j = r#"{"id":"cat_miso","name":"miso","face":"ginger","purpose":"Keeps the docs tidy.","workspace":"ws_1","thread":"task_m","model":"sonnet","access":"ask","created_ms":1}"#;
    pinned(Reply::Cat { cat: cat() }, &format!(r#"{{"type":"cat","cat":{j}}}"#));
    pinned(Reply::Cats { cats: vec![cat()] }, &format!(r#"{{"type":"cats","cats":[{j}]}}"#));
    pinned(ServerMessage::Event { revision: 3, change: Change::CatAdded { cat: cat() } }, &format!(r#"{{"kind":"event","revision":3,"change":{{"type":"cat_added","cat":{j}}}}}"#));
    let mut retired = cat();
    retired.retired_ms = Some(20);
    retired.purpose = None;
    let encoded = serde_json::to_string(&Change::CatUpdated { cat: retired.clone() }).unwrap();
    assert!(encoded.starts_with(r#"{"type":"cat_updated","cat":{"id":"cat_miso","name":"miso","face":"ginger","workspace":"ws_1""#) && encoded.ends_with(r#""access":"ask","created_ms":1,"retired_ms":20}}"#), "{encoded}");
    assert!(!retired.is_active());
    pinned(ErrorCode::InvalidName, "\"invalid_name\"");
    // A run says which cat and which version of its rules went with it; a quiet part says when they changed.
    let mut r = run();
    r.cat = Some(c());
    r.rules_hash = Some("cbf29ce484222325".into());
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.ends_with(r#""cat":"cat_miso","rules_hash":"cbf29ce484222325"}"#), "{json}");
    pinned(PartBody::RulesChanged { cat: c(), name: "miso".into(), file: None }, r#"{"kind":"rules_changed","cat":"cat_miso","name":"miso"}"#);
    pinned(PartBody::RulesChanged { cat: c(), name: "miso".into(), file: Some("CREW.md".into()) }, r#"{"kind":"rules_changed","cat":"cat_miso","name":"miso","file":"CREW.md"}"#);
    assert_eq!(PROTOCOL_VERSION, 23);
    // A Cattain's own thread says so; any other thread writes nothing.
    let mut own = task();
    own.cattain = true;
    assert!(serde_json::to_string(&own).unwrap().ends_with(r#""cattain":true}"#));
    assert!((own.crew_of(), own.listed()) == (None, false));
    assert_eq!((task().crew_of(), task().listed()), (Some("cat_miso"), true));
    // A thread with no cat writes none; one that chose its place writes the choice; an old screen's task (a cat, no choice) still reads.
    let mut t = task();
    t.cat = None;
    t.in_folder = true;
    t.from_branch = Some("next".into());
    let json = serde_json::to_string(&t).unwrap();
    assert!(!json.contains("\"cat\"") && json.contains(r#""in_folder":true,"from_branch":"next""#), "{json}");
    assert_eq!(serde_json::from_str::<Task>(&json).unwrap(), t);
    let old: Task = serde_json::from_str(r#"{"id":"task_1","workspace":"default","words":"w","title":"t","created_ms":1,"status":"new","cat":"cat_miso"}"#).unwrap();
    assert_eq!((old.cat.as_deref(), old.in_folder, old.from_branch), (Some("cat_miso"), false, None));
}

#[test]
fn subagents_have_their_own_shapes_and_the_old_ones_do_not_change() {
    // A thread has no `subagent`; a subagent says whose it is and its number.
    let t = task();
    assert!(!serde_json::to_string(&t).unwrap().contains("subagent"));
    let mut s = task();
    s.subagent = Some(SubagentOf::new(TaskId("task_1".into()), "1.2", TaskId("task_0".into())));
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.ends_with(r#""subagent":{"parent":"task_1","number":"1.2","root":"task_0"}}"#), "{json}");
    let back: Task = serde_json::from_str(&json).unwrap();
    assert_eq!((back.subagent.as_ref().unwrap().depth(), back.subagent_name().as_deref(), back.is_thread()), (2, Some("Kitten 1.2"), false));
    assert_eq!(SubagentOf::new(TaskId("p".into()), "1", TaskId("p".into())).depth(), 1);
    // Questions go up (20): a run's question, a kitten's, a question's wake, the note that the agent answered, a passed-up item.
    pinned(RunAsk { to: AskTo::Parent, text: "Which database?".into(), chain: vec![], unanswered: false }, r#"{"to":"parent","text":"Which database?"}"#);
    pinned(RunAsk { to: AskTo::Person, text: "Which database?".into(), chain: vec!["1.2".into(), "1".into()], unanswered: true }, r#"{"to":"person","text":"Which database?","chain":["1.2","1"],"unanswered":true}"#);
    let mut asked = run();
    asked.ask = Some(RunAsk { to: AskTo::Person, text: "Shall I?".into(), chain: vec!["1".into()], unanswered: false });
    assert!(serde_json::to_string(&asked).unwrap().ends_with(r#""ask":{"to":"person","text":"Shall I?","chain":["1"]}}"#));
    let mut of = SubagentOf::new(TaskId("p".into()), "1", TaskId("p".into()));
    of.question = Some(KittenQuestion { text: "Which database?".into(), chain: vec![], with: QuestionWith::Person });
    pinned(of, r#"{"parent":"p","number":"1","root":"p","question":{"text":"Which database?","with":"person"}}"#);
    pinned(QuestionWith::Up, "\"up\"");
    pinned(
        PartBody::Wake { number: "1".into(), title: "Migrate".into(), status: "asked".into(), summary: "Which one?".into(), files: vec![], branch: None, question: Some("Which one?".into()) },
        r#"{"kind":"wake","number":"1","title":"Migrate","status":"asked","summary":"Which one?","question":"Which one?"}"#,
    );
    pinned(PartBody::ToSubagent { number: "1".into(), text: "Use SQLite.".into(), answer: true }, r#"{"kind":"to_subagent","number":"1","text":"Use SQLite.","answer":true}"#);
    pinned(
        Need::Question { run: RunId("run_1".into()), question: "Which database?".into(), kitten: Some("1.2".into()), via: vec!["1".into()], unanswered: false },
        r#"{"kind":"question","run":"run_1","question":"Which database?","kitten":"1.2","via":["1"]}"#,
    );
    pinned(TaskStatus::Waiting, "\"waiting\"");
    pinned(PartBody::Subagent { subagent: TaskId("task_2".into()), number: "1".into(), title: "Check links".into() }, r#"{"kind":"subagent","subagent":"task_2","number":"1","title":"Check links"}"#);
    pinned(
        PartBody::Wake { number: "1".into(), title: "Check links".into(), status: "done".into(), summary: "Fixed 2.".into(), files: vec!["a.rs +1 -0".into()], branch: Some("neko/x".into()), question: None },
        r#"{"kind":"wake","number":"1","title":"Check links","status":"done","summary":"Fixed 2.","files":["a.rs +1 -0"],"branch":"neko/x"}"#,
    );
    pinned(PartBody::Wake { number: "2".into(), title: "T".into(), status: "failed".into(), summary: String::new(), files: vec![], branch: None, question: None }, r#"{"kind":"wake","number":"2","title":"T","status":"failed","summary":""}"#);
    // The setting that gives runs the person's own MCP servers is written only when it is on.
    pinned(Settings { runtime: "claude".into(), permission_mode: PermissionMode::Follow, own_mcp: false, queue_first: false, ask_first_subagent: false, cattains_by_hand: false }, r#"{"runtime":"claude","permission_mode":"follow"}"#);
    pinned(Settings { runtime: "claude".into(), permission_mode: PermissionMode::Follow, own_mcp: true, queue_first: false, ask_first_subagent: false, cattains_by_hand: false }, r#"{"runtime":"claude","permission_mode":"follow","own_mcp":true}"#);
    let old: Settings = serde_json::from_str(r#"{"runtime":"claude","permission_mode":"ask"}"#).unwrap();
    assert!(!old.own_mcp, "a screen from before the setting sends none");
}

#[test]
fn steer_queue_and_interrupt_round_trip() {
    let t = || TaskId("task_1".into());
    pinned(ClientMessage { id: 6, request: Request::FollowUp { task: t(), words: "And the tests".into(), answering: None, mode: Some(SendMode::Queue) } }, r#"{"id":6,"type":"follow_up","task":"task_1","words":"And the tests","mode":"queue"}"#);
    pinned(ClientMessage { id: 6, request: Request::FollowUp { task: t(), words: "Stop".into(), answering: None, mode: Some(SendMode::Interrupt) } }, r#"{"id":6,"type":"follow_up","task":"task_1","words":"Stop","mode":"interrupt"}"#);
    pinned(ClientMessage { id: 7, request: Request::SendQueuedNow { task: t(), queued: QueuedId("queued_1".into()) } }, r#"{"id":7,"type":"send_queued_now","task":"task_1","queued":"queued_1"}"#);
    pinned(ClientMessage { id: 8, request: Request::DiscardQueued { task: t(), queued: QueuedId("queued_1".into()) } }, r#"{"id":8,"type":"discard_queued","task":"task_1","queued":"queued_1"}"#);
    let q = Queued { id: QueuedId("queued_1".into()), task: t(), words: "Next".into(), kind: QueuedKind::Steered, at_ms: 5 };
    pinned(q.clone(), r#"{"id":"queued_1","task":"task_1","words":"Next","kind":"steered","at_ms":5}"#);
    pinned(
        ServerMessage::Event { revision: 3, change: Change::QueueChanged { task: t(), queued: vec![q] } },
        r#"{"kind":"event","revision":3,"change":{"type":"queue_changed","task":"task_1","queued":[{"id":"queued_1","task":"task_1","words":"Next","kind":"steered","at_ms":5}]}}"#,
    );
    // Queue-first is written only when on, and a screen from before it sends none.
    pinned(Settings { runtime: "claude".into(), permission_mode: PermissionMode::Follow, own_mcp: false, queue_first: true, ask_first_subagent: false, cattains_by_hand: false }, r#"{"runtime":"claude","permission_mode":"follow","queue_first":true}"#);
    pinned(QueuedKind::Interrupting, "\"interrupting\"");
}

#[test]
fn a_thread_can_be_pinned_settled_or_archived_and_says_so() {
    pinned(ClientMessage { id: 5, request: Request::PinTask { task: TaskId("task_1".into()), pinned: true } }, r#"{"id":5,"type":"pin_task","task":"task_1","pinned":true}"#);
    pinned(ClientMessage { id: 6, request: Request::SettleTask { task: TaskId("task_1".into()), settled: false } }, r#"{"id":6,"type":"settle_task","task":"task_1","settled":false}"#);
    pinned(ClientMessage { id: 7, request: Request::ArchiveTask { task: TaskId("task_1".into()), archived: true } }, r#"{"id":7,"type":"archive_task","task":"task_1","archived":true}"#);
    // None of the three is written until it is true, so a thread that is none looks as it did before version 17, and a task from a
    // daemon that knows nothing of them reads as none.
    assert!(!serde_json::to_string(&task()).unwrap().contains("pinned"));
    let mut t = task();
    t.pinned = true;
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.ends_with(r#""pinned":true}"#), "{json}");
    assert_eq!(serde_json::from_str::<Task>(&json).unwrap(), t);
    let old: Task = serde_json::from_str(r#"{"id":"task_1","workspace":"default","words":"w","title":"t","created_ms":1,"status":"new"}"#).unwrap();
    assert!(!old.pinned && !old.settled && !old.archived);
}

#[test]
fn a_search_asks_with_its_text_and_is_answered_with_why_each_thread_is_there() {
    use neko_protocol::{Found, FoundBy, PartId};
    pinned(ClientMessage { id: 8, request: Request::Search { query: "login status:needs".into(), limit: None } }, r#"{"id":8,"type":"search","query":"login status:needs"}"#);
    pinned(ClientMessage { id: 9, request: Request::Search { query: String::new(), limit: Some(6) } }, r#"{"id":9,"type":"search","query":"","limit":6}"#);
    let title = Found { task: TaskId("task_1".into()), kitten: false, by: FoundBy::Title, snippet: String::new(), part: None, seq: 0 };
    let said = Found { task: TaskId("task_2".into()), kitten: true, by: FoundBy::Message, snippet: "…the login page…".into(), part: Some(PartId("part_3".into())), seq: 3 };
    pinned(
        ServerMessage::Reply { id: 8, reply: Reply::Found { query: "login".into(), hits: vec![title, said], problem: None, took_ms: 4 } },
        r#"{"kind":"reply","id":8,"reply":{"type":"found","query":"login","hits":[{"task":"task_1","by":"title"},{"task":"task_2","kitten":true,"by":"message","snippet":"…the login page…","part":"part_3","seq":3}],"took_ms":4}}"#,
    );
    pinned(
        ServerMessage::Reply { id: 9, reply: Reply::Found { query: "status:x".into(), hits: Vec::new(), problem: Some("“x” is not a status.".into()), took_ms: 0 } },
        r#"{"kind":"reply","id":9,"reply":{"type":"found","query":"status:x","hits":[],"problem":"“x” is not a status.","took_ms":0}}"#,
    );
}

#[test]
fn a_kitten_says_where_it_works_what_it_follows_and_whether_it_is_a_draft() {
    // The old shape is unchanged: a kitten in the shared folder writes none of the new fields.
    let mut t = task();
    t.subagent = Some(SubagentOf::new(TaskId("task_1".into()), "1", TaskId("task_1".into())));
    assert!(serde_json::to_string(&t).unwrap().ends_with(r#""subagent":{"parent":"task_1","number":"1","root":"task_1"}}"#));
    // A kitten from a daemon of version 17 reads as in the shared folder, not a draft.
    let old: Task = serde_json::from_str(r#"{"id":"task_2","workspace":"default","words":"w","title":"t","created_ms":1,"status":"new","subagent":{"parent":"task_1","number":"1","root":"task_1"}}"#).unwrap();
    let of = old.subagent.unwrap();
    assert_eq!((of.placement, of.after, of.draft, of.discarded, of.branch), (SubPlace::Shared, None, false, false, None));
    // A stacked draft in a copy of its own.
    let mut of = SubagentOf::new(TaskId("task_1".into()), "2", TaskId("task_1".into()));
    of.placement = SubPlace::OwnCopy;
    of.after = Some("1".into());
    of.draft = true;
    of.branch = Some("neko/4f2a-2".into());
    t.subagent = Some(of);
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.ends_with(r#""subagent":{"parent":"task_1","number":"2","root":"task_1","placement":"own_copy","after":"1","draft":true,"branch":"neko/4f2a-2"}}"#), "{json}");
    assert_eq!(serde_json::from_str::<Task>(&json).unwrap(), t);
    assert!(t.subagent.as_ref().is_some_and(|s| s.draft) && !t.is_live_subagent(), "a draft is not waited on");
    pinned(ClientMessage { id: 5, request: Request::StartDraft { task: TaskId("task_2".into()) } }, r#"{"id":5,"type":"start_draft","task":"task_2"}"#);
    pinned(ClientMessage { id: 6, request: Request::EditDraft { task: TaskId("task_2".into()), prompt: "Read the docs".into() } }, r#"{"id":6,"type":"edit_draft","task":"task_2","prompt":"Read the docs"}"#);
    pinned(ClientMessage { id: 7, request: Request::DiscardDraft { task: TaskId("task_2".into()) } }, r#"{"id":7,"type":"discard_draft","task":"task_2"}"#);
    pinned(Settings { runtime: "claude".into(), permission_mode: PermissionMode::Follow, own_mcp: false, queue_first: false, ask_first_subagent: true, cattains_by_hand: false }, r#"{"runtime":"claude","permission_mode":"follow","ask_first_subagent":true}"#);
    let old: Settings = serde_json::from_str(r#"{"runtime":"claude","permission_mode":"ask"}"#).unwrap();
    assert!(!old.ask_first_subagent);
}

#[test]
fn a_cattain_made_by_asking_round_trips() {
    // Version 22: a cat says the thread whose agent made it; the note in that thread; Undo; the setting.
    let mut made = cat();
    made.made_in = Some(TaskId("task_1".into()));
    let json = serde_json::to_string(&made).unwrap();
    assert!(json.ends_with(r#""created_ms":1,"made_in":"task_1"}"#), "{json}");
    assert_eq!(serde_json::from_str::<Cat>(&json).unwrap(), made);
    assert!(!serde_json::to_string(&cat()).unwrap().contains("made_in"), "a cat made by hand writes none");
    pinned(PartBody::CattainMade { cat: "cat_y".into(), name: "yuzu".into(), purpose: Some("iOS work".into()) }, r#"{"kind":"cattain_made","cat":"cat_y","name":"yuzu","purpose":"iOS work"}"#);
    pinned(PartBody::CattainMade { cat: "cat_y".into(), name: "yuzu".into(), purpose: None }, r#"{"kind":"cattain_made","cat":"cat_y","name":"yuzu"}"#);
    pinned(ClientMessage { id: 1, request: Request::UndoMakeCattain { cat: "cat_y".into() } }, r#"{"id":1,"type":"undo_make_cattain","cat":"cat_y"}"#);
    pinned(Settings { runtime: "claude".into(), permission_mode: PermissionMode::Follow, own_mcp: false, queue_first: false, ask_first_subagent: false, cattains_by_hand: true }, r#"{"runtime":"claude","permission_mode":"follow","cattains_by_hand":true}"#);
    let old: Settings = serde_json::from_str(r#"{"runtime":"claude","permission_mode":"ask"}"#).unwrap();
    assert!(!old.cattains_by_hand, "agents may make Cattains unless the person said otherwise");
    let pool = neko_protocol::cats::NAME_POOL;
    assert!(pool.len() > neko_protocol::cats::MAX_ACTIVE && !pool.iter().any(|n| neko_protocol::cats::STARTERS.iter().any(|s| s.1 == *n)));
    assert_eq!(PROTOCOL_VERSION, 23);
}

#[test]
fn a_crew_round_trips() {
    // A thread a Cattain's agent started says so; one the person started writes nothing.
    let mut t = task();
    assert!(!serde_json::to_string(&t).unwrap().contains("started_by"));
    t.started_by = Some("cat_miso".into());
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains(r#""started_by":"cat_miso""#), "{json}");
    assert_eq!(serde_json::from_str::<Task>(&json).unwrap(), t);
    pinned(
        PartBody::CrewWake { thread: TaskId("task_2".into()), title: "Fix login".into(), status: "done".into(), summary: "Fixed.".into(), files: vec!["a.rs +1 -0".into()], question: None },
        r#"{"kind":"crew_wake","thread":"task_2","title":"Fix login","status":"done","summary":"Fixed.","files":["a.rs +1 -0"]}"#,
    );
    pinned(
        PartBody::CrewWake { thread: TaskId("task_2".into()), title: "Fix login".into(), status: "asked".into(), summary: "Which?".into(), files: vec![], question: Some("Which?".into()) },
        r#"{"kind":"crew_wake","thread":"task_2","title":"Fix login","status":"asked","summary":"Which?","question":"Which?"}"#,
    );
    assert_eq!(PROTOCOL_VERSION, 23);
}

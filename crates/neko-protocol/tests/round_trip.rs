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
        cat: "cat_miso".into(),
        round: None,
        last_read_ms: 1_764_000_000_000,
        activity_ms: 1_764_000_000_000,
        latest_line: String::new(),
        failure_line: String::new(),
        thread_status: ThreadStatus::Idle,
        needs_you: false,
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
    pinned(ClientMessage { id: 2, request: Request::CreateTask { words: "Fix the build".into(), workspace: None, cat: None } }, r#"{"id":2,"type":"create_task","words":"Fix the build"}"#);
    pinned(
        ClientMessage { id: 2, request: Request::CreateTask { words: "Fix the build".into(), workspace: Some("ws_1".into()), cat: None } },
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
    pinned(ClientMessage { id: 6, request: Request::FollowUp { task: t(), words: "And the tests".into(), answering: None } }, r#"{"id":6,"type":"follow_up","task":"task_1","words":"And the tests"}"#);
    pinned(ClientMessage { id: 7, request: Request::StopTask { task: t() } }, r#"{"id":7,"type":"stop_task","task":"task_1"}"#);
    pinned(
        ClientMessage { id: 8, request: Request::AnswerPermission { permission: PermissionId("perm_1".into()), decision: Decision::AlwaysAllow } },
        r#"{"id":8,"type":"answer_permission","permission":"perm_1","decision":"always_allow"}"#,
    );
    pinned(ClientMessage { id: 9, request: Request::WatchTask { task: Some(t()) } }, r#"{"id":9,"type":"watch_task","task":"task_1"}"#);
    pinned(ClientMessage { id: 9, request: Request::WatchTask { task: None } }, r#"{"id":9,"type":"watch_task"}"#);
    pinned(ClientMessage { id: 10, request: Request::ListRuntimes }, r#"{"id":10,"type":"list_runtimes"}"#);
    pinned(
        ClientMessage { id: 11, request: Request::SetSettings { settings: Settings { runtime: "claude".into(), permission_mode: PermissionMode::Full } } },
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
    let transcript = Transcript { task: task(), runs: vec![run()], parts: Vec::new(), permissions: Vec::new() };
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
        ClientMessage { id: 2, request: Request::FollowUp { task: TaskId("task_1".into()), words: "On main.".into(), answering: Some(RunId("run_1".into())) } },
        r#"{"id":2,"type":"follow_up","task":"task_1","words":"On main.","answering":"run_1"}"#,
    );
    let item = |id: &str, need: Need| NeedsItem { id: id.into(), task: TaskId("task_1".into()), workspace: "ws_1".into(), title: "Fix the build".into(), cat: "cat_tofu".into(), cat_name: "tofu".into(), since_ms: 5, need };
    let head = |id: &str| format!(r#"{{"id":"{id}","task":"task_1","workspace":"ws_1","title":"Fix the build","cat":"cat_tofu","cat_name":"tofu","since_ms":5,"need":"#);
    let p = r#"{"id":"perm_1","task":"task_1","run":"run_1","tool":"Bash","title":"Run a command","why":"Commands can change anything on this Mac.","detail":"cargo test","destructive":false,"state":"pending","asked_ms":3}"#;
    pinned(item("perm:perm_1", Need::Permission { permission: perm() }), &format!(r#"{}{{"kind":"permission","permission":{p}}}}}"#, head("perm:perm_1")));
    pinned(
        item("question:run_1", Need::Question { run: RunId("run_1".into()), question: "Which branch should it go on?".into() }),
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
        ServerMessage::Event { revision: 12, change: Change::NeedsChanged { items: vec![item("question:run_1", Need::Question { run: RunId("run_1".into()), question: "Shall I?".into() })] } },
        &format!(r#"{{"kind":"event","revision":12,"change":{{"type":"needs_changed","items":[{}{{"kind":"question","run":"run_1","question":"Shall I?"}}}}]}}}}"#, head("question:run_1")),
    );
}

#[test]
fn undo_round_trips() {
    let t = || TaskId("task_1".into());
    let r = || RunId("run_1".into());
    pinned(ClientMessage { id: 1, request: Request::GetDiff { task: t(), run: Some(r()) } }, r#"{"id":1,"type":"get_diff","task":"task_1","run":"run_1"}"#);
    pinned(ClientMessage { id: 2, request: Request::GetDiff { task: t(), run: None } }, r#"{"id":2,"type":"get_diff","task":"task_1"}"#);
    pinned(
        ClientMessage { id: 3, request: Request::GetDiffFile { task: t(), run: Some(r()), path: "src/lib.rs".into(), full: true } },
        r#"{"id":3,"type":"get_diff_file","task":"task_1","run":"run_1","path":"src/lib.rs","full":true}"#,
    );
    pinned(ClientMessage { id: 4, request: Request::UndoRun { task: t(), run: r(), anyway: false } }, r#"{"id":4,"type":"undo_run","task":"task_1","run":"run_1"}"#);
    pinned(ClientMessage { id: 5, request: Request::RedoRun { task: t(), run: r(), anyway: true } }, r#"{"id":5,"type":"redo_run","task":"task_1","run":"run_1","anyway":true}"#);
    pinned(ClientMessage { id: 6, request: Request::UndoStorage }, r#"{"id":6,"type":"undo_storage"}"#);
    pinned(ClientMessage { id: 7, request: Request::ClearUndo }, r#"{"id":7,"type":"clear_undo"}"#);
    let file = DiffFile { path: "docs/start.md".into(), old_path: Some("docs/guide.md".into()), status: FileStatus::Renamed, added: 1, removed: 0, binary: false, old_size: None, new_size: None, old_mode: None, new_mode: None };
    let logo = DiffFile { path: "logo.png".into(), old_path: None, status: FileStatus::Modified, added: 0, removed: 0, binary: true, old_size: Some(6), new_size: Some(8), old_mode: Some("100644".into()), new_mode: Some("100755".into()) };
    pinned(
        Reply::Diff { diff: DiffSummary { task: t(), run: Some(r()), root: "/Users/example/acme-api".into(), files: vec![file, logo], added: 1, removed: 0 } },
        r#"{"type":"diff","diff":{"task":"task_1","run":"run_1","root":"/Users/example/acme-api","files":[{"path":"docs/start.md","old_path":"docs/guide.md","status":"renamed","added":1,"removed":0},{"path":"logo.png","status":"modified","added":0,"removed":0,"binary":true,"old_size":6,"new_size":8,"old_mode":"100644","new_mode":"100755"}],"added":1,"removed":0}}"#,
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
        rules: "- Use British spelling.".into(),
        rules_hash: cats::rules_hash("- Use British spelling."),
        previous_rules: Some(String::new()),
        rules_saved_ms: Some(9),
        workspace: Some("ws_1".into()),
        created_ms: 1,
        retired_ms: None,
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
    pinned(ClientMessage { id: 8, request: Request::SaveCatRules { cat: c(), rules: "Rules\n".into() } }, r#"{"id":8,"type":"save_cat_rules","cat":"cat_miso","rules":"Rules\n"}"#);
    pinned(ClientMessage { id: 9, request: Request::RestoreCatRules { cat: c() } }, r#"{"id":9,"type":"restore_cat_rules","cat":"cat_miso"}"#);
    pinned(ClientMessage { id: 10, request: Request::RetireCat { cat: c() } }, r#"{"id":10,"type":"retire_cat","cat":"cat_miso"}"#);
    pinned(ClientMessage { id: 11, request: Request::BringBackCat { cat: c() } }, r#"{"id":11,"type":"bring_back_cat","cat":"cat_miso"}"#);
    pinned(ClientMessage { id: 12, request: Request::SetTaskCat { task: TaskId("task_1".into()), cat: c() } }, r#"{"id":12,"type":"set_task_cat","task":"task_1","cat":"cat_miso"}"#);
    pinned(
        ClientMessage { id: 13, request: Request::CreateTask { words: "Fix it".into(), workspace: None, cat: Some(c()) } },
        r#"{"id":13,"type":"create_task","words":"Fix it","cat":"cat_miso"}"#,
    );
    let j = r#"{"id":"cat_miso","name":"miso","face":"ginger","purpose":"Keeps the docs tidy.","rules":"- Use British spelling.","rules_hash":"HASH","previous_rules":"","rules_saved_ms":9,"workspace":"ws_1","created_ms":1}"#.replace("HASH", &cats::rules_hash("- Use British spelling."));
    pinned(Reply::Cat { cat: cat() }, &format!(r#"{{"type":"cat","cat":{j}}}"#));
    pinned(Reply::Cats { cats: vec![cat()] }, &format!(r#"{{"type":"cats","cats":[{j}]}}"#));
    pinned(ServerMessage::Event { revision: 3, change: Change::CatAdded { cat: cat() } }, &format!(r#"{{"kind":"event","revision":3,"change":{{"type":"cat_added","cat":{j}}}}}"#));
    let mut retired = cat();
    retired.retired_ms = Some(20);
    retired.purpose = None;
    retired.previous_rules = None;
    let encoded = serde_json::to_string(&Change::CatUpdated { cat: retired.clone() }).unwrap();
    assert!(encoded.starts_with(r#"{"type":"cat_updated","cat":{"id":"cat_miso","name":"miso","face":"ginger","rules":"#) && encoded.ends_with(r#""workspace":"ws_1","created_ms":1,"retired_ms":20}}"#), "{encoded}");
    assert!(!retired.is_active());
    pinned(ErrorCode::InvalidName, "\"invalid_name\"");
    // A run says which cat and which version of its rules went with it; a quiet part says when they changed.
    let mut r = run();
    r.cat = Some(c());
    r.rules_hash = Some("cbf29ce484222325".into());
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.ends_with(r#""cat":"cat_miso","rules_hash":"cbf29ce484222325"}"#), "{json}");
    pinned(PartBody::RulesChanged { cat: c(), name: "miso".into() }, r#"{"kind":"rules_changed","cat":"cat_miso","name":"miso"}"#);
    assert_eq!(PROTOCOL_VERSION, 12);
}

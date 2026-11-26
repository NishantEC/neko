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
    pinned(ClientMessage { id: 2, request: Request::CreateTask { words: "Fix the build".into(), folder: None } }, r#"{"id":2,"type":"create_task","words":"Fix the build"}"#);
    pinned(
        ClientMessage { id: 2, request: Request::CreateTask { words: "Fix the build".into(), folder: Some("/Users/example/acme-api".into()) } },
        r#"{"id":2,"type":"create_task","words":"Fix the build","folder":"/Users/example/acme-api"}"#,
    );
    pinned(ClientMessage { id: 3, request: Request::GetTask { task: TaskId("task_1".into()) } }, r#"{"id":3,"type":"get_task","task":"task_1"}"#);
    pinned(ClientMessage { id: 4, request: Request::Subscribe }, r#"{"id":4,"type":"subscribe"}"#);
}

#[test]
fn replies_and_events_round_trip() {
    pinned(
        ServerMessage::Reply { id: 0, reply: Reply::Hello { daemon: "0.1.0".into(), protocol: 1, workspace: "default".into() } },
        r#"{"kind":"reply","id":0,"reply":{"type":"hello","daemon":"0.1.0","protocol":1,"workspace":"default"}}"#,
    );
    let t = r#"{"id":"task_01HZX3J0000000000000000000","workspace":"default","words":"Rename the sidebar label\nand check ENG-123","title":"Rename the sidebar label","created_ms":1764000000000,"status":"new"}"#;
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
    ] {
        pinned(code, &format!("\"{name}\""));
    }
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
    pinned(ClientMessage { id: 6, request: Request::FollowUp { task: t(), words: "And the tests".into() } }, r#"{"id":6,"type":"follow_up","task":"task_1","words":"And the tests"}"#);
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
        usage: Some(Usage { input_tokens: 12, output_tokens: 34, cached_tokens: 56, cost_usd: Some(0.25), model: Some("claude-x".into()), pricing: Pricing::ApiEquivalent, reported_by: "Claude Code".into() }),
        changes: Some(Changes { files: 2, added: 10, removed: 3 }),
    }
}

#[test]
fn runs_parts_and_permissions_round_trip() {
    pinned(
        run(),
        r#"{"id":"run_1","task":"task_1","runtime":"claude","model":"claude-x","status":"done","started_ms":10,"ended_ms":20,"session":"5f0c","workdir":"/Users/example/acme-api","branch":"neko/abcd1234","usage":{"input_tokens":12,"output_tokens":34,"cached_tokens":56,"cost_usd":0.25,"model":"claude-x","pricing":"api_equivalent","reported_by":"Claude Code"},"changes":{"files":2,"added":10,"removed":3}}"#,
    );
    let part = |seq, body| Part { id: PartId(format!("part_{seq}")), task: TaskId("task_1".into()), run: Some(RunId("run_1".into())), seq, at_ms: 1, ended_ms: None, body };
    pinned(part(1, PartBody::Person { text: "Hi".into() }), r#"{"id":"part_1","task":"task_1","run":"run_1","seq":1,"at_ms":1,"body":{"kind":"person","text":"Hi"}}"#);
    pinned(part(2, PartBody::Agent { text: "**Done**".into(), done: true }), r#"{"id":"part_2","task":"task_1","run":"run_1","seq":2,"at_ms":1,"body":{"kind":"agent","text":"**Done**","done":true}}"#);
    pinned(part(3, PartBody::Thinking { text: "Hm".into(), done: false }), r#"{"id":"part_3","task":"task_1","run":"run_1","seq":3,"at_ms":1,"body":{"kind":"thinking","text":"Hm","done":false}}"#);
    pinned(
        part(4, PartBody::Tool { call: "t1".into(), name: "Read".into(), summary: "src/main.rs".into(), input: "{}".into(), output: Some("fn main".into()), state: ToolState::Done }),
        r#"{"id":"part_4","task":"task_1","run":"run_1","seq":4,"at_ms":1,"body":{"kind":"tool","call":"t1","name":"Read","summary":"src/main.rs","input":"{}","output":"fn main","state":"done"}}"#,
    );
    pinned(part(5, PartBody::Permission { permission: PermissionId("perm_1".into()) }), r#"{"id":"part_5","task":"task_1","run":"run_1","seq":5,"at_ms":1,"body":{"kind":"permission","permission":"perm_1"}}"#);
    pinned(
        part(6, PartBody::Notice { tone: Tone::Error, text: "Claude Code is not signed in.".into(), next: Some("Run claude".into()) }),
        r#"{"id":"part_6","task":"task_1","run":"run_1","seq":6,"at_ms":1,"body":{"kind":"notice","tone":"error","text":"Claude Code is not signed in.","next":"Run claude"}}"#,
    );
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
        RuntimeInfo { id: "codex".into(), name: "Codex".into(), found: false, version: None, signed_in: None, login: None, sign_in: "codex login".into(), install: "npm install -g @openai/codex".into(), supported: false },
        r#"{"id":"codex","name":"Codex","found":false,"sign_in":"codex login","install":"npm install -g @openai/codex","supported":false}"#,
    );
    pinned(UsageTotal { runs: 2, input_tokens: 3, output_tokens: 4, cost_usd: Some(0.5), api_equivalent: true }, r#"{"runs":2,"input_tokens":3,"output_tokens":4,"cost_usd":0.5,"api_equivalent":true}"#);
    pinned(ServerMessage::Reply { id: 3, reply: Reply::Ok }, r#"{"kind":"reply","id":3,"reply":{"type":"ok"}}"#);
}

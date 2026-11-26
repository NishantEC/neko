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
    pinned(ClientMessage { id: 2, request: Request::CreateTask { words: "Fix the build".into() } }, r#"{"id":2,"type":"create_task","words":"Fix the build"}"#);
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

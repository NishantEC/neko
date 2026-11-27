//! The shapes of the rounds messages are pinned (protocol 9): a change here is a change to the contract's version.

use neko_protocol::rounds::{RunEnd, RunKind};
use neko_protocol::schedule::Weekday;
use neko_protocol::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

fn pinned<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: T, json: &str) {
    let encoded = serde_json::to_string(&value).expect("encodes");
    assert_eq!(encoded, json, "the JSON shape of {value:?}");
    let back: T = serde_json::from_str(&encoded).expect("decodes");
    assert_eq!(back, value);
}

fn round() -> Round {
    Round {
        id: "round_01HZX3J0000000000000000000".into(),
        name: "Docs check".into(),
        words: "Check the docs for broken links".into(),
        cat: "cat_mochi".into(),
        workspace: "ws_1".into(),
        schedule: Schedule::Weekdays { days: vec![Weekday::Mon, Weekday::Fri], at: "09:00".into() },
        on: true,
        created_ms: 1_764_000_000_000,
        changed_ms: 1_764_000_000_001,
        status: RoundStatus::Scheduled,
        next_due_ms: Some(1_764_100_000_000),
        note: None,
        last: None,
    }
}

const ROUND_JSON: &str = r#"{"id":"round_01HZX3J0000000000000000000","name":"Docs check","words":"Check the docs for broken links","cat":"cat_mochi","workspace":"ws_1","schedule":{"kind":"weekdays","days":["mon","fri"],"at":"09:00"},"on":true,"created_ms":1764000000000,"changed_ms":1764000000001,"status":"scheduled","next_due_ms":1764100000000}"#;

#[test]
fn rounds_requests_are_pinned() {
    pinned(ClientMessage { id: 1, request: Request::ListRounds }, r#"{"id":1,"type":"list_rounds"}"#);
    pinned(
        ClientMessage { id: 2, request: Request::CreateRound { name: "Docs check".into(), words: "Check".into(), cat: "cat_mochi".into(), workspace: None, schedule: Schedule::Every { hours: 6 }, on: true } },
        r#"{"id":2,"type":"create_round","name":"Docs check","words":"Check","cat":"cat_mochi","schedule":{"kind":"every","hours":6}}"#,
    );
    pinned(
        ClientMessage { id: 3, request: Request::CreateRound { name: "A".into(), words: "B".into(), cat: "cat_miso".into(), workspace: Some("ws_1".into()), schedule: Schedule::Manual, on: false } },
        r#"{"id":3,"type":"create_round","name":"A","words":"B","cat":"cat_miso","workspace":"ws_1","schedule":{"kind":"manual"},"on":false}"#,
    );
    pinned(
        ClientMessage { id: 4, request: Request::UpdateRound { round: "round_1".into(), name: "A".into(), words: "B".into(), cat: "cat_miso".into(), workspace: None, schedule: Schedule::Daily { at: "07:15".into() } } },
        r#"{"id":4,"type":"update_round","round":"round_1","name":"A","words":"B","cat":"cat_miso","schedule":{"kind":"daily","at":"07:15"}}"#,
    );
    pinned(ClientMessage { id: 5, request: Request::SetRoundOn { round: "round_1".into(), on: false } }, r#"{"id":5,"type":"set_round_on","round":"round_1","on":false}"#);
    pinned(ClientMessage { id: 6, request: Request::RemoveRound { round: "round_1".into() } }, r#"{"id":6,"type":"remove_round","round":"round_1"}"#);
    pinned(ClientMessage { id: 7, request: Request::RunRoundNow { round: "round_1".into() } }, r#"{"id":7,"type":"run_round_now","round":"round_1"}"#);
    pinned(ClientMessage { id: 8, request: Request::SetRoundsPaused { paused: true } }, r#"{"id":8,"type":"set_rounds_paused","paused":true}"#);
    pinned(ClientMessage { id: 9, request: Request::RoundHistory { round: "round_1".into() } }, r#"{"id":9,"type":"round_history","round":"round_1"}"#);
    pinned(ClientMessage { id: 10, request: Request::DebugClock { offset_ms: Some(60_000) } }, r#"{"id":10,"type":"debug_clock","offset_ms":60000}"#);
    pinned(ClientMessage { id: 11, request: Request::DebugClock { offset_ms: None } }, r#"{"id":11,"type":"debug_clock"}"#);
}

#[test]
fn rounds_replies_and_events_are_pinned() {
    pinned(ServerMessage::Reply { id: 1, reply: Reply::Round { round: round() } }, &format!(r#"{{"kind":"reply","id":1,"reply":{{"type":"round","round":{ROUND_JSON}}}}}"#));
    pinned(ServerMessage::Reply { id: 1, reply: Reply::Rounds { rounds: vec![], paused: true } }, r#"{"kind":"reply","id":1,"reply":{"type":"rounds","rounds":[],"paused":true}}"#);
    let run = RoundRun { id: "rrun_1".into(), round: "round_1".into(), due_ms: Some(5), at_ms: 6, kind: RunKind::Skipped, task: None, reason: Some("Paused.".into()), end: None };
    pinned(
        ServerMessage::Reply { id: 1, reply: Reply::RoundRuns { runs: vec![run] } },
        r#"{"kind":"reply","id":1,"reply":{"type":"round_runs","runs":[{"id":"rrun_1","round":"round_1","due_ms":5,"at_ms":6,"kind":"skipped","reason":"Paused."}]}}"#,
    );
    let started = RoundRun { id: "rrun_2".into(), round: "round_1".into(), due_ms: None, at_ms: 6, kind: RunKind::Started, task: Some(TaskId("task_1".into())), reason: None, end: Some(RunEnd::Failed) };
    pinned(started, r#"{"id":"rrun_2","round":"round_1","at_ms":6,"kind":"started","task":"task_1","end":"failed"}"#);
    pinned(ServerMessage::Event { revision: 3, change: Change::RoundAdded { round: round() } }, &format!(r#"{{"kind":"event","revision":3,"change":{{"type":"round_added","round":{ROUND_JSON}}}}}"#));
    pinned(ServerMessage::Event { revision: 4, change: Change::RoundRemoved { round: "round_1".into() } }, r#"{"kind":"event","revision":4,"change":{"type":"round_removed","round":"round_1"}}"#);
    pinned(ServerMessage::Event { revision: 5, change: Change::RoundsPaused { paused: true } }, r#"{"kind":"event","revision":5,"change":{"type":"rounds_paused","paused":true}}"#);
    for (status, name) in [
        (RoundStatus::Manual, "manual"),
        (RoundStatus::Off, "off"),
        (RoundStatus::Braked, "braked"),
        (RoundStatus::Paused, "paused"),
        (RoundStatus::CatRetired, "cat_retired"),
        (RoundStatus::FolderMissing, "folder_missing"),
        (RoundStatus::WorkspaceRemoved, "workspace_removed"),
    ] {
        pinned(status, &format!("\"{name}\""));
    }
}

#[test]
fn a_task_made_by_a_round_says_so_and_one_made_by_a_person_does_not() {
    let mut t = Task {
        id: TaskId("task_1".into()),
        workspace: "default".into(),
        words: "w".into(),
        title: "t".into(),
        created_ms: 1,
        status: TaskStatus::New,
        folder: None,
        cat: Some("cat_miso".into()),
        in_folder: false,
        from_branch: None,
        round: None,
        last_read_ms: 0,
        activity_ms: 1,
        latest_line: String::new(),
        failure_line: String::new(),
        thread_status: ThreadStatus::Idle,
        needs_you: false,
        subagent: None,
    };
    assert!(!serde_json::to_string(&t).unwrap().contains("round"));
    t.round = Some(TaskRound { id: "round_1".into(), name: "Docs check".into(), due_ms: Some(1_764_000_000_000) });
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains(r#""round":{"id":"round_1","name":"Docs check","due_ms":1764000000000}"#), "{json}");
    assert_eq!(serde_json::from_str::<Task>(&json).unwrap(), t);
}

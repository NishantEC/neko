//! The shapes of the Briefing's messages are pinned (protocol 10): a change here is a change to the contract's version.

use neko_protocol::briefing::*;
use neko_protocol::undo::{Action, Offer};
use neko_protocol::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

fn pinned<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: T, json: &str) {
    let encoded = serde_json::to_string(&value).expect("encodes");
    assert_eq!(encoded, json, "the JSON shape of {value:?}");
    let back: T = serde_json::from_str(&encoded).expect("decodes");
    assert_eq!(back, value);
}

fn empty() -> Briefing {
    Briefing {
        from_ms: 1_764_000_000_000,
        to_ms: 1_764_086_400_000,
        first: true,
        read_ms: None,
        next_morning_ms: Some(1_764_100_000_000),
        needs: vec![],
        finished: vec![],
        rounds: vec![],
        rounds_ran: 0,
        rounds_not_run: 0,
        working: vec![],
        totals: BriefingTotals::default(),
    }
}

const EMPTY_JSON: &str = r#"{"from_ms":1764000000000,"to_ms":1764086400000,"first":true,"next_morning_ms":1764100000000,"needs":[],"finished":[],"rounds":[],"rounds_ran":0,"rounds_not_run":0,"working":[],"totals":{"finished":0,"files":0,"added":0,"removed":0}}"#;

#[test]
fn the_requests_are_pinned() {
    pinned(ClientMessage { id: 1, request: Request::GetBriefing { from_ms: None, to_ms: None } }, r#"{"id":1,"type":"get_briefing"}"#);
    pinned(ClientMessage { id: 2, request: Request::GetBriefing { from_ms: Some(5), to_ms: Some(9) } }, r#"{"id":2,"type":"get_briefing","from_ms":5,"to_ms":9}"#);
    pinned(ClientMessage { id: 3, request: Request::MarkBriefingRead }, r#"{"id":3,"type":"mark_briefing_read"}"#);
    pinned(ClientMessage { id: 4, request: Request::GetBriefingSettings }, r#"{"id":4,"type":"get_briefing_settings"}"#);
    pinned(
        ClientMessage { id: 5, request: Request::SetBriefingSettings { settings: BriefingSettings { morning_on: true, morning_at: "08:00".into() } } },
        r#"{"id":5,"type":"set_briefing_settings","settings":{"morning_on":true,"morning_at":"08:00"}}"#,
    );
}

#[test]
fn the_replies_and_events_are_pinned() {
    pinned(Reply::Briefing { briefing: empty() }, &format!(r#"{{"type":"briefing","briefing":{EMPTY_JSON}}}"#));
    pinned(Reply::BriefingSettings { settings: BriefingSettings::default() }, r#"{"type":"briefing_settings","settings":{"morning_on":true,"morning_at":"08:00"}}"#);
    pinned(ServerMessage::Event { revision: 4, change: Change::BriefingChanged { briefing: empty() } }, &format!(r#"{{"kind":"event","revision":4,"change":{{"type":"briefing_changed","briefing":{EMPTY_JSON}}}}}"#));
    pinned(
        ServerMessage::Event { revision: 4, change: Change::BriefingSettingsChanged { settings: BriefingSettings { morning_on: false, morning_at: "07:30".into() } } },
        r#"{"kind":"event","revision":4,"change":{"type":"briefing_settings_changed","settings":{"morning_on":false,"morning_at":"07:30"}}}"#,
    );
    let counts = BriefingCounts { needs_you: 1, finished: 3, working: 0, rounds_ran: 1, rounds_not_run: 2, news: true };
    pinned(
        ServerMessage::Event { revision: 4, change: Change::BriefingDue { counts } },
        r#"{"kind":"event","revision":4,"change":{"type":"briefing_due","counts":{"needs_you":1,"finished":3,"working":0,"rounds_ran":1,"rounds_not_run":2,"news":true}}}"#,
    );
}

#[test]
fn a_finished_task_and_a_round_line_are_pinned() {
    let task = BriefingTask {
        task: TaskId("task_1".into()),
        workspace: "default".into(),
        title: "Fix the build".into(),
        cat: "cat_miso".into(),
        cat_name: "miso".into(),
        ended_ms: 5,
        end: FinishedEnd::Done,
        answer: Some("Fixed it.".into()),
        changes: Some(Changes { files: 2, added: 10, removed: 1 }),
        branch: Some("neko/abc".into()),
        run: RunId("run_1".into()),
        offer: Offer { review: true, action: Some((Action::Undo, true)), line: None, undone: false },
    };
    pinned(
        task,
        r#"{"task":"task_1","workspace":"default","title":"Fix the build","cat":"cat_miso","cat_name":"miso","ended_ms":5,"end":"done","answer":"Fixed it.","changes":{"files":2,"added":10,"removed":1},"branch":"neko/abc","run":"run_1","offer":{"review":true,"action":["undo",true],"undone":false}}"#,
    );
    let line = BriefingRound {
        round: "round_1".into(),
        name: "Docs check".into(),
        workspace: "default".into(),
        cat: "cat_mochi".into(),
        cat_name: "mochi".into(),
        outcome: RoundOutcome::Skipped,
        at_ms: 7,
        due_ms: Some(6),
        task: None,
        task_title: None,
        task_status: None,
        reason: Some("The last run is still working.".into()),
    };
    pinned(
        line,
        r#"{"round":"round_1","name":"Docs check","workspace":"default","cat":"cat_mochi","cat_name":"mochi","outcome":"skipped","at_ms":7,"due_ms":6,"reason":"The last run is still working."}"#,
    );
    for (o, n) in [(RoundOutcome::Ran, "ran"), (RoundOutcome::Missed, "missed"), (RoundOutcome::Braked, "braked")] {
        pinned(o, &format!("\"{n}\""));
    }
    for (e, n) in [(FinishedEnd::Failed, "failed"), (FinishedEnd::Stopped, "stopped"), (FinishedEnd::Interrupted, "interrupted")] {
        pinned(e, &format!("\"{n}\""));
    }
}

#[test]
fn the_counts_and_the_lists_past_their_caps_add_up() {
    let mut b = empty();
    b.totals.finished = 53;
    b.rounds_ran = 3;
    b.rounds_not_run = 40;
    assert_eq!((b.finished_more(), b.rounds_more()), (53, 43));
    assert!(b.counts().news);
    assert!(b.same_content(&Briefing { to_ms: 99, ..b.clone() }), "the moment it was made is not content");
    assert!(!b.same_content(&Briefing { read_ms: Some(1), ..b.clone() }));
}

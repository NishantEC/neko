use neko_core::scheduled_plans as plans;
use neko_core::{Db, workbench as store};
use neko_protocol::workbench::{Command, Snapshot};
use neko_protocol::{
    scheduled_plans::{Schedule, ScheduleCommand},
    workbench::{TaskStatus, Workspace},
};

fn fixture() -> (Db, Snapshot, Schedule) {
    let db = Db::open_in_memory().unwrap();
    let mut state = Snapshot::default();
    state.workspaces.push(Workspace {
        id: "w".into(),
        name: "Repo".into(),
        repository: "/tmp/repo".into(),
        instructions: String::new(),
        away_enabled: true,
    });
    let schedule = Schedule {
        name: "Review".into(),
        prompt: "Review changes".into(),
        workspace_id: Some("w".into()),
        rule: "FREQ=HOURLY".into(),
        timezone: "UTC".into(),
        anchor_ms: 0,
        ..Default::default()
    };
    plans::apply(&mut state, ScheduleCommand::Save { schedule }).unwrap();
    let schedule = state.schedules[0].clone();
    (db, state, schedule)
}

#[test]
fn due_claim_is_atomic_durable_and_never_replayed_after_reload() {
    let (db, mut state, schedule) = fixture();
    plans::enable(
        &mut state,
        &schedule,
        plans::next_due(&schedule, 0).unwrap(),
    )
    .unwrap();
    let expected = state.schedules[0].clone();
    plans::claim(
        &mut state,
        &expected,
        plans::following_due(&expected, 7_200_000),
        7_200_000,
        false,
    )
    .unwrap();
    store::save(&db, &state).unwrap();
    let mut reloaded = store::load(&db).unwrap();
    assert_eq!(reloaded.tasks.len(), 1);
    assert_eq!(reloaded.tasks[0].status, TaskStatus::Queued);
    assert!(reloaded.tasks[0].supervision.is_none());
    assert_eq!(reloaded.schedules[0].next_due_ms, Some(10_800_000));
    assert!(
        plans::claim(
            &mut reloaded,
            &expected,
            Ok(Some(10_800_000)),
            7_200_000,
            false
        )
        .is_err()
    );
    assert_eq!(reloaded.tasks.len(), 1);
}

#[test]
fn overlap_suppresses_even_awaiting_approval_and_advances_due() {
    let (_, mut state, schedule) = fixture();
    plans::enable(&mut state, &schedule, 3_600_000).unwrap();
    let expected = state.schedules[0].clone();
    plans::claim(&mut state, &expected, Ok(Some(7_200_000)), 3_600_000, false).unwrap();
    state.tasks[0].status = TaskStatus::AwaitingApproval;
    let expected = state.schedules[0].clone();
    plans::claim(
        &mut state,
        &expected,
        Ok(Some(10_800_000)),
        7_200_000,
        false,
    )
    .unwrap();
    assert_eq!(state.tasks.len(), 1);
    assert_eq!(state.schedules[0].next_due_ms, Some(10_800_000));
    assert!(state.schedules[0].last_result.contains("previous"));
    assert!(plans::claim(&mut state, &expected, Ok(Some(10_800_000)), 7_200_000, true).is_err());
}

#[test]
fn invalid_scope_timezone_and_evaluation_error_fail_closed() {
    let (_, mut state, mut schedule) = fixture();
    schedule.timezone = "invalid".into();
    assert!(plans::next_due(&schedule, 0).is_err());
    schedule.timezone = "UTC".into();
    schedule.workspace_id = Some("missing".into());
    state.schedules[0] = schedule.clone();
    assert!(plans::enable(&mut state, &schedule, 3_600_000).is_err());
    schedule.workspace_id = Some("w".into());
    state.schedules[0] = schedule.clone();
    plans::enable(&mut state, &schedule, 3_600_000).unwrap();
    let expected = state.schedules[0].clone();
    plans::claim(
        &mut state,
        &expected,
        Err("Schedule evaluation failed".into()),
        3_600_000,
        false,
    )
    .unwrap();
    assert!(!state.schedules[0].enabled);
    assert!(state.tasks.is_empty());
}

#[test]
fn manual_run_of_paused_draft_requires_scope_but_not_cadence_and_remove_keeps_task() {
    let (_, mut state, mut schedule) = fixture();
    schedule.rule.clear();
    schedule.timezone.clear();
    state.schedules[0] = schedule.clone();
    plans::claim(&mut state, &schedule, Err("No rule".into()), 0, true).unwrap();
    assert!(!state.schedules[0].enabled);
    assert_eq!(state.tasks.len(), 1);
    let expected = state.schedules[0].clone();
    assert!(plans::claim(&mut state, &expected, Ok(Some(1)), 0, true).is_err());
    plans::apply(&mut state, ScheduleCommand::Remove { id: schedule.id }).unwrap();
    assert!(state.schedules.is_empty());
    assert_eq!(state.tasks.len(), 1);
}

#[test]
fn schedule_wire_save_creates_paused_draft_and_cannot_smuggle_authority() {
    let db = Db::open_in_memory().unwrap();
    let command =
        serde_json::from_value::<Command>(serde_json::json!({"Schedules":{"Save":{"schedule":{
            "id":"", "name":"Morning review", "prompt":"Review changes", "workspace_id":null,
            "rule":"", "timezone":"", "anchor_ms":0, "enabled":true, "last_task_id":"forged"
        }}}}));
    assert!(command.is_ok(), "schedule wire command must be available");
    let result = store::apply(&db, command.unwrap()).unwrap();
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["schedules"][0]["enabled"], false);
    assert_eq!(
        value["schedules"][0]["last_task_id"],
        serde_json::Value::Null
    );
    assert!(store::load(&db).unwrap().tasks.is_empty());
    assert_eq!(
        serde_json::to_value(store::load(&db).unwrap()).unwrap()["schedules"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn old_snapshot_without_schedules_still_reads() {
    let mut value = serde_json::to_value(Snapshot::default()).unwrap();
    value.as_object_mut().unwrap().remove("schedules");
    assert!(serde_json::from_value::<Snapshot>(value).is_ok());
}

#[test]
fn malformed_durable_schedule_and_oversized_fields_are_rejected() {
    let (db, mut state, _) = fixture();
    state.schedules[0].last_result = "x".repeat(8192);
    assert!(store::save(&db, &state).is_err());
    state.schedules[0].last_result.clear();
    state.schedules[0].enabled = true;
    assert!(
        store::save(&db, &state).is_err(),
        "enabled record needs a durable due time"
    );
}

#[test]
fn last_plan_outcome_is_saved_with_the_task_result() {
    let (db, mut state, schedule) = fixture();
    plans::claim(&mut state, &schedule, Err("No rule needed".into()), 0, true).unwrap();
    state.tasks[0].status = TaskStatus::Failed;
    state.tasks[0].result = "Model unavailable".into();
    store::save(&db, &state).unwrap();
    let state = store::load(&db).unwrap();
    assert!(state.schedules[0].last_result.contains("Model unavailable"));
}

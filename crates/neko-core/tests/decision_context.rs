use neko_core::decision_context::{self as context, HostObservation};
use neko_core::{Db, workbench};
use neko_protocol::decision_context::*;
use neko_protocol::workbench::{
    Command, Risk, Snapshot, SupervisorAction, SupervisorDecision, Task, TaskEvent, TaskStatus,
    Workspace,
};

fn fixture(db: &Db) -> Snapshot {
    let mut s = Snapshot::default();
    for id in ["w", "other"] {
        s.workspaces.push(Workspace {
            id: id.into(),
            name: id.into(),
            repository: format!("/repo/{id}"),
            instructions: String::new(),
            away_enabled: false,
        });
        s.tasks.push(Task {
            id: format!("task-{id}"),
            workspace_id: id.into(),
            issue_id: None,
            title: "Fix tests".into(),
            goal: "Focused tests pass".into(),
            status: TaskStatus::AwaitingApproval,
            plan: "Run focused tests".into(),
            result: String::new(),
            worktree: None,
            events: vec![],
            created_at_ms: 1,
            updated_at_ms: 1,
            source_revision: Some("revision-1".into()),
            supervision: None,
        });
    }
    workbench::save(db, &s).unwrap();
    workbench::load(db).unwrap()
}

fn save_preference(workspace_id: &str, records: Vec<String>) -> Command {
    Command::DecisionContext(DecisionContextCommand::SavePreference {
        workspace_id: workspace_id.into(),
        id: String::new(),
        expected_version: None,
        applicability: PreferenceApplicability {
            terms: vec!["tests".into()],
            task_ids: vec![],
        },
        instruction: "Run focused tests first".into(),
        supporting_record_ids: records,
        exceptions: vec!["documentation".into()],
    })
}

#[test]
fn confirmed_preference_survives_restart_without_starting_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let (id, original) = {
        let db = Db::open(&path).unwrap();
        let original = fixture(&db);
        let saved = workbench::apply(&db, save_preference("w", vec![])).unwrap();
        let p = &saved.working_preferences[0];
        assert_eq!(p.state, PreferenceState::Proposed);
        let kept = workbench::apply(
            &db,
            Command::DecisionContext(DecisionContextCommand::KeepPreference {
                workspace_id: "w".into(),
                id: p.id.clone(),
                expected_version: p.version,
            }),
        )
        .unwrap();
        assert_eq!(
            kept.working_preferences[0].state,
            PreferenceState::Confirmed
        );
        assert_eq!(kept.tasks, original.tasks);
        assert_eq!(kept.mcp, original.mcp);
        assert_eq!(kept.start_when_planned, original.start_when_planned);
        (p.id.clone(), original)
    };
    let db = Db::open(&path).unwrap();
    let restarted = workbench::recover_interrupted(&db).unwrap();
    assert_eq!(restarted.working_preferences[0].id, id);
    assert_eq!(restarted.working_preferences[0].version, 2);
    assert_eq!(restarted.tasks, original.tasks);
}

#[test]
fn corrupt_decision_storage_is_reported_and_preserved() {
    let db = Db::open_in_memory().unwrap();
    fixture(&db);
    db.set_setting("neko_decision_context_v1", "{broken")
        .unwrap();
    assert!(
        workbench::load(&db)
            .unwrap_err()
            .contains("decision context")
    );
    assert_eq!(
        db.get_setting("neko_decision_context_v1").unwrap().unwrap(),
        "{broken"
    );
}

#[test]
fn successful_commands_record_facts_without_inferred_cancellation_reason() {
    let db = Db::open_in_memory().unwrap();
    fixture(&db);
    let approved = workbench::apply(
        &db,
        Command::ApproveTask {
            task_id: "task-w".into(),
        },
    )
    .unwrap();
    assert_eq!(approved.decision_records.len(), 1);
    assert_eq!(
        approved.decision_records[0].action,
        DecisionAction::ApproveLocalBuild
    );
    let cancelled = workbench::apply(
        &db,
        Command::CancelTask {
            task_id: "task-w".into(),
        },
    )
    .unwrap();
    let record = cancelled.decision_records.last().unwrap();
    assert_eq!(record.action, DecisionAction::CancelTask);
    assert_eq!(record.rationale, "User cancelled the task; reason unknown.");
    assert_eq!(record.observed_outcome, None);
    assert_eq!(record.delivery_stage, DeliveryStage::Unknown);
    assert!(
        workbench::apply(
            &db,
            Command::CancelTask {
                task_id: "task-w".into()
            }
        )
        .is_err()
    );
    assert_eq!(workbench::load(&db).unwrap().decision_records.len(), 2);
}

fn keep(db: &Db, p: &WorkingPreference) -> Snapshot {
    workbench::apply(
        db,
        Command::DecisionContext(DecisionContextCommand::KeepPreference {
            workspace_id: p.workspace_id.clone(),
            id: p.id.clone(),
            expected_version: p.version,
        }),
    )
    .unwrap()
}

fn supervisor(action: SupervisorAction) -> SupervisorDecision {
    SupervisorDecision {
        action,
        risk: Risk::Low,
        is_bug: true,
        reason: "Need the target environment".into(),
        evidence: vec!["Actual source snapshot".into()],
        files: vec![],
        tests: vec![],
        sensitive_areas: vec![],
        uncertainties: vec![],
        plan: "Run focused tests".into(),
    }
}

#[test]
fn asking_or_skipping_needs_a_reply_and_replanning_before_approval() {
    for action in [SupervisorAction::AskUser, SupervisorAction::Skip] {
        let db = Db::open_in_memory().unwrap();
        let mut s = fixture(&db);
        s.tasks[0].supervision = Some(supervisor(action));
        workbench::save(&db, &s).unwrap();
        for cmd in [
            Command::ApproveTask {
                task_id: "task-w".into(),
            },
            Command::StartTask {
                task_id: "task-w".into(),
            },
        ] {
            assert!(workbench::apply(&db, cmd).unwrap_err().contains("reply"));
        }
        assert_eq!(
            workbench::load(&db).unwrap().tasks[0].status,
            TaskStatus::AwaitingApproval
        );
        assert!(workbench::load(&db).unwrap().decision_records.is_empty());
        let replied = workbench::apply(
            &db,
            Command::ReplyToTask {
                task_id: "task-w".into(),
                text: "Use staging".into(),
            },
        )
        .unwrap();
        assert_eq!(replied.tasks[0].status, TaskStatus::Queued);
        assert_eq!(replied.tasks[0].supervision, s.tasks[0].supervision,
            "factual context preserves scope until the worker interprets the reply");
        assert!(!replied.task_replies["task-w"].handled);
        assert!(
            replied.working_preferences.is_empty(),
            "a reply does not imply a preference"
        );
        assert_eq!(replied.decision_records[0].action, DecisionAction::AddNote);
        assert_eq!(replied.decision_records[0].rationale, "Use staging");
        assert!(
            workbench::apply(
                &db,
                Command::ApproveTask {
                    task_id: "task-w".into()
                }
            )
            .is_err()
        );
    }
}

#[test]
fn newest_supervisor_wait_marker_blocks_but_a_new_plan_clears_it() {
    for marker in [
        "Needs your input before building: choose environment",
        "Nothing to build: already fixed",
    ] {
        let db = Db::open_in_memory().unwrap();
        let mut s = fixture(&db);
        s.tasks[0].events.push(TaskEvent {
            at_ms: 1,
            role: "supervisor".into(),
            message: marker.into(),
        });
        s.tasks[0].events.push(TaskEvent {
            at_ms: 2,
            role: "note".into(),
            message: "Context only".into(),
        });
        workbench::save(&db, &s).unwrap();
        assert!(
            workbench::apply(
                &db,
                Command::ApproveTask {
                    task_id: "task-w".into()
                }
            )
            .is_err()
        );
        s.tasks[0].events.push(TaskEvent {
            at_ms: 3,
            role: "supervisor".into(),
            message: "Plan ready.".into(),
        });
        workbench::save(&db, &s).unwrap();
        assert_eq!(
            workbench::apply(
                &db,
                Command::ApproveTask {
                    task_id: "task-w".into()
                }
            )
            .unwrap()
            .tasks[0]
                .status,
            TaskStatus::Building
        );
    }
}

#[test]
fn guidance_requires_confirmation_context_scope_and_use_memory() {
    let db = Db::open_in_memory().unwrap();
    fixture(&db);
    let proposed = workbench::apply(&db, save_preference("w", vec![])).unwrap();
    assert!(context::context_about(&proposed, "w", Some("task-w"), "Fix tests").is_empty());
    let mut confirmed = keep(&db, &proposed.working_preferences[0]);
    assert!(
        context::context_about(&confirmed, "w", Some("task-w"), "Fix tests")
            .contains("focused tests first")
    );
    for (ws, task, about) in [
        ("other", Some("task-other"), "Fix tests"),
        ("w", Some("task-other"), "Fix tests"),
        ("w", None, "Fix detests"),
        ("w", Some("task-w"), "tests documentation"),
    ] {
        assert!(
            context::context_about(&confirmed, ws, task, about).is_empty(),
            "{ws}: {about}"
        );
    }
    confirmed.memory_options.use_memory = false;
    assert!(context::context_about(&confirmed, "w", Some("task-w"), "Fix tests").is_empty());
}

#[test]
fn edits_dismissal_and_forgetting_are_versioned_and_scope_checked() {
    let db = Db::open_in_memory().unwrap();
    fixture(&db);
    let proposed = workbench::apply(&db, save_preference("w", vec![])).unwrap();
    let confirmed = keep(&db, &proposed.working_preferences[0]);
    let p = &confirmed.working_preferences[0];
    assert!(
        workbench::apply(
            &db,
            Command::DecisionContext(DecisionContextCommand::KeepPreference {
                workspace_id: "other".into(),
                id: p.id.clone(),
                expected_version: p.version,
            })
        )
        .is_err()
    );
    let edit = |version| {
        Command::DecisionContext(DecisionContextCommand::SavePreference {
            workspace_id: "w".into(),
            id: p.id.clone(),
            expected_version: Some(version),
            applicability: p.applicability.clone(),
            instruction: "Run Rust tests first".into(),
            supporting_record_ids: vec![],
            exceptions: vec![],
        })
    };
    assert!(workbench::apply(&db, edit(1)).is_err());
    let edited = workbench::apply(&db, edit(2)).unwrap();
    assert_eq!(edited.working_preferences[0].version, 3);
    assert_eq!(
        edited.working_preferences[0].state,
        PreferenceState::Proposed
    );
    assert!(context::context_about(&edited, "w", Some("task-w"), "tests").is_empty());
    let dismissed = workbench::apply(
        &db,
        Command::DecisionContext(DecisionContextCommand::DismissPreference {
            workspace_id: "w".into(),
            id: p.id.clone(),
            expected_version: 3,
        }),
    )
    .unwrap();
    assert_eq!(
        dismissed.working_preferences[0].state,
        PreferenceState::Dismissed
    );
    assert_eq!(dismissed.working_preferences[0].version, 4);
    assert!(
        workbench::apply(
            &db,
            Command::DecisionContext(DecisionContextCommand::ForgetPreference {
                workspace_id: "w".into(),
                id: p.id.clone(),
                expected_version: 3,
            })
        )
        .is_err()
    );
    let forgotten = workbench::apply(
        &db,
        Command::DecisionContext(DecisionContextCommand::ForgetPreference {
            workspace_id: "w".into(),
            id: p.id.clone(),
            expected_version: 4,
        }),
    )
    .unwrap();
    assert!(forgotten.working_preferences.is_empty());
}

#[test]
fn host_records_survive_restart_and_are_idempotent_by_full_source_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let original = {
        let db = Db::open(&path).unwrap();
        let mut s = fixture(&db);
        s.tasks[0].plan = "界🙂".repeat(4000);
        workbench::save(&db, &s).unwrap();
        let first = context::record_task_observation(
            &db,
            &s,
            "task-w",
            HostObservation::Plan,
            &s.agent_runtime,
        )
        .unwrap();
        assert_eq!(first.rationale.len(), context::MAX_TEXT_BYTES - 1);
        assert!(first.rationale.ends_with('…'));
        assert_eq!(first.observed_outcome, None);
        assert_eq!(
            first.expected_outcome.as_deref(),
            Some("Focused tests pass")
        );
        assert_eq!(
            context::record_task_observation(
                &db,
                &s,
                "task-w",
                HostObservation::Plan,
                &s.agent_runtime
            )
            .unwrap(),
            first
        );
        // The visible prefix is unchanged, but the untruncated source changed.
        s.tasks[0].plan.push_str("different suffix");
        let second = context::record_task_observation(
            &db,
            &s,
            "task-w",
            HostObservation::Plan,
            &s.agent_runtime,
        )
        .unwrap();
        assert_ne!(first.source.fingerprint, second.source.fingerprint);
        first
    };
    let db = Db::open(&path).unwrap();
    let s = workbench::load(&db).unwrap();
    assert_eq!(s.decision_records.len(), 2);
    assert_eq!(s.decision_records[0], original);
    let stored: serde_json::Value =
        serde_json::from_str(&db.get_setting("workbench_snapshot_v1").unwrap().unwrap()).unwrap();
    assert_eq!(stored["decision_records"], serde_json::json!([]));
    assert_eq!(stored["working_preferences"], serde_json::json!([]));
}

#[test]
fn correction_appends_immutable_version_and_stale_or_cross_scope_edits_fail() {
    let db = Db::open_in_memory().unwrap();
    let s = fixture(&db);
    let original = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    let correction = |workspace_id: &str, record_id: &str, version| {
        Command::DecisionContext(DecisionContextCommand::CorrectDecision {
            workspace_id: workspace_id.into(),
            record_id: record_id.into(),
            expected_version: version,
            correction: "Do this for staging only".into(),
        })
    };
    assert!(workbench::apply(&db, correction("other", &original.id, 1)).is_err());
    assert!(workbench::apply(&db, correction("w", &original.id, 2)).is_err());
    let corrected = workbench::apply(&db, correction("w", &original.id, 1)).unwrap();
    assert_eq!(corrected.decision_records[0], original);
    let second = &corrected.decision_records[1];
    assert_ne!(second.id, original.id);
    assert_eq!(second.episode_id, original.episode_id);
    assert_eq!(second.version, 2);
    assert_eq!(
        second.supersedes_record_id.as_deref(),
        Some(original.id.as_str())
    );
    assert_eq!(second.rationale, original.rationale);
    assert_eq!(second.provenance, original.provenance);
    assert_eq!(second.expected_outcome, original.expected_outcome);
    assert_eq!(second.observed_outcome, original.observed_outcome);
    assert_eq!(corrected.working_preferences.len(), 1);
    let proposal = &corrected.working_preferences[0];
    assert_eq!(proposal.state, PreferenceState::Proposed);
    assert_eq!(proposal.applicability.task_ids, ["task-w"]);
    assert!(proposal.applicability.terms.is_empty());
    assert_eq!(proposal.supporting_record_ids, [second.id.clone()]);
    assert_eq!(proposal.instruction, "Do this for staging only");
    assert!(context::context_about(&corrected, "w", Some("task-w"), "tests").is_empty());
    assert!(workbench::apply(&db, correction("w", &original.id, 1)).is_err());
    let third = workbench::apply(&db, correction("w", &second.id, 2)).unwrap();
    assert_eq!(third.decision_records[2].version, 3);
}

#[test]
fn preference_references_cannot_cross_workspaces_and_applicability_is_required() {
    let db = Db::open_in_memory().unwrap();
    let s = fixture(&db);
    let record = context::record_task_observation(
        &db,
        &s,
        "task-other",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    assert!(workbench::apply(&db, save_preference("w", vec![record.id])).is_err());
    for applicability in [
        PreferenceApplicability::default(),
        PreferenceApplicability {
            terms: vec![],
            task_ids: vec!["task-other".into()],
        },
    ] {
        assert!(
            workbench::apply(
                &db,
                Command::DecisionContext(DecisionContextCommand::SavePreference {
                    workspace_id: "w".into(),
                    id: String::new(),
                    expected_version: None,
                    applicability,
                    instruction: "Run tests".into(),
                    supporting_record_ids: vec![],
                    exceptions: vec![],
                })
            )
            .is_err()
        );
    }
    assert!(workbench::load(&db).unwrap().working_preferences.is_empty());
}

#[test]
fn only_successful_user_local_acceptance_sets_delivery_stage() {
    let db = Db::open_in_memory().unwrap();
    let mut s = fixture(&db);
    assert!(
        context::record_task_observation(
            &db,
            &s,
            "task-w",
            HostObservation::Review,
            &s.agent_runtime
        )
        .is_err()
    );
    s.tasks[0].status = TaskStatus::ReadyForReview;
    s.tasks[0].result = "One check failed; inspect the patch".into();
    workbench::save(&db, &s).unwrap();
    let reviewed = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Review,
        &s.agent_runtime,
    )
    .unwrap();
    assert_ne!(reviewed.expected_outcome, reviewed.observed_outcome);
    assert_eq!(reviewed.delivery_stage, DeliveryStage::Unknown);
    assert_eq!(
        reviewed.observed_outcome.as_deref(),
        Some("Independent verification gate passed; useful/interaction outcome not observed")
    );
    let accepted = workbench::apply(
        &db,
        Command::CompleteTask {
            task_id: "task-w".into(),
        },
    )
    .unwrap();
    let record = accepted.decision_records.last().unwrap();
    assert_eq!(record.delivery_stage, DeliveryStage::LocalAccepted);
    assert_eq!(record.action, DecisionAction::AcceptLocalResult);
    assert_eq!(
        record.observed_outcome.as_deref(),
        Some("User accepted the local result; useful/interaction outcome not observed")
    );
    assert!(
        !record
            .observed_outcome
            .as_ref()
            .unwrap()
            .contains("One check failed")
    );
    assert_eq!(record.runtime.provider, "user");
    let deleted = workbench::apply(
        &db,
        Command::DeleteTask {
            task_id: "task-w".into(),
        },
    )
    .unwrap();
    assert_eq!(
        deleted.decision_records, accepted.decision_records,
        "history outlives deleted tickets"
    );
}

#[test]
fn explicit_correction_learning_is_gated_and_proposal_confirmation_has_no_authority() {
    for learning in [false, true] {
        let db = Db::open_in_memory().unwrap();
        fixture(&db);
        let mut s = workbench::apply(
            &db,
            Command::SetMemoryOptions {
                options: neko_protocol::workbench::MemoryOptions {
                    learning,
                    use_memory: true,
                },
            },
        )
        .unwrap();
        s.tasks[0].status = TaskStatus::Cancelled;
        workbench::save(&db, &s).unwrap();
        let s = workbench::apply(
            &db,
            Command::AddTicketNote {
                task_id: "task-w".into(),
                text: "Scope changed".into(),
            },
        )
        .unwrap();
        assert!(
            s.working_preferences.is_empty(),
            "notes don't infer preferences"
        );
        let original = &s.decision_records[0];
        let corrected = workbench::apply(
            &db,
            Command::DecisionContext(DecisionContextCommand::CorrectDecision {
                workspace_id: "w".into(),
                record_id: original.id.clone(),
                expected_version: 1,
                correction: "Keep this task focused on Rust".into(),
            }),
        )
        .unwrap();
        assert_eq!(corrected.working_preferences.len(), usize::from(learning));
        assert_eq!(corrected.tasks, s.tasks);
        assert_eq!(corrected.mcp, s.mcp);
        if learning {
            let confirmed = keep(&db, &corrected.working_preferences[0]);
            assert_eq!(confirmed.tasks[0].status, TaskStatus::Cancelled);
            assert_eq!(confirmed.mcp, s.mcp);
            assert_eq!(confirmed.start_without_approval, s.start_without_approval);
            assert_eq!(confirmed.start_when_planned, s.start_when_planned);
            assert!(
                context::context_about(&confirmed, "w", Some("task-w"), "tests")
                    .contains("focused on Rust")
            );
            assert!(
                context::context_about(&confirmed, "w", None, "tests").is_empty(),
                "a task correction is never global"
            );
        }
    }
}

#[test]
fn decision_time_versions_and_runtime_survive_edits_while_worker_runs() {
    let db = Db::open_in_memory().unwrap();
    fixture(&db);
    let proposed = workbench::apply(&db, save_preference("w", vec![])).unwrap();
    let mut decision_time = keep(&db, &proposed.working_preferences[0]);
    decision_time.agent_runtime.provider = "codex".into();
    decision_time.agent_runtime.model = "actual-model".into();
    let p = &decision_time.working_preferences[0];
    let edited = workbench::apply(
        &db,
        Command::DecisionContext(DecisionContextCommand::SavePreference {
            workspace_id: "w".into(),
            id: p.id.clone(),
            expected_version: Some(p.version),
            applicability: p.applicability.clone(),
            instruction: "Changed mid-run".into(),
            supporting_record_ids: vec![],
            exceptions: vec![],
        }),
    )
    .unwrap();
    let mut result = keep(&db, &edited.working_preferences[0]);
    result.agent_runtime.model = "later-model".into();
    result.tasks[0].status = TaskStatus::ReadyForReview;
    workbench::save(&db, &result).unwrap();
    let observed = context::record_task_observation_with_context(
        &db,
        &result,
        "task-w",
        HostObservation::Review,
        &decision_time,
    )
    .unwrap();
    assert_eq!(
        observed.preference_versions,
        vec![PreferenceVersion {
            id: p.id.clone(),
            version: 2
        }]
    );
    assert_eq!(observed.runtime, decision_time.agent_runtime);
    let mut no_memory = decision_time.clone();
    no_memory.memory_options.use_memory = false;
    let unassisted = context::record_task_observation_with_context(
        &db,
        &result,
        "task-w",
        HostObservation::Review,
        &no_memory,
    )
    .unwrap();
    assert!(unassisted.preference_versions.is_empty());
}

#[test]
fn prompt_budget_and_logged_versions_include_exactly_the_delivered_guidance() {
    let db = Db::open_in_memory().unwrap();
    fixture(&db);
    for i in 0..4 {
        let saved = workbench::apply(
            &db,
            Command::DecisionContext(DecisionContextCommand::SavePreference {
                workspace_id: "w".into(),
                id: String::new(),
                expected_version: None,
                applicability: PreferenceApplicability {
                    terms: vec!["tests".into()],
                    task_ids: vec![],
                },
                instruction: format!("{i}: {}", "界".repeat(550)),
                supporting_record_ids: vec![],
                exceptions: vec![],
            }),
        )
        .unwrap();
        keep(&db, saved.working_preferences.last().unwrap());
    }
    let s = workbench::load(&db).unwrap();
    let prompt = context::context_about(&s, "w", Some("task-w"), "Fix tests Focused tests pass");
    assert!(prompt.len() <= context::PROMPT_BUDGET);
    let observed = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    assert_eq!(observed.preference_versions.len(), 2);
    for p in &s.working_preferences {
        assert_eq!(
            prompt.contains(&p.id),
            observed.preference_versions.iter().any(|v| v.id == p.id)
        );
    }
}

#[test]
fn review_and_failure_record_host_gates_and_scout_questions_record_ask_user() {
    let db = Db::open_in_memory().unwrap();
    let mut s = fixture(&db);
    s.tasks[0].events.push(TaskEvent {
        at_ms: 1,
        role: "supervisor".into(),
        message: "Needs your input before building: which environment?".into(),
    });
    let ask = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    assert_eq!(ask.action, DecisionAction::AskUser);
    s.tasks[0].status = TaskStatus::Building;
    s.tasks[0].events.push(TaskEvent {
        at_ms: 2,
        role: "supervisor".into(),
        message: "Plan ready.".into(),
    });
    let plan = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    assert_eq!(plan.action, DecisionAction::ProposePlan);
    s.tasks[0].status = TaskStatus::Failed;
    s.tasks[0].result = "Builder claims everything passed".into();
    s.tasks[0].events.push(TaskEvent {
        at_ms: 3,
        role: "supervisor".into(),
        message: "Required check exited 1".into(),
    });
    s.tasks[0].events.push(TaskEvent {
        at_ms: 4,
        role: "builder".into(),
        message: "Actually it was fine".into(),
    });
    let failure = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Failure,
        &s.agent_runtime,
    )
    .unwrap();
    assert_eq!(
        failure.observed_outcome.as_deref(),
        Some("Task failed: Required check exited 1")
    );
    assert_eq!(failure.delivery_stage, DeliveryStage::Unknown);
    assert!(
        !failure
            .observed_outcome
            .unwrap()
            .contains("everything passed")
    );
}

#[test]
fn count_and_serialized_byte_limits_bound_history_with_unicode_and_escaping() {
    let db = Db::open_in_memory().unwrap();
    let mut s = fixture(&db);
    s.tasks[0].supervision = Some(supervisor(SupervisorAction::PrepareFix));
    s.tasks[0].supervision.as_mut().unwrap().evidence = (0..16)
        .map(|i| format!("{i}{}", "\u{0001}".repeat(512)))
        .collect();
    s.tasks[0].plan = "界🙂".repeat(4000);
    for i in 0..(context::MAX_RECORDS + 4) {
        s.tasks[0].source_revision = Some(format!("revision-{i}"));
        let observed = context::record_task_observation(
            &db,
            &s,
            "task-w",
            HostObservation::Plan,
            &s.agent_runtime,
        )
        .unwrap();
        assert!(serde_json::to_vec(&observed).unwrap().len() <= context::MAX_RECORD_BYTES);
    }
    let json = db.get_setting("neko_decision_context_v1").unwrap().unwrap();
    assert!(json.len() <= context::MAX_STORAGE_BYTES);
    let persisted = workbench::load(&db).unwrap();
    assert!(!persisted.decision_records.is_empty());
    assert!(persisted.decision_records.len() <= context::MAX_RECORDS);
    assert_eq!(
        persisted
            .decision_records
            .last()
            .unwrap()
            .source
            .revision
            .as_deref(),
        Some("revision-131")
    );
}

#[test]
fn preference_capacity_does_not_grow_and_failed_correction_is_atomic() {
    let db = Db::open_in_memory().unwrap();
    let s = fixture(&db);
    let observed = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    for _ in 0..context::MAX_PREFERENCES {
        workbench::apply(&db, save_preference("w", vec![])).unwrap();
    }
    assert!(workbench::apply(&db, save_preference("w", vec![])).is_err());
    let before = db.get_setting("neko_decision_context_v1").unwrap().unwrap();
    assert!(
        workbench::apply(
            &db,
            Command::DecisionContext(DecisionContextCommand::CorrectDecision {
                workspace_id: "w".into(),
                record_id: observed.id,
                expected_version: 1,
                correction: "Explicit correction".into(),
            })
        )
        .is_err()
    );
    assert_eq!(
        db.get_setting("neko_decision_context_v1").unwrap().unwrap(),
        before
    );
}

#[test]
fn structurally_corrupt_cross_workspace_and_duplicate_records_fail_closed() {
    let db = Db::open_in_memory().unwrap();
    let s = fixture(&db);
    context::record_task_observation(&db, &s, "task-w", HostObservation::Plan, &s.agent_runtime)
        .unwrap();
    let valid = db.get_setting("neko_decision_context_v1").unwrap().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&valid).unwrap();
    value["decision_records"][0]["task_id"] = serde_json::json!("task-other");
    db.set_setting("neko_decision_context_v1", &value.to_string())
        .unwrap();
    assert!(
        workbench::load(&db)
            .unwrap_err()
            .contains("another workspace")
    );
    let mut value: serde_json::Value = serde_json::from_str(&valid).unwrap();
    let duplicate = value["decision_records"][0].clone();
    value["decision_records"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    db.set_setting("neko_decision_context_v1", &value.to_string())
        .unwrap();
    assert!(workbench::load(&db).unwrap_err().contains("duplicate"));
}

#[test]
fn maximum_escaped_correction_fits_without_rewriting_host_observation() {
    let db = Db::open_in_memory().unwrap();
    let mut s = fixture(&db);
    s.tasks[0].supervision = Some(supervisor(SupervisorAction::PrepareFix));
    s.tasks[0].supervision.as_mut().unwrap().evidence = (0..16)
        .map(|i| format!("{i}{}", "\u{0001}".repeat(512)))
        .collect();
    s.tasks[0].plan = "\u{0001}".repeat(20_000);
    let original = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    let result = workbench::apply(
        &db,
        Command::DecisionContext(DecisionContextCommand::CorrectDecision {
            workspace_id: "w".into(),
            record_id: original.id.clone(),
            expected_version: 1,
            correction: "\u{0001}".repeat(context::MAX_TEXT_BYTES),
        }),
    );
    assert!(
        result.is_ok(),
        "bounded correction needs reserved byte space: {result:?}"
    );
    let corrected = result.unwrap();
    assert_eq!(corrected.decision_records[0], original);
    assert_eq!(corrected.decision_records[1].rationale, original.rationale);
}

fn source_fixture(db: &Db) -> Snapshot {
    use neko_protocol::mcp_host::*;
    let mut s = fixture(db);
    s.mcp.connections.push(McpConnection {
        id: "connection".into(),
        workspace_id: "w".into(),
        label: "User source".into(),
        config: ServerConfig::Http {
            url: "https://example.com/mcp".into(),
        },
        enabled: true,
        trusted: true,
        has_credentials: false,
        oauth: false,
        source_link: None,
        tools: vec![McpTool {
            name: "read".into(),
            description: String::new(),
            input_schema: "{}".into(),
            schema_hash: "schema-1".into(),
            read_only: true,
        }],
        discovered_ms: Some(1),
        error: None,
    });
    s.mcp.responsibilities.push(Responsibility {
        id: "responsibility".into(),
        workspace_id: "w".into(),
        instruction: "Read source".into(),
        connection_ids: vec!["connection".into()],
        enabled: true,
        prepare_low_risk: false,
        next_due_ms: 1,
        last_attempt_ms: None,
        last_result: String::new(),
        failures: 0,
    });
    s.mcp.grants.push(ToolGrant {
        connection_id: "connection".into(),
        workspace_id: "w".into(),
        tool_name: "read".into(),
        schema_hash: "schema-1".into(),
    });
    s.mcp.receipts.push(ToolReceipt {
        id: "receipt".into(),
        run_id: "run".into(),
        workspace_id: "w".into(),
        connection_id: "connection".into(),
        tool_name: "read".into(),
        schema_hash: "schema-1".into(),
        at_ms: 1,
        success: true,
    });
    s.mcp.sources.push(SourceEvidence {
        id: "source".into(),
        task_id: Some("task-w".into()),
        responsibility_id: "responsibility".into(),
        connection_id: "connection".into(),
        external_id: "source-external".into(),
        revision: "source-revision-1".into(),
        title: "Test failure".into(),
        description: "Actual source snapshot".into(),
        retrieved_ms: 1,
        receipt_ids: vec!["receipt".into()],
        eligible: true,
    });
    workbench::save(&db, &s).unwrap();
    s
}

#[test]
fn source_receipts_and_current_grants_are_validated_in_result_scope() {
    let db = Db::open_in_memory().unwrap();
    let s = source_fixture(&db);
    let observed =
        context::record_task_observation_with_context(&db, &s, "task-w", HostObservation::Plan, &s)
            .unwrap();
    assert_eq!(
        observed.source.evidence[0].description,
        "Actual source snapshot"
    );
    assert_eq!(observed.source.evidence[0].receipt_ids, ["receipt"]);
    assert_eq!(observed.scope.connection_ids, ["connection"]);
    let mut bad = s.clone();
    bad.mcp.sources[0].receipt_ids.insert(0, "pruned-receipt".into());
    bad.mcp.receipts[0].workspace_id = "other".into();
    assert!(
        context::record_task_observation_with_context(
            &db,
            &bad,
            "task-w",
            HostObservation::Plan,
            &s
        )
        .is_err()
    );
    let mut bad = s.clone();
    bad.mcp.grants.clear();
    assert!(
        context::record_task_observation_with_context(
            &db,
            &bad,
            "task-w",
            HostObservation::Plan,
            &s
        )
        .is_err(),
        "old prompt grants cannot substitute for current source validation"
    );
    let mut bad = s.clone();
    bad.mcp.connections[0].tools[0].schema_hash = "schema-2".into();
    assert!(
        context::record_task_observation_with_context(
            &db,
            &bad,
            "task-w",
            HostObservation::Plan,
            &s
        )
        .is_err()
    );
    // Frozen observations survive refresh, reassignment, revocation and removal.
    let history = workbench::load(&db).unwrap().decision_records;
    let mut refreshed = s.clone();
    refreshed.mcp.sources[0].task_id = Some("task-other".into());
    refreshed.mcp.sources[0].revision = "source-revision-2".into();
    workbench::save(&db, &refreshed).unwrap();
    assert_eq!(workbench::load(&db).unwrap().decision_records, history);
    refreshed.mcp.connections[0].workspace_id = "other".into();
    refreshed.mcp.responsibilities[0].workspace_id = "other".into();
    refreshed.mcp.grants.clear();
    workbench::save(&db, &refreshed).unwrap();
    assert_eq!(workbench::load(&db).unwrap().decision_records, history);
    let mut wrong_scope = refreshed.clone();
    wrong_scope.mcp.sources[0].task_id = Some("task-w".into());
    assert!(
        context::record_task_observation_with_context(
            &db,
            &wrong_scope,
            "task-w",
            HostObservation::Plan,
            &s
        )
        .is_err()
    );
    refreshed.mcp.connections.clear();
    refreshed.mcp.responsibilities.clear();
    refreshed.mcp.receipts.clear();
    refreshed.mcp.sources.clear();
    workbench::save(&db, &refreshed).unwrap();
    assert_eq!(workbench::load(&db).unwrap().decision_records, history);
}

#[test]
fn pruned_source_receipts_do_not_block_replies_or_claim_fresh_evidence() {
    let db = Db::open_in_memory().unwrap();
    let mut s = source_fixture(&db);
    s.mcp.receipts.clear(); // Bounded receipt history can expire before a ticket.
    workbench::save(&db, &s).unwrap();
    let replied = workbench::apply(&db, Command::ReplyToTask {
        task_id: "task-w".into(), text: "Investigate and fix it".into(),
    }).unwrap();
    assert_eq!(replied.tasks[0].status, TaskStatus::Queued);
    assert_eq!(replied.task_replies["task-w"].pending, ["Investigate and fix it"]);
    assert!(!replied.start_when_planned.contains("task-w"));
    let note = replied.decision_records.last().unwrap();
    assert_eq!(note.provenance, DecisionProvenance::UserCommand);
    assert!(note.source.evidence.is_empty());
    assert!(note.rationale.contains("Source evidence unavailable"));
    assert!(note.scope.connection_ids.is_empty());

    let mut planned = replied;
    planned.tasks[0].status = TaskStatus::AwaitingApproval;
    let record = context::record_task_observation_with_context(
        &db, &planned, "task-w", HostObservation::Plan, &planned,
    ).unwrap();
    assert!(record.source.evidence.is_empty());
    assert!(record.rationale.contains("no longer retained"));
    assert!(planned.mcp.receipts.is_empty(), "history never fabricates receipts");
}

#[test]
fn missing_responsibilities_and_evicted_receipts_never_block_cancellation_or_failure() {
    for missing_responsibility in [true, false] {
        let db = Db::open_in_memory().unwrap();
        let mut s = source_fixture(&db);
        if missing_responsibility {
            s.mcp.responsibilities.clear();
        } else {
            s.mcp.receipts.clear();
        }
        workbench::save(&db, &s).unwrap();
        let stopped = workbench::apply(
            &db,
            Command::CancelTask {
                task_id: "task-w".into(),
            },
        )
        .unwrap();
        assert_eq!(stopped.tasks[0].status, TaskStatus::Cancelled);
        let record = stopped.decision_records.last().unwrap();
        assert!(record.source.evidence.is_empty());
        assert!(record.rationale.contains("Source evidence unavailable"));
        assert!(record.rationale.contains("reason unknown"));
        let mut failed = s;
        failed.tasks[0].status = TaskStatus::Failed;
        failed.tasks[0].events.push(TaskEvent {
            at_ms: 2,
            role: "supervisor".into(),
            message: "Worker stopped before starting".into(),
        });
        workbench::save(&db, &failed).unwrap();
        let host = neko_protocol::workbench::AgentRuntime {
            provider: "host".into(),
            model: String::new(), ..Default::default()
        };
        let observed = context::record_task_observation(
            &db,
            &failed,
            "task-w",
            HostObservation::Failure,
            &host,
        )
        .unwrap();
        assert!(observed.source.evidence.is_empty());
        assert!(observed.rationale.contains("Source evidence unavailable"));
        assert_eq!(
            workbench::load(&db).unwrap().tasks[0].status,
            TaskStatus::Failed
        );
    }
}

#[test]
fn historical_preference_versions_survive_a_profile_change_and_preference_edit() {
    let db = Db::open_in_memory().unwrap();
    fixture(&db);
    let proposed = workbench::apply(&db, save_preference("w", vec![])).unwrap();
    let original = keep(&db, &proposed.working_preferences[0]);
    let record = context::record_task_observation(
        &db,
        &original,
        "task-w",
        HostObservation::Plan,
        &original.agent_runtime,
    )
    .unwrap();
    let mut updated = original.clone();
    updated
        .agent_profiles
        .profiles
        .push(neko_protocol::agent_profiles::AgentProfile {
            id: "new-agent".into(),
            name: "New agent".into(),
            instructions: String::new(),
        });
    updated
        .agent_profiles
        .assignments
        .push(neko_protocol::agent_profiles::WorkspaceAssignment {
            workspace_id: "w".into(),
            profile_id: "new-agent".into(),
        });
    updated.agent_profiles.revision += 1;
    workbench::save(&db, &updated).unwrap();
    assert!(
        context::context_about(&workbench::load(&db).unwrap(), "w", Some("task-w"), "tests")
            .is_empty()
    );
    let p = &original.working_preferences[0];
    let edited = workbench::apply(
        &db,
        Command::DecisionContext(DecisionContextCommand::SavePreference {
            workspace_id: "w".into(),
            id: p.id.clone(),
            expected_version: Some(p.version),
            applicability: p.applicability.clone(),
            instruction: "Updated preference".into(),
            supporting_record_ids: vec![],
            exceptions: vec![],
        }),
    )
    .unwrap();
    assert_eq!(edited.working_preferences[0].agent_profile_id, "new-agent");
    assert_eq!(edited.decision_records[0], record);
}

#[test]
fn user_and_host_actions_do_not_claim_to_have_consulted_model_preferences() {
    let db = Db::open_in_memory().unwrap();
    fixture(&db);
    let proposed = workbench::apply(&db, save_preference("w", vec![])).unwrap();
    keep(&db, &proposed.working_preferences[0]);
    let approved = workbench::apply(
        &db,
        Command::ApproveTask {
            task_id: "task-w".into(),
        },
    )
    .unwrap();
    assert_eq!(approved.decision_records[0].runtime.provider, "user");
    assert!(approved.decision_records[0].preference_versions.is_empty());
    let cancelled = workbench::apply(
        &db,
        Command::CancelTask {
            task_id: "task-w".into(),
        },
    )
    .unwrap();
    assert!(
        cancelled
            .decision_records
            .last()
            .unwrap()
            .preference_versions
            .is_empty()
    );
    let mut failed = cancelled;
    failed.tasks[0].status = TaskStatus::Failed;
    failed.tasks[0].events.push(TaskEvent {
        at_ms: 2,
        role: "supervisor".into(),
        message: "Host could not start worker".into(),
    });
    let runtime = neko_protocol::workbench::AgentRuntime {
        provider: "host".into(),
        model: String::new(), ..Default::default()
    };
    let observation = context::record_task_observation(
        &db,
        &failed,
        "task-w",
        HostObservation::Failure,
        &runtime,
    )
    .unwrap();
    assert_eq!(observation.runtime.provider, "host");
    assert!(observation.preference_versions.is_empty());
}

#[test]
fn duplicate_scoped_source_receipts_capture_once_without_mutating_the_source() {
    let db = Db::open_in_memory().unwrap();
    let mut s = source_fixture(&db);
    s.mcp.sources[0].receipt_ids = vec!["receipt".into(), "receipt".into()];
    workbench::save(&db, &s).unwrap();
    let s = workbench::load(&db).unwrap();
    let record = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    assert_eq!(record.source.evidence[0].receipt_ids, ["receipt"]);
    assert_eq!(
        record.source.evidence[0].description,
        "Actual source snapshot"
    );
    assert_eq!(workbench::load(&db).unwrap().mcp, s.mcp);
    let repeated = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    assert_eq!(record, repeated);
    let mut invalid = s;
    invalid.mcp.sources[0]
        .receipt_ids
        .push("missing-receipt".into());
    let incomplete = context::record_task_observation(
            &db,
            &invalid,
            "task-w",
            HostObservation::Plan,
            &invalid.agent_runtime
        ).unwrap();
    assert!(incomplete.source.evidence.is_empty(), "partial receipts cannot prove the source");
    assert!(incomplete.rationale.contains("Source evidence unavailable"));
}

#[test]
fn duplicate_scoped_source_receipts_never_roll_back_cancellation() {
    let db = Db::open_in_memory().unwrap();
    let mut s = source_fixture(&db);
    s.mcp.sources[0].receipt_ids = vec!["receipt".into(), "receipt".into()];
    workbench::save(&db, &s).unwrap();
    let cancelled = workbench::apply(
        &db,
        Command::CancelTask {
            task_id: "task-w".into(),
        },
    )
    .unwrap();
    assert_eq!(cancelled.tasks[0].status, TaskStatus::Cancelled);
    assert_eq!(
        cancelled.decision_records[0].source.evidence[0].receipt_ids,
        ["receipt"]
    );
    assert_eq!(
        cancelled.decision_records[0].rationale,
        "User cancelled the task; reason unknown."
    );
    assert_eq!(
        workbench::load(&db).unwrap().tasks[0].status,
        TaskStatus::Cancelled
    );
    assert_eq!(
        cancelled.mcp, s.mcp,
        "normalization doesn't change the retained intake or grants"
    );
}

#[test]
fn duplicate_scoped_receipts_survive_atomic_failure_and_restart_recovery() {
    for restart in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        {
            let db = Db::open(&path).unwrap();
            let mut s = source_fixture(&db);
            s.mcp.sources[0].receipt_ids = vec!["receipt".into(), "receipt".into()];
            s.tasks[0].status = TaskStatus::Building;
            workbench::save(&db, &s).unwrap();
            if restart {
                for _ in 0..2 {
                    assert_eq!(
                        workbench::recover_interrupted(&db).unwrap().tasks[0].status,
                        TaskStatus::Building
                    );
                }
            }
        }
        let db = Db::open(&path).unwrap();
        let record = db
            .atomic(|| {
                let failed = if restart {
                    workbench::recover_interrupted(&db)?
                } else {
                    let mut s = workbench::load(&db)?;
                    s.tasks[0].status = TaskStatus::Failed;
                    workbench::append_event(
                        &mut s.tasks[0],
                        "supervisor",
                        "Worker exited before completion",
                    );
                    workbench::save(&db, &s)?;
                    s
                };
                assert_eq!(failed.tasks[0].status, TaskStatus::Failed);
                let host = neko_protocol::workbench::AgentRuntime {
                    provider: "host".into(),
                    model: String::new(), ..Default::default()
                };
                context::record_task_observation(
                    &db,
                    &failed,
                    "task-w",
                    HostObservation::Failure,
                    &host,
                )
            })
            .unwrap();
        assert_eq!(record.action, DecisionAction::ReportFailure);
        assert_eq!(record.source.evidence[0].receipt_ids, ["receipt"]);
        let persisted = workbench::load(&db).unwrap();
        assert_eq!(persisted.tasks[0].status, TaskStatus::Failed);
        assert_eq!(persisted.mcp.sources[0].receipt_ids, ["receipt", "receipt"]);
        drop(db);
        assert_eq!(
            workbench::load(&Db::open(&path).unwrap())
                .unwrap()
                .decision_records,
            [record]
        );
    }
}

fn edit_retained_preference(p: &WorkingPreference) -> Command {
    Command::DecisionContext(DecisionContextCommand::SavePreference {
        workspace_id: p.workspace_id.clone(),
        id: p.id.clone(),
        expected_version: Some(p.version),
        applicability: p.applicability.clone(),
        instruction: "Updated guidance after history was pruned".into(),
        supporting_record_ids: p.supporting_record_ids.clone(),
        exceptions: p.exceptions.clone(),
    })
}

#[test]
fn retained_preference_edit_survives_fifo_eviction_and_ticket_deletion() {
    let db = Db::open_in_memory().unwrap();
    let mut s = fixture(&db);
    let original = context::record_task_observation(
        &db,
        &s,
        "task-w",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    let proposed = workbench::apply(
        &db,
        Command::DecisionContext(DecisionContextCommand::SavePreference {
            workspace_id: "w".into(),
            id: String::new(),
            expected_version: None,
            applicability: PreferenceApplicability {
                terms: vec![],
                task_ids: vec!["task-w".into()],
            },
            instruction: "Keep this task focused".into(),
            supporting_record_ids: vec![original.id.clone()],
            exceptions: vec![],
        }),
    )
    .unwrap();
    let confirmed = keep(&db, &proposed.working_preferences[0]);
    let p = confirmed.working_preferences[0].clone();
    for i in 0..context::MAX_RECORDS {
        s.tasks[1].source_revision = Some(format!("eviction-{i}"));
        context::record_task_observation(
            &db,
            &s,
            "task-other",
            HostObservation::Plan,
            &s.agent_runtime,
        )
        .unwrap();
    }
    let mut s = workbench::load(&db).unwrap();
    assert!(
        !s.decision_records.iter().any(|r| r.id == original.id),
        "use real FIFO eviction"
    );
    s.tasks[0].status = TaskStatus::Cancelled;
    workbench::save(&db, &s).unwrap();
    let before = workbench::apply(
        &db,
        Command::DeleteTask {
            task_id: "task-w".into(),
        },
    )
    .unwrap();
    let edited = workbench::apply(&db, edit_retained_preference(&p)).unwrap();
    let updated = &edited.working_preferences[0];
    assert_eq!(updated.version, p.version + 1);
    assert_eq!(updated.state, PreferenceState::Proposed);
    assert_eq!(updated.supporting_record_ids, p.supporting_record_ids);
    assert_eq!(updated.applicability, p.applicability);
    assert_eq!(updated.created_at_ms, p.created_at_ms);
    assert_eq!(edited.tasks, before.tasks);
    assert_eq!(edited.mcp, before.mcp);
    assert_eq!(edited.start_when_planned, before.start_when_planned);
    assert!(context::context_about(&edited, "w", Some("task-other"), "tests").is_empty());
    assert!(context::context_about(&edited, "w", Some("task-w"), "tests").is_empty());
    assert_eq!(
        workbench::load(&db).unwrap().working_preferences[0],
        *updated
    );
}

#[test]
fn retained_reference_edit_exemption_rejects_new_missing_cross_scope_and_stale_refs() {
    let db = Db::open_in_memory().unwrap();
    let s = fixture(&db);
    let foreign = context::record_task_observation(
        &db,
        &s,
        "task-other",
        HostObservation::Plan,
        &s.agent_runtime,
    )
    .unwrap();
    let proposed = workbench::apply(
        &db,
        Command::DecisionContext(DecisionContextCommand::SavePreference {
            workspace_id: "w".into(),
            id: String::new(),
            expected_version: None,
            applicability: PreferenceApplicability {
                terms: vec![],
                task_ids: vec!["task-w".into()],
            },
            instruction: "Task-scoped guidance".into(),
            supporting_record_ids: vec![],
            exceptions: vec![],
        }),
    )
    .unwrap();
    let p = &proposed.working_preferences[0];
    let mut state = workbench::load(&db).unwrap();
    state.tasks[0].status = TaskStatus::Cancelled;
    workbench::save(&db, &state).unwrap();
    let before = workbench::apply(
        &db,
        Command::DeleteTask {
            task_id: "task-w".into(),
        },
    )
    .unwrap();
    for (new_task, new_record) in [
        (Some("missing-task"), None),
        (Some("task-other"), None),
        (None, Some("missing-record")),
        (None, Some(foreign.id.as_str())),
    ] {
        let mut command = edit_retained_preference(p);
        if let Command::DecisionContext(DecisionContextCommand::SavePreference {
            applicability,
            supporting_record_ids,
            ..
        }) = &mut command
        {
            if let Some(id) = new_task {
                applicability.task_ids.push(id.into());
            }
            if let Some(id) = new_record {
                supporting_record_ids.push(id.into());
            }
        }
        assert!(workbench::apply(&db, command).is_err());
        assert_eq!(
            workbench::load(&db).unwrap().working_preferences,
            before.working_preferences
        );
    }
    let mut wrong_scope = edit_retained_preference(p);
    if let Command::DecisionContext(DecisionContextCommand::SavePreference {
        workspace_id, ..
    }) = &mut wrong_scope
    {
        *workspace_id = "other".into();
    }
    assert!(workbench::apply(&db, wrong_scope).is_err());
    let edited = workbench::apply(&db, edit_retained_preference(p)).unwrap();
    assert!(
        workbench::apply(&db, edit_retained_preference(p)).is_err(),
        "stale versions must still fail"
    );
    let mut recreate = edit_retained_preference(&edited.working_preferences[0]);
    if let Command::DecisionContext(DecisionContextCommand::SavePreference {
        id,
        expected_version,
        ..
    }) = &mut recreate
    {
        id.clear();
        *expected_version = None;
    }
    assert!(
        workbench::apply(&db, recreate).is_err(),
        "new preferences cannot inherit another row's exemption"
    );
}

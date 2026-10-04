//! Child tickets use the same per-ticket worker admission and authority checks.
use super::*;
use std::path::Path;

pub(super) const INSTRUCTION: &str = "Propose two or three bounded subtasks for the user's ticket. This is READ ONLY, awaiting explicit approval. Return ONLY a JSON array with objects {\"title\":\"...\",\"goal\":\"...\",\"files\":[\"exact relative file\"],\"tests\":[\"specific command\"],\"depends_on\":[]}. Dependencies are zero-based indices of earlier subtasks. Prefer independent disjoint files for parallel work. A dependent task must not edit its dependencies' files. Include all needed integration checks. No writes.";

/// Reconcile in the same durable scheduling transaction, before claiming work.
pub(super) fn reconcile(state: &mut Snapshot) {
    for split in state
        .splits
        .clone()
        .into_iter()
        .filter(|s| s.approved && !s.integrated)
    {
        let Some(parent) = state.tasks.iter().find(|t| t.id == split.parent_id) else {
            continue;
        };
        if matches!(
            parent.status,
            TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::Completed
        ) {
            continue;
        }
        let ids: Vec<_> = split
            .subtasks
            .iter()
            .filter_map(|p| p.task_id.as_ref())
            .collect();
        let failed = ids.iter().any(|id| {
            state.tasks.iter().any(|t| {
                &t.id == *id && matches!(t.status, TaskStatus::Failed | TaskStatus::Cancelled)
            })
        });
        if failed {
            for t in &mut state.tasks {
                if t.id == split.parent_id {
                    t.status = TaskStatus::Failed;
                    store::append_event(
                        t,
                        "supervisor",
                        "A subtask failed or was cancelled. Integration blocked; inspect child evidence and explicitly retry the parent.",
                    );
                } else if ids.contains(&&t.id)
                    && matches!(
                        t.status,
                        TaskStatus::Building
                            | TaskStatus::Queued
                            | TaskStatus::Planning
                            | TaskStatus::Reviewing
                    )
                {
                    t.status = TaskStatus::Cancelled;
                    store::append_event(
                        t,
                        "supervisor",
                        "Dependency group failed; worktree preserved.",
                    );
                }
            }
        } else if ids.len() == split.subtasks.len()
            && ids.iter().all(|id| {
                state.tasks.iter().any(|t| {
                    &t.id == *id
                        && matches!(t.status, TaskStatus::ReadyForReview | TaskStatus::Completed)
                })
            })
        {
            if let Some(parent) = state.tasks.iter_mut().find(|t| t.id == split.parent_id) {
                parent.status = TaskStatus::Building;
            }
        }
    }
}

impl Controller {
    pub(super) fn save_split_proposal(
        &self,
        task: &Task,
        answer: &str,
        base: &str,
        authority: &responsibilities::RunAuthority,
        decision_snapshot: &Snapshot,
    ) -> Result<(), String> {
        let plans = neko_core::decomposition::parse(answer)?;
        self.commit_phase_authorized(authority, Some(decision_snapshot), |state| {
            let split = state
                .splits
                .iter_mut()
                .find(|s| s.parent_id == task.id)
                .ok_or("Split missing")?;
            split.subtasks = plans;
            split.base = Some(base.into());
            let t = state
                .tasks
                .iter_mut()
                .find(|t| t.id == task.id)
                .ok_or("Task missing")?;
            t.plan = split
                .subtasks
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    format!(
                        "{}. {}\n{}\nFiles: {}\nChecks: {}\nDepends on: {:?}",
                        i + 1,
                        p.title,
                        p.goal,
                        p.files.join(", "),
                        p.tests.join("; "),
                        p.depends_on
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            t.status = TaskStatus::AwaitingApproval;
            store::append_event(
                t,
                "scout",
                "Split proposed. Approve these exact subtask plans before any child writes.",
            );
            Ok(())
        })
    }

    pub(super) fn child_context(
        &self,
        task: &Task,
        directory: &Path,
        cancel: &AtomicBool,
        authority: &responsibilities::RunAuthority,
    ) -> Result<Vec<String>, String> {
        let state = store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
        if !authority.valid(&state) {
            return Err("Task authority changed before dependency setup".into());
        }
        let Some(split) = state.splits.iter().find(|s| {
            s.subtasks
                .iter()
                .any(|p| p.task_id.as_deref() == Some(&task.id))
        }) else {
            return Ok(vec![]);
        };
        let own = split
            .subtasks
            .iter()
            .find(|p| p.task_id.as_deref() == Some(&task.id))
            .unwrap();
        let mut dependencies = own.depends_on.clone();
        let mut at = 0;
        while at < dependencies.len() {
            for d in &split.subtasks[dependencies[at]].depends_on {
                if !dependencies.contains(d) {
                    dependencies.push(*d);
                }
            }
            at += 1;
        }
        dependencies.sort_unstable();
        let mut inherited = Vec::new();
        for index in dependencies {
            let p = &split.subtasks[index];
            let child = state
                .tasks
                .iter()
                .find(|t| Some(&t.id) == p.task_id.as_ref())
                .ok_or("Dependency missing")?;
            if !matches!(
                child.status,
                TaskStatus::ReadyForReview | TaskStatus::Completed
            ) {
                return Err("Dependency is not verified".into());
            }
            let patch = native_runner::task_patch_scoped(
                Path::new(
                    child
                        .worktree
                        .as_deref()
                        .ok_or("Dependency worktree missing")?,
                ),
                p.base.as_deref().ok_or("Dependency base missing")?,
                &p.files,
                cancel,
            )?;
            native_runner::apply_task_patch(directory, &patch, cancel)?;
            inherited.extend(p.files.clone());
        }
        let base = native_runner::head(directory, cancel)?;
        self.commit_authorized(authority, |state| {
            for split in &mut state.splits {
                for p in &mut split.subtasks {
                    if p.task_id.as_deref() == Some(&task.id) {
                        p.base = Some(base.clone());
                    }
                }
            }
            Ok(())
        })?;
        Ok(inherited)
    }

    pub(super) fn integrate_split(
        &self,
        split: &TaskSplit,
        directory: &Path,
        cancel: &AtomicBool,
        authority: &responsibilities::RunAuthority,
    ) -> Result<String, String> {
        let state = store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
        if !authority.valid(&state) {
            return Err("Task authority changed before split integration".into());
        }
        for p in &split.subtasks {
            let child = state
                .tasks
                .iter()
                .find(|t| Some(&t.id) == p.task_id.as_ref())
                .ok_or("Child missing")?;
            if !matches!(
                child.status,
                TaskStatus::ReadyForReview | TaskStatus::Completed
            ) {
                return Err(
                    "All subtasks must pass independent verification before integration".into(),
                );
            }
            let patch = native_runner::task_patch_scoped(
                Path::new(child.worktree.as_deref().ok_or("Child worktree missing")?),
                p.base.as_deref().ok_or("Child base missing")?,
                &p.files,
                cancel,
            )?;
            native_runner::apply_task_patch(directory, &patch, cancel)?;
        }
        self.commit_authorized(authority, |state| {
            state
                .splits
                .iter_mut()
                .find(|s| s.parent_id == split.parent_id)
                .ok_or("Split missing")?
                .integrated = true;
            Ok(())
        })?;
        Ok("Integrated independently verified subtask diffs into this isolated parent worktree. Original repository untouched. Final independent integration verification follows.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_edit_after_split_return_rejects_proposal_and_metadata_commit() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Planning);
        let mut state = controller.command(Command::Snapshot).unwrap();
        state.splits.push(TaskSplit {
            parent_id: "t".into(),
            subtasks: vec![],
            approved: false,
            integrated: false,
            base: None,
        });
        store::save(&controller.db.lock().unwrap(), &state).unwrap();
        let authority = responsibilities::RunAuthority::for_task(&state, &state.tasks[0]).unwrap();
        controller
            .command(Command::AgentProfiles(
                neko_protocol::agent_profiles::ProfileCommand::Save {
                    profile: neko_protocol::agent_profiles::AgentProfile {
                        id: "default".into(),
                        name: "Neko".into(),
                        instructions: "Edited after split worker returned".into(),
                    },
                },
            ))
            .unwrap();
        let answer = r#"[{"title":"Left","goal":"Fix left","files":["left.rs"],"tests":["cargo test left"],"depends_on":[]},{"title":"Right","goal":"Fix right","files":["right.rs"],"tests":["cargo test right"],"depends_on":[]}]"#;
        assert!(
            controller
                .save_split_proposal(&state.tasks[0], answer, "base", &authority, &state)
                .unwrap_err()
                .contains("authority changed")
        );
        // Integration completion and child dependency bases use this identical
        // snapshot transaction, rather than unguarded individual metadata saves.
        assert!(
            controller
                .commit_authorized(&authority, |s| {
                    s.splits[0].integrated = true;
                    s.splits[0].base = Some("stale".into());
                    Ok(())
                })
                .is_err()
        );
        let after = controller.command(Command::Snapshot).unwrap();
        assert_eq!(after.splits, state.splits);
        assert_eq!(after.tasks, state.tasks);
        let fresh = responsibilities::RunAuthority::for_task(&after, &after.tasks[0]).unwrap();
        controller
            .save_split_proposal(&after.tasks[0], answer, "base", &fresh, &after)
            .unwrap();
        let after = controller.command(Command::Snapshot).unwrap();
        assert_eq!(after.tasks[0].status, TaskStatus::AwaitingApproval);
        assert_eq!(after.splits[0].subtasks.len(), 2);
    }

    #[test]
    fn split_proposal_uses_decision_time_guidance_after_an_edit() {
        use neko_protocol::decision_context::*;
        let controller = super::super::tests::controller_with_task(TaskStatus::Planning);
        let mut state = controller.command(Command::Snapshot).unwrap();
        state.splits.push(TaskSplit { parent_id: "t".into(), subtasks: vec![], approved: false, integrated: false, base: None });
        store::save(&controller.db.lock().unwrap(), &state).unwrap();
        let proposed = controller.command(Command::DecisionContext(DecisionContextCommand::SavePreference {
            workspace_id: "w".into(), id: String::new(), expected_version: None,
            applicability: PreferenceApplicability { terms: vec!["Task".into()], task_ids: vec![] },
            instruction: "Use bounded subtasks".into(), supporting_record_ids: vec![], exceptions: vec![],
        })).unwrap();
        let preference = proposed.working_preferences[0].clone();
        let prompt_snapshot = controller.command(Command::DecisionContext(DecisionContextCommand::KeepPreference {
            workspace_id: "w".into(), id: preference.id, expected_version: preference.version,
        })).unwrap();
        let preference = prompt_snapshot.working_preferences[0].clone();
        controller.command(Command::DecisionContext(DecisionContextCommand::SavePreference {
            workspace_id: "w".into(), id: preference.id.clone(), expected_version: Some(preference.version),
            applicability: preference.applicability, instruction: "Changed while splitting".into(),
            supporting_record_ids: vec![], exceptions: vec![],
        })).unwrap();
        let authority = responsibilities::RunAuthority::for_task(&prompt_snapshot, &prompt_snapshot.tasks[0]).unwrap();
        let answer = r#"[{"title":"Left","goal":"Fix left","files":["left.rs"],"tests":["cargo test left"],"depends_on":[]},{"title":"Right","goal":"Fix right","files":["right.rs"],"tests":["cargo test right"],"depends_on":[]}]"#;
        controller.save_split_proposal(&prompt_snapshot.tasks[0], answer, "base", &authority, &prompt_snapshot).unwrap();
        let result = controller.command(Command::Snapshot).unwrap();
        assert_eq!(result.decision_records[0].preference_versions, vec![PreferenceVersion { id: preference.id, version: preference.version }]);
        assert_eq!(result.decision_records[0].runtime, prompt_snapshot.agent_runtime);
    }

    #[test]
    fn failed_child_records_parent_failure_in_the_scheduling_transaction() {
        let controller = Arc::new(super::super::tests::controller_with_task(TaskStatus::Reviewing));
        let mut state = controller.command(Command::Snapshot).unwrap();
        let mut child = state.tasks[0].clone();
        child.id = "child".into(); child.status = TaskStatus::Failed;
        state.tasks.push(child);
        let mut sibling = state.tasks[0].clone(); sibling.id = "sibling".into(); sibling.status = TaskStatus::Completed;
        state.tasks.push(sibling);
        state.splits.push(TaskSplit {
            parent_id: "t".into(), approved: true, integrated: false, base: None,
            subtasks: vec![
                SubtaskPlan { title: "Child".into(), goal: "G".into(), files: vec!["a".into()], tests: vec!["check".into()], depends_on: vec![], task_id: Some("child".into()), base: None },
                SubtaskPlan { title: "Sibling".into(), goal: "H".into(), files: vec!["b".into()], tests: vec!["check".into()], depends_on: vec![], task_id: Some("sibling".into()), base: None },
            ],
        });
        store::save(&controller.db.lock().unwrap(), &state).unwrap();
        controller.tick_with(|_, _, _| panic!("Failed dependency must not start work")).unwrap();
        let result = controller.command(Command::Snapshot).unwrap();
        assert_eq!(result.tasks[0].status, TaskStatus::Failed);
        let record = result.decision_records.iter().find(|r| r.task_id.as_deref() == Some("t")).unwrap();
        assert_eq!(record.action, neko_protocol::decision_context::DecisionAction::ReportFailure);
        assert_eq!(record.runtime.provider, "host");
    }

    #[test]
    fn restarted_parent_waits_for_verified_children_before_integration() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Reviewing);
        let mut state = controller.command(Command::Snapshot).unwrap();
        let mut child = state.tasks[0].clone();
        child.id = "child".into();
        child.status = TaskStatus::Building;
        state.tasks.push(child);
        state.splits.push(TaskSplit {
            parent_id: "t".into(), approved: true, integrated: false, base: None,
            subtasks: vec![SubtaskPlan {
                title: "Child".into(), goal: "G".into(), files: vec!["a".into()],
                tests: vec!["check".into()], depends_on: vec![],
                task_id: Some("child".into()), base: None,
            }],
        });
        let mut sibling = state.tasks[1].clone();
        sibling.id = "sibling".into();
        sibling.status = TaskStatus::Completed;
        state.tasks.push(sibling);
        let mut sibling_plan = state.splits[0].subtasks[0].clone();
        sibling_plan.task_id = Some("sibling".into());
        sibling_plan.files = vec!["b".into()];
        state.splits[0].subtasks.push(sibling_plan);
        let db = controller.db.lock().unwrap();
        store::save(&db, &state).unwrap();
        let mut state = store::recover_interrupted(&db).unwrap();
        assert_eq!(state.tasks[0].status, TaskStatus::Building);
        assert!(!neko_core::decomposition::ready(&state, &state.tasks[0]));
        for status in [TaskStatus::Reviewing, TaskStatus::Failed, TaskStatus::Cancelled] {
            state.tasks[1].status = status;
            assert!(!neko_core::decomposition::ready(&state, &state.tasks[0]));
        }
        for status in [TaskStatus::ReadyForReview, TaskStatus::Completed] {
            state.tasks[1].status = status;
            assert!(neko_core::decomposition::ready(&state, &state.tasks[0]));
        }
        state.tasks.pop();
        assert!(!neko_core::decomposition::ready(&state, &state.tasks[0]));
    }

    #[test]
    fn acknowledged_child_still_unblocks_parent_integration() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Reviewing);
        let mut state = controller.command(Command::Snapshot).unwrap();
        let mut child = state.tasks[0].clone();
        child.id = "child".into();
        child.status = TaskStatus::Completed;
        state.tasks.push(child);
        state.splits.push(TaskSplit {
            parent_id: "t".into(),
            approved: true,
            integrated: false,
            base: None,
            subtasks: vec![SubtaskPlan {
                title: "Child".into(),
                goal: "G".into(),
                files: vec!["a".into()],
                tests: vec!["check".into()],
                depends_on: vec![],
                task_id: Some("child".into()),
                base: None,
            }],
        });
        reconcile(&mut state);
        assert_eq!(state.tasks[0].status, TaskStatus::Building);
    }
}

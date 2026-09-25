//! Schedule commands and due evaluation. Recurrences never run under the DB lock.
use super::*;
use neko_core::scheduled_plans as plans;
use neko_protocol::scheduled_plans::ScheduleCommand;

impl Controller {
    pub(super) fn schedule_command(&self, command: ScheduleCommand) -> Result<Snapshot, String> {
        if matches!(command, ScheduleCommand::List) {
            return store::load(&*self.db.lock().map_err(|_| "Schedule storage unavailable")?);
        }
        let id = match &command {
            ScheduleCommand::SetEnabled { id, enabled: true } | ScheduleCommand::RunNow { id } => {
                Some(id)
            }
            _ => None,
        };
        let expected = if let Some(id) = id {
            let db = self.db.lock().map_err(|_| "Schedule storage unavailable")?;
            Some(
                store::load(&db)?
                    .schedules
                    .into_iter()
                    .find(|s| &s.id == id)
                    .ok_or("Schedule no longer exists")?,
            )
        } else {
            None
        };
        let now = store::now_ms();
        let next = if matches!(command, ScheduleCommand::SetEnabled { enabled: true, .. }) {
            expected.as_ref().map(|s| plans::next_due(s, now))
        } else {
            None
        };
        let db = self.db.lock().map_err(|_| "Schedule storage unavailable")?;
        let mut state = store::load(&db)?;
        match command {
            ScheduleCommand::SetEnabled { enabled: true, .. } => plans::enable(
                &mut state,
                expected.as_ref().ok_or("Schedule missing")?,
                next.ok_or("Recurrence missing")??,
            )?,
            ScheduleCommand::RunNow { .. } => plans::claim(
                &mut state,
                expected.as_ref().ok_or("Schedule missing")?,
                Err("Manual run does not require recurrence".into()),
                now,
                true,
            )?,
            command => plans::apply(&mut state, command)?,
        }
        store::save(&db, &state)?;
        store::load(&db)
    }

    pub(super) fn schedule_tick(&self, now: i64) -> Result<(), String> {
        let expected = {
            let db = self.db.lock().map_err(|_| "Schedule storage unavailable")?;
            store::load(&db)?
                .schedules
                .into_iter()
                .find(|s| s.enabled && s.next_due_ms.is_some_and(|at| at <= now))
        };
        let Some(expected) = expected else {
            return Ok(());
        };
        let next = plans::following_due(&expected, now);
        let db = self.db.lock().map_err(|_| "Schedule storage unavailable")?;
        let original = store::load(&db)?;
        // A command may edit, pause, remove or manually run during evaluation.
        if !original.schedules.contains(&expected) {
            return Ok(());
        }
        let mut state = original.clone();
        let result = plans::claim(&mut state, &expected, next, now, false)
            .and_then(|()| store::save(&db, &state));
        if let Err(error) = result {
            // On capacity or invalid scope, commit a paused record WITHOUT the
            // proposed task. This prevents a tight retry loop or a lost claim.
            let mut fallback = original;
            if let Some(schedule) = fallback.schedules.iter_mut().find(|s| s.id == expected.id) {
                schedule.enabled = false;
                schedule.next_due_ms = None;
                schedule.last_result = format!(
                    "Paused before planning: {}",
                    error.chars().take(256).collect::<String>()
                );
            }
            store::save(&db, &fallback)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::scheduled_plans::Schedule;
    #[test]
    fn final_finite_occurrence_is_queued_once_then_stays_disabled_after_restart() {
        for rule in ["FREQ=HOURLY;COUNT=1", "FREQ=HOURLY;UNTIL=20260101T010000Z"] {
            let controller = controller();
            let due = chrono_fixture_time();
            {
                let db = controller.db.lock().unwrap();
                let mut state = store::load(&db).unwrap();
                let schedule = &mut state.schedules[0];
                schedule.rule = rule.into();
                schedule.anchor_ms = due;
                assert_eq!(plans::next_due(schedule, due - 1).unwrap(), due);
                schedule.next_due_ms = Some(due);
                schedule.enabled = true;
                store::save(&db, &state).unwrap();
            }
            controller.schedule_tick(due).unwrap();
            let restarted = Controller::new(controller.db.clone());
            restarted.schedule_tick(due).unwrap();
            restarted.schedule_tick(due + 3_600_000).unwrap();
            let after = restarted.command(Command::Snapshot).unwrap();
            assert_eq!(
                after.tasks.len(),
                2,
                "{rule}: final due occurrence must not be dropped"
            );
            assert_eq!(after.tasks.last().unwrap().status, TaskStatus::Queued);
            assert!(!after.schedules[0].enabled);
            assert!(after.schedules[0].next_due_ms.is_none());
        }
    }
    fn chrono_fixture_time() -> i64 {
        1_767_229_200_000
    } // 2026-01-01T01:00:00Z
    fn controller() -> Controller {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        controller
            .schedule_command(ScheduleCommand::Save {
                schedule: Schedule {
                    name: "Review".into(),
                    prompt: "Review the workspace".into(),
                    workspace_id: Some("w".into()),
                    rule: "FREQ=HOURLY".into(),
                    timezone: "UTC".into(),
                    anchor_ms: store::now_ms(),
                    ..Default::default()
                },
            })
            .unwrap();
        controller
    }
    #[test]
    fn schedule_tick_queues_once_and_survives_controller_restart() {
        let controller = controller();
        let state = controller.command(Command::Snapshot).unwrap();
        let id = state.schedules[0].id.clone();
        let enabled = controller
            .schedule_command(ScheduleCommand::SetEnabled { id, enabled: true })
            .unwrap();
        let due = enabled.schedules[0].next_due_ms.unwrap();
        controller.schedule_tick(due).unwrap();
        let restarted = Controller::new(controller.db.clone());
        restarted.schedule_tick(due).unwrap();
        let after = restarted.command(Command::Snapshot).unwrap();
        assert_eq!(after.tasks.len(), 2);
        assert_eq!(after.tasks.last().unwrap().status, TaskStatus::Queued);
        assert!(after.mcp.grants.is_empty());
        restarted
            .schedule_tick(after.schedules[0].next_due_ms.unwrap())
            .unwrap();
        assert_eq!(restarted.command(Command::Snapshot).unwrap().tasks.len(), 2);
    }
    #[test]
    fn invalid_enable_and_stale_evaluation_are_rejected() {
        let controller = controller();
        let state = controller.command(Command::Snapshot).unwrap();
        let mut schedule = state.schedules[0].clone();
        schedule.timezone = "bad zone".into();
        controller
            .schedule_command(ScheduleCommand::Save {
                schedule: schedule.clone(),
            })
            .unwrap();
        assert!(
            controller
                .schedule_command(ScheduleCommand::SetEnabled {
                    id: schedule.id.clone(),
                    enabled: true
                })
                .is_err()
        );
        let after = controller.command(Command::Snapshot).unwrap();
        assert!(!after.schedules[0].enabled);
        assert_eq!(after.tasks.len(), 1);
        controller
            .schedule_command(ScheduleCommand::RunNow { id: schedule.id })
            .unwrap();
        assert_eq!(
            controller.command(Command::Snapshot).unwrap().tasks.len(),
            2
        );
    }
    #[test]
    fn exhausted_storage_pauses_without_partial_claim_or_busy_retry() {
        let controller = controller();
        let id = controller.command(Command::Snapshot).unwrap().schedules[0]
            .id
            .clone();
        let state = controller
            .schedule_command(ScheduleCommand::SetEnabled { id, enabled: true })
            .unwrap();
        let due = state.schedules[0].next_due_ms.unwrap();
        {
            let db = controller.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            for _ in 0..999 {
                let mut task =
                    store::create_task("w".into(), None, "Done".into(), "Done".into()).unwrap();
                task.status = TaskStatus::Completed;
                state.tasks.push(task);
            }
            store::save(&db, &state).unwrap();
        }
        controller.schedule_tick(due).unwrap();
        controller.schedule_tick(due + 1).unwrap();
        let state = controller.command(Command::Snapshot).unwrap();
        assert_eq!(state.tasks.len(), 1000);
        assert!(!state.schedules[0].enabled);
        assert!(state.schedules[0].last_task_id.is_none());
        assert!(
            state.schedules[0]
                .last_result
                .contains("Paused before planning")
        );
    }
}

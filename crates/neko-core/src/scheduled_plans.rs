//! Durable schedules live in the same atomic snapshot as the tasks they enqueue.
use neko_protocol::{
    scheduled_plans::{Schedule, ScheduleCommand},
    workbench::{Snapshot, TaskStatus},
};

/// Potentially expensive recurrence traversal: evaluate before acquiring the DB lock.
pub fn next_due(schedule: &Schedule, now: i64) -> Result<i64, String> {
    crate::schedule_time::next_occurrence(
        &schedule.rule,
        &schedule.timezone,
        schedule.anchor_ms,
        now,
    )
}

/// A clean finite end is distinct from evaluation failure. Outside the DB lock.
pub fn following_due(schedule: &Schedule, now: i64) -> Result<Option<i64>, String> {
    crate::schedule_time::following_occurrence(
        &schedule.rule,
        &schedule.timezone,
        schedule.anchor_ms,
        now,
    )
}

fn current(state: &Snapshot, expected: &Schedule) -> Result<usize, String> {
    state
        .schedules
        .iter()
        .position(|s| s == expected)
        .ok_or("Schedule changed; refresh and try again".into())
}

fn workspace(state: &Snapshot, schedule: &Schedule) -> Result<String, String> {
    schedule
        .workspace_id
        .as_ref()
        .filter(|id| state.workspaces.iter().any(|w| &w.id == *id))
        .cloned()
        .ok_or("Choose an existing workspace before running or enabling this schedule".into())
}

pub fn enable(state: &mut Snapshot, expected: &Schedule, next: i64) -> Result<(), String> {
    let index = current(state, expected)?;
    workspace(state, expected)?;
    state.schedules[index].enabled = true;
    state.schedules[index].next_due_ms = Some(next);
    Ok(())
}

fn unfinished(status: TaskStatus) -> bool {
    !matches!(
        status,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
    )
}

pub fn validate(state: &Snapshot) -> Result<(), String> {
    if state.schedules.len() > 100 {
        return Err("Schedule limit reached".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut sources = std::collections::HashSet::new();
    for schedule in &state.schedules {
        if schedule.id.is_empty()
            || schedule.id.len() > 128
            || !ids.insert(&schedule.id)
            || schedule.name.trim().is_empty()
            || schedule.name.len() > 256
            || schedule.prompt.trim().is_empty()
            || schedule.prompt.len() > 32_768
            || schedule.rule.len() > 2048
            || schedule.timezone.len() > 128
            || schedule.last_result.len() > 4096
            || schedule
                .workspace_id
                .as_ref()
                .is_some_and(|id| id.len() > 128)
            || schedule
                .source_id
                .as_ref()
                .is_some_and(|id| id.len() > 128 || !sources.insert(id))
            || schedule
                .last_task_id
                .as_ref()
                .is_some_and(|id| !state.tasks.iter().any(|t| &t.id == id))
        {
            return Err("Invalid schedule record".into());
        }
        if schedule.enabled
            && (schedule.next_due_ms.is_none()
                || workspace(state, schedule).is_err()
                || schedule.rule.is_empty()
                || schedule.timezone.is_empty())
        {
            return Err(
                "Enabled schedule requires workspace, recurrence, timezone and next due time"
                    .into(),
            );
        }
    }
    Ok(())
}

/// Outcome is committed in the same upsert as its task. Keep a newer scheduler
/// skip/failure notice rather than replacing it with an older task's result.
pub fn refresh_results(state: &mut Snapshot) {
    for schedule in &mut state.schedules {
        if schedule.last_result.starts_with("Skipped:")
            || schedule.last_result.starts_with("Paused")
        {
            continue;
        }
        let Some(task) = schedule
            .last_task_id
            .as_ref()
            .and_then(|id| state.tasks.iter().find(|t| &t.id == id))
        else {
            continue;
        };
        let status = match task.status {
            TaskStatus::AwaitingApproval => "Plan ready; awaiting explicit approval",
            TaskStatus::ReadyForReview => "Result ready for review",
            TaskStatus::Completed => "Completed",
            TaskStatus::Failed => "Failed",
            TaskStatus::Cancelled => "Cancelled",
            _ => continue,
        };
        schedule.last_result = format!(
            "{status}: {}",
            task.result.chars().take(512).collect::<String>()
        );
    }
}

/// Change the claim and enqueue its plan together; callers commit this snapshot
/// once before any worker can see it. Stale evaluated rules cannot claim edits.
pub fn claim(
    state: &mut Snapshot,
    expected: &Schedule,
    next: Result<Option<i64>, String>,
    now: i64,
    manual: bool,
) -> Result<(), String> {
    let index = current(state, expected)?;
    if !manual && (!expected.enabled || !expected.next_due_ms.is_some_and(|at| at <= now)) {
        return Err("Schedule is not due".into());
    }
    let workspace = workspace(state, expected)?;
    let overlapping = expected.last_task_id.as_ref().is_some_and(|id| {
        state
            .tasks
            .iter()
            .any(|t| &t.id == id && unfinished(t.status))
    });
    if manual && overlapping {
        return Err("The previous scheduled task still needs attention".into());
    }
    if !manual {
        match next {
            Ok(Some(next)) if next > now => state.schedules[index].next_due_ms = Some(next),
            Ok(None) => {
                // This persisted occurrence was valid when enabled/last claimed.
                // Commit its final task and disabled future together below.
                state.schedules[index].enabled = false;
                state.schedules[index].next_due_ms = None;
            }
            _ => {
                state.schedules[index].enabled = false;
                state.schedules[index].next_due_ms = None;
                state.schedules[index].last_result = "Paused: recurrence evaluation failed or exceeded its search bound. Review its rule and timezone.".into();
                return Ok(());
            }
        }
    }
    if overlapping {
        state.schedules[index].last_result =
            "Skipped: previous scheduled task still needs attention.".into();
        return Ok(());
    }
    let mut task = crate::workbench::create_task(
        workspace,
        None,
        expected.name.clone(),
        expected.prompt.clone(),
    )?;
    crate::workbench::append_event(
        &mut task,
        "schedule",
        "Scheduled read-only plan. Building requires explicit approval; source permissions were not imported.",
    );
    state.schedules[index].last_task_id = Some(task.id.clone());
    state.schedules[index].last_result =
        "Queued for read-only planning; edits require approval.".into();
    state.tasks.push(task);
    Ok(())
}

pub fn apply(state: &mut Snapshot, command: ScheduleCommand) -> Result<(), String> {
    match command {
        ScheduleCommand::List => {}
        ScheduleCommand::Save { schedule } => {
            if schedule.name.trim().is_empty()
                || schedule.name.len() > 256
                || schedule.prompt.trim().is_empty()
                || schedule.prompt.len() > 32_768
            {
                return Err("Provide a schedule name and prompt within their limits".into());
            }
            if schedule.rule.len() > 2048
                || schedule.timezone.len() > 128
                || schedule
                    .workspace_id
                    .as_ref()
                    .is_some_and(|id| id.len() > 128)
            {
                return Err("Schedule fields exceed their limits".into());
            }
            if schedule
                .workspace_id
                .as_ref()
                .is_some_and(|id| !state.workspaces.iter().any(|w| &w.id == id))
            {
                return Err("Schedule workspace no longer exists".into());
            }
            let previous = if schedule.id.is_empty() {
                None
            } else {
                Some(
                    state
                        .schedules
                        .iter()
                        .position(|s| s.id == schedule.id)
                        .ok_or("Schedule no longer exists")?,
                )
            };
            if previous.is_none() && state.schedules.len() >= 100 {
                return Err("Schedule limit reached".into());
            }
            let old = previous
                .map(|i| state.schedules[i].clone())
                .unwrap_or_default();
            let saved = Schedule {
                id: if old.id.is_empty() {
                    crate::workbench::new_id()
                } else {
                    old.id
                },
                name: schedule.name.trim().into(),
                prompt: schedule.prompt,
                workspace_id: schedule.workspace_id,
                rule: schedule.rule,
                timezone: schedule.timezone,
                anchor_ms: schedule.anchor_ms,
                source_id: old.source_id,
                last_task_id: old.last_task_id,
                last_result: old.last_result,
                ..Default::default()
            };
            if let Some(i) = previous {
                state.schedules[i] = saved;
            } else {
                state.schedules.push(saved);
            }
        }
        ScheduleCommand::SetEnabled { id, enabled: false } => {
            let schedule = state
                .schedules
                .iter_mut()
                .find(|s| s.id == id)
                .ok_or("Schedule no longer exists")?;
            schedule.enabled = false;
            schedule.next_due_ms = None;
        }
        ScheduleCommand::Remove { id } => {
            let index = state
                .schedules
                .iter()
                .position(|s| s.id == id)
                .ok_or("Schedule no longer exists")?;
            state.schedules.remove(index);
        }
        _ => return Err("Schedule operation requires the daemon".into()),
    }
    Ok(())
}

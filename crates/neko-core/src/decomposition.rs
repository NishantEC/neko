//! Bounded, explicitly approved child tickets; no separate worker pool.
use neko_protocol::workbench::*;

pub fn validate(plans: &[SubtaskPlan]) -> Result<(), String> {
    if !(2..=3).contains(&plans.len()) {
        return Err("Split into two or three bounded subtasks".into());
    }
    for (index, p) in plans.iter().enumerate() {
        if p.title.trim().is_empty()
            || p.title.len() > 512
            || p.goal.trim().is_empty()
            || p.goal.len() > 4096
            || p.files.is_empty()
            || p.files.len() > 20
            || p.tests.is_empty()
            || p.tests.len() > 16
        {
            return Err("Each subtask needs a bounded title, goal, exact files and tests".into());
        }
        if p.depends_on.len() > 2 || p.depends_on.iter().any(|d| *d >= index) {
            return Err("Dependencies must reference earlier subtasks".into());
        }
        let mut dependencies = p.depends_on.clone();
        let mut at = 0;
        while at < dependencies.len() {
            for d in &plans[dependencies[at]].depends_on {
                if !dependencies.contains(d) {
                    dependencies.push(*d);
                }
            }
            at += 1;
        }
        if dependencies
            .iter()
            .any(|i| plans[*i].files.iter().any(|f| p.files.contains(f)))
        {
            return Err("Dependent subtasks must use separate file scopes; combine overlapping work in one subtask".into());
        }
        if p.files.iter().any(|f| {
            f.is_empty()
                || f.len() > 1024
                || std::path::Path::new(f)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
                || f.split('/').any(|c| c == ".git")
        }) || p
            .tests
            .iter()
            .any(|t| t.trim().is_empty() || t.len() > 1024)
        {
            return Err(
                "Subtask paths and checks must stay bounded and repository-relative".into(),
            );
        }
    }
    Ok(())
}

pub fn parse(answer: &str) -> Result<Vec<SubtaskPlan>, String> {
    let mut plans: Vec<SubtaskPlan> =
        serde_json::from_str(answer).map_err(|e| format!("Invalid split proposal: {e}"))?;
    validate(&plans)?;
    for p in &mut plans {
        p.task_id = None;
        p.base = None;
    }
    Ok(plans)
}

pub fn ready(state: &Snapshot, task: &Task) -> bool {
    for split in &state.splits {
        if let Some(plan) = split
            .subtasks
            .iter()
            .find(|p| p.task_id.as_deref() == Some(&task.id))
        {
            return split.approved
                && plan.depends_on.iter().all(|i| {
                    split
                        .subtasks
                        .get(*i)
                        .and_then(|p| p.task_id.as_ref())
                        .is_some_and(|id| {
                            state
                                .tasks
                                .iter()
                                .any(|t| &t.id == id && matches!(t.status, TaskStatus::ReadyForReview | TaskStatus::Completed))
                        })
                });
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_cycles_unbounded_and_escaped_plans() {
        let p = SubtaskPlan {
            title: "A".into(),
            goal: "Fix A".into(),
            files: vec!["a.rs".into()],
            tests: vec!["cargo test a".into()],
            depends_on: vec![],
            task_id: None,
            base: None,
        };
        assert!(validate(&[p.clone()]).is_err());
        assert!(validate(&[p.clone(), p.clone()]).is_ok());
        let mut bad = p.clone();
        bad.depends_on = vec![1];
        assert!(validate(&[p.clone(), bad]).is_err());
        let mut bad = p.clone();
        bad.files = vec!["../a".into()];
        assert!(validate(&[p, bad]).is_err());
    }
}

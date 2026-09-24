//! Neko task attention in the quick launcher; editing happens in the workspace.
use crate::{
    Db, Provider,
    provider::ProviderError,
    search::{Candidate, fuzzy_score},
    workbench,
};
use neko_protocol::workbench::TaskStatus;
use std::sync::{Arc, Mutex};

pub struct NativeTasksProvider {
    db: Arc<Mutex<Db>>,
}
impl NativeTasksProvider {
    pub fn new(db: Arc<Mutex<Db>>) -> Self {
        Self { db }
    }
}
impl Provider for NativeTasksProvider {
    fn id(&self) -> &'static str {
        "neko-task"
    }
    fn section_label(&self) -> &'static str {
        "Tickets"
    }
    fn search(&self, query: &str, _now: i64) -> Vec<Candidate> {
        let Ok(snapshot) = workbench::load(&self.db.lock().unwrap()) else {
            return vec![];
        };
        let Some(template) = crate::commands::CommandsProvider::standalone()
            .search("Neko", 0)
            .into_iter()
            .next()
        else {
            return vec![];
        };
        snapshot
            .tasks
            .iter()
            .rev()
            .filter_map(|task| {
                if matches!(task.status, TaskStatus::Cancelled | TaskStatus::Completed)
                    && query.is_empty()
                {
                    return None;
                }
                let score = if query.is_empty() {
                    1.0
                } else {
                    fuzzy_score(query, &task.title)?
                };
                let mut item = template.item.clone();
                item.id = task.id.clone();
                item.kind = "neko-task".into();
                item.title = task.title.clone();
                item.section_label = "Tickets".into();
                item.badge = Some(badge(task.status).into());
                item.subtitle = snapshot
                    .workspaces
                    .iter()
                    .find(|w| w.id == task.workspace_id)
                    .map(|w| w.name.clone());
                item.preview = Some(if task.plan.is_empty() {
                    task.goal.clone()
                } else {
                    task.plan.clone()
                });
                item.preview_markdown = true;
                Some(Candidate {
                    score: score + priority(task.status),
                    item,
                })
            })
            .take(50)
            .collect()
    }
    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        Err(ProviderError("Open this task in the Neko workspace".into()))
    }
}

/// The same words the main window uses.
pub fn badge(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::AwaitingApproval => "Needs approval",
        TaskStatus::ReadyForReview => "Ready for review",
        TaskStatus::Failed => "Stopped",
        TaskStatus::Queued => "Queued",
        TaskStatus::Planning => "Planning",
        TaskStatus::Building => "Building",
        TaskStatus::Reviewing => "Reviewing",
        TaskStatus::Completed => "Done",
        TaskStatus::Cancelled => "Cancelled",
    }
}

/// Needs you first, then running work, then the rest.
fn priority(status: TaskStatus) -> f32 {
    match status {
        TaskStatus::AwaitingApproval | TaskStatus::ReadyForReview | TaskStatus::Failed => 100.0,
        TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => 50.0,
        TaskStatus::Completed | TaskStatus::Cancelled => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_imported_agent_stores_are_needed_for_an_empty_task_list() {
        let provider =
            NativeTasksProvider::new(Arc::new(Mutex::new(Db::open_in_memory().unwrap())));
        assert!(provider.search("", 0).is_empty());
    }

    #[test]
    fn tickets_that_need_you_rank_above_running_work() {
        assert!(priority(TaskStatus::ReadyForReview) > priority(TaskStatus::Building));
        assert!(priority(TaskStatus::Building) > priority(TaskStatus::Completed));
        assert_eq!(badge(TaskStatus::AwaitingApproval), "Needs approval");
    }
}

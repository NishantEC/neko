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
        "Neko Tasks"
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
                item.section_label = "Neko Tasks".into();
                item.badge = Some(format!("{:?}", task.status));
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
                    score: score
                        + if matches!(
                            task.status,
                            TaskStatus::AwaitingApproval | TaskStatus::Failed
                        ) {
                            100.0
                        } else {
                            0.0
                        },
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_imported_agent_stores_are_needed_for_an_empty_task_list() {
        let provider =
            NativeTasksProvider::new(Arc::new(Mutex::new(Db::open_in_memory().unwrap())));
        assert!(provider.search("", 0).is_empty());
    }
}

//! Neko ticket attention in the quick launcher; editing happens in the workspace.
use crate::{
    Db, Provider,
    provider::ProviderError,
    search::{Candidate, fuzzy_score},
    workbench,
};
use neko_protocol::SearchItem;
use neko_protocol::workbench::{Snapshot, TaskStatus};
use std::sync::{Arc, Mutex};

const MAX_RESULTS: usize = 50;
const PREVIEW_BYTES: usize = 4 * 1024;

/// What search needs from one ticket, built once per store revision.
struct Row {
    status: TaskStatus,
    title: String,
    item: SearchItem,
}

pub struct NativeTasksProvider {
    db: Arc<Mutex<Db>>,
    /// Rows for the store revision they were built from. Search reads only
    /// this; the store is re-parsed when (and only when) it has changed.
    cache: Mutex<(u64, Arc<Vec<Row>>)>,
}
impl NativeTasksProvider {
    pub fn new(db: Arc<Mutex<Db>>) -> Self {
        Self { db, cache: Mutex::new((0, Arc::new(Vec::new()))) }
    }

    fn rows(&self) -> Arc<Vec<Row>> {
        let revision = workbench::revision();
        {
            let cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if cache.0 == revision {
                return cache.1.clone();
            }
        }
        let loaded = workbench::load(&self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        let rows = Arc::new(match loaded {
            Ok(snapshot) => build_rows(&snapshot),
            Err(_) => return self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).1.clone(),
        });
        *self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = (revision, rows.clone());
        rows
    }
}

fn build_rows(snapshot: &Snapshot) -> Vec<Row> {
    let Some(template) = crate::commands::CommandsProvider::standalone().search("Neko", 0).into_iter().next() else {
        return vec![];
    };
    snapshot
        .tasks
        .iter()
        .rev()
        .map(|task| {
            let mut item = template.item.clone();
            item.id = task.id.clone();
            item.kind = "neko-task".into();
            item.title = task.title.clone();
            item.section_label = "Tickets".into();
            item.badge = Some(badge(task.status).into());
            item.subtitle = snapshot.workspaces.iter().find(|w| w.id == task.workspace_id).map(|w| w.name.clone());
            let preview = if task.plan.is_empty() { &task.goal } else { &task.plan };
            item.preview = Some(shorten(preview, PREVIEW_BYTES));
            item.preview_markdown = true;
            Row { status: task.status, title: task.title.clone(), item }
        })
        .collect()
}

fn shorten(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

impl Provider for NativeTasksProvider {
    fn id(&self) -> &'static str {
        "neko-task"
    }
    fn section_label(&self) -> &'static str {
        "Tickets"
    }
    fn search(&self, query: &str, _now: i64) -> Vec<Candidate> {
        let rows = self.rows();
        let mut candidates: Vec<Candidate> = rows
            .iter()
            .filter_map(|row| {
                if query.is_empty() && matches!(row.status, TaskStatus::Cancelled | TaskStatus::Completed) {
                    return None;
                }
                let score = if query.is_empty() { 1.0 } else { fuzzy_score(query, &row.title)? };
                Some(Candidate { score: score + priority(row.status), item: row.item.clone() })
            })
            .collect();
        // Rank every match first, then cut: a strong match deep in history
        // must not be dropped before scoring sees it.
        candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
        candidates.truncate(MAX_RESULTS);
        candidates
    }
    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        Err(ProviderError("Open this ticket in the Neko workspace".into()))
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

    #[test]
    fn a_strong_match_deep_in_history_is_not_cut_before_ranking() {
        use neko_protocol::workbench::Workspace;
        let db = Db::open_in_memory().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.workspaces.push(Workspace { id: "w".into(), name: "hme".into(), repository: "/tmp".into(), instructions: String::new(), away_enabled: false });
        snapshot.tasks.push(workbench::create_task("w".into(), None, "zebra crossing".into(), "g".into()).unwrap());
        for i in 0..120 {
            let mut filler = workbench::create_task("w".into(), None, format!("zeta item {i} with a long name"), "g".into()).unwrap();
            filler.status = TaskStatus::Completed;
            snapshot.tasks.push(filler);
        }
        workbench::save(&db, &snapshot).unwrap();
        let provider = NativeTasksProvider::new(Arc::new(Mutex::new(db)));
        let results = provider.search("zebra", 0);
        assert_eq!(results.first().map(|c| c.item.title.as_str()), Some("zebra crossing"));
        assert!(provider.search("", 0).len() <= MAX_RESULTS);
    }
}

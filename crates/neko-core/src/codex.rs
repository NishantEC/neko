//! A process-free projection of the Codex app-server state needed by neko's UI.
//!
//! The daemon actor owns transport and polling. This module only reduces its
//! JSON results and notifications into stable, displayable state.

use std::sync::{Arc, RwLock};

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};
use serde_json::Value;

use crate::{
    provider::{Provider, ProviderError},
    search::{fuzzy_score, Candidate},
};

/// A task state deliberately reduced to the distinctions the UI presents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Working,
    Waiting,
    Idle,
    Failed,
    Unknown,
}

impl TaskStatus {
    fn from_json(value: Option<&Value>) -> Self {
        match value
            .and_then(|status| status.get("type"))
            .and_then(Value::as_str)
        {
            Some("working") | Some("running") => Self::Working,
            Some("waiting") | Some("needsInput") => Self::Waiting,
            Some("idle") | Some("completed") => Self::Idle,
            Some("failed") | Some("error") => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

/// A Codex task row rendered by neko.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub provider: Option<String>,
    pub updated_at: i64,
    pub status: TaskStatus,
}

/// The category of request that needs the person's approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalKind {
    CommandExecution,
    FileChange,
    Network,
    Unknown,
}

/// An outstanding Codex approval request displayed by neko.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub thread_id: String,
    pub request_id: String,
    pub title: String,
    pub detail: Option<String>,
    pub kind: ApprovalKind,
}

/// The UI-relevant state reduced from Codex app-server messages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub tasks: Vec<Task>,
    pub approvals: Vec<Approval>,
    pub generation: u64,
    pub refreshed_at_unix_ms: i64,
    pub available: bool,
}

impl Snapshot {
    /// Reduces a successful `thread/list` result. Unknown fields are ignored,
    /// and entries without an id or title are not displayable so are skipped.
    pub fn from_thread_list(result: &Value) -> Self {
        let mut snapshot = Self {
            available: result.get("data").and_then(Value::as_array).is_some(),
            refreshed_at_unix_ms: crate::now_unix_ms(),
            ..Self::default()
        };
        snapshot.apply_thread_list(result);
        snapshot
    }

    /// Applies a successful `thread/list` result without replacing unrelated
    /// state, such as pending approvals or the daemon-owned availability flag.
    /// Malformed results leave the current task list intact.
    pub fn apply_thread_list(&mut self, value: &Value) {
        let Some(entries) = value.get("data").and_then(Value::as_array) else {
            return;
        };
        let tasks = entries.iter().filter_map(parse_task).collect();

        self.replace_tasks(tasks);
        self.refreshed_at_unix_ms = crate::now_unix_ms();
    }

    /// Creates one pending approval for focused reducer tests and callers that
    /// receive an approval request before the full notification reducer grows.
    pub fn with_approval(thread_id: impl Into<String>, request_id: impl Into<String>) -> Self {
        let mut snapshot = Self {
            available: true,
            refreshed_at_unix_ms: crate::now_unix_ms(),
            ..Self::default()
        };
        snapshot.approvals.push(Approval {
            thread_id: thread_id.into(),
            request_id: request_id.into(),
            title: "Approval required".into(),
            detail: None,
            kind: ApprovalKind::Unknown,
        });
        snapshot.generation = 1;
        snapshot
    }

    /// Replaces task rows and advances the UI generation only for a visible
    /// change. Pending approvals are independent from a list refresh.
    pub fn replace_tasks(&mut self, tasks: Vec<Task>) {
        if self.tasks != tasks {
            self.tasks = tasks;
            self.generation = self.generation.saturating_add(1);
        }
    }

    /// Applies the subset of app-server notifications that changes this
    /// snapshot. Unknown notifications are deliberately harmless.
    pub fn apply_notification(&mut self, notification: &Value) {
        if notification.get("method").and_then(Value::as_str) != Some("serverRequest/resolved") {
            return;
        }

        let Some(params) = notification.get("params") else {
            return;
        };
        let (Some(thread_id), Some(request_id)) = (
            params.get("threadId").and_then(Value::as_str),
            params.get("requestId").and_then(Value::as_str),
        ) else {
            return;
        };

        let previous_len = self.approvals.len();
        self.approvals.retain(|approval| {
            approval.thread_id != thread_id || approval.request_id != request_id
        });
        if self.approvals.len() != previous_len {
            self.generation = self.generation.saturating_add(1);
            self.refreshed_at_unix_ms = crate::now_unix_ms();
        }
    }
}

fn parse_task(entry: &Value) -> Option<Task> {
    let id = entry.get("id")?.as_str()?;
    let title = entry.get("name")?.as_str()?;
    if id.trim().is_empty() || title.trim().is_empty() {
        return None;
    }

    Some(Task {
        id: id.to_owned(),
        title: title.to_owned(),
        cwd: entry.get("cwd").and_then(Value::as_str).map(str::to_owned),
        provider: entry
            .get("modelProvider")
            .and_then(Value::as_str)
            .map(str::to_owned),
        updated_at: entry.get("updatedAt").and_then(Value::as_i64).unwrap_or(0),
        status: TaskStatus::from_json(entry.get("status")),
    })
}

/// Searchable task rows backed exclusively by the daemon-owned app-server
/// projection. The app-server actor is the only writer; a palette keystroke
/// only takes this short read lock and never starts a process or reads disk.
pub struct CodexTasksProvider {
    snapshot: Arc<RwLock<Snapshot>>,
}

impl CodexTasksProvider {
    pub fn with_snapshot(snapshot: Arc<RwLock<Snapshot>>) -> Self {
        Self { snapshot }
    }
}

impl Provider for CodexTasksProvider {
    fn id(&self) -> &'static str {
        "codex-task"
    }

    fn section_label(&self) -> &'static str {
        "Agents"
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        // Clone while holding the lock, then match and construct wire rows
        // after releasing it so an app-server refresh is never held behind a
        // fuzzy search or allocation pass.
        let snapshot = self.snapshot.read().unwrap().clone();
        let query = query.trim();
        let mut candidates: Vec<(u8, i64, Candidate)> = snapshot
            .tasks
            .iter()
            .filter_map(|task| {
                let score = if query.is_empty() {
                    0.0
                } else {
                    [
                        fuzzy_score(query, &task.title),
                        task.cwd.as_deref().and_then(|cwd| fuzzy_score(query, cwd)),
                        task.provider
                            .as_deref()
                            .and_then(|provider| fuzzy_score(query, provider)),
                    ]
                    .into_iter()
                    .flatten()
                    .reduce(f32::max)?
                };
                Some((
                    task_status_priority(&task.status),
                    task.updated_at,
                    Candidate {
                        score,
                        item: task_item(task, snapshot.available),
                    },
                ))
            })
            .collect();

        if query.is_empty() {
            // An empty palette is an attention surface: a task blocked on a
            // person precedes active work, which precedes context. Updated
            // time deliberately breaks ties only *within* a status.
            candidates.sort_by(|left, right| {
                right
                    .0
                    .cmp(&left.0)
                    .then_with(|| right.1.cmp(&left.1))
                    .then_with(|| left.2.item.id.cmp(&right.2.item.id))
            });
            // `search::allocate` orders each provider by score. Encode this
            // settled ordering in scores rather than relying on `Vec` order.
            let len = candidates.len();
            return candidates
                .into_iter()
                .enumerate()
                .map(|(rank, (_, _, mut candidate))| {
                    candidate.score = (len - rank) as f32;
                    candidate
                })
                .collect();
        }

        candidates
            .into_iter()
            .map(|(_, _, candidate)| candidate)
            .collect()
    }

    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        // The client observes `enters_mode` and never routes this primary
        // action back to the daemon. Task 5 owns that mode's implementation.
        Err(ProviderError("Codex task mode is not available yet".to_string()))
    }
}

fn task_status_priority(status: &TaskStatus) -> u8 {
    match status {
        TaskStatus::Waiting => 4,
        TaskStatus::Working => 3,
        TaskStatus::Idle => 2,
        // A failed run is history rather than fresh work, but is still more
        // useful than an unclassified task when the palette is at rest.
        TaskStatus::Failed => 1,
        TaskStatus::Unknown => 0,
    }
}

fn task_item(task: &Task, available: bool) -> SearchItem {
    let subtitle = match (&task.provider, &task.cwd) {
        (Some(provider), Some(cwd)) => Some(format!("{provider} · {cwd}")),
        (Some(provider), None) => Some(provider.clone()),
        (None, Some(cwd)) => Some(cwd.clone()),
        (None, None) => None,
    };
    let (icon, badge) = match task.status {
        TaskStatus::Working => (Glyph::AgentLive, Some("LIVE".to_string())),
        TaskStatus::Waiting => (Glyph::AgentLive, Some("WAITING".to_string())),
        TaskStatus::Idle => (Glyph::Agent, None),
        TaskStatus::Failed => (Glyph::Agent, Some("FAILED".to_string())),
        TaskStatus::Unknown => (Glyph::Agent, Some("UNKNOWN".to_string())),
    };
    let actions = (task.status == TaskStatus::Waiting)
        .then(|| ItemAction {
            id: "review".to_string(),
            label: "Review".to_string(),
            destructive: false,
        })
        .into_iter()
        .collect();

    SearchItem {
        id: task.id.clone(),
        kind: "codex-task".to_string(),
        title: task.title.clone(),
        subtitle,
        icon: Icon::Glyph(icon),
        section_label: "Agents".to_string(),
        action_label: "Open task  ↵".to_string(),
        badge,
        accessory: (!available).then(|| "Codex unavailable".to_string()),
        enters_mode: Some("codex-task".to_string()),
        group_label: None,
        actions,
        source: Some("Codex".to_string()),
        meter: None,
        keeps_open: false,
        preview_markdown: false,
        speaker: None,
        images: Vec::new(),
        preview: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::sync::{Arc, RwLock};

    use crate::provider::Provider;

    use super::{CodexTasksProvider, Snapshot, Task, TaskStatus};

    fn task(id: &str, title: &str, cwd: Option<&str>, provider: Option<&str>, updated_at: i64, status: TaskStatus) -> Task {
        Task {
            id: id.into(),
            title: title.into(),
            cwd: cwd.map(str::to_owned),
            provider: provider.map(str::to_owned),
            updated_at,
            status,
        }
    }

    #[test]
    fn codex_tasks_empty_query_prioritizes_waiting_then_working_then_idle_over_recency() {
        let snapshot = Snapshot {
            available: true,
            tasks: vec![
                task("idle", "Oldest priority", None, None, 30, TaskStatus::Idle),
                task("working", "Middle priority", None, None, 20, TaskStatus::Working),
                task("waiting", "Newest priority", None, None, 10, TaskStatus::Waiting),
            ],
            ..Snapshot::default()
        };
        let provider = CodexTasksProvider::with_snapshot(Arc::new(RwLock::new(snapshot)));

        let candidates = provider.search("", 0);

        assert_eq!(
            candidates.iter().map(|candidate| candidate.item.id.as_str()).collect::<Vec<_>>(),
            ["waiting", "working", "idle"]
        );
    }

    #[test]
    fn codex_tasks_fuzzy_match_title_working_directory_and_provider() {
        let snapshot = Snapshot {
            available: true,
            tasks: vec![task(
                "thr-1",
                "Repair launcher ranking",
                Some("/work/neko"),
                Some("openai"),
                10,
                TaskStatus::Working,
            )],
            ..Snapshot::default()
        };
        let provider = CodexTasksProvider::with_snapshot(Arc::new(RwLock::new(snapshot)));

        for query in ["launcher", "neko", "openai"] {
            assert_eq!(provider.search(query, 0).len(), 1, "{query} should find the task");
        }
    }

    #[test]
    fn thread_list_retains_ui_relevant_task_fields_and_skips_malformed_entries() {
        let snapshot = Snapshot::from_thread_list(&json!({"data": [
            {
                "id": "thr-1",
                "name": "Fix ranking",
                "cwd": "/work/neko",
                "modelProvider": "openai",
                "updatedAt": 42,
                "status": {"type": "working"},
                "ignored": "future field"
            },
            {
                "id": "missing-required-fields"
            }
        ]}));

        assert_eq!(snapshot.tasks.len(), 1);
        assert_eq!(snapshot.tasks[0].id, "thr-1");
        assert_eq!(snapshot.tasks[0].title, "Fix ranking");
        assert_eq!(snapshot.tasks[0].cwd.as_deref(), Some("/work/neko"));
        assert_eq!(snapshot.tasks[0].provider.as_deref(), Some("openai"));
        assert_eq!(snapshot.tasks[0].updated_at, 42);
        assert_eq!(snapshot.tasks[0].status, TaskStatus::Working);
    }

    #[test]
    fn thread_list_skips_entries_with_blank_ids_or_titles() {
        let snapshot = Snapshot::from_thread_list(&json!({"data": [
            {"id": "", "name": "Has no id"},
            {"id": "   ", "name": "Has no id"},
            {"id": "thr-2", "name": ""},
            {"id": "thr-3", "name": "\t"},
            {"id": "thr-4", "name": "Visible task"}
        ]}));

        assert_eq!(snapshot.tasks.len(), 1);
        assert_eq!(snapshot.tasks[0].id, "thr-4");
    }

    #[test]
    fn thread_list_refresh_keeps_pending_approvals() {
        let mut snapshot = Snapshot::with_approval("thr-1", "request-1");
        snapshot.available = false;

        snapshot.apply_thread_list(&json!({"data": [{
            "id": "thr-1",
            "name": "Fix ranking"
        }]}));

        assert!(!snapshot.available);
        assert_eq!(snapshot.approvals.len(), 1);
        assert_eq!(snapshot.approvals[0].request_id, "request-1");
    }

    #[test]
    fn changed_thread_list_refresh_advances_generation() {
        let mut snapshot = Snapshot::from_thread_list(&json!({"data": [{
            "id": "thr-1",
            "name": "Fix ranking"
        }]}));
        let generation = snapshot.generation;

        snapshot.apply_thread_list(&json!({"data": [{
            "id": "thr-1",
            "name": "Fix the ranking"
        }]}));

        assert_eq!(snapshot.generation, generation + 1);
        assert_eq!(snapshot.tasks[0].title, "Fix the ranking");
    }

    #[test]
    fn resolved_notification_removes_only_its_matching_approval() {
        let mut snapshot = Snapshot::with_approval("thr-1", "request-1");
        snapshot.approvals.push(super::Approval {
            thread_id: "thr-2".into(),
            request_id: "request-2".into(),
            title: "Other approval".into(),
            detail: None,
            kind: super::ApprovalKind::Unknown,
        });

        snapshot.apply_notification(&json!({
            "method": "serverRequest/resolved",
            "params": {"threadId": "thr-1", "requestId": "request-1"}
        }));

        assert_eq!(snapshot.approvals.len(), 1);
        assert_eq!(snapshot.approvals[0].thread_id, "thr-2");
        assert_eq!(snapshot.approvals[0].request_id, "request-2");
    }

    #[test]
    fn replace_tasks_only_advances_generation_for_visible_changes() {
        let mut snapshot = Snapshot::default();
        let tasks = vec![super::Task {
            id: "thr-1".into(),
            title: "Fix ranking".into(),
            cwd: None,
            provider: None,
            updated_at: 42,
            status: TaskStatus::Working,
        }];

        snapshot.replace_tasks(tasks.clone());
        assert_eq!(snapshot.generation, 1);

        snapshot.replace_tasks(tasks);
        assert_eq!(snapshot.generation, 1);
    }

    #[test]
    fn unmatched_notification_does_not_advance_generation() {
        let mut snapshot = Snapshot::with_approval("thr-1", "request-1");
        let generation = snapshot.generation;

        snapshot.apply_notification(&json!({
            "method": "serverRequest/resolved",
            "params": {"threadId": "thr-1", "requestId": "different-request"}
        }));

        assert_eq!(snapshot.generation, generation);
        assert_eq!(snapshot.approvals.len(), 1);
    }
}

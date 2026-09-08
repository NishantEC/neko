//! A process-free projection of the Codex app-server state needed by neko's UI.
//!
//! The daemon actor owns transport and polling. This module only reduces its
//! JSON results and notifications into stable, displayable state.

use serde_json::Value;

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
        let tasks = result
            .get("data")
            .and_then(Value::as_array)
            .map(|entries| entries.iter().filter_map(parse_task).collect())
            .unwrap_or_default();

        let mut snapshot = Self {
            available: result.get("data").and_then(Value::as_array).is_some(),
            refreshed_at_unix_ms: crate::now_unix_ms(),
            ..Self::default()
        };
        snapshot.replace_tasks(tasks);
        snapshot
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
    Some(Task {
        id: entry.get("id")?.as_str()?.to_owned(),
        title: entry.get("name")?.as_str()?.to_owned(),
        cwd: entry.get("cwd").and_then(Value::as_str).map(str::to_owned),
        provider: entry
            .get("modelProvider")
            .and_then(Value::as_str)
            .map(str::to_owned),
        updated_at: entry.get("updatedAt").and_then(Value::as_i64).unwrap_or(0),
        status: TaskStatus::from_json(entry.get("status")),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Snapshot, TaskStatus};

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

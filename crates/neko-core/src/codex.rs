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
    Permissions,
    Network,
    Unknown,
}

/// An outstanding Codex approval request displayed by neko.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub thread_id: String,
    pub request_id: String,
    /// The JSON-RPC id is preserved with its original string/number type so
    /// the daemon can answer this exact app-server request.
    pub response_id: Value,
    pub title: String,
    pub detail: Option<String>,
    pub kind: ApprovalKind,
    /// The requested profile is needed to acknowledge a permissions request
    /// with the app-server's distinct `result.permissions` shape.
    pub permissions: Option<Value>,
}

/// The daemon-owned control path for one explicit decision on a current
/// Codex server request. The UI only ever reaches this through a provider;
/// it cannot manufacture a decision without a row that came from the
/// daemon's live snapshot.
pub trait CodexControl: Send + Sync {
    fn resolve_approval(
        &self,
        thread_id: &str,
        request_id: &str,
        approve: bool,
    ) -> Result<(), ProviderError>;
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
        let request_id = request_id.into();
        let mut snapshot = Self {
            available: true,
            refreshed_at_unix_ms: crate::now_unix_ms(),
            ..Self::default()
        };
        snapshot.approvals.push(Approval {
            thread_id: thread_id.into(),
            response_id: Value::String(request_id.clone()),
            request_id,
            title: "Approval required".into(),
            detail: None,
            kind: ApprovalKind::Unknown,
            permissions: None,
        });
        snapshot.generation = 1;
        snapshot
    }

    /// Whether this exact server request is still outstanding. A thread can
    /// receive another request after an older one was resolved, so checking
    /// the thread alone would authorize the wrong decision.
    pub fn can_resolve(&self, thread_id: &str, request_id: &str) -> bool {
        self.approvals
            .iter()
            .any(|approval| approval.thread_id == thread_id && approval.request_id == request_id)
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
        match notification.get("method").and_then(Value::as_str) {
            Some("item/commandExecution/requestApproval")
            | Some("item/fileChange/requestApproval")
            | Some("item/permissions/requestApproval") => {
                self.add_approval(notification);
                return;
            }
            Some("serverRequest/resolved") => {}
            _ => return,
        }

        let Some(params) = notification.get("params") else {
            return;
        };
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return;
        };
        let Some(request_id) = params.get("requestId").and_then(response_id_to_string) else {
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

    /// Removes the exact current request after its response was written to
    /// the app-server. The same pair is checked once more at the actor edge,
    /// so a stale UI action cannot consume a later request in the same thread.
    pub fn resolve(&mut self, thread_id: &str, request_id: &str) -> bool {
        let previous_len = self.approvals.len();
        self.approvals.retain(|approval| {
            approval.thread_id != thread_id || approval.request_id != request_id
        });
        let changed = self.approvals.len() != previous_len;
        if changed {
            self.generation = self.generation.saturating_add(1);
            self.refreshed_at_unix_ms = crate::now_unix_ms();
        }
        changed
    }

    /// An app-server request id is only valid for that actor session. When
    /// its stdio connection goes away, retain task history but drop pending
    /// decisions rather than offering a stale button that could target a
    /// later request after reconnect.
    pub fn clear_approvals(&mut self) {
        if !self.approvals.is_empty() {
            self.approvals.clear();
            self.generation = self.generation.saturating_add(1);
            self.refreshed_at_unix_ms = crate::now_unix_ms();
        }
    }

    fn add_approval(&mut self, notification: &Value) {
        let Some(response_id) = notification.get("id").cloned() else {
            return;
        };
        let Some(request_id) = response_id_to_string(&response_id) else {
            return;
        };
        let Some(params) = notification.get("params") else {
            return;
        };
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return;
        };
        if self.can_resolve(thread_id, &request_id) {
            return;
        }
        let method = notification
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (title, detail, kind, permissions) = match method {
            "item/commandExecution/requestApproval" => (
                params
                    .get("command")
                    .and_then(Value::as_str)
                    .map(|command| format!("Allow command: {command}"))
                    .unwrap_or_else(|| "Allow command execution".to_string()),
                params
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| {
                        params
                            .get("cwd")
                            .and_then(Value::as_str)
                            .map(|cwd| format!("In {cwd}"))
                    }),
                if params
                    .get("networkApprovalContext")
                    .is_some_and(|value| !value.is_null())
                {
                    ApprovalKind::Network
                } else {
                    ApprovalKind::CommandExecution
                },
                None,
            ),
            "item/fileChange/requestApproval" => (
                "Allow file changes".to_string(),
                params
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                ApprovalKind::FileChange,
                None,
            ),
            "item/permissions/requestApproval" => {
                let Some(permissions) = params.get("permissions").cloned() else {
                    return;
                };
                (
                    "Allow additional permissions".to_string(),
                    params
                        .get("reason")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    ApprovalKind::Permissions,
                    Some(permissions),
                )
            }
            _ => return,
        };
        self.approvals.push(Approval {
            thread_id: thread_id.to_owned(),
            request_id,
            response_id,
            title,
            detail,
            kind,
            permissions,
        });
        self.generation = self.generation.saturating_add(1);
        self.refreshed_at_unix_ms = crate::now_unix_ms();
    }
}

fn response_id_to_string(response_id: &Value) -> Option<String> {
    match response_id {
        Value::String(id) => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
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

/// Urgent, explicit decisions from the same daemon-owned Codex projection.
/// Kept separate from task tiles so an approval remains a readable list row
/// with its title/detail and explicit Actions-menu affordances.
pub struct CodexApprovalsProvider {
    snapshot: Arc<RwLock<Snapshot>>,
    control: Arc<dyn CodexControl>,
}

impl CodexApprovalsProvider {
    pub fn with_snapshot_and_control(
        snapshot: Arc<RwLock<Snapshot>>,
        control: Arc<dyn CodexControl>,
    ) -> Self {
        Self { snapshot, control }
    }
}

impl Provider for CodexApprovalsProvider {
    fn id(&self) -> &'static str {
        "codex-approval"
    }

    fn section_label(&self) -> &'static str {
        "Needs you"
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let snapshot = self.snapshot.read().unwrap().clone();
        let query = query.trim();
        snapshot
            .approvals
            .iter()
            .filter_map(|approval| {
                let score = if query.is_empty() {
                    // This is a live request for an explicit human decision,
                    // so it leads an otherwise empty palette.
                    1000.0
                } else {
                    [
                        fuzzy_score(query, &approval.title),
                        approval
                            .detail
                            .as_deref()
                            .and_then(|detail| fuzzy_score(query, detail)),
                    ]
                    .into_iter()
                    .flatten()
                    .reduce(f32::max)?
                };
                Some(Candidate {
                    score,
                    item: SearchItem {
                        // The raw app-server request id is the provider id;
                        // `perform_action` recovers the matching thread from
                        // the same current snapshot before calling control.
                        id: approval.request_id.clone(),
                        kind: self.id().to_string(),
                        title: approval.title.clone(),
                        subtitle: approval.detail.clone(),
                        icon: Icon::Glyph(Glyph::AgentLive),
                        section_label: self.section_label().to_string(),
                        action_label: "Review approval  ⌘K".to_string(),
                        badge: Some("APPROVAL".to_string()),
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions: vec![
                            ItemAction {
                                id: "approve".to_string(),
                                label: "Approve".to_string(),
                                destructive: false,
                            },
                            ItemAction {
                                id: "decline".to_string(),
                                label: "Decline".to_string(),
                                destructive: true,
                            },
                        ],
                        source: Some("Codex".to_string()),
                        meter: None,
                        keeps_open: true,
                        preview_markdown: false,
                        speaker: None,
                        images: Vec::new(),
                        preview: None,
                    },
                })
            })
            .collect()
    }

    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        Err(ProviderError(
            "choose Approve or Decline from Actions".to_string(),
        ))
    }

    fn perform_action(&self, request_id: &str, action_id: &str) -> Result<(), ProviderError> {
        let approve = match action_id {
            "approve" => true,
            "decline" => false,
            _ => {
                return Err(ProviderError(format!(
                    "no action '{action_id}' on this Codex approval"
                )))
            }
        };
        let snapshot = self.snapshot.read().unwrap();
        let matching: Vec<_> = snapshot
            .approvals
            .iter()
            .filter(|approval| approval.request_id == request_id)
            .collect();
        let [approval] = matching.as_slice() else {
            return Err(ProviderError("approval was already resolved".to_string()));
        };
        self.control
            .resolve_approval(&approval.thread_id, &approval.request_id, approve)
    }
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
        Err(ProviderError(
            "Codex task mode is not available yet".to_string(),
        ))
    }

    fn perform_action(&self, id: &str, action_id: &str) -> Result<(), ProviderError> {
        if action_id != "review" {
            return Err(ProviderError(format!(
                "no action '{action_id}' on this Codex task"
            )));
        }
        let snapshot = self.snapshot.read().unwrap();
        match snapshot.tasks.iter().find(|task| task.id == id) {
            Some(task) if task.status == TaskStatus::Waiting => Ok(()),
            Some(_) => Err(ProviderError(
                "this Codex task no longer needs review".to_string(),
            )),
            None => Err(ProviderError(
                "Codex task is no longer available".to_string(),
            )),
        }
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
    // Tiles only render title and subtitle, not a row's trailing accessory.
    // Keep the unavailable state in both places so a promoted task cannot
    // look current merely because it moved out of the result list.
    let subtitle = if available {
        subtitle
    } else {
        Some(match subtitle {
            Some(subtitle) => format!("{subtitle} · Codex unavailable"),
            None => "Codex unavailable".to_string(),
        })
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
    use std::sync::{Arc, Mutex, RwLock};

    use crate::provider::Provider;

    use super::{
        CodexApprovalsProvider, CodexControl, CodexTasksProvider, Snapshot, Task, TaskStatus,
    };

    fn snapshot_with_approval(thread_id: &str, request_id: &str) -> Snapshot {
        Snapshot::with_approval(thread_id, request_id)
    }

    #[derive(Default)]
    struct RecordingControl(Mutex<Vec<(String, String, bool)>>);

    impl CodexControl for RecordingControl {
        fn resolve_approval(
            &self,
            thread_id: &str,
            request_id: &str,
            approve: bool,
        ) -> Result<(), crate::provider::ProviderError> {
            self.0
                .lock()
                .unwrap()
                .push((thread_id.to_string(), request_id.to_string(), approve));
            Ok(())
        }
    }

    fn task(
        id: &str,
        title: &str,
        cwd: Option<&str>,
        provider: Option<&str>,
        updated_at: i64,
        status: TaskStatus,
    ) -> Task {
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
                task(
                    "working",
                    "Middle priority",
                    None,
                    None,
                    20,
                    TaskStatus::Working,
                ),
                task(
                    "waiting",
                    "Newest priority",
                    None,
                    None,
                    10,
                    TaskStatus::Waiting,
                ),
            ],
            ..Snapshot::default()
        };
        let provider = CodexTasksProvider::with_snapshot(Arc::new(RwLock::new(snapshot)));

        let candidates = provider.search("", 0);

        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.item.id.as_str())
                .collect::<Vec<_>>(),
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
            assert_eq!(
                provider.search(query, 0).len(),
                1,
                "{query} should find the task"
            );
        }
    }

    #[test]
    fn unavailable_codex_tasks_put_their_status_in_tile_visible_subtitle() {
        let snapshot = Snapshot {
            available: false,
            tasks: vec![task(
                "thr-1",
                "Repair launcher ranking",
                Some("/work/neko"),
                Some("openai"),
                10,
                TaskStatus::Idle,
            )],
            ..Snapshot::default()
        };
        let provider = CodexTasksProvider::with_snapshot(Arc::new(RwLock::new(snapshot)));

        let item = provider
            .search("", 0)
            .pop()
            .expect("the task is retained")
            .item;

        assert_eq!(
            item.subtitle.as_deref(),
            Some("openai · /work/neko · Codex unavailable")
        );
        assert_eq!(item.accessory.as_deref(), Some("Codex unavailable"));
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
            response_id: json!("request-2"),
            title: "Other approval".into(),
            detail: None,
            kind: super::ApprovalKind::Unknown,
            permissions: None,
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
    fn numeric_resolved_notification_removes_the_matching_approval() {
        let mut snapshot = Snapshot::with_approval("thr-1", "42");

        snapshot.apply_notification(&json!({
            "method": "serverRequest/resolved",
            "params": {"threadId": "thr-1", "requestId": 42}
        }));

        assert!(snapshot.approvals.is_empty());
    }

    #[test]
    fn resolving_an_old_request_does_not_resolve_a_new_one() {
        let snapshot = snapshot_with_approval("thr", "new-request");
        assert!(snapshot.can_resolve("thr", "new-request"));
        assert!(!snapshot.can_resolve("thr", "old-request"));
    }

    #[test]
    fn approval_provider_exposes_detail_and_routes_only_the_visible_request() {
        let snapshot = Arc::new(RwLock::new(snapshot_with_approval("thr", "request")));
        snapshot.write().unwrap().approvals[0].title = "Allow file changes".to_string();
        snapshot.write().unwrap().approvals[0].detail = Some("Update Cargo.toml".to_string());
        let control = Arc::new(RecordingControl::default());
        let provider = CodexApprovalsProvider::with_snapshot_and_control(snapshot, control.clone());

        let approval = provider.search("", 0).pop().expect("one approval row").item;
        assert_eq!(approval.title, "Allow file changes");
        assert_eq!(approval.subtitle.as_deref(), Some("Update Cargo.toml"));
        assert_eq!(approval.actions[0].id, "approve");
        assert!(
            approval.actions[1].destructive,
            "Decline uses the panel's existing confirmation"
        );
        provider.perform_action(&approval.id, "approve").unwrap();

        assert_eq!(
            *control.0.lock().unwrap(),
            vec![("thr".to_string(), "request".to_string(), true)]
        );
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

//! A process-free projection of the Codex app-server state needed by neko's UI.
//!
//! The daemon actor owns transport and polling. This module only reduces its
//! JSON results and notifications into stable, displayable state.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, RwLock},
};

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};
use serde_json::Value;

use crate::{
    provider::{Provider, ProviderError},
    search::{Candidate, fuzzy_score},
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
        let waiting_on_input = value
            .and_then(|status| status.get("activeFlags"))
            .and_then(Value::as_array)
            .is_some_and(|flags| {
                flags.iter().any(|flag| {
                    matches!(
                        flag.as_str(),
                        Some("waitingOnApproval") | Some("waitingOnUserInput")
                    )
                })
            });
        if waiting_on_input {
            return Self::Waiting;
        }
        let status = value.and_then(|status| {
            status
                .as_str()
                .or_else(|| status.get("type").and_then(Value::as_str))
        });
        match status {
            Some("active") | Some("working") | Some("running") => Self::Working,
            Some("waiting") | Some("needsInput") => Self::Waiting,
            Some("idle") | Some("completed") => Self::Idle,
            Some("systemError") | Some("failed") | Some("error") => Self::Failed,
            Some("notLoaded") => Self::Unknown,
            _ => Self::Unknown,
        }
    }
}

/// A Codex task row rendered by neko.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub title: String,
    /// The opening request that Codex exposes for an indexed task. This is
    /// deliberately not called a transcript: the local app-server can omit
    /// every turn while still supplying this one truthful piece of context.
    pub opening_prompt: Option<String>,
    pub cwd: Option<String>,
    pub provider: Option<String>,
    pub updated_at: i64,
    pub status: TaskStatus,
}

/// One explicit request to create a Codex task. A task never inherits a
/// daemon/client directory or creates a worktree: the selected local project
/// path is the only working directory the actor is allowed to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartTask {
    pub prompt: String,
    pub cwd: PathBuf,
}

impl StartTask {
    pub fn new(prompt: &str, cwd: Option<PathBuf>) -> Result<Self, &'static str> {
        if prompt.trim().is_empty() {
            return Err("type the task first");
        }
        let Some(cwd) = cwd else {
            return Err("choose a project first");
        };
        cwd.is_dir()
            .then_some(Self {
                prompt: prompt.trim().to_string(),
                cwd,
            })
            .ok_or("selected project is unavailable")
    }
}

/// A deliberately compact, read-only turn summary. This is not a transcript:
/// the daemon stores only the app-server's visible summary and status for the
/// task the person explicitly opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub id: String,
    pub summary: String,
    pub status: Option<String>,
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
    /// The exact JSON-RPC id. A numeric `42` and string `"42"` are separate
    /// requests, so this stays a typed JSON value all the way to the response.
    pub request_id: Value,
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
        request_id: &Value,
        approve: bool,
    ) -> Result<(), ProviderError>;

    /// Fetches the bounded recent activity for one task the person explicitly
    /// selected. Search never calls this path.
    fn open_task(&self, thread_id: &str) -> Result<(), ProviderError>;

    /// Starts a task in the explicitly selected local project. The daemon
    /// actor owns the two app-server calls; providers only validate and route.
    fn start_task(&self, start: StartTask) -> Result<(), ProviderError>;
}

/// The UI-relevant state reduced from Codex app-server messages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub tasks: Vec<Task>,
    pub approvals: Vec<Approval>,
    pub generation: u64,
    pub refreshed_at_unix_ms: i64,
    pub available: bool,
    /// True only when the app-server accepted the experimental capability
    /// negotiation needed for `thread/turns/list`.
    pub history_available: bool,
    /// Recent summaries, keyed by their exact Codex thread id.
    pub activity: HashMap<String, Vec<Activity>>,
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

    pub fn set_history_available(&mut self, available: bool) {
        if self.history_available != available {
            self.history_available = available;
            self.generation = self.generation.saturating_add(1);
        }
    }

    /// Replaces only one explicitly selected task's bounded activity. Entries
    /// without an id or visible summary are ignored; raw turn content never
    /// enters this projection.
    pub fn apply_turn_list(&mut self, thread_id: &str, value: &Value) {
        let Some(entries) = value.get("data").and_then(Value::as_array) else {
            return;
        };
        let activity = entries
            .iter()
            .take(40)
            .filter_map(parse_activity)
            .collect::<Vec<_>>();
        if self.activity.get(thread_id) != Some(&activity) || self.activity.len() != 1 {
            // The panel can show one task detail at a time. Retaining one
            // cache entry gives a selected task a stable refresh without
            // turning a long browsing session into an unbounded transcript
            // cache.
            self.activity.clear();
            self.activity.insert(thread_id.to_owned(), activity);
            self.generation = self.generation.saturating_add(1);
            self.refreshed_at_unix_ms = crate::now_unix_ms();
        }
    }

    fn prune_activity_for_current_tasks(&mut self) {
        let before = self.activity.len();
        self.activity
            .retain(|thread_id, _| self.tasks.iter().any(|task| task.id == *thread_id));
        if self.activity.len() != before {
            self.generation = self.generation.saturating_add(1);
        }
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
            request_id: Value::String(request_id),
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
        self.can_resolve_id(thread_id, &Value::String(request_id.to_owned()))
    }

    /// Whether this exact typed JSON-RPC id is still outstanding.
    pub fn can_resolve_id(&self, thread_id: &str, request_id: &Value) -> bool {
        self.approvals
            .iter()
            .any(|approval| approval.thread_id == thread_id && approval.request_id == *request_id)
    }

    /// Replaces task rows and advances the UI generation only for a visible
    /// change. Pending approvals are independent from a list refresh.
    pub fn replace_tasks(&mut self, tasks: Vec<Task>) {
        if self.tasks != tasks {
            self.tasks = tasks;
            self.generation = self.generation.saturating_add(1);
        }
        self.prune_activity_for_current_tasks();
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
        let Some(request_id) = params.get("requestId").filter(|id| is_request_id(id)) else {
            return;
        };

        let previous_len = self.approvals.len();
        self.approvals.retain(|approval| {
            approval.thread_id != thread_id || approval.request_id != *request_id
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
        self.resolve_id(thread_id, &Value::String(request_id.to_owned()))
    }

    /// Removes only the outstanding request carrying this exact typed id.
    pub fn resolve_id(&mut self, thread_id: &str, request_id: &Value) -> bool {
        let previous_len = self.approvals.len();
        self.approvals.retain(|approval| {
            approval.thread_id != thread_id || approval.request_id != *request_id
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
        let Some(request_id) = notification
            .get("id")
            .filter(|id| is_request_id(id))
            .cloned()
        else {
            return;
        };
        let Some(params) = notification.get("params") else {
            return;
        };
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return;
        };
        if self.can_resolve_id(thread_id, &request_id) {
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
                approval_detail(
                    approval_reason(params),
                    network_detail(params).or_else(|| {
                        params
                            .get("cwd")
                            .and_then(Value::as_str)
                            .map(|cwd| format!("In {cwd}"))
                    }),
                ),
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
                approval_detail(approval_reason(params), file_change_detail(params)),
                ApprovalKind::FileChange,
                None,
            ),
            "item/permissions/requestApproval" => {
                let Some(permissions) = params.get("permissions").cloned() else {
                    return;
                };
                (
                    "Allow additional permissions".to_string(),
                    approval_detail(approval_reason(params), permission_detail(&permissions)),
                    ApprovalKind::Permissions,
                    Some(permissions),
                )
            }
            _ => return,
        };
        self.approvals.push(Approval {
            thread_id: thread_id.to_owned(),
            request_id,
            title,
            detail,
            kind,
            permissions,
        });
        self.generation = self.generation.saturating_add(1);
        self.refreshed_at_unix_ms = crate::now_unix_ms();
    }
}

fn is_request_id(value: &Value) -> bool {
    matches!(value, Value::String(_) | Value::Number(_))
}

fn approval_reason(params: &Value) -> Option<String> {
    params
        .get("reason")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn approval_detail(reason: Option<String>, context: Option<String>) -> Option<String> {
    match (reason, context) {
        (Some(reason), Some(context)) => Some(format!("{reason} · {context}")),
        (Some(reason), None) => Some(reason),
        (None, Some(context)) => Some(context),
        (None, None) => None,
    }
}

fn network_detail(params: &Value) -> Option<String> {
    let context = params.get("networkApprovalContext")?;
    let host = context.get("host")?.as_str()?;
    let protocol = context.get("protocol")?.as_str()?;
    Some(format!("Allow {protocol} access to {host}"))
}

fn file_change_detail(params: &Value) -> Option<String> {
    if let Some(root) = params.get("grantRoot").and_then(Value::as_str) {
        return Some(format!("Allow writes under {root}"));
    }
    params
        .get("itemId")
        .and_then(Value::as_str)
        .map(|item| format!("File changes requested for {item}"))
}

fn permission_detail(permissions: &Value) -> Option<String> {
    let mut grants = Vec::new();
    if permissions.get("network").is_some() {
        grants.push("network access".to_string());
    }
    if let Some(filesystem) = permissions
        .get("fileSystem")
        .or_else(|| permissions.get("filesystem"))
    {
        grants.push(format!("file access ({filesystem})"));
    }
    if grants.is_empty() {
        grants.push(format!("permission profile {permissions}"));
    }
    Some(format!("Grant for this turn: {}", grants.join(", ")))
}

fn request_id_row_id(request_id: &Value) -> String {
    // JSON encoding is opaque and reversible, keeping `42` distinct from
    // `"42"` rather than making a lossy provider-row key.
    serde_json::to_string(request_id).expect("JSON-RPC request ids are JSON values")
}

fn request_id_from_row_id(id: &str) -> Option<Value> {
    let request_id: Value = serde_json::from_str(id).ok()?;
    is_request_id(&request_id).then_some(request_id)
}

fn parse_task(entry: &Value) -> Option<Task> {
    let id = entry.get("id")?.as_str()?;
    if id.trim().is_empty() {
        return None;
    }
    let title = entry
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| entry.get("preview").and_then(Value::as_str))
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Untitled task");
    let status = TaskStatus::from_json(entry.get("status"));

    Some(Task {
        id: id.to_owned(),
        title: title.to_string(),
        opening_prompt: entry
            .get("preview")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|preview| !preview.is_empty())
            .map(str::to_owned),
        cwd: entry.get("cwd").and_then(Value::as_str).map(str::to_owned),
        provider: entry
            .get("modelProvider")
            .and_then(Value::as_str)
            .map(str::to_owned),
        updated_at: entry.get("updatedAt").and_then(Value::as_i64).unwrap_or(0),
        status,
    })
}

const MAX_ACTIVITY_SUMMARY_CHARS: usize = 240;

fn parse_activity(entry: &Value) -> Option<Activity> {
    let id = entry.get("id")?.as_str()?.trim();
    let summary = entry
        .get("items")
        .and_then(first_visible_text)
        .or_else(|| entry.get("itemsView").and_then(first_visible_text))
        .or_else(|| {
            entry
                .get("summary")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            entry
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
        })?;
    let summary = summary.trim();
    if id.is_empty() || summary.is_empty() {
        return None;
    }
    let status = entry
        .get("status")
        .and_then(Value::as_str)
        .or_else(|| {
            entry
                .get("status")
                .and_then(|status| status.get("type"))
                .and_then(Value::as_str)
        })
        .map(str::to_owned);
    Some(Activity {
        id: id.to_owned(),
        summary: summary.chars().take(MAX_ACTIVITY_SUMMARY_CHARS).collect(),
        status,
    })
}

/// Extracts only readable, bounded text from Codex's visible turn items. It
/// deliberately does not stringify raw item JSON, which could expose large
/// tool payloads or arbitrary command output in the compact quick view.
fn first_visible_text(value: &Value) -> Option<String> {
    match value {
        Value::Array(values) => values.iter().find_map(first_visible_text),
        Value::Object(object) => {
            let readable_item = object
                .get("type")
                .and_then(Value::as_str)
                .map(|kind| {
                    matches!(
                        kind,
                        "userMessage" | "agentMessage" | "inputText" | "text" | "message"
                    )
                })
                .unwrap_or(true);
            if readable_item {
                for key in ["text", "summary", "title"] {
                    if let Some(text) = object.get(key).and_then(Value::as_str).map(str::trim)
                        && !text.is_empty()
                    {
                        return Some(text.to_string());
                    }
                }
            }
            for key in ["content", "items", "itemsView"] {
                if let Some(value) = object.get(key)
                    && let Some(text) = first_visible_text(value)
                {
                    return Some(text);
                }
            }
            None
        }
        _ => None,
    }
}

/// Searchable task rows backed exclusively by the daemon-owned app-server
/// projection. The app-server actor is the only writer; a palette keystroke
/// only takes this short read lock and never starts a process or reads disk.
pub struct CodexTasksProvider {
    snapshot: Arc<RwLock<Snapshot>>,
}

/// Explicit places where a new Codex task may start. The query is always the
/// task prompt, so every row remains visible while it is being composed.
pub struct CodexStartTaskProvider {
    snapshot: Arc<RwLock<Snapshot>>,
    current_project: Option<PathBuf>,
    control: Option<Arc<dyn CodexControl>>,
}

impl CodexStartTaskProvider {
    pub fn with_snapshot_and_current_project(
        snapshot: Arc<RwLock<Snapshot>>,
        current_project: Option<PathBuf>,
    ) -> Self {
        Self {
            snapshot,
            current_project,
            control: None,
        }
    }

    pub fn with_snapshot_current_project_and_control(
        snapshot: Arc<RwLock<Snapshot>>,
        current_project: Option<PathBuf>,
        control: Arc<dyn CodexControl>,
    ) -> Self {
        Self {
            snapshot,
            current_project,
            control: Some(control),
        }
    }

    fn paths(&self) -> Vec<PathBuf> {
        let snapshot = self.snapshot.read().unwrap().clone();
        let mut seen = HashSet::new();
        let mut paths = Vec::new();
        for path in snapshot
            .tasks
            .iter()
            .filter_map(|task| task.cwd.as_ref())
            .map(PathBuf::from)
            .chain(self.current_project.clone())
        {
            if path.is_dir() && seen.insert(path.clone()) {
                paths.push(path);
            }
        }
        paths
    }
}

impl Provider for CodexStartTaskProvider {
    fn id(&self) -> &'static str {
        "new-codex-task"
    }

    fn section_label(&self) -> &'static str {
        "New Codex task"
    }

    fn answers_empty_root_query(&self) -> bool {
        false
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let action_label = if query.trim().is_empty() {
            "Type the task first"
        } else {
            "Start Codex task  ↵"
        };
        let paths = self.paths();
        let len = paths.len();
        paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                let path = path.to_string_lossy().to_string();
                Candidate {
                    score: (len - index) as f32,
                    item: SearchItem {
                        id: path.clone(),
                        kind: self.id().to_string(),
                        // This must stay the full path: directory names alone
                        // make two similarly named projects indistinguishable.
                        title: path,
                        subtitle: None,
                        icon: Icon::Glyph(Glyph::Agent),
                        section_label: self.section_label().to_string(),
                        action_label: action_label.to_string(),
                        badge: Some("CODEX".to_string()),
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions: Vec::new(),
                        source: Some("Codex".to_string()),
                        meter: None,
                        keeps_open: false,
                        preview_markdown: false,
                        speaker: None,
                        images: Vec::new(),
                        preview: None,
                    },
                }
            })
            .collect()
    }

    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        Err(ProviderError(
            "starting a Codex task needs the task that was typed".to_string(),
        ))
    }

    fn activate_with_query(&self, id: &str, query: &str) -> Result<(), ProviderError> {
        let start = StartTask::new(query, Some(PathBuf::from(id)))
            .map_err(|message| ProviderError(message.to_string()))?;
        // A client cannot turn an arbitrary filesystem path into an approved
        // task location by forging an Activate frame. The path must still be
        // one of the current, visible snapshot/client-project rows.
        if !self.paths().contains(&start.cwd) {
            return Err(ProviderError("choose a project first".to_string()));
        }
        let control = self
            .control
            .as_ref()
            .ok_or_else(|| ProviderError("Codex is unavailable".to_string()))?;
        control.start_task(start)
    }
}

/// Mode-only, read-only activity for one exact selected Codex task. Its
/// search path takes the snapshot lock and performs no process or transport
/// work; explicit activation above is the sole refresh trigger.
pub struct CodexTaskProvider {
    snapshot: Arc<RwLock<Snapshot>>,
    control: Arc<dyn CodexControl>,
}

impl CodexTaskProvider {
    pub fn with_snapshot_and_control(
        snapshot: Arc<RwLock<Snapshot>>,
        control: Arc<dyn CodexControl>,
    ) -> Self {
        Self { snapshot, control }
    }
}

impl Provider for CodexTaskProvider {
    fn id(&self) -> &'static str {
        "codex-task"
    }

    fn section_label(&self) -> &'static str {
        "Recent activity"
    }

    fn search(&self, thread_id: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let snapshot = self.snapshot.read().unwrap().clone();
        let Some(task) = snapshot.tasks.iter().find(|task| task.id == thread_id) else {
            return Vec::new();
        };
        match snapshot.activity.get(thread_id) {
            Some(activity) if !activity.is_empty() => activity
                .iter()
                .enumerate()
                .map(|(index, activity)| {
                    SearchItem {
                        id: format!("{thread_id}:{}", activity.id),
                        kind: self.id().to_string(),
                        title: activity.summary.clone(),
                        subtitle: activity.status.clone(),
                        icon: Icon::Glyph(Glyph::Agent),
                        section_label: self.section_label().to_string(),
                        action_label: "Read-only".to_string(),
                        badge: None,
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions: Vec::new(),
                        source: Some("Codex".to_string()),
                        meter: None,
                        keeps_open: true,
                        preview_markdown: false,
                        speaker: None,
                        images: Vec::new(),
                        preview: None,
                    }
                    .into_candidate((40 - index) as f32)
                })
                .collect(),
            _ => vec![
                SearchItem {
                    id: thread_id.to_owned(),
                    kind: self.id().to_string(),
                    title: task.title.clone(),
                    subtitle: Some(if snapshot.history_available {
                        "No recent visible activity.".to_string()
                    } else {
                        "Codex has not exposed this task's history.".to_string()
                    }),
                    icon: Icon::Glyph(Glyph::Agent),
                    section_label: self.section_label().to_string(),
                    action_label: "Read-only".to_string(),
                    badge: None,
                    accessory: None,
                    enters_mode: None,
                    group_label: None,
                    actions: Vec::new(),
                    source: Some("Codex".to_string()),
                    meter: None,
                    keeps_open: true,
                    preview_markdown: false,
                    speaker: None,
                    images: Vec::new(),
                    preview: task.opening_prompt.clone(),
                }
                .into_candidate(1.0),
            ],
        }
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        let snapshot = self.snapshot.read().unwrap();
        if snapshot.activity.iter().any(|(thread_id, activity)| {
            activity
                .iter()
                .any(|activity| id == format!("{thread_id}:{}", activity.id))
        }) {
            // A summary is deliberately read-only. Enter is a harmless
            // no-op, not an activation error or a second actor request.
            return Ok(());
        }
        if snapshot.tasks.iter().any(|task| task.id == id) {
            drop(snapshot);
            self.control.open_task(id)
        } else {
            Err(ProviderError(
                "Codex task is no longer available".to_string(),
            ))
        }
    }

    fn perform_action(&self, thread_id: &str, action_id: &str) -> Result<(), ProviderError> {
        if action_id == "review" {
            self.activate(thread_id)
        } else {
            Err(ProviderError(format!(
                "no action '{action_id}' on this Codex task"
            )))
        }
    }
}

trait IntoCandidate {
    fn into_candidate(self, score: f32) -> Candidate;
}

impl IntoCandidate for SearchItem {
    fn into_candidate(self, score: f32) -> Candidate {
        Candidate { score, item: self }
    }
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
                        // The opaque JSON encoding preserves the JSON-RPC id
                        // type. `42` and `"42"` must never become one row.
                        id: request_id_row_id(&approval.request_id),
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
                )));
            }
        };
        let request_id = request_id_from_row_id(request_id)
            .ok_or_else(|| ProviderError("approval was already resolved".to_string()))?;
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
        "codex-tasks"
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
        // Codex's list API returns `notLoaded` for saved tasks it has not
        // hydrated. That is an honest lack of execution state, not a task in
        // an error state. Call out the useful fact — it is indexed and can be
        // opened — without presenting an alarming pseudo-status.
        TaskStatus::Unknown => (Glyph::Agent, Some("INDEXED".to_string())),
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
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, RwLock};

    use crate::provider::Provider;

    use super::{
        CodexApprovalsProvider, CodexControl, CodexStartTaskProvider, CodexTaskProvider,
        CodexTasksProvider, MAX_ACTIVITY_SUMMARY_CHARS, Snapshot, StartTask, Task, TaskStatus,
        task_item,
    };

    fn snapshot_with_approval(thread_id: &str, request_id: &str) -> Snapshot {
        Snapshot::with_approval(thread_id, request_id)
    }

    #[test]
    fn start_request_requires_a_visible_workspace_path() {
        assert_eq!(
            StartTask::new("Fix it", None).unwrap_err(),
            "choose a project first"
        );

        let project = tempfile::tempdir().expect("temporary project directory");
        assert_eq!(
            StartTask::new("", Some(project.path().to_path_buf())).unwrap_err(),
            "type the task first"
        );
    }

    #[test]
    fn start_request_uses_the_selected_path_verbatim() {
        let project = tempfile::tempdir().expect("temporary project directory");
        let selected = project.path().to_path_buf();
        let request =
            StartTask::new("Fix ranking", Some(selected.clone())).expect("valid start request");

        assert_eq!(request.cwd, selected);
    }

    #[test]
    fn start_request_rejects_a_project_that_is_no_longer_local() {
        let missing = PathBuf::from("/tmp/neko-start-task-missing-project");
        assert_eq!(
            StartTask::new("Fix it", Some(missing)).unwrap_err(),
            "selected project is unavailable"
        );
    }

    #[test]
    fn new_codex_task_lists_each_existing_snapshot_or_client_project_path_once() {
        let first = tempfile::tempdir().expect("first project");
        let second = tempfile::tempdir().expect("second project");
        let missing = PathBuf::from("/tmp/neko-codex-task-missing-project");
        let snapshot = Snapshot {
            tasks: vec![
                task(
                    "one",
                    "One",
                    first.path().to_str(),
                    None,
                    0,
                    TaskStatus::Idle,
                ),
                task(
                    "duplicate",
                    "Duplicate",
                    first.path().to_str(),
                    None,
                    0,
                    TaskStatus::Idle,
                ),
                task(
                    "missing",
                    "Missing",
                    missing.to_str(),
                    None,
                    0,
                    TaskStatus::Idle,
                ),
            ],
            ..Snapshot::default()
        };
        let provider = CodexStartTaskProvider::with_snapshot_and_current_project(
            Arc::new(RwLock::new(snapshot)),
            Some(second.path().to_path_buf()),
        );

        let rows = provider.search("Fix ranking", 0);
        let paths: Vec<_> = rows.iter().map(|row| row.item.id.as_str()).collect();
        assert_eq!(
            paths,
            [
                first.path().to_str().unwrap(),
                second.path().to_str().unwrap()
            ]
        );
        assert!(rows.iter().all(|row| row.item.title == row.item.id));
        assert!(
            rows.iter()
                .all(|row| row.item.action_label == "Start Codex task  ↵"),
            "the prompt is a task, never a path filter"
        );
    }

    #[test]
    fn new_codex_task_activation_passes_the_selected_path_and_typed_prompt_to_control() {
        let project = tempfile::tempdir().expect("project");
        let selected = project.path().to_path_buf();
        let control = Arc::new(StartControl::default());
        let provider = CodexStartTaskProvider::with_snapshot_current_project_and_control(
            Arc::new(RwLock::new(Snapshot::default())),
            Some(selected.clone()),
            control.clone(),
        );

        provider
            .activate_with_query(selected.to_str().unwrap(), "  Fix ranking  ")
            .expect("the control receives an explicit local start");

        assert_eq!(
            &*control.0.lock().unwrap(),
            &[StartTask {
                prompt: "Fix ranking".to_string(),
                cwd: selected,
            }]
        );
    }

    #[test]
    fn new_codex_task_rejects_an_existing_directory_that_was_not_visible_as_a_project() {
        let visible = tempfile::tempdir().expect("visible project");
        let arbitrary = tempfile::tempdir().expect("arbitrary directory");
        let control = Arc::new(StartControl::default());
        let snapshot = Snapshot {
            tasks: vec![task(
                "visible",
                "Visible",
                visible.path().to_str(),
                None,
                0,
                TaskStatus::Idle,
            )],
            ..Snapshot::default()
        };
        let provider = CodexStartTaskProvider::with_snapshot_current_project_and_control(
            Arc::new(RwLock::new(snapshot)),
            None,
            control.clone(),
        );

        let error = provider
            .activate_with_query(arbitrary.path().to_str().unwrap(), "Fix ranking")
            .unwrap_err();

        assert_eq!(error.to_string(), "choose a project first");
        assert!(
            control.0.lock().unwrap().is_empty(),
            "the unlisted path never reaches Codex"
        );
    }

    #[derive(Default)]
    struct RecordingControl(Mutex<Vec<(String, Value, bool)>>);

    impl CodexControl for RecordingControl {
        fn resolve_approval(
            &self,
            thread_id: &str,
            request_id: &Value,
            approve: bool,
        ) -> Result<(), crate::provider::ProviderError> {
            self.0
                .lock()
                .unwrap()
                .push((thread_id.to_string(), request_id.clone(), approve));
            Ok(())
        }

        fn open_task(&self, _thread_id: &str) -> Result<(), crate::provider::ProviderError> {
            Ok(())
        }

        fn start_task(&self, _start: StartTask) -> Result<(), crate::provider::ProviderError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct TaskControl(Mutex<Vec<String>>);

    impl CodexControl for TaskControl {
        fn resolve_approval(
            &self,
            _thread_id: &str,
            _request_id: &Value,
            _approve: bool,
        ) -> Result<(), crate::provider::ProviderError> {
            Ok(())
        }

        fn open_task(&self, thread_id: &str) -> Result<(), crate::provider::ProviderError> {
            self.0.lock().unwrap().push(thread_id.to_owned());
            Ok(())
        }

        fn start_task(&self, _start: StartTask) -> Result<(), crate::provider::ProviderError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct StartControl(Mutex<Vec<StartTask>>);

    impl CodexControl for StartControl {
        fn resolve_approval(
            &self,
            _thread_id: &str,
            _request_id: &Value,
            _approve: bool,
        ) -> Result<(), crate::provider::ProviderError> {
            Ok(())
        }

        fn open_task(&self, _thread_id: &str) -> Result<(), crate::provider::ProviderError> {
            Ok(())
        }

        fn start_task(&self, start: StartTask) -> Result<(), crate::provider::ProviderError> {
            self.0.lock().unwrap().push(start);
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
            opening_prompt: None,
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
    fn task_mode_provider_reads_only_the_selected_tasks_cached_summaries() {
        let mut snapshot = Snapshot::default();
        snapshot.replace_tasks(vec![
            task("thr-1", "One", None, None, 0, TaskStatus::Idle),
            task("thr-2", "Two", None, None, 0, TaskStatus::Idle),
        ]);
        snapshot.apply_turn_list(
            "thr-2",
            &json!({"data":[
                {"id":"two","summary":"Must stay scoped away","status":"working"},
            ]}),
        );
        snapshot.apply_turn_list(
            "thr-1",
            &json!({"data":[
                {"id":"one","summary":"Ran the focused test","status":"completed"},
            ]}),
        );
        let control = Arc::new(TaskControl::default());
        let provider = CodexTaskProvider::with_snapshot_and_control(
            Arc::new(RwLock::new(snapshot)),
            control.clone(),
        );

        let rows = provider.search("thr-1", 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].item.title, "Ran the focused test");
        assert!(provider.search("unknown", 0).is_empty());
        assert!(
            control.0.lock().unwrap().is_empty(),
            "search must not trigger a task read"
        );
        provider.activate(&rows[0].item.id).unwrap();
        assert!(
            control.0.lock().unwrap().is_empty(),
            "a read-only summary must not trigger a second task read"
        );
        provider.activate("thr-1").unwrap();
        assert_eq!(&*control.0.lock().unwrap(), &["thr-1"]);
    }

    #[test]
    fn task_activity_cache_keeps_one_selection_and_prunes_disappeared_tasks() {
        let mut snapshot = Snapshot::default();
        snapshot.replace_tasks(vec![
            task("thr-1", "One", None, None, 0, TaskStatus::Idle),
            task("thr-2", "Two", None, None, 0, TaskStatus::Idle),
        ]);
        snapshot.apply_turn_list("thr-1", &json!({"data":[{"id":"one","summary":"One"}]}));
        snapshot.apply_turn_list("thr-2", &json!({"data":[{"id":"two","summary":"Two"}]}));
        assert_eq!(
            snapshot.activity.len(),
            1,
            "only the selected task stays cached"
        );
        assert!(snapshot.activity.contains_key("thr-2"));

        snapshot.apply_thread_list(&json!({"data":[{"id":"thr-1","name":"One"}]}));
        assert!(
            snapshot.activity.is_empty(),
            "a disappeared task cannot retain cached activity"
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
    fn thread_list_retains_ui_relevant_task_fields_and_only_skips_entries_without_ids() {
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

        assert_eq!(snapshot.tasks.len(), 2);
        assert_eq!(snapshot.tasks[0].id, "thr-1");
        assert_eq!(snapshot.tasks[0].title, "Fix ranking");
        assert_eq!(snapshot.tasks[0].cwd.as_deref(), Some("/work/neko"));
        assert_eq!(snapshot.tasks[0].provider.as_deref(), Some("openai"));
        assert_eq!(snapshot.tasks[0].updated_at, 42);
        assert_eq!(snapshot.tasks[0].status, TaskStatus::Working);
        assert_eq!(snapshot.tasks[1].id, "missing-required-fields");
        assert_eq!(snapshot.tasks[1].title, "Untitled task");
    }

    #[test]
    fn thread_list_uses_the_current_codex_task_shape_including_nested_active_flags() {
        let snapshot = Snapshot::from_thread_list(&json!({"data": [
            {
                "id": "active",
                "name": null,
                "preview": "Repair task ranking",
                "cwd": "/work/neko",
                "status": {"type": "active", "activeFlags": ["waitingOnApproval"]}
            },
            {
                "id": "input",
                "name": "Answer the question",
                "status": {"type": "active", "activeFlags": ["waitingOnUserInput"]}
            },
            {"id": "broken", "name": "", "preview": "", "status": "systemError"},
            {"id": "cold", "name": null, "preview": null, "status": "notLoaded"}
        ]}));

        assert_eq!(snapshot.tasks.len(), 4);
        assert_eq!(snapshot.tasks[0].title, "Repair task ranking");
        assert_eq!(snapshot.tasks[0].status, TaskStatus::Waiting);
        assert_eq!(snapshot.tasks[1].status, TaskStatus::Waiting);
        assert_eq!(snapshot.tasks[2].title, "Untitled task");
        assert_eq!(snapshot.tasks[2].status, TaskStatus::Failed);
        assert_eq!(snapshot.tasks[3].title, "Untitled task");
        assert_eq!(snapshot.tasks[3].status, TaskStatus::Unknown);
    }

    #[test]
    fn indexed_task_does_not_claim_an_unknown_status() {
        let task = task(
            "indexed",
            "Indexed task",
            None,
            Some("openai"),
            0,
            TaskStatus::Unknown,
        );

        let item = task_item(&task, true);

        assert_eq!(item.badge.as_deref(), Some("INDEXED"));
    }

    #[test]
    fn turn_list_summarizes_visible_turn_items_not_a_legacy_top_level_summary() {
        let mut snapshot = Snapshot::default();
        snapshot.replace_tasks(vec![task("thr-1", "Task", None, None, 0, TaskStatus::Idle)]);
        snapshot.apply_turn_list("thr-1", &json!({"data": [{
            "id": "turn-1",
            "status": "completed",
            "items": [{"type": "userMessage", "content": [{"type": "inputText", "text": "Fix the ranking regression"}]}],
            "itemsView": [{"type": "agentMessage", "text": "The focused test now passes"}]
        }]}));

        assert_eq!(
            snapshot.activity["thr-1"][0].summary,
            "Fix the ranking regression"
        );
        assert_eq!(
            snapshot.activity["thr-1"][0].status.as_deref(),
            Some("completed")
        );

        let provider = CodexTaskProvider::with_snapshot_and_control(
            Arc::new(RwLock::new(snapshot)),
            Arc::new(TaskControl::default()),
        );
        assert_eq!(
            provider.search("thr-1", 0)[0].item.title,
            "Fix the ranking regression",
            "the compact task view renders the actual turn content"
        );
    }

    #[test]
    fn turn_list_falls_back_to_a_bounded_items_view_summary_without_stringifying_tool_payloads() {
        let mut snapshot = Snapshot::default();
        snapshot.replace_tasks(vec![task("thr-1", "Task", None, None, 0, TaskStatus::Idle)]);
        let visible = "x".repeat(MAX_ACTIVITY_SUMMARY_CHARS + 20);
        snapshot.apply_turn_list(
            "thr-1",
            &json!({"data": [{
                "id": "turn-1",
                "items": [{"type": "commandExecution", "text": "secret raw tool output"}],
                "itemsView": [{"type": "agentMessage", "text": visible}]
            }]}),
        );

        let summary = &snapshot.activity["thr-1"][0].summary;
        assert_eq!(summary.chars().count(), MAX_ACTIVITY_SUMMARY_CHARS);
        assert!(!summary.contains("secret raw tool output"));
    }

    #[test]
    fn thread_list_uses_fallback_titles_for_blank_names_but_skips_blank_ids() {
        let snapshot = Snapshot::from_thread_list(&json!({"data": [
            {"id": "", "name": "Has no id"},
            {"id": "   ", "name": "Has no id"},
            {"id": "thr-2", "name": ""},
            {"id": "thr-3", "name": "\t"},
            {"id": "thr-4", "name": "Visible task"}
        ]}));

        assert_eq!(snapshot.tasks.len(), 3);
        assert_eq!(snapshot.tasks[0].id, "thr-2");
        assert_eq!(snapshot.tasks[0].title, "Untitled task");
        assert_eq!(snapshot.tasks[1].id, "thr-3");
        assert_eq!(snapshot.tasks[1].title, "Untitled task");
        assert_eq!(snapshot.tasks[2].id, "thr-4");
    }

    #[test]
    fn thread_list_keeps_the_indexed_tasks_opening_prompt() {
        let snapshot = Snapshot::from_thread_list(&json!({"data": [{
            "id": "thr-1",
            "name": "Read Novo Paseo agent",
            "preview": "can you read paseo agent b74478f7"
        }]}));

        assert_eq!(
            snapshot.tasks[0].opening_prompt.as_deref(),
            Some("can you read paseo agent b74478f7"),
            "the task detail must show Codex's real opening request rather than repeat its title"
        );
    }

    #[test]
    fn indexed_task_view_carries_the_opening_request_to_the_detail_pane() {
        let snapshot = Snapshot::from_thread_list(&json!({"data": [{
            "id": "thr-1",
            "name": "Read Novo Paseo agent",
            "preview": "can you read paseo agent b74478f7"
        }]}));
        let provider = CodexTaskProvider::with_snapshot_and_control(
            Arc::new(RwLock::new(snapshot)),
            Arc::new(TaskControl::default()),
        );

        let row = provider.search("thr-1", 0).pop().expect("indexed task row").item;
        assert_eq!(row.preview.as_deref(), Some("can you read paseo agent b74478f7"));
        assert_eq!(
            row.subtitle.as_deref(),
            Some("Codex has not exposed this task's history.")
        );
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
        assert_eq!(snapshot.approvals[0].request_id, json!("request-1"));
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
            request_id: json!("request-2"),
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
        assert_eq!(snapshot.approvals[0].request_id, json!("request-2"));
    }

    #[test]
    fn numeric_resolved_notification_removes_the_matching_approval() {
        let mut snapshot = Snapshot::default();
        snapshot.apply_notification(&json!({
            "id": 42,
            "method": "item/fileChange/requestApproval",
            "params": {"threadId": "thr-1", "itemId": "item", "turnId": "turn", "startedAtMs": 1}
        }));

        snapshot.apply_notification(&json!({
            "method": "serverRequest/resolved",
            "params": {"threadId": "thr-1", "requestId": 42}
        }));

        assert!(snapshot.approvals.is_empty());
    }

    #[test]
    fn numeric_and_string_request_ids_coexist_and_resolve_only_the_exact_id() {
        let mut snapshot = Snapshot::default();
        for id in [json!(42), json!("42")] {
            snapshot.apply_notification(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "item/fileChange/requestApproval",
                "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1}
            }));
        }

        assert_eq!(snapshot.approvals.len(), 2);
        snapshot.apply_notification(&json!({
            "method": "serverRequest/resolved",
            "params": {"threadId": "thr", "requestId": 42}
        }));
        assert_eq!(snapshot.approvals.len(), 1);
        assert_eq!(snapshot.approvals[0].request_id, json!("42"));
    }

    #[test]
    fn approval_rows_and_actions_keep_numeric_and_string_ids_distinct() {
        let snapshot = Arc::new(RwLock::new(Snapshot::default()));
        for id in [json!(42), json!("42")] {
            snapshot.write().unwrap().apply_notification(&json!({
                "id": id,
                "method": "item/fileChange/requestApproval",
                "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1}
            }));
        }
        let control = Arc::new(RecordingControl::default());
        let provider = CodexApprovalsProvider::with_snapshot_and_control(snapshot, control.clone());
        let rows = provider.search("", 0);

        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| row.item.id == "42"));
        assert!(rows.iter().any(|row| row.item.id == r#""42""#));
        provider.perform_action(r#""42""#, "approve").unwrap();
        assert_eq!(control.0.lock().unwrap()[0].1, json!("42"));
    }

    #[test]
    fn approval_context_is_actionable_without_a_reason() {
        let mut snapshot = Snapshot::default();
        snapshot.apply_notification(&json!({
            "id": "network",
            "method": "item/commandExecution/requestApproval",
            "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1,
                "command": "curl", "networkApprovalContext": {"host": "api.example.com", "protocol": "https"}}
        }));
        snapshot.apply_notification(&json!({
            "id": "file",
            "method": "item/fileChange/requestApproval",
            "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1,
                "grantRoot": "/work/neko"}
        }));
        snapshot.apply_notification(&json!({
            "id": "permissions",
            "method": "item/permissions/requestApproval",
            "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1,
                "cwd": "/work/neko", "permissions": {"network": {"enabled": true}}}
        }));
        snapshot.apply_notification(&json!({
            "id": "permissions-with-reason",
            "method": "item/permissions/requestApproval",
            "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1,
                "cwd": "/work/neko", "reason": "Needed to fetch the dependency", "permissions": {"network": {"enabled": true}}}
        }));
        snapshot.apply_notification(&json!({
            "id": "permissions-file-system",
            "method": "item/permissions/requestApproval",
            "params": {"threadId": "thr", "itemId": "item", "turnId": "turn", "startedAtMs": 1,
                "cwd": "/work/neko", "permissions": {"fileSystem": "workspace-write"}}
        }));

        let detail = |id: &str| {
            snapshot
                .approvals
                .iter()
                .find(|approval| approval.request_id == json!(id))
                .and_then(|approval| approval.detail.as_deref())
                .unwrap_or_default()
        };
        assert!(detail("network").contains("https access to api.example.com"));
        assert!(detail("file").contains("/work/neko"));
        assert!(detail("permissions").contains("network access"));
        assert!(detail("permissions").contains("this turn"));
        assert!(detail("permissions-with-reason").contains("network access"));
        assert!(detail("permissions-with-reason").contains("Needed to fetch the dependency"));
        assert!(detail("permissions-file-system").contains("file access (\"workspace-write\")"));

        let provider = CodexApprovalsProvider::with_snapshot_and_control(
            Arc::new(RwLock::new(snapshot)),
            Arc::new(RecordingControl::default()),
        );
        let rows = provider.search("", 0);
        let row_detail = |title: &str| {
            rows.iter()
                .find(|row| row.item.title == title)
                .and_then(|row| row.item.subtitle.as_deref())
                .unwrap_or_default()
        };
        assert!(row_detail("Allow command: curl").contains("api.example.com"));
        assert!(row_detail("Allow file changes").contains("/work/neko"));
        assert!(row_detail("Allow additional permissions").contains("network access"));
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
            vec![("thr".to_string(), json!("request"), true)]
        );
    }

    #[test]
    fn replace_tasks_only_advances_generation_for_visible_changes() {
        let mut snapshot = Snapshot::default();
        let tasks = vec![super::Task {
            id: "thr-1".into(),
            title: "Fix ranking".into(),
            opening_prompt: None,
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

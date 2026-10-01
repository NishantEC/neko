//! Neko-owned work. Credentials are write-only and never returned in snapshots.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
pub struct Secret(pub String);
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub repository: String,
    pub instructions: String,
    /// Standing responsibility: prepare local fixes for assessed low-risk assigned bugs.
    pub away_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinearConnection {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub organization_id: String,
    pub viewer_id: String,
    /// Empty means all teams/projects visible to this explicitly connected account.
    pub team_ids: Vec<String>,
    pub project_ids: Vec<String>,
    pub enabled: bool,
    pub last_sync_ms: Option<i64>,
    pub error: Option<String>,
    /// Queue backpressure is not a provider/auth failure and must not revoke fresh evidence.
    #[serde(default)]
    pub intake_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Issue {
    pub id: String,
    pub connection_id: String,
    pub workspace_id: String,
    pub external_id: String,
    pub identifier: String,
    pub title: String,
    pub description: String,
    pub url: String,
    pub priority: u8,
    pub updated_at: String,
    /// Confirmed in the latest successful assigned-issues sync. Old caches fail closed.
    #[serde(default)]
    pub assigned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorAction {
    PrepareFix,
    AskUser,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    Medium,
    High,
    Unknown,
}

/// Model judgment, not a security guarantee. Host permission checks remain mandatory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SupervisorDecision {
    pub action: SupervisorAction,
    pub risk: Risk,
    pub is_bug: bool,
    pub reason: String,
    pub evidence: Vec<String>,
    pub files: Vec<String>,
    pub tests: Vec<String>,
    pub sensitive_areas: Vec<String>,
    pub uncertainties: Vec<String>,
    pub plan: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Planning,
    AwaitingApproval,
    Building,
    Reviewing,
    ReadyForReview,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskEvent {
    pub at_ms: i64,
    pub role: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub workspace_id: String,
    pub issue_id: Option<String>,
    pub title: String,
    pub goal: String,
    pub status: TaskStatus,
    pub plan: String,
    pub result: String,
    pub worktree: Option<String>,
    pub events: Vec<TaskEvent>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub source_revision: Option<String>,
    #[serde(default)]
    pub supervision: Option<SupervisorDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubtaskPlan {
    pub title: String,
    pub goal: String,
    pub files: Vec<String>,
    pub tests: Vec<String>,
    /// Zero-based indices of earlier subtasks. Keeps the plan acyclic.
    pub depends_on: Vec<usize>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSplit {
    pub parent_id: String,
    pub subtasks: Vec<SubtaskPlan>,
    pub approved: bool,
    pub integrated: bool,
    pub base: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRuntime {
    /// Empty means the signed-in Codex account. Local values use Codex's OSS adapter.
    #[serde(default)]
    pub provider: String,
    /// Empty lets the selected provider choose its default model.
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    #[serde(default)]
    pub agent_runtime: AgentRuntime,
    /// Every folder in a logical workspace. Legacy workspaces use `repository`.
    #[serde(default)]
    pub workspace_folders: BTreeMap<String, Vec<String>>,
    /// Immutable checkout source chosen when a task is created.
    #[serde(default)]
    pub task_roots: BTreeMap<String, String>,
    #[serde(default)]
    pub agent_profiles: crate::agent_profiles::AgentProfiles,
    #[serde(default)]
    pub schedules: Vec<crate::scheduled_plans::Schedule>,
    #[serde(default)]
    pub import_preview: crate::setup_import::ImportPreview,
    #[serde(default)]
    pub splits: Vec<TaskSplit>,
    #[serde(default)]
    pub skills: crate::skills::SkillState,
    #[serde(default = "crate::mcp_host::legacy_state")]
    pub mcp: crate::mcp_host::McpState,
    pub workspaces: Vec<Workspace>,
    pub connections: Vec<LinearConnection>,
    pub issues: Vec<Issue>,
    pub tasks: Vec<Task>,
    pub heartbeat_ms: i64,
    /// The user's conversation with Neko. Stored separately from the task
    /// store, so it never competes with capacity reserved for task results.
    #[serde(default)]
    pub conversation: Vec<ChatMessage>,
    /// What Neko has learned about the user and their workspaces. Stored
    /// separately from the task store, visible and editable by the user.
    #[serde(default)]
    pub memory: Vec<MemoryEntry>,
    /// Suggested learning requires an explicit accept or reject.
    #[serde(default)]
    pub memory_proposals: Vec<MemoryProposal>,
}

impl Snapshot {
    pub fn folders_for(&self, workspace: &Workspace) -> Vec<String> {
        self.workspace_folders
            .get(&workspace.id)
            .filter(|folders| !folders.is_empty())
            .cloned()
            .unwrap_or_else(|| vec![workspace.repository.clone()])
    }

    pub fn root_for(&self, task: &Task, workspace: &Workspace) -> String {
        self.task_roots
            .get(&task.id)
            .cloned()
            .unwrap_or_else(|| workspace.repository.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryProposal {
    pub id: String,
    pub agent_profile_id: String,
    pub workspace_id: Option<String>,
    pub kind: MemoryKind,
    pub text: String,
    pub source: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// About the user: preferences, habits, how they like work done.
    Profile,
    /// About one workspace: conventions, commands, context.
    Workspace,
    /// A decision the user made that should guide similar work later.
    Decision,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    #[serde(default = "crate::agent_profiles::default_profile_id")]
    pub agent_profile_id: String,
    pub id: String,
    pub kind: MemoryKind,
    /// Set for workspace notes and workspace-specific decisions.
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub text: String,
    /// Where it came from: "user" (typed on the Memory page) or "chat".
    pub source: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Neko,
}

/// One turn in the conversation with Neko. A Neko turn starts pending and is
/// completed (or marked failed) by the daemon; tickets it opened are linked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    #[serde(default)]
    pub agent_profile_revision: u64,
    #[serde(default = "crate::agent_profiles::default_profile_id")]
    pub agent_profile_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ChatToolCall>,
    pub id: String,
    pub at_ms: i64,
    pub role: ChatRole,
    pub text: String,
    #[serde(default)]
    pub ticket_ids: Vec<String>,
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub queued: bool,
    #[serde(default)]
    pub failed: bool,
    /// Memories Neko saved from this turn, shown under the reply.
    #[serde(default)]
    pub remembered: Vec<String>,
    /// Responsibilities Neko suggested in this turn. The daemon starts an
    /// untried suggestion when its selected source has readable tools.
    #[serde(default)]
    pub responsibility_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatToolCall {
    pub id: String,
    pub workspace_id: String,
    pub connection_id: String,
    pub tool_name: String,
    pub arguments_json: String,
    pub status: ChatToolStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatToolStatus {
    AwaitingApproval,
    Approved,
    Running,
    Succeeded,
    Failed,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    SetAgentRuntime { runtime: AgentRuntime },
    AgentProfiles(crate::agent_profiles::ProfileCommand),
    Schedules(crate::scheduled_plans::ScheduleCommand),
    SetupImport(crate::setup_import::ImportCommand),
    /// Read-only proposal; approval remains a separate explicit command.
    ProposeSplit {
        task_id: String,
    },
    Skills(crate::skills::SkillCommand),
    DecideChatTool {
        turn_id: String,
        call_id: String,
        approve: bool,
    },
    CancelChat {
        turn_id: String,
    },
    Mcp(crate::mcp_host::McpCommand),
    Snapshot,
    SaveWorkspace {
        workspace: Workspace,
    },
    /// Atomically save the logical workspace and its selected local folders.
    SaveWorkspaceWithFolders {
        workspace: Workspace,
        folders: Vec<String>,
    },
    ConnectLinear {
        workspace_id: String,
        api_key: Secret,
        team_ids: Vec<String>,
        project_ids: Vec<String>,
    },
    SyncLinear {
        connection_id: String,
    },
    SetConnectionEnabled {
        connection_id: String,
        enabled: bool,
    },
    CreateTask {
        workspace_id: String,
        title: String,
        goal: String,
    },
    /// A Neko agent chooses a specific attached folder; the daemon validates it.
    CreateTaskInFolder {
        workspace_id: String,
        title: String,
        goal: String,
        folder: String,
    },
    PlanIssue {
        issue_id: String,
    },
    ApproveTask {
        task_id: String,
    },
    CancelTask {
        task_id: String,
    },
    RetryTask {
        task_id: String,
    },
    CompleteTask {
        task_id: String,
    },
    /// Talk to Neko. The reply arrives in a later snapshot.
    SendMessage {
        text: String,
        /// Where new tickets should go when the message doesn't say.
        workspace_id: Option<String>,
    },
    /// Stop the current reply, then send this message before other queued messages.
    InterruptAndSendMessage {
        text: String,
        workspace_id: Option<String>,
    },
    /// Steer one ticket. Notes reach its planner and builder as user
    /// direction; they never grant tools or permissions.
    AddTicketNote {
        task_id: String,
        text: String,
    },
    /// Add (empty id) or replace a memory entry.
    SaveMemory {
        entry: MemoryEntry,
    },
    DeleteMemory {
        id: String,
    },
    DecideMemoryProposal {
        id: String,
        accept: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_folders_and_task_roots_survive_snapshot_round_trip() {
        let mut value = serde_json::to_value(Snapshot::default()).unwrap();
        value["workspace_folders"] = serde_json::json!({"w": ["/repo/a", "/repo/b"]});
        value["task_roots"] = serde_json::json!({"t": "/repo/b"});
        let snapshot: Snapshot = serde_json::from_value(value).unwrap();
        let saved = serde_json::to_value(snapshot).unwrap();
        assert_eq!(saved["workspace_folders"]["w"][1], "/repo/b");
        assert_eq!(saved["task_roots"]["t"], "/repo/b");
    }
    #[test]
    fn credential_debug_does_not_expose_key() {
        let command = Command::ConnectLinear {
            workspace_id: "w".into(),
            api_key: Secret("sensitive-key".into()),
            team_ids: vec![],
            project_ids: vec![],
        };
        assert!(!format!("{command:?}").contains("sensitive-key"));
    }
}

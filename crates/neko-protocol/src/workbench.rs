//! Neko-owned work. Credentials are write-only and never returned in snapshots.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Orders human replies against explicit Start/Approve actions across restarts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskReplyState {
    pub revision: String,
    pub handled: bool,
    /// Bounded unread human messages, in order. A clarification cannot erase
    /// an earlier restriction that has not reached the interpreter yet.
    #[serde(default)]
    pub pending: Vec<String>,
}

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
    /// Commit the task worktree was created from: the base of everything the
    /// ticket changed, including commits Neko makes when publishing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    /// The pull request Neko opened for this ticket's reviewed result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PullRequest>,
    /// The local `main` commit this ticket's reviewed result landed as, for
    /// repositories set to land on main instead of opening pull requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landed_commit: Option<String>,
    /// A short human name for the ticket ("Fix crash when saving a client's
    /// basic info"). Generated in the background by a tool-free model call
    /// only while `None`; `Command::RenameTask` sets it and it is then never
    /// regenerated. `title` stays the source's own wording.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    /// Set while the ticket is parked: no agent is working on it, so its
    /// folder (`worktree`, path kept) is removed and its work lives on
    /// `branch` in `repository`. Running it again restores the folder first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parked: Option<ParkedWork>,
}

/// Where a parked ticket's work lives while its folder is removed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParkedWork {
    pub branch: String,
    /// The source checkout the branch belongs to.
    pub repository: String,
    /// Uncommitted work was saved as one marked commit that restoring removes.
    pub wip: bool,
}

/// A reviewed result Neko pushed and opened for review. Neko merges it only
/// when you ask (`SetMergeWhenReady`), and never closes it on its own.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequest {
    pub url: String,
    pub branch: String,
    /// What GitHub last said about it. Read by Neko's pull request watch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PrStatus>,
    /// What the watch has already handed back to the task's agent, so one
    /// failure or comment is acted on once.
    #[serde(default, skip_serializing_if = "PrFollowUp::is_empty")]
    pub follow_up: PrFollowUp,
    /// You asked Neko to merge it: it does once CI passes and nothing conflicts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub merge_when_ready: bool,
    /// The task's agent found nothing in it worth merging (a duplicate,
    /// already fixed elsewhere), and why. Closing still waits for you.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_suggestion: Option<String>,
}

impl PullRequest {
    pub fn is_open(&self) -> bool {
        self.status.as_ref().is_none_or(|s| s.state == "OPEN")
    }
}

/// A pull request as GitHub last reported it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrStatus {
    /// "OPEN", "MERGED" or "CLOSED".
    pub state: String,
    /// "passing", "failing", "pending" or "none" (no checks configured).
    pub checks: String,
    /// Names of the checks that failed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failing: Vec<String>,
    /// "MERGEABLE", "CONFLICTING" or "UNKNOWN".
    pub mergeable: String,
    /// "APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED" or empty.
    #[serde(default)]
    pub review: String,
    /// The branch tip on GitHub.
    #[serde(default)]
    pub head: String,
    /// The branch it merges into ("main", "master").
    #[serde(default)]
    pub base: String,
    /// GitHub's `mergeStateStatus`: "CLEAN", "DIRTY" (conflicts), "BEHIND",
    /// "BLOCKED" (a review or required check), "UNSTABLE" (only optional
    /// checks failing), "DRAFT" or "UNKNOWN" (still computing).
    #[serde(default)]
    pub merge_state: String,
    /// The base branch's tip.
    #[serde(default)]
    pub base_head: String,
    /// The base branch's own checks are failing, so a failure here may not be this PR's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub base_failing: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub draft: bool,
}

/// What Neko's pull request watch has already acted on.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrFollowUp {
    /// Review comments already handed to the agent (GitHub ids, newest kept).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seen_comments: Vec<String>,
    /// The head and failing checks last handed back, so a failure is sent once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handled_failure: Option<String>,
    /// The head whose merge conflict was last handed back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handled_conflict: Option<String>,
    /// Why a requested merge is waiting, last said on the task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_note: Option<String>,
    /// Automatic hand-backs so far; past the limit the task waits for you.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub count: u32,
}

impl PrFollowUp {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

fn is_zero(n: &u32) -> bool {
    *n == 0
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
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Adapter wire ID. `default` explicitly requests normal service on Codex.
    #[serde(default)]
    pub service_tier: Option<String>,
}

/// Each pin is independent. Unpinned axes are chosen at the next run boundary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimePreferences {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub allow_paid_speed: bool,
}

/// Captured before execution; requested settings are not proof of served tier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSelection {
    pub conversation_id: String,
    pub run_id: String,
    pub runtime: AgentRuntime,
    pub preferences: RuntimePreferences,
    pub reason: String,
    pub automatic: bool,
    pub selected_at_ms: i64,
    pub catalog_read_at_ms: i64,
}

/// Default per-repository cap on tickets holding a writable run. 84 tickets
/// once made 72 worktrees in one repository, 48 of them in a day.
pub const DEFAULT_WRITABLE_PER_REPOSITORY: u32 = 4;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    #[serde(default)]
    pub agent_runtime: AgentRuntime,
    #[serde(default)]
    pub conversation_runtime: BTreeMap<String, RuntimePreferences>,
    /// Advances even for a no-op reset, so a slow check cannot undo it.
    #[serde(default)]
    pub conversation_runtime_revisions: BTreeMap<String, u64>,
    /// Most recent 200 dispatch decisions, in dispatch order.
    #[serde(default)]
    pub runtime_selections: Vec<RuntimeSelection>,
    /// Stop a ticket once its reported agent cost reaches this many cents.
    /// Only runtimes that report a price (Claude Code, OpenCode) count.
    #[serde(default)]
    pub task_budget_cents: Option<u32>,
    /// At most this many tickets have a worker at once; new ones wait their
    /// turn. `None` is no limit beyond the machine-load pacing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_running_tickets: Option<u32>,
    /// At most this many tickets per repository hold a writable run (their
    /// own worktree being changed) at once; the rest wait their turn.
    /// Read-only phases are not counted. `None` uses
    /// `DEFAULT_WRITABLE_PER_REPOSITORY`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_writable_per_repository: Option<u32>,
    /// The user's own rulebook for Neko and every agent it delegates to.
    /// `None` uses the built-in default (`rulebook_default`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rulebook: Option<String>,
    /// The built-in rulebook, attached for readers so the app can show and
    /// restore it. Never stored.
    #[serde(default)]
    pub rulebook_default: String,
    /// What this snapshot holds, as a short fingerprint (machine health and
    /// the heartbeat excluded). Attached by the daemon for readers; a client
    /// sends it back with `WorkbenchIfChanged`. Never stored.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub digest: String,
    /// The Mac's health as Neko last read it, and what that means for new
    /// work. Attached for readers; never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<MachineHealth>,
    /// Tasks the user asked to run next, in order; they start before other
    /// waiting tasks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run_next: Vec<String>,
    /// Repositories (canonical checkout paths) where Neko may merge your
    /// pull requests without the approving review GitHub requires
    /// (`gh pr merge --admin`). Off everywhere unless you switch it on.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub merge_without_review: BTreeSet<String>,
    /// A task an agent proposed in another repository, keyed by the new
    /// task's id, naming the task whose work found it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub follow_up_of: BTreeMap<String, String>,
    /// Tasks a watch filed that Neko has not triaged yet. No agent starts
    /// them until it has (or the check times out and releases them).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub untriaged: Vec<String>,
    /// When on, a planned chat or manual ticket starts building without
    /// waiting for approval. Off by default. Nothing is ever published.
    #[serde(default)]
    pub start_without_approval: bool,
    /// Off (the default): a ticket is a normal session that writes code right
    /// away, and the agent itself stops for approval only for critical work.
    /// On: every ticket first gets a read-only plan to approve.
    #[serde(default)]
    pub plan_first: bool,
    /// Tickets you started by hand (for example by dropping them on Working).
    /// Once planned they build without asking again.
    #[serde(default)]
    pub start_when_planned: BTreeSet<String>,
    /// Repositories (canonical checkout paths) whose reviewed tickets
    /// fast-forward local `main` instead of opening a pull request. Nothing
    /// is pushed for these.
    #[serde(default)]
    pub land_on_main: BTreeSet<String>,
    /// A human asked for an answer or plan without changes. Clarifications do
    /// not lift this restriction; a new work request or explicit approval does.
    #[serde(default)]
    pub task_read_only: BTreeSet<String>,
    #[serde(default)]
    pub task_replies: BTreeMap<String, TaskReplyState>,
    /// Each ticket's saved agent session (Codex thread id). Planning and
    /// building continue it so the agent keeps what it learned; reviews
    /// never use it.
    #[serde(default)]
    pub task_sessions: BTreeMap<String, String>,
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
    /// store, so it never competes with stored task plans and results.
    #[serde(default)]
    pub conversation: Vec<ChatMessage>,
    /// What Neko has learned about the user and their workspaces. Stored
    /// separately from the task store, visible and editable by the user.
    #[serde(default)]
    pub memory: Vec<MemoryEntry>,
    /// Suggested learning requires an explicit accept or reject.
    #[serde(default)]
    pub memory_proposals: Vec<MemoryProposal>,
    /// Whether Neko suggests new memories and uses memory at all.
    #[serde(default)]
    pub memory_options: MemoryOptions,
    /// Separate bounded storage; host observations cannot be injected by clients.
    #[serde(default)]
    pub decision_records: Vec<crate::decision_context::DecisionRecord>,
    /// Contextual guidance only. Keeping one never starts work or grants tools.
    #[serde(default)]
    pub working_preferences: Vec<crate::decision_context::WorkingPreference>,
}

/// Both on by default. Off means off: no suggestions are queued, and no
/// memory reaches a prompt. Saved memories are kept either way.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryOptions {
    pub learning: bool,
    pub use_memory: bool,
}

impl Default for MemoryOptions {
    fn default() -> Self {
        Self { learning: true, use_memory: true }
    }
}

impl Snapshot {
    pub fn folders_for(&self, workspace: &Workspace) -> Vec<String> {
        self.workspace_folders
            .get(&workspace.id)
            .filter(|folders| !folders.is_empty())
            .cloned()
            .unwrap_or_else(|| vec![workspace.repository.clone()])
    }

    /// The per-repository cap on tickets holding a writable run.
    pub fn writable_per_repository(&self) -> usize {
        self.max_writable_per_repository.unwrap_or(DEFAULT_WRITABLE_PER_REPOSITORY) as usize
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
    /// Actions Neko proposed on existing tasks in this turn. Each waits for
    /// the user's one-click confirmation before anything happens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ChatAction>,
    /// What Neko did while working on this turn, newest last ("Ran `git
    /// status`"), shown live under a pending reply. Bounded and short.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activity: Vec<String>,
    /// When Neko started on a reply. `at_ms` becomes the finish time once
    /// the turn completes, so the two give how long it worked. 0 = unknown.
    #[serde(default, skip_serializing_if = "is_zero_ms")]
    pub started_ms: i64,
}

fn is_zero_ms(value: &i64) -> bool { *value == 0 }

/// One reading of the Mac, and the limit it puts on new agent work.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MachineHealth {
    pub at_ms: i64,
    pub load: f64,
    pub cores: u32,
    pub free_disk_gb: f64,
    pub compressed_gb: f64,
    pub swap_used_gb: f64,
    pub swap_total_gb: f64,
    /// "normal", "warning" or "critical", as macOS reports memory pressure.
    pub pressure: String,
    /// kernel_task CPU: high when macOS is throttling to cool down.
    pub kernel_task_cpu: f64,
    /// 01:00–06:00 local: one heavy job at a time.
    pub night: bool,
    /// Why no new agent work starts right now, in plain words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holding: Option<String>,
    /// At most this many agents run at once right now, if the machine limits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<u32>,
    /// Build and test processes slowed (reniced) on the last reading.
    #[serde(default)]
    pub slowed: u32,
    /// Memory held by each kind of agent process, largest first.
    #[serde(default)]
    pub agents: Vec<AgentMemory>,
}

// Readings are finite by construction (`neko_core::machine` never stores a
// NaN), so equality is reflexive and the snapshot can stay `Eq`.
impl Eq for MachineHealth {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMemory {
    pub name: String,
    pub processes: u32,
    pub mb: u64,
}

/// Something Neko, as chief of staff, proposes doing to an existing task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatAction {
    pub id: String,
    pub task_id: String,
    /// One of `chat_action_kinds()`.
    pub kind: String,
    /// The message for `message`, the name for `rename`; empty otherwise.
    #[serde(default)]
    pub text: String,
    pub status: ChatActionStatus,
    /// Why it failed, in plain words, when it did.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChatActionStatus {
    Proposed,
    Done,
    Declined,
    Failed,
}

/// What Neko may propose doing to a task: tell its agent something, start
/// it, stop it, close it, merge it into another task as a duplicate, run it
/// next, approve its plan, accept its result, retry it or rename it, and for
/// a task with an open pull request, merge it (`merge_pr`, once CI passes) or
/// close it (`close_pr`, `text` is the comment saying why).
pub fn chat_action_kinds() -> &'static [&'static str] {
    &["message", "start", "stop", "close", "merge", "run_next", "approve", "accept", "retry", "rename", "merge_pr", "close_pr"]
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
    DecisionContext(crate::decision_context::DecisionContextCommand),
    SetAgentRuntime { runtime: AgentRuntime },
    SetConversationRuntime { conversation_id: String, preferences: RuntimePreferences },
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
    /// Start a ticket as if you dropped it on Working: approve a waiting plan,
    /// or retry a stopped ticket and build as soon as it is planned.
    StartTask {
        task_id: String,
    },
    /// Choose the repository a ticket works in, before it has a worktree.
    SetTaskFolder {
        task_id: String,
        folder: String,
    },
    /// Talk to a ticket's agent. A waiting or stopped ticket picks up your
    /// reply and decides what to do next; a ticket ready for review goes
    /// back to building with it; running work reads it at its next step.
    ReplyToTask {
        task_id: String,
        text: String,
    },
    CompleteTask {
        task_id: String,
    },
    /// Give a ticket its own short name. An empty title clears it, so Neko
    /// names it again. A name set here is never overwritten.
    RenameTask {
        task_id: String,
        title: String,
    },
    /// Commit, push and open a pull request for a reviewed result. Neko also
    /// does this on its own once an independent review passes; this retries
    /// or asks for it explicitly. Never merges.
    PublishTask {
        task_id: String,
    },
    /// Whether tickets plan read-only before working (see `Snapshot::plan_first`).
    SetPlanFirst {
        enabled: bool,
    },
    /// Choose how reviewed tickets in one repository are delivered: landed
    /// on local `main`, or (the default) opened as a pull request.
    SetLandOnMain {
        repository: String,
        enabled: bool,
    },
    /// Remove one finished (completed, failed or cancelled) ticket and its
    /// subtasks from history. Worktrees on disk and files are untouched.
    DeleteTask {
        task_id: String,
    },
    /// Remove every completed or cancelled ticket, optionally in one
    /// workspace. Failed tickets stay so they can be retried.
    ClearFinishedTasks {
        workspace_id: Option<String>,
    },
    /// Stop everything at once: every unfinished ticket and Neko's reply.
    CancelAllWork,
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
    SetMemoryOptions {
        options: MemoryOptions,
    },
    SetTaskBudget {
        cents: Option<u32>,
    },
    SetMaxRunningTickets {
        limit: Option<u32>,
    },
    /// Cap tickets writing at once in one repository; `None` is the default (4).
    SetMaxWritablePerRepository {
        limit: Option<u32>,
    },
    /// Replace the rulebook every agent works to. Empty restores the default.
    SetRulebook {
        text: String,
    },
    /// Move a waiting task to the front of the queue.
    PrioritizeTask {
        task_id: String,
    },
    /// Close a task as the same problem as another one, leaving that task a note.
    MergeTask {
        task_id: String,
        into: String,
    },
    /// Confirm or decline an action Neko proposed in chat.
    DecideChatAction {
        message_id: String,
        action_id: String,
        accept: bool,
    },
    /// Remove every ticket (keeping ones with a pull request or landed commit
    /// when `keep_published`) and forget which watched issues were filed, so
    /// watches file them again from scratch. Memory, settings, workspaces,
    /// connections and watches are kept. Refused while work is running.
    StartOver {
        keep_published: bool,
    },
    SetStartWithoutApproval {
        enabled: bool,
    },
    /// Merge a task's pull request once its checks pass and nothing
    /// conflicts (squashed, with an empty body). `false` cancels the request.
    SetMergeWhenReady {
        task_id: String,
        enabled: bool,
    },
    /// Close a task's pull request on GitHub with a comment saying why, and
    /// cancel the task. Never done without you.
    ClosePullRequest {
        task_id: String,
        comment: String,
    },
    /// Whether Neko may merge without GitHub's required approving review in
    /// one repository (see `Snapshot::merge_without_review`).
    SetMergeWithoutReview {
        repository: String,
        enabled: bool,
    },
    /// Hand every open pull request (optionally in one workspace) back to
    /// its agent to re-check against main, cut to the smallest fix, and say
    /// whether it is ready, a duplicate to close, or needs your decision.
    ReviewPullRequests {
        workspace_id: Option<String>,
    },
    /// Check every open pull request on GitHub now instead of on the timer.
    CheckPullRequests,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_snapshots_expose_empty_decision_context() {
        let mut value = serde_json::to_value(Snapshot::default()).unwrap();
        value.as_object_mut().unwrap().remove("decision_records");
        value.as_object_mut().unwrap().remove("working_preferences");
        let restored: Snapshot = serde_json::from_value(value).unwrap();
        let value = serde_json::to_value(restored).unwrap();
        assert_eq!(value["decision_records"], serde_json::json!([]));
        assert_eq!(value["working_preferences"], serde_json::json!([]));
    }

    #[test]
    fn preference_commands_are_narrow_and_round_trip() {
        let value = serde_json::json!({"DecisionContext": {"KeepPreference": {
            "workspace_id": "w", "id": "p", "expected_version": 1
        }}});
        let parsed = serde_json::from_value::<Command>(value.clone());
        assert!(parsed.is_ok(), "missing preference command: {parsed:?}");
        assert_eq!(serde_json::to_value(parsed.unwrap()).unwrap(), value);
        assert!(serde_json::from_value::<Command>(serde_json::json!({
            "DecisionContext": {"SaveDecision": {"action": "prepare_fix"}}
        })).is_err(), "clients cannot inject host observations");
        assert!(serde_json::from_value::<Command>(serde_json::json!({
            "DecisionContext": {"KeepPreference": {
                "workspace_id": "w", "id": "p", "expected_version": 1,
                "execute": true
            }}
        })).is_err(), "preference commands cannot carry execution fields");
    }
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
    fn display_names_are_optional_in_old_snapshots_and_round_trip() {
        let task = serde_json::json!({
            "id": "t", "workspace_id": "w", "issue_id": null, "title": "TypeError", "goal": "g",
            "status": "Queued", "plan": "", "result": "", "worktree": null, "events": [],
            "created_at_ms": 1, "updated_at_ms": 1
        });
        let watch = serde_json::json!({
            "id": "r", "workspace_id": "w", "instruction": "Check Sentry", "connection_ids": ["c"],
            "enabled": true, "prepare_low_risk": false, "next_due_ms": 0, "last_attempt_ms": null,
            "last_result": "", "failures": 0
        });
        let mut value = serde_json::to_value(Snapshot::default()).unwrap();
        value["tasks"] = serde_json::json!([task]);
        value["mcp"]["responsibilities"] = serde_json::json!([watch]);
        let mut snapshot: Snapshot = serde_json::from_value(value).unwrap();
        assert_eq!(snapshot.tasks[0].display_title, None);
        assert_eq!(snapshot.mcp.responsibilities[0].name, None);
        assert!(!snapshot.mcp.responsibilities[0].name_by_user);
        // Unset fields stay out of the JSON, so older readers see no change.
        let saved = serde_json::to_value(&snapshot).unwrap();
        assert!(saved["tasks"][0].get("display_title").is_none());
        assert!(saved["mcp"]["responsibilities"][0].get("name").is_none());
        assert!(saved["mcp"]["responsibilities"][0].get("name_by_user").is_none());
        snapshot.tasks[0].display_title = Some("Fix crash when saving".into());
        snapshot.mcp.responsibilities[0].name = Some("Sentry: errors".into());
        snapshot.mcp.responsibilities[0].name_by_user = true;
        let saved = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(saved["tasks"][0]["display_title"], "Fix crash when saving");
        assert_eq!(saved["mcp"]["responsibilities"][0]["name"], "Sentry: errors");
        assert_eq!(saved["mcp"]["responsibilities"][0]["name_by_user"], true);
        assert_eq!(serde_json::from_value::<Snapshot>(saved).unwrap().tasks, snapshot.tasks);
    }
    #[test]
    fn rename_commands_have_stable_json() {
        let rename = serde_json::json!({"RenameTask": {"task_id": "t", "title": "Fix login"}});
        let parsed: Command = serde_json::from_value(rename.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), rename);
        let rename = serde_json::json!({"Mcp": {"RenameResponsibility": {"responsibility_id": "r", "name": "Sentry: errors"}}});
        let parsed: Command = serde_json::from_value(rename.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), rename);
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

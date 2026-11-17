//! What the client keeps of a workbench snapshot. The multi-megabyte snapshot itself is dropped
//! as soon as it is summarised, so the client's memory does not grow with the daemon's history:
//! rows for every task, and the full detail of only the one the person has open.

use neko_protocol::crew::QueuedMessage;
use neko_protocol::goals::Goal;
use neko_protocol::workbench::{AgentRuntime, ChatMessage, ChatRole, RuntimePreferences, SelfUpdate, Snapshot, Task, TaskStatus};

/// One task as a row: what lists and counts need, and nothing of its events or plan.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TaskRow {
    pub id: String,
    /// The source's short key (`ENG-12`), when the task has one.
    pub key: Option<String>,
    pub title: String,
    pub status: TaskStatus,
    pub updated_at_ms: i64,
    /// The first line of the task's goal, cut short.
    #[serde(default)]
    pub goal: String,
    /// What the person is being waited on for (the reason icon in the sessions column).
    #[serde(default)]
    pub wait: Wait,
    /// How far along it is, as (done, of): a goal's turns used of its cap, else the plan's ticked steps of its steps.
    #[serde(default)]
    pub progress: Option<(u32, u32)>,
}

/// Why a task is waiting on the person, as the sessions column's reason icon says (polish spec §1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Wait {
    /// Not waiting on the person.
    #[default]
    None,
    /// A plan to approve.
    Plan,
    /// A question or an answer the agent needs.
    Question,
    /// A result to review, or a merge to decide.
    Review,
    /// It failed.
    Failed,
    /// A goal reached its turn or budget cap.
    Cap,
    /// Waiting, for something that is none of the above.
    Other,
}

impl Wait {
    /// Waiting on the person for a decision (a failure is a separate kind of attention).
    pub fn waits(self) -> bool {
        !matches!(self, Wait::None | Wait::Failed)
    }
}

/// The agent's own words that mean "I need an answer before I build".
const ASKED: &str = "Needs your input before building:";

/// Events that say nothing about what the task waits for.
fn is_noise(message: &str) -> bool {
    message.starts_with("USAGE ") || message.starts_with("Command ") || message.starts_with("Advisory:") || message.starts_with('{')
}

/// What `task` waits on the person for, given the goals that run on it.
pub fn wait_of(task: &Task, goals: &[Goal]) -> Wait {
    use neko_protocol::goals::GoalStatus;
    match task.status {
        TaskStatus::Failed => Wait::Failed,
        TaskStatus::Completed | TaskStatus::Cancelled => Wait::None,
        TaskStatus::ReadyForReview => Wait::Review,
        TaskStatus::AwaitingApproval => {
            let last = task.events.iter().rev().map(|e| e.message.as_str()).find(|m| !is_noise(m));
            if last.is_some_and(|m| m.starts_with(ASKED)) {
                Wait::Question
            } else if task.plan.trim().is_empty() {
                Wait::Other
            } else {
                Wait::Plan
            }
        }
        TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => {
            let goal = goals.iter().rfind(|g| g.ticket_id.as_deref() == Some(task.id.as_str()) && !g.status.is_finished());
            match goal.map(|g| g.status) {
                Some(GoalStatus::Blocked) => Wait::Question,
                Some(GoalStatus::Paused) => {
                    let why = goal.map_or(String::new(), |g| g.paused_reason.to_lowercase());
                    if why.contains("cap") || why.contains("budget") || why.contains("turn") || why.contains("limit") { Wait::Cap } else { Wait::None }
                }
                _ => Wait::None,
            }
        }
    }
}

/// `(done, of)`: a live goal's turns used of its cap; else the ticked `- [x]` steps of the plan's
/// `- [ ]`/`- [x]` steps; else nothing (the ring turns instead).
pub fn progress_of(task: &Task, goals: &[Goal]) -> Option<(u32, u32)> {
    let goal = goals.iter().rfind(|g| g.ticket_id.as_deref() == Some(task.id.as_str()) && !g.status.is_finished());
    if let Some(g) = goal.filter(|g| g.budget.max_turns > 0) {
        return Some((g.turns.min(g.budget.max_turns), g.budget.max_turns));
    }
    let (mut done, mut total) = (0u32, 0u32);
    for line in task.plan.lines() {
        let line = line.trim_start().trim_start_matches(['-', '*', '+']).trim_start();
        if line.starts_with("[x]") || line.starts_with("[X]") {
            done += 1;
            total += 1;
        } else if line.starts_with("[ ]") {
            total += 1;
        }
    }
    (total > 0).then_some((done, total))
}

/// A place work can start: the workspaces "+ New" offers.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceRow {
    pub id: String,
    pub name: String,
}

/// A workspace as Settings and the Workspaces page list it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
    pub repository: String,
    pub folders: Vec<String>,
}

/// What Settings and Usage read from the workbench, beyond the sessions.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SettingsView {
    pub runtime: AgentRuntime,
    pub task_budget_cents: Option<u32>,
    pub daily_budget_cents: Option<u32>,
    pub max_running: Option<u32>,
    pub max_writable: Option<u32>,
    pub plan_first: bool,
    pub start_without_approval: bool,
    pub self_update: Option<SelfUpdate>,
    pub workspaces: Vec<WorkspaceInfo>,
    /// Reported spend, newest 8 days: (when, millionths of a US dollar). Each priced agent run is one
    /// `USAGE` event on its task (the web's Usage page reads the same events).
    pub usage: Vec<(i64, u64)>,
    /// The person's own rulebook ("" when none: Neko's default is used) and the built-in default (Settings, Rulebook).
    #[serde(default)]
    pub rulebook: String,
    #[serde(default)]
    pub rulebook_default: String,
    /// The model and reasoning effort of each recent run (the web's Usage page, "Models").
    #[serde(default)]
    pub model_runs: Vec<(String, Option<String>)>,
}

/// How far back the client keeps priced runs (the Usage chart shows seven days).
pub const USAGE_DAYS: i64 = 8;

/// The open task in full: its events, plan and result, and what the daemon knows around it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Detail {
    pub task: Task,
    /// The task has a folder its checkpoints live in (a working copy, or parked work).
    pub has_folder: bool,
    pub workspace: String,
    /// The runtime that last ran it, as the header says it: "Claude · default".
    #[serde(default)]
    pub runtime: String,
    /// "Propose parallel work" is offered (the web's `canProposeSplit`: awaiting approval, no split yet).
    #[serde(default)]
    pub can_split: bool,
    /// The ticket's source, when a watch filed it: its identifier and its address.
    #[serde(default)]
    pub source: Option<(String, String)>,
    /// What the conversation `task:<id>` has pinned (model, effort, speed), and the revision of that pin.
    #[serde(default)]
    pub prefs: RuntimePreferences,
    #[serde(default)]
    pub prefs_revision: u64,
    /// The parallel work proposed or approved for it (the Details tab's "Parallel work").
    #[serde(default)]
    pub splits: Vec<SplitView>,
    /// What was decided about it (the Details tab's "Decisions"), newest first.
    #[serde(default)]
    pub decisions: Vec<DecisionRow>,
}

/// A parallel-work proposal (`TaskSplit`) with its subtasks' states, as the Details tab lists it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SplitView {
    pub approved: bool,
    pub integrated: bool,
    pub children: Vec<ChildView>,
}

/// One subtask of a split: what was proposed, and the task it became (if it did).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChildView {
    pub title: String,
    pub goal: String,
    /// The task made for it and its state, once the split was approved.
    pub task: Option<(String, TaskStatus)>,
}

/// One decision record of a task: when, what, and what came of it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DecisionRow {
    pub at_ms: i64,
    pub title: String,
    /// What was observed after it, when anything was.
    pub outcome: Option<String>,
    pub accepted_locally: bool,
    /// The person's own correction of it.
    pub correction: Option<String>,
}

/// The web's `decisionTitle`.
pub fn decision_title(action: neko_protocol::decision_context::DecisionAction) -> &'static str {
    use neko_protocol::decision_context::DecisionAction::*;
    match action {
        PrepareFix => "Prepare a local fix",
        AskUser => "Ask for your decision",
        Skip => "Defer this work",
        ApproveLocalBuild => "You approved a local build",
        AcceptLocalResult => "You accepted the local result",
        CancelTask => "You stopped this task",
        ReportFailure => "Work stopped",
        ReviewLocalResult => "Local work is ready to review",
        AddNote => "Your direction was recorded",
        ProposePlan => "Prepare a plan",
    }
}

/// The agent runtime's name for a task, as the Pull requests view names an author: the newest runtime chosen for it, else the default.
pub fn author_of(snapshot: &Snapshot, task_id: &str) -> String {
    let words = runtime_for(snapshot, &format!("task:{task_id}"));
    words.split(" · ").next().unwrap_or("").to_owned()
}

/// The web's `latestDecisions`: the newest version of each episode, newest first.
pub fn decisions_of(snapshot: &Snapshot, task_id: &str) -> Vec<DecisionRow> {
    let mut episodes: std::collections::BTreeMap<&str, &neko_protocol::decision_context::DecisionRecord> = std::collections::BTreeMap::new();
    for r in snapshot.decision_records.iter().filter(|r| r.task_id.as_deref() == Some(task_id)) {
        let keep = episodes.get(r.episode_id.as_str()).is_none_or(|seen| r.version > seen.version);
        if keep {
            episodes.insert(r.episode_id.as_str(), r);
        }
    }
    let mut rows: Vec<DecisionRow> = episodes
        .values()
        .map(|r| DecisionRow {
            at_ms: r.created_at_ms,
            title: decision_title(r.action).to_owned(),
            outcome: r.observed_outcome.clone().filter(|o| !o.trim().is_empty()),
            accepted_locally: r.delivery_stage == neko_protocol::decision_context::DeliveryStage::LocalAccepted,
            correction: r.correction.as_ref().map(|c| c.text.clone()).filter(|t| !t.trim().is_empty()),
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.at_ms));
    rows
}

/// The parallel work proposed for a task.
pub fn splits_of(snapshot: &Snapshot, task_id: &str) -> Vec<SplitView> {
    snapshot
        .splits
        .iter()
        .filter(|s| s.parent_id == task_id)
        .map(|s| SplitView {
            approved: s.approved,
            integrated: s.integrated,
            children: s
                .subtasks
                .iter()
                .map(|c| ChildView { title: c.title.clone(), goal: c.goal.clone(), task: c.task_id.as_ref().and_then(|id| snapshot.tasks.iter().find(|t| &t.id == id)).map(|t| (t.id.clone(), t.status)) })
                .collect(),
        })
        .collect()
}

/// Who started whom: a split's subtasks sit under their parent, a fix an agent proposed in another repository under the task that found it.
pub fn parents_of(snapshot: &Snapshot) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for split in &snapshot.splits {
        for sub in &split.subtasks {
            if let Some(id) = sub.task_id.as_ref().filter(|i| !i.is_empty()) {
                out.insert(id.clone(), split.parent_id.clone());
            }
        }
    }
    for (child, parent) in &snapshot.follow_up_of {
        out.insert(child.clone(), parent.clone());
    }
    out
}

/// What a task's menus and filters need that its row does not carry: the web's `taskActions` reads the same fields
/// (`task_roots`, the open pull request, the display title).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TaskMeta {
    pub workspace_id: String,
    /// The repository it works in (`snapshot.task_roots`), empty until it has one.
    #[serde(default)]
    pub repository: String,
    /// The pull request that is still open (the web's `openPullRequest`).
    #[serde(default)]
    pub pull_request: Option<String>,
    /// The name the person gave it (`Rename`), when they did.
    #[serde(default)]
    pub display_title: Option<String>,
    #[serde(default)]
    pub created_at_ms: i64,
}

/// A runtime in words, as the web's header chip says it (`modelLabel` with a default): "Claude · default".
pub fn runtime_words(provider: &str, model: &str) -> String {
    let name = match provider {
        "" | "codex" => "Codex".to_owned(),
        "claude" => "Claude".to_owned(),
        "opencode" => "OpenCode".to_owned(),
        other => { let mut c = other.chars(); c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str()) }
    };
    format!("{name} · {}", if model.is_empty() { "default" } else { model })
}

/// The newest runtime chosen for a conversation (`task:<id>`, `home:…`), else the workbench's own.
fn runtime_for(snapshot: &Snapshot, prefix: &str) -> String {
    let own = snapshot.runtime_selections.iter().filter(|s| s.conversation_id.starts_with(prefix)).max_by_key(|s| s.selected_at_ms);
    match own {
        Some(s) if !s.runtime.provider.is_empty() || !s.runtime.model.is_empty() => runtime_words(&s.runtime.provider, &s.runtime.model),
        _ => runtime_words(&snapshot.agent_runtime.provider, ""),
    }
}

/// The web's `shortId`: `NEK-` and the first four characters of the id, upper-cased.
pub fn short_id(id: &str) -> String {
    format!("NEK-{}", id.chars().take(4).collect::<String>().to_uppercase())
}

/// What the app keeps of a workbench snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    pub digest: String,
    /// The runtime new work uses, as words ("Codex · default model").
    #[serde(default)]
    pub runtime: String,
    /// The runtime Neko's own chat last ran on ("Claude · default").
    #[serde(default)]
    pub home_runtime: String,
    pub tasks: Vec<TaskRow>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceRow>,
    /// Every goal (a few rows, each with its ledger and journal).
    #[serde(default)]
    pub goals: Vec<Goal>,
    /// Messages waiting for a running agent, across tasks.
    #[serde(default)]
    pub queued: Vec<QueuedMessage>,
    /// The end of Neko's own conversation: the last [`CHAT_KEPT`] turns.
    #[serde(default)]
    pub conversation: Vec<ChatMessage>,
    /// The task the person has open, in full.
    #[serde(default)]
    pub detail: Option<Detail>,
    /// Settings, budgets and spend.
    #[serde(default)]
    pub settings: SettingsView,
    /// Per task: its workspace, repository, open pull request and given name.
    #[serde(default)]
    pub meta: std::collections::BTreeMap<String, TaskMeta>,
    /// The repositories whose reviewed tasks land on local main instead of opening a pull request.
    #[serde(default)]
    pub land_on_main: Vec<String>,
    /// The tool connections an `@` mention can name: (id, label, how many tools). Only enabled ones.
    #[serde(default)]
    pub connections: Vec<(String, String, usize)>,
    /// The profile Neko speaks to for the whole workbench, and the one each workspace is assigned (`profileFor`).
    #[serde(default)]
    pub active_profile: String,
    #[serde(default)]
    pub profile_of: std::collections::BTreeMap<String, String>,
    /// What each conversation has pinned (model, effort, speed) and the revision of that pin.
    #[serde(default)]
    pub pins: std::collections::BTreeMap<String, (RuntimePreferences, u64)>,
    /// What the Setup pages read: workspaces, connections, watches, skills, memory, profiles, schedules.
    #[serde(default)]
    pub setup: super::SetupData,
    /// Every pull request Neko opened, decisions first (the Pull requests view and the sidebar's count).
    #[serde(default)]
    pub pull_requests: Vec<super::PrRow>,
    /// The repositories where Neko may merge without GitHub's approving review.
    #[serde(default)]
    pub merge_without_review: Vec<String>,
    /// Who started whom: a task made by a split, or proposed by another task, by its parent's id (the Tree layout).
    #[serde(default)]
    pub parents: std::collections::BTreeMap<String, String>,
}

/// How many of Neko's messages the client keeps (a long reply is ~8 MB of layout; DESIGN: bounded). The column
/// draws the last 30 of the conversation, and "Earlier messages" reveals more of these.
pub const CHAT_KEPT: usize = 120;
/// How many of the open task's newest events the client keeps.
pub const EVENTS_KEPT: usize = 600;
/// The most a kept event message holds, so one huge command output cannot be the app's memory.
pub const EVENT_BYTES: usize = 12 * 1024;

/// The first line of a goal, at most 140 characters.
pub fn first_line(goal: &str) -> String {
    let line = goal.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.chars().count() > 140 { format!("{}…", line.chars().take(139).collect::<String>()) } else { line.to_owned() }
}

fn clip_bytes(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

impl Summary {
    pub fn from_snapshot(snapshot: &Snapshot) -> Summary {
        Summary::from_snapshot_for(snapshot, None)
    }

    /// The summary, with the full detail of `open` when it is a task the snapshot has.
    pub fn from_snapshot_for(snapshot: &Snapshot, open: Option<&str>) -> Summary {
        let skip = snapshot.conversation.len().saturating_sub(CHAT_KEPT);
        let now = neko_protocol_now_ms();
        Summary {
            settings: SettingsView {
                runtime: snapshot.agent_runtime.clone(),
                task_budget_cents: snapshot.task_budget_cents,
                daily_budget_cents: snapshot.daily_budget_cents,
                max_running: snapshot.max_running_tickets,
                max_writable: snapshot.max_writable_per_repository,
                plan_first: snapshot.plan_first,
                start_without_approval: snapshot.start_without_approval,
                self_update: Some(snapshot.self_update.clone()),
                workspaces: snapshot
                    .workspaces
                    .iter()
                    .map(|w| WorkspaceInfo { id: w.id.clone(), name: w.name.clone(), repository: w.repository.clone(), folders: snapshot.workspace_folders.get(&w.id).cloned().unwrap_or_default() })
                    .collect(),
                usage: usage_points(snapshot, now),
                rulebook: snapshot.rulebook.clone().unwrap_or_default(),
                rulebook_default: snapshot.rulebook_default.clone(),
                model_runs: snapshot.runtime_selections.iter().map(|r| (r.runtime.model.clone(), r.runtime.reasoning_effort.clone())).collect(),
            },
            setup: super::SetupData::from_snapshot(snapshot),
            pull_requests: super::prs::rows(snapshot, |t| author_of(snapshot, &t.id)),
            merge_without_review: snapshot.merge_without_review.iter().cloned().collect(),
            parents: parents_of(snapshot),
            digest: snapshot.digest.clone(),
            meta: snapshot
                .tasks
                .iter()
                .map(|t| {
                    let pull_request = t.pull_request.as_ref().filter(|p| !p.url.is_empty() && !p.status.as_ref().is_some_and(|s| matches!(s.state.as_str(), "MERGED" | "CLOSED"))).map(|p| p.url.clone());
                    (t.id.clone(), TaskMeta { workspace_id: t.workspace_id.clone(), repository: snapshot.task_roots.get(&t.id).cloned().unwrap_or_default(), pull_request, display_title: t.display_title.clone().filter(|d| !d.is_empty()), created_at_ms: t.created_at_ms })
                })
                .collect(),
            land_on_main: snapshot.land_on_main.iter().cloned().collect(),
            connections: snapshot.mcp.connections.iter().filter(|c| c.enabled).map(|c| (c.id.clone(), c.label.clone(), c.tools.len())).collect(),
            active_profile: snapshot.agent_profiles.active_profile_id.clone(),
            profile_of: snapshot.agent_profiles.assignments.iter().map(|a| (a.workspace_id.clone(), a.profile_id.clone())).collect(),
            pins: snapshot.conversation_runtime.iter().map(|(k, v)| (k.clone(), (v.clone(), snapshot.conversation_runtime_revisions.get(k).copied().unwrap_or(0)))).collect(),
            home_runtime: runtime_for(snapshot, "home:"),
            runtime: format!(
                "{} · {}",
                if snapshot.agent_runtime.provider.is_empty() { "Codex" } else { snapshot.agent_runtime.provider.as_str() },
                if snapshot.agent_runtime.model.is_empty() { "default model" } else { snapshot.agent_runtime.model.as_str() }
            ),
            tasks: snapshot
                .tasks
                .iter()
                .map(|t| TaskRow {
                    id: t.id.clone(),
                    key: t.issue_id.clone(),
                    title: t.display_title.clone().unwrap_or_else(|| t.title.clone()),
                    status: t.status,
                    updated_at_ms: t.updated_at_ms,
                    goal: first_line(&t.goal),
                    wait: wait_of(t, &snapshot.goals),
                    progress: progress_of(t, &snapshot.goals),
                })
                .collect(),
            workspaces: snapshot.workspaces.iter().map(|w| WorkspaceRow { id: w.id.clone(), name: w.name.clone() }).collect(),
            goals: snapshot.goals.iter().map(|g| Goal { ledger: g.ledger.iter().rev().take(20).rev().cloned().collect(), ..g.clone() }).collect(),
            queued: snapshot.queued_messages.clone(),
            conversation: snapshot.conversation[skip..].to_vec(),
            detail: open.and_then(|id| snapshot.tasks.iter().find(|t| t.id == id)).map(|t| {
                let mut task = t.clone();
                let skip = task.events.len().saturating_sub(EVENTS_KEPT);
                task.events.drain(..skip);
                for event in &mut task.events {
                    event.message = clip_bytes(&event.message, EVENT_BYTES);
                }
                let workspace = snapshot.workspaces.iter().find(|w| w.id == t.workspace_id).map(|w| w.name.clone()).unwrap_or_default();
                let can_split = t.status == TaskStatus::AwaitingApproval && !snapshot.splits.iter().any(|s| s.parent_id == t.id || s.subtasks.iter().any(|sub| sub.task_id.as_deref() == Some(t.id.as_str())));
                let source = t.issue_id.as_ref().and_then(|id| snapshot.issues.iter().find(|i| &i.id == id)).filter(|i| !i.url.is_empty()).map(|i| (if i.identifier.is_empty() { i.external_id.clone() } else { i.identifier.clone() }, i.url.clone()));
                let conversation = format!("task:{}", t.id);
                let prefs = snapshot.conversation_runtime.get(&conversation).cloned().unwrap_or_default();
                let prefs_revision = snapshot.conversation_runtime_revisions.get(&conversation).copied().unwrap_or(0);
                let (splits, decisions) = (splits_of(snapshot, &t.id), decisions_of(snapshot, &t.id));
                Detail { has_folder: t.worktree.is_some() || t.parked.is_some(), workspace, runtime: runtime_for(snapshot, &conversation), can_split, source, prefs, prefs_revision, splits, decisions, task }
            }),
        }
    }

    fn count(&self, f: impl Fn(TaskStatus) -> bool) -> usize {
        self.tasks.iter().filter(|t| f(t.status)).count()
    }

    /// Waiting on the person: a plan or step needs approval, a result needs review, or it failed.
    /// The sessions column's "Needs you" group counts the same.
    pub fn needs_you(&self) -> usize {
        self.count(|s| matches!(s, TaskStatus::AwaitingApproval | TaskStatus::ReadyForReview | TaskStatus::Failed))
    }

    pub fn in_review(&self) -> usize {
        self.count(|s| s == TaskStatus::ReadyForReview)
    }

    /// An agent is working on it.
    pub fn working(&self) -> usize {
        self.count(|s| matches!(s, TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing))
    }

    /// The profile Home speaks to for a workspace (the web's `profileFor`).
    pub fn profile_for(&self, workspace: Option<&str>) -> String {
        match workspace {
            Some(w) => self.profile_of.get(w).cloned().unwrap_or_else(|| "default".to_owned()),
            None if self.active_profile.is_empty() => "default".to_owned(),
            None => self.active_profile.clone(),
        }
    }

    /// The conversation Neko's own chat is in for a workspace scope: `home:<profile>:<workspace or *>`.
    pub fn home_conversation(&self, workspace: Option<&str>) -> String {
        format!("home:{}:{}", self.profile_for(workspace), workspace.unwrap_or("*"))
    }

    /// A task's workspace, repository, open pull request and given name.
    pub fn meta_of(&self, id: &str) -> Option<&TaskMeta> {
        self.meta.get(id)
    }

    /// The task's workspace id ("" when the snapshot does not say).
    pub fn workspace_of(&self, id: &str) -> &str {
        self.meta.get(id).map_or("", |m| m.workspace_id.as_str())
    }

    pub fn task(&self, id: &str) -> Option<&TaskRow> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// The messages still waiting for this task's agent, oldest first.
    pub fn queued_for(&self, id: &str) -> Vec<&QueuedMessage> {
        self.queued.iter().filter(|m| m.task_id == id && m.state == neko_protocol::crew::QueueState::Queued).collect()
    }

    /// Goals on this task: live ones and the latest finished one (the web's `TaskGoal`).
    pub fn goals_of(&self, id: &str) -> Vec<&Goal> {
        let all: Vec<&Goal> = self.goals.iter().filter(|g| g.ticket_id.as_deref() == Some(id)).collect();
        let last = all.last().copied();
        all.iter().copied().filter(|g| !g.status.is_finished() || last.is_some_and(|l| std::ptr::eq(l, *g))).collect()
    }

    /// A turn of Neko's still being written.
    pub fn replying(&self) -> bool {
        self.conversation.last().is_some_and(|m| m.role == ChatRole::Neko && m.pending)
    }
}

/// The wall clock, for deciding which priced runs are recent. (Summaries are made on the link
/// thread; a test that needs another time builds the numbers itself.)
fn neko_protocol_now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Every priced run of the last [`USAGE_DAYS`] days as (time, micro-dollars), from the `USAGE`
/// events agents leave on their tasks.
pub fn usage_points(snapshot: &Snapshot, now_ms: i64) -> Vec<(i64, u64)> {
    let since = now_ms - USAGE_DAYS * 86_400_000;
    let mut points = Vec::new();
    for task in &snapshot.tasks {
        for event in task.events.iter().filter(|e| e.at_ms >= since) {
            let Some(body) = event.message.strip_prefix("USAGE ") else { continue };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else { continue };
            let cost = value["cost_usd"].as_f64().unwrap_or(0.0);
            if cost > 0.0 && cost.is_finite() {
                points.push((event.at_ms, (cost * 1_000_000.0) as u64));
            }
        }
    }
    points.sort_unstable();
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::goals::{Budget, GoalStatus};

    fn task(status: &str, plan: &str, events: &[&str]) -> Task {
        serde_json::from_value(serde_json::json!({
            "id": "t1", "workspace_id": "w", "issue_id": null, "title": "T", "goal": "G", "status": status, "plan": plan, "result": "",
            "worktree": null, "created_at_ms": 0, "updated_at_ms": 0,
            "events": events.iter().map(|m| serde_json::json!({"at_ms": 1, "role": "scout", "message": m})).collect::<Vec<_>>(),
        }))
        .expect("a task")
    }

    fn goal(status: GoalStatus, turns: u32, max: u32, paused: &str) -> Goal {
        Goal { ticket_id: Some("t1".into()), status, turns, budget: Budget { max_turns: max, ..Budget::default() }, paused_reason: paused.into(), ..Goal::default() }
    }

    #[test]
    fn what_a_task_waits_on_the_person_for_picks_its_reason_icon() {
        assert_eq!(wait_of(&task("AwaitingApproval", "1. do it", &[]), &[]), Wait::Plan);
        assert_eq!(wait_of(&task("AwaitingApproval", "1. do it", &["Needs your input before building: which file?"]), &[]), Wait::Question);
        // Noise after the question (a usage line) does not hide it.
        assert_eq!(wait_of(&task("AwaitingApproval", "", &["Needs your input before building: which file?", "USAGE {}"]), &[]), Wait::Question);
        assert_eq!(wait_of(&task("AwaitingApproval", "", &["Plan ready."]), &[]), Wait::Other, "no plan and no question: something else");
        assert_eq!(wait_of(&task("ReadyForReview", "", &[]), &[]), Wait::Review);
        assert_eq!(wait_of(&task("Failed", "", &[]), &[]), Wait::Failed);
        assert_eq!(wait_of(&task("Completed", "", &[]), &[]), Wait::None);
        assert_eq!(wait_of(&task("Building", "", &[]), &[]), Wait::None);
        // A running ticket whose goal is blocked on a question or stopped at its cap waits on the person.
        assert_eq!(wait_of(&task("Building", "", &[]), &[goal(GoalStatus::Blocked, 2, 10, "")]), Wait::Question);
        assert_eq!(wait_of(&task("Building", "", &[]), &[goal(GoalStatus::Paused, 10, 10, "Turn cap reached")]), Wait::Cap);
        assert_eq!(wait_of(&task("Building", "", &[]), &[goal(GoalStatus::Paused, 3, 10, "Paused by you")]), Wait::None, "the person's own pause is not a wait on them");
        assert_eq!(wait_of(&task("Building", "", &[]), &[goal(GoalStatus::Done, 3, 10, "")]), Wait::None);
    }

    #[test]
    fn progress_is_the_goals_turns_else_the_plans_ticked_steps_else_nothing() {
        assert_eq!(progress_of(&task("Building", "", &[]), &[goal(GoalStatus::Running, 4, 10, "")]), Some((4, 10)));
        assert_eq!(progress_of(&task("Building", "", &[]), &[goal(GoalStatus::Running, 40, 10, "")]), Some((10, 10)), "never past its cap");
        let plan = "- [x] read\n- [x] write\n* [ ] test\n- [ ] ship\nnot a step";
        assert_eq!(progress_of(&task("Building", plan, &[]), &[]), Some((2, 4)));
        assert_eq!(progress_of(&task("Building", "1. read\n2. write", &[]), &[]), None, "no ticks: the ring turns");
        // A finished goal's turns are history.
        assert_eq!(progress_of(&task("Building", plan, &[]), &[goal(GoalStatus::Done, 9, 10, "")]), Some((2, 4)));
    }
}

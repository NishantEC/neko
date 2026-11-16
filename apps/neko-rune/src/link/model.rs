//! What the client keeps of a workbench snapshot. The multi-megabyte snapshot itself is dropped
//! as soon as it is summarised, so the client's memory does not grow with the daemon's history:
//! rows for every task, and the full detail of only the one the person has open.

use neko_protocol::crew::QueuedMessage;
use neko_protocol::goals::Goal;
use neko_protocol::workbench::{AgentRuntime, ChatMessage, ChatRole, SelfUpdate, Snapshot, Task, TaskStatus};

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
}

/// What the app keeps of a workbench snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    pub digest: String,
    /// The runtime new work uses, as words ("Codex · default model").
    #[serde(default)]
    pub runtime: String,
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
}

/// How many of Neko's turns the client keeps (a long reply is ~8 MB of layout; DESIGN: bounded).
pub const CHAT_KEPT: usize = 24;
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
            },
            digest: snapshot.digest.clone(),
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
                Detail { has_folder: t.worktree.is_some() || t.parked.is_some(), workspace, task }
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

//! The web's agent tree, in Rust: a faithful port of `buildTree`, `runsOf`, `taskCount` and the helpers they call
//! (`web/src/work/model.ts`), and of `byStage` / `flatten` (`web/src/work/filters.ts`). Who started whom: Neko on top, the
//! watches and tasks it answers for, the agent runs under each task, with cost and duration. The rules are the web's word for
//! word, so the Tree view and the Board read the same facts the web does.
//!
//! It lives in the link layer because the facts come from the full snapshot (a task's events, the splits, the watches' sources,
//! the runtime picks), which the client drops as soon as it is summarised. [`TreeInput`] is what the tree needs and nothing
//! else (a task's runs are worked out once, from its events); [`build_tree`] is cheap and runs only when the snapshot changes
//! (`Summary::from_snapshot`) or when a filter changes what the page shows, never per frame.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use neko_protocol::workbench::{RuntimeSelection, Snapshot, Task, TaskEvent, TaskStatus};

use super::model::{pr_of, repo_of, source_name, source_of};
use crate::all_sessions::Stage;

/// A dollar amount that compares by its bits, so a summary can stay `Eq`.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Usd(pub f64);

impl PartialEq for Usd {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}
impl Eq for Usd {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NodeKind {
    Neko,
    Watch,
    Task,
    Run,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NodeState {
    Running,
    UpNext,
    NeedsYou,
    Review,
    Done,
    Passed,
    ChangesAsked,
    Stopped,
    Failed,
    Watching,
    Paused,
    Failing,
}

impl NodeState {
    /// The web's `NODE_STATE_TITLE`.
    pub fn title(self) -> &'static str {
        match self {
            NodeState::Running => "Running",
            NodeState::UpNext => "Up next",
            NodeState::NeedsYou => "Needs you",
            NodeState::Review => "Ready to review",
            NodeState::Done => "Done",
            NodeState::Passed => "Passed",
            NodeState::ChangesAsked => "Changes asked",
            NodeState::Stopped => "Stopped",
            NodeState::Failed => "Failed",
            NodeState::Watching => "Watching",
            NodeState::Paused => "Paused",
            NodeState::Failing => "Failing",
        }
    }
}

/// One node of the tree: Neko, a watch, a task or an agent run (the web's `AgentNode`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentNode {
    pub id: String,
    pub kind: NodeKind,
    pub name: String,
    pub subtitle: String,
    pub work: String,
    /// "Filed from Linear · ENG-123", "Split from “…”", "You asked": a task's origin.
    pub origin: Option<String>,
    pub state: NodeState,
    pub cost: Option<Usd>,
    pub duration_ms: Option<i64>,
    pub tools: Vec<String>,
    pub task_id: Option<String>,
    pub children: Vec<AgentNode>,
}

/// A node, as the Tree and the Board's cards name it.
pub type TreeNode = AgentNode;

/// The built tree and the totals its summary line reads (the web's `AgentTree`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentTree {
    pub root: AgentNode,
    pub tasks: usize,
    pub running: usize,
    pub cost: Usd,
}

impl Default for AgentTree {
    fn default() -> Self {
        build_tree(&TreeInput::default(), &[], &Options { workspace: None, show_idle_watches: true, now: 0 })
    }
}

pub const UNRECORDED_MODEL: &str = "Model not recorded";

/// The web's `RUN_ROLES`: the roles whose events make an agent run, with the run's name and what it did.
fn run_role(role: &str) -> Option<(&'static str, &'static str)> {
    match role {
        "scout" => Some(("Scout", "Read the code and planned the work")),
        "builder" => Some(("Builder", "Made the change and checked it")),
        "reviewer" => Some(("Reviewer", "Checked the change independently")),
        "coordinator" => Some(("Recovery", "Worked out why it stopped and what to try next")),
        _ => None,
    }
}

impl AgentNode {
    /// Every node below this one, depth first (the web's `descendants`).
    pub fn descendants(&self) -> Vec<&AgentNode> {
        let mut out = Vec::new();
        for c in &self.children {
            out.push(c);
            out.extend(c.descendants());
        }
        out
    }

    /// The web's `isLive`.
    pub fn is_live(&self) -> bool {
        self.state == NodeState::Running
    }

    /// The web's `expandsByDefault`: Neko is always open; a watch or task opens only while an agent below it works.
    pub fn expands_by_default(&self) -> bool {
        match self.kind {
            NodeKind::Neko => true,
            NodeKind::Run => false,
            _ => self.descendants().iter().any(|n| n.kind == NodeKind::Run && n.is_live()),
        }
    }
}

/// The web's `visibleOrder`: ids of the nodes shown, top to bottom.
pub fn visible_order<'a>(node: &'a AgentNode, is_expanded: &dyn Fn(&AgentNode) -> bool, out: &mut Vec<&'a AgentNode>) {
    out.push(node);
    if is_expanded(node) {
        for c in &node.children {
            visible_order(c, is_expanded, out);
        }
    }
}

fn task_node_state(status: TaskStatus) -> NodeState {
    match status {
        TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => NodeState::Running,
        TaskStatus::Queued => NodeState::UpNext,
        TaskStatus::AwaitingApproval => NodeState::NeedsYou,
        TaskStatus::ReadyForReview => NodeState::Review,
        TaskStatus::Completed => NodeState::Done,
        TaskStatus::Failed => NodeState::Failed,
        TaskStatus::Cancelled => NodeState::Stopped,
    }
}

/// The web's `modelLabel`: "Claude Opus", "Codex gpt-5.5", or the provider alone.
pub fn model_label(provider: &str, model: &str) -> String {
    let upper = |s: &str| {
        let mut c = s.chars();
        c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
    };
    let name = match provider {
        "" | "codex" => "Codex".to_owned(),
        "claude" => "Claude".to_owned(),
        "opencode" => "OpenCode".to_owned(),
        other => upper(other),
    };
    if model.is_empty() {
        return name;
    }
    let shown = if model.chars().all(|c| c.is_ascii_lowercase()) { upper(model) } else { model.to_owned() };
    format!("{name} {shown}")
}

/// The web's `modelFor`: the runtime the task's conversation had picked at that moment, else its first pick.
fn model_for(task_id: &str, at: i64, selections: &[RuntimeSelection]) -> Option<String> {
    let conversation = format!("task:{task_id}");
    let own: Vec<&RuntimeSelection> = selections.iter().filter(|s| s.conversation_id == conversation).collect();
    let chosen = own.iter().rev().find(|s| s.selected_at_ms <= at).or(own.first())?;
    Some(model_label(&chosen.runtime.provider, &chosen.runtime.model))
}

fn stage_running(status: TaskStatus) -> bool {
    Stage::of(status) == Stage::Running
}

/// The web's `runsOf`: the agent runs in a task's event log, oldest first.
pub fn runs_of(task_id: &str, status: TaskStatus, events: &[TaskEvent], selections: &[RuntimeSelection], now: i64) -> Vec<AgentNode> {
    struct Run {
        role: String,
        start: i64,
        end: i64,
        cost: Option<f64>,
        closed: bool,
        verdict: Option<bool>,
        stopped_by: Option<String>,
    }
    let mut runs: Vec<Run> = Vec::new();
    for event in events {
        let (role, message, at) = (event.role.as_str(), event.message.as_str(), event.at_ms);
        if run_role(role).is_none() {
            // Anything else (Neko, the host, you) ends the run in progress.
            if let Some(last) = runs.last_mut().filter(|l| !l.closed) {
                last.closed = true;
                if role == "supervisor" {
                    last.stopped_by = Some(message.to_owned());
                }
            }
            continue;
        }
        let restart = message.starts_with("Agent started");
        let continues = runs.last().is_some_and(|l| l.role == role && !l.closed) && !restart;
        if continues {
            runs.last_mut().expect("checked").end = at;
        } else {
            runs.push(Run { role: role.to_owned(), start: at, end: at, cost: None, closed: false, verdict: None, stopped_by: None });
        }
        let last = runs.last_mut().expect("just pushed or checked");
        if let Some(body) = message.strip_prefix("USAGE ") {
            if let Some(cost) = serde_json::from_str::<serde_json::Value>(body).ok().and_then(|u| u.get("cost_usd").filter(|c| c.is_number()).and_then(|c| c.as_f64())) {
                last.cost = Some(last.cost.unwrap_or(0.0) + cost);
                last.closed = true;
            }
        } else if let Some(passed) = (role == "reviewer" && message.starts_with('{')).then(|| serde_json::from_str::<serde_json::Value>(message).ok().and_then(|v| v.get("passed").and_then(|p| p.as_bool()))).flatten() {
            last.verdict = Some(passed);
        }
    }
    let live = stage_running(status);
    let count = runs.len();
    runs.into_iter()
        .enumerate()
        .map(|(index, run)| {
            let is_last = index + 1 == count;
            let state = if is_last && live && !run.closed {
                NodeState::Running
            } else if let Some(passed) = run.verdict {
                if passed { NodeState::Passed } else { NodeState::ChangesAsked }
            } else if run.cost.is_none() && ((is_last && matches!(status, TaskStatus::Failed | TaskStatus::Cancelled)) || run.stopped_by.as_deref().is_some_and(|s| !s.starts_with("Result ready") && !s.starts_with("Opened") && !s.starts_with("Plan ready"))) {
                NodeState::Stopped
            } else {
                NodeState::Done
            };
            let end = if state == NodeState::Running { now } else { run.end };
            let (name, work) = run_role(&run.role).expect("a run has a known role");
            AgentNode {
                id: format!("{task_id}#{index}"),
                kind: NodeKind::Run,
                name: name.to_owned(),
                subtitle: model_for(task_id, run.start, selections).unwrap_or_else(|| UNRECORDED_MODEL.to_owned()),
                work: work.to_owned(),
                origin: None,
                state,
                cost: run.cost.map(Usd),
                duration_ms: (end >= run.start).then_some(end - run.start),
                tools: Vec::new(),
                task_id: Some(task_id.to_owned()),
                children: Vec::new(),
            }
        })
        .collect()
}

/// One task as the tree needs it (its runs are already worked out from its events).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TreeTask {
    pub id: String,
    pub workspace_id: String,
    pub status: TaskStatus,
    pub title: String,
    pub display_title: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
    /// `repositoryOf`, `pullRequestNumber` and `sourceOf`, empty when it has none.
    pub repo: String,
    pub pr: String,
    pub source: String,
    pub runs: Vec<AgentNode>,
}

impl TreeTask {
    /// The web's `taskTitle`.
    pub fn name(&self) -> &str {
        match self.display_title.as_deref().map(str::trim) {
            Some(d) if !d.is_empty() => d,
            _ => &self.title,
        }
    }
}

/// One watch (`snapshot.mcp.responsibilities`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TreeWatch {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub instruction: String,
    pub enabled: bool,
    pub failures: u32,
    pub connection_ids: Vec<String>,
}

/// Everything `build_tree` reads that is not a task's own fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TreeInput {
    pub tasks: Vec<TreeTask>,
    /// Who started whom: a split's subtasks under their parent, then a follow-up under the task that found it.
    pub parent_of: BTreeMap<String, String>,
    pub follow_up_of: BTreeSet<String>,
    /// The watch that filed a task, and the connection it read it from.
    pub watch_of: BTreeMap<String, String>,
    pub source_connection: BTreeMap<String, String>,
    pub from_chat: BTreeSet<String>,
    pub connection_names: BTreeMap<String, String>,
    pub watches: Vec<TreeWatch>,
}

impl TreeInput {
    /// What the tree reads of a snapshot. `now` ends the runs still going.
    pub fn from_snapshot(snapshot: &Snapshot, now: i64) -> (TreeInput, AgentTree) {
        let tasks = snapshot
            .tasks
            .iter()
            .map(|t: &Task| TreeTask {
                id: t.id.clone(),
                workspace_id: t.workspace_id.clone(),
                status: t.status,
                title: t.title.clone(),
                display_title: t.display_title.clone(),
                created_ms: t.created_at_ms,
                updated_ms: t.updated_at_ms,
                repo: repo_of(snapshot, t),
                pr: pr_of(t),
                source: source_of(snapshot, t),
                runs: runs_of(&t.id, t.status, &t.events, &snapshot.runtime_selections, now),
            })
            .collect();
        let mut parent_of = BTreeMap::new();
        for split in &snapshot.splits {
            for sub in &split.subtasks {
                if let Some(id) = sub.task_id.as_ref().filter(|i| !i.is_empty()) {
                    parent_of.insert(id.clone(), split.parent_id.clone());
                }
            }
        }
        let mut follow_up_of = BTreeSet::new();
        for (child, parent) in &snapshot.follow_up_of {
            follow_up_of.insert(child.clone());
            parent_of.insert(child.clone(), parent.clone());
        }
        let mut watch_of = BTreeMap::new();
        let mut source_connection = BTreeMap::new();
        for source in &snapshot.mcp.sources {
            if let Some(id) = source.task_id.as_ref().filter(|i| !i.is_empty()) {
                watch_of.insert(id.clone(), source.responsibility_id.clone());
                source_connection.entry(id.clone()).or_insert_with(|| source.connection_id.clone());
            }
        }
        let input = TreeInput {
            tasks,
            parent_of,
            follow_up_of,
            watch_of,
            source_connection,
            from_chat: snapshot.conversation.iter().flat_map(|m| m.ticket_ids.iter().cloned()).collect(),
            connection_names: snapshot.mcp.connections.iter().map(|c| (c.id.clone(), source_name(&c.label))).collect(),
            watches: snapshot
                .mcp
                .responsibilities
                .iter()
                .map(|w| TreeWatch {
                    id: w.id.clone(),
                    workspace_id: w.workspace_id.clone(),
                    name: w.name.clone().unwrap_or_default(),
                    instruction: w.instruction.clone(),
                    enabled: w.enabled,
                    failures: w.failures,
                    connection_ids: w.connection_ids.clone(),
                })
                .collect(),
        };
        // The Tree page's own call: every workspace, failed and cancelled included, no search.
        let visible = flatten(&by_stage(&input.tasks, &Filters { include_stopped: true, ..Filters::default() }));
        let tree = build_tree(&input, &visible, &Options { workspace: None, show_idle_watches: true, now });
        (input, tree)
    }
}

impl TreeInput {
    /// The tree's input from rows alone (no events, so no runs): for a summary that was not made from a snapshot (the demo scenes).
    pub fn from_rows(tasks: &[super::model::TaskRow], meta: &BTreeMap<String, super::model::TaskMeta>, parents: &BTreeMap<String, String>) -> TreeInput {
        TreeInput {
            tasks: tasks
                .iter()
                .map(|t| {
                    let m = meta.get(&t.id).cloned().unwrap_or_default();
                    TreeTask {
                        id: t.id.clone(),
                        workspace_id: m.workspace_id,
                        status: t.status,
                        title: t.title.clone(),
                        display_title: None,
                        created_ms: m.created_at_ms,
                        updated_ms: t.updated_at_ms,
                        repo: t.line.repo.clone(),
                        pr: t.line.pr.clone(),
                        source: t.line.source.clone(),
                        runs: Vec::new(),
                    }
                })
                .collect(),
            parent_of: parents.clone(),
            ..TreeInput::default()
        }
    }
}

/// The web's `Filters`.
#[derive(Clone, Debug, Default)]
pub struct Filters {
    /// A workspace id, or "" for all.
    pub workspace: String,
    pub stage: Option<Stage>,
    pub search: String,
    pub include_stopped: bool,
}

/// The web's `matches`.
pub fn matches(task: &TreeTask, f: &Filters) -> bool {
    if !f.workspace.is_empty() && task.workspace_id != f.workspace {
        return false;
    }
    if f.stage.is_some_and(|s| Stage::of(task.status) != s) {
        return false;
    }
    if !f.include_stopped && matches!(task.status, TaskStatus::Failed | TaskStatus::Cancelled) {
        return false;
    }
    let q = f.search.trim().to_lowercase();
    !(!q.is_empty() && !task.name().to_lowercase().contains(&q) && !task.title.to_lowercase().contains(&q))
}

/// The web's `byStage`: tasks per column (`COLUMNS`), filtered, newest first.
pub fn by_stage<'a>(tasks: &'a [TreeTask], f: &Filters) -> [Vec<&'a TreeTask>; 5] {
    let mut out: [Vec<&TreeTask>; 5] = Default::default();
    for task in tasks.iter().filter(|t| matches(t, f)) {
        out[stage_index(Stage::of(task.status))].push(task);
    }
    for list in &mut out {
        list.sort_by_key(|t| std::cmp::Reverse(t.updated_ms));
    }
    out
}

/// The web's `flatten`.
pub fn flatten<'a>(grouped: &[Vec<&'a TreeTask>; 5]) -> Vec<&'a TreeTask> {
    grouped.iter().flatten().copied().collect()
}

fn stage_index(stage: Stage) -> usize {
    Stage::ALL.iter().position(|s| *s == stage).expect("every stage is in ALL")
}

/// The web's `taskCount`: "1 task", "3 tasks".
pub fn task_count(n: usize) -> String {
    format!("{n} {}", if n == 1 { "task" } else { "tasks" })
}

/// The web's `formatCost`: "$0.27", "$12", or "<$0.01".
pub fn format_cost(usd: f64) -> String {
    if usd > 0.0 && usd < 0.01 {
        return "<$0.01".to_owned();
    }
    if usd >= 100.0 { format!("${usd:.0}") } else { format!("${usd:.2}") }
}

/// The web's `formatDuration`: "18s", "1m 30s", "2h 05m", "3d".
pub fn format_duration(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {}s", s / 60, s % 60)
    } else if s < 86_400 {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}d", s / 86_400)
    }
}

/// The web's `treeSummary`.
pub fn tree_summary(tree: &AgentTree) -> String {
    format!("{} · {} running · {} reported", task_count(tree.tasks), tree.running, format_cost(tree.cost.0))
}

/// What `buildTree` is asked.
pub struct Options {
    pub workspace: Option<String>,
    pub show_idle_watches: bool,
    pub now: i64,
}

fn task_order(a: &TreeTask, b: &TreeTask) -> std::cmp::Ordering {
    let rank = |t: &TreeTask| match Stage::of(t.status) {
        Stage::Running => 0,
        Stage::NeedsYou => 1,
        Stage::Review => 2,
        Stage::UpNext => 3,
        Stage::Done => 4,
    };
    rank(a).cmp(&rank(b)).then(b.updated_ms.cmp(&a.updated_ms))
}

fn sum(values: impl Iterator<Item = f64>) -> f64 {
    values.fold(0.0, |a, b| a + b)
}

/// The web's `buildTree`: the tree for `tasks` (already filtered to what the page shows).
pub fn build_tree(input: &TreeInput, tasks: &[&TreeTask], opts: &Options) -> AgentTree {
    let now = opts.now;
    let visible: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    let by_id: HashMap<&str, &TreeTask> = tasks.iter().map(|t| (t.id.as_str(), *t)).collect();
    let parent_of = |id: &str| input.parent_of.get(id).map(String::as_str);

    fn task_node(input: &TreeInput, tasks: &[&TreeTask], by_id: &HashMap<&str, &TreeTask>, task: &TreeTask, now: i64) -> AgentNode {
        let id = task.id.as_str();
        let runs = task.runs.clone();
        let mut kids: Vec<&TreeTask> = tasks.iter().copied().filter(|t| input.parent_of.get(&t.id).map(String::as_str) == Some(id)).collect();
        kids.sort_by(|a, b| task_order(a, b));
        let split_children: Vec<AgentNode> = kids.into_iter().map(|t| task_node(input, tasks, by_id, t, now)).collect();
        let own: Vec<f64> = runs.iter().filter_map(|r| r.cost.map(|c| c.0)).collect();
        let below: Vec<f64> = split_children.iter().filter_map(|c| c.cost.map(|c| c.0)).collect();
        let cost = (!own.is_empty() || !below.is_empty()).then(|| Usd(sum(own.into_iter().chain(below))));
        let ended = if stage_running(task.status) { now } else { task.updated_ms };
        let origin = match input.parent_of.get(id).and_then(|p| by_id.get(p.as_str())) {
            Some(parent) => {
                if input.follow_up_of.contains(id) { format!("Proposed by “{}”", parent.name()) } else { format!("Split from “{}”", parent.name()) }
            }
            None if input.watch_of.contains_key(id) => {
                if task.source.is_empty() { "Filed by a watch".to_owned() } else { format!("Filed from {}", task.source) }
            }
            None if input.from_chat.contains(id) => "From your chat with Neko".to_owned(),
            None => "You asked".to_owned(),
        };
        let tool = input.watch_of.contains_key(id).then(|| input.source_connection.get(id)).flatten().and_then(|c| input.connection_names.get(c)).cloned();
        AgentNode {
            id: id.to_owned(),
            kind: NodeKind::Task,
            name: task.name().to_owned(),
            subtitle: runs.iter().rev().find(|r| r.subtitle != UNRECORDED_MODEL).map(|r| r.subtitle.clone()).unwrap_or_default(),
            work: [task.repo.as_str(), task.pr.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · "),
            origin: Some(origin),
            state: task_node_state(task.status),
            cost,
            duration_ms: (task.created_ms > 0 && ended >= task.created_ms).then_some(ended - task.created_ms),
            tools: tool.into_iter().collect(),
            task_id: Some(id.to_owned()),
            children: runs.into_iter().chain(split_children).collect(),
        }
    }

    // Tasks a split started sit under their parent, not at the top.
    let top_level: Vec<&TreeTask> = tasks.iter().copied().filter(|t| parent_of(&t.id).is_none_or(|p| !visible.contains(p))).collect();

    let mut watch_nodes: Vec<AgentNode> = Vec::new();
    for watch in &input.watches {
        if opts.workspace.as_deref().is_some_and(|w| !w.is_empty() && watch.workspace_id != w) {
            continue;
        }
        let mut mine: Vec<&TreeTask> = top_level.iter().copied().filter(|t| input.watch_of.get(&t.id).is_some_and(|w| *w == watch.id)).collect();
        mine.sort_by(|a, b| task_order(a, b));
        let filed: Vec<AgentNode> = mine.into_iter().map(|t| task_node(input, tasks, &by_id, t, now)).collect();
        if filed.is_empty() && !(opts.show_idle_watches && watch.enabled) {
            continue;
        }
        let name = if watch.name.is_empty() { watch.instruction.chars().take(60).collect() } else { watch.name.clone() };
        let costs: Vec<f64> = filed.iter().filter_map(|c| c.cost.map(|c| c.0)).collect();
        let mut tools: Vec<String> = Vec::new();
        for c in &watch.connection_ids {
            let n = input.connection_names.get(c).cloned().unwrap_or_default();
            if !n.is_empty() && !tools.contains(&n) {
                tools.push(n);
            }
        }
        watch_nodes.push(AgentNode {
            id: format!("watch:{}", watch.id),
            kind: NodeKind::Watch,
            name,
            subtitle: "Watch".to_owned(),
            work: watch.instruction.trim().to_owned(),
            origin: None,
            state: if !watch.enabled {
                NodeState::Paused
            } else if watch.failures > 0 {
                NodeState::Failing
            } else {
                NodeState::Watching
            },
            cost: (!costs.is_empty()).then(|| Usd(sum(costs.into_iter()))),
            duration_ms: None,
            tools,
            task_id: None,
            children: filed,
        });
    }
    // Watches with live work first, then the busiest.
    let live_below = |n: &AgentNode| if n.descendants().iter().any(|d| d.is_live()) { 0 } else { 1 };
    watch_nodes.sort_by(|a, b| live_below(a).cmp(&live_below(b)).then(b.children.len().cmp(&a.children.len())).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.name.cmp(&b.name))));

    let mut direct: Vec<&TreeTask> = top_level
        .iter()
        .copied()
        .filter(|t| match input.watch_of.get(&t.id) {
            None => true,
            Some(w) => !watch_nodes.iter().any(|n| n.id == format!("watch:{w}")),
        })
        .collect();
    direct.sort_by(|a, b| task_order(a, b));
    let direct: Vec<AgentNode> = direct.into_iter().map(|t| task_node(input, tasks, &by_id, t, now)).collect();

    let children: Vec<AgentNode> = watch_nodes.into_iter().chain(direct).collect();
    let mut everything: Vec<&AgentNode> = Vec::new();
    for c in &children {
        everything.push(c);
        everything.extend(c.descendants());
    }
    let run_nodes: Vec<&&AgentNode> = everything.iter().filter(|n| n.kind == NodeKind::Run).collect();
    let running = run_nodes.iter().filter(|n| n.is_live()).count();
    let cost = sum(run_nodes.iter().map(|n| n.cost.map_or(0.0, |c| c.0)));
    let task_nodes: Vec<&&AgentNode> = everything.iter().filter(|n| n.kind == NodeKind::Task).collect();
    let waiting = task_nodes.iter().filter(|n| matches!(n.state, NodeState::NeedsYou | NodeState::Review)).count();
    let work = if task_nodes.is_empty() {
        "Nothing to look after yet".to_owned()
    } else if waiting > 0 {
        format!("Looking after {}, {waiting} waiting on you", task_count(task_nodes.len()))
    } else {
        format!("Looking after {}", task_count(task_nodes.len()))
    };
    let root = AgentNode {
        id: "neko".to_owned(),
        kind: NodeKind::Neko,
        name: "Neko".to_owned(),
        subtitle: "Chief of staff".to_owned(),
        work,
        origin: None,
        state: if running > 0 {
            NodeState::Running
        } else if waiting > 0 {
            NodeState::NeedsYou
        } else {
            NodeState::Done
        },
        cost: run_nodes.iter().any(|n| n.cost.is_some()).then_some(Usd(cost)),
        duration_ms: None,
        tools: Vec::new(),
        task_id: None,
        children: Vec::new(),
    };
    let tasks_count = task_nodes.len();
    AgentTree { root: AgentNode { children, ..root }, tasks: tasks_count, running, cost: Usd(cost) }
}

/// The tree under the page's filters, as `Tasks.tsx` asks for it: the tasks the page shows, the workspace's watches, and the idle
/// watches only while nothing is searched or picked.
pub fn tree_for(input: &TreeInput, filters: &Filters, now: i64) -> AgentTree {
    let visible = flatten(&by_stage(&input.tasks, filters));
    let workspace = (!filters.workspace.is_empty()).then(|| filters.workspace.clone());
    build_tree(input, &visible, &Options { workspace, show_idle_watches: filters.search.is_empty() && filters.stage.is_none(), now })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A default snapshot with these top-level fields replaced.
    fn snapshot(v: serde_json::Value) -> Snapshot {
        let mut base = serde_json::to_value(Snapshot::default()).expect("a default snapshot");
        for (k, value) in v.as_object().expect("an object") {
            base[k] = value.clone();
        }
        serde_json::from_value(base).expect("a snapshot")
    }

    fn task(id: &str, title: &str, status: &str, updated: i64, events: serde_json::Value) -> serde_json::Value {
        json!({"id": id, "workspace_id": "w1", "issue_id": null, "title": title, "goal": "", "status": status, "plan": "", "result": "",
               "worktree": null, "created_at_ms": 1000, "updated_at_ms": updated, "events": events})
    }

    fn ev(at: i64, role: &str, message: &str) -> serde_json::Value {
        json!({"at_ms": at, "role": role, "message": message})
    }

    fn built(s: &Snapshot, now: i64) -> AgentTree {
        TreeInput::from_snapshot(s, now).1
    }

    #[test]
    fn a_run_is_opened_by_its_role_and_closed_by_another_role() {
        let s = snapshot(json!({"tasks": [task("t1", "Fix the login", "ReadyForReview", 9000, json!([
            ev(1000, "scout", "Reading"), ev(2000, "scout", "Planned"), ev(3000, "supervisor", "Plan ready."),
            ev(4000, "builder", "Building"), ev(7000, "builder", "Done"), ev(8000, "reviewer", r#"{"passed": true}"#),
        ]))]}));
        let tree = built(&s, 20_000);
        let task = &tree.root.children[0];
        let runs: Vec<(&str, NodeState, Option<i64>)> = task.children.iter().map(|r| (r.name.as_str(), r.state, r.duration_ms)).collect();
        assert_eq!(runs, [("Scout", NodeState::Done, Some(1000)), ("Builder", NodeState::Done, Some(3000)), ("Reviewer", NodeState::Passed, Some(0))]);
        // The supervisor closed the scout with "Plan ready", which is not a stop.
        assert_eq!(task.children[0].id, "t1#0");
        assert_eq!(task.children[0].work, "Read the code and planned the work");
        assert_eq!(task.subtitle, "", "no runtime was recorded");
        assert_eq!(task.children[0].subtitle, UNRECORDED_MODEL);
    }

    #[test]
    fn agent_started_opens_a_new_run_of_the_same_role() {
        let runs = |events: serde_json::Value, status| {
            let t: Task = serde_json::from_value(task("t1", "T", status, 9000, events)).unwrap();
            runs_of("t1", t.status, &t.events, &[], 50_000)
        };
        let two = runs(json!([ev(1000, "builder", "Agent started: Builder"), ev(2000, "builder", "edit"), ev(3000, "builder", "Agent started: Builder"), ev(4000, "builder", "edit")]), "Building");
        assert_eq!(two.len(), 2, "a restart is its own run");
        assert_eq!((two[0].state, two[0].duration_ms), (NodeState::Done, Some(1000)));
        assert_eq!((two[1].state, two[1].duration_ms), (NodeState::Running, Some(47_000)), "the last open run of a running task is live and lasts until now");
        let one = runs(json!([ev(1000, "builder", "Agent started: Builder"), ev(2000, "builder", "edit")]), "Completed");
        assert_eq!(one.len(), 1);
        // A closed run is not continued.
        let closed = runs(json!([ev(1000, "builder", "x"), ev(1500, "neko", "note"), ev(2000, "builder", "y")]), "Completed");
        assert_eq!(closed.len(), 2);
    }

    #[test]
    fn usage_cost_is_summed_into_the_run_then_the_task_and_across_split_children() {
        let usage = |c: f64| format!(r#"USAGE {{"cost_usd": {c}}}"#);
        let s = snapshot(json!({
            "tasks": [
                task("p", "Parent", "Building", 5000, json!([ev(1000, "builder", "work"), ev(2000, "builder", &usage(0.5)), ev(3000, "builder", "Agent started: Builder")])),
                task("c1", "Child one", "Completed", 4000, json!([ev(1000, "builder", &usage(0.5))])),
                task("c2", "Child two", "Completed", 3000, json!([ev(1000, "builder", "no cost")])),
            ],
            "splits": [{"parent_id": "p", "subtasks": [
                {"title": "a", "goal": "", "files": [], "tests": [], "depends_on": [], "task_id": "c1"},
                {"title": "b", "goal": "", "files": [], "tests": [], "depends_on": [], "task_id": "c2"}], "approved": true, "integrated": false, "base": null}],
        }));
        let tree = built(&s, 10_000);
        assert_eq!(tree.root.children.len(), 1, "the children sit under their parent");
        let parent = &tree.root.children[0];
        assert_eq!(parent.children[0].cost, Some(Usd(0.5)), "a usage line prices the run and closes it");
        assert_eq!((parent.children[1].state, parent.children[1].cost), (NodeState::Running, None), "the restart is a new, live run");
        assert_eq!(parent.cost, Some(Usd(1.0)), "own 0.5 plus the child's 0.5");
        let kids: Vec<&str> = parent.children.iter().filter(|c| c.kind == NodeKind::Task).map(|c| c.name.as_str()).collect();
        assert_eq!(kids, ["Child one", "Child two"], "newest first inside the same stage");
        assert_eq!(parent.children[3].origin.as_deref(), Some("Split from “Parent”"));
        assert_eq!(parent.children[3].cost, None, "a child with no usage has no cost");
        assert_eq!(tree.cost, Usd(1.0), "the root sums runs, never twice");
        assert_eq!(tree_summary(&tree), "3 tasks · 1 running · $1.00 reported");
        assert!(parent.expands_by_default(), "an agent below it works");
    }

    #[test]
    fn a_supervisor_stop_reason_stops_the_run_unless_it_is_a_result_plan_or_pull_request() {
        let state = |why: &str| {
            let t: Task = serde_json::from_value(task("t1", "T", "AwaitingApproval", 9000, json!([ev(1000, "scout", "Reading"), ev(2000, "supervisor", why)]))).unwrap();
            runs_of("t1", t.status, &t.events, &[], 9000)[0].state
        };
        assert_eq!(state("Needs your input before building: which file?"), NodeState::Stopped);
        assert_eq!(state("Plan ready."), NodeState::Done);
        assert_eq!(state("Result ready for review"), NodeState::Done);
        assert_eq!(state("Opened a pull request"), NodeState::Done);
        // A failed task's last run with no cost is stopped even with no supervisor line.
        let t: Task = serde_json::from_value(task("t1", "T", "Failed", 9000, json!([ev(1000, "builder", "x")]))).unwrap();
        assert_eq!(runs_of("t1", t.status, &t.events, &[], 9000)[0].state, NodeState::Stopped);
        let changes: Task = serde_json::from_value(task("t1", "T", "ReadyForReview", 9000, json!([ev(1000, "reviewer", r#"{"passed": false}"#)]))).unwrap();
        assert_eq!(runs_of("t1", changes.status, &changes.events, &[], 9000)[0].state, NodeState::ChangesAsked);
    }

    #[test]
    fn a_task_a_watch_filed_sits_under_that_watch_with_its_origin_and_tool() {
        let s = snapshot(json!({
            "tasks": [task("t1", "Crash on save", "Queued", 5000, json!([])), task("t2", "Asha asked for this", "Queued", 4000, json!([]))],
            "mcp": {"version": 1, "connections": [{"id": "c1", "workspace_id": "w1", "label": "Acme Linear", "config": {"transport": "stdio", "command": "x", "args": []}, "enabled": true, "trusted": true, "has_credentials": true, "tools": [], "discovered_ms": null, "error": null}],
                    "grants": [], "receipts": [], "auto_watch_scopes": [],
                    "responsibilities": [
                        {"id": "r1", "workspace_id": "w1", "instruction": "Watch the Acme board", "connection_ids": ["c1"], "enabled": true, "prepare_low_risk": false, "next_due_ms": 0, "last_attempt_ms": null, "last_result": "", "failures": 0, "name": "Linear: Acme"},
                        {"id": "r2", "workspace_id": "w1", "instruction": "Idle but on", "connection_ids": [], "enabled": true, "prepare_low_risk": false, "next_due_ms": 0, "last_attempt_ms": null, "last_result": "", "failures": 2},
                        {"id": "r3", "workspace_id": "w1", "instruction": "Switched off", "connection_ids": [], "enabled": false, "prepare_low_risk": false, "next_due_ms": 0, "last_attempt_ms": null, "last_result": "", "failures": 0}],
                    "sources": [{"task_id": "t1", "id": "s1", "responsibility_id": "r1", "connection_id": "c1", "external_id": "ENG-123", "revision": "1", "title": "Crash on save", "description": "", "retrieved_ms": 0, "receipt_ids": [], "eligible": true}]},
        }));
        let tree = built(&s, 9000);
        let names: Vec<(&str, NodeKind, NodeState)> = tree.root.children.iter().map(|n| (n.name.as_str(), n.kind, n.state)).collect();
        assert_eq!(names, [("Linear: Acme", NodeKind::Watch, NodeState::Watching), ("Idle but on", NodeKind::Watch, NodeState::Failing), ("Asha asked for this", NodeKind::Task, NodeState::UpNext)], "the busiest watch first; the paused one has nothing filed so it is left out");
        let filed = &tree.root.children[0].children[0];
        assert_eq!(filed.origin.as_deref(), Some("Filed from Linear · ENG-123"));
        assert_eq!(filed.tools, ["Linear"]);
        assert_eq!(tree.root.children[0].tools, ["Linear"]);
        assert_eq!(tree.root.children[2].origin.as_deref(), Some("You asked"));
        // Searching hides the idle watches.
        let input = TreeInput::from_snapshot(&s, 9000).0;
        let searched = tree_for(&input, &Filters { search: "asha".into(), include_stopped: true, ..Filters::default() }, 9000);
        assert_eq!(searched.root.children.len(), 1);
        assert_eq!(searched.root.work, "Looking after 1 task");
        // Another workspace shows none of it.
        let other = tree_for(&input, &Filters { workspace: "w2".into(), include_stopped: true, ..Filters::default() }, 9000);
        assert!(other.root.children.is_empty());
    }

    #[test]
    fn counts_and_stage_filtering_are_the_webs() {
        assert_eq!((task_count(0).as_str(), task_count(1).as_str(), task_count(2).as_str()), ("0 tasks", "1 task", "2 tasks"));
        let s = snapshot(json!({"tasks": [
            task("a", "Alpha", "Building", 5000, json!([])), task("b", "Beta", "Failed", 6000, json!([])), task("c", "Gamma", "Cancelled", 7000, json!([])),
            task("d", "Delta", "Queued", 8000, json!([])), task("e", "Epsilon", "ReadyForReview", 1000, json!([])), task("f", "Zeta", "Completed", 2000, json!([]))]}));
        let input = TreeInput::from_snapshot(&s, 9000).0;
        let ids = |f: &Filters| flatten(&by_stage(&input.tasks, f)).into_iter().map(|t| t.id.as_str()).collect::<Vec<_>>().join("");
        assert_eq!(ids(&Filters { include_stopped: true, ..Filters::default() }), "beadcf", "Needs you (newest first), Ready, Running, Up next, Done (newest first)");
        assert_eq!(ids(&Filters::default()), "eadf", "failed and cancelled stay out unless asked for");
        assert_eq!(ids(&Filters { stage: Some(Stage::Running), include_stopped: true, ..Filters::default() }), "a");
        assert_eq!(ids(&Filters { search: " ALP ".into(), include_stopped: true, ..Filters::default() }), "a");
        assert_eq!(ids(&Filters { workspace: "nowhere".into(), include_stopped: true, ..Filters::default() }), "");
        let tree = built(&s, 9000);
        assert_eq!(tree.tasks, 6);
        assert_eq!(tree.root.children.iter().map(|c| c.id.as_str()).collect::<String>(), "abedcf", "the web's taskOrder: running, needs you, ready, up next, done; newest first inside");
        assert_eq!(tree.root.work, "Looking after 6 tasks, 1 waiting on you");
        assert_eq!(tree.root.state, NodeState::NeedsYou, "no run is live, one task waits");
    }

    #[test]
    fn follow_ups_and_chat_tickets_say_where_they_came_from() {
        let s = snapshot(json!({
            "tasks": [task("p", "Parent", "Completed", 5000, json!([])), task("f", "Follow up", "Queued", 4000, json!([])), task("h", "From chat", "Queued", 3000, json!([]))],
            "follow_up_of": {"f": "p"},
            "conversation": [{"id": "m1", "role": "user", "text": "hi", "at_ms": 1, "ticket_ids": ["h"]}],
        }));
        let tree = built(&s, 9000);
        let parent = &tree.root.children.iter().find(|c| c.id == "p").unwrap();
        assert_eq!(parent.children[0].origin.as_deref(), Some("Proposed by “Parent”"));
        assert_eq!(tree.root.children.iter().find(|c| c.id == "h").unwrap().origin.as_deref(), Some("From your chat with Neko"));
    }

    #[test]
    fn model_labels_costs_and_durations_read_as_the_web_writes_them() {
        assert_eq!(model_label("claude", "opus"), "Claude Opus");
        assert_eq!(model_label("", "gpt-5.5"), "Codex gpt-5.5");
        assert_eq!(model_label("opencode", ""), "OpenCode");
        assert_eq!(model_label("acme", ""), "Acme");
        assert_eq!((format_cost(0.004).as_str(), format_cost(0.0).as_str(), format_cost(0.27).as_str(), format_cost(12.0).as_str(), format_cost(123.4).as_str()), ("<$0.01", "$0.00", "$0.27", "$12.00", "$123"));
        assert_eq!((format_duration(18_000).as_str(), format_duration(90_000).as_str(), format_duration(7_500_000).as_str(), format_duration(3 * 86_400_000).as_str()), ("18s", "1m 30s", "2h 05m", "3d"));
        assert_eq!(format_duration(-5), "0s");
    }
}

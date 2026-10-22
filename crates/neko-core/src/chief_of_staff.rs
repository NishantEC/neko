//! Neko as chief of staff: the record it keeps of every task its agents work
//! on, and the actions it may propose on them.
//!
//! The chat turn sees each task's repository, branch, pull request, stage,
//! what is blocking it and its latest activity, so "what's going on with
//! athena?" has a real answer. It may propose an action on a task (tell its
//! agent something, start or stop it, run it next, approve, accept, retry,
//! rename); nothing happens until the user confirms it, and a confirmed
//! action runs as the same command the app's own buttons send.

use neko_protocol::workbench::{chat_action_kinds, ChatAction, ChatActionStatus, Command, Snapshot, Task, TaskStatus};
use serde::Deserialize;
use serde_json::{json, Value};

/// Tasks in the record. Needs-you and running ones always come first.
pub const RECORD_TASKS: usize = 60;
/// Actions one reply may propose.
pub const MAX_ACTIONS: usize = 5;
const EVENT_CHARS: usize = 220;

fn short(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max { return text; }
    format!("{}…", text.chars().take(max).collect::<String>())
}

fn stage(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::AwaitingApproval | TaskStatus::Failed => "needs you",
        TaskStatus::ReadyForReview => "ready to review",
        TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => "running",
        TaskStatus::Queued => "up next",
        _ => "done",
    }
}

fn rank(status: TaskStatus) -> u8 {
    match stage(status) {
        "needs you" => 0,
        "running" => 1,
        "ready to review" => 2,
        "up next" => 3,
        _ => 4,
    }
}

/// The agent's last word on a stopped task: why it waits or why it failed.
fn blocker(task: &Task) -> Option<String> {
    if !matches!(task.status, TaskStatus::AwaitingApproval | TaskStatus::Failed) {
        return None;
    }
    task.events.iter().rev()
        .find(|e| e.role != "user" && e.role != "host" && !e.message.trim().is_empty())
        .map(|e| short(&e.message, 400))
}

/// One line per task: where it is, what it is, and what is happening to it.
pub fn record(snapshot: &Snapshot, in_scope: impl Fn(&str) -> bool, now_ms: i64) -> Vec<Value> {
    let queue: Vec<&str> = {
        let mut queued: Vec<&Task> = snapshot.tasks.iter().filter(|t| t.status == TaskStatus::Queued).collect();
        queued.sort_by_key(|t| snapshot.run_next.iter().position(|id| id == &t.id).unwrap_or(usize::MAX));
        queued.into_iter().map(|t| t.id.as_str()).collect()
    };
    let mut tasks: Vec<&Task> = snapshot.tasks.iter().filter(|t| in_scope(&t.workspace_id)).collect();
    tasks.sort_by_key(|t| (rank(t.status), std::cmp::Reverse(t.updated_at_ms)));
    tasks.into_iter().take(RECORD_TASKS).map(|t| {
        let repository = snapshot.task_roots.get(&t.id)
            .and_then(|r| std::path::Path::new(r).file_name()).map(|n| n.to_string_lossy().into_owned());
        let branch = t.parked.as_ref().map(|p| p.branch.clone())
            .or_else(|| t.pull_request.as_ref().map(|p| p.branch.clone()))
            .or_else(|| t.worktree.as_deref().and_then(|w| std::path::Path::new(w).file_name()).map(|n| format!("codex/neko-{}", n.to_string_lossy())));
        let source = snapshot.mcp.sources.iter().find(|s| s.task_id.as_deref() == Some(&t.id)).map(|s| {
            let label = snapshot.mcp.connections.iter().find(|c| c.id == s.connection_id).map(|c| c.label.as_str()).unwrap_or("");
            format!("{label} {}", s.external_id).trim().to_owned()
        });
        let latest: Vec<String> = t.events.iter().rev().take(3)
            .map(|e| format!("{}: {}", e.role, short(&e.message, EVENT_CHARS))).collect();
        let mut line = json!({
            "id": t.id,
            "title": short(t.display_title.as_deref().unwrap_or(&t.title), 160),
            "stage": stage(t.status),
            "status": crate::neko_chat::status_word(t.status),
            "workspace_id": t.workspace_id,
            "updated_minutes_ago": ((now_ms - t.updated_at_ms).max(0) / 60_000),
            "latest": latest,
        });
        let fields = line.as_object_mut().expect("object");
        if t.display_title.is_some() { fields.insert("source_title".into(), json!(short(&t.title, 160))); }
        if let Some(r) = repository { fields.insert("repository".into(), json!(r)); }
        if let Some(b) = branch { fields.insert("branch".into(), json!(b)); }
        if let Some(pr) = &t.pull_request {
            fields.insert("pull_request".into(), json!(pr.url));
            if let Some(status) = &pr.status {
                fields.insert("pull_request_state".into(), json!({
                    "state": status.state, "checks": status.checks, "failing": status.failing,
                    "mergeable": status.mergeable, "review": status.review,
                }));
            }
            if pr.merge_when_ready { fields.insert("merging_when_checks_pass".into(), json!(true)); }
            if let Some(why) = &pr.close_suggestion { fields.insert("agent_suggests_closing".into(), json!(short(why, 400))); }
        }
        if let Some(c) = &t.landed_commit { fields.insert("landed_on_main".into(), json!(short(c, 10))); }
        if let Some(s) = source { fields.insert("filed_from".into(), json!(s)); }
        if let Some(b) = blocker(t) { fields.insert("blocked_on".into(), json!(b)); }
        if let Some(position) = queue.iter().position(|id| *id == t.id) { fields.insert("queue_position".into(), json!(position + 1)); }
        line
    }).collect()
}

/// The machine at a glance, for "how busy is it?".
pub fn overview(snapshot: &Snapshot) -> Value {
    let count = |s: &str| snapshot.tasks.iter().filter(|t| stage(t.status) == s).count();
    json!({
        "needs_you": count("needs you"),
        "running": count("running"),
        "ready_to_review": count("ready to review"),
        "up_next": count("up next"),
        "tasks_at_once": snapshot.max_running_tickets,
        "writing_per_repository": snapshot.writable_per_repository(),
        "folders_on_disk": snapshot.tasks.iter().filter(|t| t.worktree.is_some() && t.parked.is_none()).count(),
        "neko_update_waiting_for_you": snapshot.self_update.pending.as_ref().map(|p| json!({
            "commit": crate::self_update::short(&p.to), "task_id": p.task_id, "why": p.reason,
        })),
    })
}

/// An action as the model writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProposedAction {
    pub task_id: String,
    pub kind: String,
    #[serde(default)]
    pub text: String,
}

/// Keeps only actions on real, in-scope tasks with a known kind and the text
/// that kind needs. Anything else is dropped rather than guessed at.
pub fn validate(snapshot: &Snapshot, proposed: Vec<ProposedAction>, in_scope: impl Fn(&str) -> bool) -> Vec<ChatAction> {
    proposed.into_iter().filter_map(|a| {
        let task = snapshot.tasks.iter().find(|t| t.id == a.task_id && in_scope(&t.workspace_id))?;
        if !chat_action_kinds().contains(&a.kind.as_str()) { return None; }
        let text = a.text.trim();
        let needs_text = matches!(a.kind.as_str(), "message" | "rename" | "merge" | "close_pr");
        // Pull request actions need one that is still open.
        if matches!(a.kind.as_str(), "merge_pr" | "close_pr") && !task.pull_request.as_ref().is_some_and(|p| p.is_open()) {
            return None;
        }
        // Applying a Neko update needs one that is waiting.
        if a.kind == "apply_update" && snapshot.self_update.pending.is_none() {
            return None;
        }
        if needs_text && (text.is_empty() || text.len() > 4000) { return None; }
        // Merging names the task it goes into: a real, different, open one.
        if a.kind == "merge" && !snapshot.tasks.iter().any(|t| t.id == text && t.id != task.id && in_scope(&t.workspace_id)
            && !matches!(t.status, TaskStatus::Completed | TaskStatus::Cancelled)) {
            return None;
        }
        Some(ChatAction {
            id: crate::workbench::new_id(),
            task_id: task.id.clone(),
            kind: a.kind,
            text: if needs_text { text.to_owned() } else { String::new() },
            status: ChatActionStatus::Proposed,
            note: String::new(),
        })
    }).take(MAX_ACTIONS).collect()
}

/// The command a confirmed action runs: the same one the app's buttons send.
pub fn command(action: &ChatAction) -> Result<Command, String> {
    let task_id = action.task_id.clone();
    Ok(match action.kind.as_str() {
        "message" => Command::ReplyToTask { task_id, text: action.text.clone() },
        "start" => Command::StartTask { task_id },
        "stop" | "close" => Command::CancelTask { task_id },
        "merge" => Command::MergeTask { task_id, into: action.text.clone() },
        "run_next" => Command::PrioritizeTask { task_id },
        "approve" => Command::ApproveTask { task_id },
        "accept" => Command::CompleteTask { task_id },
        "retry" => Command::RetryTask { task_id },
        "rename" => Command::RenameTask { task_id, title: action.text.clone() },
        "merge_pr" => Command::SetMergeWhenReady { task_id, enabled: true },
        "close_pr" => Command::ClosePullRequest { task_id, comment: action.text.clone() },
        "apply_update" => Command::ApplyPendingUpdate,
        other => return Err(format!("Unknown action: {other}")),
    })
}

/// What Neko is, for the chat prompt.
pub const ROLE: &str = "You are Neko, the user's chief of staff for their engineering agents. Every piece of work goes through you: you hand tasks to task agents (scouts, builders, independent reviewers), keep track of what each one is doing, and answer for all of it. The task record below is your view of every task: its repository, branch, pull request, stage, what is blocking it, its place in the queue and its latest activity. When the user asks what is going on, answer from the record: lead with what needs them, then what is running, then what is waiting; name tasks by title and say where each one is. Never invent progress the record does not show. When the user wants something done to an existing task, propose an action instead of a new ticket: message (pass the user's instruction to that task's agent, in their words, plus any context the agent needs), start, stop (end its current work), close (close a task nobody needs), merge (close a task as the same problem as another open task; text is that task's id), run_next (move it to the front of the queue), approve (approve a plan that is waiting), accept (accept a reviewed result), retry (try a failed task again), rename, merge_pr (merge the task's open pull request once its checks pass), close_pr (close the task's open pull request on GitHub; text is the comment saying why, naming what covers it) or apply_update (apply the Neko update shown as waiting for the user, on the task that brought it). The record shows each pull request's checks, conflicts, review and whether its agent suggests closing it; when asked about pull requests, answer as a table of each one's verdict: ready to merge, close (and why), needs the user's decision, or still being worked on. Actions wait for the user's one-click confirmation; say what you proposed, never claim it happened. Hold every agent to Neko's rulebook below, and when a task's activity shows it breaking the rulebook, say so and propose the message that corrects it.";

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::workbench::{PullRequest, TaskEvent};

    fn task(id: &str, status: TaskStatus, updated: i64) -> Task {
        Task {
            id: id.into(), workspace_id: "w".into(), issue_id: None, title: format!("Task {id}"), goal: "g".into(),
            status, plan: String::new(), result: String::new(), worktree: None, events: vec![],
            created_at_ms: 0, updated_at_ms: updated, source_revision: None, supervision: None, base_commit: None,
            pull_request: None, landed_commit: None, display_title: None, parked: None,
        }
    }

    #[test]
    fn the_record_leads_with_what_needs_you_and_says_where_each_task_is() {
        let mut snapshot = Snapshot::default();
        let mut blocked = task("b", TaskStatus::AwaitingApproval, 10);
        blocked.events.push(TaskEvent { at_ms: 1, role: "scout".into(), message: "Should I use the 375px designs?".into() });
        let mut review = task("r", TaskStatus::ReadyForReview, 50);
        review.pull_request = Some(PullRequest { url: "https://github.com/acme/athena/pull/7".into(), branch: "codex/neko-r".into(), ..Default::default() });
        snapshot.tasks = vec![task("q1", TaskStatus::Queued, 90), review, task("q2", TaskStatus::Queued, 80), blocked];
        snapshot.task_roots.insert("r".into(), "/Users/x/hme/athena".into());
        snapshot.run_next = vec!["q2".into()];
        let record = record(&snapshot, |_| true, 60_000 * 100);
        assert_eq!(record[0]["id"], "b");
        assert_eq!(record[0]["blocked_on"], "Should I use the 375px designs?");
        assert_eq!(record[1]["repository"], "athena");
        assert_eq!(record[1]["pull_request"], "https://github.com/acme/athena/pull/7");
        let q2 = record.iter().find(|t| t["id"] == "q2").unwrap();
        assert_eq!(q2["queue_position"], 1, "run next goes to the front of the queue");
    }

    #[test]
    fn only_real_well_formed_actions_survive_and_map_to_the_app_s_own_commands() {
        let mut snapshot = Snapshot::default();
        snapshot.tasks = vec![task("t", TaskStatus::AwaitingApproval, 0)];
        let actions = validate(&snapshot, vec![
            ProposedAction { task_id: "t".into(), kind: "message".into(), text: "Use the 375px designs".into() },
            ProposedAction { task_id: "t".into(), kind: "message".into(), text: " ".into() },
            ProposedAction { task_id: "missing".into(), kind: "stop".into(), text: String::new() },
            ProposedAction { task_id: "t".into(), kind: "merge".into(), text: String::new() },
            ProposedAction { task_id: "t".into(), kind: "approve".into(), text: "ignored".into() },
        ], |_| true);
        assert_eq!(actions.len(), 2);
        assert!(matches!(command(&actions[0]).unwrap(), Command::ReplyToTask { task_id, text } if task_id == "t" && text == "Use the 375px designs"));
        assert!(matches!(command(&actions[1]).unwrap(), Command::ApproveTask { task_id } if task_id == "t"));
        assert!(actions[1].text.is_empty());
        assert!(validate(&snapshot, vec![ProposedAction { task_id: "t".into(), kind: "stop".into(), text: String::new() }], |w| w != "w").is_empty(), "out of scope");
    }

    #[test]
    fn applying_a_neko_update_needs_one_waiting() {
        let mut snapshot = Snapshot::default();
        snapshot.tasks = vec![task("t", TaskStatus::Completed, 0)];
        let apply = || vec![ProposedAction { task_id: "t".into(), kind: "apply_update".into(), text: String::new() }];
        assert!(validate(&snapshot, apply(), |_| true).is_empty(), "nothing waits");
        snapshot.self_update.pending = Some(neko_protocol::workbench::UpdatePlan { to: "abc".into(), ..Default::default() });
        let actions = validate(&snapshot, apply(), |_| true);
        assert!(matches!(command(&actions[0]).unwrap(), Command::ApplyPendingUpdate));
        assert!(overview(&snapshot)["neko_update_waiting_for_you"]["commit"] == "abc");
    }

    #[test]
    fn merging_needs_a_real_open_target_and_closes_the_duplicate_with_a_note() {
        let db = crate::Db::open_in_memory().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.tasks = vec![task("dup", TaskStatus::AwaitingApproval, 0), task("main", TaskStatus::ReadyForReview, 0), task("done", TaskStatus::Completed, 0)];
        snapshot.workspaces = vec![neko_protocol::workbench::Workspace { id: "w".into(), name: "W".into(), repository: "/tmp".into(), instructions: String::new(), away_enabled: false }];
        crate::workbench::save(&db, &snapshot).unwrap();
        let kept = validate(&snapshot, vec![
            ProposedAction { task_id: "dup".into(), kind: "merge".into(), text: "main".into() },
            ProposedAction { task_id: "dup".into(), kind: "merge".into(), text: "done".into() },
            ProposedAction { task_id: "dup".into(), kind: "merge".into(), text: "dup".into() },
        ], |_| true);
        assert_eq!(kept.len(), 1, "only a different, open, real target");
        let after = crate::workbench::apply(&db, command(&kept[0]).unwrap()).unwrap();
        let dup = after.tasks.iter().find(|t| t.id == "dup").unwrap();
        assert_eq!(dup.status, TaskStatus::Completed);
        assert!(dup.result.contains("Merged into “Task main”"));
        assert!(after.tasks.iter().find(|t| t.id == "main").unwrap().events.last().unwrap().message.contains("“Task dup” was merged into this task"));
        assert!(crate::workbench::apply(&db, Command::MergeTask { task_id: "main".into(), into: "done".into() }).is_err());
    }
}

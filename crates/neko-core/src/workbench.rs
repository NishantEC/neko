//! Durable Neko-owned work. The daemon serializes access to this store.
use crate::Db;
use neko_protocol::workbench::{
    Command, Risk, Snapshot, SupervisorAction, SupervisorDecision, Task, TaskEvent, TaskSplit,
    TaskStatus,
};
use std::collections::HashSet;

const SETTING: &str = "workbench_snapshot_v1";
const RESUMED: &str = "Resumed after a daemon restart";
const MAX_RESTART_RESUMES: usize = 2;
pub const MAX_CONTENT_BYTES: usize = 32_768;
pub const MAX_EVENT_BYTES: usize = 2048;
pub const MAX_RESULT_BYTES: usize = 132 * 1024;
pub const MAX_EVENTS: usize = 100;
pub const MAX_NOTE_BYTES: usize = 2048;
/// Event role for the user's steering notes on a ticket.
pub const NOTE_ROLE: &str = "note";
const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
const STATE_RESERVE_BYTES: usize = 64 * 1024;

/// Bumped after every successful write of the task store. Readers that poll
/// (authority watchdogs, the quick panel's ticket list) re-parse the store
/// only when this moves, instead of on every tick or keystroke.
static REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn revision() -> u64 {
    REVISION.load(std::sync::atomic::Ordering::Acquire)
}

fn bump_revision() {
    REVISION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
}

pub fn load(db: &Db) -> Result<Snapshot, String> {
    let mut snapshot = if let Some(json) = db.get_setting(SETTING).map_err(|e| e.to_string())? {
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err("Workbench storage exceeds its size limit".into());
        }
        serde_json::from_str(&json).map_err(|e| format!("Cannot read workbench storage: {e}"))?
    } else {
        Snapshot::default()
    };
    let migrated = crate::mcp_host::store::migrate(&mut snapshot)?;
    validate(&snapshot)?;
    if migrated {
        // One upsert atomically disables old authority without deleting evidence.
        let json = serde_json::to_string(&snapshot).map_err(|e| e.to_string())?;
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err(
                "Migration needs storage headroom; original workspace data has not been changed"
                    .into(),
            );
        }
        db.set_setting(SETTING, &json).map_err(|e| e.to_string())?;
        bump_revision();
    }
    // Attached for readers only; save() never writes it into this setting.
    snapshot.conversation = crate::neko_chat::load(db)?;
    snapshot.memory = crate::neko_memory::load(db)?;
    snapshot.memory_options = crate::neko_memory::options(db)?;
    snapshot.memory_proposals = crate::memory_learning::proposals(db, &snapshot)?;
    snapshot.skills = crate::skills::load(db)?;
    snapshot.import_preview = crate::setup_import::load_preview(db)?;
    Ok(snapshot)
}

/// One SQLite upsert commits the whole snapshot; validation failures write nothing.
/// Callers must hold the daemon's database lock across load/modify/save.
/// Old event messages are shortened and oldest events are evicted under pressure.
/// Plans, results and task identities are never evicted. Reload after saving when
/// a caller needs the exact persisted event history.
pub fn save(db: &Db, snapshot: &Snapshot) -> Result<(), String> {
    validate(snapshot)?;
    let projected = reserved_capacity(snapshot)?;
    if projected > MAX_SNAPSHOT_BYTES - STATE_RESERVE_BYTES {
        // Older stores can already exceed the newly introduced reservation.
        // Let work finish or be cancelled while refusing growth/new intake.
        let previous = load(db)?;
        if projected > reserved_capacity(&previous)? {
            return Err("Workbench capacity is reserved for pending task results; finish or cancel pending work before adding more".into());
        }
    }
    let mut compacted = snapshot.clone();
    crate::scheduled_plans::refresh_results(&mut compacted);
    compacted.conversation.clear();
    compacted.memory.clear();
    compacted.memory_proposals.clear();
    compacted.skills = Default::default();
    compacted.import_preview = Default::default();
    for task in &mut compacted.tasks {
        for event in &mut task.events {
            event.message = truncate(&event.message, MAX_EVENT_BYTES);
        }
    }
    let mut json = serde_json::to_string(&compacted).map_err(|e| e.to_string())?;
    if json.len() > MAX_SNAPSHOT_BYTES {
        // Serialize individual events to account for JSON escaping, including
        // control characters that use six bytes on the wire. Drop the oldest
        // across tasks, preserving the original order of retained events.
        let mut oldest = Vec::new();
        for (task_index, task) in compacted.tasks.iter().enumerate() {
            for (event_index, event) in task.events.iter().enumerate() {
                let bytes = serde_json::to_vec(event).map_err(|e| e.to_string())?.len();
                oldest.push((event.at_ms, task_index, event_index, bytes));
            }
        }
        oldest.sort_unstable();
        let mut remove = HashSet::new();
        let mut remaining = json.len();
        for (_, task_index, event_index, bytes) in oldest {
            if remaining <= MAX_SNAPSHOT_BYTES - STATE_RESERVE_BYTES {
                break;
            }
            remove.insert((task_index, event_index));
            // Counting only the object (not its comma) is conservative.
            remaining = remaining.saturating_sub(bytes);
        }
        for (task_index, task) in compacted.tasks.iter_mut().enumerate() {
            let mut event_index = 0;
            task.events.retain(|_| {
                let keep = !remove.contains(&(task_index, event_index));
                event_index += 1;
                keep
            });
        }
        json = serde_json::to_string(&compacted).map_err(|e| e.to_string())?;
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err(
                "Workbench durable content is full; task plans and results have been preserved"
                    .into(),
            );
        }
    }
    db.set_setting(SETTING, &json).map_err(|e| e.to_string())?;
    bump_revision();
    Ok(())
}

/// Reserve the worst possible JSON size of every pending plan/result/worktree.
/// A future result can therefore displace logs without competing with new work.
/// Six is JSON's maximum expansion for a UTF-8 byte (a control character).
fn reserved_capacity(snapshot: &Snapshot) -> Result<usize, String> {
    let mut metadata = snapshot.clone();
    metadata.conversation.clear();
    metadata.memory.clear();
    metadata.memory_proposals.clear();
    for task in &mut metadata.tasks {
        task.events.clear();
    }
    let mut bytes = serde_json::to_vec(&metadata)
        .map_err(|e| e.to_string())?
        .len();
    for task in snapshot.tasks.iter().filter(|task| !terminal(task.status)) {
        for (text, limit) in [
            (&task.plan, MAX_CONTENT_BYTES * 2),
            (&task.result, MAX_RESULT_BYTES),
        ] {
            let occupied = serde_json::to_string(text)
                .map_err(|e| e.to_string())?
                .len()
                - 2;
            bytes += (limit * 6).saturating_sub(occupied);
        }
        let occupied = task
            .worktree
            .as_ref()
            .map(|path| serde_json::to_string(path).map(|s| s.len() - 2))
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(0);
        bytes += (4096_usize * 6).saturating_sub(occupied);
        bytes += 512; // status/timestamps and optional-path representation
        let occupied = serde_json::to_vec(&task.supervision)
            .map_err(|e| e.to_string())?
            .len();
        bytes += crate::supervision::MAX_DECISION_BYTES.saturating_sub(occupied);
    }
    Ok(bytes)
}

pub fn apply(db: &Db, command: Command) -> Result<Snapshot, String> {
    if matches!(command, Command::Snapshot) {
        return load(db);
    }
    if let Command::StartTask { task_id } = command {
        return start_task(db, task_id);
    }
    let completed = match &command {
        Command::CompleteTask { task_id } => Some(task_id.clone()),
        _ => None,
    };
    let decision = match &command {
        Command::ApproveTask { task_id } => Some((task_id.clone(), "approval", None)),
        Command::CancelTask { task_id } => Some((
            task_id.clone(),
            "cancellation",
            Some("Cancelled this task.".to_owned()),
        )),
        Command::AddTicketNote { task_id, text } => {
            Some((task_id.clone(), "note", Some(text.trim().to_owned())))
        }
        _ => None,
    };
    db.atomic(|| {
        let mut snapshot = apply_inner(db, command)?;
        if let Some(id) = completed {
            if crate::memory_learning::has_capacity(db)? {
                crate::memory_learning::enqueue(db, &snapshot, crate::memory_learning::Source::Ticket(id))?;
            } else if let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == id) {
                append_event(task, "learning", "Memory learning queue is full. This ticket is complete; no memory or skill extraction was started.");
                save(db, &snapshot)?;
            }
        }
        if let Some((id, action, text)) = decision {
            let task = snapshot.tasks.iter().find(|t| t.id == id).ok_or("Task no longer exists")?;
            // Notes are retained preferentially, so the corresponding event may
            // already have been evicted. Record the successful command itself.
            let text = text.unwrap_or_else(|| if snapshot.splits.iter().any(|s| s.parent_id == id && s.approved) {
                "Approved subtask plans. Children use isolated worktrees; integration stays in this parent worktree.".into()
            } else { "Approved the plan for a local build.".into() });
            let entry = neko_protocol::workbench::MemoryEntry {
                id: String::new(),
                agent_profile_id: snapshot.agent_profiles.owner(&task.workspace_id).into(),
                workspace_id: Some(task.workspace_id.clone()),
                kind: neko_protocol::workbench::MemoryKind::Decision,
                text: truncate(&text, crate::neko_memory::MAX_TEXT),
                source: format!("ticket:{}:{action}:{}:{}", task.id, now_ms(), new_id()),
                created_at_ms: 0, updated_at_ms: 0,
            };
            crate::neko_memory::record_decision(db, entry)?;
        }
        load(db)
    })
}

/// Stage a definition in the candidate snapshot; callers save only after all
/// related fields (including folder mappings) and task-history guards are ready.
fn stage_workspace(snapshot: &mut Snapshot, mut workspace: neko_protocol::workbench::Workspace) -> Result<String, String> {
    workspace.name = workspace.name.trim().to_owned();
    required("Workspace name", &workspace.name, 256)?;
    bounded("Workspace instructions", &workspace.instructions, MAX_CONTENT_BYTES)?;
    workspace.repository = canonical_workspace_directory(&workspace.repository)?;
    if workspace.id.is_empty() {
        workspace.id = new_id();
        let id = workspace.id.clone();
        snapshot.workspaces.push(workspace);
        Ok(id)
    } else {
        let previous = snapshot.workspaces.iter_mut()
            .find(|w| w.id == workspace.id).ok_or("Workspace no longer exists")?;
        if previous.repository != workspace.repository
            && snapshot.tasks.iter().any(|t| t.workspace_id == workspace.id)
        {
            return Err("This workspace already has task history. Create a new workspace for a different repository".into());
        }
        let id = workspace.id.clone();
        *previous = workspace;
        Ok(id)
    }
}

fn apply_inner(db: &Db, command: Command) -> Result<Snapshot, String> {
    let mut snapshot = load(db)?;
    match command {
        Command::SetAgentRuntime { runtime } => {
            if !matches!(runtime.provider.as_str(), "" | "codex" | "ollama" | "lmstudio" | "opencodex" | "claude" | "opencode") {
                return Err("Choose Codex, Claude Code, OpenCode, Ollama or LM Studio".into());
            }
            if runtime.provider == "opencode" && !runtime.model.is_empty() && !runtime.model.split_once('/').is_some_and(|(p, m)| !p.is_empty() && !m.is_empty()) {
                return Err("OpenCode models are written provider/model".into());
            }
            if runtime.provider == "opencodex" && !runtime.model.split_once('/').is_some_and(|(provider, model)| !provider.is_empty() && !model.is_empty()) {
                return Err("Choose a configured OpenCodex provider and model".into());
            }
            if runtime.model.len() > 120
                || !runtime.model.chars().all(|c| c.is_ascii_alphanumeric() || "-_:./".contains(c))
            {
                return Err("Model name may use letters, numbers, hyphens, underscores, colons, dots, and slashes (up to 120 characters)".into());
            }
            snapshot.agent_runtime = runtime;
        }
        Command::AgentProfiles(command) => crate::agent_profiles::apply(&mut snapshot, command)?,
        Command::Schedules(neko_protocol::scheduled_plans::ScheduleCommand::List) => {
            return Ok(snapshot);
        }
        Command::Schedules(command) => crate::scheduled_plans::apply(&mut snapshot, command)?,
        Command::Mcp(command) => {
            crate::mcp_host::store::apply_command(&mut snapshot, command, now_ms())?
        }
        Command::Skills(_) => return Err("Skill commands must be handled by the daemon".into()),
        Command::Snapshot => return Ok(snapshot),
        Command::SaveWorkspace { workspace } => {
            stage_workspace(&mut snapshot, workspace)?;
            for id in snapshot
                .mcp
                .connections
                .iter()
                .filter(|c| c.workspace_id.is_empty())
                .map(|c| c.id.clone())
                .collect::<Vec<_>>()
            {
                crate::mcp_host::store::allow_discovered_tools(&mut snapshot, &id);
            }
        }
        Command::SaveWorkspaceWithFolders {
            mut workspace,
            folders,
        } => {
            if folders.is_empty() || folders.len() > 20 {
                return Err("Choose one to twenty workspace folders".into());
            }
            let folders = folders
                .iter()
                .map(|folder| canonical_workspace_directory(folder))
                .collect::<Result<Vec<_>, _>>()?;
            if folders.iter().collect::<HashSet<_>>().len() != folders.len() {
                return Err("A folder is already in this workspace".into());
            }
            workspace.repository = folders[0].clone();
            let id = stage_workspace(&mut snapshot, workspace)?;
            for task in snapshot.tasks.iter().filter(|task| task.workspace_id == id) {
                if let Some(root) = snapshot.task_roots.get(&task.id) {
                    if !folders.contains(root) {
                        return Err(
                            "A folder with task history cannot be removed from this workspace"
                                .into(),
                        );
                    }
                }
            }
            snapshot.workspace_folders.insert(id, folders);
            for id in snapshot
                .mcp
                .connections
                .iter()
                .filter(|c| c.workspace_id.is_empty())
                .map(|c| c.id.clone())
                .collect::<Vec<_>>()
            {
                crate::mcp_host::store::allow_discovered_tools(&mut snapshot, &id);
            }
        }
        Command::CreateTask {
            workspace_id,
            title,
            goal,
        } => {
            let workspace = snapshot
                .workspaces
                .iter()
                .find(|w| w.id == workspace_id)
                .ok_or("Workspace no longer exists")?;
            let folder = route_folder(&snapshot, workspace, &title, &goal)?;
            canonical_repository(&folder)
                .map_err(|_| "Code tasks require a Git repository")?;
            let task = create_task(workspace_id, None, title, goal)?;
            snapshot.task_roots.insert(task.id.clone(), folder);
            snapshot.tasks.push(task);
        }
        Command::CreateTaskInFolder {
            workspace_id,
            title,
            goal,
            folder,
        } => {
            let workspace = snapshot
                .workspaces
                .iter()
                .find(|w| w.id == workspace_id)
                .ok_or("Workspace no longer exists")?;
            let folder = canonical_workspace_directory(&folder)?;
            if !snapshot.folders_for(workspace).iter().any(|root| {
                std::path::Path::new(&folder).starts_with(root)
            }) {
                return Err("That folder is not in this workspace".into());
            }
            canonical_repository(&folder).map_err(|_| "Code tasks require a Git repository")?;
            let task = create_task(workspace_id, None, title, goal)?;
            snapshot.task_roots.insert(task.id.clone(), folder);
            snapshot.tasks.push(task);
        }
        Command::PlanIssue { issue_id } => {
            let issue = snapshot
                .issues
                .iter()
                .find(|i| i.id == issue_id)
                .ok_or("Issue no longer exists")?;
            if snapshot
                .tasks
                .iter()
                .any(|t| t.issue_id.as_deref() == Some(&issue_id))
            {
                return Ok(snapshot);
            }
            let workspace = snapshot
                .workspaces
                .iter()
                .find(|w| w.id == issue.workspace_id)
                .ok_or("Workspace no longer exists")?;
            let goal = if issue.description.trim().is_empty() {
                issue.title.clone()
            } else {
                issue.description.clone()
            };
            let folder = route_folder(&snapshot, workspace, &issue.title, &goal)?;
            canonical_repository(&folder).map_err(|_| "Code tasks require a Git repository")?;
            let mut task = create_task(
                issue.workspace_id.clone(),
                Some(issue.id.clone()),
                issue.title.clone(),
                goal,
            )?;
            task.source_revision = Some(issue.updated_at.clone());
            snapshot.task_roots.insert(task.id.clone(), folder);
            snapshot.tasks.push(task);
        }
        Command::ProposeSplit { task_id } => {
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|t| t.id == task_id)
                .ok_or("Task missing")?;
            if task.status != TaskStatus::AwaitingApproval
                || snapshot.splits.iter().any(|s| {
                    s.parent_id == task_id
                        || s.subtasks
                            .iter()
                            .any(|p| p.task_id.as_deref() == Some(&task_id))
                })
            {
                return Err("Only an unsplit plan awaiting approval can request a split".into());
            }
            task.status = TaskStatus::Queued;
            task.supervision = None;
            snapshot.splits.push(TaskSplit {
                parent_id: task_id,
                subtasks: vec![],
                approved: false,
                integrated: false,
                base: None,
            });
        }
        Command::ApproveTask { task_id } => {
            if let Some(index) = snapshot.splits.iter().position(|s| s.parent_id == task_id) {
                let split = snapshot.splits[index].clone();
                if split.approved {
                    return Err("Split already approved; retry the parent after failure".into());
                }
                crate::decomposition::validate(&split.subtasks)?;
                let parent = snapshot
                    .tasks
                    .iter()
                    .find(|t| t.id == task_id)
                    .ok_or("Parent missing")?
                    .clone();
                if parent.status != TaskStatus::AwaitingApproval {
                    return Err("Split proposal is not ready for approval".into());
                }
                for (i, plan) in split.subtasks.iter().enumerate() {
                    let mut child = create_task(
                        parent.workspace_id.clone(),
                        None,
                        plan.title.clone(),
                        plan.goal.clone(),
                    )?;
                    child.plan = format!(
                        "{}\nFiles: {}\nRequired checks: {}",
                        plan.goal,
                        plan.files.join(", "),
                        plan.tests.join("; ")
                    );
                    child.status = TaskStatus::Building;
                    child.supervision = Some(SupervisorDecision {
                        action: SupervisorAction::PrepareFix,
                        risk: Risk::Low,
                        is_bug: false,
                        reason: "User approved this subtask and its scope".into(),
                        evidence: vec![parent.id.clone()],
                        files: plan.files.clone(),
                        tests: plan.tests.clone(),
                        sensitive_areas: vec![],
                        uncertainties: vec![],
                        plan: child.plan.clone(),
                    });
                    snapshot.splits[index].subtasks[i].task_id = Some(child.id.clone());
                    if let Some(root) = snapshot.task_roots.get(&parent.id).cloned() {
                        snapshot.task_roots.insert(child.id.clone(), root);
                    }
                    snapshot.tasks.push(child);
                }
                snapshot.splits[index].approved = true;
                let parent = snapshot.tasks.iter_mut().find(|t| t.id == task_id).unwrap();
                parent.status = TaskStatus::Reviewing;
                append_event(
                    parent,
                    "user",
                    "Approved subtask plans. Children use isolated worktrees; integration stays in this parent worktree.",
                );
                save(db, &snapshot)?;
                return load(db);
            }
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|t| t.id == task_id)
                .ok_or("Task no longer exists")?;
            if task.status != TaskStatus::AwaitingApproval {
                return Err("Only a task awaiting approval can be approved".into());
            }
            task.status = TaskStatus::Building;
            append_event(task, "user", "Approved the plan for a local build.");
        }
        Command::CancelTask { task_id } => {
            let children: Vec<String> = snapshot
                .splits
                .iter()
                .filter(|s| s.parent_id == task_id)
                .flat_map(|s| s.subtasks.iter().filter_map(|p| p.task_id.clone()))
                .collect();
            for child in snapshot
                .tasks
                .iter_mut()
                .filter(|t| children.contains(&t.id) && !terminal(t.status))
            {
                child.status = TaskStatus::Cancelled;
                append_event(
                    child,
                    "supervisor",
                    "Parent cancelled; child worktree preserved.",
                );
            }
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|t| t.id == task_id)
                .ok_or("Task no longer exists")?;
            if terminal(task.status) {
                return Err("This task has already finished".into());
            }
            task.status = TaskStatus::Cancelled;
            append_event(task, "user", "Cancelled this task.");
        }
        Command::CompleteTask { task_id } => {
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .ok_or("Task no longer exists")?;
            if task.status != TaskStatus::ReadyForReview {
                return Err("Only a task ready for review can be marked complete".into());
            }
            task.status = TaskStatus::Completed;
            append_event(
                task,
                "user",
                "Acknowledged the result and marked this task complete. Nothing merged or published.",
            );
        }
        Command::DeleteTask { task_id } => {
            let task = snapshot.tasks.iter().find(|t| t.id == task_id).ok_or("Task no longer exists")?;
            if !deletable(task.status) {
                return Err("Only a completed, failed or cancelled task can be deleted. Cancel it first".into());
            }
            if snapshot.splits.iter().any(|s| s.subtasks.iter().any(|p| p.task_id.as_deref() == Some(&task_id))) {
                return Err("Delete the parent ticket; its subtasks go with it".into());
            }
            let mut ids = vec![task_id.clone()];
            ids.extend(snapshot.splits.iter().filter(|s| s.parent_id == task_id).flat_map(|s| s.subtasks.iter().filter_map(|p| p.task_id.clone())));
            if snapshot.tasks.iter().any(|t| ids.contains(&t.id) && !deletable(t.status)) {
                return Err("A subtask of this ticket is still running. Cancel it first".into());
            }
            remove_tasks(&mut snapshot, &ids);
        }
        Command::ClearFinishedTasks { workspace_id } => {
            let child_ids: Vec<String> = snapshot.splits.iter().flat_map(|s| s.subtasks.iter().filter_map(|p| p.task_id.clone())).collect();
            let mut ids: Vec<String> = Vec::new();
            for task in snapshot.tasks.iter().filter(|t| {
                matches!(t.status, TaskStatus::Completed | TaskStatus::Cancelled)
                    && !child_ids.contains(&t.id)
                    && workspace_id.as_ref().is_none_or(|w| &t.workspace_id == w)
            }) {
                let children: Vec<String> = snapshot.splits.iter().filter(|s| s.parent_id == task.id).flat_map(|s| s.subtasks.iter().filter_map(|p| p.task_id.clone())).collect();
                if snapshot.tasks.iter().filter(|t| children.contains(&t.id)).all(|t| deletable(t.status)) {
                    ids.push(task.id.clone());
                    ids.extend(children);
                }
            }
            remove_tasks(&mut snapshot, &ids);
        }
        Command::CancelAllWork => {
            for task in snapshot.tasks.iter_mut().filter(|t| !terminal(t.status)) {
                task.status = TaskStatus::Cancelled;
                append_event(task, "user", "Stopped with Stop all work. Worktree preserved.");
            }
        }
        Command::RetryTask { task_id } => {
            if snapshot.splits.iter().any(|s| {
                s.subtasks
                    .iter()
                    .any(|p| p.task_id.as_deref() == Some(&task_id))
            }) {
                return Err("Retry the parent ticket to propose a fresh bounded split".into());
            }
            if let Some(split) = snapshot.splits.iter_mut().find(|s| s.parent_id == task_id) {
                split.approved = false;
                split.integrated = false;
                split.subtasks.clear();
            }
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .ok_or("Task no longer exists")?;
            if !matches!(task.status, TaskStatus::Failed | TaskStatus::Cancelled) {
                return Err("Only a failed or cancelled task can be retried".into());
            }
            task.status = TaskStatus::Queued;
            task.supervision = None;
            append_event(
                task,
                "user",
                "Requested a retry from planning. Previous results and the task worktree are preserved as evidence.",
            );
        }
        Command::StartTask { .. } => return Err("Start a ticket through the workbench".into()),
        Command::ReplyToTask { task_id, text } => {
            let text = text.trim().to_owned();
            required("Reply", &text, MAX_NOTE_BYTES)?;
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|t| t.id == task_id)
                .ok_or("Task no longer exists")?;
            append_event(task, NOTE_ROLE, &text);
            match task.status {
                TaskStatus::AwaitingApproval | TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::Completed => {
                    let reopened = task.status == TaskStatus::Completed;
                    task.status = TaskStatus::Queued;
                    task.supervision = None;
                    append_event(task, "system", if reopened { "Reopened with your reply; the agent decides what to do next." } else { "Picked up your reply; the agent decides what to do next." });
                }
                TaskStatus::ReadyForReview => {
                    task.status = TaskStatus::Building;
                    append_event(task, "system", "Sent back to building with your reply.");
                }
                _ => {}
            }
        }
        Command::SetTaskFolder { task_id, folder } => {
            let task = snapshot
                .tasks
                .iter()
                .find(|t| t.id == task_id)
                .ok_or("Task no longer exists")?;
            // A stopped ticket may move to another repository; its old working
            // copy stays on disk as evidence. A waiting plan keeps its copy.
            if task.worktree.is_some() && task.status == TaskStatus::AwaitingApproval {
                return Err("This ticket already works in its own copy of a repository".into());
            }
            if !matches!(
                task.status,
                TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::AwaitingApproval
            ) {
                return Err("Choose a folder once the ticket has stopped".into());
            }
            let folder = canonical_repository(&folder)
                .map_err(|_| "Choose a folder that is a Git repository")?;
            let workspace = snapshot
                .workspaces
                .iter()
                .find(|w| w.id == task.workspace_id)
                .ok_or("Workspace no longer exists")?;
            if !snapshot
                .folders_for(workspace)
                .iter()
                .any(|root| std::path::Path::new(&folder).starts_with(root))
            {
                return Err("That folder isn’t in this ticket’s workspace. Add it to the workspace first.".into());
            }
            // The old session worked in another repository.
            snapshot.task_sessions.remove(&task_id);
            snapshot.task_roots.insert(task_id.clone(), folder.clone());
            let task = snapshot.tasks.iter_mut().find(|t| t.id == task_id).unwrap();
            match task.worktree.take() {
                Some(old) => append_event(task, "user", &format!("Chose {folder} for this ticket. The previous working copy stays at {old}.")),
                None => append_event(task, "user", &format!("Chose {folder} for this ticket.")),
            }
            task.plan.clear();
            task.supervision = None;
        }
        Command::SetConnectionEnabled {
            connection_id,
            enabled,
        } => {
            snapshot
                .connections
                .iter_mut()
                .find(|c| c.id == connection_id)
                .ok_or("Connection no longer exists")?
                .enabled = enabled;
        }
        Command::ConnectLinear { .. } | Command::SyncLinear { .. } => {
            return Err("This command requires the workbench orchestrator".into());
        }
        Command::SetupImport(_) => return Err("Import requires the daemon".into()),
        Command::SendMessage { .. }
        | Command::InterruptAndSendMessage { .. }
        | Command::DecideChatTool { .. }
        | Command::CancelChat { .. } => {
            return Err("Talking to Neko requires the daemon".into());
        }
        Command::AddTicketNote { task_id, text } => {
            let text = text.trim().to_owned();
            required("Note", &text, MAX_NOTE_BYTES)?;
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|t| t.id == task_id)
                .ok_or("Task no longer exists")?;
            if task.status == TaskStatus::Completed {
                return Err("This ticket is done. Ask Neko for a follow-up instead".into());
            }
            append_event(task, NOTE_ROLE, &text);
        }
        Command::SaveMemory { entry } => {
            crate::agent_profiles::validate_memory(&snapshot, &entry)?;
            let known: Vec<String> = snapshot.workspaces.iter().map(|w| w.id.clone()).collect();
            crate::neko_memory::upsert(db, entry, &known)?;
            return load(db);
        }
        Command::DeleteMemory { id } => {
            crate::neko_memory::delete(db, &id)?;
            return load(db);
        }
        Command::SetMemoryOptions { options } => {
            crate::neko_memory::set_options(db, options)?;
            return load(db);
        }
        Command::SetTaskBudget { cents } => {
            if cents.is_some_and(|c| c == 0 || c > 100_000) {
                return Err("Choose a budget between $0.01 and $1,000, or none".into());
            }
            snapshot.task_budget_cents = cents;
        }
        Command::SetStartWithoutApproval { enabled } => {
            snapshot.start_without_approval = enabled;
        }
        Command::DecideMemoryProposal { id, accept } => {
            crate::memory_learning::decide(db, &snapshot, &id, accept)?;
            return load(db);
        }
    }
    save(db, &snapshot)?;
    load(db)
}

pub fn now_ms() -> i64 {
    crate::now_unix_ms()
}

/// Random identity is independent of clocks, process restarts and imported IDs.
pub fn new_id() -> String {
    use std::io::Read;
    let mut bytes = [0_u8; 16];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok()
    {
        return bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    }
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ticks = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{ticks:x}-{:x}-{:x}",
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

pub fn append_event(task: &mut Task, role: &str, message: &str) {
    let at_ms = now_ms();
    task.events.push(TaskEvent {
        at_ms,
        role: truncate(role, 64),
        message: truncate(message, MAX_EVENT_BYTES),
    });
    // Evict the oldest progress events first. The user's notes are direction
    // for future runs and go only when nothing else is left to evict.
    while task.events.len() > MAX_EVENTS {
        let index = task
            .events
            .iter()
            .position(|e| e.role != NOTE_ROLE)
            .unwrap_or(0);
        task.events.remove(index);
    }
    task.updated_at_ms = at_ms;
}

/// A restarted daemon cannot promise that an interrupted child completed work,
/// so interrupted work runs again from a safe point: planning re-plans
/// read-only, building and review rebuild in the preserved task worktree.
/// After `MAX_RESTART_RESUMES` such resumes the ticket stops for a person.
/// Queued work and explicit approval waits retain their original state.
pub fn recover_interrupted(db: &Db) -> Result<Snapshot, String> {
    let mut snapshot = load(db)?;
    let mut changed = false;
    for task in &mut snapshot.tasks {
        if matches!(
            task.status,
            TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing
        ) {
            let resumes = task.events.iter().filter(|e| e.message.starts_with(RESUMED)).count();
            if resumes < MAX_RESTART_RESUMES {
                let (status, step) = if task.status == TaskStatus::Planning {
                    (TaskStatus::Queued, "planning")
                } else {
                    (TaskStatus::Building, "building in the same worktree")
                };
                task.status = status;
                append_event(task, "system", &format!("{RESUMED}: {step} again."));
                changed = true;
                continue;
            }
            task.status = TaskStatus::Failed;
            let message = "Interrupted by a daemon restart again after resuming twice. Inspect the worktree, then start it again.";
            if task.result.is_empty() {
                task.result = message.into();
            }
            append_event(task, "system", message);
            changed = true;
        }
    }
    if changed {
        save(db, &snapshot)?;
        return load(db);
    }
    Ok(snapshot)
}

/// Agent cost a ticket has reported so far, from its USAGE events.
pub fn task_cost_usd(task: &neko_protocol::workbench::Task) -> f64 {
    task.events
        .iter()
        .filter_map(|e| e.message.strip_prefix("USAGE "))
        .filter_map(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .filter_map(|v| v["cost_usd"].as_f64())
        .filter(|c| c.is_finite() && *c >= 0.0)
        .sum()
}

/// Err with a plain explanation once the ticket has used its budget.
pub fn within_budget(snapshot: &Snapshot, task_id: &str) -> Result<(), String> {
    let (Some(cents), Some(task)) = (snapshot.task_budget_cents, snapshot.tasks.iter().find(|t| t.id == task_id)) else { return Ok(()) };
    let spent = task_cost_usd(task);
    let budget = f64::from(cents) / 100.0;
    if spent >= budget {
        return Err(format!("Stopped: this ticket reached its ${budget:.2} budget (${spent:.2} reported). Raise the budget in Settings → AI, then retry."));
    }
    Ok(())
}

fn deletable(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
}

/// History only: task worktrees, source evidence and files stay on disk.
/// Source evidence keeps its task id so a watcher never re-files the same item.
fn remove_tasks(snapshot: &mut Snapshot, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    snapshot.tasks.retain(|t| !ids.contains(&t.id));
    snapshot.splits.retain(|s| !ids.contains(&s.parent_id));
    for id in ids {
        snapshot.task_roots.remove(id);
        snapshot.start_when_planned.remove(id);
        snapshot.task_sessions.remove(id);
    }
    for schedule in &mut snapshot.schedules {
        if schedule.last_task_id.as_ref().is_some_and(|id| ids.contains(id)) {
            schedule.last_task_id = None;
        }
    }
}

fn terminal(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::ReadyForReview
            | TaskStatus::Completed
            | TaskStatus::Failed
            | TaskStatus::Cancelled
    )
}

/// Approve a waiting plan, or retry a stopped ticket and remember to build it
/// as soon as it is planned. Runs as two writes because the store has no
/// nested transactions; the daemon holds its database lock across both.
fn start_task(db: &Db, task_id: String) -> Result<Snapshot, String> {
    let snapshot = load(db)?;
    let status = snapshot
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or("Task no longer exists")?
        .status;
    match status {
        TaskStatus::AwaitingApproval => apply(db, Command::ApproveTask { task_id }),
        TaskStatus::Failed | TaskStatus::Cancelled => {
            apply(db, Command::RetryTask { task_id: task_id.clone() })?;
            let mut snapshot = load(db)?;
            snapshot.start_when_planned.insert(task_id);
            save(db, &snapshot)?;
            load(db)
        }
        TaskStatus::Completed => Err("This ticket is already done".into()),
        _ => Err("This ticket is already working".into()),
    }
}

/// A Git checkout found inside a workspace folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryCandidate {
    pub path: String,
    /// The origin remote URL, when the checkout has one.
    pub remote: String,
    /// The `name` in its top-level package.json, when it has one.
    pub package: String,
}

const SKIPPED_FOLDERS: &[&str] = &[
    "Library", "Applications", "Movies", "Music", "Pictures", "Public", "node_modules",
    "target", "build", "dist", "Pods", "DerivedData", "vendor", "venv",
];

/// Main Git checkouts (with a `.git` directory) inside the given folders.
/// Bounded in depth, directories visited and results; symlinks are not followed.
pub fn discover_repositories(folders: &[String], cancel: &std::sync::atomic::AtomicBool) -> Vec<RepositoryCandidate> {
    const MAX_DEPTH: usize = 5;
    const MAX_VISITED: usize = 30_000;
    const MAX_FOUND: usize = 300;
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut visited = 0;
    let mut queue: std::collections::VecDeque<(std::path::PathBuf, usize)> =
        folders.iter().map(|f| (std::path::PathBuf::from(f), 0)).collect();
    while let Some((dir, depth)) = queue.pop_front() {
        if visited >= MAX_VISITED || found.len() >= MAX_FOUND || cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        visited += 1;
        let git = dir.join(".git");
        if git.symlink_metadata().is_ok_and(|m| m.is_dir()) {
            if let Some(path) = dir.to_str().filter(|p| seen.insert(p.to_string())) {
                found.push(RepositoryCandidate { path: path.to_owned(), remote: origin_url(&git.join("config")), package: package_name(&dir) });
            }
            continue;
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut children: Vec<_> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| !n.starts_with('.') && !SKIPPED_FOLDERS.contains(&n))
            })
            .collect();
        children.sort();
        queue.extend(children.into_iter().map(|p| (p, depth + 1)));
    }
    found
}

fn origin_url(config: &std::path::Path) -> String {
    let Ok(text) = std::fs::read_to_string(config) else { return String::new() };
    let mut in_origin = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
        } else if in_origin {
            if let Some(url) = line.strip_prefix("url").map(str::trim_start).and_then(|l| l.strip_prefix('=')) {
                return url.trim().to_owned();
            }
        }
    }
    String::new()
}

fn package_name(dir: &std::path::Path) -> String {
    std::fs::read(dir.join("package.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_owned))
        .unwrap_or_default()
}

/// Names too generic to identify a repository from ticket text.
const GENERIC_NAMES: &[&str] = &[
    "init", "app", "apps", "web", "main", "repo", "test", "tests", "code", "site", "src", "core",
    "api", "server", "client", "frontend", "backend", "project", "demo", "docs", "data", "tools",
];

/// Lowercase words separated by single spaces, padded so ` word ` matches whole words only.
fn words(text: &str) -> String {
    let mut out = String::from(" ");
    for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        out.push_str(word);
        out.push(' ');
    }
    out
}

/// The one repository whose folder, remote or package name appears in the
/// ticket as whole words. Generic names like `init` or `web` never count.
pub fn repository_named_in(candidates: &[RepositoryCandidate], title: &str, goal: &str) -> Option<String> {
    let text = words(&format!("{title} {goal}"));
    let mut matches = candidates.iter().filter(|c| {
        let folder = std::path::Path::new(&c.path).file_name().and_then(|n| n.to_str()).unwrap_or("");
        let remote = c.remote.trim_end_matches('/').trim_end_matches(".git").rsplit(['/', ':']).next().unwrap_or("");
        let package = c.package.rsplit('/').next().unwrap_or("");
        [folder, remote, package].iter().any(|name| {
            let name = words(name);
            name.trim().len() >= 4 && !GENERIC_NAMES.contains(&name.trim()) && text.contains(&name)
        })
    });
    let first = matches.next()?;
    matches.next().is_none().then(|| first.path.clone())
}

fn route_folder(
    snapshot: &Snapshot,
    workspace: &neko_protocol::workbench::Workspace,
    title: &str,
    goal: &str,
) -> Result<String, String> {
    let folders = snapshot.folders_for(workspace);
    if folders.len() == 1 {
        return Ok(folders[0].clone());
    }
    let request = format!("{title} {goal}").to_lowercase();
    let matches = folders.iter().filter(|folder| {
        std::path::Path::new(folder)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.len() >= 3 && request.contains(&name.to_lowercase()))
    }).collect::<Vec<_>>();
    match matches.as_slice() {
        [folder] => Ok((*folder).clone()),
        _ => Err("More than one folder is available. Ask Neko to choose the right one or name the folder in your task.".into()),
    }
}

pub fn create_task(
    workspace_id: String,
    issue_id: Option<String>,
    title: String,
    goal: String,
) -> Result<Task, String> {
    let title = title.trim().to_owned();
    let goal = goal.trim().to_owned();
    required("Task title", &title, 1024)?;
    required("Task goal", &goal, MAX_CONTENT_BYTES)?;
    let now = now_ms();
    let mut task = Task {
        id: new_id(),
        workspace_id,
        issue_id,
        title,
        goal,
        status: TaskStatus::Queued,
        plan: String::new(),
        result: String::new(),
        worktree: None,
        events: Vec::new(),
        created_at_ms: now,
        updated_at_ms: now,
        source_revision: None,
        supervision: None,
    };
    append_event(&mut task, "system", "Queued for planning.");
    Ok(task)
}

/// Validate a workspace path without changing workbench state. Import uses
/// this to preflight every selected workspace before writing any of them.
pub fn canonical_workspace_directory(path: &str) -> Result<String, String> {
    required("Workspace folder", path, 4096)?;
    let directory = std::path::Path::new(path)
        .canonicalize()
        .map_err(|_| "Workspace folder does not exist")?;
    if !directory.is_dir() {
        return Err("Workspace folder must be a directory".into());
    }
    if let Some(reason) = protected_folder(&directory) {
        return Err(format!("Neko won’t use {} as a workspace: {reason}", directory.display()));
    }
    directory
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Workspace folder path must be valid UTF-8".into())
}

/// Folders agents must never work in, whatever is asked: system locations,
/// credentials, and Neko's own data. A home folder is allowed; one of these
/// inside it is not. Paths are canonical (symlinks resolved) when checked.
pub fn protected_folder(path: &std::path::Path) -> Option<&'static str> {
    if path == std::path::Path::new("/") || path == std::path::Path::new("/Users") {
        return Some("it contains every user’s files");
    }
    const SYSTEM: &[&str] = &[
        "/System", "/bin", "/sbin", "/usr/bin", "/usr/sbin", "/usr/lib", "/usr/libexec", "/private/etc",
        "/private/var/db", "/Library/LaunchDaemons", "/Library/LaunchAgents", "/Library/Keychains",
    ];
    if SYSTEM.iter().any(|p| path.starts_with(p)) {
        return Some("it is a protected system location");
    }
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        let home = home.canonicalize().unwrap_or(home);
        const SECRETS: &[&str] = &[".ssh", ".aws", ".gnupg", ".kube", ".docker", ".config/gh", "Library/Keychains", "Library/Cookies", "Library/Mail", "Library/Messages"];
        if SECRETS.iter().any(|p| path.starts_with(home.join(p))) {
            return Some("it holds credentials or private data");
        }
    }
    if path.starts_with(neko_protocol::support_dir()) {
        return Some("it is Neko’s own data folder");
    }
    None
}

/// Code-changing tasks require a Git checkout, even though workspace context
/// itself may be any existing local folder.
pub fn canonical_repository(path: &str) -> Result<String, String> {
    required("Repository path", path, 4096)?;
    let path = std::path::Path::new(path)
        .canonicalize()
        .map_err(|_| "Repository directory does not exist")?;
    if !path.is_dir() {
        return Err("Repository must be a directory".into());
    }
    let mut command = std::process::Command::new("git");
    // Git context inherited from another tool must not redirect validation.
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_CEILING_DIRECTORIES",
    ] {
        command.env_remove(name);
    }
    let output = command
        .args(["--no-optional-locks", "-C"])
        .arg(&path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("Cannot inspect repository: {e}"))?;
    if !output.status.success() {
        return Err("Choose a directory inside a Git working repository".into());
    }
    let root =
        String::from_utf8(output.stdout).map_err(|_| "Repository path must be valid UTF-8")?;
    let root = std::path::Path::new(root.trim_end_matches(['\r', '\n']))
        .canonicalize()
        .map_err(|_| "Repository root does not exist")?;
    root.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Repository path must be valid UTF-8".into())
}

fn truncate(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn bounded(label: &str, text: &str, limit: usize) -> Result<(), String> {
    if text.len() > limit {
        Err(format!("{label} exceeds {limit} bytes"))
    } else {
        Ok(())
    }
}

fn required(label: &str, text: &str, limit: usize) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(format!("{label} is required"));
    }
    bounded(label, text, limit)
}

fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Result<HashSet<&'a str>, String> {
    let mut seen = HashSet::new();
    for id in ids {
        required("ID", id, 256)?;
        if !seen.insert(id) {
            return Err("Duplicate workbench ID".into());
        }
    }
    Ok(seen)
}

fn validate(snapshot: &Snapshot) -> Result<(), String> {
    crate::agent_profiles::validate(snapshot)?;
    crate::scheduled_plans::validate(snapshot)?;
    if snapshot.splits.len() > snapshot.tasks.len() {
        return Err("Split history exceeds task history".into());
    }
    let mut parents = HashSet::new();
    let mut children = HashSet::new();
    for split in &snapshot.splits {
        if !parents.insert(&split.parent_id)
            || !snapshot.tasks.iter().any(|t| t.id == split.parent_id)
        {
            return Err("Invalid split parent".into());
        }
        if !split.subtasks.is_empty() {
            crate::decomposition::validate(&split.subtasks)?;
        }
        if split.approved && split.subtasks.is_empty() {
            return Err("Approved split has no subtasks".into());
        }
        for p in &split.subtasks {
            if let Some(id) = &p.task_id {
                if !children.insert(id) || !snapshot.tasks.iter().any(|t| &t.id == id) {
                    return Err("Invalid or duplicate subtask claim".into());
                }
            } else if split.approved {
                return Err("Approved subtask missing its ticket".into());
            }
        }
    }
    crate::mcp_host::store::validate(snapshot)?;
    if snapshot.workspaces.len() > 100
        || snapshot.connections.len() > 100
        || snapshot.issues.len() > 5000
        || snapshot.tasks.len() > 1000
    {
        return Err("Workbench record limit reached".into());
    }
    let workspaces = unique_ids(snapshot.workspaces.iter().map(|w| w.id.as_str()))?;
    unique_ids(snapshot.connections.iter().map(|c| c.id.as_str()))?;
    unique_ids(snapshot.issues.iter().map(|i| i.id.as_str()))?;
    unique_ids(snapshot.tasks.iter().map(|t| t.id.as_str()))?;
    for workspace in &snapshot.workspaces {
        required("Workspace name", &workspace.name, 256)?;
        required("Repository path", &workspace.repository, 4096)?;
        bounded(
            "Workspace instructions",
            &workspace.instructions,
            MAX_CONTENT_BYTES,
        )?;
    }
    for (workspace_id, folders) in &snapshot.workspace_folders {
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|workspace| &workspace.id == workspace_id)
            .ok_or("Folder list references a missing workspace")?;
        if folders.is_empty()
            || folders.len() > 20
            || folders[0] != workspace.repository
            || folders.iter().collect::<HashSet<_>>().len() != folders.len()
        {
            return Err("Invalid workspace folders".into());
        }
        for folder in folders {
            required("Workspace folder", folder, 4096)?;
        }
    }
    for (task_id, root) in &snapshot.task_roots {
        if !snapshot.tasks.iter().any(|task| &task.id == task_id) {
            return Err("Task root references a missing task".into());
        }
        required("Task root", root, 4096)?;
    }
    for connection in &snapshot.connections {
        if !workspaces.contains(connection.workspace_id.as_str()) {
            return Err("Connection references a missing workspace".into());
        }
        required("Connection name", &connection.name, 256)?;
        required("Organization ID", &connection.organization_id, 256)?;
        required("Viewer ID", &connection.viewer_id, 256)?;
        if connection.team_ids.len() > 100 || connection.project_ids.len() > 100 {
            return Err("Too many connection filters".into());
        }
        for id in connection.team_ids.iter().chain(&connection.project_ids) {
            required("Filter ID", id, 256)?;
        }
        if let Some(error) = &connection.error {
            bounded("Connection error", error, MAX_CONTENT_BYTES)?;
        }
        if let Some(notice) = &connection.intake_notice {
            bounded("Intake notice", notice, MAX_CONTENT_BYTES)?;
        }
    }
    let mut external_issues = HashSet::new();
    for issue in &snapshot.issues {
        if !snapshot
            .connections
            .iter()
            .any(|c| c.id == issue.connection_id && c.workspace_id == issue.workspace_id)
        {
            return Err("Issue connection and workspace do not match".into());
        }
        if !external_issues.insert((&issue.connection_id, &issue.external_id)) {
            return Err("Duplicate external issue".into());
        }
        required("External issue ID", &issue.external_id, 256)?;
        required("Issue identifier", &issue.identifier, 256)?;
        required("Issue title", &issue.title, 1024)?;
        bounded("Issue description", &issue.description, MAX_CONTENT_BYTES)?;
        bounded("Issue URL", &issue.url, 4096)?;
        bounded("Issue update time", &issue.updated_at, 128)?;
    }
    let mut planned_issues = HashSet::new();
    for task in &snapshot.tasks {
        if !workspaces.contains(task.workspace_id.as_str()) {
            return Err("Task references a missing workspace".into());
        }
        if let Some(issue_id) = &task.issue_id {
            if !snapshot
                .issues
                .iter()
                .any(|i| &i.id == issue_id && i.workspace_id == task.workspace_id)
            {
                return Err("Task issue and workspace do not match".into());
            }
            if !planned_issues.insert(issue_id) {
                return Err("Issue already has a task".into());
            }
        }
        required("Task title", &task.title, 1024)?;
        required("Task goal", &task.goal, MAX_CONTENT_BYTES)?;
        bounded("Task plan", &task.plan, MAX_CONTENT_BYTES * 2)?;
        bounded("Task result", &task.result, MAX_RESULT_BYTES)?;
        if let Some(revision) = &task.source_revision {
            bounded("Source revision", revision, 128)?;
        }
        if let Some(decision) = &task.supervision {
            crate::supervision::validate_decision(decision)?;
        }
        if let Some(path) = &task.worktree {
            bounded("Worktree path", path, 4096)?;
        }
        if task.events.len() > MAX_EVENTS {
            return Err("Task event limit reached".into());
        }
        for event in &task.events {
            bounded("Event role", &event.role, 64)?;
            bounded("Event message", &event.message, MAX_CONTENT_BYTES)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn ticket_budgets_count_reported_cost_only() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let ticket = task(&db, &ws);
        let mut state = load(&db).unwrap();
        for (i, message) in ["USAGE {\"cost_usd\":0.30}", "USAGE {\"input_tokens\":5}", "USAGE {\"cost_usd\":0.25}", "USAGE not json", "Command completed"].iter().enumerate() {
            state.tasks[0].events.push(TaskEvent { at_ms: i as i64, role: "builder".into(), message: (*message).into() });
        }
        save(&db, &state).unwrap();
        let state = apply(&db, Command::SetTaskBudget { cents: Some(100) }).unwrap();
        assert!((task_cost_usd(&state.tasks[0]) - 0.55).abs() < 1e-9);
        assert!(within_budget(&state, &ticket.id).is_ok());
        let state = apply(&db, Command::SetTaskBudget { cents: Some(50) }).unwrap();
        assert!(within_budget(&state, &ticket.id).unwrap_err().contains("$0.50 budget ($0.55 reported)"));
        assert!(apply(&db, Command::SetTaskBudget { cents: Some(0) }).is_err());
        let state = apply(&db, Command::SetTaskBudget { cents: None }).unwrap();
        assert!(!state.start_without_approval, "tickets wait for approval unless you opt in");
        assert!(apply(&db, Command::SetStartWithoutApproval { enabled: true }).unwrap().start_without_approval);
        assert!(!apply(&db, Command::SetStartWithoutApproval { enabled: false }).unwrap().start_without_approval);
        assert!(within_budget(&state, &ticket.id).is_ok());
    }

    #[test]
    fn protected_folders_cannot_become_workspaces() {
        use std::path::Path;
        assert!(protected_folder(Path::new("/")).is_some());
        assert!(protected_folder(Path::new("/System/Library")).is_some());
        assert!(protected_folder(Path::new("/usr/bin")).is_some());
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
        assert!(protected_folder(&home.join(".ssh")).is_some());
        assert!(protected_folder(&home.join("Library/Keychains/x")).is_some());
        assert!(protected_folder(&home).is_none(), "the home workspace stays allowed");
        assert!(protected_folder(&home.join("Documents/project")).is_none());
        assert!(protected_folder(Path::new("/usr/local/src/app")).is_none());
        assert!(canonical_workspace_directory("/System").unwrap_err().contains("protected system location"));
    }

    #[test]
    fn finished_tasks_can_be_deleted_or_cleared_and_stop_all_cancels_the_rest() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let running = task(&db, &ws);
        let done = task(&db, &ws);
        let failed = task(&db, &ws);
        let mut state = load(&db).unwrap();
        for t in &mut state.tasks {
            t.status = if t.id == done.id { TaskStatus::Completed } else if t.id == failed.id { TaskStatus::Failed } else { TaskStatus::Building };
        }
        state.task_roots.insert(done.id.clone(), repo.path().to_string_lossy().into());
        save(&db, &state).unwrap();

        assert!(apply(&db, Command::DeleteTask { task_id: running.id.clone() }).unwrap_err().contains("Cancel it first"));
        let cleared = apply(&db, Command::ClearFinishedTasks { workspace_id: None }).unwrap();
        assert!(!cleared.tasks.iter().any(|t| t.id == done.id), "completed task cleared");
        assert!(!cleared.task_roots.contains_key(&done.id));
        assert!(cleared.tasks.iter().any(|t| t.id == failed.id), "failed tasks stay for retry");
        let deleted = apply(&db, Command::DeleteTask { task_id: failed.id.clone() }).unwrap();
        assert!(!deleted.tasks.iter().any(|t| t.id == failed.id));

        let stopped = apply(&db, Command::CancelAllWork).unwrap();
        let survivor = stopped.tasks.iter().find(|t| t.id == running.id).unwrap();
        assert_eq!(survivor.status, TaskStatus::Cancelled);
        assert!(survivor.events.last().unwrap().message.contains("Stop all work"));
        assert!(repo.path().exists(), "files are never touched");
    }

    #[test]
    fn starting_a_stopped_ticket_retries_it_and_builds_once_planned() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let stopped = task(&db, &ws);
        let waiting = task(&db, &ws);
        let mut state = load(&db).unwrap();
        for t in &mut state.tasks {
            t.status = if t.id == stopped.id { TaskStatus::Failed } else { TaskStatus::AwaitingApproval };
        }
        save(&db, &state).unwrap();

        let state = apply(&db, Command::StartTask { task_id: stopped.id.clone() }).unwrap();
        assert_eq!(state.tasks.iter().find(|t| t.id == stopped.id).unwrap().status, TaskStatus::Queued);
        assert!(state.start_when_planned.contains(&stopped.id));
        let state = apply(&db, Command::StartTask { task_id: waiting.id.clone() }).unwrap();
        assert_eq!(state.tasks.iter().find(|t| t.id == waiting.id).unwrap().status, TaskStatus::Building);
        assert!(!state.start_when_planned.contains(&waiting.id), "an approved plan needs no flag");
        assert!(apply(&db, Command::StartTask { task_id: waiting.id.clone() }).unwrap_err().contains("already working"));
    }

    #[test]
    fn replying_to_a_ticket_hands_the_next_move_to_its_agent() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let ids: Vec<String> = (0..4).map(|_| task(&db, &ws).id).collect();
        let mut state = load(&db).unwrap();
        for (t, status) in state.tasks.iter_mut().zip([TaskStatus::AwaitingApproval, TaskStatus::ReadyForReview, TaskStatus::Building, TaskStatus::Failed]) {
            t.status = status;
        }
        state.task_sessions.insert(ids[0].clone(), "session".into());
        save(&db, &state).unwrap();
        let reply = |id: &String| apply(&db, Command::ReplyToTask { task_id: id.clone(), text: "Use the staging config".into() }).unwrap();
        let status = |s: &Snapshot, id: &String| s.tasks.iter().find(|t| &t.id == id).unwrap().status;
        let s = reply(&ids[0]);
        assert_eq!(status(&s, &ids[0]), TaskStatus::Queued, "a waiting agent picks the reply up");
        assert_eq!(s.task_sessions[&ids[0]], "session", "and keeps its memory");
        assert!(s.tasks.iter().find(|t| t.id == ids[0]).unwrap().events.iter().any(|e| e.role == NOTE_ROLE && e.message == "Use the staging config"));
        assert_eq!(status(&reply(&ids[1]), &ids[1]), TaskStatus::Building, "review feedback goes back to building");
        assert_eq!(status(&reply(&ids[2]), &ids[2]), TaskStatus::Building, "running work reads it next step");
        assert_eq!(status(&reply(&ids[3]), &ids[3]), TaskStatus::Queued);
        assert!(apply(&db, Command::ReplyToTask { task_id: ids[0].clone(), text: "  ".into() }).is_err());
    }

    #[test]
    fn a_stopped_ticket_can_be_pointed_at_a_repository_in_its_workspace() {
        let db = Db::open_in_memory().unwrap();
        let home = tempfile::tempdir().unwrap();
        let repo = home.path().join("app");
        std::fs::create_dir(&repo).unwrap();
        assert!(std::process::Command::new("git").arg("init").arg("-q").arg(&repo).status().unwrap().success());
        let ws = workspace(&db, repo.as_path());
        let mut state = load(&db).unwrap();
        state.workspaces[0].repository = home.path().canonicalize().unwrap().to_string_lossy().into();
        state.workspace_folders.insert(ws.id.clone(), vec![state.workspaces[0].repository.clone()]);
        let t = create_task(ws.id.clone(), None, "Fix login".into(), "Login fails".into()).unwrap();
        let id = t.id.clone();
        state.tasks.push(Task { status: TaskStatus::Failed, ..t });
        save(&db, &state).unwrap();

        let outside = repository();
        assert!(apply(&db, Command::SetTaskFolder { task_id: id.clone(), folder: outside.path().to_string_lossy().into() })
            .unwrap_err().contains("isn’t in this ticket’s workspace"));
        assert!(apply(&db, Command::SetTaskFolder { task_id: id.clone(), folder: home.path().to_string_lossy().into() })
            .unwrap_err().contains("Git repository"));
        let state = apply(&db, Command::SetTaskFolder { task_id: id.clone(), folder: repo.to_string_lossy().into() }).unwrap();
        assert_eq!(state.task_roots[&id], repo.canonicalize().unwrap().to_string_lossy());
    }

    #[test]
    fn repositories_are_found_inside_workspace_folders_and_matched_by_name() {
        let home = tempfile::tempdir().unwrap();
        for path in ["Documents/webfrontend", "Documents/hme/athena", "Library/cache-repo", "Documents/webfrontend/nested"] {
            std::fs::create_dir_all(home.path().join(path).join(".git")).unwrap();
        }
        std::fs::write(home.path().join("Documents/hme/athena/.git/config"), "[core]\n[remote \"origin\"]\n\turl = git@github.com:acme/diagnostics.git\n").unwrap();
        let found = discover_repositories(&[home.path().to_string_lossy().into()], &std::sync::atomic::AtomicBool::new(false));
        let paths: Vec<_> = found.iter().map(|c| c.path.rsplit('/').next().unwrap().to_owned()).collect();
        assert_eq!(paths, ["webfrontend", "athena"], "shallow first; skips Library and does not descend into a checkout");
        assert_eq!(found[1].remote, "git@github.com:acme/diagnostics.git");
        let pick = |title: &str| repository_named_in(&found, title, "").map(|p| p.rsplit('/').next().unwrap().to_owned());
        assert_eq!(pick("Error on GET /web/login in webfrontend-ssr").as_deref(), Some("webfrontend"));
        assert_eq!(pick("Diagnostics booking fails").as_deref(), Some("athena"), "remote name counts");
        assert_eq!(pick("Something vague"), None);
        let tricky = vec![RepositoryCandidate { path: "/x/basel".into(), remote: "https://github.com/curtainbasel/init.git".into(), package: "basel".into() }];
        assert_eq!(repository_named_in(&tricky, "Channel hasn't been initialized", "uninitialized channel"), None, "no substring or generic-name matches");
        assert!(repository_named_in(&tricky, "Basel checkout breaks", "").is_some());
    }

    #[test]
    fn decisions_remain_exact_when_notes_fill_the_event_history() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let ticket = task(&db, &ws);
        let mut state = load(&db).unwrap();
        state.tasks[0].status = TaskStatus::AwaitingApproval;
        state.tasks[0].events = (0..MAX_EVENTS)
            .map(|i| TaskEvent {
                at_ms: i as i64,
                role: NOTE_ROLE.into(),
                message: format!("Retained note {i}"),
            })
            .collect();
        save(&db, &state).unwrap();
        let approved = apply(
            &db,
            Command::ApproveTask {
                task_id: ticket.id.clone(),
            },
        )
        .unwrap();
        let decision = approved
            .memory
            .iter()
            .find(|m| m.source.contains(":approval:"))
            .unwrap();
        assert_eq!(decision.text, "Approved the plan for a local build.");
        let cancelled = apply(&db, Command::CancelTask { task_id: ticket.id }).unwrap();
        let decision = cancelled
            .memory
            .iter()
            .find(|m| m.source.contains(":cancellation:"))
            .unwrap();
        assert_eq!(decision.text, "Cancelled this task.");
    }
    #[test]
    fn full_learning_queue_never_blocks_ticket_completion() {
        let db = Db::open_in_memory().unwrap();
        for _ in 0..32 {
            let id = crate::neko_chat::begin_turn(&db, "Evidence").unwrap();
            crate::neko_chat::finish_turn(&db, &id, "Answer", vec![], false).unwrap();
            crate::memory_learning::enqueue(
                &db,
                &load(&db).unwrap(),
                crate::memory_learning::Source::Chat(id),
            )
            .unwrap();
        }
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let ticket = task(&db, &ws);
        let mut state = load(&db).unwrap();
        state.tasks[0].status = TaskStatus::ReadyForReview;
        save(&db, &state).unwrap();
        let state = apply(&db, Command::CompleteTask { task_id: ticket.id }).unwrap();
        assert_eq!(state.tasks[0].status, TaskStatus::Completed);
        assert!(
            state.tasks[0]
                .events
                .iter()
                .any(|e| e.role == "learning" && e.message.contains("queue is full"))
        );
    }
    #[test]
    fn successful_ticket_steering_is_a_durable_exact_decision() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let task = task(&db, &ws);
        let noted = apply(
            &db,
            Command::AddTicketNote {
                task_id: task.id.clone(),
                text: "Use the existing parser".into(),
            },
        )
        .unwrap();
        assert!(
            noted
                .memory
                .iter()
                .any(|m| m.kind == neko_protocol::workbench::MemoryKind::Decision
                    && m.text.contains("Use the existing parser")
                    && m.source.contains(&task.id))
        );
        let cancelled = apply(
            &db,
            Command::CancelTask {
                task_id: task.id.clone(),
            },
        )
        .unwrap();
        assert!(
            cancelled
                .memory
                .iter()
                .any(|m| m.text == "Cancelled this task.")
        );
        let count = cancelled.memory.len();
        assert!(apply(&db, Command::CancelTask { task_id: task.id }).is_err());
        assert_eq!(load(&db).unwrap().memory.len(), count);
    }
    use super::*;
    use neko_protocol::workbench::{Issue, LinearConnection, TaskStatus, Workspace};

    #[test]
    fn new_workspace_inherits_discovered_global_connection_tools() {
        use neko_protocol::mcp_host::{McpConnection, McpTool, ServerConfig};
        let db = Db::open_in_memory().unwrap();
        let mut initial = Snapshot::default();
        initial.mcp.connections.push(McpConnection {
            id: "global".into(),
            workspace_id: String::new(),
            label: "Global".into(),
            config: ServerConfig::Http {
                url: "https://example.com/mcp".into(),
            },
            enabled: true,
            trusted: true,
            oauth: false,
            has_credentials: false,
            tools: vec![McpTool {
                name: "lookup".into(),
                description: String::new(),
                input_schema: "{}".into(),
                schema_hash: "v1".into(),
                read_only: true,
            }],
            discovered_ms: Some(1),
            error: None,
            source_link: None,
        });
        save(&db, &initial).unwrap();
        let folder = tempfile::tempdir().unwrap();
        let state = apply(
            &db,
            Command::SaveWorkspace {
                workspace: Workspace {
                    id: String::new(),
                    name: "New".into(),
                    repository: folder.path().to_string_lossy().into(),
                    instructions: String::new(),
                    away_enabled: false,
                },
            },
        )
        .unwrap();
        assert!(crate::mcp_host::store::authorize(
            &state,
            &state.workspaces[0].id,
            "global",
            "lookup"
        )
        .is_ok());
    }

    #[test]
    fn runtime_choice_is_validated_and_persisted() {
        let db = Db::open_in_memory().unwrap();
        let runtime = neko_protocol::workbench::AgentRuntime {
            provider: "ollama".into(),
            model: "qwen3:8b".into(),
        };
        apply(&db, Command::SetAgentRuntime { runtime: runtime.clone() }).unwrap();
        assert_eq!(load(&db).unwrap().agent_runtime, runtime);
        let routed = neko_protocol::workbench::AgentRuntime {
            provider: "opencodex".into(), model: "anthropic/claude-sonnet-5".into(),
        };
        apply(&db, Command::SetAgentRuntime { runtime: routed.clone() }).unwrap();
        assert_eq!(load(&db).unwrap().agent_runtime, routed);
        assert!(apply(&db, Command::SetAgentRuntime {
            runtime: neko_protocol::workbench::AgentRuntime {
                provider: "opencodex".into(), model: "/".into(),
            }
        }).is_err());
        assert!(apply(&db, Command::SetAgentRuntime {
            runtime: neko_protocol::workbench::AgentRuntime {
                provider: "unknown".into(), model: String::new()
            }
        }).is_err());
        assert_eq!(load(&db).unwrap().agent_runtime, routed);
    }

    #[test]
    fn migration_never_writes_a_snapshot_that_exceeds_the_read_limit() {
        let db = Db::open_in_memory().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.workspaces.push(Workspace {
            id: "w".into(),
            name: "W".into(),
            repository: "/tmp/w".into(),
            instructions: String::new(),
            away_enabled: true,
        });
        for _ in 0..64 {
            let mut task = create_task("w".into(), None, "Keep".into(), "Keep".into()).unwrap();
            task.status = TaskStatus::Completed;
            snapshot.tasks.push(task);
        }
        let mut legacy = serde_json::to_value(&snapshot).unwrap();
        legacy.as_object_mut().unwrap().remove("mcp");
        let target = MAX_SNAPSHOT_BYTES - 1;
        let mut remaining = target - serde_json::to_vec(&legacy).unwrap().len();
        for task in legacy["tasks"].as_array_mut().unwrap() {
            let size = remaining.min(MAX_RESULT_BYTES);
            task["result"] = serde_json::Value::String("x".repeat(size));
            remaining -= size;
        }
        assert_eq!(remaining, 0);
        let raw = serde_json::to_string(&legacy).unwrap();
        assert_eq!(raw.len(), target);
        db.set_setting(SETTING, &raw).unwrap();
        assert!(
            load(&db).is_err(),
            "migration must reject growth before committing"
        );
        assert_eq!(db.get_setting(SETTING).unwrap().unwrap(), raw);
    }

    #[test]
    fn durable_legacy_migration_preserves_task_artifacts_and_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.workspaces.push(Workspace {
            id: "w".into(),
            name: "W".into(),
            repository: "/tmp/w".into(),
            instructions: String::new(),
            away_enabled: true,
        });
        let mut task =
            create_task("w".into(), None, "Keep task".into(), "Keep goal".into()).unwrap();
        task.plan = "Keep plan".into();
        task.result = "Keep result".into();
        task.worktree = Some("/tmp/keep-worktree".into());
        task.status = TaskStatus::Completed;
        snapshot.tasks.push(task.clone());
        snapshot.connections.push(LinearConnection {
            id: "old".into(),
            workspace_id: "w".into(),
            name: "Old".into(),
            organization_id: "org".into(),
            viewer_id: "viewer".into(),
            team_ids: vec!["team".into()],
            project_ids: vec![],
            enabled: true,
            last_sync_ms: Some(1),
            error: None,
            intake_notice: None,
        });
        snapshot.issues.push(Issue {
            id: "i".into(),
            connection_id: "old".into(),
            workspace_id: "w".into(),
            external_id: "external".into(),
            identifier: "OLD-1".into(),
            title: "Evidence".into(),
            description: "Preserve source".into(),
            url: "https://example.com/issue".into(),
            priority: 1,
            updated_at: "rev".into(),
            assigned: true,
        });
        let mut legacy = serde_json::to_value(&snapshot).unwrap();
        legacy.as_object_mut().unwrap().remove("mcp");
        db.set_setting(SETTING, &serde_json::to_string(&legacy).unwrap())
            .unwrap();
        let migrated = load(&db).unwrap();
        assert_eq!(migrated.tasks[0], task);
        assert_eq!(migrated.issues[0].description, "Preserve source");
        assert!(!migrated.issues[0].assigned);
        assert!(!migrated.connections[0].enabled);
        assert_eq!(migrated.connections[0].id, "old"); // original Keychain identity preserved
        assert!(!migrated.workspaces[0].away_enabled);
        assert_eq!(load(&db).unwrap(), migrated);
        let mut future = migrated;
        future.mcp.version = 999;
        let raw = serde_json::to_string(&future).unwrap();
        db.set_setting(SETTING, &raw).unwrap();
        assert!(load(&db).is_err());
        assert_eq!(db.get_setting(SETTING).unwrap().unwrap(), raw);
    }

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "--quiet"])
                .arg(dir.path())
                .status()
                .unwrap()
                .success()
        );
        dir
    }

    #[test]
    fn home_workspace_can_pin_a_ticket_to_a_nested_git_checkout() {
        let db = Db::open_in_memory().unwrap();
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("Documents/project");
        std::fs::create_dir_all(&project).unwrap();
        assert!(std::process::Command::new("git").args(["init", "--quiet"])
            .arg(&project).status().unwrap().success());
        let state = apply(&db, Command::SaveWorkspaceWithFolders {
            workspace: Workspace { id: String::new(), name: "Home".into(),
                repository: home.path().to_string_lossy().into(),
                instructions: String::new(), away_enabled: false },
            folders: vec![home.path().to_string_lossy().into()],
        }).unwrap();
        let task = apply(&db, Command::CreateTaskInFolder {
            workspace_id: state.workspaces[0].id.clone(),
            title: "Repair build".into(), goal: "Fix this repo".into(),
            folder: project.to_string_lossy().into(),
        }).unwrap().tasks.pop().unwrap();
        let saved = load(&db).unwrap();
        assert_eq!(saved.root_for(&task, &saved.workspaces[0]),
            project.canonicalize().unwrap().to_string_lossy());
    }
    fn workspace(db: &Db, repo: &std::path::Path) -> Workspace {
        apply(
            db,
            Command::SaveWorkspace {
                workspace: Workspace {
                    id: String::new(),
                    name: "Product".into(),
                    repository: repo.to_string_lossy().into(),
                    instructions: "Keep changes focused".into(),
                    away_enabled: false,
                },
            },
        )
        .unwrap()
        .workspaces
        .last()
        .unwrap()
        .clone()
    }
    fn task(db: &Db, ws: &Workspace) -> Task {
        apply(
            db,
            Command::CreateTask {
                workspace_id: ws.id.clone(),
                title: "Repair search".into(),
                goal: "Make search correct".into(),
            },
        )
        .unwrap()
        .tasks
        .last()
        .unwrap()
        .clone()
    }
    fn issue(db: &Db, ws: &Workspace) -> Issue {
        let mut snapshot = load(db).unwrap();
        let connection = LinearConnection {
            id: new_id(),
            workspace_id: ws.id.clone(),
            name: "Work".into(),
            organization_id: "org".into(),
            viewer_id: "viewer".into(),
            team_ids: vec![],
            project_ids: vec![],
            enabled: true,
            last_sync_ms: None,
            error: None,
            intake_notice: None,
        };
        let issue = Issue {
            id: new_id(),
            connection_id: connection.id.clone(),
            workspace_id: ws.id.clone(),
            external_id: "external".into(),
            identifier: "ENG-1".into(),
            title: "Fix bug".into(),
            description: "Bug details".into(),
            url: "https://linear.app/example/issue/ENG-1".into(),
            priority: 2,
            updated_at: String::new(),
            assigned: true,
        };
        snapshot.connections.push(connection);
        snapshot.issues.push(issue.clone());
        save(db, &snapshot).unwrap();
        issue
    }

    #[test]
    fn durable_snapshot_survives_database_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repository();
        let path = dir.path().join("neko.db");
        let expected = {
            let db = Db::open(&path).unwrap();
            assert_eq!(load(&db).unwrap(), Snapshot::default());
            let ws = workspace(&db, repo.path());
            assert!(!ws.id.is_empty());
            task(&db, &ws);
            load(&db).unwrap()
        };
        assert_eq!(load(&Db::open(&path).unwrap()).unwrap(), expected);
    }

    #[test]
    fn approval_only_advances_awaiting_approval() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let task = task(&db, &ws);
        for state in [
            TaskStatus::Queued,
            TaskStatus::Planning,
            TaskStatus::Building,
            TaskStatus::Reviewing,
            TaskStatus::ReadyForReview,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ] {
            let mut snapshot = load(&db).unwrap();
            snapshot.tasks[0].status = state;
            save(&db, &snapshot).unwrap();
            assert!(
                apply(
                    &db,
                    Command::ApproveTask {
                        task_id: task.id.clone()
                    }
                )
                .is_err(),
                "{state:?}"
            );
            assert_eq!(load(&db).unwrap(), snapshot);
        }
        let mut snapshot = load(&db).unwrap();
        snapshot.tasks[0].status = TaskStatus::AwaitingApproval;
        save(&db, &snapshot).unwrap();
        assert_eq!(
            apply(&db, Command::ApproveTask { task_id: task.id })
                .unwrap()
                .tasks[0]
                .status,
            TaskStatus::Building
        );
    }

    #[test]
    fn cancellation_preserves_terminal_results() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let task = task(&db, &ws);
        for state in [
            TaskStatus::Queued,
            TaskStatus::Planning,
            TaskStatus::AwaitingApproval,
            TaskStatus::Building,
            TaskStatus::Reviewing,
        ] {
            let mut snapshot = load(&db).unwrap();
            snapshot.tasks[0].status = state;
            save(&db, &snapshot).unwrap();
            assert_eq!(
                apply(
                    &db,
                    Command::CancelTask {
                        task_id: task.id.clone()
                    }
                )
                .unwrap()
                .tasks[0]
                    .status,
                TaskStatus::Cancelled
            );
        }
        for state in [
            TaskStatus::ReadyForReview,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ] {
            let mut snapshot = load(&db).unwrap();
            snapshot.tasks[0].status = state;
            save(&db, &snapshot).unwrap();
            assert!(
                apply(
                    &db,
                    Command::CancelTask {
                        task_id: task.id.clone()
                    }
                )
                .is_err()
            );
            assert_eq!(load(&db).unwrap(), snapshot);
        }
    }

    #[test]
    fn duplicate_issue_planning_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let issue = issue(&db, &ws);
        let first = apply(
            &db,
            Command::PlanIssue {
                issue_id: issue.id.clone(),
            },
        )
        .unwrap();
        let second = apply(&db, Command::PlanIssue { issue_id: issue.id }).unwrap();
        assert_eq!(first, second);
        assert_eq!(second.tasks.len(), 1);
        assert_eq!(second.tasks[0].workspace_id, ws.id);
    }

    #[test]
    fn completing_only_acknowledges_ready_results_and_preserves_evidence() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let original = task(&db, &ws);
        for status in [
            TaskStatus::Queued,
            TaskStatus::Planning,
            TaskStatus::AwaitingApproval,
            TaskStatus::Building,
            TaskStatus::Reviewing,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
            TaskStatus::Completed,
        ] {
            let mut snapshot = load(&db).unwrap();
            snapshot.tasks[0].status = status;
            save(&db, &snapshot).unwrap();
            assert!(
                apply(
                    &db,
                    Command::CompleteTask {
                        task_id: original.id.clone()
                    }
                )
                .is_err(),
                "{status:?}"
            );
            assert_eq!(load(&db).unwrap(), snapshot);
        }
        let mut snapshot = load(&db).unwrap();
        snapshot.tasks[0].status = TaskStatus::ReadyForReview;
        snapshot.tasks[0].result = "Implementation and review evidence".into();
        snapshot.tasks[0].plan = "Approved plan".into();
        snapshot.tasks[0].worktree = Some("/partial/worktree".into());
        save(&db, &snapshot).unwrap();
        let completed = apply(
            &db,
            Command::CompleteTask {
                task_id: original.id,
            },
        )
        .unwrap();
        assert_eq!(completed.tasks[0].status, TaskStatus::Completed);
        assert_eq!(completed.tasks[0].result, snapshot.tasks[0].result);
        assert_eq!(completed.tasks[0].plan, snapshot.tasks[0].plan);
        assert_eq!(completed.tasks[0].worktree, snapshot.tasks[0].worktree);
        assert_eq!(completed.tasks[0].events.last().unwrap().role, "user");
        assert!(
            completed.tasks[0]
                .events
                .last()
                .unwrap()
                .message
                .contains("Acknowledged")
        );
        assert_eq!(completed, load(&db).unwrap());
    }

    #[test]
    fn retry_is_explicit_and_keeps_one_issue_task_with_partial_evidence() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let issue = issue(&db, &ws);
        let original = apply(
            &db,
            Command::PlanIssue {
                issue_id: issue.id.clone(),
            },
        )
        .unwrap()
        .tasks[0]
            .clone();
        for status in [
            TaskStatus::Queued,
            TaskStatus::Planning,
            TaskStatus::AwaitingApproval,
            TaskStatus::Building,
            TaskStatus::Reviewing,
            TaskStatus::ReadyForReview,
            TaskStatus::Completed,
        ] {
            let mut snapshot = load(&db).unwrap();
            snapshot.tasks[0].status = status;
            save(&db, &snapshot).unwrap();
            assert!(
                apply(
                    &db,
                    Command::RetryTask {
                        task_id: original.id.clone()
                    }
                )
                .is_err(),
                "{status:?}"
            );
            assert_eq!(load(&db).unwrap(), snapshot);
        }
        for status in [TaskStatus::Failed, TaskStatus::Cancelled] {
            let mut snapshot = load(&db).unwrap();
            snapshot.tasks[0].status = status;
            snapshot.tasks[0].result = "Previous partial result".into();
            snapshot.tasks[0].plan = "Previous plan".into();
            snapshot.tasks[0].worktree = Some("/partial/worktree".into());
            save(&db, &snapshot).unwrap();
            let retried = apply(
                &db,
                Command::RetryTask {
                    task_id: original.id.clone(),
                },
            )
            .unwrap();
            assert_eq!(retried.tasks.len(), 1);
            assert_eq!(retried.tasks[0].id, original.id);
            assert_eq!(
                retried.tasks[0].issue_id.as_deref(),
                Some(issue.id.as_str())
            );
            assert_eq!(retried.tasks[0].status, TaskStatus::Queued);
            assert_eq!(retried.tasks[0].result, snapshot.tasks[0].result);
            assert_eq!(retried.tasks[0].plan, snapshot.tasks[0].plan);
            assert_eq!(retried.tasks[0].worktree, snapshot.tasks[0].worktree);
            assert_eq!(retried.tasks[0].created_at_ms, original.created_at_ms);
            assert_eq!(retried.tasks[0].events.last().unwrap().role, "user");
            assert!(
                retried.tasks[0]
                    .events
                    .last()
                    .unwrap()
                    .message
                    .contains("retry")
            );
            assert_eq!(
                apply(
                    &db,
                    Command::PlanIssue {
                        issue_id: issue.id.clone()
                    }
                )
                .unwrap(),
                retried
            );
        }
    }

    #[test]
    fn completed_tasks_release_capacity_but_all_task_history_pins_repository_identity() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        task(&db, &ws);
        let mut snapshot = load(&db).unwrap();
        snapshot.tasks[0].status = TaskStatus::ReadyForReview;
        let ready_capacity = reserved_capacity(&snapshot).unwrap();
        snapshot.tasks[0].status = TaskStatus::Completed;
        let complete_capacity = reserved_capacity(&snapshot).unwrap();
        assert!(complete_capacity <= ready_capacity);
        assert!(terminal(TaskStatus::Completed));
        let other = repository();
        for status in [
            TaskStatus::Completed,
            TaskStatus::ReadyForReview,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ] {
            snapshot.tasks[0].status = status;
            save(&db, &snapshot).unwrap();
            let error = apply(
                &db,
                Command::SaveWorkspace {
                    workspace: Workspace {
                        repository: other.path().to_string_lossy().into(),
                        ..ws.clone()
                    },
                },
            )
            .unwrap_err();
            assert!(error.contains("new workspace"), "{error}");
            assert_eq!(load(&db).unwrap(), snapshot);
        }
    }

    #[test]
    fn missing_folders_and_empty_names_are_rejected() {
        let db = Db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let invalid = Workspace {
            id: String::new(),
            name: "Project".into(),
            repository: dir.path().join("missing").to_string_lossy().into(),
            instructions: String::new(),
            away_enabled: false,
        };
        assert!(
            apply(
                &db,
                Command::SaveWorkspace {
                    workspace: invalid.clone()
                }
            )
            .is_err()
        );
        let repo = repository();
        assert!(
            apply(
                &db,
                Command::SaveWorkspace {
                    workspace: Workspace {
                        name: "  ".into(),
                        repository: repo.path().to_string_lossy().into(),
                        ..invalid
                    }
                }
            )
            .is_err()
        );
        assert!(load(&db).unwrap().workspaces.is_empty());
    }

    #[test]
    fn replacing_workspace_folders_updates_repository_and_folder_map_together() {
        let db = Db::open_in_memory().unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let third = tempfile::tempdir().unwrap();
        let path = |dir: &tempfile::TempDir| dir.path().canonicalize().unwrap().to_string_lossy().into_owned();
        let initial = apply(&db, Command::SaveWorkspaceWithFolders {
            workspace: Workspace {
                id: String::new(), name: "Initial".into(), repository: path(&first),
                instructions: String::new(), away_enabled: false,
            },
            folders: vec![path(&first), path(&second)],
        }).unwrap();
        let mut edited = initial.workspaces[0].clone();
        edited.name = "Updated".into();
        let updated = apply(&db, Command::SaveWorkspaceWithFolders {
            workspace: edited,
            folders: vec![path(&second), path(&third)],
        }).unwrap();
        assert_eq!(updated.workspaces[0].id, initial.workspaces[0].id);
        assert_eq!(updated.workspaces[0].repository, path(&second));
        assert_eq!(updated.folders_for(&updated.workspaces[0]), vec![path(&second), path(&third)]);
        assert_eq!(load(&db).unwrap().workspaces[0].name, "Updated");
    }

    #[test]
    fn workspace_folder_update_rejections_preserve_saved_workspace_and_task_roots() {
        let db = Db::open_in_memory().unwrap();
        let first = repository();
        let second = repository();
        let path = |dir: &tempfile::TempDir| dir.path().canonicalize().unwrap().to_string_lossy().into_owned();
        let initial = apply(&db, Command::SaveWorkspaceWithFolders {
            workspace: Workspace {
                id: String::new(), name: "Initial".into(), repository: path(&first),
                instructions: String::new(), away_enabled: false,
            },
            folders: vec![path(&first), path(&second)],
        }).unwrap();
        let saved = apply(&db, Command::CreateTaskInFolder {
            workspace_id: initial.workspaces[0].id.clone(), title: "Pinned".into(),
            goal: "Keep source root".into(), folder: path(&second),
        }).unwrap();
        for folders in [vec![path(&first)], vec![path(&second), path(&first)], vec![path(&first), path(&first)]] {
            let mut edited = saved.workspaces[0].clone();
            edited.name = "Must not persist".into();
            assert!(apply(&db, Command::SaveWorkspaceWithFolders { workspace: edited, folders }).is_err());
            let after = load(&db).unwrap();
            assert_eq!(after.workspaces, saved.workspaces);
            assert_eq!(after.workspace_folders, saved.workspace_folders);
            assert_eq!(after.task_roots, saved.task_roots);
        }
    }

    #[test]
    fn one_workspace_can_hold_two_folders_and_pin_a_task_to_the_chosen_one() {
        let db = Db::open_in_memory().unwrap();
        let first = repository();
        let second = repository();
        let workspace = Workspace {
            id: String::new(),
            name: "Product".into(),
            repository: first.path().to_string_lossy().into_owned(),
            instructions: String::new(),
            away_enabled: false,
        };
        let state = apply(
            &db,
            Command::SaveWorkspaceWithFolders {
                workspace,
                folders: vec![
                    first.path().to_string_lossy().into_owned(),
                    second.path().to_string_lossy().into_owned(),
                ],
            },
        )
        .unwrap();
        let id = state.workspaces[0].id.clone();
        assert_eq!(state.folders_for(&state.workspaces[0]).len(), 2);
        let task = apply(
            &db,
            Command::CreateTaskInFolder {
                workspace_id: id.clone(),
                title: "Fix API".into(),
                goal: "Fix the API".into(),
                folder: second.path().to_string_lossy().into_owned(),
            },
        )
        .unwrap()
        .tasks
        .pop()
        .unwrap();
        let saved = load(&db).unwrap();
        assert_eq!(
            saved.root_for(&task, &saved.workspaces[0]),
            second.path().canonicalize().unwrap().to_string_lossy()
        );
        assert!(
            apply(
                &db,
                Command::CreateTaskInFolder {
                    workspace_id: id.clone(),
                    title: "Wrong".into(),
                    goal: "Wrong".into(),
                    folder: std::env::temp_dir().to_string_lossy().into_owned(),
                }
            )
            .is_err()
        );
        assert!(apply(&db, Command::CreateTask {
            workspace_id: id, title: "Fix a bug".into(), goal: "Fix a bug".into(),
        }).is_err(), "ambiguous manual work must not silently run in the primary folder");
    }

    #[test]
    fn repository_is_canonical_git_root_and_active_tasks_pin_it() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let subdir = repo.path().join("src");
        std::fs::create_dir(&subdir).unwrap();
        let ws = workspace(&db, &subdir);
        assert_eq!(
            std::path::Path::new(&ws.repository),
            subdir.canonicalize().unwrap()
        );
        task(&db, &ws);
        let other = repository();
        assert!(
            apply(
                &db,
                Command::SaveWorkspace {
                    workspace: Workspace {
                        repository: other.path().to_string_lossy().into(),
                        ..ws.clone()
                    }
                }
            )
            .is_err()
        );
        assert_eq!(load(&db).unwrap().workspaces[0], ws);
    }

    #[test]
    fn folder_workspace_saves_without_git_but_cannot_start_a_code_task() {
        let db = Db::open_in_memory().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let saved = apply(
            &db,
            Command::SaveWorkspace {
                workspace: Workspace {
                    id: String::new(),
                    name: "Notes".into(),
                    repository: folder.path().to_string_lossy().into(),
                    instructions: String::new(),
                    away_enabled: false,
                },
            },
        )
        .unwrap();
        let workspace = &saved.workspaces[0];
        assert_eq!(
            workspace.repository,
            folder.path().canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(
            apply(
                &db,
                Command::CreateTask {
                    workspace_id: workspace.id.clone(),
                    title: "Fix".into(),
                    goal: "Fix".into(),
                },
            )
            .unwrap_err(),
            "Code tasks require a Git repository",
        );
        assert!(load(&db).unwrap().tasks.is_empty());
    }

    #[test]
    fn non_git_workspace_cannot_queue_an_issue_code_task() {
        let db = Db::open_in_memory().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let workspace = workspace(&db, folder.path());
        let issue = issue(&db, &workspace);
        assert_eq!(
            apply(&db, Command::PlanIssue { issue_id: issue.id }).unwrap_err(),
            "Code tasks require a Git repository",
        );
        assert!(load(&db).unwrap().tasks.is_empty());
    }

    #[test]
    fn save_rejects_cross_workspace_and_dangling_references_atomically() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let other = workspace(&db, repo.path());
        let issue = issue(&db, &ws);
        task(&db, &other);
        let valid = load(&db).unwrap();
        let mut invalid = valid.clone();
        invalid.tasks[0].issue_id = Some(issue.id);
        assert!(save(&db, &invalid).is_err());
        invalid = valid.clone();
        invalid.issues[0].workspace_id = other.id;
        assert!(save(&db, &invalid).is_err());
        invalid = valid.clone();
        invalid.tasks[0].workspace_id = "missing".into();
        assert!(save(&db, &invalid).is_err());
        assert_eq!(load(&db).unwrap(), valid);
        assert!(
            apply(
                &db,
                Command::CreateTask {
                    workspace_id: "missing".into(),
                    title: "Title".into(),
                    goal: "Goal".into()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn interrupted_work_resumes_twice_then_stops_and_queued_work_survives() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        for _ in 0..4 {
            task(&db, &ws);
        }
        let mut snapshot = load(&db).unwrap();
        for (task, state) in snapshot.tasks.iter_mut().zip([
            TaskStatus::Planning,
            TaskStatus::Building,
            TaskStatus::Reviewing,
            TaskStatus::Queued,
        ]) {
            task.status = state;
        }
        save(&db, &snapshot).unwrap();
        let recovered = recover_interrupted(&db).unwrap();
        let states: Vec<_> = recovered.tasks.iter().map(|t| t.status).collect();
        assert_eq!(states, [TaskStatus::Queued, TaskStatus::Building, TaskStatus::Building, TaskStatus::Queued], "interrupted work resumes");
        assert!(recovered.tasks[..3].iter().all(|t| t.events.last().unwrap().message.starts_with(RESUMED)));
        assert_eq!(recovered.tasks[3], snapshot.tasks[3]);

        // A ticket that keeps getting interrupted stops after two resumes.
        // Recovery runs once per daemon start, so each call is one restart.
        for _ in 0..2 {
            let mut state = load(&db).unwrap();
            state.tasks[1].status = TaskStatus::Building;
            save(&db, &state).unwrap();
            recover_interrupted(&db).unwrap();
        }
        let stopped = load(&db).unwrap();
        assert_eq!(stopped.tasks[1].status, TaskStatus::Failed);
        assert!(stopped.tasks[1].events.last().unwrap().message.contains("restart"));
    }

    #[test]
    fn restart_during_a_retry_keeps_previous_result_evidence() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        task(&db, &ws);
        let mut snapshot = load(&db).unwrap();
        snapshot.tasks[0].status = TaskStatus::Planning;
        snapshot.tasks[0].result = "Previous partial implementation and review".into();
        snapshot.tasks[0].worktree = Some("/partial/worktree".into());
        save(&db, &snapshot).unwrap();
        let recovered = recover_interrupted(&db).unwrap();
        assert_eq!(recovered.tasks[0].status, TaskStatus::Queued);
        assert_eq!(recovered.tasks[0].result, snapshot.tasks[0].result);
        assert_eq!(recovered.tasks[0].worktree, snapshot.tasks[0].worktree);
        assert!(
            recovered.tasks[0]
                .events
                .last()
                .unwrap()
                .message
                .contains("restart")
        );
    }

    #[test]
    fn event_history_is_bounded_and_utf8_safe() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let mut task = task(&db, &ws);
        for _ in 0..120 {
            append_event(&mut task, "assistant", &"猫".repeat(20_000));
        }
        assert_eq!(task.events.len(), 100);
        assert!(
            task.events
                .iter()
                .all(|event| event.message.len() <= 2048
                    && event.message.chars().all(|ch| ch == '猫'))
        );
        assert!(task.updated_at_ms > 0);
    }

    #[test]
    fn notes_outlive_progress_events() {
        let mut task = create_task("w".into(), None, "t".into(), "g".into()).unwrap();
        append_event(&mut task, NOTE_ROLE, "cover discount-only carts");
        for i in 0..250 {
            append_event(&mut task, "builder", &format!("step {i}"));
        }
        assert_eq!(task.events.len(), MAX_EVENTS);
        assert!(task.events.iter().any(|e| e.role == NOTE_ROLE));
        assert_eq!(task.events.last().unwrap().message, "step 249");
    }

    #[test]
    fn oversized_user_content_is_rejected() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        assert!(
            apply(
                &db,
                Command::CreateTask {
                    workspace_id: ws.id,
                    title: "Task".into(),
                    goal: "x".repeat(32_769)
                }
            )
            .is_err()
        );
        assert!(load(&db).unwrap().tasks.is_empty());
    }

    #[test]
    fn connection_toggle_is_durable_and_unknown_id_is_rejected() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let issue = issue(&db, &ws);
        apply(
            &db,
            Command::SetConnectionEnabled {
                connection_id: issue.connection_id,
                enabled: false,
            },
        )
        .unwrap();
        assert!(!load(&db).unwrap().connections[0].enabled);
        assert!(
            apply(
                &db,
                Command::SetConnectionEnabled {
                    connection_id: "missing".into(),
                    enabled: false
                }
            )
            .is_err()
        );
    }

    #[test]
    fn generated_ids_are_unique_and_clock_is_current() {
        let ids: std::collections::HashSet<_> = (0..1000).map(|_| new_id()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(now_ms() > 1_700_000_000_000);
    }

    #[test]
    fn full_event_history_compacts_without_losing_results_or_blocking_cancellation() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let original = task(&db, &ws);
        let mut snapshot = load(&db).unwrap();
        snapshot.tasks.clear();
        snapshot.task_roots.clear();
        for index in 0..42 {
            let mut task = original.clone();
            task.id = format!("task-{index}");
            task.status = if index == 0 {
                TaskStatus::Building
            } else {
                TaskStatus::ReadyForReview
            };
            task.plan = "Retained plan 猫".repeat(100);
            task.result = "Retained result 猫".repeat(100);
            task.events = (0..100)
                .map(|event_index| TaskEvent {
                    at_ms: (index * 100 + event_index) as i64,
                    role: "assistant".into(),
                    message: "猫".repeat(682),
                })
                .collect();
            snapshot.tasks.push(task);
        }
        assert!(serde_json::to_vec(&snapshot).unwrap().len() > MAX_SNAPSHOT_BYTES);
        save(&db, &snapshot).unwrap();
        let compacted = load(&db).unwrap();
        assert!(
            compacted
                .tasks
                .iter()
                .map(|task| task.events.len())
                .sum::<usize>()
                < 4200
        );
        let cancelled = apply(
            &db,
            Command::CancelTask {
                task_id: "task-0".into(),
            },
        )
        .unwrap();
        assert_eq!(cancelled.tasks[0].status, TaskStatus::Cancelled);
        assert_eq!(
            cancelled.tasks[0].events.last().unwrap().message,
            "Cancelled this task."
        );
        for (before, after) in snapshot.tasks.iter().zip(&cancelled.tasks) {
            assert_eq!(before.plan, after.plan);
            assert_eq!(before.result, after.result);
        }
        assert_eq!(cancelled, load(&db).unwrap());
        assert!(db.get_setting(SETTING).unwrap().unwrap().len() <= MAX_SNAPSHOT_BYTES);
    }

    #[test]
    fn old_long_unicode_events_are_normalized_when_saved() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        task(&db, &ws);
        let mut snapshot = load(&db).unwrap();
        snapshot.tasks[0].events[0].message = "🦊猫".repeat(4000);
        save(&db, &snapshot).unwrap();
        let loaded = load(&db).unwrap();
        let message = &loaded.tasks[0].events[0].message;
        assert!(message.len() <= 2048);
        assert!(snapshot.tasks[0].events[0].message.starts_with(message));
        assert!(!message.is_empty());
    }

    #[test]
    fn intake_reserves_space_for_pending_results_and_emergency_cancellation() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        let original = task(&db, &ws);
        let mut snapshot = load(&db).unwrap();
        for index in 0..200 {
            let mut previous = original.clone();
            previous.id = format!("archived-{index}");
            previous.status = TaskStatus::ReadyForReview;
            previous.goal = "g".repeat(32_768);
            snapshot.tasks.push(previous);
        }
        save(&db, &snapshot).unwrap();
        assert!(
            apply(
                &db,
                Command::CreateTask {
                    workspace_id: ws.id,
                    title: "Extra task".into(),
                    goal: "More work".into()
                }
            )
            .is_err()
        );
        assert_eq!(load(&db).unwrap(), snapshot);
        // Worst-case JSON escaping still fits the reserved output allowance.
        snapshot.tasks[0].status = TaskStatus::Reviewing;
        snapshot.tasks[0].plan = "\u{0001}".repeat(65_536);
        snapshot.tasks[0].result = "\u{0001}".repeat(132 * 1024);
        save(&db, &snapshot).unwrap();
        let cancelled = apply(
            &db,
            Command::CancelTask {
                task_id: original.id,
            },
        )
        .unwrap();
        assert_eq!(cancelled.tasks[0].status, TaskStatus::Cancelled);
        assert_eq!(cancelled.tasks[0].result, snapshot.tasks[0].result);
        assert_eq!(cancelled.tasks[0].plan, snapshot.tasks[0].plan);
    }

    #[test]
    fn completed_build_and_review_result_can_use_132_kib() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let ws = workspace(&db, repo.path());
        task(&db, &ws);
        let mut snapshot = load(&db).unwrap();
        snapshot.tasks[0].result = "x".repeat(132 * 1024);
        save(&db, &snapshot).unwrap();
        assert_eq!(load(&db).unwrap().tasks[0].result.len(), 132 * 1024);
        snapshot.tasks[0].result.push('x');
        assert!(save(&db, &snapshot).is_err());
    }
}

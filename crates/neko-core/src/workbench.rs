//! Durable Neko-owned work. The daemon serializes access to this store.
use crate::Db;
use neko_protocol::workbench::{Command, Snapshot, Task, TaskEvent, TaskStatus};
use std::collections::HashSet;

const SETTING: &str = "workbench_snapshot_v1";
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
    let Some(json) = db.get_setting(SETTING).map_err(|e| e.to_string())? else {
        return Ok(Snapshot::default());
    };
    if json.len() > MAX_SNAPSHOT_BYTES {
        return Err("Workbench storage exceeds its size limit".into());
    }
    let mut snapshot =
        serde_json::from_str(&json).map_err(|e| format!("Cannot read workbench storage: {e}"))?;
    let migrated = crate::mcp_host::store::migrate(&mut snapshot)?;
    validate(&snapshot)?;
    if migrated {
        // One upsert atomically disables old authority without deleting evidence.
        let json = serde_json::to_string(&snapshot).map_err(|e| e.to_string())?;
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err("Migration needs storage headroom; original workspace data has not been changed".into());
        }
        db.set_setting(SETTING, &json).map_err(|e| e.to_string())?;
        bump_revision();
    }
    // Attached for readers only; save() never writes it into this setting.
    snapshot.conversation = crate::neko_chat::load(db)?;
    snapshot.memory = crate::neko_memory::load(db)?;
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
    compacted.conversation.clear();
    compacted.memory.clear();
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
        let occupied = serde_json::to_vec(&task.supervision).map_err(|e| e.to_string())?.len();
        bytes += crate::supervision::MAX_DECISION_BYTES.saturating_sub(occupied);
    }
    Ok(bytes)
}

pub fn apply(db: &Db, command: Command) -> Result<Snapshot, String> {
    let mut snapshot = load(db)?;
    match command {
        Command::Mcp(command) => crate::mcp_host::store::apply_command(&mut snapshot, command, now_ms())?,
        Command::Snapshot => return Ok(snapshot),
        Command::SaveWorkspace { mut workspace } => {
            workspace.name = workspace.name.trim().to_owned();
            required("Workspace name", &workspace.name, 256)?;
            bounded(
                "Workspace instructions",
                &workspace.instructions,
                MAX_CONTENT_BYTES,
            )?;
            workspace.repository = canonical_repository(&workspace.repository)?;
            if workspace.id.is_empty() {
                workspace.id = new_id();
                snapshot.workspaces.push(workspace);
            } else {
                let previous = snapshot
                    .workspaces
                    .iter_mut()
                    .find(|w| w.id == workspace.id)
                    .ok_or("Workspace no longer exists")?;
                if previous.repository != workspace.repository
                    && snapshot
                        .tasks
                        .iter()
                        .any(|t| t.workspace_id == workspace.id)
                {
                    return Err("This workspace already has task history. Create a new workspace for a different repository".into());
                }
                *previous = workspace;
            }
        }
        Command::CreateTask {
            workspace_id,
            title,
            goal,
        } => {
            if !snapshot.workspaces.iter().any(|w| w.id == workspace_id) {
                return Err("Workspace no longer exists".into());
            }
            snapshot
                .tasks
                .push(create_task(workspace_id, None, title, goal)?);
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
            let goal = if issue.description.trim().is_empty() {
                issue.title.clone()
            } else {
                issue.description.clone()
            };
            let mut task = create_task(
                issue.workspace_id.clone(),
                Some(issue.id.clone()),
                issue.title.clone(),
                goal,
            )?;
            task.source_revision = Some(issue.updated_at.clone());
            snapshot.tasks.push(task);
        }
        Command::ApproveTask { task_id } => {
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
        Command::RetryTask { task_id } => {
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
        Command::SendMessage { .. } | Command::DecideChatTool { .. } | Command::CancelChat { .. } => {
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
            let known: Vec<String> = snapshot.workspaces.iter().map(|w| w.id.clone()).collect();
            crate::neko_memory::upsert(db, entry, &known)?;
            return load(db);
        }
        Command::DeleteMemory { id } => {
            crate::neko_memory::delete(db, &id)?;
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
        let index = task.events.iter().position(|e| e.role != NOTE_ROLE).unwrap_or(0);
        task.events.remove(index);
    }
    task.updated_at_ms = at_ms;
}

/// A restarted daemon cannot promise that an interrupted child completed work.
/// Queued work and explicit approval waits retain their original state.
pub fn recover_interrupted(db: &Db) -> Result<Snapshot, String> {
    let mut snapshot = load(db)?;
    let mut changed = false;
    for task in &mut snapshot.tasks {
        if matches!(
            task.status,
            TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing
        ) {
            task.status = TaskStatus::Failed;
            let message = "Interrupted by a daemon restart. Inspect any existing worktree before starting another task.";
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

fn terminal(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::ReadyForReview
            | TaskStatus::Completed
            | TaskStatus::Failed
            | TaskStatus::Cancelled
    )
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

fn canonical_repository(path: &str) -> Result<String, String> {
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
    use super::*;
    use neko_protocol::workbench::{Issue, LinearConnection, TaskStatus, Workspace};

    #[test]
    fn migration_never_writes_a_snapshot_that_exceeds_the_read_limit() {
        let db = Db::open_in_memory().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.workspaces.push(Workspace { id: "w".into(), name: "W".into(), repository: "/tmp/w".into(), instructions: String::new(), away_enabled: true });
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
        assert!(load(&db).is_err(), "migration must reject growth before committing");
        assert_eq!(db.get_setting(SETTING).unwrap().unwrap(), raw);
    }

    #[test]
    fn durable_legacy_migration_preserves_task_artifacts_and_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.workspaces.push(Workspace { id: "w".into(), name: "W".into(), repository: "/tmp/w".into(), instructions: String::new(), away_enabled: true });
        let mut task = create_task("w".into(), None, "Keep task".into(), "Keep goal".into()).unwrap();
        task.plan = "Keep plan".into(); task.result = "Keep result".into(); task.worktree = Some("/tmp/keep-worktree".into()); task.status = TaskStatus::Completed;
        snapshot.tasks.push(task.clone());
        snapshot.connections.push(LinearConnection { id: "old".into(), workspace_id: "w".into(), name: "Old".into(), organization_id: "org".into(), viewer_id: "viewer".into(), team_ids: vec!["team".into()], project_ids: vec![], enabled: true, last_sync_ms: Some(1), error: None, intake_notice: None });
        snapshot.issues.push(Issue { id: "i".into(), connection_id: "old".into(), workspace_id: "w".into(), external_id: "external".into(), identifier: "OLD-1".into(), title: "Evidence".into(), description: "Preserve source".into(), url: "https://example.com/issue".into(), priority: 1, updated_at: "rev".into(), assigned: true });
        let mut legacy = serde_json::to_value(&snapshot).unwrap(); legacy.as_object_mut().unwrap().remove("mcp");
        db.set_setting(SETTING, &serde_json::to_string(&legacy).unwrap()).unwrap();
        let migrated = load(&db).unwrap();
        assert_eq!(migrated.tasks[0], task);
        assert_eq!(migrated.issues[0].description, "Preserve source");
        assert!(!migrated.issues[0].assigned);
        assert!(!migrated.connections[0].enabled);
        assert_eq!(migrated.connections[0].id, "old"); // original Keychain identity preserved
        assert!(!migrated.workspaces[0].away_enabled);
        assert_eq!(load(&db).unwrap(), migrated);
        let mut future = migrated; future.mcp.version = 999;
        let raw = serde_json::to_string(&future).unwrap(); db.set_setting(SETTING, &raw).unwrap();
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
    fn invalid_repositories_and_empty_names_are_rejected() {
        let db = Db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let invalid = Workspace {
            id: String::new(),
            name: "Project".into(),
            repository: dir.path().to_string_lossy().into(),
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
    fn repository_is_canonical_git_root_and_active_tasks_pin_it() {
        let db = Db::open_in_memory().unwrap();
        let repo = repository();
        let subdir = repo.path().join("src");
        std::fs::create_dir(&subdir).unwrap();
        let ws = workspace(&db, &subdir);
        assert_eq!(
            std::path::Path::new(&ws.repository),
            repo.path().canonicalize().unwrap()
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
    fn interrupted_work_fails_once_and_queued_work_survives() {
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
        assert!(
            recovered.tasks[..3]
                .iter()
                .all(|task| task.status == TaskStatus::Failed
                    && task.events.last().unwrap().message.contains("restart"))
        );
        assert_eq!(recovered.tasks[3], snapshot.tasks[3]);
        assert_eq!(recover_interrupted(&db).unwrap(), recovered);
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
        assert_eq!(recovered.tasks[0].status, TaskStatus::Failed);
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

//! The chat directs the crew: the orchestration tools an agent run reaches
//! through its lease, and the delivery of the durable command queue. See
//! docs/knowledge/the-chat-directs-the-crew.md.
//!
//! Every call carries a capability (the bridge token) whose scope has a
//! `CrewRole`; the role, not the model, decides which tools exist for a run.
//! Mutating calls are idempotent (a retried call returns the first result) and
//! are written to the task's events.
use super::*;
use neko_core::{command_queue as queue, crew as rows};
use neko_protocol::crew::*;
use serde_json::{json, Value};

/// How often `wait_for` looks at the task.
const WAIT_POLL: Duration = Duration::from_millis(1000);
const MAX_ARGUMENT_BYTES: usize = 262_144;

/// One delivery at a time (the tick, a steer call and the web can all ask), and a
/// taken-back message cannot slip out while one is in flight. Order: this, then the
/// controller's own locks.
static DELIVERY: Mutex<()> = Mutex::new(());
/// Mutating tool calls run one at a time, so two identical calls cannot both miss the
/// idempotency cache and both do the work.
static MUTATING: Mutex<()> = Mutex::new(());

/// What the delivery loop does with one queued message right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Plan {
    /// Not yet: the turn is running (or the task is between turns).
    Wait,
    /// Into the live session.
    Inject,
    /// A reply to the task: it picks it up, or (running) stops and restarts with it.
    Reply,
    /// A note the next phase reads (the task has not started).
    Note,
    Drop(&'static str),
}

/// The delivery rule, in one place. `running` means a worker holds the task;
/// `live` whether the runtime can take input mid-turn.
pub(super) fn plan(status: TaskStatus, running: bool, mode: DeliveryMode, live: bool, reply_waiting: bool) -> Plan {
    if status == TaskStatus::Cancelled {
        return Plan::Drop("The task was cancelled before this was delivered.");
    }
    if running {
        return match mode {
            DeliveryMode::AfterTurn => Plan::Wait,
            DeliveryMode::SteerNow if live => Plan::Inject,
            // The runtime cannot take input mid-turn: the turn boundary it is.
            DeliveryMode::SteerNow => Plan::Wait,
            DeliveryMode::InterruptRestart => Plan::Reply,
        };
    }
    match status {
        // A reply already waits for its turn: more messages join it, in order.
        TaskStatus::Queued if reply_waiting => Plan::Reply,
        TaskStatus::Queued => Plan::Note,
        TaskStatus::ReadyForReview | TaskStatus::Completed | TaskStatus::Failed | TaskStatus::AwaitingApproval => Plan::Reply,
        // Between phases with no worker (a restart is recovering it): wait for it.
        _ => Plan::Wait,
    }
}

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max { text.to_owned() } else { format!("{}…", text.chars().take(max).collect::<String>()) }
}

fn short_ref(task: &Task) -> String {
    format!("NEK-{}", task.id.chars().take(4).collect::<String>().to_uppercase())
}

fn title_of(task: &Task) -> String {
    task.display_title.clone().unwrap_or_else(|| task.title.clone())
}

fn norm(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

fn string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn required(args: &Value, key: &str) -> Result<String, String> {
    string_arg(args, key).ok_or_else(|| format!("{key} is required"))
}

/// Whether the run's role may see this task at all.
fn visible(role: &CrewRole, scope_workspace: &str, own_workspace: Option<&str>, task: &Task, snapshot: &Snapshot) -> bool {
    match role {
        CrewRole::Full => true,
        // Without a chosen workspace, only the active agent profile's own workspaces.
        CrewRole::ReadOnly if scope_workspace.is_empty() => snapshot.agent_profiles.owner(&task.workspace_id) == snapshot.agent_profiles.active_profile_id,
        CrewRole::ReadOnly => task.workspace_id == scope_workspace,
        CrewRole::Task { .. } => own_workspace.is_some_and(|w| w == task.workspace_id),
    }
}

impl Controller {
    /// One orchestration tool call. `artifacts` runs `save_artifact` /
    /// `show_artifact` (they live with the server's artifact store).
    pub fn crew_call(&self, token: &str, tool: &str, arguments_json: &str, artifacts: &dyn Fn(&str, Value) -> Result<String, String>) -> Result<String, String> {
        let scope = self.mcp.crew_scope(token)?;
        let role = scope.crew.clone().ok_or("This run has no crew tools")?;
        if tool == "tools" {
            return Ok(json!({"role": role.label(), "tools": role.tools()}).to_string());
        }
        if !role.allows(tool) {
            return Err(format!("{tool} is not available to this run ({}). It can use: {}.", role.label(), role.tools().join(", ")));
        }
        if arguments_json.len() > MAX_ARGUMENT_BYTES {
            return Err("Arguments exceed limit".into());
        }
        let args: Value = serde_json::from_str(arguments_json).map_err(|_| "Arguments must be a JSON object")?;
        if !args.is_object() {
            return Err("Arguments must be a JSON object".into());
        }
        // The same fence `bridge()` applies: a run started under another profile revision has no authority now.
        if store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?.agent_profiles.revision != scope.profile_revision {
            return Err("Agent profile authority changed; start a new run".into());
        }
        let mutating = matches!(tool, "create_task" | "steer_task" | "cancel_task" | "schedule");
        let key = rows::idem_key(&scope.run_id, tool, &args);
        let _one_at_a_time = mutating.then(|| MUTATING.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        if mutating {
            let cached = rows::cached_result(&self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner), &key, store::now_ms())?;
            if let Some(cached) = cached {
                let mut value: Value = serde_json::from_str(&cached).unwrap_or_else(|_| json!({"result": cached}));
                if let Some(object) = value.as_object_mut() {
                    object.insert("duplicate".into(), json!(true));
                    object.insert("note".into(), json!("This exact call already ran in this session; nothing was done again."));
                }
                return Ok(value.to_string());
            }
        }
        let result = match tool {
            "list_tasks" => self.crew_list(&scope, &role, &args),
            "task_status" => self.crew_status(&scope, &role, &args),
            "wait_for" => self.crew_wait(&scope, &role, &args),
            "create_task" => self.crew_create(&scope, &role, &args),
            "steer_task" => self.crew_steer(&scope, &role, &args, &key),
            "cancel_task" => self.crew_cancel(&scope, &role, &args),
            "schedule" => self.crew_schedule(&scope, &args),
            "save_artifact" | "show_artifact" => artifacts(tool, args.clone()),
            _ => Err(format!("Unknown tool {tool}")),
        };
        if let (true, Ok(text)) = (mutating, &result) {
            let _ = rows::cache_result(&self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner), &key, text, store::now_ms());
        }
        result
    }

    /// A line in the task's events saying who asked the crew to do what.
    fn crew_audit(&self, task_id: &str, role: &CrewRole, text: &str) {
        let db = self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(mut snapshot) = store::load(&db) else { return };
        if let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == task_id) {
            store::append_event(task, "crew", &format!("{}: {text}", role.label()));
            let _ = store::save(&db, &snapshot);
        }
    }

    fn own_workspace(&self, role: &CrewRole, snapshot: &Snapshot) -> Option<String> {
        match role {
            CrewRole::Task { ticket_id, .. } => snapshot.tasks.iter().find(|t| &t.id == ticket_id).map(|t| t.workspace_id.clone()),
            _ => None,
        }
    }

    fn crew_task<'a>(&self, snapshot: &'a Snapshot, scope: &neko_core::mcp_host::capability::Scope, role: &CrewRole, id: &str) -> Result<&'a Task, String> {
        let own = self.own_workspace(role, snapshot);
        snapshot
            .tasks
            .iter()
            .find(|t| t.id == id)
            .filter(|t| visible(role, &scope.workspace_id, own.as_deref(), t, snapshot))
            .ok_or_else(|| format!("No task {id} that this run can see"))
    }

    fn crew_list(&self, scope: &neko_core::mcp_host::capability::Scope, role: &CrewRole, args: &Value) -> Result<String, String> {
        let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
        let snapshot = store::load(&db)?;
        let own = self.own_workspace(role, &snapshot);
        let status = string_arg(args, "status").map(|s| norm(&s));
        let workspace = string_arg(args, "workspace_id");
        let origin = origin_of(role);
        let mine_only = args.get("mine").and_then(Value::as_bool).unwrap_or(false);
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(30).clamp(1, 100) as usize;
        let mut tasks: Vec<&Task> = snapshot
            .tasks
            .iter()
            .filter(|t| visible(role, &scope.workspace_id, own.as_deref(), t, &snapshot))
            .filter(|t| workspace.as_ref().is_none_or(|w| &t.workspace_id == w))
            .filter(|t| status.as_ref().is_none_or(|s| norm(&format!("{:?}", t.status)) == *s))
            .collect();
        if mine_only {
            let delegated = rows::open_delegated(&db, &snapshot, &origin)?;
            tasks.retain(|t| delegated.contains(&t.id));
        }
        tasks.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
        let list: Vec<Value> = tasks
            .iter()
            .take(limit)
            .map(|t| {
                let goal = snapshot.goals.iter().find(|g| g.ticket_id.as_deref() == Some(&t.id));
                json!({
                    "task_id": t.id, "ref": short_ref(t), "title": title_of(t), "status": format!("{:?}", t.status),
                    "workspace_id": t.workspace_id, "updated_ms": t.updated_at_ms,
                    "goal_id": goal.map(|g| g.id.clone()), "goal_status": goal.map(|g| format!("{:?}", g.status)),
                    "queued_messages": snapshot.queued_messages.iter().filter(|m| m.task_id == t.id && m.state == QueueState::Queued).count(),
                })
            })
            .collect();
        let open = rows::open_delegated(&db, &snapshot, &origin)?.len();
        Ok(json!({"tasks": list, "total": tasks.len(), "open_delegated": open, "delegation_cap": rows::cap(&db)?}).to_string())
    }

    fn status_json(snapshot: &Snapshot, queue_rows: &[QueuedMessage], task: &Task) -> Value {
        let goal = snapshot.goals.iter().find(|g| g.ticket_id.as_deref() == Some(&task.id));
        let summary = if !task.result.trim().is_empty() {
            task.result.clone()
        } else {
            task.events.iter().rev().find(|e| matches!(e.role.as_str(), "builder" | "scout" | "reviewer")).map(|e| e.message.clone()).unwrap_or_default()
        };
        json!({
            "task_id": task.id, "ref": short_ref(task), "title": title_of(task), "status": format!("{:?}", task.status),
            "workspace_id": task.workspace_id,
            "last_turn_summary": clip(&summary, 1500),
            "pull_request": task.pull_request.as_ref().map(|p| p.url.clone()),
            "goal": goal.map(|g| json!({
                "goal_id": g.id, "status": format!("{:?}", g.status), "objective": clip(&g.objective, 300),
                "turns": g.turns, "max_turns": g.budget.max_turns, "verdict": g.verdict, "reason": clip(&g.reason, 300),
                "evidence": g.ledger.iter().rev().take(3).map(|c| json!({"command": clip(&c.command, 160), "exit_code": c.exit_code, "passed": c.passed(), "source": c.source, "tail": clip(&c.tail, 300)})).collect::<Vec<_>>(),
            })),
            "queued_messages": queue_rows.iter().map(|m| json!({"id": m.id, "mode": m.mode, "state": m.state, "text": clip(&m.text, 120), "detail": m.detail})).collect::<Vec<_>>(),
            "recent_events": task.events.iter().rev().take(5).rev().map(|e| json!({"role": e.role, "message": clip(&e.message, 200)})).collect::<Vec<_>>(),
        })
    }

    fn crew_status(&self, scope: &neko_core::mcp_host::capability::Scope, role: &CrewRole, args: &Value) -> Result<String, String> {
        let id = required(args, "task_id")?;
        let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
        let snapshot = store::load(&db)?;
        let task = self.crew_task(&snapshot, scope, role, &id)?;
        Ok(Self::status_json(&snapshot, &queue::of_task(&db, &id)?, task).to_string())
    }

    fn crew_wait(&self, scope: &neko_core::mcp_host::capability::Scope, role: &CrewRole, args: &Value) -> Result<String, String> {
        let task_id = string_arg(args, "task_id");
        let goal_id = string_arg(args, "goal_id");
        if task_id.is_some() == goal_id.is_some() {
            return Err("Give either task_id or goal_id".into());
        }
        let until = string_arg(args, "until").map(|u| norm(&u));
        let seconds = args.get("timeout_seconds").and_then(Value::as_f64).unwrap_or(600.0).clamp(0.0, MAX_WAIT_SECONDS as f64);
        let started = Instant::now();
        let deadline = Duration::from_secs_f64(seconds);
        loop {
            let (snapshot, queues) = {
                let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
                let snapshot = store::load(&db)?;
                let ids = [task_id.clone(), goal_id.as_ref().and_then(|g| snapshot.goals.iter().find(|x| &x.id == g)).and_then(|g| g.ticket_id.clone())];
                let queues: Vec<QueuedMessage> = ids.into_iter().flatten().map(|id| queue::of_task(&db, &id)).collect::<Result<Vec<_>, _>>()?.concat();
                (snapshot, queues)
            };
            let queue_of = |id: &str| -> Vec<QueuedMessage> { queues.iter().filter(|m| m.task_id == id).cloned().collect() };
            let (met, state, detail) = if let Some(id) = &task_id {
                let task = self.crew_task(&snapshot, scope, role, id)?;
                let state = format!("{:?}", task.status);
                let met = match until.as_deref() {
                    None | Some("settled") => matches!(task.status, TaskStatus::AwaitingApproval | TaskStatus::ReadyForReview | TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled),
                    Some("finished") => matches!(task.status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled),
                    Some(other) => norm(&state) == other,
                };
                (met, state, Self::status_json(&snapshot, &queue_of(&task.id), task))
            } else {
                let id = goal_id.clone().unwrap_or_default();
                let goal = snapshot.goals.iter().find(|g| g.id == id).ok_or_else(|| format!("No goal {id}"))?;
                // A goal belongs to a ticket the run must be able to see.
                if let Some(ticket) = &goal.ticket_id {
                    self.crew_task(&snapshot, scope, role, ticket)?;
                } else if !matches!(role, CrewRole::Full) {
                    return Err(format!("No goal {id} that this run can see"));
                }
                let state = format!("{:?}", goal.status);
                let met = match until.as_deref() {
                    None | Some("finished") => goal.status.is_finished(),
                    Some("settled") => goal.status.is_finished() || matches!(goal.status, neko_protocol::goals::GoalStatus::Blocked | neko_protocol::goals::GoalStatus::Paused),
                    Some(other) => norm(&state) == other,
                };
                let ticket = goal.ticket_id.as_ref().and_then(|t| snapshot.tasks.iter().find(|x| &x.id == t));
                let detail = ticket.map(|t| Self::status_json(&snapshot, &queue_of(&t.id), t)).unwrap_or_else(|| json!({"goal_id": goal.id, "status": state.clone(), "reason": goal.reason}));
                (met, state, detail)
            };
            let waited = started.elapsed();
            if met || waited >= deadline || scope.cancelled.load(Ordering::Acquire) {
                let ended = scope.cancelled.load(Ordering::Acquire);
                return Ok(json!({"met": met, "timed_out": !met && !ended, "run_ended": ended, "state": state, "waited_seconds": waited.as_secs(), "status": detail}).to_string());
            }
            std::thread::sleep(WAIT_POLL.min(deadline.saturating_sub(waited)).max(Duration::from_millis(10)));
        }
    }

    fn crew_create(&self, scope: &neko_core::mcp_host::capability::Scope, role: &CrewRole, args: &Value) -> Result<String, String> {
        let title = required(args, "title")?;
        let request = required(args, "request")?;
        let origin = origin_of(role);
        let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
        let snapshot = store::load(&db)?;
        // A task run delegates only inside its own ticket's workspace.
        let workspace = match role {
            CrewRole::Task { .. } => {
                let own = self.own_workspace(role, &snapshot).ok_or("Your own ticket no longer exists")?;
                if string_arg(args, "workspace_id").is_some_and(|w| w != own) {
                    return Err("A sub-task belongs to the same workspace as its ticket".into());
                }
                own
            }
            _ => string_arg(args, "workspace_id")
                .or_else(|| (!scope.workspace_id.is_empty()).then(|| scope.workspace_id.clone()))
                .or_else(|| neko_chat::pending_turn(&db).and_then(|(_, w)| w))
                .or_else(|| (snapshot.workspaces.len() == 1).then(|| snapshot.workspaces[0].id.clone()))
                .ok_or("workspace_id is required: there is more than one workspace")?,
        };
        if !snapshot.workspaces.iter().any(|w| w.id == workspace) {
            return Err("That workspace does not exist".into());
        }
        if let CrewRole::Task { ticket_id, .. } = role {
            if snapshot.task_read_only.contains(ticket_id) {
                return Err("A read-only ticket cannot open sub-tasks".into());
            }
        }
        rows::check_cap(&db, &snapshot, &origin)?;
        let after = match string_arg(args, "folder") {
            Some(folder) => store::apply(&db, Command::CreateTaskInFolder { workspace_id: workspace.clone(), title: title.clone(), goal: request.clone(), folder })?,
            None => store::apply(&db, Command::CreateTask { workspace_id: workspace.clone(), title: title.clone(), goal: request.clone() })?,
        };
        let mut after = after;
        let task_id = {
            let task = after.tasks.last_mut().ok_or("The task was not created")?;
            // Only the person's own chat carries their words; an agent's text is never presented as theirs.
            if matches!(role, CrewRole::Full) {
                task.request = neko_chat::ticket_request(&request);
            }
            store::append_event(task, "crew", &format!("Opened by {}: {}", role.label(), clip(&request, 200)));
            task.id.clone()
        };
        // The person asked their own chat for it: it builds without waiting for a
        // plan approval. A sub-task a ticket opens still needs the usual approval.
        if matches!(role, CrewRole::Full) {
            after.start_when_planned.insert(task_id.clone());
        }
        store::save(&db, &after)?;
        rows::record_origin(&db, &task_id, &origin)?;
        if let Some(turn) = scope.run_id.strip_prefix("chat:") {
            let _ = neko_chat::link_ticket(&db, turn, &task_id);
        }
        let mut goal_id = None;
        if let (CrewRole::Full, Some(goal)) = (role, args.get("goal").filter(|g| g.is_object() || g.is_string())) {
            let (check, constraints, turns) = match goal {
                Value::Object(_) => (string_arg(goal, "done_check").unwrap_or_default(), string_arg(goal, "constraints").unwrap_or_default(), goal.get("max_turns").and_then(Value::as_u64).map(|n| n as u32)),
                _ => (String::new(), String::new(), None),
            };
            let objective = match goal {
                Value::String(text) if !text.trim().is_empty() => text.trim().to_owned(),
                Value::Object(_) => string_arg(goal, "objective").unwrap_or_else(|| request.clone()),
                _ => request.clone(),
            };
            // A goal an agent proposes waits paused until the person has read its check.
            let made = super::goals::create(&db, &super::goals::Spec { workspace_id: &workspace, objective: &objective, done_check: &check, constraints: &constraints, max_turns: turns, ticket_id: Some(&task_id), conversation_id: None, by_person: false })?;
            goal_id = Some(made.id);
        }
        let task = after.tasks.iter().find(|t| t.id == task_id).ok_or("The task was not created")?;
        Ok(json!({
            "task_id": task_id, "ref": short_ref(task), "title": title_of(task), "status": "Queued", "workspace_id": workspace,
            "goal_id": goal_id,
            "note": if goal_id.is_some() { "Queued. Neko starts it when the machine has room. The goal is paused until the person reads its check and resumes it." } else if matches!(role, CrewRole::Full) { "Queued. Neko plans it and starts it when the machine has room (a burst of tasks waits its turn). Use wait_for or task_status to follow it." } else { "Queued as a sub-task. It is planned and waits for the person's approval before it builds." },
            "open_delegated": rows::open_delegated(&db, &after, &origin)?.len(),
            "delegation_cap": rows::cap(&db)?,
        }).to_string())
    }

    fn crew_steer(&self, scope: &neko_core::mcp_host::capability::Scope, role: &CrewRole, args: &Value, key: &str) -> Result<String, String> {
        let id = required(args, "task_id")?;
        let message = required(args, "message")?;
        let mode = match string_arg(args, "mode") {
            None => DeliveryMode::AfterTurn,
            Some(m) => DeliveryMode::parse(&m).ok_or("mode is one of steer_now, after_turn, interrupt_restart")?,
        };
        let kind = match mode {
            DeliveryMode::AfterTurn => QueueKind::Send,
            DeliveryMode::SteerNow => QueueKind::Steer,
            DeliveryMode::InterruptRestart => QueueKind::Interrupt,
        };
        {
            let snapshot = store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
            self.crew_task(&snapshot, scope, role, &id)?;
        }
        let command_id = format!("crew-{}", key.rsplit('|').next().unwrap_or(key));
        let (entry, duplicate) = {
            let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
            queue::enqueue(&db, &id, Some(&command_id), &message, mode, kind, &role.label(), store::now_ms())?
        };
        if !duplicate {
            self.crew_audit(&id, role, &format!("queued a message ({}): {}", mode.word(), clip(&message, 160)));
        }
        self.deliver_queued()?;
        let now = {
            let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
            queue::of_task(&db, &id)?.into_iter().find(|m| m.id == entry.id).unwrap_or(entry)
        };
        Ok(json!({"task_id": id, "queue_id": now.id, "mode": now.mode, "state": now.state, "detail": now.detail, "duplicate": duplicate,
            "note": match now.state { QueueState::Queued => "Queued. It reaches the agent as described by the mode; a queued message is not a delivered one. Check task_status.", QueueState::Delivered => "Delivered to the task.", _ => "It was not delivered." }}).to_string())
    }

    fn crew_cancel(&self, scope: &neko_core::mcp_host::capability::Scope, role: &CrewRole, args: &Value) -> Result<String, String> {
        let id = required(args, "task_id")?;
        {
            let snapshot = store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
            let task = self.crew_task(&snapshot, scope, role, &id)?;
            if matches!(task.status, TaskStatus::Completed | TaskStatus::Cancelled) {
                return Err(format!("{} is already {:?}", short_ref(task), task.status));
            }
        }
        self.command(Command::CancelTask { task_id: id.clone() })?;
        self.crew_audit(&id, role, &format!("cancelled the task{}", string_arg(args, "reason").map(|r| format!(": {}", clip(&r, 160))).unwrap_or_default()));
        Ok(json!({"task_id": id, "status": "Cancelled"}).to_string())
    }

    /// A one-off or recurring read-only plan, through the schedules Neko has.
    fn crew_schedule(&self, scope: &neko_core::mcp_host::capability::Scope, args: &Value) -> Result<String, String> {
        let name = required(args, "name")?;
        let prompt = required(args, "prompt")?;
        let now = store::now_ms();
        let (rule, anchor) = if let Some(rule) = string_arg(args, "recurrence") {
            (rule, args.get("start_unix_ms").and_then(Value::as_i64).unwrap_or(now))
        } else {
            let at = match (args.get("at_unix_ms").and_then(Value::as_i64), args.get("in_minutes").and_then(Value::as_f64)) {
                (Some(at), _) => at,
                (None, Some(minutes)) if minutes >= 0.0 => now + (minutes * 60_000.0) as i64,
                _ => return Err("Give at_unix_ms, in_minutes, or a recurrence (an RRULE such as FREQ=DAILY;BYHOUR=9)".into()),
            };
            if at < now {
                return Err("That time is in the past".into());
            }
            ("FREQ=DAILY;COUNT=1".to_owned(), at)
        };
        let workspace = {
            let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
            let snapshot = store::load(&db)?;
            string_arg(args, "workspace_id")
                .or_else(|| (!scope.workspace_id.is_empty()).then(|| scope.workspace_id.clone()))
                .or_else(|| neko_chat::pending_turn(&db).and_then(|(_, w)| w))
                .or_else(|| (snapshot.workspaces.len() == 1).then(|| snapshot.workspaces[0].id.clone()))
                .ok_or("workspace_id is required: there is more than one workspace")?
        };
        let workspace = Some(workspace);
        let timezone = string_arg(args, "timezone").unwrap_or_else(|| "UTC".into());
        let saved = self.schedule_command(neko_protocol::scheduled_plans::ScheduleCommand::Save {
            schedule: neko_protocol::scheduled_plans::Schedule { name: name.clone(), prompt, workspace_id: workspace, rule: rule.clone(), timezone, anchor_ms: anchor, ..Default::default() },
        })?;
        let id = saved.schedules.iter().rev().find(|s| s.name == name).map(|s| s.id.clone()).ok_or("The schedule was not saved")?;
        let enabled = self.schedule_command(neko_protocol::scheduled_plans::ScheduleCommand::SetEnabled { id: id.clone(), enabled: true })?;
        let schedule = enabled.schedules.iter().find(|s| s.id == id).ok_or("The schedule disappeared")?;
        Ok(json!({"schedule_id": id, "next_due_ms": schedule.next_due_ms, "rule": rule, "note": "Each run is a read-only plan that opens a task for the person's approval; it does not change anything by itself."}).to_string())
    }

    // -----------------------------------------------------------------------
    // The durable queue
    // -----------------------------------------------------------------------

    pub(super) fn crew_command(&self, command: CrewCommand) -> Result<Snapshot, String> {
        match command {
            CrewCommand::Enqueue { task_id, command_id, text, mode, kind } => {
                {
                    let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
                    let snapshot = store::load(&db)?;
                    let task = snapshot.tasks.iter().find(|t| t.id == task_id).ok_or("Task no longer exists")?;
                    if task.status == TaskStatus::Cancelled {
                        return Err("That task was cancelled".into());
                    }
                    queue::enqueue(&db, &task_id, command_id.as_deref(), &text, mode, kind, "you", store::now_ms())?;
                }
                self.deliver_queued()?;
            }
            CrewCommand::CancelQueued { task_id, id } => {
                let _delivery = DELIVERY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
                if queue::settle(&db, &task_id, &id, QueueState::Cancelled, "Taken back before it was delivered.", store::now_ms())?.is_none() {
                    return Err("That message was already delivered or taken back".into());
                }
            }
            CrewCommand::SetDelegationCap { limit } => rows::set_cap(&*self.db.lock().map_err(|_| "Task storage unavailable")?, limit)?,
        }
        store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)
    }

    /// Delivers what can be delivered now; the rest stays queued. Runs on every
    /// scheduler tick and right after a message is queued.
    pub(super) fn deliver_queued(&self) -> Result<usize, String> {
        self.deliver_queued_with(&|_, _| false)
    }

    /// `live` is the runtime's way to take input mid-turn (`task_id`, text → accepted).
    /// No runtime Neko drives takes input mid-turn yet (`claude -p` and `codex exec`
    /// read their prompt once), so the real one declines and steer-now falls back to
    /// the turn boundary.
    pub(super) fn deliver_queued_with(&self, live: &dyn Fn(&str, &str) -> bool) -> Result<usize, String> {
        let _delivery = DELIVERY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let waiting = queue::queued(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
        if waiting.is_empty() {
            return Ok(0);
        }
        let mut delivered = 0;
        for entry in waiting {
            let snapshot = store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
            let now = store::now_ms();
            let settle = |state: QueueState, detail: &str| -> Result<(), String> {
                queue::settle(&*self.db.lock().map_err(|_| "Task storage unavailable")?, &entry.task_id, &entry.id, state, detail, now).map(|_| ())
            };
            // Taken back or delivered by another pass since the list was read.
            if !snapshot.queued_messages.iter().any(|m| m.task_id == entry.task_id && m.id == entry.id) {
                continue;
            }
            let Some(task) = snapshot.tasks.iter().find(|t| t.id == entry.task_id) else {
                settle(QueueState::Dropped, "The task no longer exists.")?;
                continue;
            };
            let running = self.active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().any(|a| a.task_id == entry.task_id);
            // Only a steer-now for a running turn asks the runtime (asking injects).
            let steerable = entry.mode == DeliveryMode::SteerNow && running && live(&entry.task_id, &entry.text);
            let reply_waiting = snapshot.task_replies.get(&entry.task_id).is_some_and(|r| !r.handled);
            let how = match plan(task.status, running, entry.mode, steerable, reply_waiting) {
                Plan::Drop(reason) => {
                    settle(QueueState::Dropped, reason)?;
                    continue;
                }
                Plan::Wait => {
                    if entry.mode == DeliveryMode::SteerNow && running && entry.detail.is_empty() {
                        // Said once, so the task's line can tell the person why it is not instant.
                        queue::annotate(&*self.db.lock().map_err(|_| "Task storage unavailable")?, &entry.task_id, &entry.id, "This agent cannot take input mid-turn, so it reads it when the turn ends.")?;
                    }
                    continue;
                }
                Plan::Inject => "Delivered into the live session.",
                step @ (Plan::Reply | Plan::Note) => {
                    let command = if step == Plan::Reply {
                        Command::ReplyToTask { task_id: entry.task_id.clone(), text: entry.text.clone() }
                    } else {
                        Command::AddTicketNote { task_id: entry.task_id.clone(), text: entry.text.clone() }
                    };
                    if let Err(error) = self.command(command) {
                        // One message that cannot be delivered must not hold up the rest. A full
                        // reply inbox clears as the agent reads it, so that one waits.
                        if error.contains("unread replies") {
                            continue;
                        }
                        settle(QueueState::Dropped, &format!("Not delivered: {error}"))?;
                        continue;
                    }
                    match (step, running) {
                        (Plan::Reply, true) => "Stopped the turn and restarted it with the message.",
                        (Plan::Reply, false) => "Delivered as a reply; the agent picked it up at the turn boundary.",
                        _ => "Delivered as a note; the agent reads it when it starts.",
                    }
                }
            };
            settle(QueueState::Delivered, how)?;
            delivered += 1;
        }
        Ok(delivered)
    }
}

/// Who a delegation is attributed to for the cap.
fn origin_of(role: &CrewRole) -> String {
    match role {
        CrewRole::Task { ticket_id, .. } => format!("ticket:{ticket_id}"),
        _ => rows::CHAT.to_owned(),
    }
}

#[cfg(test)]
#[path = "crew_tests.rs"]
mod tests;

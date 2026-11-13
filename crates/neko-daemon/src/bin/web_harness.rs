//! Verification host for the web UI trial (`scripts/web-verify.mjs`). Never
//! part of the shipped app.
//!
//! It runs the daemon's real `server` module (the same `handle_request`, the
//! same workbench snapshot, and the same web bridge) against an isolated data
//! directory, with two deliberate differences from `neko-daemon`:
//!
//! * no clipboard capture loop, so nothing the user copies lands in a test
//!   database (`CLAUDE.md`: the real daemon is never launched for
//!   verification);
//! * the workbench supervisors are not started, so no agent, `gh` call or
//!   model run can happen. A message sent from the web composer is stored and
//!   stays queued, which is exactly what a verification wants to see.
//!
//! It refuses to start unless `NEKO_DATA_DIR` is an absolute path under
//! `/tmp`. `NEKO_WEB_SEED=<snapshot.json>` copies the conversation and tasks
//! out of a saved snapshot, and adds one finished turn with a work log plus
//! one turn that keeps streaming, so every state of the Home chat exists.

// The harness hosts only the socket and the web bridge; the rest of the
// daemon's modules are compiled in but their entry points go unused here.
#![allow(dead_code)]

#[path = "../codex.rs"]
mod codex;
#[path = "../mcp_host.rs"]
mod mcp_host;
#[allow(dead_code)]
#[path = "../phone.rs"]
mod phone;
#[path = "../routines.rs"]
mod routines;
#[path = "../self_update.rs"]
mod self_update;
#[allow(dead_code)]
#[path = "../server.rs"]
mod server;
#[allow(dead_code)]
#[path = "../web.rs"]
mod web;
#[path = "../workbench.rs"]
mod workbench;

use std::path::Path;
use std::sync::Arc;

use neko_core::ui_message as ui;
use neko_core::Db;
use serde_json::json;
use neko_protocol::workbench::{ChatAction, ChatActionStatus, ChatMessage, ChatRole, ChatToolCall, ChatToolStatus, Snapshot, Task};

fn main() {
    let data = neko_protocol::support_dir();
    if std::env::var_os("NEKO_DATA_DIR").is_none() || !data.starts_with("/tmp") {
        eprintln!("web-harness: refusing to start: set NEKO_DATA_DIR to an absolute path under /tmp");
        std::process::exit(2);
    }
    let _ = std::fs::create_dir_all(&data);
    let socket_path = neko_protocol::socket_path();
    let listener = match server::bind_singleton(&socket_path) {
        Ok(Some(listener)) => listener,
        Ok(None) => {
            eprintln!("web-harness: another instance already uses {}, exiting", socket_path.display());
            return;
        }
        Err(e) => {
            eprintln!("web-harness: cannot bind {}: {e}", socket_path.display());
            std::process::exit(1);
        }
    };
    let db = Db::open(&neko_protocol::database_path()).unwrap_or_else(|e| {
        eprintln!("web-harness: cannot open the database: {e}");
        std::process::exit(1);
    });
    // First run is finished unless a check asks for it (`NEKO_WEB_ONBOARDING=1`),
    // so the first-run screen does not sit over every other page's checks.
    let first_run = std::env::var_os("NEKO_WEB_ONBOARDING").is_some_and(|v| v == "1");
    if let Err(e) = neko_core::onboarding::set_onboarding_completed(&db, !first_run) {
        eprintln!("web-harness: cannot set the onboarding state: {e}");
        std::process::exit(1);
    }
    if let Some(seed) = std::env::var_os("NEKO_WEB_SEED") {
        if let Err(e) = seed_from(&db, Path::new(&seed)) {
            eprintln!("web-harness: seeding failed: {e}");
            std::process::exit(1);
        }
    }
    // `NEKO_WEB_SELF_UPDATE=1` runs the real self-update thread (against the
    // remote named by `NEKO_UPDATE_REMOTE`, cloned to `NEKO_UPDATE_SOURCE_DIR`);
    // otherwise Settings → Neko updates is shown sample history and a pending
    // update.
    let self_update = std::env::var_os("NEKO_WEB_SELF_UPDATE").is_some_and(|v| v == "1");
    if !self_update {
        if let Err(e) = seed_update_history(&db) {
            eprintln!("web-harness: cannot seed update history: {e}");
            std::process::exit(1);
        }
    }
    // No app scan: the web UI does not search apps, and the scan is slow.
    let state = Arc::new(server::AppState::new(db, Vec::new()));
    if let Err(e) = seed_checkpoints(&state.db) {
        eprintln!("web-harness: cannot seed checkpoints: {e}");
        std::process::exit(1);
    }
    if self_update {
        self_update::spawn(state.clone());
    }
    // The routines get their runs here, not from an agent: open items and
    // items closed for each reason, so the Inbox's "Closed recently" shows.
    if let Err(e) = seed_inbox(&state) {
        eprintln!("web-harness: cannot seed the inbox: {e}");
        std::process::exit(1);
    }
    // A reply is "in progress" for the whole run, so a message sent from the
    // web composer queues behind it and no model, `gh` or agent ever starts.
    // `NEKO_WEB_SCRIPTED=1` instead answers every message with a scripted
    // reply, streamed through the real chunks, reader and completion path.
    if std::env::var_os("NEKO_WEB_SCRIPTED").is_some_and(|v| v == "1") {
        workbench::SCRIPTED_CHAT.store(true, std::sync::atomic::Ordering::Release);
    } else {
        state.workbench.hold_chat_for_verification();
    }
    server::spawn_web_bridge(state.clone());
    if let Some(id) = std::env::var_os("NEKO_WEB_STREAM").and_then(|_| pending_turn_id(&state)) {
        let state = state.clone();
        std::thread::spawn(move || stream_text(&state, &id));
    }
    eprintln!("web-harness: listening on {}", socket_path.display());
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = state.clone();
        std::thread::spawn(move || server::handle_connection(state, stream));
    }
}

/// A real temporary repository for one seeded ticket, with three checkpointed
/// runs (the second one commits and pushes to a local bare remote), so the
/// task view's run lines, per-run diffs and undo have something to act on.
fn seed_checkpoints(db: &std::sync::Mutex<Db>) -> Result<(), String> {
    use std::process::Command as Git;
    let mut snapshot = {
        let guard = db.lock().map_err(|_| "db")?;
        neko_core::workbench::load(&guard)?
    };
    let Some(task) = snapshot.tasks.iter_mut().find(|t| t.title == "Stop the nightly export running twice") else { return Ok(()) };
    let data = neko_protocol::support_dir();
    let repo = data.join("seed-repo");
    let worktree = data.join("task-worktrees").join(&task.id);
    let git = |dir: &Path, args: &[&str]| -> Result<(), String> {
        let out = Git::new("git").current_dir(dir).args(["-c", "user.name=Seed", "-c", "user.email=seed@example.com", "-c", "commit.gpgsign=false"]).args(args).output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok(()) } else { Err(format!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr))) }
    };
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(worktree.parent().unwrap_or(&data)).map_err(|e| e.to_string())?;
    let bare = data.join("seed-origin.git");
    std::fs::create_dir_all(&bare).map_err(|e| e.to_string())?;
    git(&bare, &["init", "-q", "--bare", "-b", "main"])?;
    git(&repo, &["init", "-q", "-b", "main"])?;
    std::fs::write(repo.join("export.sh"), "#!/bin/sh\nrun_export\n").map_err(|e| e.to_string())?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "-qm", "export script"])?;
    git(&repo, &["remote", "add", "origin", bare.to_str().unwrap_or_default()])?;
    git(&repo, &["worktree", "add", "-q", "-b", "codex/neko-seed", worktree.to_str().unwrap_or_default()])?;
    let run = |work: &dyn Fn() -> Result<(), String>| -> Result<(), String> {
        let n = neko_core::checkpoints::begin_run(db, &task.id, &worktree, neko_core::workbench::now_ms())?;
        work()?;
        neko_core::checkpoints::end_run(db, &task.id, n, &worktree, neko_core::checkpoints::Outcome::Done, neko_core::workbench::now_ms())?;
        Ok(())
    };
    let write = |name: &str, text: &str| std::fs::write(worktree.join(name), text).map_err(|e| e.to_string());
    run(&|| write("export.sh", "#!/bin/sh\nlock_file=/tmp/export.lock\nrun_export\n"))?;
    run(&|| {
        write("export.sh", "#!/bin/sh\nlock_file=/tmp/export.lock\ntest -e $lock_file && exit 0\nrun_export\n")?;
        git(&worktree, &["commit", "-qam", "skip when the lock exists"])?;
        git(&worktree, &["push", "-q", "origin", "codex/neko-seed"])
    })?;
    run(&|| write("NOTES.md", "The export now takes a lock.\n"))?;
    task.worktree = Some(worktree.to_string_lossy().into_owned());
    let guard = db.lock().map_err(|_| "db")?;
    neko_core::workbench::save(&guard, &snapshot)
}

/// Made-up Inbox items on the home scope's first routine: three open, and
/// closed ones with each reason (plus one closed long ago, which stays hidden).
fn seed_inbox(state: &server::AppState) -> Result<(), String> {
    use neko_core::inbox;
    use neko_protocol::routines::{NeedsYou, RoutineRun};
    // Routines are off on a fresh data folder; the Inbox pages need them on.
    // (scripts/verify/firstrun.mjs switches them off again to start fresh.)
    neko_core::routines::set_master(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner), true)?;
    // Asking for the routines creates the home scope's.
    routines::handle(state, neko_protocol::Request::Routines);
    let db = state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let store = neko_core::routines::load(&db)?;
    let routine = store.routines.first().ok_or("no routine was seeded")?;
    let now = neko_core::workbench::now_ms();
    let item = |seq: u64, title: &str, detail: &str, link: Option<&str>, reason: Option<(&str, i64)>, expires: Option<i64>| {
        let mut item = NeedsYou { seq, title: title.into(), detail: detail.into(), link: link.map(Into::into), created_ms: now - 2 * inbox::DAY_MS, touched_ms: now - 2 * inbox::DAY_MS, expires_ms: expires, ..Default::default() };
        if let Some((why, ago)) = reason {
            inbox::close(&mut item, why, now - ago);
        }
        item
    };
    let hour = 3_600_000;
    let items = vec![
        item(1, "Review the retry change", "Reviewer asked for a second look. Pull request: acme/widgets#42.", Some("https://example.com/acme/widgets/pull/42"), None, None),
        item(2, "Reply before the 3pm call", "Sam is waiting on the budget numbers.", None, None, Some(now + 4 * hour)),
        item(3, "Enable the calendar API", "The calendar server needs the API enabled for its project.", None, None, None),
        item(4, "Review the schema change", "acme/widgets#40 was waiting on you.", None, Some((inbox::PR_MERGED, 2 * hour)), None),
        item(5, "Reply before standup", "Quick question about the rollout.", None, Some((inbox::EXPIRED, 5 * hour)), None),
        item(6, "Ask about the old launch plan", "Nobody has touched this for a week.", None, Some((inbox::AGED_OUT, 26 * hour)), None),
        item(7, "Sign in to the issue tracker", "The token expired.", None, Some((inbox::BY_ROUTINE, 30 * hour)), None),
        item(8, "Check the staging deploy", "You handled it.", None, Some((inbox::BY_YOU, 50 * hour)), None),
        item(9, "Closed long ago", "Past the window.", None, Some((inbox::BY_YOU, 10 * inbox::DAY_MS)), None),
    ];
    let run = RoutineRun { id: "seed-run".into(), routine_id: routine.id.clone(), started_ms: now - hour, finished_ms: Some(now - hour), ok: true, needs_you: items, ..Default::default() };
    neko_core::routines::push_run(&db, &routine.id, run)
}

/// Sample self-update state: an applied update, a rollback, a recent check,
/// and an update waiting for a click because auto-update is off for the app.
fn seed_update_history(db: &Db) -> Result<(), String> {
    use neko_protocol::workbench::{UpdatePlan, UpdateRecord};
    let mut snapshot = neko_core::workbench::load(db)?;
    let now = neko_core::workbench::now_ms();
    let u = &mut snapshot.self_update;
    u.checkout = Some("/Users/example/Library/Application Support/neko/source".into());
    u.check = neko_protocol::workbench::UpdateCheck { at_ms: now - 4 * 60_000, remote_head: Some("d03c977a9b10".into()), error: String::new() };
    u.native_commit = Some("9acc5352f3c1".into());
    u.web_commit = Some("c7b6c58d0e4f".into());
    u.last_update = Some(UpdateRecord { commit: "c7b6c58d0e4f".into(), at_ms: now - 42 * 60_000, summary: "Neko updated itself to c7b6c58d (UI live).".into() });
    u.last_rollback = Some(UpdateRecord { commit: "5dc59a5b1e22".into(), at_ms: now - 26 * 3_600_000, summary: "Rolled back the update to 5dc59a5b: the new version did not become healthy within two minutes. The previous app is running again.".into() });
    u.pending = Some(UpdatePlan {
        from: Some("c7b6c58d0e4f".into()), to: "d03c977a9b10".into(), task_id: None, web: true, native: true,
        sensitive: vec!["crates/neko-daemon/src/web.rs".into()],
        reason: "Auto-update is off for the app.".into(),
        approved: false, at_ms: now - 5 * 60_000,
    });
    neko_core::workbench::save(db, &snapshot)
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max { text.to_string() } else { text.chars().take(max).collect::<String>() + "…" }
}

/// Copies workspaces, tasks and the recent conversation, shortens the heavy
/// fields, and appends the two demonstration turns.
fn seed_from(db: &Db, path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let source: Snapshot = serde_json::from_str(&text).map_err(|e| format!("cannot read the seed snapshot: {e}"))?;
    let now = neko_core::workbench::now_ms();

    let mut snapshot = Snapshot::default();
    // Keep every pull-request task (the plugin's subject) and the most recent others.
    let mut tasks: Vec<Task> = source.tasks.iter().filter(|t| t.pull_request.is_some()).cloned().collect();
    tasks.extend(source.tasks.iter().rev().filter(|t| t.pull_request.is_none()).take(14).cloned());
    for task in &mut tasks {
        let keep = task.events.len().saturating_sub(2);
        task.events.drain(..keep);
        for event in &mut task.events { event.message = clip(&event.message, 400); }
        task.plan = clip(&task.plan, 1500);
        task.result = clip(&task.result, 1500);
        task.worktree = None;
        task.parked = None;
    }
    snapshot.workspaces = source.workspaces.clone();
    snapshot.tasks = tasks;
    // Real workspaces point at real checkouts; nothing here may reach them.
    let nowhere = neko_protocol::support_dir().join("no-checkout");
    for workspace in &mut snapshot.workspaces { workspace.repository = nowhere.to_string_lossy().into_owned(); }
    neko_core::workbench::save(db, &snapshot)?;
    // Goals are rows of their own; the blocked and the paused one raise their Inbox items.
    for source_goal in &source.goals {
        let mut goal = source_goal.clone();
        let (ledger, journal) = (std::mem::take(&mut goal.ledger), std::mem::take(&mut goal.journal));
        let subject = goal.subject();
        let item = match goal.status {
            neko_protocol::goals::GoalStatus::Blocked => goal.blocked.as_ref().map(|b| (b.question.clone(), format!("If you do not answer, Neko will go with: {}", b.default_answer))),
            neko_protocol::goals::GoalStatus::Paused => Some(("Extend by 10 turns?".to_owned(), format!("Paused: it used its {} turns.\nGoal: {}", goal.budget.max_turns, goal.objective))),
            _ => None,
        };
        // Only for a run that checks the goals (`NEKO_WEB_ONLY=goals`): the Inbox area counts its items.
        let item = item.filter(|_| std::env::var_os("NEKO_WEB_GOAL_ITEMS").is_some());
        if let Some((title, detail)) = item {
            let seq = routines::raise_goal_item(db, &goal.workspace_id, &title, &detail, &format!("goal:{}", goal.id))?;
            match goal.status {
                neko_protocol::goals::GoalStatus::Blocked => if let Some(b) = goal.blocked.as_mut() { b.inbox_seq = Some(seq); },
                _ => goal.extend_seq = Some(seq),
            }
        }
        neko_core::goals::create(db, goal)?;
        for entry in ledger { neko_core::goals::push_ledger(db, &subject, entry)?; }
        for entry in journal { neko_core::goals::push_journal(db, &subject, entry)?; }
    }

    let mut conversation: Vec<ChatMessage> = source.conversation.iter().rev().take(8).rev().cloned().collect();
    // A saved snapshot from before the AI SDK stored `text`; read it back as one text part.
    let saved: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let saved = saved["conversation"].as_array().cloned().unwrap_or_default();
    let skipped = source.conversation.len() - conversation.len();
    for (index, message) in conversation.iter_mut().enumerate() {
        if message.parts.is_empty() {
            if let Some(text) = saved.get(skipped + index).and_then(|m| m["text"].as_str()) { message.parts = ChatMessage::text_parts(text); }
        }
    }
    // The last real reply becomes the "worked for 1m 12s" example: reasoning,
    // commands with their output (one failed), an edit, then its prose.
    if let Some(last) = conversation.iter_mut().rev().find(|m| m.role == ChatRole::Neko && !m.pending) {
        last.started_ms = last.at_ms - 72_000;
        let prose = last.text();
        let mut chunks = vec![json!({"type": "start-step"})];
        chunks.extend([
            json!({"type": "reasoning-start", "id": "r"}),
            json!({"type": "reasoning-delta", "id": "r", "delta": "Look at what GitHub reports for each open pull request before answering."}),
            json!({"type": "reasoning-end", "id": "r"}),
            ui::tool_input("h1", "Bash", &json!({"command": "gh pr list --state open --json number,title,statusCheckRollup"})),
            ui::tool_output("h1", json!({"exit_code": 0, "output": "101 Update the branch\n102 Fix the sync\n103 Cache the index"})),
            ui::tool_input("h2", "Bash", &json!({"command": "gh pr view 101 --json reviews,mergeStateStatus"})),
            ui::tool_output("h2", json!({"exit_code": 0, "output": "{\"mergeStateStatus\":\"BEHIND\"}"})),
            ui::tool_input("h3", "Read", &json!({"file_path": "/Users/you/Documents/neko/crates/neko-core/src/neko_chat.rs"})),
            ui::tool_output("h3", json!({"output": "…"})),
            ui::tool_input("h4", "Bash", &json!({"command": "git log --oneline -5"})),
            ui::tool_output("h4", json!({"exit_code": 128, "output": "fatal: not a git repository"})),
        ]);
        chunks.extend(ui::text_chunks("t", &prose));
        last.parts = parts_of(&chunks);
    }
    // An earlier reply that asked a question with a choices block, and one with a chart.
    let asked = ChatMessage {
        id: neko_core::workbench::new_id(), at_ms: now - 120_000, started_ms: now - 126_000, role: ChatRole::Neko,
        parts: parts_of(&[
            json!({"type": "start-step"}),
            ui::text_chunks("a", "Two of them are blocked. Which should I take first?")[0].clone(),
            json!({"type": "text-delta", "id": "a", "delta": "Two of them are blocked. Which should I take first?"}),
            json!({"type": "text-end", "id": "a"}),
            json!({"type": "data-neko-choices", "data": {"question": "Which pull request first?", "options": [
                {"label": "acme-api #101", "detail": "Branch is behind master"}, {"label": "acme-api #104", "detail": "Reviewer asked for a change"}]}}),
            json!({"type": "data-neko-chart", "data": {"title": "Open for", "source": "GitHub", "unit": " days", "bars": [
                {"label": "acme-api #101", "value": 6.0, "highlight": true}, {"label": "acme-api #104", "value": 3.0}, {"label": "acme-web #110", "value": 1.0}]}}),
        ]),
        ..blank_message(ChatRole::Neko)
    };
    let at = conversation.len().saturating_sub(0);
    conversation.insert(at, asked);
    let first_task = snapshot.tasks.first().map(|t| t.id.clone()).unwrap_or_default();
    let user_id = neko_core::workbench::new_id();
    conversation.push(ChatMessage {
        agent_profile_revision: 0,
        agent_profile_id: "default".into(),
        workspace_id: None,
        tool_calls: vec![],
        id: user_id,
        at_ms: now - 25_000,
        role: ChatRole::User,
        parts: ChatMessage::text_parts("what is blocking the pull requests that are ready to review?"),
        ticket_ids: vec![],
        pending: false,
        queued: false,
        failed: false,
        responsibility_ids: vec![],
        actions: vec![],
        started_ms: 0,
    });
    conversation.push(ChatMessage {
        agent_profile_revision: 0,
        agent_profile_id: "default".into(),
        workspace_id: None,
        tool_calls: vec![ChatToolCall {
            id: "call-1".into(),
            workspace_id: "w".into(),
            connection_id: "linear".into(),
            tool_name: "save_issue".into(),
            arguments_json: r#"{"title":"Update acme-api #101 branch","team":"ENG","description":"Rebase onto master and rerun checks"}"#.into(),
            status: ChatToolStatus::AwaitingApproval,
        }],
        id: neko_core::workbench::new_id(),
        at_ms: now - 24_000,
        role: ChatRole::Neko,
        parts: pending_parts(&first_task),
        ticket_ids: vec![],
        pending: true,
        queued: false,
        failed: false,
        responsibility_ids: vec![],
        actions: vec![ChatAction {
            id: "act-1".into(),
            task_id: first_task.clone(),
            kind: "message".into(),
            text: "Bring the branch up to date with master, resolve any conflicts without changing the PR's scope, and rerun the checks. Do not merge.".into(),
            status: ChatActionStatus::Proposed,
            note: String::new(),
        }],
        started_ms: now - 24_000,
    });
    // The pending reply is in flight: followers attach to it like any other.
    if let Some(turn) = conversation.last() { ui::begin(&turn.id, turn.parts.clone(), turn.started_ms); }
    neko_core::neko_chat::save(db, &conversation)
}

fn blank_message(role: ChatRole) -> ChatMessage {
    ChatMessage {
        agent_profile_revision: 0, agent_profile_id: "default".into(), workspace_id: None, tool_calls: vec![], id: String::new(), at_ms: 0,
        role, parts: vec![], ticket_ids: vec![], pending: false, queued: false, failed: false, responsibility_ids: vec![], actions: vec![], started_ms: 0,
    }
}

fn parts_of(chunks: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut parts = Vec::new();
    for chunk in chunks { ui::apply_chunk(&mut parts, chunk); }
    parts
}

/// What the in-flight reply holds when the page opens: prose, a command, and
/// the two things waiting for a click (a tool call and a proposed action).
fn pending_parts(task_id: &str) -> Vec<serde_json::Value> {
    let mut chunks = vec![json!({"type": "start-step"})];
    chunks.extend(ui::text_chunks("p", "Checking the five open pull requests against what GitHub reports now."));
    chunks.extend([
        ui::tool_input("p1", "Bash", &json!({"command": "gh pr list --state open"})),
        ui::tool_output("p1", json!({"exit_code": 0, "output": "5 open"})),
        { let mut call = ui::tool_input("call-1", "save_issue", &json!({"title": "Update acme-api #101 branch", "team": "HEA", "description": "Rebase onto master and rerun checks"})); call["toolMetadata"] = json!({"connectionId": "linear"}); call },
        json!({"type": "tool-approval-request", "toolCallId": "call-1", "approvalId": "call-1", "reason": "Uses linear"}),
        ui::tool_input("action:act-1", neko_core::neko_chat::ACTION_TOOL, &json!({"taskId": task_id, "kind": "message", "text": "Bring the branch up to date with master, resolve any conflicts without changing the PR's scope, and rerun the checks. Do not merge."})),
        json!({"type": "tool-approval-request", "toolCallId": "action:act-1", "approvalId": "act-1"}),
    ]);
    parts_of(&chunks)
}

fn pending_turn_id(state: &server::AppState) -> Option<String> {
    let db = state.db.lock().ok()?;
    neko_core::neko_chat::load(&db).ok()?.into_iter().rev().find(|m| m.pending).map(|m| m.id)
}

/// Grows the pending turn's text word by word, as a streaming reply does:
/// chunks into the reply in flight (every window following it sees them), and
/// the parts written when a write is due.
fn stream_text(state: &server::AppState, id: &str) {
    let words = "Three of the five only need an approving review: **acme-api #102**, **acme-api #103** and **acme-web #110**.\n\nTheir checks pass and none has a conflict. The other two need work first.\n\n| PR | Blocker |\n|---|---|\n| acme-api #101 | Branch is behind `master` |\n| acme-api #104 | Reviewer asked for a change |\n\nI have not messaged any agent yet.";
    let send = |chunk: serde_json::Value| {
        if let Some(parts) = ui::emit(id, &chunk) {
            if let Ok(db) = state.db.lock() { let _ = neko_core::neko_chat::persist_parts(&db, id, parts); }
        }
    };
    send(json!({"type": "text-start", "id": "s"}));
    for word in words.split_inclusive(' ') {
        std::thread::sleep(std::time::Duration::from_millis(220));
        send(json!({"type": "text-delta", "id": "s", "delta": word}));
    }
}

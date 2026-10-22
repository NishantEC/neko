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
#[path = "../server.rs"]
mod server;
#[allow(dead_code)]
#[path = "../web.rs"]
mod web;
#[path = "../workbench.rs"]
mod workbench;

use std::path::Path;
use std::sync::Arc;

use neko_core::Db;
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
    // No app scan: the web UI does not search apps, and the scan is slow.
    let state = Arc::new(server::AppState::new(db, Vec::new()));
    // A reply is "in progress" for the whole run, so a message sent from the
    // web composer queues behind it and no model, `gh` or agent ever starts.
    state.workbench.hold_chat_for_verification();
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

    let mut conversation: Vec<ChatMessage> = source.conversation.iter().rev().take(8).rev().cloned().collect();
    // The last real reply becomes the "worked for 1m 12s" example.
    if let Some(last) = conversation.iter_mut().rev().find(|m| m.role == ChatRole::Neko && !m.pending) {
        last.started_ms = last.at_ms - 72_000;
        last.activity = vec![
            "Ran `gh pr list --state open --json number,title,statusCheckRollup`".into(),
            "Ran `gh pr view 4641 --json reviews,mergeStateStatus`".into(),
            "Read neko_chat.rs".into(),
            "Ran `git log --oneline -5` · failed".into(),
        ];
    }
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
        text: "what is blocking the pull requests that are ready to review?".into(),
        ticket_ids: vec![],
        pending: false,
        queued: false,
        failed: false,
        remembered: vec![],
        responsibility_ids: vec![],
        actions: vec![],
        activity: vec![],
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
            arguments_json: r#"{"title":"Update athena #4641 branch","team":"HEA","description":"Rebase onto master and rerun checks"}"#.into(),
            status: ChatToolStatus::AwaitingApproval,
        }],
        id: neko_core::workbench::new_id(),
        at_ms: now - 24_000,
        role: ChatRole::Neko,
        text: "Checking the five open pull requests against what GitHub reports now.\n\n".into(),
        ticket_ids: vec![],
        pending: true,
        queued: false,
        failed: false,
        remembered: vec![],
        responsibility_ids: vec![],
        actions: vec![ChatAction {
            id: "act-1".into(),
            task_id: first_task.clone(),
            kind: "message".into(),
            text: "Bring the branch up to date with master, resolve any conflicts without changing the PR's scope, and rerun the checks. Do not merge.".into(),
            status: ChatActionStatus::Proposed,
            note: String::new(),
        }],
        activity: vec![
            "Ran `gh pr list --state open`".into(),
            "Ran `gh pr checks 4628`".into(),
            "Ran `gh pr view 4616 --json reviewDecision`".into(),
        ],
        started_ms: now - 24_000,
    });
    neko_core::neko_chat::save(db, &conversation)
}

fn pending_turn_id(state: &server::AppState) -> Option<String> {
    let db = state.db.lock().ok()?;
    neko_core::neko_chat::load(&db).ok()?.into_iter().rev().find(|m| m.pending).map(|m| m.id)
}

/// Grows the pending turn's text word by word, as a streaming reply does.
fn stream_text(state: &server::AppState, id: &str) {
    let words = "Three of the five only need an approving review: **athena #4628**, **athena #4616** and **riavox #410**.\n\nTheir checks pass and none has a conflict. The other two need work first.\n\n| PR | Blocker |\n|---|---|\n| athena #4641 | Branch is behind `master` |\n| athena #4623 | Reviewer asked for a change |\n\nI have not messaged any agent yet.";
    let mut shown = String::from("Checking the five open pull requests against what GitHub reports now.\n\n");
    for word in words.split_inclusive(' ') {
        std::thread::sleep(std::time::Duration::from_millis(220));
        shown.push_str(word);
        if let Ok(db) = state.db.lock() {
            let _ = neko_core::neko_chat::record_partial(&db, id, &shown);
        }
    }
}

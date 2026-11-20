//! Named states for headless snapshots (`--snap out.png --scene live`). Each modifies the
//! initial (headless) state, so a scene is also a test fixture.
//!
//! The `harness` scene is the exception that proves the link: it reads the workbench summary that
//! `neko-rune probe json` printed from a test daemon (`scripts/snap-harness.sh`), so the rows in
//! that snapshot are what a real daemon sent, not made up here.

use neko_protocol::checkpoints::{CheckpointList, CheckpointRun};
use neko_protocol::crew::{DeliveryMode, QueueKind, QueueState, QueuedMessage};
use neko_protocol::digest::{Digest, DigestLine};
use neko_protocol::goals::{Blocked, Budget, CheckResult, Goal, GoalStatus, JournalEntry};
use neko_protocol::workbench::{ChatMessage, ChatRole, Task, TaskStatus};
use rune_kit::diff::DiffState;

use crate::app::{App, DEMO_NOW, down};
use crate::link::{Detail, InboxItem, LinkError, LinkEvent, Summary, TaskRow, Wait, WorkspaceRow};
use crate::views::FullView;
use crate::web_link::WebTarget;
use crate::work::{DiffLoad, RunKey, diff_key};

/// A made-up workbench: a few of each status, titles that name nothing real, ages from minutes to
/// days before [`DEMO_NOW`].
pub fn demo_summary() -> Summary {
    use TaskStatus::*;
    let shape = [
        (AwaitingApproval, 2),
        (ReadyForReview, 3),
        (Failed, 1),
        (Building, 3),
        (Planning, 1),
        (Reviewing, 1),
        (Queued, 4),
        (Completed, 6),
        (Cancelled, 1),
    ];
    let titles = ["Add a retry to the sync job", "Fix the empty state on the board", "Split the settings page", "Rename the export command", "Cache the repository list"];
    let goals = ["failed requests are retried before the run stops", "the board shows a message when it is empty", "one section per file, same tests", "old name still works for a release", "refresh at most once a minute"];
    let mut tasks = Vec::new();
    for (status, n) in shape {
        for i in 0..n {
            let at = tasks.len();
            // Finished work is older the further down the list; the rest changed in the last hour.
            let ago_min: i64 = match status {
                Completed | Cancelled => 90 + 120 * i as i64,
                _ => 2 + 7 * at as i64,
            };
            tasks.push(TaskRow {
                id: format!("t{at}"),
                key: Some(format!("NK-{}", 100 + at)),
                title: titles[(at + i) % titles.len()].to_owned(),
                status,
                updated_at_ms: DEMO_NOW - ago_min * 60_000,
                goal: goals[(at + i) % goals.len()].to_owned(),
                line: crate::link::TaskLine { short_id: format!("NEK-{:04X}", 0x1BB3 + at), note: if matches!(status, Failed) || (status == AwaitingApproval && i % 2 != 0) { "Needs your input: which branch?".to_owned() } else { String::new() }, ..Default::default() },
                wait: match status {
                    AwaitingApproval if i % 2 == 0 => Wait::Plan,
                    AwaitingApproval => Wait::Question,
                    ReadyForReview => Wait::Review,
                    Failed => Wait::Failed,
                    _ => Wait::None,
                },
                // Two running rows have a fraction, the rest turn.
                progress: matches!(status, Building).then_some((2 + i as u32, 7)).filter(|_| i < 2),
            });
        }
    }
    Summary {
        digest: "demo".into(),
        runtime: "Codex · default model".into(),
        tasks,
        workspaces: vec![WorkspaceRow { id: "w1".into(), name: "Neko".into() }, WorkspaceRow { id: "w2".into(), name: "Docs site".into() }],
        ..Summary::default()
    }
}

fn task(row: &TaskRow, events: Vec<(&str, String)>, plan: &str, result: &str) -> Task {
    let mut t: Task = serde_json::from_value(serde_json::json!({
        "id": row.id, "workspace_id": "w1", "issue_id": row.key, "title": row.title, "goal": row.title, "status": "Queued",
        "plan": plan, "result": result, "worktree": null, "events": [],
        "created_at_ms": row.updated_at_ms - 3_600_000, "updated_at_ms": row.updated_at_ms,
    }))
    .expect("a task");
    t.status = row.status;
    t.goal = format!("{}.", row.title);
    t.events = events.into_iter().enumerate().map(|(i, (role, message))| neko_protocol::workbench::TaskEvent { at_ms: row.updated_at_ms - 3_000_000 + 60_000 * i as i64, role: role.into(), message }).collect();
    t
}

fn receipt(command: &str, output: &str, exit: i64) -> String {
    format!("VERIFICATION_COMMAND {}", serde_json::json!({"command": command, "output": output, "exit_code": exit}))
}

/// What a session that is waiting for approval looks like: the brief, the planner's words, a plan.
fn awaiting_detail(row: &TaskRow) -> Detail {
    let plan = "**Plan**\n\n1. Read `sync_job.rs` and find where a failure ends the run.\n2. Retry up to three times, waiting longer each time.\n3. Add a test that fails twice and then succeeds.";
    let events = vec![
        ("scout", "I read the sync job. A failed request ends the run at once, so one bad moment loses the whole batch.".to_owned()),
        ("scout", receipt("rg -n retry crates/sync/src", "crates/sync/src/job.rs:42: // TODO retry", 0)),
        ("scout", "Plan ready.".to_owned()),
    ];
    Detail { task: task(row, events, plan, ""), has_folder: false, workspace: "Neko".into(), runtime: "Claude · default".into(), can_split: true, source: None, prefs: Default::default(), prefs_revision: 0, splits: Vec::new(), decisions: Vec::new() }
}

/// A session being built: the planner's words, commands, an edit, an open question for review.
fn running_detail(row: &TaskRow) -> Detail {
    let events = vec![
        ("scout", "The board component draws nothing when there are no tasks. I will add an empty state.".to_owned()),
        ("scout", "Plan ready.".to_owned()),
        ("system", "Plan approved".to_owned()),
        ("builder", "Reading the board first.".to_owned()),
        ("builder", receipt("rg -n EmptyBoard web/src", "web/src/board.tsx:88: const rows = tasks.map(Row)", 0)),
        ("builder", receipt("cargo test -p board", "test board::empty_state ... FAILED\n\nfailures:\n    board::empty_state\n\ntest result: FAILED. 11 passed; 1 failed", 101)),
        ("builder", "The new test fails as it should, because nothing draws the message yet. Adding it now.".to_owned()),
        ("builder", receipt("cargo test -p board", "test result: ok. 12 passed; 0 failed", 0)),
        ("builder", "Added the message, with a test. All twelve board tests pass.".to_owned()),
    ];
    Detail { task: task(row, events, "", ""), has_folder: true, workspace: "Neko".into(), runtime: "Claude · default".into(), can_split: false, source: None, prefs: Default::default(), prefs_revision: 0, splits: Vec::new(), decisions: Vec::new() }
}

fn goal_for(task_id: &str) -> Goal {
    Goal {
        id: "g1".into(),
        workspace_id: "w1".into(),
        objective: "The board shows a message when it is empty".into(),
        done_check: "cargo test -p board".into(),
        budget: Budget { max_turns: 20, ..Budget::default() },
        ticket_id: Some(task_id.into()),
        status: GoalStatus::Running,
        turns: 4,
        last_check: Some(CheckResult { command: "cargo test -p board".into(), exit_code: Some(101), tail: "1 failed".into(), at_ms: DEMO_NOW - 120_000, source: "neko".into() }),
        ledger: vec![
            CheckResult { command: "cargo test -p board".into(), exit_code: Some(101), tail: "board::empty_state FAILED".into(), at_ms: DEMO_NOW - 600_000, source: "neko".into() },
            CheckResult { command: "cargo test -p board".into(), exit_code: Some(101), tail: "1 failed".into(), at_ms: DEMO_NOW - 120_000, source: "neko".into() },
        ],
        journal: vec![
            JournalEntry { turn: 3, done: "Wrote the failing test.".into(), next: "Draw the message.".into(), ..JournalEntry::default() },
            JournalEntry { turn: 4, done: "Drew the message.".into(), next: "Run the check again.".into(), ..JournalEntry::default() },
        ],
        verdict: "CONTINUE".into(),
        reason: "The check still fails on one test.".into(),
        ..Goal::default()
    }
}

fn runs() -> CheckpointList {
    let run = |n, files, added, removed, undone| CheckpointRun { n, outcome: "done".into(), files, added, removed, undone, ..CheckpointRun::default() };
    CheckpointList { runs: vec![run(1, 2, 30, 4, false), run(2, 4, 120, 30, false), run(3, 1, 8, 2, false)], restorable: true, ..CheckpointList::default() }
}

const PATCH: &str = "diff --git a/web/src/board.tsx b/web/src/board.tsx\n--- a/web/src/board.tsx\n+++ b/web/src/board.tsx\n@@ -86,6 +86,10 @@ export function Board({ tasks }) {\n   const rows = tasks.map(Row);\n+  if (rows.length === 0) {\n+    return <p>No tasks yet.</p>;\n+  }\n   return <section>{rows}</section>;\n }\n";

fn chat() -> Vec<ChatMessage> {
    let parts = |text: &str| ChatMessage::text_parts(text);
    let mut msgs = vec![
        ChatMessage { id: "m1".into(), at_ms: DEMO_NOW - 600_000, role: ChatRole::User, parts: parts("what is blocking the board work?"), ..blank() },
        ChatMessage {
            id: "m2".into(),
            at_ms: DEMO_NOW - 590_000,
            role: ChatRole::Neko,
            parts: parts("One test fails: **board::empty_state**. The builder is on it and expects to be done after one more turn."),
            ticket_ids: vec!["t6".into()],
            ..blank()
        },
        ChatMessage { id: "m3".into(), at_ms: DEMO_NOW - 20_000, role: ChatRole::User, parts: parts("summarise what finished today"), ..blank() },
    ];
    msgs.push(ChatMessage {
        id: "m4".into(),
        at_ms: DEMO_NOW - 15_000,
        role: ChatRole::Neko,
        pending: true,
        parts: vec![
            serde_json::json!({"type": "dynamic-tool", "toolName": "Bash", "toolCallId": "c1", "state": "output-available", "input": {"command": "neko list_tasks --done"}, "output": {"exit_code": 0, "output": "6 tasks"}}),
            serde_json::json!({"type": "text", "text": "Six things finished today. The two largest were the **export rename** and the **settings split**, and", "state": "streaming"}),
        ],
        ..blank()
    });
    msgs
}

fn blank() -> ChatMessage {
    ChatMessage {
        agent_profile_revision: 0,
        agent_profile_id: "default".into(),
        workspace_id: None,
        tool_calls: vec![],
        id: String::new(),
        at_ms: 0,
        role: ChatRole::User,
        parts: vec![],
        ticket_ids: vec![],
        pending: false,
        queued: false,
        failed: false,
        responsibility_ids: vec![],
        actions: vec![],
        started_ms: 0,
    }
}

fn inbox() -> Vec<InboxItem> {
    vec![
        InboxItem { goal: None, routine_id: "r-messages".into(), run_id: "run-3".into(), closed_ms: None, reason: String::new(), seq: 3, title: "Reply before the 3pm call".into(), detail: "Sam is waiting on the budget numbers.".into(), default: Some("Send the numbers from last week".into()), link: None, urgent: false, from: "Messages".into(), at_ms: DEMO_NOW - 600_000 },
        InboxItem { goal: Some("g1".into()), routine_id: "goals:g1".into(), run_id: "run-2".into(), closed_ms: None, reason: String::new(), seq: 2, title: "Extend by 10 turns?".into(), detail: "Paused: it used its 20 turns.".into(), default: Some("Extend by 10 turns".into()), link: None, urgent: false, from: "Goals".into(), at_ms: DEMO_NOW - 3_600_000 },
        InboxItem { goal: None, routine_id: "r-calendar".into(), run_id: "run-1".into(), closed_ms: None, reason: String::new(), seq: 1, title: "Enable the calendar API".into(), detail: "The calendar server needs the API enabled for its project.".into(), default: None, link: Some("https://example.com/enable".into()), urgent: false, from: "Calendar".into(), at_ms: DEMO_NOW - 7_200_000 },
    ]
}

fn digest() -> Digest {
    let line = |text: &str, default: Option<&str>| DigestLine { text: text.into(), default: default.map(Into::into), link: None };
    Digest {
        at_ms: DEMO_NOW - 3_600_000,
        day: "2027-01-15".into(),
        headline: "3 things finished, 1 thing is stuck (waiting on a check). 2 things need you.".into(),
        finished: vec![line("Rename the export command", None), line("Split the settings page", None), line("Cache the repository list", None)],
        stuck: vec![line("Fix the empty state on the board: the check still fails", None)],
        decisions: vec![line("Reply before the 3pm call", Some("Send the numbers from last week")), line("Enable the calendar API", None)],
        calendar: "3 events today, first: Standup at 09:30.".into(),
        spend: String::new(),
        read: false,
    }
}

fn queued() -> Vec<QueuedMessage> {
    vec![
        QueuedMessage { id: "q1".into(), task_id: "t6".into(), kind: QueueKind::Send, mode: DeliveryMode::AfterTurn, text: "Also add the empty state to the list view.".into(), state: QueueState::Queued, source: "you".into(), ..QueuedMessage::default() },
        QueuedMessage { id: "q2".into(), task_id: "t6".into(), kind: QueueKind::Steer, mode: DeliveryMode::SteerNow, text: "Keep the message short.".into(), state: QueueState::Queued, source: "you".into(), ..QueuedMessage::default() },
    ]
}

/// The full demo: rows, workspaces, Neko's conversation, goals and the queue; the task named by
/// `open` is in detail.
pub fn demo_full(open: &str) -> Summary {
    let mut s = demo_summary();
    s.conversation = chat();
    s.goals = vec![goal_for("t6")];
    s.queued = queued();
    let row = s.task(open).cloned().expect("a demo task");
    s.detail = Some(match row.status {
        TaskStatus::Building | TaskStatus::Planning | TaskStatus::Reviewing | TaskStatus::ReadyForReview => running_detail(&row),
        _ => awaiting_detail(&row),
    });
    s
}

/// The session views of P1 and P2 are shown with the context panel closed, as the person left it:
/// the demo sessions have commands in their events, which would otherwise open it (`panel.rs`).
fn panel_closed(s: &mut App) {
    if let Some(id) = s.selected.clone() {
        s.panel.sessions.entry(id).or_default().person_closed = true;
    }
    s.layout.context.set_open(false);
}

pub fn live_scene(s: &mut App) {
    live(s);
}

fn live(s: &mut App) {
    s.apply_link(LinkEvent::Live(demo_full("t0")));
    s.open_first_session();
    s.set_web(Some(WebTarget::new(4747, &"0".repeat(64))));
    s.neko.inbox = inbox();
    s.neko.loaded = true;
    s.neko.digest = Some(digest());
    s.unseen.insert("t2".into());
    s.unseen.insert("t3".into());
    panel_closed(s);
}

/// A session being built, with its goal, runs and a queue of waiting messages.
pub(crate) fn running(s: &mut App) {
    s.apply_link(LinkEvent::Live(demo_full("t6")));
    s.set_web(Some(WebTarget::new(4747, &"0".repeat(64))));
    s.selected = Some("t6".into());
    s.neko.inbox = inbox();
    s.neko.loaded = true;
    s.neko.digest = None;
    s.work.checkpoints.insert("t6".into(), runs());
    panel_closed(s);
}

fn with_diff(s: &mut App, run: Option<u32>) {
    running(s);
    let key = diff_key("t6", run);
    s.work.diffs.insert(key.clone(), DiffState::new(rune_kit::diff::parse(PATCH)));
    s.work.diff_load.insert(key, DiffLoad::Ready { truncated: false });
    s.work.open_run = Some(run.map_or(RunKey::Whole, RunKey::Run));
}

/// A long markdown reply, `words` words: paragraphs, a list, a code block and a table, repeated.
pub fn long_reply(words: usize) -> Vec<String> {
    let block = "## Where things stand\n\nThe sync job **retries** a failed request up to three times, waiting a little longer each time, and then reports the failure. \n\n- the first retry waits `200 ms`\n- the second waits `800 ms`\n- a third failure is reported\n\n```rust\nfn backoff(n: u32) -> Duration { Duration::from_millis(200 * 4u64.pow(n)) }\n```\n\n| attempt | wait |\n|---|---|\n| 1 | 200 ms |\n| 2 | 800 ms |\n\n";
    let mut out: Vec<String> = Vec::new();
    while out.len() < words {
        out.extend(block.split_inclusive(' ').map(str::to_owned));
    }
    out.truncate(words);
    out
}

/// A reply of `words` words, streaming at 60 a second, in Neko's column.
fn stream(s: &mut App) {
    live(s);
    if let Some(summary) = s.summary.as_mut() {
        summary.conversation.truncate(3);
        summary.conversation.push(ChatMessage { id: "m4".into(), at_ms: DEMO_NOW - 15_000, role: ChatRole::Neko, pending: true, parts: vec![], ..blank() });
    }
    s.stream = Some(crate::app::Stream { words: long_reply(4000), per_sec: 60.0, started: None, shown: 0 });
}

/// Ten finished replies of about 400 words each in Neko's column, and a long session: what a day
/// of use leaves on screen.
fn ten_turns(s: &mut App) {
    live(s);
    if let Some(summary) = s.summary.as_mut() {
        summary.conversation.clear();
        for i in 0..10 {
            let body: String = long_reply(400).concat();
            summary.conversation.push(ChatMessage { id: format!("u{i}"), at_ms: DEMO_NOW - 100_000 + i, role: ChatRole::User, parts: ChatMessage::text_parts(&format!("question number {i}")), ..blank() });
            summary.conversation.push(ChatMessage { id: format!("a{i}"), at_ms: DEMO_NOW - 90_000 + i, role: ChatRole::Neko, parts: ChatMessage::text_parts(&body), ..blank() });
        }
    }
}

/// A summary printed by `neko-rune probe json`, from the file named by `NEKO_RUNE_SUMMARY`.
fn harness(s: &mut App) {
    let loaded = std::env::var("NEKO_RUNE_SUMMARY").ok().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice::<Summary>(&b).ok());
    match loaded {
        Some(summary) => {
            // The daemon's own times, so "5m" counts from the newest change in it.
            let newest = summary.tasks.iter().map(|t| t.updated_at_ms).max().unwrap_or(DEMO_NOW);
            s.apply_link(LinkEvent::Live(summary));
            s.now_ms = newest + 60_000;
            // The first session, against this clock.
            s.open_first_session();
        }
        None => live(s),
    }
    s.set_web(Some(WebTarget::new(4747, &"0".repeat(64))));
}

/// A scene's name and what it does to the starting state.
/// The workbench the page scenes open over.
pub fn live_for_pages(s: &mut App) {
    live(s);
}

pub type Scene = (&'static str, fn(&mut App));

pub const SCENES: &[Scene] = &[
    ("default", |_| {}),
    // Home over a live workbench: the greeting, the status line, the composer and the art (polish spec §2, §3).
    ("home", |s| {
        live(s);
        s.account.name = "Ada Lovelace".into();
        s.open_new();
    }),
    // Home's glass in its other states: opaque (Reduce Transparency), a request typed (the words' contrast over the glass), and the card's
    // backing left out so a capture shows the raw art under it (a test probe, `tests/glass.rs`).
    ("home-opaque", |s| {
        live(s);
        s.account.name = "Ada Lovelace".into();
        s.open_new();
        s.opaque_glass = true;
    }),
    ("home-typed", |s| {
        live(s);
        s.account.name = "Ada Lovelace".into();
        s.open_new();
        s.new_request.replace_text("Add a retry to the sync job and cover it with a test".to_owned());
    }),
    ("home-art-probe", |s| {
        live(s);
        s.account.name = "Ada Lovelace".into();
        s.open_new();
        s.glass_probe = true;
    }),
    // The sessions column folded: a session's header carries "Show sidebar" after the traffic lights, and Home floats the same button over the art.
    ("sessions-collapsed", |s| {
        live(s);
        s.layout.sessions.set_open(false);
    }),
    ("home-sessions-collapsed", |s| {
        live(s);
        s.account.name = "Ada Lovelace".into();
        s.open_new();
        s.layout.sessions.set_open(false);
    }),
    // The Neko column folded to its 44 point strip: the spark at the top, the count of what needs you under it (docs/knowledge/rune-identity.md).
    ("neko-strip", |s| {
        live(s);
        s.layout.set_neko_open(false);
    }),
    // Neko's column with the Activity line open: the daemon's own notices folded out of the chat (polish spec §0).
    ("activity", |s| {
        live(s);
        if let Some(m) = s.summary.as_mut() {
            for (i, text) in ["Neko updated itself to 3df08dcd (UI live).", "Reinstalling Neko at 3df08dcd now that it is idle.", "Neko updated itself to 3df08dcd (app reinstalled)."].into_iter().enumerate() {
                m.conversation.push(ChatMessage { id: format!("notice-{i}"), at_ms: DEMO_NOW - 5_000 + i as i64, role: ChatRole::Neko, parts: ChatMessage::text_parts(text), ..blank() });
            }
        }
        s.activity_open = true;
    }),
    ("settings-narrow", crate::scenes_pages::settings_narrow),
    ("ba-review", |s| crate::scenes_batch_a::run("ba-review", s)),
    ("ba-model", |s| crate::scenes_batch_a::run("ba-model", s)),
    ("ba-menu", |s| crate::scenes_batch_a::run("ba-menu", s)),
    ("ba-delete", |s| crate::scenes_batch_a::run("ba-delete", s)),
    ("ba-stop-all", |s| crate::scenes_batch_a::run("ba-stop-all", s)),
    ("ba-all-sessions", |s| crate::scenes_batch_a::run("ba-all-sessions", s)),
    ("ba-slash", |s| crate::scenes_batch_a::run("ba-slash", s)),
    ("ba-mention", |s| crate::scenes_batch_a::run("ba-mention", s)),
    ("ba-chips", |s| crate::scenes_batch_a::run("ba-chips", s)),
    ("ba-drop", |s| crate::scenes_batch_a::run("ba-drop", s)),
    ("ba-queue", |s| crate::scenes_batch_a::run("ba-queue", s)),
    ("ba-palette", |s| crate::scenes_batch_a::run("ba-palette", s)),
    ("ba-palette-typed", |s| crate::scenes_batch_a::run("ba-palette-typed", s)),
    ("ba-shortcuts", |s| crate::scenes_batch_a::run("ba-shortcuts", s)),
    ("ba-text-menu", |s| crate::scenes_batch_a::run("ba-text-menu", s)),
    ("ba-scope", |s| crate::scenes_batch_a::run("ba-scope", s)),
    ("ba-inbox", |s| crate::scenes_batch_a::run("ba-inbox", s)),
    ("ba-cadence", |s| crate::scenes_batch_a::run("ba-cadence", s)),
    ("ba-cadence-daily", |s| crate::scenes_batch_a::run("ba-cadence-daily", s)),
    ("ba-month", |s| crate::scenes_batch_a::run("ba-month", s)),
    ("ba-event", |s| crate::scenes_batch_a::run("ba-event", s)),
    ("ba-toasts", |s| crate::scenes_batch_a::run("ba-toasts", s)),
    ("routines", crate::scenes_pages::routines),
    ("routines-open", crate::scenes_pages::routines_open),
    ("calendar", crate::scenes_pages::calendar),
    ("calendar-day", crate::scenes_pages::calendar_day),
    ("calendar-event", crate::scenes_pages::calendar_event),
    ("settings", crate::scenes_pages::settings),
    ("settings-ai", crate::scenes_pages::settings_ai),
    ("settings-budget", crate::scenes_pages::settings_budget),
    ("settings-phone", crate::scenes_pages::settings_phone),
    ("settings-updates", crate::scenes_pages::settings_updates),
    ("settings-search", crate::scenes_pages::settings_search),
    ("usage", crate::scenes_pages::usage),
    ("bb-board", crate::scenes_batch_b::board),
    ("bb-board-drag", crate::scenes_batch_b::board_drag),
    ("bb-board-refused", crate::scenes_batch_b::board_refused),
    ("bb-tree", crate::scenes_batch_b::tree),
    ("bb-tree-folded", crate::scenes_batch_b::tree_folded),
    ("bb-prs", crate::scenes_batch_b::prs),
    ("bb-prs-close", crate::scenes_batch_b::prs_close),
    ("bb-prs-merge-all", crate::scenes_batch_b::prs_merge_all),
    ("bb-prs-menu", crate::scenes_batch_b::prs_menu),
    ("bb-details", crate::scenes_batch_b::details),
    ("bb-details-propose", crate::scenes_batch_b::details_proposable),
    ("bb-slash", crate::scenes_batch_b::slash),
    ("setup-improvements", crate::scenes_setup::improvements),
    ("setup-improvements-off", crate::scenes_setup::improvements_off),
    ("setup-improvements-empty", crate::scenes_setup::improvements_empty),
    ("setup-improvement-detail", crate::scenes_setup::improvement_detail),
    ("setup-improvement-detail-pack", crate::scenes_setup::improvement_detail_pack),
    ("setup-packs", crate::scenes_setup::packs),
    ("setup-packs-empty", crate::scenes_setup::packs_empty),
    ("setup-pack-detail", crate::scenes_setup::pack_detail),
    ("setup-pack-add", crate::scenes_setup::pack_add),
    ("setup-pack-preview", crate::scenes_setup::pack_preview),
    ("setup-pack-remove", crate::scenes_setup::pack_remove),
    ("setup-pack-update", crate::scenes_setup::pack_update),
    ("setup-pack-changed", crate::scenes_setup::pack_changed),
    ("setup-workspaces", crate::scenes_setup::workspaces),
    ("setup-workspaces-open", crate::scenes_setup::workspaces_open),
    ("setup-workspace-dialog", crate::scenes_setup::workspace_dialog),
    ("setup-watching", crate::scenes_setup::watching),
    ("setup-watch-dialog", crate::scenes_setup::watch_dialog),
    ("setup-tools", crate::scenes_setup::tools),
    ("setup-tool-detail", crate::scenes_setup::tool_detail),
    ("setup-tool-signin", crate::scenes_setup::tool_signin),
    ("setup-add-connection", crate::scenes_setup::add_connection),
    ("setup-skills", crate::scenes_setup::skills),
    ("setup-memory", crate::scenes_setup::memory),
    ("setup-profiles", crate::scenes_setup::profiles),
    ("setup-schedules", crate::scenes_setup::schedules),
    ("setup-confirm", crate::scenes_setup::confirm),
    ("setup-counted", crate::scenes_setup::counted),
    ("plugins", crate::scenes_pages::plugins),
    ("plugins-none", crate::scenes_pages::plugins_none),
    // Luau plugins: the example pack's card and view over made-up data, a card that stopped, more cards than fit, the status rows.
    ("luau-card", crate::scenes_luau::card),
    ("luau-cards-more", crate::scenes_luau::cards_more),
    ("luau-stopped", crate::scenes_luau::stopped),
    ("luau-view", crate::scenes_luau::view),
    ("luau-status", crate::scenes_luau::status),
    ("luau-pack", crate::scenes_luau::pack),
    ("bd-appearance", crate::scenes_batch_d::appearance),
    ("bd-general", crate::scenes_batch_d::general),
    ("bd-general-recording", crate::scenes_batch_d::general_recording),
    ("bd-ai", crate::scenes_batch_d::ai),
    ("bd-ai-failed", crate::scenes_batch_d::ai_failed),
    ("bd-start-over", crate::scenes_batch_d::start_over),
    ("bd-rulebook", crate::scenes_batch_d::rulebook),
    ("bd-rulebook-guard", crate::scenes_batch_d::rulebook_guard),
    ("bd-quick-panel", crate::scenes_batch_d::quick_panel),
    ("bd-system", crate::scenes_batch_d::system),
    ("bd-updates", crate::scenes_batch_d::updates),
    ("bd-roll-back", crate::scenes_batch_d::roll_back),
    ("bd-remove-theme", crate::scenes_batch_d::remove_theme),
    ("bd-make-theme", crate::scenes_batch_d::make_theme),
    ("bd-usage", crate::scenes_batch_d::usage),
    ("bd-usage-hold", crate::scenes_batch_d::usage_hold),
    ("bd-onboarding-welcome", crate::scenes_batch_d::onboarding_welcome),
    ("bd-onboarding-agents", crate::scenes_batch_d::onboarding_agents),
    ("bd-onboarding-workspaces", crate::scenes_batch_d::onboarding_workspaces),
    ("bd-onboarding-routines", crate::scenes_batch_d::onboarding_routines),
    ("bd-onboarding-mac", crate::scenes_batch_d::onboarding_mac),
    ("bd-onboarding-done", crate::scenes_batch_d::onboarding_done),
    ("bd-plugins-list", crate::scenes_batch_d::plugins_list),
    ("bd-plugin-details", crate::scenes_batch_d::plugin_details),
    ("bd-plugin-remove", crate::scenes_batch_d::plugin_remove),
    ("bd-change-neko", crate::scenes_batch_d::change_neko),
    ("bd-themed", crate::scenes_batch_d::themed),
    ("p3a-auto-canvas", crate::scenes_panel::auto_canvas),
    ("p3a-auto-diff", crate::scenes_panel::auto_diff),
    ("p3a-auto-closed", crate::scenes_panel::auto_closed),
    ("p3a-diff", crate::scenes_panel::diff),
    ("p3a-files", crate::scenes_panel::files),
    ("p3a-file-open", crate::scenes_panel::file_open),
    ("p3a-terminal", crate::scenes_panel::terminal),
    ("panel-browser", crate::scenes_panel::browser),
    ("panel-browser-none", crate::scenes_panel::browser_none),
    ("p3a-canvas", crate::scenes_panel::canvas_image),
    ("p3a-gif", crate::scenes_panel::image_gif),
    ("p3a-svg", crate::scenes_panel::image_svg),
    ("p3a-markdown", crate::scenes_panel::markdown),
    ("p3a-code", crate::scenes_panel::code),
    ("p3a-table", crate::scenes_panel::table),
    ("p3a-pdf", crate::scenes_panel::pdf),
    ("p3a-model3d", crate::scenes_panel::model),
    ("p3a-model3d-solid", crate::scenes_panel::model_solid),
    ("p3a-model3d-48k", crate::scenes_panel::model_48k),
    ("p3a-web", crate::scenes_panel::web),
    ("p3b-web-menu-hidden", crate::scenes_panel::web_menu_open),
    ("p3b-plugin-more", crate::scenes_panel::plugin_more),
    ("p3b-plugin-page", crate::scenes_panel::plugin_page),
    ("p3b-pressure-icons", crate::scenes_panel::pressure_icons),
    ("p3a-fallback", crate::scenes_panel::fallback),
    ("p3a-too-large", crate::scenes_panel::too_large),
    ("p3a-expanded", crate::scenes_panel::expanded),
    ("live", live),
    ("harness", harness),
    ("offline", |s| {
        s.apply_link(down(LinkError::NotRunning, 4));
    }),
    ("empty", |s| {
        s.apply_link(LinkEvent::Live(Summary { digest: "empty".into(), ..Summary::default() }));
        s.neko.loaded = true;
    }),
    ("context", |s| {
        live(s);
        s.layout.context.set_open(true);
    }),
    ("collapsed", |s| {
        live(s);
        s.layout.set_neko_open(false);
        s.collapsed = [false, true, false, true];
    }),
    ("steer", |s| {
        running(s);
        s.steer_open = true;
    }),
    ("more", |s| {
        live(s);
        s.more_open = true;
    }),
    ("full-view", |s| {
        live(s);
        s.open_view(FullView::Settings);
    }),
    ("notice", |s| {
        s.open_view(FullView::Routines);
        s.open_in_web(Some(FullView::Routines.web_id()));
    }),
    ("lab", |s| {
        live(s);
        s.lab = true;
    }),
    ("running", running),
    ("stream", stream),
    ("ten-turns", ten_turns),
    ("tool-open", |s| {
        running(s);
        let tool = s.detail().and_then(|d| crate::turns::task_turns(&d.task).iter().flat_map(|t| t.tools().cloned().collect::<Vec<_>>()).nth(1)).expect("a tool card");
        s.toggle_tool(&tool);
    }),
    ("goal-open", |s| {
        running(s);
        s.toggle("goal:g1");
    }),
    ("run-diff", |s| with_diff(s, Some(2))),
    ("undo-confirm", |s| {
        with_diff(s, Some(2));
        s.ask_undo("t6", crate::work::UndoOptions { run: 2, ..Default::default() });
    }),
    ("undo-refused", |s| {
        running(s);
        s.work.refusal = crate::work::parse_refusal("checkpoint:later_runs: Run 3 came after run 2 and has not been undone.", crate::work::UndoOptions { run: 2, ..Default::default() });
    }),
    ("queued", |s| {
        running(s);
        s.composer.replace_text("Also check the dark theme");
    }),
    ("new", |s| {
        live(s);
        s.open_new();
        s.new_request.replace_text("Make the sync job retry when a request fails");
        s.outbox.clear();
    }),
    ("palette", |s| {
        live(s);
        s.palette_toggle();
        s.palette_query.replace_text("board");
    }),
    ("menu", |s| {
        live(s);
        s.row_menu = Some(("t6".into(), rune::PointerPress { x: 130.0, y: 330.0 }));
    }),
    ("answer", |s| {
        live(s);
        let mut m = demo_full("t0");
        if let Some(g) = m.goals.first_mut() {
            g.status = GoalStatus::Blocked;
            g.blocked = Some(Blocked { question: "Should the retry also cover timeouts?".into(), default_answer: "Yes, three tries".into(), inbox_seq: None, ..Default::default() });
        }
        s.apply_link(LinkEvent::Live(m));
        s.open_task("t6");
        s.apply_link(LinkEvent::Live({
            let mut m = demo_full("t6");
            if let Some(g) = m.goals.first_mut() {
                g.status = GoalStatus::Blocked;
                g.blocked = Some(Blocked { question: "Should the retry also cover timeouts?".into(), default_answer: "Yes, three tries".into(), inbox_seq: None, ..Default::default() });
            }
            m
        }));
    }),
];

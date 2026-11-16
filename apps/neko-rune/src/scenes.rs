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
use crate::link::{Detail, InboxItem, LinkError, LinkEvent, Summary, TaskRow, WorkspaceRow};
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
    let goals = ["tests pass · turn 4/20", "the board shows a message when it is empty", "one section per file, same tests", "old name still works for a release", "refresh at most once a minute"];
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
    t.goal = format!("{}.\n{}", row.title, row.goal);
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
    Detail { task: task(row, events, plan, ""), has_folder: false, workspace: "Neko".into() }
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
    Detail { task: task(row, events, "", ""), has_folder: true, workspace: "Neko".into() }
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
        InboxItem { seq: 3, title: "Reply before the 3pm call".into(), detail: "Sam is waiting on the budget numbers.".into(), default: Some("Send the numbers from last week".into()), link: None, urgent: false, from: "Messages".into(), at_ms: DEMO_NOW - 600_000 },
        InboxItem { seq: 2, title: "Extend by 10 turns?".into(), detail: "Paused: it used its 20 turns.".into(), default: Some("Extend by 10 turns".into()), link: None, urgent: false, from: "Goals".into(), at_ms: DEMO_NOW - 3_600_000 },
        InboxItem { seq: 1, title: "Enable the calendar API".into(), detail: "The calendar server needs the API enabled for its project.".into(), default: None, link: Some("https://example.com/enable".into()), urgent: false, from: "Calendar".into(), at_ms: DEMO_NOW - 7_200_000 },
    ]
}

fn digest() -> Digest {
    let line = |text: &str, default: Option<&str>| DigestLine { text: text.into(), default: default.map(Into::into) };
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

fn live(s: &mut App) {
    s.apply_link(LinkEvent::Live(demo_full("t0")));
    s.set_web(Some(WebTarget::new(4747, &"0".repeat(64))));
    s.neko.inbox = inbox();
    s.neko.loaded = true;
    s.neko.digest = Some(digest());
    s.unseen.insert("t2".into());
    s.unseen.insert("t3".into());
}

/// A session being built, with its goal, runs and a queue of waiting messages.
fn running(s: &mut App) {
    s.apply_link(LinkEvent::Live(demo_full("t6")));
    s.set_web(Some(WebTarget::new(4747, &"0".repeat(64))));
    s.selected = Some("t6".into());
    s.neko.inbox = inbox();
    s.neko.loaded = true;
    s.neko.digest = None;
    s.work.checkpoints.insert("t6".into(), runs());
}

fn with_diff(s: &mut App, run: Option<u32>) {
    running(s);
    let key = diff_key("t6", run);
    s.work.diffs.insert(key.clone(), DiffState::new(rune_kit::diff::parse(PATCH)));
    s.work.diff_load.insert(key, DiffLoad::Ready { truncated: false });
    s.work.open_run = Some(run.map_or(RunKey::Whole, RunKey::Run));
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
            // Selection was chosen against the demo clock; choose again against this one.
            s.selected = s.summary.as_ref().and_then(|m| crate::groups::first_session(m, s.now_ms));
        }
        None => live(s),
    }
    s.set_web(Some(WebTarget::new(4747, &"0".repeat(64))));
}

/// A scene's name and what it does to the starting state.
pub type Scene = (&'static str, fn(&mut App));

pub const SCENES: &[Scene] = &[
    ("default", |_| {}),
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
        s.context = crate::app::ContextKind::Browser;
    }),
    ("collapsed", |s| {
        live(s);
        s.layout.neko.set_open(false);
        s.collapsed = [false, true, false];
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
            g.blocked = Some(Blocked { question: "Should the retry also cover timeouts?".into(), default_answer: "Yes, three tries".into(), inbox_seq: None });
        }
        s.apply_link(LinkEvent::Live(m));
        s.open_task("t6");
        s.apply_link(LinkEvent::Live({
            let mut m = demo_full("t6");
            if let Some(g) = m.goals.first_mut() {
                g.status = GoalStatus::Blocked;
                g.blocked = Some(Blocked { question: "Should the retry also cover timeouts?".into(), default_answer: "Yes, three tries".into(), inbox_seq: None });
            }
            m
        }));
    }),
];

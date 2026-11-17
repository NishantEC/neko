//! Scenes for parity batch B (the owner's brief of 2026-11-17): the work views. The Board with a card carried (allowed and
//! refused), the Tree, Pull requests (the list, "Close…", "Merge all ready"), the Details tab and the `/` popover in the session's
//! reply box. Synthetic data only, like every committed scene; the pictures are `docs/design/rune/batch-b-*.png`.

use neko_protocol::decision_context::DecisionAction;
use neko_protocol::workbench::{SupervisorDecision, TaskStatus};

use crate::all_sessions::Stage;
use crate::app::{App, DEMO_NOW};
use crate::board::{Carry, Layout};
use crate::link::{Checks, ChildView, DecisionRow, PrRow, SplitView, Summary, TaskMeta, Verdict};
use crate::panel::{Section, Tab};
use crate::scenes::{demo_full, live_scene};
use crate::views::FullView;

const MIN: i64 = 60_000;

/// Distinct titles for the first demo tasks, so a tree and a board read as different pieces of work.
const TITLES: [&str; 22] = [
    "Add a retry to the sync job",
    "Validate phone numbers on the profile form",
    "Fix the date picker in Safari",
    "Show the empty state on the orders page",
    "Cache the settings lookup",
    "Stop the nightly export running twice",
    "Rework the export batching",
    "Paginate the audit log endpoint",
    "Rename the billing feature flag",
    "Split the reports module",
    "Make the order list sortable by date",
    "Guard the missing client in the reports job",
    "Cover the reports job with a test",
    "Document the reports job’s retries",
    "Drop the unused logger import",
    "Handle a missing customer in the invoice job",
    "Explain the 502s on the health check",
    "Upgrade the HTTP client",
    "Log slow database queries",
    "Trim whitespace in signup emails",
    "Add dark mode to the email preview",
    "Move the uploads bucket to the new region",
];

/// The demo workbench with distinct titles, each task in a workspace, and who started whom.
fn with_work(m: &mut Summary) {
    for (i, t) in m.tasks.iter_mut().enumerate() {
        if let Some(title) = TITLES.get(i) {
            t.title = (*title).to_owned();
        }
        let workspace = if i % 3 == 0 { "w2" } else { "w1" };
        m.meta.insert(t.id.clone(), TaskMeta { workspace_id: workspace.into(), ..TaskMeta::default() });
    }
    // The export rework split into parts; one of those started a follow-up; a review found a fix elsewhere.
    for (child, parent) in [("t9", "t6"), ("t11", "t6"), ("t12", "t6"), ("t13", "t11"), ("t4", "t3")] {
        m.parents.insert(child.to_owned(), parent.to_owned());
    }
}

fn live(s: &mut App) {
    live_scene(s);
    if let Some(m) = s.summary.as_mut() {
        with_work(m);
    }
}

fn pr(id: &str, number: u32, repo: &str, verdict: Verdict, checks: Checks, ago: i64, author: &str) -> PrRow {
    PrRow {
        id: id.into(),
        title: TITLES[id.trim_start_matches('t').parse::<usize>().unwrap_or(0).min(TITLES.len() - 1)].into(),
        url: format!("https://github.com/acme/{repo}/pull/{number}"),
        number: format!("#{number}"),
        repository: repo.into(),
        repository_path: format!("/work/{repo}"),
        verdict,
        reason: None,
        checks,
        review: String::new(),
        conflicts: false,
        behind: false,
        draft: false,
        updated_ms: DEMO_NOW - ago * MIN,
        author: author.into(),
        merging: verdict == Verdict::Merging,
        follow_ups: 0,
    }
}

fn pull_requests() -> Vec<PrRow> {
    let mut rows = vec![
        pr("t2", 101, "acme-api", Verdict::Ready, Checks::Passing, 25, "Claude"),
        pr("t3", 103, "acme-api", Verdict::Ready, Checks::Passing, 70, "Codex"),
        pr("t4", 57, "acme-web", Verdict::Close, Checks::Failing(vec!["Lint".into(), "Unit".into()]), 190, "Claude"),
        pr("t5", 58, "acme-web", Verdict::NeedsYou, Checks::Failing(vec!["Browser".into()]), 320, "Codex"),
        pr("t7", 104, "acme-api", Verdict::Merging, Checks::Pending, 45, "Claude"),
        pr("t8", 105, "acme-api", Verdict::Working, Checks::Pending, 12, "Claude"),
        pr("t15", 95, "acme-api", Verdict::Merged, Checks::Passing, 2900, "Claude"),
        pr("t16", 51, "acme-web", Verdict::Merged, Checks::Passing, 3100, "Codex"),
    ];
    rows[2].reason = Some("A duplicate of #52, which already fixed the picker.".into());
    rows[2].conflicts = true;
    rows[3].reason = Some("It waits on your answer.".into());
    rows[0].review = "REVIEW_REQUIRED".into();
    rows
}

fn with_prs(s: &mut App) {
    live(s);
    if let Some(m) = s.summary.as_mut() {
        m.pull_requests = pull_requests();
        m.merge_without_review = vec!["/work/acme-web".into()];
    }
}

// ------------------------------------------------------------------------------------------------ the board and the tree

fn all_sessions(s: &mut App, layout: Layout) {
    live(s);
    s.open_all_sessions();
    s.all.include_stopped = true;
    s.set_all_layout(layout);
    s.all.layout_dirty = false;
}

pub fn board(s: &mut App) {
    all_sessions(s, Layout::Board);
    s.all.cursor_shown = true;
    s.all.cursor = 2;
}

/// A stopped task carried over Running: the column lights up and says what the drop does (Start work).
pub fn board_drag(s: &mut App) {
    all_sessions(s, Layout::Board);
    s.all.carry = Some(Carry { id: "t5".into(), from: Stage::NeedsYou, over: Stage::Running, grab: (40.0, 18.0), at: (960.0, 250.0), moved: true });
}

/// A running task carried over Needs you: the web does not allow it, so the column does not light up and the cursor says so.
pub fn board_refused(s: &mut App) {
    all_sessions(s, Layout::Board);
    s.all.carry = Some(Carry { id: "t6".into(), from: Stage::Running, over: Stage::NeedsYou, grab: (40.0, 18.0), at: (360.0, 330.0), moved: true });
}

pub fn tree(s: &mut App) {
    all_sessions(s, Layout::Tree);
}

/// The tree with one parent folded.
pub fn tree_folded(s: &mut App) {
    all_sessions(s, Layout::Tree);
    s.tree_toggle("t6");
}

// ------------------------------------------------------------------------------------------------ pull requests

pub fn prs(s: &mut App) {
    with_prs(s);
    s.open_view(FullView::PullRequests);
}

pub fn prs_close(s: &mut App) {
    prs(s);
    s.ask_close_pr("t4");
}

pub fn prs_merge_all(s: &mut App) {
    prs(s);
    s.ask_merge_all();
}

pub fn prs_menu(s: &mut App) {
    prs(s);
    s.prs.menu_open = true;
}

// ------------------------------------------------------------------------------------------------ the details tab and the slash

fn risk() -> SupervisorDecision {
    serde_json::from_value(serde_json::json!({
        "action": "prepare_fix", "risk": "medium", "is_bug": true,
        "reason": "The sync job ends the whole batch on one failed request. The change is small, but it touches the retry loop every job shares.",
        "evidence": ["job.rs:42 returns on the first error", "Two incidents last week were this"],
        "files": ["crates/sync/src/job.rs", "crates/sync/src/job_tests.rs"],
        "tests": ["cargo test -p sync"],
        "sensitive_areas": ["The retry loop is shared with the export job"],
        "uncertainties": ["Whether three attempts is enough for the slow region"],
        "plan": "1. Retry up to three times.",
    }))
    .expect("a risk")
}

pub fn details(s: &mut App) {
    live(s);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/you".into());
    if let Some(m) = s.summary.as_mut() {
        let mut d = demo_full("t0").detail.expect("a detail");
        d.task.supervision = Some(risk());
        d.task.worktree = Some(format!("{home}/Documents/acme-api/.worktrees/neko-sync-retry-4f2a"));
        d.can_split = false;
        d.splits = vec![SplitView {
            approved: true,
            integrated: false,
            children: vec![
                ChildView { title: "Retry the request".into(), goal: "Wrap the send in a bounded back-off.".into(), task: Some(("t11".into(), TaskStatus::Queued)) },
                ChildView { title: "Cover the third attempt".into(), goal: "Add a test that fails twice and then succeeds.".into(), task: Some(("t12".into(), TaskStatus::Building)) },
            ],
        }];
        d.decisions = vec![
            DecisionRow { at_ms: DEMO_NOW - 40 * MIN, title: crate::link::decision_title(DecisionAction::ApproveLocalBuild).into(), outcome: Some("The build started in the task’s working copy.".into()), accepted_locally: false, correction: None },
            DecisionRow { at_ms: DEMO_NOW - 95 * MIN, title: crate::link::decision_title(DecisionAction::PrepareFix).into(), outcome: Some("A plan with three steps was ready for review.".into()), accepted_locally: true, correction: Some("Keep the public client unchanged.".into()) },
        ];
        // A longer log, newest last in the data, so the tab shows newest first.
        let base = d.task.events.last().map_or(DEMO_NOW, |e| e.at_ms);
        for (i, (role, text)) in [("system", "Plan approved"), ("builder", "Reading the sync job first."), ("builder", "Added the back-off and a test."), ("reviewer", "Checked the change independently.")].into_iter().enumerate() {
            d.task.events.push(neko_protocol::workbench::TaskEvent { at_ms: base + (i as i64 + 1) * 4 * MIN, role: role.into(), message: text.into() });
        }
        m.detail = Some(d);
    }
    s.selected = Some("t0".into());
    s.panel_choose_tab(Tab::Details);
    s.panel_set_open(true);
    s.panel.toggled.insert(Section::Log);
}

pub fn details_proposable(s: &mut App) {
    live(s);
    if let Some(d) = s.summary.as_mut().and_then(|m| m.detail.as_mut()) {
        d.can_split = true;
        d.task.worktree = None;
    }
    s.selected = Some("t0".into());
    s.panel_choose_tab(Tab::Details);
    s.panel_set_open(true);
}

/// `/` typed in the session's reply box: the web's commands above it.
pub fn slash(s: &mut App) {
    live(s);
    s.composer.replace_text("/");
    s.composer.focused = true;
}

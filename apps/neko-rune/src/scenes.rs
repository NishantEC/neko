//! Named states for headless snapshots (`--snap out.png --scene live`). Each modifies the
//! initial (headless) state, so a scene is also a test fixture.
//!
//! The `harness` scene is the exception that proves the link: it reads the workbench summary that
//! `neko-rune probe json` printed from a test daemon (`scripts/snap-harness.sh`), so the rows in
//! that snapshot are what a real daemon sent, not made up here.

use neko_protocol::workbench::TaskStatus;

use crate::app::{App, DEMO_NOW, down};
use crate::link::{LinkError, LinkEvent, Summary, TaskRow};
use crate::views::FullView;
use crate::web_link::WebTarget;

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
    Summary { digest: "demo".into(), runtime: "Codex · default model".into(), tasks }
}

fn live(s: &mut App) {
    s.apply_link(LinkEvent::Live(demo_summary()));
    s.set_web(Some(WebTarget::new(4747, &"0".repeat(64))));
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
        s.apply_link(LinkEvent::Live(Summary { digest: "empty".into(), runtime: String::new(), tasks: Vec::new() }));
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
        live(s);
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
];

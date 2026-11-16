//! Named states for headless snapshots (`--snap out.png --scene live`). Each modifies the
//! initial (headless) state, so a scene is also a test fixture.

use neko_protocol::workbench::TaskStatus;

use crate::app::{App, down};
use crate::link::{LinkError, LinkEvent, Summary, TaskRow};
use crate::pages::Page;
use crate::web_link::WebTarget;

/// A made-up workbench: a few of each status, titles that name nothing real.
pub fn demo_summary() -> Summary {
    use TaskStatus::*;
    let shape = [
        (AwaitingApproval, 2),
        (Building, 3),
        (Planning, 1),
        (Reviewing, 1),
        (ReadyForReview, 4),
        (Queued, 5),
        (Completed, 8),
        (Failed, 1),
    ];
    let titles = ["Add a retry to the sync job", "Fix the empty state on the board", "Split the settings page", "Rename the export command", "Cache the repository list"];
    let mut tasks = Vec::new();
    for (status, n) in shape {
        for i in 0..n {
            let at = tasks.len();
            tasks.push(TaskRow { id: format!("t{at}"), key: Some(format!("NK-{}", 100 + at)), title: titles[(at + i) % titles.len()].to_owned(), status, updated_at_ms: 0 });
        }
    }
    Summary { digest: "demo".into(), tasks }
}

fn live(s: &mut App) {
    s.apply_link(LinkEvent::Live(demo_summary()));
    s.set_web(Some(WebTarget::new(4747, &"0".repeat(64))));
}

/// A scene's name and what it does to the starting state.
pub type Scene = (&'static str, fn(&mut App));

pub const SCENES: &[Scene] = &[
    ("default", |_| {}),
    ("live", live),
    ("offline", |s| {
        s.apply_link(down(LinkError::NotRunning, 4));
    }),
    ("tabs", |s| {
        live(s);
        for page in [Page::Inbox, Page::Tasks, Page::Calendar, Page::Usage] {
            s.open(page);
        }
        s.tabs.select(Page::Tasks);
    }),
    ("more", |s| {
        live(s);
        s.more_open = true;
    }),
    ("palette", |s| {
        live(s);
        s.open_palette();
    }),
    ("palette-query", |s| {
        live(s);
        s.open_palette();
        s.palette.query.text = "in".into();
    }),
    ("lab", |s| {
        live(s);
        s.lab = true;
    }),
    ("notice", |s| {
        s.open(Page::Inbox);
        s.open_in_web(Page::Inbox);
    }),
];

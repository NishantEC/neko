//! Scenes for Luau plugins (docs/knowledge/luau-plugins.md): the example pack's two panels over made-up data, a card that
//! stopped, more cards than the column shows, a view, and the status rows. Synthetic data only, like every committed scene; the
//! pictures are `docs/design/rune/luau-*.png` (`scripts/snap-luau.sh`). The example's own files are the ones drawn.

use neko_protocol::workbench::Snapshot;
use serde_json::json;

use crate::app::{App, DEMO_NOW};
use crate::link::luau::Wire;
use crate::luau::install_for_tests;
use crate::views::FullView;

/// The example pack's files, as the repository ships them.
pub const NEEDS_YOU: &str = include_str!("../../../packs/hello-luau/ui/needs-you.luau");
pub const ROUTINES: &str = include_str!("../../../packs/hello-luau/ui/routines.luau");

const HELLO: &str = "hello-luau";

/// A workbench of made-up sessions: four wait on the person, one runs, one is done.
pub fn demo_snapshot() -> Snapshot {
    let task = |id: &str, title: &str, status: &str, ago_min: i64| {
        json!({
            "id": id, "workspace_id": "w1", "issue_id": null, "title": title, "goal": "", "status": status, "plan": "", "result": "",
            "worktree": null, "events": [], "created_at_ms": DEMO_NOW - 3_600_000, "updated_at_ms": DEMO_NOW - ago_min * 60_000
        })
    };
    let tasks = [
        task("t1", "Fix the flaky login test", "AwaitingApproval", 4),
        task("t2", "Review the release notes", "ReadyForReview", 9),
        task("t3", "Approve the schema migration plan", "AwaitingApproval", 31),
        task("t4", "Add a retry to the export job", "Failed", 55),
        task("t5", "Tidy the changelog", "Building", 3),
        task("t6", "Update the onboarding copy", "Completed", 120),
    ];
    Snapshot { tasks: tasks.into_iter().map(|t| serde_json::from_value(t).expect("a made-up task")).collect(), ..Snapshot::default() }
}

/// The daemon as a scene or a test sees it: the made-up workbench and routines, the clock stopped at the demo's now.
pub fn demo_wire() -> Wire {
    Wire::demo(demo_snapshot(), crate::scenes_pages::routines_data(), DEMO_NOW)
}

/// A card with a static line: for the scenes that need more cards than the one the example has.
pub fn static_card(id: &str, title: &str, icon: &str, line: &str) -> String {
    format!("return {{ slot = \"card\", id = \"{id}\", title = \"{title}\", icon = \"{icon}\", render = function(state, host) local ui = require(\"@rune/ui\") return ui.text(\"{line}\", {{ muted = true }}) end }}")
}

/// A card whose render fails.
pub const BROKEN: &str = "return { slot = \"card\", id = \"weather\", title = \"Weather\", icon = \"clock\", render = function(state, host) local forecast = nil return forecast.today end }";

fn live(s: &mut App) {
    crate::scenes::live_for_pages(s);
    s.neko.digest = None;
    // The Inbox folded, so the column's top block shows the cards whole.
    s.toggle("inbox:closed");
}

/// The example's card in the Neko column, below the Digest.
pub fn card(s: &mut App) {
    live(s);
    install_for_tests(s, HELLO, &[("ui/needs-you.luau", NEEDS_YOU)], demo_wire());
}

/// Five cards: the column shows three and "and 2 more".
pub fn cards_more(s: &mut App) {
    live(s);
    let cards = [
        ("today", "Today", "calendar", "Two meetings and a review"),
        ("focus", "Focus", "clock", "Until 15:00"),
        ("inbox", "Inbox", "inbox", "Nothing waiting"),
        ("mood", "Mood", "unknown-icon", "Steady"),
        ("notes", "Notes", "files", "Three open"),
    ];
    let sources: Vec<(String, String)> = cards.iter().map(|(id, title, icon, line)| (format!("ui/{id}.luau"), static_card(id, title, icon, line))).collect();
    let files: Vec<(&str, &str)> = sources.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    install_for_tests(s, HELLO, &files, demo_wire());
}

/// A card that stopped: one quiet line and Reload, between two that run.
pub fn stopped(s: &mut App) {
    live(s);
    let ok = static_card("today", "Today", "calendar", "Two meetings and a review");
    install_for_tests(s, HELLO, &[("ui/needs-you.luau", NEEDS_YOU), ("ui/weather.luau", BROKEN), ("ui/today.luau", &ok)], demo_wire());
}

/// The example's view: a page of its own, with the filter box and the routines.
pub fn view(s: &mut App) {
    live(s);
    install_for_tests(s, HELLO, &[("ui/needs-you.luau", NEEDS_YOU), ("ui/routines.luau", ROUTINES)], demo_wire());
    if let Some(page) = s.luau.views().iter().find_map(|p| p.page()) {
        s.open_plugin_page(page);
    }
}

/// The Plugins page with the native plugins' status: loaded with a mint check, stopped with a coral cross and why.
pub fn status(s: &mut App) {
    crate::scenes_pages::plugins(s);
    install_for_tests(s, HELLO, &[("ui/needs-you.luau", NEEDS_YOU), ("ui/routines.luau", ROUTINES), ("ui/weather.luau", BROKEN)], demo_wire());
    s.open_view(FullView::Plugins);
}

/// A pack's page in Setup, with the same rows under "Native panels".
pub fn pack(s: &mut App) {
    crate::scenes_setup::pack_detail(s);
    install_for_tests(s, "demo-board", &[("layout.luau", &static_card("board", "Board", "files", "Three columns")), ("broken.luau", BROKEN)], demo_wire());
}

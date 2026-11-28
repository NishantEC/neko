//! Settings ▸ General: what Return does while the agent works, and how the sidebar lists threads.

use rune::{El, col};

use super::{Found, note, section};
use crate::state::App;
use crate::threads::Grouping;
use crate::ui::base::Theme;
use crate::ui::radio_group::RadioGroup;
use crate::ui::switch::Switch;

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    let mut p = col().w_full().gap(t.space.at(8.0));
    if let Some(sending) = crate::composer::settings_choice(s, t) {
        p = p.child(f.lit(t, "settings-sending", sending));
    }
    p.child(threads(s, t, f))
}

/// "Threads in the sidebar": the View menu's choices, kept here too, and the week after which a quiet thread settles.
fn threads(s: &App, t: &Theme, f: &Found) -> El {
    let options = [("By project", Some("One group per project, No project last.")), ("One list", Some("Every thread together, the newest activity first."))];
    let chosen = match s.grouping {
        Grouping::Project => 0,
        Grouping::None => 1,
    };
    let group = RadioGroup::<App>::new("settings-grouping", "Group threads", &options, Some(chosen))
        .on_change(|s: &mut App, i| s.set_grouping(if i == 1 { Grouping::None } else { Grouping::Project }))
        .build(t);
    let settled = s.show_settled;
    let archived = s.show_archived;
    let settle = Switch::<App>::new("settings-show-settled", "Show settled threads", settled).help("Open each group's Settled line. Folded, it still says how many.").on_toggle(move |s: &mut App| s.set_show_settled(!settled)).build(t);
    let archive = Switch::<App>::new("settings-show-archived", "Show archived threads", archived).help("Archived threads sleep out of every list until you restore one.").on_toggle(move |s: &mut App| s.set_show_archived(!archived)).build(t);
    let days = AUTO_SETTLE_DAYS;
    section(t, "settings-threads", "Threads in the sidebar", None)
        .child(super::h4(t, "Group threads"))
        .child(f.lit(t, "settings-grouping", group))
        .child(col().w_full().gap(t.space.at(1.0)).pt(t.space.at(2.0)).child(f.lit(t, "settings-show-settled", settle)).child(f.lit(t, "settings-show-archived", archive)))
        .child(f.lit(
            t,
            "settings-auto-settle",
            note(t, "settings-auto-settle", format!("A thread with nothing new for {days} days settles by itself, unless it is pinned, working or needs you. New activity brings it back.")),
        ))
}

/// The daemon's auto-settle week, in days (`neko_core::shelf::AUTO_SETTLE_AFTER_MS`: the app does not link the core, so the test
/// below reads it there).
pub const AUTO_SETTLE_DAYS: i64 = 7;

#[cfg(test)]
mod tests {
    #[test]
    fn the_week_is_the_daemons() {
        let core = include_str!("../../../../crates/neko-core/src/shelf.rs");
        assert!(core.contains(&format!("AUTO_SETTLE_AFTER_MS: i64 = {} * 24 * 3_600_000;", super::AUTO_SETTLE_DAYS)));
    }
}

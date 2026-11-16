//! "View all": every session, newest first, in one column of the same rows the sessions column draws. It is
//! where work older than a week that is not waiting on you lives (polish spec §0).

use rune::{El, ScrollSpec, col, text};

use super::{band, sessions};
use crate::app::App;
use crate::groups::{Group, all_sessions, group_of};
use crate::theme::Neko;

pub fn page(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let mut list = col().key("all-list").role("list").label("All sessions").focus_group().w_full().max_w(680.0).px(24.0).pb(24.0).gap(0.0);
    match s.summary.as_ref() {
        None => list = list.child(text("Neko isn't running yet.").font(t.ty.meta).color(t.text.muted).py(6.0)),
        Some(summary) => {
            for task in all_sessions(summary) {
                // The icon a row wears is the one of the group it would be in; work no group lists is history.
                let group = group_of(task, s.now_ms).unwrap_or(Group::Recent);
                list = list.child(sessions::session_row(s, task, group, t, layer));
            }
        }
    }
    col()
        .key("all-sessions")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(band(t).child(text("All sessions").key("all-title").font(t.ty.brand).color(t.text.base).role("heading")))
        .child(col().w_full().grow(1.0).min_h(0.0).items_center().scroll(ScrollSpec { stick_bottom: false }).child(list))
}

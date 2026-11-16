//! The sessions column: every task across workspaces, grouped by state (Needs you, Running, Done
//! today; each collapsible, with a count). A row is one line: status glyph, faint id, title,
//! quiet meta at the right (DESIGN.md §8 rule 3). "+ New" sits at the foot of the list; under a
//! quiet divider are Calendar, Routines and Settings, and "More" for the other full views.

use rune::{El, ScrollSpec, col, row, text};
use rune_kit::shell::SidebarHeader;
use rune_kit::{Menu, MenuItem, Placement};

use super::{band, nav_row};
use crate::app::{App, status_line};
use crate::groups::{age, grouped};
use crate::icons;
use crate::link::TaskRow;
use crate::status::{status_glyph, status_label, status_of};
use crate::theme::Neko;
use crate::views::FullView;

pub fn column(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    col().w_full().h_full().min_w(0.0).child(band(t)).child(list(s, t)).child(foot(s, t, layer))
}

fn list(s: &App, t: &Neko) -> El {
    let mut c = col().key("sessions-list").w_full().px(8.0).gap(2.0);
    match &s.summary {
        None => {
            // Before the first snapshot, and while Neko is not running: say which, calmly.
            c = c.child(text(status_line(&s.link, None)).key("sessions-status").font(t.ty.meta).color(t.text.muted).px(10.0).py(6.0).live_region(rune::LiveRegion::Polite));
        }
        Some(summary) => {
            let groups = grouped(summary, s.now_ms);
            if groups.iter().all(|(_, rows)| rows.is_empty()) {
                c = c.child(text("No sessions").key("sessions-empty").font(t.ty.meta).color(t.text.muted).px(10.0).py(6.0));
            }
            for (group, rows) in groups {
                let collapsed = s.collapsed[group.index()];
                // A group with nothing in it is not a heading with a zero (DESIGN.md §8 rule 11);
                // the three stay in the same order so the column does not jump.
                if rows.is_empty() {
                    continue;
                }
                c = c.child(SidebarHeader::new(group.title()).count(rows.len()).collapsed(collapsed).on_toggle(move |s: &mut App, _| s.toggle_group(group)).build(&t.kit).key(format!("group-{}", group.index())));
                if !collapsed {
                    for task in rows {
                        c = c.child(session_row(task, s.selected.as_deref() == Some(task.id.as_str()) && s.view.is_none(), s.now_ms, t));
                    }
                }
            }
        }
    }
    col().key("sessions-scroll").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c)
}

/// One session: glyph, faint id, title, quiet meta. A press selects it.
fn session_row(task: &TaskRow, selected: bool, now_ms: i64, t: &Neko) -> El {
    let id = task.id.clone();
    let mut r = row()
        .key(format!("session-{}", task.id))
        .role("button")
        .label(format!("{}, {}", task.title, status_label(task.status)))
        .checked(selected)
        .focusable(true)
        .w_full()
        .h(32.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if selected { t.surface.active } else { rune::Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.select(&id))
        .child(status_glyph(status_of(task.status), t, 14.0, Some("")));
    if let Some(key) = &task.key {
        r = r.child(text(key.clone()).font(t.ty.id).color(t.text.faint).a11y_hidden());
    }
    r.child(text(task.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(text(age(now_ms, task.updated_at_ms)).font(t.ty.id).color(t.text.faint).a11y_hidden())
}

fn foot(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let new = nav_row(&icons::PLUS, "New", "new-session", false, t).help("Start a session (in the web UI for now)").on_press(|s: &mut App| s.open_in_web(None));
    let divider = row().key("foot-divider").w_full().h(1.0).fill(t.surface.border).mt(6.0).mb(6.0);
    let mut c = col().key("sessions-foot").w_full().px(8.0).pb(10.0).child(new).child(divider);
    for view in FullView::FOOT {
        c = c.child(nav_row(view.icon(), view.title(), &format!("view-{}", view.web_id()), s.view == Some(view), t).on_press(move |s: &mut App| s.open_view(view)));
    }
    let more_open = s.more_open || FullView::MORE.contains(&s.view.unwrap_or(FullView::Calendar));
    let trigger = nav_row(&icons::MORE, "More", "more", more_open, t).on_press(|s: &mut App| s.more_open = !s.more_open);
    let mut menu = Menu::new("more-menu").trigger(trigger).open(s.more_open).placement(Placement::RightStart).on_close(|s: &mut App| s.more_open = false);
    for view in FullView::MORE {
        menu = menu.item(MenuItem::new(view.title()).icon(view.icon()).checked(s.view == Some(view)).on_select(move |s: &mut App| s.open_view(view)));
    }
    c.child(menu.build(&t.kit, layer))
}

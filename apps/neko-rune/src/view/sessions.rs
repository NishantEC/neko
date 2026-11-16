//! The sessions column: every task across workspaces, grouped by state (Needs you, Running, Done
//! today; each collapsible, with a count). A row is one line: status glyph, faint id, title,
//! quiet meta at the right: the state in words, then its age (the amber half circle is both
//! "working" and "needs you", as in the web UI, so the words say which; DESIGN.md §3). A dot
//! marks work that finished since you last opened it. Up and Down move between rows, Return
//! opens one, right-click offers Mark as unread, Stop and Open in web UI. "+ New" sits at the
//! foot of the list; under a quiet divider are Calendar, Routines and Settings, and "More".

use neko_protocol::workbench::TaskStatus;
use rune::{El, ScrollSpec, col, el, row, text};
use rune_kit::shell::SidebarHeader;
use rune_kit::{Menu, MenuItem, Placement, context_menu};

use super::{band, nav_row};
use crate::app::{App, status_line};
use crate::groups::{age, grouped, status_word};
use crate::icons;
use crate::link::TaskRow;
use crate::status::{status_glyph, status_label, status_of};
use crate::theme::Neko;
use crate::views::FullView;

pub fn column(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    col().w_full().h_full().min_w(0.0).child(band(t)).child(list(s, t, layer)).child(foot(s, t, layer))
}

fn list(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    // The rows are one focus group: Up and Down rove between them, Return opens the one in focus.
    let mut c = col().key("sessions-list").role("list").label("Sessions").focus_group().w_full().px(8.0).gap(2.0);
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
                        c = c.child(session_row(s, task, t, layer));
                    }
                }
            }
        }
    }
    col().key("sessions-scroll").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c)
}

fn running(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing)
}

/// One session: glyph, faint id, title, quiet meta. A press selects it; a right-press opens its menu.
fn session_row(s: &App, task: &TaskRow, t: &Neko, layer: &mut Vec<El>) -> El {
    let selected = s.selected.as_deref() == Some(task.id.as_str()) && s.view.is_none() && s.new_form.is_none();
    let unseen = s.unseen.contains(&task.id);
    let id = task.id.clone();
    let press_id = id.clone();
    let mut r = row()
        .key(format!("session-{}", task.id))
        .role("button")
        .label(format!("{}, {}, {}{}", task.title, status_word(task.status), age(s.now_ms, task.updated_at_ms), if unseen { ", not yet seen" } else { "" }))
        .help(format!("{} · {}", status_label(task.status), age(s.now_ms, task.updated_at_ms)))
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
        .on_press(move |s: &mut App| s.select(&press_id))
        .on_secondary_press(move |s: &mut App, p, _| s.row_menu = Some((id.clone(), p)))
        .child(status_glyph(status_of(task.status), t, 14.0, Some("")));
    if let Some(key) = &task.key {
        r = r.child(text(key.clone()).font(t.ty.id).color(t.text.faint).a11y_hidden());
    }
    r = r.child(text(task.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if unseen {
        // Iris, 6 points: what changed since you looked (DESIGN.md §8 rule 13).
        r = r.child(el().key(format!("unseen-{}", task.id)).size(6.0, 6.0).radius(3.0).fill(t.meaning.iris).a11y_hidden());
    }
    // The amber half circle is both "working" and "needs you": those rows say which in words;
    // the others say how long ago (the words are in the row's name for assistive technology).
    let meta = if status_of(task.status) == crate::status::StatusKind::InProgress { status_word(task.status).to_owned() } else { age(s.now_ms, task.updated_at_ms) };
    r = r.child(text(meta).key(format!("meta-{}", task.id)).font(t.ty.id).color(t.text.faint).a11y_hidden());

    match s.row_menu.as_ref().filter(|(open, _)| *open == task.id) {
        Some((_, at)) => {
            let (a, b, c, d) = (task.id.clone(), task.id.clone(), task.id.clone(), task.title.clone());
            let mut menu = context_menu(format!("row-menu-{}", task.id), r, Some(*at)).on_close(|s: &mut App| s.row_menu = None);
            menu = menu.item(MenuItem::new("Mark as unread").on_select(move |s: &mut App| {
                s.row_menu = None;
                s.mark_unread(&a);
            }));
            if running(task.status) {
                menu = menu.item(MenuItem::new("Stop").on_select(move |s: &mut App| {
                    s.row_menu = None;
                    s.open_task(&b);
                    s.ask_stop(&b, &d);
                }));
            }
            menu.item(MenuItem::new("Open in web UI").on_select(move |s: &mut App| {
                s.row_menu = None;
                s.open_in_web(Some(&format!("tasks?task={c}&view=full")));
            }))
            .build(&t.kit, layer)
        }
        None => r,
    }
}

fn foot(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let new = nav_row(&icons::PLUS, "New", "new-session", s.new_form.is_some(), t).help("Start a session").on_press(|s: &mut App| s.open_new());
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

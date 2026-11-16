//! The one view P0 has: a plain root with the connection's state and a few live task rows drawn
//! with the status glyph. It is a placeholder that proves the link, the theme and the glyphs on a
//! real window, not a layout: the harness layout (sessions, an agent workspace, Neko as a side
//! agent, contextual panels) is pending an owner decision, so there are no tabs, no rail and no
//! pages here to throw away.
//!
//! Colours, sizes and fonts come from `theme.rs`; this file names none.

use rune::{Cx, El, col, row, text};
use rune_kit::shell::WindowTitlebar;
use rune_kit::root;

use crate::app::{App, FEATURED, featured, status_line};
use crate::lab::lab;
use crate::link::TaskRow;
use crate::status::{status_glyph, status_label, status_of};
use crate::theme::{Neko, theme};

/// One column (DESIGN.md §8 rule 8).
pub const COLUMN: f32 = 880.0;

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let t = theme(cx.system_dark());
    // An empty band the window can be dragged by, clear of the traffic lights.
    let bar = WindowTitlebar::from_insets(cx.chrome_insets()).fill(t.surface.window).build(&t.kit);
    let page = if s.lab { lab(&t) } else { tasks(s, &t) };
    let status = status_line(&s.link, s.summary.as_ref());
    root(
        col()
            .key("window")
            .w_full()
            .h_full()
            .fill(t.surface.window)
            .child(bar)
            .child(row().key("page-scroll").w_full().grow(1.0).min_h(0.0).justify_center().child(page))
            .child(row().key("status").w_full().justify_center().pb(14.0).child(
                row().w_full().max_w(COLUMN).px(32.0).child(text(status).font(t.ty.meta).color(t.text.faint).truncate().live_region(rune::LiveRegion::Polite).key("status-text")),
            )),
        Vec::new(),
    )
}

/// The placeholder: a header with the count, a few rows, one way to the web UI.
fn tasks(s: &App, t: &Neko) -> El {
    let rows = s.summary.as_ref().map(|m| featured(m, FEATURED)).unwrap_or_default();
    let count = s.summary.as_ref().map_or(0, |m| m.tasks.len());
    let mut header = row().key("header").gap(8.0).items_end().child(text("Tasks").key("title").font(t.ty.title).color(t.text.base).role("heading"));
    if s.summary.is_some() {
        header = header.child(text(count.to_string()).key("count").font(t.ty.meta).color(t.text.faint).pb(2.0));
    }
    let mut list = col().key("rows").w_full();
    if s.summary.is_some() && rows.is_empty() {
        // A stage with nothing in it says so in one line (DESIGN.md §3).
        list = list.child(text("No tasks").key("empty").font(t.ty.meta).color(t.text.muted).px(10.0));
    }
    for task in &rows {
        list = list.child(task_row(task, t));
    }
    let mut page = col()
        .key("page")
        .w_full()
        .max_w(COLUMN)
        .px(32.0)
        .pt(24.0)
        .gap(18.0)
        .items_start()
        .child(header)
        .child(list)
        .child(primary_button("Open in the web UI", "open-web", t, |s: &mut App| s.open_in_web()))
        .child(text("The new app is under construction. The web UI has everything today.").key("note").font(t.ty.meta).color(t.text.muted));
    if let Some(notice) = &s.notice {
        page = page.child(text(notice.clone()).key("notice").font(t.ty.meta).color(t.text.muted).live_region(rune::LiveRegion::Polite));
    }
    page
}

/// One line per task (DESIGN.md §8 rule 3): glyph, faint id, title, quiet meta at the right.
fn task_row(task: &TaskRow, t: &Neko) -> El {
    let id = task.key.clone().unwrap_or_else(|| task.id.chars().take(8).collect());
    row()
        .key(format!("task-{}", task.id))
        .w_full()
        .h(36.0)
        .px(10.0)
        .gap(10.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .child(status_glyph(status_of(task.status), t, 14.0, Some("")))
        .child(text(id).font(t.ty.id).color(t.text.faint).min_w(52.0))
        .child(text(task.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0))
        .child(text(status_label(task.status)).font(t.ty.meta).color(t.text.muted))
}

/// The one primary action on a page: Iris, the colour of "do this".
fn primary_button(label: &str, key: &str, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .h(28.0)
        .px(12.0)
        .radius(t.radius.control)
        .center()
        .fill(t.meaning.iris)
        .hover(rune::Color::WHITE.alpha(0.14))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(text(label.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(rune::Color::WHITE).a11y_hidden())
}

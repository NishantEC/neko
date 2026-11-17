//! The Tree layout of All sessions: who started whom (the web's "Agent tree", `pages/work/Tree.tsx`, `work/TreeView.tsx`, as a layout
//! here). A task a split started, or another task proposed, sits under its parent; each level is indented 20 points, a chevron folds the
//! children, and a row is 32 high: the status glyph, the title, the state in words and the age. The filters of the other layouts apply.

use rune::{Color, El, col, el, icon, row, text};
use rune_kit::{MenuItem, context_menu};

use super::ui;
use crate::app::App;
use crate::board::{INDENT, card_words};
use crate::groups::age;
use crate::icons;
use crate::link::TaskRow;
use crate::status::{status_glyph, status_of};
use crate::theme::Neko;

#[allow(clippy::too_many_arguments)]
fn node(s: &App, t: &Neko, task: &TaskRow, depth: usize, children: usize, folded: bool, cursor_on: bool, layer: &mut Vec<El>) -> El {
    let selected = cursor_on || (s.selected.as_deref() == Some(task.id.as_str()) && s.view.is_none());
    let (words, _) = card_words(task);
    let when = age(s.now_ms, task.updated_at_ms);
    let id = task.id.clone();
    let (press_id, menu_id, fold_id) = (id.clone(), id.clone(), id.clone());
    let unseen = s.unseen.contains(&task.id);
    let label = format!("{}, {words}, {when}{}{}", task.title, if children > 0 { format!(", {children} below, {}", if folded { "hidden" } else { "shown" }) } else { String::new() }, if unseen { ", not yet seen" } else { "" });
    let chevron = if children > 0 {
        row()
            .key(format!("tree-fold-{id}"))
            .role("button")
            .label(if folded { format!("Show {children} below {}", task.title) } else { format!("Hide {children} below {}", task.title) })
            .help(if folded { "Show what it started" } else { "Hide what it started" })
            .focusable(true)
            .size(24.0, 24.0)
            .center()
            .radius(6.0)
            .hover(t.surface.active)
            .focus_ring(2.0, 2.0, t.surface.ring)
            .on_press(move |s: &mut App| s.tree_toggle(&fold_id))
            .child(icon(if folded { &icons::CHEVRON_RIGHT } else { &icons::CHEVRON_DOWN }, 16.0).color(t.text.muted).a11y_hidden())
    } else {
        el().key(format!("tree-lead-{id}")).size(24.0, 24.0)
    };
    let mut r = row()
        .key(format!("tree-row-{id}"))
        .role("treeitem")
        .label(label)
        .checked(selected)
        .focusable(true)
        .w_full()
        .h(32.0)
        .pl(8.0 + INDENT * depth as f32)
        .pr(8.0)
        .gap(6.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if selected { t.surface.selected } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.select(&press_id))
        .on_secondary_press(move |s: &mut App, p, _| s.row_menu = Some((menu_id.clone(), p)))
        .child(chevron)
        .child(status_glyph(status_of(task.status), t, 14.0, Some("")))
        .child(text(task.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if unseen {
        r = r.child(el().key(format!("tree-unseen-{id}")).size(6.0, 6.0).radius(3.0).fill(t.meaning.iris).a11y_hidden());
    }
    r = r.child(text(words).key(format!("tree-state-{id}")).font(t.ty.meta).color(t.text.muted).a11y_hidden()).child(text(when).font(t.ty.id).color(t.text.faint).w(32.0).a11y_hidden());
    match s.row_menu.as_ref().filter(|(open, _)| *open == task.id) {
        Some((_, at)) => {
            let mut menu = context_menu(format!("row-menu-{}", task.id), r, Some(*at)).on_close(|s: &mut App| s.row_menu = None);
            if children > 0 {
                let owner = task.id.clone();
                menu = menu.item(MenuItem::new(if folded { "Show what it started" } else { "Hide what it started" }).on_select(move |s: &mut App| {
                    s.row_menu = None;
                    s.tree_toggle(&owner);
                }));
                menu = menu.separator();
            }
            crate::task_actions::fill_menu(menu, s, &task.id).build(&t.kit, layer)
        }
        None => r,
    }
}

/// The tree: the open rows, each under its parent.
pub fn body(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(summary) = s.summary.as_ref() else { return ui::quiet_line(t, "tree-none", "Neko isn't running yet.") };
    let rows = s.tree_visible();
    let mut list = col().key("tree").role("tree").label("Who started whom").focus_group().w_full().max_w(880.0).px(24.0).pb(24.0).gap(0.0);
    if rows.is_empty() {
        let line = if s.all.search.text.trim().is_empty() { "Nothing to look after yet." } else { "No session matches." };
        list = list.child(ui::quiet_line(t, "tree-empty", line));
    }
    let cursor = s.all.cursor.min(rows.len().saturating_sub(1));
    for (i, r) in rows.iter().enumerate() {
        if let Some(task) = summary.task(&r.id) {
            list = list.child(node(s, t, task, r.depth, r.children, r.folded, s.all.cursor_shown && i == cursor, layer));
        }
    }
    col().key("tree-scroll").w_full().grow(1.0).min_h(0.0).items_center().scroll(rune::ScrollSpec { stick_bottom: false }).child(list)
}

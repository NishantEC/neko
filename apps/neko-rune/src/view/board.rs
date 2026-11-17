//! The Board layout of All sessions (`board.rs` holds the rules; the web files are `work/board/BoardCard.tsx` and `TaskBoard.tsx`).
//! One 280 point column per stage with a 12 point gap, scrolling sideways; a column has a header (12.5 medium muted, then the
//! count) and its stage's "Nothing…" line when empty. A card is DESIGN.md §8 rule 10: no frame, the foreground at 3% (4% in light)
//! over the page, radius 10, padding 12; the title is 13.5 on at most two lines, the meta line is 12.5 muted (the status glyph, the
//! state in words, the workspace, the age); a finished task not yet seen carries Iris dot at the top right; the one action of a
//! waiting task (Approve, Answer, Start again) shows at the end of the state line on hover or focus; the selected card has a 2 point
//! Iris ring. A card is dragged to another column: only what the web allows highlights the column, a refused column shows the
//! not-allowed cursor and does not.

use rune::{Color, Cursor, El, Phase, ScrollSpec, Shadow, TextStyle, col, el, row, text};
use rune_kit::{MenuItem, context_menu};

use super::{accent_button, ui};
use crate::all_sessions::Stage;
use crate::app::App;
use crate::board::{COLUMN_GAP, COLUMN_W, CardAction, Decision, card_words, decide, stage_empty};
use crate::groups::age;
use crate::link::TaskRow;
use crate::status::{status_glyph, status_of};
use crate::theme::Neko;

/// A card's own padding and corner.
const PAD: f32 = 12.0;
const RADIUS: f32 = 10.0;

/// What dropping the carried card on `stage` would do (None when nothing is carried).
fn verdict(s: &App, stage: Stage) -> Option<Decision> {
    let carry = s.all.carry.as_ref().filter(|c| c.moved)?;
    let status = s.summary.as_ref()?.task(&carry.id)?.status;
    Some(decide(status, stage))
}

/// The card's surface: the foreground at 3% in dark, a step off the page in light (4%).
fn surface(t: &Neko) -> Color {
    t.text.base.alpha(if t.dark { 0.03 } else { 0.04 })
}

fn meta_line(s: &App, task: &TaskRow) -> String {
    let (words, _) = card_words(task);
    let workspace = s.summary.as_ref().and_then(|m| m.workspaces.iter().find(|w| w.id == m.workspace_of(&task.id))).map(|w| w.name.clone());
    let mut parts = vec![words.to_owned()];
    if let Some(w) = workspace.filter(|w| !w.is_empty()) {
        parts.push(w);
    }
    parts.push(age(s.now_ms, task.updated_at_ms));
    parts.join(" · ")
}

/// The card's face: what the carried copy shows too.
fn face(s: &App, t: &Neko, task: &TaskRow, action: Option<CardAction>, key: String) -> El {
    let unseen = s.unseen.contains(&task.id);
    let mut title_row = row().w_full().gap(8.0).items_start().child(text(task.title.clone()).font(t.ty.row).color(t.text.base).max_lines(2).grow(1.0).min_w(0.0).a11y_hidden());
    if unseen {
        title_row = title_row.child(el().key(format!("board-unseen-{}", task.id)).size(6.0, 6.0).radius(3.0).fill(t.meaning.iris).mt(6.0).a11y_hidden());
    }
    let mut meta = row().w_full().gap(6.0).center_y().h(20.0).child(status_glyph(status_of(task.status), t, 12.0, Some(""))).child(text(meta_line(s, task)).key(format!("board-meta-{}", task.id)).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if let Some(a) = action {
        // The action lies over the end of the state line on hover or focus: a solid pad in the page's colour under the card's own
        // tint, so the words under it do not show through.
        let id = task.id.clone();
        let button = accent_button(a.label(), &format!("board-action-{}", task.id), t, move |s: &mut App| s.card_act(&id, a)).h(20.0).px(6.0);
        meta = meta.child(row().key(format!("board-action-slot-{}", task.id)).pin(None, Some(0.0), Some(0.0), Some(0.0)).fill(t.surface.window).group_opacity(0.0, 1.0).child(row().h_full().pl(8.0).center_y().fill(surface(t)).child(button)));
    }
    col().key(key).w(COLUMN_W).gap(6.0).padding(PAD).radius(RADIUS).fill(surface(t)).child(title_row).child(meta)
}

/// One card: a button that selects, drags, and has the same right-click menu as the list's rows.
fn card(s: &App, t: &Neko, task: &TaskRow, cursor_on: bool, layer: &mut Vec<El>) -> El {
    let selected = cursor_on || s.selected.as_deref() == Some(task.id.as_str());
    let (_, action) = card_words(task);
    let id = task.id.clone();
    let carried = s.all.carry.as_ref().is_some_and(|c| c.moved && c.id == task.id);
    let carrying = s.all.carry.as_ref().is_some_and(|c| c.moved);
    let (press_id, drag_id, menu_id) = (id.clone(), id.clone(), id.clone());
    let unseen = s.unseen.contains(&task.id);
    let label = format!("{}, {}{}", task.title, meta_line(s, task), if unseen { ", not yet seen" } else { "" });
    let mut c = face(s, t, task, action, format!("board-card-{id}"))
        .role("button")
        .label(label)
        .help("Drag to another column to move it")
        .focusable(true)
        .hover_group()
        .hover(t.text.base.alpha(0.03))
        .focus_ring(2.0, 2.0, t.surface.ring)
        .opacity(if carried { 0.4 } else { 1.0 })
        .on_press(move |s: &mut App| {
            // The press that ends a carry is not a click.
            if std::mem::take(&mut s.all.suppress_press) {
                return;
            }
            s.select(&press_id);
        })
        .on_drag(move |s: &mut App, d: &mut rune::Drag, _| {
            let travel = (d.dx, d.dy);
            s.board_drag(&drag_id, d.phase, (d.start_x, d.start_y), travel, d.window_point(), d.phase == Phase::Up && d.cancelled());
        })
        .on_secondary_press(move |s: &mut App, p, _| s.row_menu = Some((menu_id.clone(), p)));
    if selected {
        // The one frame a card has: a 2 point Iris ring.
        c = c.box_shadow(Shadow { spread: 2.0, color: t.meaning.iris, ..Default::default() });
    }
    if carrying {
        c = c.cursor(match s.all.carry.as_ref().and_then(|carry| verdict(s, carry.over)) {
            Some(Decision::Refuse(_)) => Cursor::NotAllowed,
            _ => Cursor::Grabbing,
        });
    }
    match s.row_menu.as_ref().filter(|(open, _)| *open == task.id) {
        Some((_, at)) => {
            let mut menu = context_menu(format!("row-menu-{}", task.id), c, Some(*at)).on_close(|s: &mut App| s.row_menu = None);
            if let Some(a) = action {
                let owner = task.id.clone();
                menu = menu.item(MenuItem::new(a.label()).on_select(move |s: &mut App| {
                    s.row_menu = None;
                    s.card_act(&owner, a);
                }));
            }
            let owner = task.id.clone();
            menu = menu.item(MenuItem::new("Open").on_select(move |s: &mut App| {
                s.row_menu = None;
                s.select(&owner);
            }));
            menu = menu.separator();
            crate::task_actions::fill_menu(menu, s, &task.id).build(&t.kit, layer)
        }
        None => c,
    }
}

/// The dashed slot a column shows at its foot while an allowed card is over it: what the drop will do.
fn drop_slot(t: &Neko, words: &str, stage: Stage) -> El {
    row()
        .key(format!("board-slot-{}", stage.title()))
        .role("status")
        .label(words.to_owned())
        .w_full()
        .min_h(64.0)
        .px(12.0)
        .center()
        .radius(12.0)
        .fill(t.meaning.iris.alpha(0.06))
        .border(1.0, t.meaning.iris.alpha(0.5))
        .border_dash(6.0, 4.0)
        .child(text(words.to_owned()).font(t.ty.meta).color(t.meaning.iris).a11y_hidden())
}

fn column(s: &App, t: &Neko, stage: Stage, cursor_id: Option<&str>, layer: &mut Vec<El>) -> El {
    let tasks = s.board_column(stage);
    let decision = verdict(s, stage);
    let over = s.all.carry.as_ref().is_some_and(|c| c.moved && c.over == stage);
    let mut cards = col().key(format!("board-cards-{}", stage.title())).w_full().gap(8.0).pb(16.0);
    for task in &tasks {
        cards = cards.child(card(s, t, task, cursor_id == Some(task.id.as_str()), layer));
    }
    match &decision {
        Some(Decision::Move(m)) if over => cards = cards.child(drop_slot(t, m.verb(), stage)),
        _ => {}
    }
    if tasks.is_empty() && !matches!(decision, Some(Decision::Move(_))) {
        cards = cards.child(text(stage_empty(stage)).key(format!("board-empty-{}", stage.title())).font(t.ty.meta).color(t.text.muted).wrap().px(4.0).py(4.0));
    }
    let head = row()
        .key(format!("board-head-{}", stage.title()))
        .w_full()
        .h(32.0)
        .px(4.0)
        .gap(6.0)
        .center_y()
        .child(text(stage.title()).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.muted).truncate().min_w(0.0).a11y_hidden())
        .child(text(tasks.len().to_string()).key(format!("board-count-{}", stage.title())).font(t.ty.meta).color(t.text.faint).a11y_hidden());
    let mut c = col().key(format!("board-col-{}", stage.title())).role("group").label(format!("{}, {}", stage.title(), tasks.len())).w(COLUMN_W).h_full().min_h(0.0).radius(14.0).child(head).child(col().key(format!("board-scroll-{}", stage.title())).w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(cards));
    match decision {
        // An allowed column lights up Iris, brighter under the pointer; a refused one dims, shows the not-allowed cursor, and does not light up.
        Some(Decision::Move(_)) => c = c.cursor(Cursor::Grabbing).fill(t.meaning.iris.alpha(if over { 0.06 } else { 0.0 })).border(1.0, t.meaning.iris.alpha(if over { 0.45 } else { 0.15 })),
        Some(Decision::Refuse(_)) => c = c.cursor(Cursor::NotAllowed).opacity(0.55),
        _ => {}
    }
    c
}

/// The carried copy, drawn over the window at the pointer where the card was taken.
fn ghost(s: &App, t: &Neko) -> Option<El> {
    let carry = s.all.carry.as_ref().filter(|c| c.moved)?;
    let task = s.summary.as_ref()?.task(&carry.id)?;
    let (x, y) = (carry.at.0 - carry.grab.0, carry.at.1 - carry.grab.1);
    Some(face(s, t, task, None, "board-ghost".to_owned()).fill(t.kit.c.surface_raised).border(1.0, t.surface.field_border).opacity(0.95).pin(Some(x), Some(y), None, None))
}

/// The board: the columns side by side, scrolling sideways; the carried card floats above.
pub fn body(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    if s.summary.is_none() {
        return ui::quiet_line(t, "board-none", "Neko isn't running yet.");
    }
    let cursor_id: Option<String> = if s.all.cursor_shown { s.nav_ids().get(s.all.cursor).cloned() } else { None };
    let mut columns = row().key("board-columns").h_full().gap(COLUMN_GAP).px(24.0);
    for stage in s.board_stages() {
        columns = columns.child(column(s, t, stage, cursor_id.as_deref(), layer));
    }
    if let Some(g) = ghost(s, t) {
        layer.push(g);
    }
    col().key("board").w_full().grow(1.0).min_h(0.0).child(row().key("board-scroll-x").w_full().h_full().min_h(0.0).scroll_x().child(columns))
}

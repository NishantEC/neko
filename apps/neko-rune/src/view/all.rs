//! "All sessions" (`all_sessions.rs` holds the rules): every session under the web's stage headings, newest first, with a
//! search field (`/` focuses it), a stage and a workspace filter, J and K to move and a header overflow with "Show failed and
//! cancelled" and "Clear finished". It is where work older than a week that is not waiting on you lives (polish spec §0).
//! The header's segmented control chooses how it is laid out: List (this file), Board (`board.rs`) or Tree (`tree.rs`); the
//! choice is kept.

use rune::{Color, El, ScrollSpec, TextStyle, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement};

use super::{band, board, tree, ui, webkit};
use crate::all_sessions::Stage;
use crate::app::App;
use crate::board::Layout;
use crate::icons;
use crate::theme::Neko;
use neko_protocol::workbench::TaskStatus;

/// The web's `relative()` (`work/model.ts`): "now", "5 min ago", "1 hr ago", "yesterday", "5 days ago".
pub fn relative(now_ms: i64, ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let secs = ((now_ms - ms) / 1000).max(0);
    if secs < 60 {
        return "now".to_owned();
    }
    let m = secs / 60;
    if m < 60 {
        return format!("{m} min ago");
    }
    let h = m / 60;
    if h < 24 {
        return format!("{h} hr ago");
    }
    let d = h / 24;
    if d == 1 {
        return "yesterday".to_owned();
    }
    if d < 30 {
        return format!("{d} days ago");
    }
    let mo = d / 30;
    if mo < 12 { format!("{mo} mo ago") } else { format!("{} yr ago", mo / 12) }
}

fn quiet(t: &Neko, size: f32, lh: f32, weight: f32) -> TextStyle {
    webkit::font(t, size, weight, lh)
}

/// The web's `StageTabs`: All, then one per stage, each with a faint count; the open one on a soft fill.
fn stage_tabs(s: &App, t: &Neko, counts: &[usize; 5], total: usize) -> El {
    let mut well = row().key("all-stage-tabs").role("tablist").gap(2.0).center_y();
    let mut one = |key: String, label: &str, n: usize, open: bool, pick: Option<Stage>| {
        well = std::mem::replace(&mut well, row()).child(
            row()
                .key(key)
                .role("tab")
                .label(format!("{label}, {n}"))
                .checked(open)
                .focusable(true)
                .h(28.0)
                .px(9.0)
                .gap(6.0)
                .center_y()
                .radius(10.0)
                .fill(if open { t.text.base.alpha(0.07) } else { Color::TRANSPARENT })
                .hover(t.text.base.alpha(0.04))
                .focus_ring(2.0, 2.0, t.kit.c.border_focus)
                .on_press(move |s: &mut App| {
                    s.all.stage = pick;
                    s.all.cursor = 0;
                })
                .child(text(label.to_owned()).font(quiet(t, 13.0, 19.5, 400.0)).color(if open { t.text.base } else { t.text.muted }).a11y_hidden())
                .child(text(n.to_string()).font(webkit::tnum(quiet(t, 12.0, 19.5, 400.0))).color(t.text.muted.alpha(0.7)).a11y_hidden()),
        );
    };
    one("all-tab-all".to_owned(), "All", total, s.all.stage.is_none(), None);
    for (i, st) in Stage::ALL.iter().enumerate() {
        one(format!("all-tab-{i}"), st.title(), counts[i], s.all.stage == Some(*st), Some(*st));
    }
    well
}

/// The search label (`label.group/search`): a magnifier, the words, and the "/" key; 32 high, 8 at the sides.
fn search_label(s: &App, t: &Neko) -> El {
    let style = quiet(t, 13.0, 19.5, 400.0);
    let field = rune::text_input(&s.all.search, style).key("all-search").grow(1.0).min_w(0.0).color(t.text.base).placeholder("Search".to_owned(), t.text.muted).caret(t.text.base, t.kit.selection()).label("Search tasks".to_owned()).on_edit(|s: &mut App| &mut s.all.search);
    row().key("all-search-label").h(32.0).w(146.0).px(8.0).gap(8.0).center_y().radius(10.0).hover(t.text.base.alpha(0.03)).child(icon(&icons::SEARCH, 14.0).color(t.text.muted).a11y_hidden()).child(field).child(webkit::kbd(t, "all-search-kbd", "/"))
}

/// The layout switch (`SegmentedControl`): three 44 x 34 icon buttons in a 40 high well; the open one is a pebble.
fn layout_switch(s: &App, t: &Neko) -> El {
    let layout = s.all_layout();
    let mut well = webkit::well(t, row().key("all-layout").role("tablist").label("Layout").h(40.0).px(2.0).gap(2.0).center_y().radius(24.0));
    for (l, glyph) in [(Layout::List, &icons::ROWS), (Layout::Board, &icons::KANBAN), (Layout::Tree, &icons::TREE)] {
        let on = l == layout;
        let tab = row().key(format!("all-layout-{}", l.title().to_lowercase())).role("tab").label(l.title()).checked(on).focusable(true).size(44.0, 34.0).center().radius(21.0).hover(if on { Color::TRANSPARENT } else { t.surface.hover }).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.set_all_layout(l));
        let tab = if on { webkit::pebble(t, tab) } else { tab };
        well = well.child(tab.child(icon(glyph, 16.0).color(if on { t.mat.pebble_fg } else { t.text.muted }).a11y_hidden()));
    }
    well
}

/// The Tasks header and its toolbar (`Tasks.tsx`): the title with its count, the overflow menu and "New task"; then the stage tabs, the
/// search and the layout switch, which wrap as the web's do.
fn header(s: &App, t: &Neko, layer: &mut Vec<El>, rows: &[&crate::link::TaskRow]) -> El {
    let q = s.all.search.text.trim().to_lowercase();
    let counts_of = |st: Stage| s.summary.as_ref().map_or(0, |m| m.tasks.iter().filter(|x| Stage::of(x.status) == st && s.all.workspace.as_deref().is_none_or(|w| m.workspace_of(&x.id) == w) && (s.all.include_stopped || !matches!(x.status, TaskStatus::Failed | TaskStatus::Cancelled)) && (q.is_empty() || x.title.to_lowercase().contains(&q))).count());
    let counts = [counts_of(Stage::NeedsYou), counts_of(Stage::Review), counts_of(Stage::Running), counts_of(Stage::UpNext), counts_of(Stage::Done)];
    let total: usize = counts.iter().sum();
    let in_ws = s.summary.as_ref().map_or(0, |m| m.tasks.iter().filter(|x| s.all.workspace.as_deref().is_none_or(|w| m.workspace_of(&x.id) == w)).count());
    let count_text = if rows.len() == in_ws { in_ws.to_string() } else { format!("{} of {in_ws}", rows.len()) };
    let workspace_name = s.all.workspace.as_ref().and_then(|id| s.summary.as_ref().and_then(|m| m.workspaces.iter().find(|w| &w.id == id)).map(|w| w.name.clone()));
    let mut title = row().gap(10.0).center_y().child(text("Tasks").key("all-title").font(rune::TextStyle { tracking: -0.41, ..webkit::font(t, 20.0, 600.0, 30.0) }).color(t.text.strong).role("heading")).child(text(count_text).key("all-count").font(webkit::tnum(quiet(t, 14.0, 22.75, 400.0))).color(t.text.muted));
    if let Some(name) = workspace_name {
        title = title.child(text(format!("· {name}")).font(quiet(t, 14.0, 22.75, 400.0)).color(t.text.muted));
    }
    let trigger = row().key("all-more").role("button").label("Work options").help("Work options").focusable(true).size(36.0, 36.0).center().radius(18.0).hover(t.text.base.alpha(0.06)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(|s: &mut App| s.all.menu_open = !s.all.menu_open).child(icon(&icons::MORE, 16.0).color(t.text.base).a11y_hidden());
    let menu = Menu::new("all-menu")
        .trigger(trigger)
        .open(s.all.menu_open)
        .placement(Placement::BottomEnd)
        .min_width(240.0)
        .on_close(|s: &mut App| s.all.menu_open = false)
        .item(MenuItem::new("Show failed and cancelled").checked(s.all.include_stopped).on_select(|s: &mut App| {
            s.all.menu_open = false;
            s.all.include_stopped = !s.all.include_stopped;
            s.all.cursor = 0;
        }))
        .item(MenuItem::new("Clear finished").danger(true).on_select(|s: &mut App| {
            s.all.menu_open = false;
            s.ask_clear_finished();
        }))
        .build(&t.kit, layer);
    let new_task = webkit::button(t, "all-new-task", "New task", Some(&icons::PLUS), true, false, |s: &mut App| s.open_new());
    let head = row().w_full().min_h(36.0).gap(10.0).center_y().child(title).child(el().grow(1.0).min_w(0.0)).child(row().gap(6.0).center_y().child(menu).child(new_task));
    // The Rune-only workspace filter sits last in the toolbar's second line.
    let mut options = vec!["All workspaces".to_owned()];
    let mut names: Vec<(String, String)> = Vec::new();
    if let Some(m) = s.summary.as_ref() {
        for w in &m.workspaces {
            options.push(w.name.clone());
            names.push((w.id.clone(), w.name.clone()));
        }
    }
    let current = s.all.workspace.as_ref().and_then(|id| names.iter().find(|(i, _)| i == id)).map_or("All workspaces".to_owned(), |(_, n)| n.clone());
    let workspace = row().key("all-workspace").child(ui::pick(s, t, layer, "all-workspace-pick", "Workspace", &options, &current, 180.0, move |s: &mut App, chosen: &str| {
        s.all.workspace = names.iter().find(|(_, n)| n == chosen).map(|(i, _)| i.clone());
        s.all.cursor = 0;
    }));
    let toolbar = row().key("all-toolbar").role("search").label("Filter tasks").w_full().mt(16.0).gaps(8.0, 12.0).flex_wrap().center_y().child(stage_tabs(s, t, &counts, total)).child(el().grow(1.0).min_w(0.0)).child(search_label(s, t)).child(layout_switch(s, t)).child(workspace);
    col().key("all-header").w_full().px(28.0).pt(24.0).pb(12.0).child(head).child(toolbar)
}

/// One task as one calm line (`ListRow`): its glyph, the faint id, the title, why it waits, where it came from, the age.
fn list_row(s: &App, t: &Neko, layer: &mut Vec<El>, task: &crate::link::TaskRow, cursor_on: bool, show_ws: bool) -> El {
    let id = task.id.clone();
    let press_id = id.clone();
    let line = &task.line;
    let selected = s.selected.as_deref() == Some(task.id.as_str()) && s.view.is_none();
    let on = selected || cursor_on;
    let where_ = if !line.repo.is_empty() { line.repo.clone() } else if show_ws { s.summary.as_ref().and_then(|m| m.workspaces.iter().find(|w| w.id == m.workspace_of(&task.id))).map(|w| w.name.clone()).unwrap_or_default() } else { String::new() };
    let mut button = row().key(format!("session-{}", task.id)).role("button").label(format!("{}, {}", task.title, crate::status::status_label(task.status))).checked(selected).focusable(true).grow(1.0).shrink().min_w(0.0).h_full().gap(12.0).center_y().radius(6.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.select(&press_id));
    button = button.child(text(line.short_id.clone()).key(format!("all-id-{}", task.id)).font(webkit::tnum(quiet(t, 12.0, 19.5, 400.0))).color(t.text.muted.alpha(0.85)).w(68.0).a11y_hidden());
    // `max-w-[60%] shrink-0 truncate`: the box is the text's width, at most 60% of the line's button.
    let title_style = quiet(t, 13.5, 20.25, 400.0);
    let title_w = rune::text::measure(&task.title, &[], title_style, None).0.min(0.6 * (webkit::page_width() - 170.0));
    button = button.child(text(task.title.clone()).key(format!("all-title-{}", task.id)).font(title_style).color(t.text.base).truncate().w(title_w).a11y_hidden());
    button = button.child(row().grow(1.0).shrink().min_w(0.0).center_y().child(text(line.note.clone()).font(quiet(t, 12.5, 18.75, 400.0)).color(t.text.muted).truncate().min_w(0.0).shrink().a11y_hidden()));
    // `@4xl:inline`: the source shows only when the list is 896 wide.
    if !line.source.is_empty() && webkit::page_width() - 32.0 >= 896.0 {
        button = button.child(text(line.source.clone()).font(quiet(t, 12.5, 18.75, 400.0)).color(t.text.muted.alpha(0.8)).truncate().max_w(176.0).a11y_hidden());
    }
    if !line.pr.is_empty() {
        button = button.child(row().gap(4.0).center_y().child(icon(&icons::PULL_REQUEST, 12.0).color(t.text.muted.alpha(0.8)).a11y_hidden()).child(text(line.pr.clone()).font(webkit::tnum(quiet(t, 12.5, 18.75, 400.0))).color(t.text.muted.alpha(0.8)).a11y_hidden()));
    }
    if !where_.is_empty() {
        button = button.child(text(where_).key(format!("all-where-{}", task.id)).font(quiet(t, 12.5, 18.75, 400.0)).color(t.text.muted.alpha(0.8)).truncate().max_w(144.0).a11y_hidden());
    }
    if !line.runtime.is_empty() {
        button = button.child(row().key(format!("all-runtime-{}", task.id)).role("img").label(line.runtime.clone()).size(20.0, 20.0).center().radius(10.0).fill(t.text.base.alpha(0.08)).child(text(line.runtime.clone()).font(quiet(t, 9.0, 14.625, 600.0)).color(t.text.base.alpha(0.7)).a11y_hidden()));
    }
    let when = relative(s.now_ms, task.updated_at_ms);
    let end = row().w(76.0).h_full().center_y().justify_end().child(text(when).key(format!("all-when-{}", task.id)).font(webkit::tnum(quiet(t, 12.5, 18.75, 400.0))).color(t.text.muted.alpha(0.7)).group_opacity(1.0, 0.0).a11y_hidden());
    let kind = crate::status::status_of(task.status);
    let r = row().key(format!("all-line-{}", task.id)).w_full().h(40.0).px(12.0).gap(12.0).center_y().radius(10.0).hover_group().fill(if on { t.text.base.alpha(0.06) } else { Color::TRANSPARENT }).hover(t.text.base.alpha(if on { 0.06 } else { 0.03 })).child(crate::status::status_glyph(kind, t, 14.0, Some(""))).child(button).child(end).on_secondary_press({
        let id = id.clone();
        move |s: &mut App, p, _| s.row_menu = Some((id.clone(), p))
    });
    match s.row_menu.as_ref().filter(|(open, _)| *open == task.id) {
        Some((_, at)) => {
            let menu = rune_kit::context_menu(format!("row-menu-{}", task.id), r, Some(*at)).on_close(|s: &mut App| s.row_menu = None);
            crate::task_actions::fill_menu(menu, s, &task.id).build(&t.kit, layer)
        }
        None => r,
    }
}

/// The list: every task under its stage heading (a button that folds it), in the web's 16 point column.
fn list_body(s: &App, t: &Neko, layer: &mut Vec<El>, rows: &[&crate::link::TaskRow]) -> El {
    let mut list = col().key("all-list").role("list").label("Tasks").focus_group().w_full().px(16.0).pt(4.0).pb(48.0).gap(0.0);
    let show_ws = s.summary.as_ref().is_some_and(|m| m.workspaces.len() > 1) && s.all.workspace.is_none();
    match s.summary.as_ref() {
        None => list = list.child(text("Neko isn't running yet.").font(t.ty.meta).color(t.text.muted).py(6.0)),
        Some(_) => {
            if rows.is_empty() {
                list = list.child(text(if s.all.search.text.trim().is_empty() { "No sessions here." } else { "No session matches." }).key("all-empty").font(t.ty.meta).color(t.text.muted).py(6.0));
            }
            let cursor = s.all.cursor.min(rows.len().saturating_sub(1));
            let mut first = true;
            for st in Stage::ALL {
                let group: Vec<(usize, &&crate::link::TaskRow)> = rows.iter().enumerate().filter(|(_, r)| Stage::of(r.status) == st).collect();
                if group.is_empty() {
                    continue;
                }
                let folded = s.all.folded.contains(st.title());
                let title = st.title();
                let key = title.to_owned();
                list = list.child(
                    row().key(format!("all-stage-{title}")).w_full().h(32.0).px(4.0).mt(if first { 0.0 } else { 20.0 }).center_y().child(
                        row()
                            .key(format!("all-stage-toggle-{title}"))
                            .role("button")
                            .label(format!("{title}, {}", group.len()))
                            .expanded(!folded)
                            .focusable(true)
                            .h(28.0)
                            .px(8.0)
                            .gap(6.0)
                            .center_y()
                            .radius(10.0)
                            .focus_ring(2.0, 2.0, t.kit.c.border_focus)
                            .on_press(move |s: &mut App| {
                                if !s.all.folded.remove(&key) {
                                    s.all.folded.insert(key.clone());
                                }
                            })
                            .child(icon(if folded { &icons::CHEVRON_RIGHT } else { &icons::CHEVRON_DOWN }, 10.0).color(t.text.muted).a11y_hidden())
                            .child(text(title).font(quiet(t, 12.5, 18.75, 500.0)).color(t.text.muted).a11y_hidden())
                            .child(text(group.len().to_string()).key(format!("all-stage-count-{title}")).font(webkit::tnum(quiet(t, 12.5, 18.75, 500.0))).color(t.text.muted.alpha(0.7)).a11y_hidden()),
                    ),
                );
                first = false;
                if !folded {
                    for (i, task) in group {
                        let on = i == cursor && s.all.cursor_shown;
                        list = list.child(col().key(format!("all-row-{}", task.id)).w_full().child(list_row(s, t, layer, task, on, show_ws)));
                    }
                }
            }
        }
    }
    col().key("all-scroll").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list)
}

pub fn page(s: &App, t: &Neko, layer: &mut Vec<El>, region_w: f32) -> El {
    webkit::set_page_width(region_w);
    let rows = s.all_rows();
    let layout = s.all_layout();
    let body = match layout {
        Layout::List => list_body(s, t, layer, &rows),
        Layout::Board => board::body(s, t, layer),
        Layout::Tree => tree::body(s, t, layer),
    };
    col()
        .key("all-sessions")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(band(t))
        .child(header(s, t, layer, &rows))
        .child(body)
}

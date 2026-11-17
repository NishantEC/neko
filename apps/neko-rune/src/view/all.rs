//! "All sessions" (`all_sessions.rs` holds the rules): every session under the web's stage headings, newest first, with a
//! search field (`/` focuses it), a stage and a workspace filter, J and K to move and a header overflow with "Show failed and
//! cancelled" and "Clear finished". It is where work older than a week that is not waiting on you lives (polish spec §0).
//! The header's segmented control chooses how it is laid out: List (this file), Board (`board.rs`) or Tree (`tree.rs`); the
//! choice is kept.

use rune::{Color, El, ScrollSpec, TextStyle, col, el, row, text};
use rune_kit::{Menu, MenuItem, Placement, SegmentedControl, TextInput};

use super::{band, board, icon_button, sessions, tree, ui};
use crate::all_sessions::Stage;
use crate::app::App;
use crate::board::Layout;
use crate::groups::Group;
use crate::icons;
use crate::theme::Neko;

/// The icon a row wears in this view: the group's, by the stage it is in.
fn group_of(stage: Stage) -> Group {
    match stage {
        Stage::NeedsYou | Stage::Review => Group::NeedsYou,
        Stage::Running | Stage::UpNext => Group::Running,
        Stage::Done => Group::Recent,
    }
}

/// The list: every session under its stage heading, in one centred column.
fn list_body(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let mut list = col().key("all-list").role("list").label("All sessions").focus_group().w_full().max_w(680.0).px(24.0).pb(24.0).gap(0.0);
    match s.summary.as_ref() {
        None => list = list.child(text("Neko isn't running yet.").font(t.ty.meta).color(t.text.muted).py(6.0)),
        Some(_) => {
            let rows = s.all_rows();
            if rows.is_empty() {
                list = list.child(text(if s.all.search.text.trim().is_empty() { "No sessions here." } else { "No session matches." }).key("all-empty").font(t.ty.meta).color(t.text.muted).py(6.0));
            }
            let mut current: Option<Stage> = None;
            let cursor = s.all.cursor.min(rows.len().saturating_sub(1));
            for (i, task) in rows.iter().enumerate() {
                let stage = Stage::of(task.status);
                if current != Some(stage) {
                    current = Some(stage);
                    let n = rows.iter().filter(|r| Stage::of(r.status) == stage).count();
                    list = list.child(
                        row()
                            .key(format!("all-stage-{}", stage.title()))
                            .w_full()
                            .h(32.0)
                            .px(8.0)
                            .mt(if i == 0 { 0.0 } else { 8.0 })
                            .gap(8.0)
                            .center_y()
                            .child(text(stage.title()).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.muted).grow(1.0).a11y_hidden())
                            .child(text(n.to_string()).font(t.ty.meta).color(t.text.muted).a11y_hidden()),
                    );
                }
                let on = i == cursor && s.all.cursor_shown;
                list = list.child(col().key(format!("all-row-{}", task.id)).w_full().radius(t.radius.row).fill(if on { t.surface.active } else { Color::TRANSPARENT }).child(sessions::session_row(s, task, group_of(stage), t, layer)));
            }
        }
    }
    col().w_full().grow(1.0).min_h(0.0).items_center().scroll(ScrollSpec { stick_bottom: false }).child(list)
}

pub fn page(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {

    let search = TextInput::new(&s.all.search)
        .label("Search sessions")
        .placeholder("Search sessions")
        .width(Some(240.0))
        .leading_icon(&icons::SEARCH)
        .on_edit(|s: &mut App| &mut s.all.search)
        .on_change(|s: &mut App, _| s.all.cursor = 0)
        .build(&t.kit)
        .key("all-search");

    // The stage filter (the web's stage tabs): one stage, or all of them. The board then shows that column alone.
    let mut stages = vec!["All stages".to_owned()];
    stages.extend(Stage::ALL.iter().map(|st| st.title().to_owned()));
    let stage_now = s.all.stage.map_or("All stages", Stage::title).to_owned();
    let stage = row().key("all-stage").child(ui::pick(s, t, layer, "all-stage-pick", "Stage", &stages, &stage_now, 150.0, |s: &mut App, chosen: &str| {
        s.all.stage = Stage::ALL.into_iter().find(|st| st.title() == chosen);
        s.all.cursor = 0;
    }));

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

    let trigger = icon_button(&icons::MORE, "More", "all-more", t, |s: &mut App| s.all.menu_open = !s.all.menu_open);
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

    // List, Board, Tree: a segmented control in the header; the choice is kept.
    let labels: Vec<&str> = Layout::ALL.iter().map(|l| l.title()).collect();
    let layout = s.all_layout();
    let selected = Layout::ALL.iter().position(|l| *l == layout);
    let switch = row().key("all-layout").child(SegmentedControl::new(&labels, selected).label("Layout").fit().on_pick(|s: &mut App, i: usize, _| s.set_all_layout(Layout::ALL[i])).build(&t.kit));

    let wide = layout == Layout::Board;
    let controls = row().key("all-controls").w_full().max_w(if wide { 100_000.0 } else { 680.0 }).px(24.0).pb(12.0).gap(8.0).center_y().child(search).child(stage).child(workspace).child(el().grow(1.0)).child(menu);
    let body = match layout {
        Layout::List => list_body(s, t, layer),
        Layout::Board => board::body(s, t, layer),
        Layout::Tree => tree::body(s, t, layer),
    };
    col()
        .key("all-sessions")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(band(t).child(text("All sessions").key("all-title").font(t.ty.brand).color(t.text.base).role("heading")).child(el().grow(1.0)).child(switch))
        .child(col().w_full().items_center().child(controls))
        .child(body)
}

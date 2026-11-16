//! "All sessions" (`all_sessions.rs` holds the rules): every session under the web's stage headings, newest first, with a
//! search field (`/` focuses it), a workspace filter, J and K to move and a header overflow with "Show failed and
//! cancelled" and "Clear finished". It is where work older than a week that is not waiting on you lives (polish spec §0).

use rune::{Color, El, ScrollSpec, TextStyle, col, el, row, text};
use rune_kit::{Menu, MenuItem, Placement, TextInput};

use super::{band, icon_button, sessions, ui};
use crate::all_sessions::Stage;
use crate::app::App;
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

pub fn page(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
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

    let search = TextInput::new(&s.all.search)
        .label("Search sessions")
        .placeholder("Search sessions")
        .width(Some(240.0))
        .leading_icon(&icons::SEARCH)
        .on_edit(|s: &mut App| &mut s.all.search)
        .on_change(|s: &mut App, _| s.all.cursor = 0)
        .build(&t.kit)
        .key("all-search");

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

    let controls = row().key("all-controls").w_full().max_w(680.0).px(24.0).pb(12.0).gap(8.0).center_y().child(search).child(workspace).child(el().grow(1.0)).child(menu);
    col()
        .key("all-sessions")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(band(t).child(text("All sessions").key("all-title").font(t.ty.brand).color(t.text.base).role("heading")))
        .child(col().w_full().items_center().child(controls))
        .child(col().w_full().grow(1.0).min_h(0.0).items_center().scroll(ScrollSpec { stick_bottom: false }).child(list))
}

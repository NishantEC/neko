//! "+ New": a workspace, the request in your own words, an optional goal with the check that
//! proves it done, and an optional runtime. It takes the session's place until you start or cancel.

use rune::{Color, El, Key, KeyEvent, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement, Switch, TextArea, TextInput};

use super::{COLUMN, band, primary_button, quiet_button};
use crate::app::App;
use crate::new_session::{NewMenu, MAX_REQUEST};
use crate::theme::Neko;

fn picker(t: &Neko, key: &str, label: &str, value: &str, open: bool, on_press: impl Fn(&mut App) + 'static) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(format!("{label}: {value}"))
        .focusable(true)
        .h(30.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(t.radius.control)
        .fill(if open { t.surface.active } else { t.surface.hover })
        .hover(t.surface.active)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(text(value.to_owned()).font(t.ty.row).color(t.text.base).truncate().a11y_hidden())
        .child(icon(t.kit.glyph.chevron_down, 14.0).color(t.text.muted).a11y_hidden())
}

pub fn form(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(f) = s.new_form.as_ref() else { return el() };
    let k = &t.kit;
    let workspaces = s.summary.as_ref().map(|m| m.workspaces.clone()).unwrap_or_default();
    let ws_name = workspaces.get(f.workspace).map_or("No workspace".to_owned(), |w| w.name.clone());
    let rt_name = f.runtime.and_then(|i| f.choices.get(i)).map_or("Default".to_owned(), |c| c.label.clone());

    let mut ws_menu = Menu::new("new-workspace-menu")
        .trigger(picker(t, "new-workspace", "Workspace", &ws_name, f.menu == Some(NewMenu::Workspace), |s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.menu = if f.menu == Some(NewMenu::Workspace) { None } else { Some(NewMenu::Workspace) };
            }
        }))
        .open(f.menu == Some(NewMenu::Workspace))
        .placement(Placement::BottomStart)
        .min_width(240.0)
        .on_close(|s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.menu = None;
            }
        });
    for (i, w) in workspaces.iter().enumerate() {
        ws_menu = ws_menu.item(MenuItem::new(w.name.clone()).checked(f.workspace == i).on_select(move |s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.workspace = i;
                f.menu = None;
            }
        }));
    }

    let mut rt_menu = Menu::new("new-runtime-menu")
        .trigger(picker(t, "new-runtime", "Runtime", &rt_name, f.menu == Some(NewMenu::Runtime), |s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.menu = if f.menu == Some(NewMenu::Runtime) { None } else { Some(NewMenu::Runtime) };
            }
        }))
        .open(f.menu == Some(NewMenu::Runtime))
        .placement(Placement::BottomStart)
        .min_width(280.0)
        .on_close(|s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.menu = None;
            }
        })
        .item(MenuItem::new("Default").description(s.summary.as_ref().map_or(String::new(), |m| m.runtime.clone())).checked(f.runtime.is_none()).on_select(|s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.runtime = None;
                f.menu = None;
            }
        }));
    for (i, c) in f.choices.iter().enumerate() {
        rt_menu = rt_menu.item(MenuItem::new(c.label.clone()).checked(f.runtime == Some(i)).on_select(move |s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.runtime = Some(i);
                f.menu = None;
            }
        }));
    }

    let request = TextArea::new(&s.new_request)
        .placeholder("What should be done? Say it the way you would to a colleague.")
        .label("Request")
        .auto_grow(4, 10)
        .on_edit(|s: &mut App| &mut s.new_request)
        .on_key(|s: &mut App, ev: &KeyEvent, _| {
            if ev.key == Key::Enter && ev.mods.cmd {
                s.submit_new();
                return true;
            }
            false
        })
        .build(k)
        .key("new-request")
        .w_full();

    let goal = Switch::new(f.goal).label("Keep going until a check passes").on_toggle(|s: &mut App| {
        if let Some(f) = s.new_form.as_mut() {
            f.goal = !f.goal;
        }
    });
    let mut body = col()
        .key("new-form")
        .w_full()
        .max_w(COLUMN - 200.0)
        .px(32.0)
        .pt(12.0)
        .gap(16.0)
        .child(text("New session").key("new-title").font(t.ty.title).color(t.text.base).role("heading"))
        .child(row().gap(10.0).center_y().child(ws_menu.build(k, layer)).child(rt_menu.build(k, layer)))
        .child(
            col()
                .w_full()
                .px(14.0)
                .py(10.0)
                .radius(t.radius.row + 4.0)
                .fill(t.surface.card)
                .child(request),
        )
        .child(goal.build(k));
    if f.goal {
        body = body.child(
            col().gap(4.0).child(text("Check: a command that passes when it is done (optional)").font(t.ty.meta).color(t.text.muted)).child(
                TextInput::new(&s.new_check)
                    .placeholder("cargo test")
                    .label("Check command")
                    .on_edit(|s: &mut App| &mut s.new_check)
                    .build(k)
                    .key("new-check"),
            ),
        );
    }
    let blocker = s.new_blocker();
    let start = primary_button(if f.busy { "Starting…" } else { "Start" }, "new-start", t, |s: &mut App| s.submit_new());
    let mut foot = row().gap(10.0).center_y().child(if blocker.is_some() || f.busy { start.fill(Color::TRANSPARENT.alpha(0.0)).fill(t.surface.active) } else { start }).child(quiet_button("Cancel", "new-cancel", t, |s: &mut App| s.close_new()));
    if let Some(b) = blocker.filter(|_| !s.new_request.text.is_empty() || workspaces.is_empty()) {
        foot = foot.child(text(b).font(t.ty.meta).color(t.text.muted));
    }
    let used = s.new_request.text.len();
    if used > MAX_REQUEST * 9 / 10 {
        foot = foot.child(text(format!("{used}/{MAX_REQUEST}")).font(t.ty.id).color(t.text.faint));
    }
    body = body.child(foot);
    if let Some(e) = &f.error {
        body = body.child(text(e.clone()).key("new-error").font(t.ty.meta).color(t.meaning.amber).wrap().live_region(rune::LiveRegion::Polite));
    }
    col().w_full().h_full().child(band(t)).child(row().w_full().grow(1.0).min_h(0.0).justify_center().child(body))
}

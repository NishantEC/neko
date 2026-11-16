//! The session view: one line on top (title, runtime and goal), the agent's turns in the body (a
//! placeholder in P0), the composer at the bottom with its steer-mode menu. A full view (Calendar,
//! Settings, ...) takes this whole region when chosen.

use rune::{Color, El, ScrollSpec, col, el, icon, row, text};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Menu, MenuItem, Placement, TextInput};

use super::{COLUMN, band, icon_button, placeholder, primary_button};
use crate::app::{App, Steer};
use crate::icons;
use crate::lab::lab;
use crate::link::TaskRow;
use crate::status::{status_glyph, status_label, status_of};
use crate::theme::Neko;
use crate::views::FullView;

/// The card in the middle: inset from the window's edges with the window's radius.
pub fn region(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let inner = if s.lab {
        col().w_full().grow(1.0).min_h(0.0).child(band(t)).child(row().w_full().grow(1.0).min_h(0.0).justify_center().child(lab(t)))
    } else if let Some(view) = s.view {
        full_view(s, view, t)
    } else {
        match s.session() {
            Some(task) => session(s, task, t, layer),
            None => empty(s, t),
        }
    };
    col().key("session-region").grow(1.0).min_w(0.0).h_full().my(8.0).mx(2.0).radius(t.radius.window).fill(t.surface.window).child(inner)
}

/// The top line: status glyph, title, then the runtime and goal in quiet text, and the context
/// panel's toggle at the right.
fn top_line(s: &App, task: &TaskRow, t: &Neko) -> El {
    let mut meta: Vec<String> = Vec::new();
    if let Some(m) = s.summary.as_ref().filter(|m| !m.runtime.is_empty()) {
        meta.push(m.runtime.clone());
    }
    meta.push(status_label(task.status).to_owned());
    if !task.goal.is_empty() {
        meta.push(task.goal.clone());
    }
    band(t)
        .px(20.0)
        .child(status_glyph(status_of(task.status), t, 14.0, Some("")))
        .child(text(task.title.clone()).key("top-title").font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).truncate().a11y_hidden().role("heading"))
        .child(text(meta.join(" · ")).key("top-meta").font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0))
        .child(icon_button(&icons::PANEL_RIGHT, if s.layout.context.is_open() { "Hide context" } else { "Show context" }, "context-toggle", t, |s: &mut App| s.layout.context.toggle()))
}

fn session(s: &App, task: &TaskRow, t: &Neko, layer: &mut Vec<El>) -> El {
    col()
        .w_full()
        .h_full()
        .min_w(0.0)
        .child(top_line(s, task, t))
        .child(
            col().key("turns").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: true }).child(placeholder(
                &["The agent's turns appear here.", "The session view arrives in P1; the web UI shows them today."],
                t,
            )),
        )
        .child(composer(s, t, layer))
}

/// Nothing selected: before the first snapshot, or a workbench with no tasks.
fn empty(s: &App, t: &Neko) -> El {
    let line = if s.summary.is_some() { "No sessions yet. Start one with + New." } else { "Waiting for Neko." };
    col().w_full().h_full().child(band(t)).child(placeholder(&[line], t))
}

/// A full view's placeholder: what it is, when it comes, and the way to the web UI today.
fn full_view(s: &App, view: FullView, t: &Neko) -> El {
    let mut c = col()
        .key(format!("full-{}", view.web_id()))
        .w_full()
        .max_w(COLUMN)
        .px(32.0)
        .pt(24.0)
        .gap(14.0)
        .items_start()
        .child(text(view.title()).key("title").font(t.ty.title).color(t.text.base).role("heading"))
        .child(text("Coming in P2. The web UI has it today.").key("coming").font(t.ty.row).color(t.text.muted))
        .child(primary_button("Open in the web UI", "open-web", t, move |s: &mut App| s.open_in_web(Some(view.web_id()))));
    if let Some(notice) = &s.notice {
        c = c.child(text(notice.clone()).key("notice").font(t.ty.meta).color(t.text.muted).live_region(rune::LiveRegion::Polite));
    }
    col().w_full().h_full().child(band(t)).child(row().w_full().grow(1.0).min_h(0.0).justify_center().child(c))
}

/// The composer: a field, the steer-mode menu and Send. Sending arrives with the session view
/// (P1), so P0's Send is off and says why; typing and choosing a mode work.
fn composer(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let k = &t.kit;
    let field = TextInput::new(&s.composer)
        .placeholder("Message the agent…")
        .label("Message the agent")
        .plain()
        .on_edit(|s: &mut App| &mut s.composer)
        .build(k)
        .key("composer-input");

    let trigger = row()
        .key("steer-trigger")
        .role("button")
        .label(format!("Steer mode: {}", s.steer.label()))
        .focusable(true)
        .h(28.0)
        .px(8.0)
        .gap(4.0)
        .center_y()
        .radius(t.radius.control)
        .fill(if s.steer_open { t.surface.active } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, k.c.border_focus)
        .on_press(|s: &mut App| s.steer_open = !s.steer_open)
        .child(text(s.steer.label()).font(t.ty.meta).color(t.text.muted).a11y_hidden())
        .child(icon(k.glyph.chevron_down, 14.0).color(t.text.muted).a11y_hidden());
    let mut menu = Menu::new("steer-menu").trigger(trigger).open(s.steer_open).placement(Placement::TopStart).min_width(260.0).on_close(|s: &mut App| s.steer_open = false);
    for mode in Steer::ALL {
        menu = menu.item(MenuItem::new(mode.label()).description(mode.detail()).checked(s.steer == mode).on_select(move |s: &mut App| s.steer = mode));
    }
    let steer = menu.build(k, layer);

    let send = IconButton::new(&icons::SEND)
        .control_size(ControlSize::Large)
        .variant(IconButtonVariant::Ghost)
        .label("Send")
        .help("Sending arrives with the session view (P1)")
        .disabled(true)
        .build(k)
        .key("send");

    col().key("composer").w_full().px(20.0).pb(16.0).child(
        col()
            .w_full()
            .px(14.0)
            .pt(10.0)
            .pb(6.0)
            .gap(4.0)
            .radius(t.radius.row + 4.0)
            .fill(t.surface.card)
            .child(field)
            .child(row().w_full().center_y().child(steer).child(el().grow(1.0)).child(send)),
    )
}

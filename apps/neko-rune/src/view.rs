//! The window: four regions, left to right (docs/decisions/0002-rune-for-everything.md). No tabs.
//!
//! 1. **Sessions** (`sessions.rs`): every task across workspaces by state, with the foot items.
//! 2. **Session** (`session.rs`): the selected session, or a full view in its place.
//! 3. **Context** (`context.rs`): what the session produces. Closed until there is something.
//! 4. **Neko** (`neko.rs`): the super agent's column, collapsible to an icon.
//!
//! The edges between regions drag to resize (`layout.rs` holds the widths and ranges). Colours,
//! sizes and fonts come from `theme.rs`; the regions name none.

mod all;
mod board;
mod calendar;
mod compose;
mod composer;
mod context;
mod context_browser;
mod context_diff;
mod context_files;
mod context_info;
mod context_terminal;
mod dialogs;
mod fences;
mod home;
mod luau;
mod more;
mod onboarding;
mod plugins;
mod neko;
pub mod palette;
mod prs;
mod routines;
mod runtime;
mod shortcuts;
mod session;
mod sessions;
pub mod setup;
mod settings;
mod settings_ai;
mod settings_d;
mod settings_system;
mod tree;
mod turn;
mod ui;
pub mod webkit;
mod work;

pub use calendar::is_meeting;
pub use compose::MsgMenu;
pub use session::goal_body_cap;

use rune::{Cursor, Cx, Drag, El, Phase, col, el, icon, row, text};
use rune_kit::root;

use crate::app::App;
use crate::layout::{GRIP, NEKO_STRIP, Region};
use crate::theme::Neko;

/// Home's scrim: the window's colour at this strength over this radius.
pub fn home_scrim() -> (f32, f32) {
    (home::SCRIM_ALPHA, home::SCRIM_RADIUS)
}

/// One column for full views and the lab (DESIGN.md §8 rule 8).
pub const COLUMN: f32 = 880.0;
/// The session's text column (turns, cards, composer): at most this wide, with `TEXT_PAD` of air at each side.
/// The sticky header and the region's background stay full width.
pub const TEXT_COLUMN: f32 = 760.0;
pub const TEXT_PAD: f32 = 32.0;
/// The window's titlebar height: the traffic lights sit in it, and every region's header row is centred in it (polish spec §0).
pub const BAND: f32 = 44.0;
/// Every region's header row (sidebar, session, Neko): 32 high, its centre 28 from the window's top.
pub const HEADER_H: f32 = 32.0;
/// Where the traffic lights sit: their left edge 12 from the window's edge, centred on the header row (14 point buttons).
pub const TRAFFIC_LIGHTS: (f32, f32) = (12.0, 28.0 - 7.0);

/// What the window gives the middle region: its height and its own width, for the parts that must
/// fit them (an open goal, Settings' rail).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub win_h: f32,
    pub region_w: f32,
}

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let t = s.theme_now(cx.system_dark());
    s.panel.dark.set(t.dark);
    // What Luau plugins draw with this frame: Neko's own theme, and the text of their inputs.
    crate::luau::host::set_theme(t.clone());
    crate::luau::nodes::mirror_inputs(&s.luau.inputs);
    s.reduce_motion.set(cx.system_reduced_motion());
    s.remember_theme(&t);
    s.browser.scale.set(cx.env().scale);
    let mut layer = Vec::new();
    let layout = &s.layout;
    if let Some([w, h]) = cx.layout_size("window") {
        s.window_size.set(Some([w as u32, h as u32]));
    }
    // What fits: the session keeps its minimum, so a small window narrows or hides the others.
    // Before the first layout there is no width to read, and the state's own widths stand.
    let (fit, win_h, win_w) = match cx.layout_size("window") {
        Some([w, h]) => (layout.fit(w), h, w),
        None => (crate::layout::Fit { context: layout.context.shown_width(), neko_open: layout.neko.is_open() }, 840.0, 1360.0),
    };
    let neko_w = if fit.neko_open { layout.neko.width() } else { NEKO_STRIP };
    // The edges between regions are drawn only beside a region that shows (the strip has none), so count the ones that are there.
    let grips = f32::from(layout.sessions.is_open()) + f32::from(fit.context > 0.0 && !(s.panel.expanded && fit.context > 0.0)) + f32::from(fit.neko_open);
    let frame = Frame { win_h, region_w: (win_w - layout.sessions.shown_width() - fit.context - neko_w - grips * GRIP - 4.0).max(0.0) };

    s.panel.hidden.set(layout.context.is_open() && fit.context <= 0.0);
    // The web view is wanted by whatever shows it below, in this very build (`viewers::web`, `context.rs`, `session.rs`).
    s.web_view.set_want(None);
    // Expanded, the panel takes the session's region and its own; the sessions and Neko columns stay.
    let expanded = s.panel.expanded && fit.context > 0.0;
    let mut window = row().key("window").w_full().h_full().fill(t.surface.sidebar);
    if layout.sessions.is_open() {
        window = window.child(sessions::column(s, &t, &mut layer).w(layout.sessions.width()).key("sessions"));
        window = window.child(grip(Region::Sessions, &t));
    }
    // The session and the context panel are two cards in the window, inset from its edges.
    if expanded {
        window = window.child(context::panel(s, &t, &mut layer, win_w).grow(1.0));
    } else {
        window = window.child(session::region(s, &t, &mut layer, frame));
        if fit.context > 0.0 {
            window = window.child(grip(Region::Context, &t));
            window = window.child(context::panel(s, &t, &mut layer, fit.context).w(fit.context).transition(panel_transition()));
        }
    }
    if fit.neko_open {
        window = window.child(grip(Region::Neko, &t));
        window = window.child(neko::column(s, &t, win_h, &mut layer).w(layout.neko.width()).key("neko"));
    } else {
        window = window.child(neko::strip(s, &t).w(NEKO_STRIP).key("neko"));
    }
    palette::mount(s, &t, (win_w, win_h), &mut layer);
    shortcuts::mount(s, &t, &mut layer);
    mount_text_menu(s, &t, &mut layer);
    compose::mount_popover(s, &t, &mut layer);
    compose::mount_message_menu(s, &t, &mut layer);
    runtime::mount(s, &t, &mut layer);
    dialogs::mount(s, &t, &mut layer);
    settings_d::mount_guard(s, &t, &mut layer);
    crate::toasts::mount(s, &t, &mut layer);
    // Every popover, menu, select, dialog, the palette and toast is an element of this layer: while there is
    // one, a native web view (always above Rune's surface) is hidden.
    s.web_view.overlays.set(layer.len());
    s.web_view.sync();
    // Hover fills arrive and leave in 120 ms, ease-out, everywhere in the window (polish spec §4).
    // A right-click on selected text anywhere that nothing else claims: Ask Neko about this, Search for ... (the web's global menu).
    let window = window.on_secondary_press(|s: &mut App, at, tx: &mut rune::Tx| {
        if let Some(sel) = tx.text_selection().map(|sel| sel.text().to_owned()).filter(|t| !t.trim().is_empty()) {
            s.text_menu = Some((sel, at));
        }
    });
    root(window, layer).hover_motion_within(crate::motion::hover(), crate::motion::hover())
}

/// The menu on selected text: Copy, "Ask Neko about this" (the Neko composer, with the words quoted) and "Search for ...".
fn mount_text_menu(s: &App, t: &Neko, layer: &mut Vec<El>) {
    let Some((words, at)) = s.text_menu.as_ref() else { return };
    let short: String = if words.chars().count() > 24 { format!("{}…", words.chars().take(24).collect::<String>()) } else { words.clone() };
    let (copy, ask, find) = (words.clone(), words.clone(), words.clone());
    rune_kit::context_menu("text-menu", el(), Some(*at))
        .on_close(|s: &mut App| s.text_menu = None)
        .item(rune_kit::MenuItem::new("Copy").icon(&crate::icons::COPY).shortcut("⌘C").on_select_tx(move |s: &mut App, tx: &mut rune::Tx| {
            s.text_menu = None;
            tx.copy(copy.clone());
        }))
        .item(rune_kit::MenuItem::new("Ask Neko about this").icon(&crate::icons::SPARK).on_select(move |s: &mut App| {
            s.text_menu = None;
            s.ask_neko_about(&ask);
        }))
        .item(rune_kit::MenuItem::new(format!("Search for “{short}”")).icon(&crate::icons::SEARCH).on_select(move |s: &mut App| {
            s.text_menu = None;
            s.palette_search(&find);
        }))
        .build(&t.kit, layer);
}

/// How the context panel comes and goes: it slides in from its edge and fades, on a 180 ms Snappy spring (polish spec §4).
fn panel_transition() -> rune::Transition {
    let spring = crate::motion::panel();
    rune::Transition::move_edge(rune::Edge::Trailing).combined(rune::Transition::opacity()).springs(rune::SpringPair::new(spring, spring))
}

/// The draggable edge between two regions: a transparent strip that resizes the region beside it
/// (a double press puts it back to its default width).
fn grip(region: Region, t: &Neko) -> El {
    let name = format!("{region:?}").to_lowercase();
    el().key(format!("grip-{name}"))
        .role("separator")
        .label(format!("Resize {name}"))
        .w(GRIP)
        .h_full()
        .cursor(Cursor::ResizeColumn)
        .hover(t.surface.hover)
        .on_drag(move |s: &mut App, d: &mut Drag, _| {
            if d.phase == Phase::Down {
                d.data = s.layout.pane(region).width();
            }
            s.layout.drag(region, d.data, d.dx);
        })
        .on_double_press(move |s: &mut App| s.layout.pane_mut(region).reset_width())
}

/// The row across the top of a card region (the session, the context panel): 32 high, 4 below the card's top edge
/// (which is 8 below the window's), so its centre is the sidebar header's. The window can be dragged by it.
fn band(t: &Neko) -> El {
    let _ = t;
    row().h(HEADER_H + 4.0).pt(4.0).w_full().px(12.0).gap(8.0).center_y().window_drag()
}

/// The row across the top of the Neko column, which is not a card: 32 high at 12 from the window's top (the sidebar
/// header's place), the window can be dragged by it.
fn flat_band(t: &Neko) -> El {
    let _ = t;
    row().h(BAND).pt(12.0).w_full().px(12.0).gap(8.0).center_y().window_drag()
}

/// The same row for Home and the session, which also carries the button that brings the sessions column back
/// when it is folded, after the traffic lights (their 80 point inset, as in the sidebar's header).
fn card_band(t: &Neko) -> El {
    band(t)
}

/// A small icon button: 28 points, the ink muted, a soft fill under the pointer.
fn icon_button(glyph: &'static rune::Icon, label: &str, key: &str, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .help(label)
        .focusable(true)
        .size(28.0, 28.0)
        .center()
        .radius(t.radius.control)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(icon(glyph, 16.0).color(t.text.muted).a11y_hidden())
}

/// A row in a list of sections: just its words, the soft fill when it is the open one.
fn nav_label_row(label: &str, key: &str, active: bool, t: &Neko) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .w_full()
        .h(32.0)
        .px(10.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if active { t.surface.selected } else { rune::Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .child(text(label.to_owned()).font(t.ty.row).color(if active { t.text.base } else { t.text.muted }).truncate().a11y_hidden())
}

/// A secondary action: text on the soft fill under the pointer (the web's outline and ghost buttons).
fn quiet_button(label: &str, key: &str, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .h(26.0)
        .px(10.0)
        .radius(t.radius.control)
        .center()
        .fill(t.surface.hover)
        .hover(t.surface.active)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(text(label.to_owned()).font(t.ty.meta).color(t.text.base).a11y_hidden())
}

/// An affirmative action that is not the region's one primary button: Iris text, no fill, a soft
/// fill under the pointer (Approve on a tool call, beside a primary elsewhere in the region).
fn accent_button(label: &str, key: &str, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .h(26.0)
        .px(10.0)
        .radius(t.radius.control)
        .center()
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(text(label.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.meta }).color(t.meaning.iris).a11y_hidden())
}

/// The page's one action in the web's shape (`Button` at `h-9`, `rounded-full`, `text-sm font-medium`, the plus before the words):
/// 36 high, 14 at the sides, a pill, Iris.
fn page_button(label: &str, key: &str, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    if true {
        return webkit::button(t, key, label, Some(&crate::icons::PLUS), true, false, on_press);
    }
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .h(36.0)
        .px(14.0)
        .gap(6.0)
        .radius(18.0)
        .center()
        .fill(t.meaning.iris)
        .hover(rune::Color::WHITE.alpha(0.14))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(icon(&crate::icons::PLUS, 16.0).color(rune::Color::WHITE.alpha(0.8)).a11y_hidden())
        .child(text(label.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.brand }).color(rune::Color::WHITE).a11y_hidden())
}

/// The one primary action on a view: Iris, the colour of "do this".
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

/// The centred, quiet line a region shows while it has nothing yet.
fn placeholder(lines: &[&str], t: &Neko) -> El {
    let mut c = col().max_w(360.0).px(16.0).gap(6.0).items_center();
    for line in lines {
        c = c.child(text((*line).to_owned()).font(t.ty.meta).color(t.text.faint).wrap());
    }
    col().w_full().grow(1.0).min_h(0.0).center().child(c)
}

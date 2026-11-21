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
mod cats;
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
mod look;
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
use crate::layout::{HANDLE, NEKO_STRIP, Region};
use crate::theme::Neko;

/// The scrim the Home art was drawn under: the window's colour at this strength over this radius. Home draws no art since the prototype's look
/// (2026-11-21); `art.rs` keeps its own contrast test against these figures.
pub fn home_scrim() -> (f32, f32) {
    (0.70, 520.0)
}

/// A cat's right panel: 372 wide (the prototype's `.ov`).
pub const CAT_PANEL: f32 = 372.0;

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
    SIDEBAR_FOLDED.with(|c| c.set(!layout.sessions.is_open()));
    if let Some([w, h]) = cx.layout_size("window") {
        s.window_size.set(Some([w as u32, h as u32]));
    }
    // What fits: the session keeps its minimum, so a small window narrows or hides the others.
    // Before the first layout there is no width to read, and the state's own widths stand.
    let (fit, win_h, win_w) = match cx.layout_size("window") {
        Some([w, h]) => (layout.fit(w), h, w),
        None => (crate::layout::Fit { context: layout.context.shown_width(), neko_open: layout.neko.is_open() }, 840.0, 1360.0),
    };
    // The prototype has no Neko column: the window is the sidebar, the content and (on a cat's page or beside a task) a panel, all flush.
    // The column is still there for the person who opens it (`layout.set_neko_open`), as a fourth region at the right.
    let neko_shown = !layout.neko_off && fit.neko_open;
    let neko_strip = !layout.neko_off && !fit.neko_open;
    let neko_w = if neko_shown { layout.neko.width() } else if neko_strip { NEKO_STRIP } else { 0.0 };
    // A cat's panel is the prototype's 372 (`.ov`); a task's context panel keeps the width the person gave it.
    let side_w = if s.cats.open.is_some() { fit.context.min(CAT_PANEL) } else { fit.context };
    let frame = Frame { win_h, region_w: (win_w - layout.sessions.shown_width() - side_w - neko_w).max(0.0) };

    s.panel.hidden.set(layout.context.is_open() && fit.context <= 0.0);
    // The web view is wanted by whatever shows it below, in this very build (`viewers::web`, `context.rs`, `session.rs`).
    s.web_view.set_want(None);
    // Expanded, the panel takes the session's region and its own; the sessions and Neko columns stay.
    let expanded = s.panel.expanded && fit.context > 0.0;
    // The window itself is the sidebar's colour (the prototype's `--chrome`): the sidebar sits on it, with no card and no gutter.
    let mut window = row().key("window").w_full().h_full().fill(t.surface.sidebar);
    let sessions_w = layout.sessions.shown_width();
    if layout.sessions.is_open() {
        window = window.child(sessions::column(s, &t, &mut layer).w(layout.sessions.width()).key("sessions"));
    }
    // The content is flush: the surface colour to the window's edges, one hairline on its left (painted by the region).
    if expanded {
        window = window.child(context::panel(s, &t, &mut layer, win_w).grow(1.0));
    } else {
        window = window.child(session::region(s, &t, &mut layer, frame));
        if fit.context > 0.0 {
            // On a cat's page the panel is the cat's (Overview, Files, Rounds); otherwise the session's context.
            let side = if s.cats.open.is_some() { cats::panel(s, &t, &mut layer) } else { context::panel(s, &t, &mut layer, fit.context) };
            window = window.child(side.w(side_w).transition(panel_transition()));
        }
    }
    if neko_shown {
        window = window.child(neko::column(s, &t, win_h, &mut layer).w(layout.neko.width()).key("neko").border_sides([0.0, 0.0, 0.0, 1.0], t.surface.border));
    } else if neko_strip {
        window = window.child(neko::strip(s, &t).w(NEKO_STRIP).key("neko").border_sides([0.0, 0.0, 0.0, 1.0], t.surface.border));
    }
    // The edges between regions are 1 px hairlines painted by the regions; what resizes them is a 7 point handle laid over the edge.
    if layout.sessions.is_open() {
        window = window.child(grip(Region::Sessions, &t).absolute(sessions_w - HANDLE / 2.0, 0.0));
    }
    if fit.context > 0.0 && !expanded {
        window = window.child(grip(Region::Context, &t).absolute(win_w - neko_w - side_w - HANDLE / 2.0, 0.0));
    }
    if neko_shown {
        window = window.child(grip(Region::Neko, &t).absolute(win_w - neko_w - HANDLE / 2.0, 0.0));
    }
    palette::mount(s, &t, (win_w, win_h), &mut layer);
    shortcuts::mount(s, &t, &mut layer);
    mount_text_menu(s, &t, &mut layer);
    compose::mount_popover(s, &t, &mut layer);
    compose::mount_message_menu(s, &t, &mut layer);
    runtime::mount(s, &t, &mut layer);
    dialogs::mount(s, &t, &mut layer);
    cats::mount_sheet(s, &t, &mut layer, win_h);
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
        .w(HANDLE)
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

/// Where the "Show sidebar" button's left edge sits, from the window's left: just after the traffic lights (polish spec §0).
pub const SHOW_SIDEBAR_X: f32 = 80.0;
/// How far the session region's own left edge is from the window's (its 2 point margin).
const REGION_LEFT: f32 = 0.0;
/// The gap between the items of a header row.
const BAND_GAP: f32 = 8.0;

thread_local! {
    /// Whether the sessions column is folded in the frame being built: the header rows of the session region
    /// (`band`) read it to make room for, and carry, the button that brings the column back.
    static SIDEBAR_FOLDED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The row across the top of a card region (the session, the context panel): 32 high, 4 below the card's top edge
/// (which is 8 below the window's), so its centre is the sidebar header's. The window can be dragged by it.
/// While the sessions column is folded it starts with the "Show sidebar" button, after the traffic lights.
fn band(t: &Neko) -> El {
    band_px(t, 12.0)
}

/// [`band`] with its own side padding (the session's title line is 20).
fn band_px(t: &Neko, px: f32) -> El {
    band_with(t, px, None)
}

/// The header row, and with it the button that unfolds the sessions column when it is folded. On Home the button is
/// glass (`glass`: how it is made, `None` elsewhere), because it floats over the art with no header bar behind it.
fn band_with(t: &Neko, px: f32, glass: Option<(crate::glass::Mode, bool)>) -> El {
    // The prototype's bar: 48 high on the hairline's line (`.bar`), so its centre is 24 from the window's top.
    let mut row = row().h(48.0).w_full().px(px).gap(BAND_GAP).center_y().window_drag();
    if SIDEBAR_FOLDED.with(std::cell::Cell::get) {
        // The row's own padding and gap are in the way: the spacer makes the button's left edge land on 80.
        row = row.child(el().w(SHOW_SIDEBAR_X - REGION_LEFT - px - BAND_GAP).h(1.0).a11y_hidden()).child(show_sidebar_button(t, glass));
    }
    row
}

/// "Show sidebar": a 28 point quiet icon button (radius 8, the foreground at 4% under the pointer) with the sidebar glyph.
/// On Home it sits on the same glass as the composer (`glass`: its mode, and whether the backing is left out for a probe).
fn show_sidebar_button(t: &Neko, glass: Option<(crate::glass::Mode, bool)>) -> El {
    let button = row()
        .key("sidebar-show")
        .role("button")
        .label("Show sidebar")
        .help("Show sidebar ⌘\\")
        .focusable(true)
        .size(28.0, 28.0)
        .center()
        .radius(crate::glass::BUTTON_RADIUS)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.layout.sessions.set_open(true));
    let button = match glass {
        Some((mode, false)) => {
            let key = if mode == crate::glass::Mode::Blur { crate::glass::BUTTON_GLASS } else { crate::glass::BUTTON_OPAQUE };
            button.child(crate::glass::backing(t, mode, crate::glass::BUTTON_RADIUS, key, false))
        }
        _ => button,
    };
    button.child(icon(&crate::icons::SIDEBAR_LEFT, 16.0).color(t.text.muted).a11y_hidden())
}

/// The row across the top of the Neko column, which is not a card: 32 high at 12 from the window's top (the sidebar
/// header's place), the window can be dragged by it.
fn flat_band(t: &Neko) -> El {
    let _ = t;
    row().h(BAND).pt(12.0).w_full().px(12.0).gap(8.0).center_y().window_drag()
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
        .h(36.0)
        .px(12.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if active { t.surface.selected } else { rune::Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .child(text(label.to_owned()).font(webkit::font(t, 14.0, 400.0, 20.0)).color(if active { t.text.base } else { t.text.base.alpha(0.75) }).truncate().a11y_hidden())
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

/// The one primary action on a view: the ink, near white in dark and near black in light (design system 4.1).
fn primary_button(label: &str, key: &str, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    let r = row().key(key.to_owned()).role("button").label(label).focusable(true).h(28.0).px(12.0).radius(14.0).center().focus_ring(2.0, 2.0, t.kit.c.border_focus).press_scale(0.97).on_press(on_press);
    webkit::ink(t, r).child(text(label.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.mat.ink_fg).a11y_hidden())
}

/// The centred, quiet line a region shows while it has nothing yet.
fn placeholder(lines: &[&str], t: &Neko) -> El {
    let mut c = col().max_w(360.0).px(16.0).gap(6.0).items_center();
    for line in lines {
        c = c.child(text((*line).to_owned()).font(t.ty.meta).color(t.text.faint).wrap());
    }
    col().w_full().grow(1.0).min_h(0.0).center().child(c)
}

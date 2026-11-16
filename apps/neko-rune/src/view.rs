//! The window: four regions, left to right (docs/decisions/0002-rune-for-everything.md). No tabs.
//!
//! 1. **Sessions** (`sessions.rs`): every task across workspaces by state, with the foot items.
//! 2. **Session** (`session.rs`): the selected session, or a full view in its place.
//! 3. **Context** (`context.rs`): what the session produces. Closed until there is something.
//! 4. **Neko** (`neko.rs`): the super agent's column, collapsible to an icon.
//!
//! The edges between regions drag to resize (`layout.rs` holds the widths and ranges). Colours,
//! sizes and fonts come from `theme.rs`; the regions name none.

mod context;
mod neko;
mod new;
mod palette;
mod session;
mod sessions;
mod turn;
mod work;

use rune::{Cursor, Cx, Drag, El, Phase, col, el, icon, row, text};
use rune_kit::root;

use crate::app::App;
use crate::layout::{GRIP, NEKO_STRIP, Region};
use crate::theme::{Neko, theme};

/// One column for full views and the lab (DESIGN.md §8 rule 8).
pub const COLUMN: f32 = 880.0;
/// The height of the band across the top of each region (the window's titlebar height).
pub const BAND: f32 = 44.0;

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let t = theme(cx.system_dark());
    let mut layer = Vec::new();
    let layout = &s.layout;
    // What fits: the session keeps its minimum, so a small window narrows or hides the others.
    // Before the first layout there is no width to read, and the state's own widths stand.
    let (fit, win_h) = match cx.layout_size("window") {
        Some([w, h]) => (layout.fit(w), h),
        None => (crate::layout::Fit { context: layout.context.shown_width(), neko_open: layout.neko.is_open() }, 840.0),
    };

    let mut window = row().key("window").w_full().h_full().fill(t.surface.sidebar);
    window = window.child(sessions::column(s, &t, &mut layer).w(layout.sessions.width()).key("sessions"));
    window = window.child(grip(Region::Sessions, &t));
    // The session and the context panel are two cards in the window, inset from its edges.
    window = window.child(session::region(s, &t, &mut layer));
    if fit.context > 0.0 {
        window = window.child(grip(Region::Context, &t));
        window = window.child(context::panel(s, &t).w(fit.context));
    }
    if fit.neko_open {
        window = window.child(grip(Region::Neko, &t));
        window = window.child(neko::column(s, &t, win_h, &mut layer).w(layout.neko.width()).key("neko"));
    } else {
        window = window.child(neko::strip(s, &t).w(NEKO_STRIP).key("neko"));
    }
    palette::mount(s, &t, &mut layer);
    root(window, layer)
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

/// The band across the top of a region: the window can be dragged by it.
fn band(t: &Neko) -> El {
    let _ = t;
    row().h(BAND).w_full().px(14.0).gap(10.0).center_y().window_drag()
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
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(icon(glyph, 16.0).color(t.text.muted).a11y_hidden())
}

/// A row in a column: an icon, a title, the soft fill when it is the open one.
fn nav_row(glyph: &'static rune::Icon, label: &str, key: &str, active: bool, t: &Neko) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .w_full()
        .h(32.0)
        .px(10.0)
        .gap(10.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if active { t.surface.active } else { rune::Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .child(icon(glyph, 16.0).color(if active { t.text.base } else { t.text.muted }).a11y_hidden())
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

/// A group heading's look for the text sections of a column.
fn heading(label: &str, count: Option<usize>, t: &Neko) -> El {
    let mut h = row().gap(6.0).center_y().child(text(label.to_owned()).font(t.ty.group).color(t.text.muted));
    if let Some(n) = count {
        h = h.child(text(n.to_string()).font(t.ty.id).color(t.text.faint));
    }
    h
}

/// The centred, quiet line a region shows while it has nothing yet.
fn placeholder(lines: &[&str], t: &Neko) -> El {
    let mut c = col().max_w(360.0).px(16.0).gap(6.0).items_center();
    for line in lines {
        c = c.child(text((*line).to_owned()).font(t.ty.meta).color(t.text.faint).wrap());
    }
    col().w_full().grow(1.0).min_h(0.0).center().child(c)
}

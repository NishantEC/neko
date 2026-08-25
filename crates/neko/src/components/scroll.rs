//! Mounting a scrollbar over a list that already scrolls.
//!
//! The scrollbar element itself is vendored
//! (`vendor::gpui_component::scrollbar`); this module owns the one decision
//! that element cannot make for itself — where to hang it.
//!
//! **It must not be a child of the scrolling container, which is how
//! `gpui-component`'s own `ScrollableElement::scrollbar` helper mounts it.**
//! gpui applies a scroll offset by wrapping the container's whole child list in
//! `window.with_element_offset(scroll_offset, ..)`
//! (`gpui/src/elements/div.rs:1851`), and that wrap does not exempt absolutely
//! positioned children. A scrollbar mounted inside would therefore be
//! translated by exactly the amount it exists to report, and would slide off
//! the top of the viewport as soon as anyone scrolled — visible only once the
//! list is long enough to scroll, which is the only time a scrollbar renders at
//! all.
//!
//! So [`with_scrollbar`] wraps: the scrolling element stays a child of a plain
//! `relative` parent, and the bar is an absolutely positioned sibling over it,
//! which puts the bar in the viewport's own coordinate space by construction.
//!
//! The wrapper is transparent to layout — it takes `flex_1` and `min_h(0)` so a
//! list that was filling its parent still fills it, and the vendored element
//! reads its geometry from the [`ScrollHandle`] rather than from the tree, so
//! nothing about the list's own styling has to change to gain a bar.

use gpui::{Div, ElementId, ParentElement, ScrollHandle, Styled, div, px};

use super::vendor::gpui_component::scrollbar::Scrollbar;

/// How wide a lane the bar is given on the right of the list.
///
/// The vendored element draws a 6px thumb inset by 4px, growing to 8px while
/// hovered or dragged; this is that at its widest, so the thumb never lands
/// under the list's own content and the lane never moves as it thickens.
pub const SCROLLBAR_LANE_PX: f32 = 16.0;

/// Puts a vertical scrollbar over `child`.
///
/// `child` is expected to already carry `.overflow_y_scroll().track_scroll(scroll)`
/// — this adds the affordance, never the scrolling itself.
pub fn with_scrollbar(
    scroll: &ScrollHandle,
    id: impl Into<ElementId>,
    child: impl gpui::IntoElement,
) -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        // Without this a flex child refuses to shrink below its content's
        // height, so the list would grow the panel instead of scrolling — the
        // same guard the containers being wrapped already carry.
        .min_h(px(0.))
        .child(child)
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(SCROLLBAR_LANE_PX))
                .child(Scrollbar::vertical(scroll).id(id)),
        )
}

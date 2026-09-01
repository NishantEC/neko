//! [`scroll_edge_fade`] — wraps a scrollable child so its top and/or bottom
//! edge visually dissolves into the panel's own background exactly when
//! there is real, scrolled-out-of-view content at that edge, instead of the
//! list being sliced by a hard cut. A plain overlay gradient painted in
//! this crate's own element tree (two `window.paint_quad` calls with a
//! `gpui::linear_gradient` fill) — `gpui = "0.2.2"` has no `EdgeFade`
//! scene primitive to reach for, and this doesn't need one: the same
//! visual effect is achievable as an ordinary paint-time overlay.
//!
//! **Gated at PAINT time on the child's own [`ScrollHandle`], not at
//! `Render::render` time.** The child's `prepaint` (called just above, in
//! this element's own `prepaint`) is what clamps the scroll handle's
//! offset to a valid range for *this* frame — reading the offset any
//! earlier (e.g. inside `Root::render`, a frame before prepaint runs) would
//! use last frame's value, which goes stale the instant the list's content
//! shrinks while scrolled (an entry deleted via the `⌘K` menu near the
//! bottom of a long scrolled clipboard history, for one real example in
//! this app): the offset gets clamped down to the new, smaller max this
//! frame, but a render-time decision computed before that clamp would still
//! think the old, larger overflow was real and leave a fade with nothing
//! left to scroll to. Painting the overlay directly inside this element's
//! own `paint()`, after the child has already prepainted, reads the
//! corrected value every time.
//!
//! Purely decorative: these quads carry no mouse handlers and register no
//! hitbox, so they never intercept a click meant for a row underneath —
//! gpui only dispatches pointer events to elements that actually installed
//! a hitbox/listener, not to whatever paints last.

use gpui::{
    AnyElement, App, Bounds, Element, GlobalElementId, Hsla, InspectorElementId, IntoElement,
    LayoutId, Pixels, ScrollHandle, Window, fill, linear_color_stop, linear_gradient, point, px, size,
};

/// Wraps `child` (expected to already be `.overflow_y_scroll().track_scroll(scroll)`)
/// so its top/bottom edges fade into `fade_color` exactly when `scroll`
/// reports real overflow at that edge. `band_px` is the ramp height.
pub fn scroll_edge_fade(
    scroll: ScrollHandle,
    fade_color: Hsla,
    band_px: f32,
    child: impl IntoElement,
) -> ScrollEdgeFade {
    ScrollEdgeFade {
        scroll,
        fade_color,
        band_px,
        bottom: true,
        child: child.into_any_element(),
    }
}

pub struct ScrollEdgeFade {
    scroll: ScrollHandle,
    fade_color: Hsla,
    band_px: f32,
    bottom: bool,
    child: AnyElement,
}

impl ScrollEdgeFade {
    /// Drops the bottom fade, for a list whose last pixel is the panel's own
    /// edge.
    ///
    /// **A fade in the panel's own colour cannot sit on a translucent panel
    /// without changing it.** The band is `surface_panel_translucent` painted
    /// *over* `surface_panel_translucent`, so at full strength the composite
    /// is `a + a(1-a)` — 0.64 becomes 0.87 for the `neutral` theme. That
    /// costs two things at once, and both were reported: the bottom band is
    /// measurably less transparent, so the material blurs visibly less there;
    /// and the extra coverage pushes the rounded corner's antialiased edge
    /// outward, which reads as the corner being squarer than the two at the
    /// top. Rounding the quad fixed the shape and could not fix the alpha —
    /// a radius sweep from 1.0x to 1.75x moved the corner not at all.
    ///
    /// There is no version of this that works here. Ending the band above the
    /// corner just moves the 0.64→0.87 step into the middle of the panel,
    /// where it reads as a line rather than as a fade. This file's own
    /// history has the finding from the other side: a fade over a translucent
    /// panel was tried once before and was *invisible*, panel-colour on
    /// panel-colour. It is visible today precisely because it double-opaques.
    ///
    /// So the bottom fade comes off wherever the list meets the panel edge,
    /// and the scrollbar — which did not exist when the fade was added — says
    /// "there is more below" without touching a single pixel's alpha. The top
    /// fade stays (no corner, under a hairline), and so does the transcript's,
    /// where the composer sits below and there is no rounded edge to spoil.
    pub fn without_bottom_fade(mut self) -> Self {
        self.bottom = false;
        self
    }
}

impl Element for ScrollEdgeFade {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);

        // Read fresh, after the child's own prepaint just clamped it for
        // this frame — see this module's own doc comment.
        let scrolled_down = -f32::from(self.scroll.offset().y);
        let max_scroll = f32::from(self.scroll.max_offset().y);
        let (show_top, show_bottom) = edge_fade_visibility(scrolled_down, max_scroll);

        let band = px(self.band_px.min(f32::from(bounds.size.height)));
        let transparent = self.fade_color.opacity(0.0);
        if show_top {
            let quad_bounds = Bounds { origin: bounds.origin, size: size(bounds.size.width, band) };
            window.paint_quad(fill(
                quad_bounds,
                linear_gradient(180.0, linear_color_stop(self.fade_color, 0.0), linear_color_stop(transparent, 1.0)),
            ));
        }
        if show_bottom && self.bottom {
            let quad_bounds = Bounds {
                origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - band),
                size: size(bounds.size.width, band),
            };
            window.paint_quad(fill(
                quad_bounds,
                linear_gradient(180.0, linear_color_stop(transparent, 0.0), linear_color_stop(self.fade_color, 1.0)),
            ));
        }
    }
}

/// Pure decision logic, pulled out of `paint()` so it's unit-testable
/// without a live `Window`/`ScrollHandle`. `scrolled_down` is how far past
/// the top the content is currently scrolled (0 at rest); `max_scroll` is
/// the total distance it could still scroll (0 for content that already
/// fits without scrolling at all). A 1px slack on both edges absorbs
/// float/layout rounding so a list that's scrollable in principle but
/// sitting exactly at rest doesn't flicker a 1px-triggered fade.
fn edge_fade_visibility(scrolled_down: f32, max_scroll: f32) -> (bool, bool) {
    let show_top = scrolled_down > 1.0;
    let show_bottom = scrolled_down < max_scroll - 1.0;
    (show_top, show_bottom)
}

impl IntoElement for ScrollEdgeFade {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_that_meets_the_panel_edge_paints_no_bottom_fade() {
        // The band is the panel's own colour painted over the panel, so at
        // full strength it composites 0.64 to 0.87 — measurably less
        // transparent, and enough extra coverage to push the rounded corner's
        // antialiased edge outward. Not a shape problem: a radius sweep from
        // 1.0x to 1.75x moved the corner not at all.
        let scroll = gpui::ScrollHandle::new();
        let default = scroll_edge_fade(scroll.clone(), gpui::white(), 24.0, gpui::div());
        assert!(default.bottom, "a fade still fades by default");
        let edge = scroll_edge_fade(scroll, gpui::white(), 24.0, gpui::div())
            .without_bottom_fade();
        assert!(!edge.bottom);
    }



    #[test]
    fn a_short_list_that_never_scrolls_shows_no_fade_at_either_edge() {
        assert_eq!(edge_fade_visibility(0.0, 0.0), (false, false));
    }

    #[test]
    fn resting_at_the_top_of_a_long_list_shows_only_the_bottom_fade() {
        assert_eq!(edge_fade_visibility(0.0, 400.0), (false, true));
    }

    #[test]
    fn scrolled_to_the_very_bottom_shows_only_the_top_fade() {
        assert_eq!(edge_fade_visibility(400.0, 400.0), (true, false));
    }

    #[test]
    fn scrolled_partway_through_a_long_list_shows_both_fades() {
        assert_eq!(edge_fade_visibility(150.0, 400.0), (true, true));
    }

    #[test]
    fn sub_pixel_scroll_noise_at_rest_does_not_trigger_a_fade() {
        // Float/layout rounding can leave a fraction-of-a-pixel offset even
        // when the list is visually at rest — the 1px slack absorbs it.
        assert_eq!(edge_fade_visibility(0.4, 400.0), (false, true));
        assert_eq!(edge_fade_visibility(399.7, 400.0), (true, false));
    }
}

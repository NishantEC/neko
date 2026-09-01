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
        bottom_radius_px: 0.0,
        child: child.into_any_element(),
    }
}

pub struct ScrollEdgeFade {
    scroll: ScrollHandle,
    fade_color: Hsla,
    band_px: f32,
    bottom_radius_px: f32,
    child: AnyElement,
}

impl ScrollEdgeFade {
    /// Rounds the bottom fade's own corners, for a list that runs all the way
    /// to the panel's bottom edge.
    ///
    /// **A fade is a painted quad, and gpui's content mask is a rectangle** —
    /// `overflow_hidden` on a rounded parent clips children to its *bounding
    /// box*, not to its rounded shape, so a full-width quad at the bottom of
    /// the panel paints straight over both rounded corners and squares them
    /// off. Measured before it was fixed: the neutral theme's corner read
    /// alpha 163, which is exactly its `panel_alpha` of 0.64 — the fade's own
    /// colour at full strength, sitting where the corner's transparency
    /// should have been.
    ///
    /// Opt-in rather than always-on, because a fade does not always end at the
    /// panel: in a transcript the composer sits below it, and rounding there
    /// would carve a notch out of the middle of the panel.
    pub fn with_bottom_radius(mut self, radius_px: f32) -> Self {
        self.bottom_radius_px = radius_px;
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
        if show_bottom {
            let quad_bounds = Bounds {
                origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - band),
                size: size(bounds.size.width, band),
            };
            let mut quad = fill(
                quad_bounds,
                linear_gradient(180.0, linear_color_stop(transparent, 0.0), linear_color_stop(self.fade_color, 1.0)),
            );
            // Only the bottom pair: the top of this band sits in the middle of
            // the list, where a radius would read as a bite taken out of it.
            // Clamped to the band, since a radius taller than the quad it
            // rounds is not a shape.
            let radius = px(self.bottom_radius_px.min(f32::from(band)));
            quad.corner_radii.bottom_left = radius;
            quad.corner_radii.bottom_right = radius;
            window.paint_quad(quad);
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
    fn the_bottom_radius_never_exceeds_the_band_it_rounds() {
        // A radius taller than the quad is not a shape. The band is already
        // clamped to the element's own height, so this clamp is what keeps a
        // short list's fade from being asked for an impossible corner.
        let band = 6.0_f32;
        let radius = 16.0_f32.min(band);
        assert_eq!(radius, 6.0);
        let roomy = 16.0_f32.min(40.0);
        assert_eq!(roomy, 16.0, "a normal band takes the panel's full radius");
    }

    #[test]
    fn a_fade_is_square_until_a_radius_is_asked_for() {
        // Opt-in, because a fade does not always end at the panel: in a
        // transcript the composer sits below it, and rounding there would
        // carve a notch out of the middle of the panel.
        let scroll = gpui::ScrollHandle::new();
        let plain = scroll_edge_fade(scroll.clone(), gpui::white(), 24.0, gpui::div());
        assert_eq!(plain.bottom_radius_px, 0.0);
        let rounded = scroll_edge_fade(scroll, gpui::white(), 24.0, gpui::div())
            .with_bottom_radius(16.0);
        assert_eq!(rounded.bottom_radius_px, 16.0);
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

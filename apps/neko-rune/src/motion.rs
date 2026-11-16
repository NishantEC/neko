//! The two motions the app has (polish spec §4); nothing else animates.
//!
//! * Hover fills: 120 ms, ease-out. Set once on the window's root with `hover_motion_within`, so every
//!   row, button and field in it follows.
//! * The context panel opening and closing: a 180 ms spring, Rune's Snappy (the kit's `SNAPPY`).
//!
//! Rune's own reduced-motion setting lands both at once, and the home art, the spinner ring and the
//! rotating arc all stop under it as well.

use rune::{Anim, Curve, Easing, Spring};

/// How long a hover fill takes to arrive and to leave.
pub const HOVER_SECONDS: f32 = 0.12;
/// How long the context panel takes to open or close.
pub const PANEL_SECONDS: f32 = 0.18;
/// Rune's Snappy (SwiftUI `.snappy`: a little bounce), over [`PANEL_SECONDS`].
pub const PANEL_BOUNCE: f32 = 0.15;

/// The hover fill's curve: 120 ms ease-out.
pub fn hover() -> Anim {
    Anim::new(Curve::tween(HOVER_SECONDS, Easing::EASE_OUT))
}

/// The panel's spring: 180 ms, Snappy's bounce.
pub fn panel() -> Spring {
    Spring::new(PANEL_SECONDS, PANEL_BOUNCE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_is_120_ms_ease_out_and_the_panel_a_180_ms_snappy_spring() {
        assert_eq!(HOVER_SECONDS, 0.12);
        assert_eq!(hover().curve, Curve::tween(0.12, Easing::EASE_OUT));
        assert_eq!(PANEL_SECONDS, 0.18);
        assert_eq!(PANEL_BOUNCE, Spring::SNAPPY.bounce, "the bounce is Snappy's own");
    }
}

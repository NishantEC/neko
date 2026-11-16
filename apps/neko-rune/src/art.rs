//! Home's art, the CPU side (polish spec §3): the effect slot Home lays out, the colours the shader is given,
//! and the rules that decide whether it moves. The picture itself is drawn by `art_gpu.rs`, one fragment
//! shader inside Rune's UI pass.
//!
//! * It exists behind Home only: the slot is an element of `view/home.rs`, so no other screen has one.
//! * It moves while the window is active and Reduce Motion is off ([`moving`]); otherwise its clock stands
//!   still and the frame is the same picture.
//! * A radial scrim of the window's colour at 70% (radius 520) sits over it, behind the greeting and the
//!   composer; [`tests`] prove the words keep AA contrast over the worst dot.

use rune::{Color, El};

use crate::app::App;
use crate::art_gpu::Look;
use crate::theme::{Neko, theme};

/// The effect slot's tag.
pub const MARK: u32 = 0x4e45_4b01;
/// The slot's key, which the compositor looks the slot up by.
pub const SLOT_KEY: &str = "home-art";

/// The slot: an element that draws nothing itself and keeps its place in the painter stream.
pub fn slot(_s: &App, t: &Neko) -> El {
    rune::effect(MARK).key(SLOT_KEY).effect_data([if t.dark { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0])
}

/// Whether the art's clock runs: the window has the keyboard (an inactive window pauses it) and Reduce Motion is off.
pub fn moving(window_active: bool, reduce_motion: bool) -> bool {
    window_active && !reduce_motion
}

fn linear(c: Color) -> [f32; 3] {
    [c.r, c.g, c.b]
}

/// What a frame of the art is drawn with, for the appearance.
pub fn look(dark: bool, time: f32, scale: f32) -> Look {
    let t = theme(dark);
    // The cat is a darker silhouette than its ground in both: near-black in the night, nearly the ink in the dawn.
    let cat = if dark { t.surface.window.mix(Color::BLACK, 0.7) } else { t.text.strong };
    Look { time, dark, scale, bg: linear(t.surface.window), ink: linear(t.meaning.iris), cat: linear(cat), floor: false }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art_gpu::dot_strength;
    use crate::theme::contrast;

    /// The pixel under a word at its worst: a fully lit dot of the art, under the scrim at its strongest.
    fn worst_pixel(dark: bool) -> Color {
        let t = theme(dark);
        let dot = t.surface.window.mix(t.meaning.iris, dot_strength(dark));
        // The scrim is the window's colour at 70% over the art, blended as CSS alpha is, in gamma-encoded sRGB.
        let mix = |a: f32, b: f32| {
            use rune::color::{srgb_decode, srgb_encode};
            srgb_decode(srgb_encode(b) * 0.7 + srgb_encode(a) * 0.3)
        };
        Color { r: mix(dot.r, t.surface.window.r), g: mix(dot.g, t.surface.window.g), b: mix(dot.b, t.surface.window.b), a: 1.0 }
    }

    #[test]
    fn the_greeting_and_the_composers_words_keep_aa_contrast_over_the_brightest_dot_under_the_scrim() {
        for dark in [true, false] {
            let t = theme(dark);
            let ground = worst_pixel(dark);
            for (name, ink) in [("strong", t.text.strong), ("base", t.text.base), ("muted", t.text.muted)] {
                let c = contrast(ink, ground);
                assert!(c >= 4.5, "{name} on the worst pixel, dark={dark}: {c:.2}");
            }
        }
    }

    #[test]
    fn the_scrim_is_the_windows_colour_at_70_percent_over_a_520_point_radius() {
        assert_eq!(crate::view::home_scrim(), (0.70, 520.0));
    }

    #[test]
    fn the_art_stands_still_when_the_window_is_inactive_or_motion_is_reduced() {
        assert!(moving(true, false));
        assert!(!moving(false, false), "an inactive window pauses it");
        assert!(!moving(true, true), "Reduce Motion stops it completely");
        assert!(!moving(false, true));
    }

    #[test]
    fn dark_is_iris_dots_at_80_and_light_is_dawn_with_iris_at_35() {
        assert_eq!(dot_strength(true), 0.80);
        assert_eq!(dot_strength(false), 0.35, "the light theme draws Iris at 35% strength");
        let (d, l) = (look(true, 0.0, 2.0), look(false, 0.0, 2.0));
        assert!(d.bg[0] < 0.05 && l.bg[0] > 0.9, "near-black ground by night, pale by dawn");
        assert!(d.cat[0] < d.bg[0] + 0.01 && l.cat[0] < 0.1, "the cat is darker than its ground in both");
    }
}

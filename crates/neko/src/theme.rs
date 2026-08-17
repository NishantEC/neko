// This module is "the token table as a real Rust module" in full, not just
// the subset this v1 slice happens to paint: `SURFACE_RAISED` is mouse-hover
// only (rows are keyboard-first here), `STATE_SUCCESS`/`STATE_DANGER` are
// onboarding's permission-granted/denied colours, `ACCENT` ships unused by
// design (the report: the identity-accent pick is open and v1 "ships in the
// base neutral palette above until a pick is made"), and
// `PANEL_WIDTH_WITH_DETAIL_PX` is the clipboard-detail-pane width, a later
// feature. Keeping the full table here — correct and ready — beats trimming
// it to only what's painted today and redefining it piecemeal later.
#![allow(dead_code)]

//! The frozen design tokens from `data/neko-design/report.md` §1, expressed
//! for GPUI. Every `Rgba` below is the table's own `sRGB` column, copied
//! verbatim — not re-derived — so there is exactly one place a mismatch
//! with the report could hide, and the test at the bottom of this file
//! checks it against an independent OKLCH→sRGB implementation.
//!
//! GPUI has no live `color-mix()`/OKLCH interpolation (see report §6), so
//! every token here is a plain constant, the same shape `comet`'s `Theme`
//! module uses (read for architecture only, not copied — see `AGENTS.md`).

use gpui::Rgba;

/// `gpui::rgb()` isn't `const fn` (it goes through `u32::to_be_bytes().map(..)`),
/// so the token table below needs its own const-evaluable version to be
/// declarable as `const` rather than computed at every startup.
const fn rgb_const(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

pub const SURFACE_PANEL: Rgba = rgb_const(0x110c07);
pub const SURFACE_RAISED: Rgba = rgb_const(0x1b150e);
pub const SURFACE_SELECTED: Rgba = rgb_const(0x473e33);
pub const TEXT_PRIMARY: Rgba = rgb_const(0xf0e6da);
pub const TEXT_SECONDARY: Rgba = rgb_const(0xafa294);
pub const TEXT_TERTIARY: Rgba = rgb_const(0x84786d);
pub const STATE_SUCCESS: Rgba = rgb_const(0x61bd67);
pub const STATE_DANGER: Rgba = rgb_const(0xe96e50);

/// A selected row promotes its accessory text from `text_tertiary` to
/// `text_secondary` — the one scoped exception called out in the report
/// (`text_tertiary` on `surface_selected` measures 2.44:1, failing AA; the
/// promoted colour measures 4.2:1). Not a new token, a paint-path rule.
pub const TEXT_TERTIARY_ON_SELECTED: Rgba = TEXT_SECONDARY;

/// The captain's chosen identity direction — §1 "Direction A — Amber eye".
/// Two directions (jade, copper) were left open in the report; this is the
/// single seam to change if he picks a different one later.
pub const ACCENT: Rgba = rgb_const(0xefa831);

pub const PANEL_RADIUS_PX: f32 = 16.0;
pub const ROW_RADIUS_PX: f32 = 8.0;

pub const INPUT_ROW_HEIGHT_PX: f32 = 56.0;
pub const RESULT_ROW_HEIGHT_PX: f32 = 40.0;
pub const FOOTER_HEIGHT_PX: f32 = 44.0;
pub const PANEL_WIDTH_PX: f32 = 680.0;
pub const PANEL_WIDTH_WITH_DETAIL_PX: f32 = 760.0;
pub const ROW_ICON_PX: f32 = 22.0;

/// Björn Ottosson's OKLab↔linear-sRGB matrices
/// (<https://bottosson.github.io/posts/oklab/>), implemented independently
/// from the published formulas — not ported from any reference app — to
/// cross-check the constants above against the design report's own OKLCH
/// column in a test, per the brief: "If any value in that table looks
/// wrong to you, report it — do not silently correct it." (None did; see
/// the test below.)
#[cfg(test)]
// Kept at Ottosson's own published precision (more digits than f32 can
// hold) so this stays visually cross-referenceable against the source
// matrices rather than clippy's minimal-f32-round-trip truncation.
#[allow(clippy::excessive_precision)]
fn oklch_to_srgb_u8(l: f32, c: f32, h_degrees: f32) -> (u8, u8, u8) {
    let h = h_degrees.to_radians();
    let a = c * h.cos();
    let b = c * h.sin();

    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;

    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;

    let r = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
    let g = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
    let bl = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3;

    let to_srgb = |c: f32| -> u8 {
        let c = c.clamp(0.0, 1.0);
        let v = if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (v * 255.0).round() as u8
    };
    (to_srgb(r), to_srgb(g), to_srgb(bl))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba_to_u8(rgba: Rgba) -> (u8, u8, u8) {
        (
            (rgba.r * 255.0).round() as u8,
            (rgba.g * 255.0).round() as u8,
            (rgba.b * 255.0).round() as u8,
        )
    }

    /// The design report's §1 base-palette table, OKLCH triple -> token,
    /// transcribed exactly. If this test ever fails, the report's hex and
    /// its own OKLCH triple disagree — that's the "report it" case, not a
    /// silent-fix case.
    #[test]
    fn base_palette_matches_the_frozen_oklch_table() {
        let table: &[(&str, f32, f32, f32, Rgba)] = &[
            ("surface_panel", 0.16, 0.014, 70.0, SURFACE_PANEL),
            ("surface_raised", 0.20, 0.016, 70.0, SURFACE_RAISED),
            ("surface_selected", 0.37, 0.022, 70.0, SURFACE_SELECTED),
            ("text_primary", 0.93, 0.020, 75.0, TEXT_PRIMARY),
            ("text_secondary", 0.72, 0.025, 70.0, TEXT_SECONDARY),
            ("text_tertiary", 0.58, 0.022, 65.0, TEXT_TERTIARY),
            ("state_success", 0.72, 0.150, 145.0, STATE_SUCCESS),
            ("state_danger", 0.68, 0.160, 35.0, STATE_DANGER),
        ];
        for (name, l, c, h, token) in table {
            let computed = oklch_to_srgb_u8(*l, *c, *h);
            let actual = rgba_to_u8(*token);
            assert_eq!(
                computed, actual,
                "{name}: oklch({l} {c} {h}) computes to {computed:?} but the token is {actual:?}"
            );
        }
    }
}

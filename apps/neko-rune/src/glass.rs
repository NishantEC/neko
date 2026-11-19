//! The frosted glass of Home (polish spec §2): the composer card and the floating "Show sidebar" button are a blur
//! of the art behind them under a tint, with the words and controls drawn above at full strength.
//!
//! The blur is Rune's own backdrop blur (`El::backdrop_blur`, `rune/src/render/backdrop_blur.rs`): at the element it
//! copies what the window has drawn so far (the art's effect slot included), blurs it at half resolution inside the
//! element's rect, and composites the result inside its rounded corners; the element's fill then draws over the blur
//! as the tint, and its border and children on top. Nothing here draws a pixel itself, so the card is clipped to its
//! radius by construction and the words above it are never blurred or dithered.
//!
//! Only Home uses it. Under Reduce Transparency, or when the window's target cannot be copied (`App::backdrop_ok`),
//! the same backing is opaque: the window's colour with the foreground at 4%, with the same border.

use rune::{Color, El, el};

use crate::theme::Neko;

/// The blur's standard deviation in logical points (Rune scales it by the window's scale).
pub const SIGMA: f32 = 20.0;
/// The composer card's corner radius; the floating button's.
pub const CARD_RADIUS: f32 = 16.0;
pub const BUTTON_RADIUS: f32 = 8.0;
/// The tint over the blur: the window's colour at this strength, dark and light.
pub const TINT_DARK: f32 = 0.55;
pub const TINT_LIGHT: f32 = 0.60;
/// The 1 px border, and the 1 px inner highlight along the top edge, as shares of the foreground.
pub const BORDER: f32 = 0.10;
pub const HIGHLIGHT: f32 = 0.06;
/// The foreground's share laid on the window's colour for the opaque fallback.
pub const OPAQUE_SHARE: f32 = 0.04;
/// The greeting's and the status line's soft shadow: black at 35% (dark) or white at 50% (light), CSS blur 8 (sigma 4), 1 down.
pub const TEXT_SHADOW_BLUR: f32 = 8.0;
pub const TEXT_SHADOW_DARK: f32 = 0.35;
pub const TEXT_SHADOW_LIGHT: f32 = 0.50;

/// Keys of the backing layer: tests tell glass from its opaque fallback by these.
pub const CARD_GLASS: &str = "composer-glass";
pub const CARD_OPAQUE: &str = "composer-opaque";
pub const BUTTON_GLASS: &str = "sidebar-show-glass";
pub const BUTTON_OPAQUE: &str = "sidebar-show-opaque";

/// Whether the system asks for less transparency (macOS Accessibility > Display > Reduce transparency).
/// `NEKO_RUNE_REDUCE_TRANSPARENCY=1` reports it on, for captures and checks, as Rune's `RUNE_INCREASE_CONTRAST` does.
pub fn system_reduce_transparency() -> bool {
    if std::env::var_os("NEKO_RUNE_REDUCE_TRANSPARENCY").is_some_and(|v| v == "1") {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        use objc2::{class, msg_send};
        // SAFETY: NSWorkspace's shared instance lives for the process; the selector returns an Objective-C BOOL.
        // The window's view is built on the UI thread (Rune reads its own display options the same way).
        unsafe {
            let workspace: *mut objc2::runtime::AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let reduced: objc2::runtime::Bool = msg_send![workspace, accessibilityDisplayShouldReduceTransparency];
            reduced.as_bool()
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// How a glass surface is made now: a blur under a tint, or the opaque card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Blur,
    Opaque,
}

impl Mode {
    pub fn pick(reduce_transparency: bool, backdrop_ok: bool) -> Mode {
        if reduce_transparency || !backdrop_ok { Mode::Opaque } else { Mode::Blur }
    }
}

/// The tint laid over the blur: the window's colour at 55% in dark, 60% in light.
pub fn tint(t: &Neko) -> Color {
    t.surface.window.alpha(if t.dark { TINT_DARK } else { TINT_LIGHT })
}

/// The opaque fallback's fill: the window's colour with the foreground at 4% on it.
pub fn opaque_fill(t: &Neko) -> Color {
    t.surface.window.mix(t.text.base, OPAQUE_SHARE).alpha(1.0)
}

/// The border of a glass surface, and its inner top highlight.
pub fn border(t: &Neko) -> Color {
    t.text.base.alpha(BORDER)
}

pub fn highlight(t: &Neko) -> Color {
    t.text.base.alpha(HIGHLIGHT)
}

/// The shadow under the greeting's and the status line's words.
pub fn text_shadow(t: &Neko) -> Color {
    if t.dark { Color::BLACK.alpha(TEXT_SHADOW_DARK) } else { Color::WHITE.alpha(TEXT_SHADOW_LIGHT) }
}

/// The backing of a glass surface: an element that fills its parent, blurs what is behind it (or is opaque), tints it,
/// and carries the 1 px border and the inner top highlight. Put it first in the parent, so what follows draws above it.
pub fn backing(t: &Neko, mode: Mode, radius: f32, key: &str, with_highlight: bool) -> El {
    let mut layer = el().key(key.to_owned()).pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(radius).border(1.0, border(t)).a11y_hidden();
    layer = match mode {
        Mode::Blur => layer.backdrop_blur(SIGMA).fill(tint(t)),
        Mode::Opaque => layer.fill(opaque_fill(t)),
    };
    if with_highlight {
        // Along the straight part of the top edge, inside the border.
        layer = layer.child(el().key(format!("{key}-highlight")).pin(Some(radius), Some(0.0), Some(radius), None).h(1.0).fill(highlight(t)).a11y_hidden());
    }
    layer
}

/// Words with a soft shadow for legibility over the art: `shade`, a copy of the text without its key, drawn first in the
/// shadow's colour, blurred and 1 point lower, then `words` themselves on top.
pub fn shadowed(t: &Neko, shade: El, words: El) -> El {
    let shade = shade.key("glass-shade").color(text_shadow(t)).blur(TEXT_SHADOW_BLUR / 2.0).pin(Some(0.0), Some(1.0), None, None).a11y_hidden();
    rune::col().child(shade).child(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::theme;

    #[test]
    fn the_tint_is_the_windows_colour_at_55_percent_in_dark_and_60_in_light() {
        let (d, l) = (theme(true), theme(false));
        assert_eq!(tint(&d).a, 0.55);
        assert_eq!(tint(&l).a, 0.60);
        assert_eq!(tint(&d).hex()[..7], d.surface.window.hex()[..7]);
        assert_eq!(tint(&l).hex()[..7], l.surface.window.hex()[..7]);
    }

    #[test]
    fn the_border_is_the_foreground_at_10_and_the_highlight_at_6() {
        for dark in [true, false] {
            let t = theme(dark);
            assert_eq!(border(&t).a, 0.10);
            assert_eq!(highlight(&t).a, 0.06);
            assert_eq!(border(&t).hex()[..7], t.text.base.hex()[..7]);
        }
    }

    #[test]
    fn the_fallback_is_opaque_and_the_shadow_is_black_by_night_and_white_by_day() {
        for dark in [true, false] {
            let t = theme(dark);
            assert_eq!(opaque_fill(&t).a, 1.0);
            assert_ne!(opaque_fill(&t).hex(), t.surface.window.hex(), "the foreground's 4% shows");
        }
        assert_eq!((text_shadow(&theme(true)).r, text_shadow(&theme(true)).a), (0.0, 0.35));
        assert_eq!((text_shadow(&theme(false)).r, text_shadow(&theme(false)).a), (1.0, 0.50));
    }

    #[test]
    fn reduced_transparency_or_a_target_that_cannot_be_copied_is_the_opaque_card() {
        assert_eq!(Mode::pick(false, true), Mode::Blur);
        assert_eq!(Mode::pick(true, true), Mode::Opaque);
        assert_eq!(Mode::pick(false, false), Mode::Opaque);
        assert_eq!(Mode::pick(true, false), Mode::Opaque);
    }
}

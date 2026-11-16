//! The Neko theme in Rune: DESIGN.md's tokens as one module, light and dark.
//!
//! Values come from the web UI's default themes (`web/src/themes/builtin/neutral*.json`), so the
//! two clients look alike while both exist. Nothing outside this module names a colour, a radius
//! or a font size (DESIGN.md §2, §7, §8 rule 7); screens read a [`Neko`].
//!
//! * Surfaces: window, card, sidebar, popover. One border tone, one large window radius.
//! * Text in three greys: base, muted, faint.
//! * Colour only for meaning, one meaning per colour: [`Meaning`] (Iris, Amber, Sky, Mint, Coral).
//! * The type scale 20 / 13.5 / 12.5 / 12 as [`NekoType`].
//!
//! `rune::hex` reads sRGB hex into Rune's linear colours, so the tokens render as the web shows
//! them. [`Neko::kit`] is a `rune_kit::Kit` built from the same tokens, for the kit's components.

use std::rc::Rc;

use rune::{Color, TextStyle};
use rune_kit::{Kit, PaletteSeed};

/// The hex values of one appearance, as the web's theme data writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hex {
    pub background: &'static str,
    pub card: &'static str,
    pub popover: &'static str,
    pub sidebar: &'static str,
    pub foreground: &'static str,
    pub muted: &'static str,
    /// Iris: the primary action, focus, selection, now, references.
    pub iris: &'static str,
    /// Amber: needs you, in progress.
    pub amber: &'static str,
    /// Sky: ready to review.
    pub sky: &'static str,
    /// Mint: done, calendar events.
    pub mint: &'static str,
    /// Coral: failed, urgent.
    pub coral: &'static str,
}

/// `web/src/themes/builtin/neutral.json` ("Neko Dark").
pub const DARK: Hex = Hex {
    background: "#0f0f0f",
    card: "#151515",
    popover: "#181818",
    sidebar: "#121212",
    foreground: "#d7d7d7",
    muted: "#8f8f8f",
    iris: "#346bf1",
    amber: "#e0a042",
    sky: "#2393f1",
    mint: "#4cc38a",
    coral: "#e5484d",
};

/// `web/src/themes/builtin/neutral-light.json` ("Neko Light").
pub const LIGHT: Hex = Hex {
    background: "#fafafa",
    card: "#ffffff",
    popover: "#ffffff",
    sidebar: "#f3f3f3",
    foreground: "#262626",
    muted: "#636363",
    iris: "#346bf1",
    amber: "#e19b1b",
    sky: "#0083e0",
    mint: "#00af6f",
    coral: "#e6293f",
};

/// Share of the muted grey that is left in the faint grey (the web's `muted-foreground/70`).
const FAINT_SHARE: f32 = 0.7;

/// Surfaces. Every page sits on `window`; a card is a step off it (light) or the same
/// near-black (dark), never ringed with a border (DESIGN.md §8 rule 10).
#[derive(Clone, Copy, Debug)]
pub struct Surfaces {
    pub window: Color,
    pub card: Color,
    pub sidebar: Color,
    pub popover: Color,
    /// The one border tone: the foreground at 12%.
    pub border: Color,
    /// A row under the pointer: the foreground at 3%.
    pub hover: Color,
    /// The active tab and the open rail place: a soft fill, the foreground at 8%.
    pub active: Color,
}

/// Text in three greys (DESIGN.md §2).
#[derive(Clone, Copy, Debug)]
pub struct Greys {
    pub base: Color,
    pub muted: Color,
    pub faint: Color,
}

/// Colour that carries meaning, one meaning per colour (DESIGN.md §2). Always paired with text
/// or a symbol; the status glyphs are the main user.
#[derive(Clone, Copy, Debug)]
pub struct Meaning {
    pub iris: Color,
    pub amber: Color,
    pub sky: Color,
    pub mint: Color,
    pub coral: Color,
}

/// Radii: one large window radius, rows and controls smaller (DESIGN.md §8 rule 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    pub window: f32,
    pub row: f32,
    pub control: f32,
}

/// The type scale (DESIGN.md §8 rule 7): page title 20 semibold; group heading 12.5 medium muted;
/// row title 13.5 regular; meta 12.5 muted; ids 12 faint tabular. Bold is for the page title only.
#[derive(Clone, Copy, Debug)]
pub struct NekoType {
    pub title: TextStyle,
    pub group: TextStyle,
    pub row: TextStyle,
    pub meta: TextStyle,
    pub id: TextStyle,
    /// Commands, output and ids of files: the mono face at the id size.
    pub mono: TextStyle,
}

impl NekoType {
    pub const SIZES: [f32; 4] = [20.0, 13.5, 12.5, 12.0];

    fn new() -> Self {
        NekoType {
            title: TextStyle::new(20.0, 600.0).lh(28.0),
            group: TextStyle::new(12.5, 500.0).lh(18.0),
            row: TextStyle::new(13.5, 400.0).lh(20.0),
            meta: TextStyle::new(12.5, 400.0).lh(18.0),
            id: TextStyle { features: "'tnum' 1", ..TextStyle::new(12.0, 400.0).lh(16.0) },
            mono: TextStyle { family: rune::Family::Mono, ..TextStyle::new(12.0, 400.0).lh(17.0) },
        }
    }
}

/// One appearance of the Neko theme.
pub struct Neko {
    pub dark: bool,
    pub hex: Hex,
    pub surface: Surfaces,
    pub text: Greys,
    pub meaning: Meaning,
    pub radius: Radii,
    pub ty: NekoType,
    /// The kit's components (buttons, menus, tabs, dialogs) read their tokens from this.
    pub kit: Kit,
}

fn c(hex: &str) -> Color {
    rune::color::hex(hex).unwrap_or_else(|| panic!("a theme colour that is not hex: {hex}"))
}

/// `share` of `over` laid on `under`, blended in gamma-encoded sRGB the way CSS alpha is.
fn mix_srgb(under: Color, over: Color, share: f32) -> Color {
    use rune::color::{srgb_decode, srgb_encode};
    let m = |a: f32, b: f32| srgb_decode(srgb_encode(a) * (1.0 - share) + srgb_encode(b) * share);
    Color { r: m(under.r, over.r), g: m(under.g, over.g), b: m(under.b, over.b), a: 1.0 }
}

impl Neko {
    pub fn build(dark: bool) -> Neko {
        let hex = if dark { DARK } else { LIGHT };
        let window = c(hex.background);
        let fg = c(hex.foreground);
        let muted = c(hex.muted);
        let ink = |a: f32| fg.alpha(a);
        let surface = Surfaces {
            window,
            card: c(hex.card),
            sidebar: c(hex.sidebar),
            popover: c(hex.popover),
            border: ink(0.12),
            hover: ink(0.03),
            active: ink(0.08),
        };
        // The faint grey is the muted grey thinned toward the window, an opaque colour so it
        // never changes with what is under it. Blended in sRGB, as the web's `/70` is.
        let faint = mix_srgb(window, muted, FAINT_SHARE);
        let text = Greys { base: fg, muted, faint };
        let meaning = Meaning { iris: c(hex.iris), amber: c(hex.amber), sky: c(hex.sky), mint: c(hex.mint), coral: c(hex.coral) };
        let radius = Radii { window: 20.0, row: 10.0, control: 8.0 };
        let ty = NekoType::new();

        let mut kit = Kit::from_palette(PaletteSeed { bg: window, fg, accent: meaning.iris });
        kit.c.text_muted = muted;
        kit.c.text_faint = faint;
        kit.c.icon_muted = muted;
        kit.c.icon_faint = faint;
        kit.c.border_base = surface.border;
        kit.c.hover = surface.hover;
        kit.c.bg_base = surface.card;
        kit.c.bg_deep = window;
        for (state, role) in [
            (&mut kit.c.success, meaning.mint),
            (&mut kit.c.warning, meaning.amber),
            (&mut kit.c.danger, meaning.coral),
            (&mut kit.c.info, meaning.sky),
        ] {
            state.fg = role;
            state.icon = role;
        }
        kit.radius = rune_kit::Radius { xs: 4.0, sm: 6.0, md: radius.control, lg: radius.row, xl: radius.window };
        // The kit's own scale follows ours, so its buttons and menus read as the same product.
        kit.ty.ui = TextStyle { size: ty.row.size, ..kit.ty.ui };
        kit.ty.ui_medium = TextStyle { size: ty.row.size, weight: 500.0, ..kit.ty.ui_medium };
        kit.ty.ui_compact = TextStyle { size: ty.row.size, ..kit.ty.ui_compact };
        kit.ty.text_small = TextStyle { size: ty.meta.size, ..kit.ty.text_small };
        kit.ty.text_small_medium = TextStyle { size: ty.meta.size, weight: 500.0, ..kit.ty.text_small_medium };
        kit.ty.text_xsmall = TextStyle { size: ty.meta.size, ..kit.ty.text_xsmall };
        kit.ty.text_xsmall_medium = TextStyle { size: ty.meta.size, weight: 500.0, ..kit.ty.text_xsmall_medium };
        kit.ty.small = TextStyle { size: ty.id.size, ..kit.ty.small };
        kit.ty.small_medium = TextStyle { size: ty.id.size, weight: 500.0, ..kit.ty.small_medium };

        Neko { dark, hex, surface, text, meaning, radius, ty, kit }
    }
}

thread_local! {
    static THEMES: [Rc<Neko>; 2] = [Rc::new(Neko::build(false)), Rc::new(Neko::build(true))];
}

/// The theme for an appearance, built once per thread. Views take `cx.system_dark()`, so the
/// window follows the system's light or dark without a setting.
pub fn theme(dark: bool) -> Rc<Neko> {
    THEMES.with(|t| t[dark as usize].clone())
}

/// WCAG contrast ratio of two opaque colours.
pub fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (a.luminance(), b.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_hex(h: &Hex) -> [&'static str; 11] {
        [h.background, h.card, h.popover, h.sidebar, h.foreground, h.muted, h.iris, h.amber, h.sky, h.mint, h.coral]
    }

    #[test]
    fn every_token_is_valid_hex() {
        for h in [DARK, LIGHT] {
            for v in all_hex(&h) {
                assert!(rune::color::hex(v).is_some(), "{v}");
            }
        }
    }

    #[test]
    fn the_two_appearances_follow_the_web_defaults() {
        let (d, l) = (theme(true), theme(false));
        assert!(d.dark && !l.dark);
        assert_eq!(d.surface.window.hex(), "#0f0f0fff");
        assert_eq!(l.surface.window.hex(), "#fafafaff");
        assert_eq!(d.meaning.mint.hex(), "#4cc38aff");
        assert_eq!(l.meaning.coral.hex(), "#e6293fff");
        // The window is darker than its text in dark, lighter in light.
        assert!(d.surface.window.luminance() < d.text.base.luminance());
        assert!(l.surface.window.luminance() > l.text.base.luminance());
    }

    #[test]
    fn text_greys_stay_readable_and_ordered() {
        for dark in [true, false] {
            let t = theme(dark);
            let bg = t.surface.window;
            let (base, muted, faint) = (contrast(t.text.base, bg), contrast(t.text.muted, bg), contrast(t.text.faint, bg));
            assert!(base >= 7.0, "base {base:.2} dark={dark}");
            assert!(muted >= 4.5, "muted {muted:.2} dark={dark}");
            assert!(faint >= 3.0, "faint {faint:.2} dark={dark}");
            assert!(base > muted && muted > faint, "three greys, in order");
        }
    }

    #[test]
    fn meanings_are_distinct_and_visible_on_the_window() {
        for dark in [true, false] {
            let t = theme(dark);
            let m = [t.meaning.iris, t.meaning.amber, t.meaning.sky, t.meaning.mint, t.meaning.coral];
            for (i, a) in m.iter().enumerate() {
                // The web's own amber is 2.3:1 on the light window, which is why colour is never
                // alone: a glyph or words always go with it.
                assert!(contrast(*a, t.surface.window) >= 2.0, "meaning {i} dark={dark}");
                for b in &m[i + 1..] {
                    assert_ne!(a.hex(), b.hex());
                }
            }
        }
    }

    #[test]
    fn the_type_scale_is_20_13_5_12_5_12() {
        let t = theme(true).ty;
        assert_eq!([t.title.size, t.row.size, t.meta.size, t.id.size], NekoType::SIZES);
        assert_eq!(t.title.weight, 600.0, "bold is for the page title only");
        assert!(t.row.weight < 500.0 && t.meta.weight < 500.0 && t.id.weight < 500.0);
        assert!(t.id.features.contains("tnum"), "ids are tabular");
    }

    #[test]
    fn the_kit_uses_the_same_tokens() {
        for dark in [true, false] {
            let t = theme(dark);
            assert_eq!(t.kit.c.text_muted.hex(), t.text.muted.hex());
            assert_eq!(t.kit.c.success.fg.hex(), t.meaning.mint.hex());
            assert_eq!(t.kit.c.danger.fg.hex(), t.meaning.coral.hex());
            assert_eq!(t.kit.radius.xl, t.radius.window);
            assert_eq!(t.kit.ty.ui.size, 13.5);
        }
    }
}

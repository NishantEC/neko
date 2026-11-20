//! The Neko theme in Rune: DESIGN.md's tokens as one module, light and dark.
//!
//! Values come from the web UI's default themes (`web/src/themes/builtin.generated.css`, built from
//! `neutral*.json`), exactly, so the two clients look alike while both exist: the test at the foot of
//! this file parses that CSS and compares every token (polish spec §0). Nothing outside this module names a colour, a radius
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hex {
    pub background: &'static str,
    pub card: &'static str,
    pub popover: &'static str,
    /// The web's own sidebar value. The window paints the sidebar at the background plus 2% lightness
    /// (polish spec §4), see [`Surfaces::sidebar`]; this stays so the table matches the CSS.
    pub sidebar: &'static str,
    pub foreground: &'static str,
    /// `--strong`: headings and the strongest text.
    pub strong: &'static str,
    pub muted: &'static str,
    /// `--border` is a colour at an alpha: this is its colour, [`Hex::border_alpha`] its opacity.
    pub border: &'static str,
    pub border_alpha: f32,
    /// `--code`: the fill behind code.
    pub code: &'static str,
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

/// `html[data-neko-theme="neutral"]` in `web/src/themes/builtin.generated.css` ("Neko Dark").
pub const DARK: Hex = Hex {
    background: "#161616",
    card: "#1b1b1b",
    popover: "#1b1b1b",
    sidebar: "#111111",
    foreground: "#f5f5f5",
    strong: "#ffffff",
    muted: "#838383",
    border: "#ffffff",
    border_alpha: 0.06,
    code: "#1b1b1b",
    iris: "#346bf1",
    amber: "#e0a042",
    sky: "#2393f1",
    mint: "#4cc38a",
    coral: "#e5484d",
};

/// `html[data-neko-theme="neutral-light"]` ("Neko Light").
pub const LIGHT: Hex = Hex {
    background: "#ffffff",
    card: "#ffffff",
    popover: "#ffffff",
    sidebar: "#fafafa",
    foreground: "#262626",
    strong: "#171717",
    muted: "#686868",
    border: "#000000",
    border_alpha: 0.08,
    code: "#ffffff",
    iris: "#346bf1",
    amber: "#e19b1b",
    sky: "#0083e0",
    mint: "#00af6f",
    coral: "#e6293f",
};

/// The web's `--font-sans` with its first face, `"Inter Variable"` (the web's bundled build), read as the
/// `Inter` Rune embeds. The rest of the list is the web's, verbatim.
pub const FONT_SANS: &str = "Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, \"SF Pro Text\", system-ui, sans-serif";
/// The web's `--font-mono`, verbatim.
pub const FONT_MONO: &str = "ui-monospace, \"SF Mono\", \"JetBrains Mono\", Menlo, monospace";

/// Share of the muted grey that is left in the faint grey. The web's `muted-foreground/70` is 0.70; 0.72 is the smallest step that keeps the
/// faint grey at 3.0:1 on both windows now that the muted greys are #838383 and #686868.
const FAINT_SHARE: f32 = 0.72;

/// Surfaces. Every page sits on `window`; a card is a step off it (light) or the same
/// near-black (dark), never ringed with a border (DESIGN.md §8 rule 10).
#[derive(Clone, Copy, Debug)]
pub struct Surfaces {
    pub window: Color,
    pub card: Color,
    pub sidebar: Color,
    pub popover: Color,
    /// The one border tone: the web's `--border` (white 7% in dark, black 8% in light).
    pub border: Color,
    /// A row under the pointer: the foreground at 4% (polish spec §1).
    pub hover: Color,
    /// The active tab and the open rail place: a soft fill, the foreground at 7%.
    pub active: Color,
    /// The open row of a list: the foreground at 7%.
    pub selected: Color,
    /// The sidebar's field: a 1 px border at the foreground's 10% (polish spec §1).
    pub field_border: Color,
    /// The focus ring: Iris at 60% (polish spec §4).
    pub ring: Color,
}

/// Text in three greys (DESIGN.md §2).
#[derive(Clone, Copy, Debug)]
pub struct Greys {
    /// `--strong`: the strongest text (headings, the greeting).
    pub strong: Color,
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

/// Radii: one large window radius, rows and controls smaller (DESIGN.md §8 rule 2). The web's
/// `--radius` is 1.25rem = 20; a row is 8 (polish spec §1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    pub window: f32,
    pub row: f32,
    pub control: f32,
}

/// The materials of the 2026-11 design system (`docs/design/system-2026-11.md` section 2.2, `web/src/styles/sky-materials.css`):
/// the pebble (a secondary control) and the ink (the primary one) are gradients a few percent apart with a lit top edge, an input
/// is a well with an inner shadow, a settings group is a slab, and the sidebar and the composer are tinted glass. Neko's own two
/// themes use the spec's literals; any other theme gets the same materials built from its own tokens, so it keeps its palette.
#[derive(Clone, Copy, Debug)]
pub struct Materials {
    pub pebble_top: Color,
    pub pebble_bottom: Color,
    /// The words on a pebble.
    pub pebble_fg: Color,
    /// The 1 px lit top edge (`inset 0 1px 0`).
    pub pebble_edge: Color,
    pub ink_top: Color,
    pub ink_bottom: Color,
    /// The words on the ink.
    pub ink_fg: Color,
    pub ink_edge: Color,
    /// The 1 px ring inside the ink's edge (`inset 0 0 0 1px`).
    pub ink_ring: Color,
    /// An input's or a track's fill, and the shadow inside its top edge (`inset 0 1px 2px`).
    pub well: Color,
    pub well_shadow: Color,
    /// A settings slab's fill, its hover, and the hairline between its rows.
    pub group: Color,
    pub group_hover: Color,
    pub divider: Color,
    /// The drop shadow of a floating layer (`0 24 60 -18` and `0 2 6`).
    pub drop_far: Color,
    pub drop_near: Color,
    /// Glass (the composer): the tint from the top edge to the bottom one.
    pub glass_top: Color,
    pub glass_bottom: Color,
    /// The sidebar panel's tint, top, middle (at 55%) and bottom.
    pub panel_top: Color,
    pub panel_mid: Color,
    pub panel_bottom: Color,
    /// A line icon: the foreground at about 46%.
    pub icon: Color,
    /// A switch's thumb when off, and when on.
    pub thumb_off: Color,
    pub thumb_on: Color,
}

impl Materials {
    fn of(dark: bool, hex: &Hex, window: Color, card: Color, fg: Color, sidebar: Color) -> Materials {
        let k = |r: u8, g: u8, b: u8, a: f32| rgb8(r, g, b).alpha(a);
        if *hex == DARK {
            return Materials {
                pebble_top: c("#232326"),
                pebble_bottom: c("#1d1d20"),
                pebble_fg: c("#fafafa"),
                pebble_edge: k(255, 255, 255, 0.07),
                ink_top: c("#ffffff"),
                ink_bottom: c("#ececec"),
                ink_fg: c("#171717"),
                ink_edge: Color::WHITE,
                ink_ring: k(255, 255, 255, 0.4),
                well: k(255, 255, 255, 0.05),
                well_shadow: k(0, 0, 0, 0.35),
                group: c("#222224"),
                group_hover: c("#2b2b2d"),
                divider: k(255, 255, 255, 0.07),
                drop_far: k(0, 0, 0, 0.65),
                drop_near: k(0, 0, 0, 0.30),
                glass_top: k(23, 23, 23, 0.55),
                glass_bottom: k(23, 23, 23, 0.38),
                panel_top: k(38, 38, 42, 0.72),
                panel_mid: k(24, 24, 27, 0.66),
                panel_bottom: k(18, 18, 20, 0.60),
                icon: k(242, 242, 243, 0.42),
                thumb_off: c("#6b6b73"),
                thumb_on: c("#141416"),
            };
        }
        if *hex == LIGHT {
            return Materials {
                pebble_top: c("#f7f7f8"),
                pebble_bottom: c("#ebebed"),
                pebble_fg: c("#171717"),
                pebble_edge: Color::WHITE,
                ink_top: c("#3d3d3d"),
                ink_bottom: c("#1c1c1c"),
                ink_fg: c("#fafafa"),
                ink_edge: k(255, 255, 255, 0.16),
                ink_ring: k(0, 0, 0, 0.5),
                well: k(0, 0, 0, 0.045),
                well_shadow: k(0, 0, 0, 0.08),
                group: c("#f6f7f9"),
                group_hover: c("#eff1f4"),
                divider: k(0, 0, 0, 0.06),
                drop_far: k(0, 0, 0, 0.22),
                drop_near: k(0, 0, 0, 0.05),
                glass_top: k(255, 255, 255, 0.62),
                glass_bottom: k(255, 255, 255, 0.36),
                panel_top: k(232, 240, 255, 0.75),
                panel_mid: k(238, 243, 255, 0.675),
                panel_bottom: k(244, 247, 255, 0.60),
                icon: k(27, 27, 29, 0.46),
                thumb_off: Color::WHITE,
                thumb_on: Color::WHITE,
            };
        }
        // Any other theme: the same materials from its own tokens (the generic block of `sky-materials.css`).
        let (white, black) = (Color::WHITE, Color::BLACK);
        let pebble_fg = fg;
        if dark {
            Materials {
                pebble_top: mix_srgb(window, fg, 0.09),
                pebble_bottom: mix_srgb(window, fg, 0.06),
                pebble_fg,
                pebble_edge: k(255, 255, 255, 0.07),
                ink_top: mix_srgb(white, fg, 0.40),
                ink_bottom: mix_srgb(black, fg, 0.96),
                ink_fg: window,
                ink_edge: white,
                ink_ring: k(255, 255, 255, 0.4),
                well: fg.alpha(0.05),
                well_shadow: k(0, 0, 0, 0.35),
                group: mix_srgb(card, fg, 0.04),
                group_hover: mix_srgb(card, fg, 0.08),
                divider: k(255, 255, 255, 0.07),
                drop_far: k(0, 0, 0, 0.65),
                drop_near: k(0, 0, 0, 0.30),
                glass_top: card.alpha(0.80),
                glass_bottom: card.alpha(0.60),
                panel_top: mix_srgb(sidebar, fg, 0.03),
                panel_mid: mix_srgb(sidebar, fg, 0.03),
                panel_bottom: mix_srgb(sidebar, fg, 0.03),
                icon: fg.alpha(0.42),
                thumb_off: fg,
                thumb_on: window,
            }
        } else {
            Materials {
                pebble_top: mix_srgb(card, fg, 0.025),
                pebble_bottom: mix_srgb(card, fg, 0.07),
                pebble_fg,
                pebble_edge: k(255, 255, 255, 0.9),
                ink_top: mix_srgb(fg, white, 0.14),
                ink_bottom: mix_srgb(fg, black, 0.28),
                ink_fg: window,
                ink_edge: k(255, 255, 255, 0.16),
                ink_ring: k(0, 0, 0, 0.5),
                well: fg.alpha(0.045),
                well_shadow: k(0, 0, 0, 0.08),
                group: mix_srgb(card, fg, 0.03),
                group_hover: mix_srgb(card, fg, 0.06),
                divider: k(0, 0, 0, 0.06),
                drop_far: k(0, 0, 0, 0.22),
                drop_near: k(0, 0, 0, 0.05),
                glass_top: k(255, 255, 255, 0.62),
                glass_bottom: k(255, 255, 255, 0.36),
                panel_top: mix_srgb(sidebar, fg, 0.03),
                panel_mid: mix_srgb(sidebar, fg, 0.03),
                panel_bottom: mix_srgb(sidebar, fg, 0.03),
                icon: fg.alpha(0.46),
                thumb_off: white,
                thumb_on: white,
            }
        }
    }
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
    /// The sidebar's brand line: "Neko" 14 semibold.
    pub brand: TextStyle,
    /// Home's greeting: 34 semibold.
    pub greeting: TextStyle,
    /// A faint shortcut hint: 12 with tabular numbers.
    pub hint: TextStyle,
    /// Commands, output and ids of files: the mono face at the id size.
    pub mono: TextStyle,
    /// The mono face at the meta size: a command's header line, a file path in a list.
    pub mono_meta: TextStyle,
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
            brand: TextStyle::new(14.0, 600.0).lh(20.0),
            greeting: TextStyle::new(34.0, 600.0).lh(40.0),
            hint: TextStyle { features: "'tnum' 1", ..TextStyle::new(12.0, 400.0).lh(16.0) },
            mono: TextStyle { family: rune::Family::Mono, ..TextStyle::new(12.0, 400.0).lh(17.0) },
            mono_meta: TextStyle { family: rune::Family::Mono, ..TextStyle::new(12.5, 400.0).lh(18.0) },
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
    /// The pebble, ink, well, slab and glass of the design system.
    pub mat: Materials,
    /// The kit's components (buttons, menus, tabs, dialogs) read their tokens from this.
    pub kit: Kit,
}

fn c(hex: &str) -> Color {
    rune::color::hex(hex).unwrap_or_else(|| panic!("a theme colour that is not hex: {hex}"))
}

/// `share` of `over` laid on `under`, blended in gamma-encoded sRGB the way CSS alpha is.
pub fn mix_srgb(under: Color, over: Color, share: f32) -> Color {
    use rune::color::{srgb_decode, srgb_encode};
    let m = |a: f32, b: f32| srgb_decode(srgb_encode(a) * (1.0 - share) + srgb_encode(b) * share);
    Color { r: m(under.r, over.r), g: m(under.g, over.g), b: m(under.b, over.b), a: 1.0 }
}

impl Neko {
    pub fn build(dark: bool) -> Neko {
        Neko::from_hex(dark, if dark { DARK } else { LIGHT })
    }

    /// The theme for a token table: the defaults above, or a picked theme mapped onto the same tokens (`themes.rs`).
    pub fn from_hex(dark: bool, hex: Hex) -> Neko {
        let window = c(hex.background);
        let fg = c(hex.foreground);
        let muted = c(hex.muted);
        let ink = |a: f32| fg.alpha(a);
        let surface = Surfaces {
            window,
            card: c(hex.card),
            // The chrome behind the floating panels: the web's `--sidebar` (darker than the page in dark, `#fafafa` in light).
            sidebar: c(hex.sidebar),
            popover: c(hex.popover),
            border: c(hex.border).alpha(hex.border_alpha),
            hover: ink(0.04),
            active: ink(0.06),
            selected: ink(0.06),
            field_border: ink(0.10),
            ring: c(hex.iris).alpha(0.6),
        };
        // The faint grey is the muted grey thinned toward the window, an opaque colour so it
        // never changes with what is under it. Blended in sRGB, as the web's `/70` is.
        let faint = mix_srgb(window, muted, FAINT_SHARE);
        let text = Greys { strong: c(hex.strong), base: fg, muted, faint };
        let meaning = Meaning { iris: c(hex.iris), amber: c(hex.amber), sky: c(hex.sky), mint: c(hex.mint), coral: c(hex.coral) };
        let radius = Radii { window: 20.0, row: 18.0, control: 24.0 };
        let ty = NekoType::new();

        let mut kit = Kit::from_palette(PaletteSeed { bg: window, fg, accent: meaning.iris });
        kit.c.text_muted = muted;
        kit.c.text_faint = faint;
        kit.c.icon_muted = muted;
        kit.c.icon_faint = faint;
        kit.c.border_base = surface.border;
        kit.c.border_focus = surface.ring;
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
        // The spec's popup scale: a menu item 14, a menu and a popover 16, the sheet 20 (the 24 control radius is ours, not the kit's).
        kit.radius = rune_kit::Radius { xs: 6.0, sm: 14.0, md: 16.0, lg: 16.0, xl: radius.window };
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

        let mat = Materials::of(dark, &hex, window, surface.card, fg, c(hex.sidebar));
        Neko { dark, hex, surface, text, meaning, radius, ty, mat, kit }
    }
}

/// An sRGB colour from its 8-bit parts.
fn rgb8(r: u8, g: u8, b: u8) -> Color {
    use rune::color::srgb_decode;
    Color { r: srgb_decode(f32::from(r) / 255.0), g: srgb_decode(f32::from(g) / 255.0), b: srgb_decode(f32::from(b) / 255.0), a: 1.0 }
}

impl Neko {
    /// The terminal's colours as the theme's: 0-7 and 8-15 are the meanings (red is Coral, green
    /// Mint, yellow Amber, blue Iris, cyan Sky) and the greys, the bright ones lifted toward the
    /// text; 16-231 and 232-255 are xterm's own cube and ramp, which a theme cannot improve on.
    pub fn ansi(&self, index: u8) -> Color {
        let m = &self.meaning;
        let base = |i: u8| match i {
            0 => self.text.faint,
            1 => m.coral,
            2 => m.mint,
            3 => m.amber,
            4 => m.iris,
            5 => m.iris.mix(m.coral, 0.5),
            6 => m.sky,
            _ => self.text.muted,
        };
        match index {
            0..=7 => base(index),
            8 => self.text.muted,
            15 => self.text.base,
            9..=14 => base(index - 8).mix(self.text.base, 0.25),
            16..=231 => {
                let n = index - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
                rgb8(level(n / 36), level(n / 6 % 6), level(n % 6))
            }
            232..=255 => {
                let v = 8 + 10 * (index - 232);
                rgb8(v, v, v)
            }
        }
    }

    /// What a terminal draws on: the card, so a command's output sits on the panel's own surface.
    pub fn terminal_bg(&self) -> Color {
        self.surface.card
    }

    /// A terminal's ink where the output names no colour.
    pub fn terminal_fg(&self) -> Color {
        self.text.base
    }

    /// A true-colour terminal colour.
    pub fn rgb(&self, r: u8, g: u8, b: u8) -> Color {
        rgb8(r, g, b)
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

    fn all_hex(h: &Hex) -> [&'static str; 14] {
        [h.background, h.card, h.popover, h.sidebar, h.foreground, h.strong, h.muted, h.border, h.code, h.iris, h.amber, h.sky, h.mint, h.coral]
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
        assert_eq!(d.surface.window.hex(), "#161616ff");
        assert_eq!(l.surface.window.hex(), "#ffffffff");
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
    fn the_terminal_palette_is_the_themes_meanings_and_xterms_cube() {
        for dark in [true, false] {
            let t = theme(dark);
            assert_eq!(t.ansi(1).hex(), t.meaning.coral.hex());
            assert_eq!(t.ansi(2).hex(), t.meaning.mint.hex());
            assert_eq!(t.ansi(3).hex(), t.meaning.amber.hex());
            assert_eq!(t.ansi(15).hex(), t.text.base.hex());
            assert_ne!(t.ansi(9).hex(), t.ansi(1).hex(), "bright is lifted");
            assert_eq!(t.ansi(16).hex(), "#000000ff");
            assert_eq!(t.ansi(231).hex(), "#ffffffff");
            assert_eq!(t.ansi(232).hex(), "#080808ff");
            assert_eq!(t.ansi(196).hex(), "#ff0000ff", "xterm 196 is pure red");
            assert_eq!(t.ty.mono_meta.size, 12.5);
            assert_eq!(t.ty.mono.size, 12.0);
        }
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

    // ---- the web's CSS, parsed (polish spec §0) ----

    const WEB_THEMES: &str = include_str!("../../../web/src/themes/builtin.generated.css");
    const WEB_GLOBALS: &str = include_str!("../../../web/src/styles/globals.css");

    /// The custom properties of one `html[data-neko-theme="id"] { ... }` block.
    fn css_block(id: &str) -> std::collections::HashMap<String, String> {
        let head = format!("html[data-neko-theme=\"{id}\"] {{");
        let at = WEB_THEMES.find(&head).unwrap_or_else(|| panic!("no block for {id}"));
        let body = &WEB_THEMES[at + head.len()..];
        let body = &body[..body.find('}').expect("block ends")];
        body.lines()
            .filter_map(|l| {
                let l = l.trim().strip_suffix(';')?;
                let (name, value) = l.split_once(':')?;
                Some((name.trim().trim_start_matches("--").to_owned(), value.trim().to_owned()))
            })
            .collect()
    }

    /// `rgba(255, 255, 255, 0.07)` as ("#ffffff", 0.07).
    fn rgba(value: &str) -> (String, f32) {
        let inner = value.strip_prefix("rgba(").and_then(|v| v.strip_suffix(')')).unwrap_or_else(|| panic!("not rgba: {value}"));
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let hex = format!("#{:02x}{:02x}{:02x}", parts[0].parse::<u8>().unwrap(), parts[1].parse::<u8>().unwrap(), parts[2].parse::<u8>().unwrap());
        (hex, parts[3].parse().unwrap())
    }

    fn css_var(name: &str) -> String {
        let at = WEB_GLOBALS.find(&format!("--{name}:")).unwrap_or_else(|| panic!("no --{name} in globals.css"));
        let rest = &WEB_GLOBALS[at + name.len() + 3..];
        rest[..rest.find(';').unwrap()].trim().to_owned()
    }

    #[test]
    fn the_token_table_is_the_web_uis_css_exactly() {
        for (id, h) in [("neutral", DARK), ("neutral-light", LIGHT)] {
            let css = css_block(id);
            let pairs = [
                ("background", h.background),
                ("card", h.card),
                ("popover", h.popover),
                ("sidebar", h.sidebar),
                ("foreground", h.foreground),
                ("strong", h.strong),
                ("muted-foreground", h.muted),
                ("code", h.code),
                ("iris", h.iris),
                ("warning", h.amber),
                ("info", h.sky),
                ("success", h.mint),
                ("destructive", h.coral),
            ];
            for (name, ours) in pairs {
                assert_eq!(css[name], ours, "{id}: --{name}");
            }
            let (border, alpha) = rgba(&css["border"]);
            assert_eq!((border.as_str(), alpha), (h.border, h.border_alpha), "{id}: --border");
        }
    }

    #[test]
    fn the_built_theme_paints_those_tokens() {
        for (dark, id) in [(true, "neutral"), (false, "neutral-light")] {
            let (t, css) = (theme(dark), css_block(id));
            let hex = |name: &str| c(&css[name]).hex();
            assert_eq!(t.surface.window.hex(), hex("background"));
            assert_eq!(t.surface.card.hex(), hex("card"));
            assert_eq!(t.surface.popover.hex(), hex("popover"));
            assert_eq!(t.text.base.hex(), hex("foreground"));
            assert_eq!(t.text.muted.hex(), hex("muted-foreground"));
            assert_eq!(t.meaning.iris.hex(), hex("iris"));
            assert_eq!(t.meaning.coral.hex(), hex("destructive"));
            // --border is a colour at an alpha, painted as that.
            let (b, a) = rgba(&css["border"]);
            assert_eq!(t.surface.border.hex(), c(&b).alpha(a).hex());
            // The faint grey is the muted grey laid on the background at FAINT_SHARE.
            assert_eq!(t.text.faint.hex(), mix_srgb(c(&css["background"]), c(&css["muted-foreground"]), FAINT_SHARE).hex());
        }
    }

    #[test]
    fn faces_and_radius_are_the_webs() {
        // `--font-sans` starts with "Inter Variable", the web's bundled build of the Inter Rune embeds.
        assert_eq!(css_var("font-sans").replace("\"Inter Variable\"", "Inter"), FONT_SANS);
        assert_eq!(css_var("font-mono"), FONT_MONO);
        // `--radius: 1.25rem` at the web's 16 px root.
        assert_eq!(css_var("radius"), "1.25rem");
        assert_eq!(theme(true).radius.window, 1.25 * 16.0);
    }

    #[test]
    fn the_polish_spec_fills_are_exact() {
        for dark in [true, false] {
            let t = theme(dark);
            let fg = t.text.base;
            assert_eq!(t.surface.hover.a, 0.04);
            // The active nav row and the open row: the foreground at 6% (design system 4.6).
            assert_eq!(t.surface.selected.a, 0.06);
            assert_eq!(t.surface.field_border.a, 0.10);
            assert_eq!(t.surface.hover.hex()[..7], fg.hex()[..7]);
            // A 36 point row is a pill (radius 18); a control's 24 clamps to its half height.
            assert_eq!(t.radius.row, 18.0);
            assert_eq!(t.radius.control, 24.0);
            assert_eq!(t.surface.ring.a, 0.6);
            assert_eq!(t.surface.ring.hex()[..7], t.meaning.iris.hex()[..7]);
        }
    }

    #[test]
    fn the_chrome_behind_the_panels_is_the_webs_sidebar() {
        for (dark, id) in [(true, "neutral"), (false, "neutral-light")] {
            let css = css_block(id);
            assert_eq!(theme(dark).surface.sidebar.hex(), c(&css["sidebar"]).hex(), "{id}");
        }
    }

    /// The pebble, ink, group and glass literals are the web's (`web/src/styles/sky-materials.css`, the Neko themes' blocks).
    #[test]
    fn the_materials_are_the_webs() {
        const MATERIALS: &str = include_str!("../../../web/src/styles/sky-materials.css");
        let block = |theme: &str| {
            let start = MATERIALS.find(&format!("html[data-neko-theme=\"{theme}\"] .sky {{")).unwrap_or_else(|| panic!("no block for {theme}"));
            let rest = &MATERIALS[start..];
            rest[..rest.find("}\n").unwrap()].to_owned()
        };
        for (dark, id) in [(true, "neutral"), (false, "neutral-light")] {
            let css = block(id);
            let m = theme(dark).mat;
            for (var, got) in [
                ("--sky-pebble-top", m.pebble_top),
                ("--sky-pebble-bottom", m.pebble_bottom),
                ("--sky-pebble-fg", m.pebble_fg),
                ("--sky-ink-top", m.ink_top),
                ("--sky-ink-bottom", m.ink_bottom),
                ("--sky-ink-fg", m.ink_fg),
                ("--sky-group", m.group),
                ("--sky-group-hover", m.group_hover),
            ] {
                let line = css.lines().find(|l| l.trim_start().starts_with(&format!("{var}:"))).unwrap_or_else(|| panic!("{id} has no {var}"));
                let want = line.split(':').nth(1).unwrap().trim().trim_end_matches(';').to_owned();
                assert_eq!(got.hex()[..7].to_lowercase(), want.to_lowercase(), "{id} {var}");
            }
        }
    }
}

//! The Neko theme in Rune: the approved prototype's tokens as one module, light and dark.
//!
//! Values come from the prototype (`~/Documents/neko-design/prototype-2026-11-21/_src/app.css`, the `.window` and
//! `.window.light` custom properties), exactly: since 2026-11-21 the prototype, not the web UI's CSS, is the source of truth for how
//! Rune looks (`docs/knowledge/rune-look-2026-11.md`). The prototype's names map as chrome = [`Surfaces::sidebar`], surface =
//! [`Surfaces::window`], raised = [`Surfaces::card`], text / text-2 / text-3 = [`Greys`] base / muted / faint, run = Iris.
//! Nothing outside this module names a colour, a radius or a font size; screens read a [`Neko`].
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

/// The prototype's `.window` (dark). `background` is its `--surface` (the content), `card` its `--raised`, `sidebar` its `--chrome`,
/// `muted` its `--text-2`; `iris` is its `--run` (the colour of work in progress and of focus). `sky` (ready to review) is the one hue the
/// prototype does not draw: a cyan a step off `--run`, so the two never read as one meaning.
pub const DARK: Hex = Hex {
    background: "#141416",
    card: "#1b1b1e",
    popover: "#1b1b1e",
    sidebar: "#0c0c0d",
    foreground: "#ededef",
    strong: "#f4f4f5",
    muted: "#a0a0a7",
    border: "#ffffff",
    border_alpha: 0.07,
    code: "#1b1b1e",
    iris: "#8ab4ff",
    amber: "#f2b24c",
    sky: "#7ec8e3",
    mint: "#6cc28a",
    coral: "#f07171",
};

/// The prototype's `.window.light`.
pub const LIGHT: Hex = Hex {
    background: "#ffffff",
    card: "#f5f5f7",
    popover: "#f5f5f7",
    sidebar: "#ececee",
    foreground: "#18181b",
    strong: "#18181b",
    muted: "#55555c",
    border: "#000000",
    border_alpha: 0.07,
    code: "#f5f5f7",
    iris: "#3d6fe0",
    amber: "#d48a12",
    sky: "#2a8fb0",
    mint: "#2f9a5a",
    coral: "#d94a4a",
};

/// The web's `--font-sans` with its first face, `"Inter Variable"` (the web's bundled build), read as the
/// `Inter` Rune embeds. The rest of the list is the web's, verbatim.
pub const FONT_SANS: &str = "Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, \"SF Pro Text\", system-ui, sans-serif";
/// The web's `--font-mono`, verbatim.
pub const FONT_MONO: &str = "ui-monospace, \"SF Mono\", \"JetBrains Mono\", Menlo, monospace";

/// Share of the muted grey that is left in the faint grey for a theme the person picked (the web's `muted-foreground/70` is 0.70; 0.72 keeps 3.0:1).
const FAINT_SHARE: f32 = 0.72;
/// The prototype's `--text-3` as a share of its `--text-2` laid on the content colour: #6b6b73 on #141416 is 0.62 of #a0a0a7, #7d7d85 on white is
/// 0.765 of #55555c (so the faint grey is exactly the prototype's, 3.4:1 in dark and 4.0:1 in light).
const FAINT_SHARE_DARK: f32 = 0.62;
const FAINT_SHARE_LIGHT: f32 = 0.765;

/// Surfaces. Every page sits on `window`; a card is a step off it (light) or the same
/// near-black (dark), never ringed with a border (DESIGN.md §8 rule 10).
#[derive(Clone, Copy, Debug)]
pub struct Surfaces {
    /// The prototype's `--surface`: the content, flush with the window's edges.
    pub window: Color,
    /// The prototype's `--raised`: the composer, a card, a popover.
    pub card: Color,
    /// The prototype's `--chrome`: the sidebar and the cat panel, on the window itself.
    pub sidebar: Color,
    pub popover: Color,
    /// The hairline (`--line`: 7%).
    pub border: Color,
    /// The stronger hairline (`--line-strong`: 11% dark, 12% light): a control's outline, the composer's ring.
    pub line_strong: Color,
    /// A row under the pointer (`--hover`: 4% dark, 3.5% light).
    pub hover: Color,
    /// The open row, the active nav row (`--sel`: 7% dark, 5.5% light).
    pub active: Color,
    pub selected: Color,
    /// A pill's or a quiet button's wash (`--tint`: 5.5% dark, 4.5% light).
    pub tint: Color,
    /// The segmented control's well (`--seg`) and its chosen segment (`--seg-on`).
    pub seg: Color,
    pub seg_on: Color,
    /// A field's 1 px outline: the stronger hairline.
    pub field_border: Color,
    /// The focus ring: Iris at 60%.
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
    /// The prototype's `--run`: work in progress, focus. (Its primary button is the ink, not this.)
    pub iris: Color,
    pub amber: Color,
    /// `--amber-t`: amber as words (a count, a line under a name), and `--amber-bg` for its wash.
    pub amber_text: Color,
    pub amber_bg: Color,
    pub sky: Color,
    pub mint: Color,
    pub coral: Color,
}

/// Radii of the prototype: row 8, button 7, card 12, composer and sheet 14, pill 999.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    /// A sheet, the composer: 14.
    pub window: f32,
    /// A nav row, a list row: 8.
    pub row: f32,
    /// A button, a field: 7.
    pub control: f32,
    /// A card (a cat on Home, a question): 12.
    pub card: f32,
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
    /// The prototype has no gradients, glows or lit edges: every material is a flat fill (or none) and a hairline. A theme the person picked gets
    /// the same flat materials from its own tokens. `ink` is the prototype's `--ink` / `--on-ink`.
    fn of(dark: bool, hex: &Hex, window: Color, card: Color, fg: Color, sidebar: Color) -> Materials {
        let k = |r: u8, g: u8, b: u8, a: f32| rgb8(r, g, b).alpha(a);
        let none = Color::TRANSPARENT;
        let (ink, on_ink, tint, group_hover, divider, drop_far, thumb_off) = if *hex == DARK {
            (c("#f4f4f5"), c("#111111"), k(255, 255, 255, 0.055), k(255, 255, 255, 0.04), k(255, 255, 255, 0.07), k(0, 0, 0, 0.4), c("#d8d8dc"))
        } else if *hex == LIGHT {
            (c("#18181b"), Color::WHITE, k(0, 0, 0, 0.045), k(0, 0, 0, 0.035), k(0, 0, 0, 0.07), k(0, 0, 0, 0.12), Color::WHITE)
        } else if dark {
            (fg, window, fg.alpha(0.055), fg.alpha(0.04), k(255, 255, 255, 0.07), k(0, 0, 0, 0.4), fg)
        } else {
            (fg, window, fg.alpha(0.045), fg.alpha(0.035), k(0, 0, 0, 0.07), k(0, 0, 0, 0.12), Color::WHITE)
        };
        let thumb_on = if dark { window } else { Color::WHITE };
        Materials {
            // A secondary control is the tint, flat.
            pebble_top: tint,
            pebble_bottom: tint,
            pebble_fg: fg,
            pebble_edge: none,
            ink_top: ink,
            ink_bottom: ink,
            ink_fg: on_ink,
            ink_edge: none,
            ink_ring: none,
            well: tint,
            well_shadow: none,
            // A settings group: the raised colour; its hover and the hairline between its rows.
            group: card,
            group_hover,
            divider,
            drop_far,
            drop_near: none,
            glass_top: card,
            glass_bottom: card,
            panel_top: sidebar,
            panel_mid: sidebar,
            panel_bottom: sidebar,
            // A line icon: the second grey at 75% (`.nav a .i`).
            icon: c(hex.muted).alpha(0.75),
            thumb_off,
            thumb_on,
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
    /// The sidebar's brand line: "Neko" 13.5 semibold.
    pub brand: TextStyle,
    /// Home's greeting: 28 semibold, tracking -0.02em.
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
            brand: TextStyle::new(13.5, 600.0).lh(20.0),
            greeting: TextStyle { tracking: -0.56, ..TextStyle::new(28.0, 600.0).lh(32.0) },
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
        // The prototype's washes: hover 4% (3.5% light), selected 7% (5.5%), tint 5.5% (4.5%), the strong hairline 11% (12%).
        let (hover_a, sel_a, tint_a, strong_a, seg_a) = if dark { (0.04, 0.07, 0.055, 0.11, 0.06) } else { (0.035, 0.055, 0.045, 0.12, 0.05) };
        let line = c(hex.border);
        let surface = Surfaces {
            window,
            card: c(hex.card),
            sidebar: c(hex.sidebar),
            popover: c(hex.popover),
            border: line.alpha(hex.border_alpha),
            line_strong: line.alpha(strong_a),
            hover: ink(hover_a),
            active: ink(sel_a),
            selected: ink(sel_a),
            tint: ink(tint_a),
            seg: ink(seg_a),
            seg_on: if dark { Color::WHITE.alpha(0.11) } else { Color::WHITE },
            field_border: line.alpha(strong_a),
            ring: c(hex.iris).alpha(0.6),
        };
        // The faint grey is the muted grey thinned toward the window, an opaque colour so it
        // never changes with what is under it. Blended in sRGB, as the web's `/70` is.
        let share = if hex == DARK { FAINT_SHARE_DARK } else if hex == LIGHT { FAINT_SHARE_LIGHT } else { FAINT_SHARE };
        let faint = mix_srgb(window, muted, share);
        let text = Greys { strong: c(hex.strong), base: fg, muted, faint };
        let (amber_text, amber_bg) = if hex == DARK {
            (c("#f6cf8f"), c(hex.amber).alpha(0.09))
        } else if hex == LIGHT {
            (c("#9a6200"), c(hex.amber).alpha(0.09))
        } else {
            (c(hex.amber), c(hex.amber).alpha(0.09))
        };
        let meaning = Meaning { iris: c(hex.iris), amber: c(hex.amber), amber_text, amber_bg, sky: c(hex.sky), mint: c(hex.mint), coral: c(hex.coral) };
        let radius = Radii { window: 14.0, row: 8.0, control: 7.0, card: 12.0 };
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
        // The prototype's scale: a menu item 7, a menu and a popover 10 (`.pop`), a card 12, the sheet 14.
        kit.radius = rune_kit::Radius { xs: 6.0, sm: 7.0, md: 10.0, lg: 12.0, xl: radius.window };
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
    fn the_two_appearances_follow_the_prototype() {
        let (d, l) = (theme(true), theme(false));
        assert!(d.dark && !l.dark);
        assert_eq!(d.surface.window.hex(), "#141416ff");
        assert_eq!(l.surface.window.hex(), "#ffffffff");
        assert_eq!(d.meaning.mint.hex(), "#6cc28aff");
        assert_eq!(l.meaning.coral.hex(), "#d94a4aff");
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

    // ---- the prototype's tokens, pinned (`_src/app.css` `.window` and `.window.light`) ----

    #[test]
    fn the_token_table_is_the_prototypes_exactly() {
        // [chrome, surface, raised, text, text-2, run, amber, red, green, ink]
        let dark = [("#0c0c0d", DARK.sidebar), ("#141416", DARK.background), ("#1b1b1e", DARK.card), ("#ededef", DARK.foreground), ("#a0a0a7", DARK.muted), ("#8ab4ff", DARK.iris), ("#f2b24c", DARK.amber), ("#f07171", DARK.coral), ("#6cc28a", DARK.mint), ("#f4f4f5", DARK.strong)];
        let light = [("#ececee", LIGHT.sidebar), ("#ffffff", LIGHT.background), ("#f5f5f7", LIGHT.card), ("#18181b", LIGHT.foreground), ("#55555c", LIGHT.muted), ("#3d6fe0", LIGHT.iris), ("#d48a12", LIGHT.amber), ("#d94a4a", LIGHT.coral), ("#2f9a5a", LIGHT.mint), ("#18181b", LIGHT.strong)];
        for (want, got) in dark.into_iter().chain(light) {
            assert_eq!(want, got);
        }
        assert_eq!((DARK.border, DARK.border_alpha, LIGHT.border, LIGHT.border_alpha), ("#ffffff", 0.07, "#000000", 0.07));
    }

    #[test]
    fn the_third_grey_is_the_prototypes_text_3() {
        // `--text-3`: #6b6b73 in dark, #7d7d85 in light; the share above reproduces it to within 4 of the 8-bit steps (one share cannot hit all three channels).
        for (dark, want) in [(true, [0x6b, 0x6b, 0x73]), (false, [0x7d, 0x7d, 0x85])] {
            let got = theme(dark).text.faint.hex();
            for (i, w) in want.iter().enumerate() {
                let g = i32::from_str_radix(&got[1 + 2 * i..3 + 2 * i], 16).unwrap();
                assert!((g - w).abs() <= 4, "{got} dark={dark}");
            }
        }
    }

    #[test]
    fn the_prototypes_washes_and_radii_are_exact() {
        for dark in [true, false] {
            let t = theme(dark);
            let fg = t.text.base;
            assert_eq!(t.surface.hover.a, if dark { 0.04 } else { 0.035 });
            assert_eq!(t.surface.selected.a, if dark { 0.07 } else { 0.055 });
            assert_eq!(t.surface.tint.a, if dark { 0.055 } else { 0.045 });
            assert_eq!(t.surface.border.a, 0.07);
            assert_eq!(t.surface.line_strong.a, if dark { 0.11 } else { 0.12 });
            assert_eq!(t.surface.hover.hex()[..7], fg.hex()[..7]);
            assert_eq!((t.radius.row, t.radius.control, t.radius.card, t.radius.window), (8.0, 7.0, 12.0, 14.0));
            assert_eq!(t.surface.ring.a, 0.6);
            assert_eq!(t.surface.ring.hex()[..7], t.meaning.iris.hex()[..7]);
        }
    }

    #[test]
    fn the_materials_are_flat() {
        for dark in [true, false] {
            let m = theme(dark).mat;
            // No gradient, no lit edge: the ink is one colour, so is the tint.
            assert_eq!(m.ink_top.hex(), m.ink_bottom.hex());
            assert_eq!(m.pebble_top.hex(), m.pebble_bottom.hex());
            assert_eq!(m.ink_edge.a, 0.0);
            assert_eq!(m.pebble_edge.a, 0.0);
        }
        assert_eq!(theme(true).mat.ink_top.hex(), "#f4f4f5ff");
        assert_eq!(theme(true).mat.ink_fg.hex(), "#111111ff");
        assert_eq!(theme(false).mat.ink_top.hex(), "#18181bff");
    }
}

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
    background: "#0f0f0f",
    card: "#151515",
    popover: "#181818",
    sidebar: "#121212",
    foreground: "#d7d7d7",
    strong: "#f7f7f7",
    muted: "#8f8f8f",
    border: "#ffffff",
    border_alpha: 0.07,
    code: "#171717",
    iris: "#346bf1",
    amber: "#e0a042",
    sky: "#2393f1",
    mint: "#4cc38a",
    coral: "#e5484d",
};

/// `html[data-neko-theme="neutral-light"]` ("Neko Light").
pub const LIGHT: Hex = Hex {
    background: "#fafafa",
    card: "#ffffff",
    popover: "#ffffff",
    sidebar: "#f3f3f3",
    foreground: "#262626",
    strong: "#0d0d0d",
    muted: "#636363",
    border: "#000000",
    border_alpha: 0.08,
    code: "#f2f2f2",
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

/// How much lighter than the window the sidebar is painted (polish spec §4), in HSL lightness.
const SIDEBAR_LIFT: f32 = 0.02;

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

/// `colour` with its HSL lightness raised by `by` (0.02 = two points), saturation and hue kept. In
/// gamma-encoded sRGB, as a designer's lightness is.
fn lighten(colour: Color, by: f32) -> Color {
    use rune::color::{srgb_decode, srgb_encode};
    let (r, g, b) = (srgb_encode(colour.r), srgb_encode(colour.g), srgb_encode(colour.b));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    let (h, sat) = if d < 1e-6 {
        (0.0, 0.0)
    } else {
        let sat = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r { ((g - b) / d).rem_euclid(6.0) } else if max == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
        (h, sat)
    };
    let l2 = (l + by).clamp(0.0, 1.0);
    let k = (1.0 - (2.0 * l2 - 1.0).abs()) * sat;
    let x = k * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0 => (k, x, 0.0),
        1 => (x, k, 0.0),
        2 => (0.0, k, x),
        3 => (0.0, x, k),
        4 => (x, 0.0, k),
        _ => (k, 0.0, x),
    };
    let m = l2 - k / 2.0;
    Color { r: srgb_decode(r1 + m), g: srgb_decode(g1 + m), b: srgb_decode(b1 + m), a: colour.a }
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
            sidebar: lighten(window, SIDEBAR_LIFT),
            popover: c(hex.popover),
            border: c(hex.border).alpha(hex.border_alpha),
            hover: ink(0.04),
            active: ink(0.07),
            selected: ink(0.07),
            field_border: ink(0.10),
            ring: c(hex.iris).alpha(0.6),
        };
        // The faint grey is the muted grey thinned toward the window, an opaque colour so it
        // never changes with what is under it. Blended in sRGB, as the web's `/70` is.
        let faint = mix_srgb(window, muted, FAINT_SHARE);
        let text = Greys { strong: c(hex.strong), base: fg, muted, faint };
        let meaning = Meaning { iris: c(hex.iris), amber: c(hex.amber), sky: c(hex.sky), mint: c(hex.mint), coral: c(hex.coral) };
        let radius = Radii { window: 20.0, row: 8.0, control: 8.0 };
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
                ("primary", h.iris),
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
            assert_eq!(t.meaning.iris.hex(), hex("primary"));
            assert_eq!(t.meaning.coral.hex(), hex("destructive"));
            // --border is a colour at an alpha, painted as that.
            let (b, a) = rgba(&css["border"]);
            assert_eq!(t.surface.border.hex(), c(&b).alpha(a).hex());
            // The faint grey is `muted-foreground/70` laid on the background, as the web writes it.
            assert_eq!(t.text.faint.hex(), mix_srgb(c(&css["background"]), c(&css["muted-foreground"]), 0.7).hex());
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
            assert_eq!(t.surface.selected.a, 0.07);
            assert_eq!(t.surface.field_border.a, 0.10);
            assert_eq!(t.surface.hover.hex()[..7], fg.hex()[..7]);
            assert_eq!(t.radius.row, 8.0);
            assert_eq!(t.surface.ring.a, 0.6);
            assert_eq!(t.surface.ring.hex()[..7], t.meaning.iris.hex()[..7]);
        }
    }

    #[test]
    fn the_sidebar_is_two_points_lighter_than_the_window() {
        let lightness = |c: Color| {
            use rune::color::srgb_encode;
            let (r, g, b) = (srgb_encode(c.r), srgb_encode(c.g), srgb_encode(c.b));
            (r.max(g).max(b) + r.min(g).min(b)) / 2.0
        };
        for dark in [true, false] {
            let t = theme(dark);
            let d = lightness(t.surface.sidebar) - lightness(t.surface.window);
            let want = if dark { 0.02 } else { 0.02f32.min(1.0 - lightness(t.surface.window)) };
            assert!((d - want).abs() < 0.004, "dark={dark}: {d}");
        }
    }
}

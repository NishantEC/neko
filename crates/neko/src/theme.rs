// This module is "the token table as a real Rust module" in full, not just
// the subset this v1 slice happens to paint: `surface_raised` is mouse-hover
// only (rows are keyboard-first here), `state_success`/`state_danger` are
// onboarding's permission-granted/denied colours, and
// `PANEL_WIDTH_WITH_DETAIL_PX` is the clipboard-detail-pane width.
#![allow(dead_code)]

//! The design tokens — **colour is a swappable table now, geometry is not.**
//!
//! # The token-table contract
//!
//! There are exactly two kinds of token in this file and they behave
//! differently on purpose:
//!
//! * **Geometry, spacing and type** stay `pub const`. A theme is colour and
//!   surface only; it cannot move a row, change a radius, or resize the
//!   panel. Nothing reads these through a theme.
//! * **Colour** lives on [`Palette`], one struct of [`PALETTE_TOKEN_COUNT`]
//!   `Rgba` fields, and every paint site reads the *active* one through
//!   [`active()`]. There is no `if themed` branch anywhere: a call site that
//!   used to say `theme::TEXT_PRIMARY` now says `theme::active().text_primary`
//!   and is otherwise unchanged.
//!
//! **A theme supplies values; it never gets to change what a token means.**
//! That is enforced structurally rather than by convention: a built-in theme
//! is written as a [`Spec`] of *independent* colours, and [`build`] derives
//! every dependent token from it with the same relationship the frozen design
//! froze —
//!
//! | derived token | rule |
//! |---|---|
//! | `surface_panel_translucent` | `surface_panel` at `panel_alpha` |
//! | `menu_glass_tint` | `surface_raised` at `menu_tint_alpha` |
//! | `text_tertiary_on_selected` | *is* `text_secondary` (the promotion rule) |
//! | `border_hairline` / `_strong` | one hairline hue at `a` / `2a` |
//! | `banner_danger_bg` | `state_danger` at [`BANNER_ALPHA`] |
//! | `state_success_border` / `state_danger_border` | the state colour at [`STATE_BORDER_ALPHA`] |
//! | `row_icon_socket_bg` | one socket hue at `icon_socket_alpha` |
//!
//! A theme that wanted `border_hairline_strong` to be *weaker* than
//! `border_hairline`, or the danger banner to be built out of the success
//! colour, cannot express that — it would have to edit [`build`], which is one
//! place, reviewed once, rather than 17 palettes each free to drift.
//!
//! # Reading the active theme costs nothing per frame
//!
//! [`active()`] is one relaxed [`AtomicUsize`] load and a slice index into a
//! `&'static [Theme]`. No allocation, no lock, no `Arc`, no clone — the
//! returned `&'static Palette` points straight into the binary's own rodata.
//! That matters because it is read dozens of times per rendered frame, on a
//! path where warm summon is measured in single-digit milliseconds.
//!
//! # Where the colours came from
//!
//! Three palettes are this project's own work: `neutral` (the shipped default,
//! byte-for-byte what `theme.rs` held before this file became a table — nobody's
//! app changes appearance on upgrade), and `ember`/`catnap` from
//! `data/neko-cozy-theme/mockups/`. The rest are vendored from upstream
//! projects, every one of them MIT, each verified from its own source rather
//! than assumed — see `AGENTS.md`'s "Themes" section for the licence table with
//! links, and `docs/evidence/themes-report.md` for the full per-theme contrast
//! numbers.
//!
//! **Vendored does not mean unmodified.** 20 of 187 vendored token values were
//! lightness-corrected (hue and chroma untouched) to clear WCAG AA 4.5:1 on
//! this app's own text-on-surface pairs, which are not the pairs a terminal
//! colour scheme was designed against. Every correction is named in the
//! comment above its own theme below, and listed with before/after numbers in
//! the evidence report. Every built-in ships passing AA on all ten checked
//! pairs — `themes_all_pass_wcag_aa` is the test that keeps it that way.
//!
//! Every `Rgba` in the `neutral`/`ember`/`catnap` specs is generated from an
//! OKLCH triple via Björn Ottosson's OKLab→linear-sRGB matrices; the test at
//! the bottom of this file re-derives them from an independently-implemented
//! conversion, and pins every *vendored* theme against its upstream hex.

use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::Rgba;

/// `gpui::rgb()` isn't `const fn` (it goes through `u32::to_be_bytes().map(..)`),
/// so the token table below needs its own const-evaluable version.
const fn rgb_const(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

const fn rgba_const(hex: u32, a: f32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a,
    }
}

/// The accessibility-declined banner's background: `state_danger`'s own hue at
/// ~7.8% alpha (`render_accessibility_banner` in `panel.rs`). Frozen from the
/// original design; a theme picks the danger *hue*, never this alpha.
pub const BANNER_ALPHA: f32 = 0.078_431_37;
/// Status-pill borders (`status_pill` in `onboarding/view.rs`): the state
/// colour at ~34.9% alpha. Frozen, same reasoning as [`BANNER_ALPHA`].
pub const STATE_BORDER_ALPHA: f32 = 0.349_019_6;
/// How many `Rgba` fields [`Palette`] has. Pinned by a test so this doc, the
/// struct, and `AGENTS.md` cannot silently disagree.
pub const PALETTE_TOKEN_COUNT: usize = 20;

/// Whether a theme reads as light or dark **to AppKit**, not just to a person.
///
/// This is not decorative: the native window material (`material.rs` —
/// `NSGlassEffectView`, or the `NSVisualEffectView(.popover)` fallback) renders
/// in whatever `NSAppearance` the window is in, so a cream panel over a
/// dark-appearance blur gets a dark halo bleeding through wherever the panel is
/// translucent. `material::set_window_appearance` reads this and sets
/// `NSAppearance(named:)` on the real `NSWindow` when the theme changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Dark,
    Light,
}

/// The full colour table for one theme. Every field is a *paint-site* token —
/// the thing a `div` actually gets handed — not a raw palette entry, which is
/// why the derived ones (`*_translucent`, `*_border`, `text_tertiary_on_selected`)
/// live here rather than being recomputed at each call site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub surface_panel: Rgba,
    /// `surface_panel`'s own RGB at reduced alpha — the panel's fill when a
    /// native material (`material.rs`) is behind the window. This is what makes
    /// the material visible at all: GPUI renders this `div`'s fill on top of the
    /// native background view in window z-order, so a fully opaque fill would
    /// paint over it completely regardless of the native view's z-position.
    /// Every built-in keeps some translucency (0.82–0.90); none discards the
    /// material outright.
    pub surface_panel_translucent: Rgba,
    pub surface_raised: Rgba,
    /// The `⌘K` actions menu's fill when the native menu-overlay material
    /// installed (`material::install_menu_overlay`) — `surface_raised`'s RGB,
    /// translucent, deliberately *more* opaque than `surface_panel_translucent`
    /// because this app has no real backdrop-blur primitive behind that tint, so
    /// the fill alone carries more of the contrast duty a true blur would share.
    pub menu_glass_tint: Rgba,
    /// The clipboard-history detail pane's recessed preview box.
    pub surface_input: Rgba,
    pub surface_selected: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    /// A selected row promotes its accessory text from `text_tertiary` to
    /// `text_secondary` — the one scoped exception the original design called
    /// out (`text_tertiary` on `surface_selected` fails AA in *every* palette
    /// checked, including the shipped neutral one, at 3.04:1). Not a new colour,
    /// a paint-path rule: [`build`] wires this to `text_secondary` and a theme
    /// cannot decouple them.
    pub text_tertiary_on_selected: Rgba,
    /// Text sitting on a `text_primary` fill (the primary button's own label).
    pub text_on_light: Rgba,
    /// Onboarding's keycap chip background (`keycap_shell()` in `view.rs`).
    pub keycap_shell_bg: Rgba,
    pub state_success: Rgba,
    pub state_danger: Rgba,
    pub state_success_border: Rgba,
    pub state_danger_border: Rgba,
    pub banner_danger_bg: Rgba,
    /// Decorative dividers only — never a load-bearing boundary.
    pub border_hairline: Rgba,
    pub border_hairline_strong: Rgba,
    /// The row-icon slot's backing plate, applied behind *every* icon, not just
    /// a not-yet-cached placeholder: real macOS icon PNGs bake in their own
    /// transparent padding at wildly different ratios per app, so a raw icon
    /// floats at an inconsistent visual size with nothing to anchor it. A
    /// shared, low-alpha tinted socket gives every icon the same plate.
    ///
    /// **This is the token that inverts between light and dark themes**, and it
    /// is the one the cozy-theme study (`data/neko-cozy-theme/report.md`)
    /// specifically flags: a pale film at 6% is invisible on a pale surface. A
    /// light theme's spec therefore supplies a *dark* `icon_socket` hue; the
    /// rule ("a low-alpha plate of the opposite polarity to the panel") is
    /// unchanged, only the value is.
    pub row_icon_socket_bg: Rgba,
}

/// One built-in theme: identity, appearance, and its finished [`Palette`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Matches `neko_protocol::BUILTIN_THEMES`' own id — the daemon serves the
    /// list and persists the choice, this crate owns the actual colours, and
    /// `themes_match_the_protocol_registry` pins the two together so they
    /// cannot drift.
    pub id: &'static str,
    pub name: &'static str,
    pub appearance: Appearance,
    pub palette: Palette,
}

impl Theme {
    /// Whether this theme keeps the native window material visible at all.
    /// True for every built-in today; the seam exists because an opaque theme
    /// is a legitimate future choice, and `main.rs` would need to know.
    pub fn keeps_translucency(&self) -> bool {
        self.palette.surface_panel_translucent.a < 1.0
    }
}

/// The *independent* colours of one theme, before [`build`] derives the rest.
/// Private on purpose: a theme is authored here, in this file, as one of
/// [`THEMES`] — there is no runtime theme loading (no user-supplied palettes,
/// no config file), so nothing outside this module needs to construct one.
struct Spec {
    id: &'static str,
    name: &'static str,
    appearance: Appearance,
    surface_panel: u32,
    panel_alpha: f32,
    surface_raised: u32,
    menu_tint_alpha: f32,
    surface_input: u32,
    surface_selected: u32,
    text_primary: u32,
    text_secondary: u32,
    text_tertiary: u32,
    text_on_light: u32,
    keycap_shell_bg: u32,
    state_success: u32,
    state_danger: u32,
    /// One hue for both hairline weights — `border_hairline_strong` is the
    /// same colour at double the alpha, never an independently-chosen value.
    hairline: u32,
    hairline_alpha: f32,
    /// See [`Palette::row_icon_socket_bg`] — dark hue on a light theme.
    icon_socket: u32,
    icon_socket_alpha: f32,
}

/// The one place a token's *meaning* is written down. See this module's own
/// doc comment for the derivation table this implements.
const fn build(s: Spec) -> Theme {
    let text_secondary = rgb_const(s.text_secondary);
    Theme {
        id: s.id,
        name: s.name,
        appearance: s.appearance,
        palette: Palette {
            surface_panel: rgb_const(s.surface_panel),
            surface_panel_translucent: rgba_const(s.surface_panel, s.panel_alpha),
            surface_raised: rgb_const(s.surface_raised),
            menu_glass_tint: rgba_const(s.surface_raised, s.menu_tint_alpha),
            surface_input: rgb_const(s.surface_input),
            surface_selected: rgb_const(s.surface_selected),
            text_primary: rgb_const(s.text_primary),
            text_secondary,
            text_tertiary: rgb_const(s.text_tertiary),
            text_tertiary_on_selected: text_secondary,
            text_on_light: rgb_const(s.text_on_light),
            keycap_shell_bg: rgb_const(s.keycap_shell_bg),
            state_success: rgb_const(s.state_success),
            state_danger: rgb_const(s.state_danger),
            state_success_border: rgba_const(s.state_success, STATE_BORDER_ALPHA),
            state_danger_border: rgba_const(s.state_danger, STATE_BORDER_ALPHA),
            banner_danger_bg: rgba_const(s.state_danger, BANNER_ALPHA),
            border_hairline: rgba_const(s.hairline, s.hairline_alpha),
            border_hairline_strong: rgba_const(s.hairline, s.hairline_alpha * 2.0),
            row_icon_socket_bg: rgba_const(s.icon_socket, s.icon_socket_alpha),
        },
    }
}

/// Alias so the table below reads as data rather than as function calls.
const fn theme(s: Spec) -> Theme {
    build(s)
}

/// Every built-in theme, in the order the `Themes` mode lists them: neko's own
/// three first (the default leading), then the vendored families.
pub const THEMES: &[Theme] = &[
    // neko's own.
    theme(Spec {
        id: "neutral",
        name: "Neko Neutral",
        appearance: Appearance::Dark,
        surface_panel: 0x0d0d0d,
        panel_alpha: 0.82,
        surface_raised: 0x161616,
        menu_tint_alpha: 0.62,
        surface_input: 0x070707,
        surface_selected: 0x3a3a3a,
        text_primary: 0xe8e8e8,
        text_secondary: 0xa9a9a9,
        text_tertiary: 0x848484,
        text_on_light: 0x0f0f0f,
        keycap_shell_bg: 0x232323,
        state_success: 0x61bd67,
        state_danger: 0xe96e50,
        hairline: 0xdedede,
        hairline_alpha: 0.08,
        icon_socket: 0xe8e8e8,
        icon_socket_alpha: 0.06,
    }),
    // neko's own.
    theme(Spec {
        id: "ember",
        name: "Ember",
        appearance: Appearance::Dark,
        surface_panel: 0x350915,
        panel_alpha: 0.86,
        surface_raised: 0x48171e,
        menu_tint_alpha: 0.66,
        surface_input: 0x23040d,
        surface_selected: 0x7b3820,
        text_primary: 0xf9eee0,
        text_secondary: 0xcebcaa,
        text_tertiary: 0xab9380,
        text_on_light: 0x270e06,
        keycap_shell_bg: 0x501e1c,
        state_success: 0x68ca80,
        state_danger: 0xf3715a,
        hairline: 0xfbf0e4,
        hairline_alpha: 0.11,
        icon_socket: 0xfbf0e4,
        icon_socket_alpha: 0.1,
    }),
    // neko's own.
    theme(Spec {
        id: "catnap",
        name: "Catnap",
        appearance: Appearance::Dark,
        surface_panel: 0x4b115b,
        panel_alpha: 0.9,
        surface_raised: 0x5f216c,
        menu_tint_alpha: 0.7,
        surface_input: 0x320842,
        surface_selected: 0xa0186f,
        text_primary: 0xfff2fd,
        text_secondary: 0xe4c9e1,
        text_tertiary: 0xc8a4c3,
        text_on_light: 0x2e0936,
        keycap_shell_bg: 0x672970,
        state_success: 0x4bdc9b,
        state_danger: 0xfe7f78,
        hairline: 0xfffafe,
        hairline_alpha: 0.14,
        icon_socket: 0xfffafe,
        icon_socket_alpha: 0.22,
    }),
    // Catppuccin (MIT) — catppuccin/palette `palette.json`.
    theme(Spec {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        appearance: Appearance::Dark,
        surface_panel: 0x1e1e2e,
        panel_alpha: 0.84,
        surface_raised: 0x313244,
        menu_tint_alpha: 0.64,
        surface_input: 0x181825,
        surface_selected: 0x45475a,
        text_primary: 0xcdd6f4,
        text_secondary: 0xbac2de,
        text_tertiary: 0xa6adc8,
        text_on_light: 0x11111b,
        keycap_shell_bg: 0x45475a,
        state_success: 0xa6e3a1,
        state_danger: 0xf38ba8,
        hairline: 0xcdd6f4,
        hairline_alpha: 0.09,
        icon_socket: 0xcdd6f4,
        icon_socket_alpha: 0.08,
    }),
    // Catppuccin (MIT) — catppuccin/palette `palette.json`.
    theme(Spec {
        id: "catppuccin-macchiato",
        name: "Catppuccin Macchiato",
        appearance: Appearance::Dark,
        surface_panel: 0x24273a,
        panel_alpha: 0.84,
        surface_raised: 0x363a4f,
        menu_tint_alpha: 0.64,
        surface_input: 0x1e2030,
        surface_selected: 0x494d64,
        text_primary: 0xcad3f5,
        text_secondary: 0xb8c0e0,
        text_tertiary: 0xa5adcb,
        text_on_light: 0x181926,
        keycap_shell_bg: 0x494d64,
        state_success: 0xa6da95,
        state_danger: 0xed8796,
        hairline: 0xcad3f5,
        hairline_alpha: 0.09,
        icon_socket: 0xcad3f5,
        icon_socket_alpha: 0.08,
    }),
    // Catppuccin (MIT) — catppuccin/palette `palette.json`.
    // AA-corrected (lightness only, hue preserved): secondary.
    theme(Spec {
        id: "catppuccin-frappe",
        name: "Catppuccin Frappé",
        appearance: Appearance::Dark,
        surface_panel: 0x303446,
        panel_alpha: 0.86,
        surface_raised: 0x414559,
        menu_tint_alpha: 0.66,
        surface_input: 0x292c3c,
        surface_selected: 0x51576d,
        text_primary: 0xc6d0f5,
        text_secondary: 0xc7cfe9,
        text_tertiary: 0xa5adce,
        text_on_light: 0x232634,
        keycap_shell_bg: 0x51576d,
        state_success: 0xa6d189,
        state_danger: 0xe78284,
        hairline: 0xc6d0f5,
        hairline_alpha: 0.09,
        icon_socket: 0xc6d0f5,
        icon_socket_alpha: 0.08,
    }),
    // Catppuccin (MIT) — catppuccin/palette `palette.json`.
    // AA-corrected (lightness only, hue preserved): secondary, success, tertiary.
    theme(Spec {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        appearance: Appearance::Light,
        surface_panel: 0xeff1f5,
        panel_alpha: 0.88,
        surface_raised: 0xe6e9ef,
        menu_tint_alpha: 0.68,
        surface_input: 0xe6e9ef,
        surface_selected: 0xccd0da,
        text_primary: 0x4c4f69,
        text_secondary: 0x53566b,
        text_tertiary: 0x676a7f,
        text_on_light: 0xdce0e8,
        keycap_shell_bg: 0xccd0da,
        state_success: 0x327c21,
        state_danger: 0xd20f39,
        hairline: 0x4c4f69,
        hairline_alpha: 0.1,
        icon_socket: 0x4c4f69,
        icon_socket_alpha: 0.07,
    }),
    // Gruvbox (MIT) — morhetz/gruvbox `colors/gruvbox.vim`.
    // AA-corrected (lightness only, hue preserved): danger.
    theme(Spec {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        appearance: Appearance::Dark,
        surface_panel: 0x282828,
        panel_alpha: 0.85,
        surface_raised: 0x3c3836,
        menu_tint_alpha: 0.65,
        surface_input: 0x1d2021,
        surface_selected: 0x504945,
        text_primary: 0xfbf1c7,
        text_secondary: 0xebdbb2,
        text_tertiary: 0xbdae93,
        text_on_light: 0x282828,
        keycap_shell_bg: 0x3c3836,
        state_success: 0xb8bb26,
        state_danger: 0xfb5643,
        hairline: 0xebdbb2,
        hairline_alpha: 0.1,
        icon_socket: 0xebdbb2,
        icon_socket_alpha: 0.08,
    }),
    // Gruvbox (MIT) — morhetz/gruvbox `colors/gruvbox.vim`.
    // AA-corrected (lightness only, hue preserved): success.
    theme(Spec {
        id: "gruvbox-light",
        name: "Gruvbox Light",
        appearance: Appearance::Light,
        surface_panel: 0xfbf1c7,
        panel_alpha: 0.9,
        surface_raised: 0xf9f5d7,
        menu_tint_alpha: 0.7,
        surface_input: 0xf2e5bc,
        surface_selected: 0xd5c4a1,
        text_primary: 0x282828,
        text_secondary: 0x3c3836,
        text_tertiary: 0x665c54,
        text_on_light: 0xfbf1c7,
        keycap_shell_bg: 0xebdbb2,
        state_success: 0x74700d,
        state_danger: 0x9d0006,
        hairline: 0x3c3836,
        hairline_alpha: 0.12,
        icon_socket: 0x3c3836,
        icon_socket_alpha: 0.07,
    }),
    // Solarized (MIT) — altercation/solarized `solarized.vim`.
    // AA-corrected (lightness only, hue preserved): danger, secondary.
    theme(Spec {
        id: "solarized-dark",
        name: "Solarized Dark",
        appearance: Appearance::Dark,
        surface_panel: 0x002b36,
        panel_alpha: 0.86,
        surface_raised: 0x073642,
        menu_tint_alpha: 0.66,
        surface_input: 0x001f28,
        surface_selected: 0x0d4553,
        text_primary: 0xfdf6e3,
        text_secondary: 0xa1adad,
        text_tertiary: 0x839496,
        text_on_light: 0x002b36,
        keycap_shell_bg: 0x073642,
        state_success: 0x859900,
        state_danger: 0xe56663,
        hairline: 0x93a1a1,
        hairline_alpha: 0.12,
        icon_socket: 0x93a1a1,
        icon_socket_alpha: 0.1,
    }),
    // Solarized (MIT) — altercation/solarized `solarized.vim`.
    // AA-corrected (lightness only, hue preserved): danger, success.
    theme(Spec {
        id: "solarized-light",
        name: "Solarized Light",
        appearance: Appearance::Light,
        surface_panel: 0xfdf6e3,
        panel_alpha: 0.9,
        surface_raised: 0xfffbf0,
        menu_tint_alpha: 0.7,
        surface_input: 0xeee8d5,
        surface_selected: 0xd9d2ba,
        text_primary: 0x002b36,
        text_secondary: 0x073642,
        text_tertiary: 0x586e75,
        text_on_light: 0xfdf6e3,
        keycap_shell_bg: 0xeee8d5,
        state_success: 0x667500,
        state_danger: 0xd72724,
        hairline: 0x586e75,
        hairline_alpha: 0.14,
        icon_socket: 0x586e75,
        icon_socket_alpha: 0.08,
    }),
    // Nord (MIT) — nordtheme/nord `src/nord.css`.
    // AA-corrected (lightness only, hue preserved): danger.
    theme(Spec {
        id: "nord",
        name: "Nord",
        appearance: Appearance::Dark,
        surface_panel: 0x2e3440,
        panel_alpha: 0.85,
        surface_raised: 0x3b4252,
        menu_tint_alpha: 0.65,
        surface_input: 0x272c36,
        surface_selected: 0x434c5e,
        text_primary: 0xeceff4,
        text_secondary: 0xe5e9f0,
        text_tertiary: 0xd8dee9,
        text_on_light: 0x2e3440,
        keycap_shell_bg: 0x4c566a,
        state_success: 0xa3be8c,
        state_danger: 0xd18d93,
        hairline: 0xeceff4,
        hairline_alpha: 0.1,
        icon_socket: 0xeceff4,
        icon_socket_alpha: 0.08,
    }),
    // Tokyo Night Storm (MIT) — tokyo-night/tokyo-night-vscode-theme.
    theme(Spec {
        id: "tokyo-night",
        name: "Tokyo Night",
        appearance: Appearance::Dark,
        surface_panel: 0x24283b,
        panel_alpha: 0.85,
        surface_raised: 0x1f2335,
        menu_tint_alpha: 0.65,
        surface_input: 0x1b1e2e,
        surface_selected: 0x3b4261,
        text_primary: 0xc0caf5,
        text_secondary: 0xa9b1d6,
        text_tertiary: 0x9aa5ce,
        text_on_light: 0x1b1e2e,
        keycap_shell_bg: 0x2c324a,
        state_success: 0x73daca,
        state_danger: 0xf7768e,
        hairline: 0xc0caf5,
        hairline_alpha: 0.1,
        icon_socket: 0xc0caf5,
        icon_socket_alpha: 0.08,
    }),
    // Rosé Pine (MIT) — rose-pine/palette `source/index.ts`.
    // AA-corrected (lightness only, hue preserved): secondary, tertiary.
    theme(Spec {
        id: "rose-pine",
        name: "Rosé Pine",
        appearance: Appearance::Dark,
        surface_panel: 0x191724,
        panel_alpha: 0.86,
        surface_raised: 0x1f1d2e,
        menu_tint_alpha: 0.66,
        surface_input: 0x21202e,
        surface_selected: 0x403d52,
        text_primary: 0xe0def4,
        text_secondary: 0xaca9c0,
        text_tertiary: 0x837f9a,
        text_on_light: 0x191724,
        keycap_shell_bg: 0x26233a,
        state_success: 0x9ccfd8,
        state_danger: 0xeb6f92,
        hairline: 0xe0def4,
        hairline_alpha: 0.1,
        icon_socket: 0xe0def4,
        icon_socket_alpha: 0.09,
    }),
    // Rosé Pine (MIT) — rose-pine/palette `source/index.ts`.
    // AA-corrected (lightness only, hue preserved): danger, primary, secondary, tertiary.
    theme(Spec {
        id: "rose-pine-dawn",
        name: "Rosé Pine Dawn",
        appearance: Appearance::Light,
        surface_panel: 0xfaf4ed,
        panel_alpha: 0.9,
        surface_raised: 0xfffaf3,
        menu_tint_alpha: 0.7,
        surface_input: 0xf4ede8,
        surface_selected: 0xcecacd,
        text_primary: 0x555076,
        text_secondary: 0x55526a,
        text_tertiary: 0x716b80,
        text_on_light: 0xfaf4ed,
        keycap_shell_bg: 0xf2e9e1,
        state_success: 0x286983,
        state_danger: 0xab526c,
        hairline: 0x575279,
        hairline_alpha: 0.14,
        icon_socket: 0x575279,
        icon_socket_alpha: 0.08,
    }),
    // Dracula (MIT) — dracula/dracula-theme `README.md`.
    // AA-corrected (lightness only, hue preserved): tertiary.
    theme(Spec {
        id: "dracula",
        name: "Dracula",
        appearance: Appearance::Dark,
        surface_panel: 0x282a36,
        panel_alpha: 0.85,
        surface_raised: 0x343746,
        menu_tint_alpha: 0.65,
        surface_input: 0x21222c,
        surface_selected: 0x44475a,
        text_primary: 0xf8f8f2,
        text_secondary: 0xbfbfc4,
        text_tertiary: 0x8692b9,
        text_on_light: 0x282a36,
        keycap_shell_bg: 0x343746,
        state_success: 0x50fa7b,
        state_danger: 0xff5555,
        hairline: 0xf8f8f2,
        hairline_alpha: 0.1,
        icon_socket: 0xf8f8f2,
        icon_socket_alpha: 0.08,
    }),
    // Everforest (MIT) — sainnhe/everforest `palette.md`.
    // AA-corrected (lightness only, hue preserved): secondary, tertiary.
    theme(Spec {
        id: "everforest-dark",
        name: "Everforest Dark",
        appearance: Appearance::Dark,
        surface_panel: 0x2d353b,
        panel_alpha: 0.86,
        surface_raised: 0x343f44,
        menu_tint_alpha: 0.66,
        surface_input: 0x232a2e,
        surface_selected: 0x3d484d,
        text_primary: 0xd3c6aa,
        text_secondary: 0xaeb7b0,
        text_tertiary: 0x95a099,
        text_on_light: 0x2d353b,
        keycap_shell_bg: 0x475258,
        state_success: 0xa7c080,
        state_danger: 0xe67e80,
        hairline: 0xd3c6aa,
        hairline_alpha: 0.1,
        icon_socket: 0xd3c6aa,
        icon_socket_alpha: 0.09,
    }),
];

/// The shipped default — the exact palette `theme.rs` held before it became a
/// table. Upgrading neko must not silently change how it looks.
pub const DEFAULT_THEME_ID: &str = "neutral";

/// Index into [`THEMES`] of the currently active theme.
///
/// A plain relaxed atomic, deliberately: the value is a small index that is
/// written rarely (a preview keystroke, a daemon reply at startup) and read on
/// every paint. Relaxed is sufficient because there is nothing to order it
/// against — the `&'static [Theme]` it indexes is immutable rodata, so no
/// reader can observe a half-published theme no matter how the load and store
/// are reordered. A `Mutex`/`RwLock` here would put a lock acquisition on the
/// render path for no correctness gain, and an `Arc<Palette>` swap would put an
/// atomic refcount increment there instead.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// The active theme's colours. **This is the render-path entry point** — one
/// relaxed atomic load plus a slice index; see this module's doc comment.
#[inline]
pub fn active() -> &'static Palette {
    &active_theme().palette
}

/// The active theme itself, when the caller needs its identity or appearance
/// rather than its colours (`material::set_window_appearance`, the `Themes`
/// mode's own "which row is current" check).
#[inline]
pub fn active_theme() -> &'static Theme {
    // `set_active_index` is the only writer and it clamps, so this index is
    // always in range; `.unwrap_or(&THEMES[0])` is belt-and-braces rather than
    // a real branch, and keeps this function panic-free by construction.
    THEMES
        .get(ACTIVE.load(Ordering::Relaxed))
        .unwrap_or(&THEMES[0])
}

/// Look a theme up by id. `None` for an id that names no built-in — a stale
/// value persisted by an older/newer build, which callers treat as "fall back
/// to the default" rather than as an error.
pub fn theme_by_id(id: &str) -> Option<&'static Theme> {
    THEMES.iter().find(|t| t.id == id)
}

/// Make `id` the active theme. Returns `false` (changing nothing) for an
/// unknown id, so a corrupted persisted setting degrades to "keep what's on
/// screen" rather than to a panic or a blank palette.
pub fn set_active(id: &str) -> bool {
    match THEMES.iter().position(|t| t.id == id) {
        Some(index) => {
            ACTIVE.store(index, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

pub const PANEL_RADIUS_PX: f32 = 16.0;
pub const ROW_RADIUS_PX: f32 = 8.0;
pub const DIALOG_RADIUS_PX: f32 = 14.0;
pub const BTN_RADIUS_PX: f32 = 8.0;
pub const CHIP_RADIUS_PX: f32 = 5.0;

pub const INPUT_ROW_HEIGHT_PX: f32 = 56.0;
pub const RESULT_ROW_HEIGHT_PX: f32 = 40.0;
pub const SECTION_HEADER_HEIGHT_PX: f32 = 28.0;
pub const FOOTER_HEIGHT_PX: f32 = 44.0;
/// The onboarding window's own width (`onboarding/view.rs`) — no longer the
/// summon panel's, since the captain overrode the frozen design's two-width
/// rule to one constant `PANEL_WIDTH_WITH_DETAIL_PX` (see that constant's own
/// doc comment and `AGENTS.md`, "One constant panel width").
pub const PANEL_WIDTH_PX: f32 = 680.0;
/// The summon panel's only width now, root list and every mode alike — on
/// direct captain instruction (`AGENTS.md`, "One constant panel width"). It has
/// to be 760, not 680: the real `NSWindow` is fixed at this size for the
/// process's whole lifetime and can never be resized at runtime (`AGENTS.md`,
/// "Mode view resize seam").
pub const PANEL_WIDTH_WITH_DETAIL_PX: f32 = 760.0;
/// The two-column mode view's fixed left (list) column width.
pub const MODE_LIST_COLUMN_WIDTH_PX: f32 = 264.0;
/// One agent tile in the grid above the search field, and the strip's own
/// height. A tile is two lines of text plus its own padding; the strip adds
/// the gap beneath it before the input row.
pub const AGENT_TILE_HEIGHT_PX: f32 = 56.0;
pub const AGENT_GRID_HEIGHT_PX: f32 = AGENT_TILE_HEIGHT_PX + 20.0;

pub const ROW_ICON_PX: f32 = 22.0;
pub const ROW_ICON_RADIUS_PX: f32 = 6.0;

/// The drawn size of the mark *inside* the 22px row-icon socket, leaving a
/// small inset of `ROW_ICON_SOCKET_BG` visible around it. Matches the
/// footprint the hand-painted marks this replaced already occupied (their
/// own bodies ran 12–16px inside the same slot), so swapping to `svg()`
/// moved no row and changed no row height. One token rather than a `px()`
/// per call site because it is now shared by every glyph — a per-icon size
/// is exactly how a list of icons stops reading as one set.
pub const ROW_ICON_GLYPH_PX: f32 = 15.0;

/// The results list's own scroll edge-fade band height (`edge_fade.rs`).
pub const EDGE_FADE_BAND_PX: f32 = 18.0;

/// Onboarding window geometry (design report §3).
pub const ONBOARDING_CONTENT_HEIGHT_PX: f32 = 420.0;
pub const ONBOARDING_HEIGHT_PX: f32 =
    INPUT_ROW_HEIGHT_PX + ONBOARDING_CONTENT_HEIGHT_PX + FOOTER_HEIGHT_PX;

/// Left padding the onboarding header reserves so its own content never sits
/// under macOS's real traffic-light cluster.
pub const ONBOARDING_TRAFFIC_LIGHT_CLEARANCE_PX: f32 = 88.0;
pub const ONBOARDING_TRAFFIC_LIGHT_CLEARANCE_FULLSCREEN_PX: f32 = 12.0;
pub const ONBOARDING_HEADER_BASE_PADDING_PX: f32 = 20.0;

/// Björn Ottosson's OKLab↔linear-sRGB matrices
/// (<https://bottosson.github.io/posts/oklab/>), implemented independently from
/// the published formulas — not ported from any reference app — to cross-check
/// this file's own OKLCH-derived specs in a test.
/// The process-global [`ACTIVE`] index is shared state, so **every** test in
/// this crate that calls [`set_active`] — here, and in `panel.rs`'s theme-mode
/// tests — must hold this same lock for its whole body. One lock, not one per
/// module: `cargo test`'s default parallelism will happily run a `theme.rs`
/// test and a `panel.rs` test at the same instant, and two separate mutexes
/// would not stop them stepping on each other's palette. Same discipline
/// `text_field::tests::pasteboard_test_lock` established for the systemwide
/// `NSPasteboard` (`AGENTS.md`, "Text field editing shortcuts").
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
// Kept at Ottosson's own published precision (more digits than f32 can hold) so
// this stays visually cross-referenceable against the source matrices rather
// than clippy's minimal-f32-round-trip truncation.
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

    fn hex_to_u8(hex: u32) -> (u8, u8, u8) {
        (
            ((hex >> 16) & 0xff) as u8,
            ((hex >> 8) & 0xff) as u8,
            (hex & 0xff) as u8,
        )
    }

    /// WCAG 2.x relative luminance, from the spec's own definition — not
    /// borrowed from any colour crate (this workspace has none), and
    /// deliberately independent of the OKLab code above so a mistake in one
    /// cannot hide a mistake in the other.
    fn relative_luminance(c: Rgba) -> f32 {
        fn channel(v: f32) -> f32 {
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
    }

    fn contrast(a: Rgba, b: Rgba) -> f32 {
        let (la, lb) = (relative_luminance(a), relative_luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// The neutral (default) palette's own frozen OKLCH table, carried over
    /// unchanged from before this file became a swappable table: every chrome
    /// token's `L` from the blue ramp with chroma taken to zero (`h` is left at
    /// the blue ramp's own value for diffability, but is inert at `c=0`);
    /// `state_success`/`state_danger` keep their original hue *and* chroma —
    /// state colours, not chrome. If this fails, the hex and its OKLCH triple
    /// disagree; that's the "report it" case, not a silent-fix case.
    #[test]
    fn base_palette_matches_the_frozen_oklch_table() {
        let p = theme_by_id("neutral").expect("the default theme must exist").palette;
        let table: &[(&str, f32, f32, f32, Rgba)] = &[
            ("surface_panel", 0.16, 0.0, 255.0, p.surface_panel),
            ("surface_raised", 0.20, 0.0, 255.0, p.surface_raised),
            ("surface_input", 0.13, 0.0, 255.0, p.surface_input),
            ("surface_selected", 0.35, 0.0, 255.0, p.surface_selected),
            ("text_primary", 0.93, 0.0, 257.0, p.text_primary),
            ("text_secondary", 0.735, 0.0, 255.0, p.text_secondary),
            ("text_tertiary", 0.615, 0.0, 252.0, p.text_tertiary),
            ("state_success", 0.72, 0.150, 145.0, p.state_success),
            ("state_danger", 0.68, 0.160, 35.0, p.state_danger),
            ("text_on_light", 0.17, 0.0, 255.0, p.text_on_light),
            ("keycap_shell_bg", 0.255, 0.0, 255.0, p.keycap_shell_bg),
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

    /// The same guarantee for the two OKLCH-derived cozy palettes, whose own
    /// triples are recorded in `data/neko-cozy-theme/tokens.md` (the firstmate
    /// home) — generated there by an independent Python implementation, so this
    /// re-derivation is a genuine cross-check rather than a restatement.
    #[test]
    fn the_cozy_palettes_match_their_frozen_oklch_tables() {
        let ember = theme_by_id("ember").expect("ember must exist").palette;
        let catnap = theme_by_id("catnap").expect("catnap must exist").palette;
        let table: &[(&str, f32, f32, f32, Rgba)] = &[
            ("ember/surface_panel", 0.225, 0.070, 8.0, ember.surface_panel),
            ("ember/surface_raised", 0.285, 0.075, 15.0, ember.surface_raised),
            ("ember/surface_input", 0.175, 0.055, 5.0, ember.surface_input),
            ("ember/surface_selected", 0.425, 0.100, 40.0, ember.surface_selected),
            ("ember/text_primary", 0.955, 0.022, 75.0, ember.text_primary),
            ("ember/text_secondary", 0.805, 0.032, 68.0, ember.text_secondary),
            ("ember/text_tertiary", 0.680, 0.040, 60.0, ember.text_tertiary),
            ("ember/text_on_light", 0.200, 0.045, 40.0, ember.text_on_light),
            ("ember/keycap_shell_bg", 0.310, 0.075, 25.0, ember.keycap_shell_bg),
            ("ember/state_success", 0.760, 0.140, 150.0, ember.state_success),
            ("ember/state_danger", 0.700, 0.165, 32.0, ember.state_danger),
            ("catnap/surface_panel", 0.315, 0.130, 318.0, catnap.surface_panel),
            ("catnap/surface_raised", 0.375, 0.135, 320.0, catnap.surface_raised),
            ("catnap/surface_input", 0.245, 0.105, 315.0, catnap.surface_input),
            ("catnap/surface_selected", 0.478, 0.185, 348.0, catnap.surface_selected),
            ("catnap/text_primary", 0.975, 0.020, 330.0, catnap.text_primary),
            ("catnap/text_secondary", 0.865, 0.045, 330.0, catnap.text_secondary),
            ("catnap/text_tertiary", 0.760, 0.060, 330.0, catnap.text_tertiary),
            ("catnap/text_on_light", 0.230, 0.090, 320.0, catnap.text_on_light),
            ("catnap/keycap_shell_bg", 0.400, 0.130, 322.0, catnap.keycap_shell_bg),
            ("catnap/state_success", 0.800, 0.155, 160.0, catnap.state_success),
            ("catnap/state_danger", 0.740, 0.155, 25.0, catnap.state_danger),
        ];
        for (name, l, c, h, token) in table {
            assert_eq!(
                oklch_to_srgb_u8(*l, *c, *h),
                rgba_to_u8(*token),
                "{name}: oklch({l} {c} {h}) and the shipped token disagree"
            );
        }
    }

    /// Every vendored theme, pinned against **both** the hex its upstream
    /// project publishes and the hex neko actually ships — the equivalent, for
    /// a borrowed palette, of the OKLCH table test above.
    ///
    /// The two differ for exactly the 20 values `themes_all_pass_wcag_aa`
    /// forced (upstream schemes are designed against a terminal's own
    /// text-on-background pairs, not this app's). Recording both here means a
    /// later edit that quietly walks a token away from upstream fails a test,
    /// *and* the size of every deliberate departure stays visible in the source
    /// rather than only in a report. `docs/evidence/themes-report.md` has the
    /// before/after contrast numbers for each.
    #[test]
    fn vendored_themes_match_their_pinned_upstream_and_shipped_hex() {
        /// `(token name, the hex upstream publishes, the hex neko ships)`.
        type PinnedToken = (&'static str, u32, u32);
        let table: &[(&str, &[PinnedToken])] = &[
        ("neutral", &[("surface_panel", 0x0d0d0d, 0x0d0d0d), ("surface_raised", 0x161616, 0x161616), ("surface_input", 0x070707, 0x070707), ("surface_selected", 0x3a3a3a, 0x3a3a3a), ("text_primary", 0xe8e8e8, 0xe8e8e8), ("text_secondary", 0xa9a9a9, 0xa9a9a9), ("text_tertiary", 0x848484, 0x848484), ("text_on_light", 0x0f0f0f, 0x0f0f0f), ("keycap_shell_bg", 0x232323, 0x232323), ("state_success", 0x61bd67, 0x61bd67), ("state_danger", 0xe96e50, 0xe96e50)]),
        ("ember", &[("surface_panel", 0x350915, 0x350915), ("surface_raised", 0x48171e, 0x48171e), ("surface_input", 0x23040d, 0x23040d), ("surface_selected", 0x7b3820, 0x7b3820), ("text_primary", 0xf9eee0, 0xf9eee0), ("text_secondary", 0xcebcaa, 0xcebcaa), ("text_tertiary", 0xab9380, 0xab9380), ("text_on_light", 0x270e06, 0x270e06), ("keycap_shell_bg", 0x501e1c, 0x501e1c), ("state_success", 0x68ca80, 0x68ca80), ("state_danger", 0xf3715a, 0xf3715a)]),
        ("catnap", &[("surface_panel", 0x4b115b, 0x4b115b), ("surface_raised", 0x5f216c, 0x5f216c), ("surface_input", 0x320842, 0x320842), ("surface_selected", 0xa0186f, 0xa0186f), ("text_primary", 0xfff2fd, 0xfff2fd), ("text_secondary", 0xe4c9e1, 0xe4c9e1), ("text_tertiary", 0xc8a4c3, 0xc8a4c3), ("text_on_light", 0x2e0936, 0x2e0936), ("keycap_shell_bg", 0x672970, 0x672970), ("state_success", 0x4bdc9b, 0x4bdc9b), ("state_danger", 0xfe7f78, 0xfe7f78)]),
        ("catppuccin-mocha", &[("surface_panel", 0x1e1e2e, 0x1e1e2e), ("surface_raised", 0x313244, 0x313244), ("surface_input", 0x181825, 0x181825), ("surface_selected", 0x45475a, 0x45475a), ("text_primary", 0xcdd6f4, 0xcdd6f4), ("text_secondary", 0xbac2de, 0xbac2de), ("text_tertiary", 0xa6adc8, 0xa6adc8), ("text_on_light", 0x11111b, 0x11111b), ("keycap_shell_bg", 0x45475a, 0x45475a), ("state_success", 0xa6e3a1, 0xa6e3a1), ("state_danger", 0xf38ba8, 0xf38ba8)]),
        ("catppuccin-macchiato", &[("surface_panel", 0x24273a, 0x24273a), ("surface_raised", 0x363a4f, 0x363a4f), ("surface_input", 0x1e2030, 0x1e2030), ("surface_selected", 0x494d64, 0x494d64), ("text_primary", 0xcad3f5, 0xcad3f5), ("text_secondary", 0xb8c0e0, 0xb8c0e0), ("text_tertiary", 0xa5adcb, 0xa5adcb), ("text_on_light", 0x181926, 0x181926), ("keycap_shell_bg", 0x494d64, 0x494d64), ("state_success", 0xa6da95, 0xa6da95), ("state_danger", 0xed8796, 0xed8796)]),
        ("catppuccin-frappe", &[("surface_panel", 0x303446, 0x303446), ("surface_raised", 0x414559, 0x414559), ("surface_input", 0x292c3c, 0x292c3c), ("surface_selected", 0x51576d, 0x51576d), ("text_primary", 0xc6d0f5, 0xc6d0f5), ("text_secondary", 0xb5bfe2, 0xc7cfe9), ("text_tertiary", 0xa5adce, 0xa5adce), ("text_on_light", 0x232634, 0x232634), ("keycap_shell_bg", 0x51576d, 0x51576d), ("state_success", 0xa6d189, 0xa6d189), ("state_danger", 0xe78284, 0xe78284)]),
        ("catppuccin-latte", &[("surface_panel", 0xeff1f5, 0xeff1f5), ("surface_raised", 0xe6e9ef, 0xe6e9ef), ("surface_input", 0xe6e9ef, 0xe6e9ef), ("surface_selected", 0xccd0da, 0xccd0da), ("text_primary", 0x4c4f69, 0x4c4f69), ("text_secondary", 0x5c5f77, 0x53566b), ("text_tertiary", 0x6c6f85, 0x676a7f), ("text_on_light", 0xdce0e8, 0xdce0e8), ("keycap_shell_bg", 0xccd0da, 0xccd0da), ("state_success", 0x40a02b, 0x327c21), ("state_danger", 0xd20f39, 0xd20f39)]),
        ("gruvbox-dark", &[("surface_panel", 0x282828, 0x282828), ("surface_raised", 0x3c3836, 0x3c3836), ("surface_input", 0x1d2021, 0x1d2021), ("surface_selected", 0x504945, 0x504945), ("text_primary", 0xfbf1c7, 0xfbf1c7), ("text_secondary", 0xebdbb2, 0xebdbb2), ("text_tertiary", 0xbdae93, 0xbdae93), ("text_on_light", 0x282828, 0x282828), ("keycap_shell_bg", 0x3c3836, 0x3c3836), ("state_success", 0xb8bb26, 0xb8bb26), ("state_danger", 0xfb4934, 0xfb5643)]),
        ("gruvbox-light", &[("surface_panel", 0xfbf1c7, 0xfbf1c7), ("surface_raised", 0xf9f5d7, 0xf9f5d7), ("surface_input", 0xf2e5bc, 0xf2e5bc), ("surface_selected", 0xd5c4a1, 0xd5c4a1), ("text_primary", 0x282828, 0x282828), ("text_secondary", 0x3c3836, 0x3c3836), ("text_tertiary", 0x665c54, 0x665c54), ("text_on_light", 0xfbf1c7, 0xfbf1c7), ("keycap_shell_bg", 0xebdbb2, 0xebdbb2), ("state_success", 0x79740e, 0x74700d), ("state_danger", 0x9d0006, 0x9d0006)]),
        ("solarized-dark", &[("surface_panel", 0x002b36, 0x002b36), ("surface_raised", 0x073642, 0x073642), ("surface_input", 0x001f28, 0x001f28), ("surface_selected", 0x0d4553, 0x0d4553), ("text_primary", 0xfdf6e3, 0xfdf6e3), ("text_secondary", 0x93a1a1, 0xa1adad), ("text_tertiary", 0x839496, 0x839496), ("text_on_light", 0x002b36, 0x002b36), ("keycap_shell_bg", 0x073642, 0x073642), ("state_success", 0x859900, 0x859900), ("state_danger", 0xdc322f, 0xe56663)]),
        ("solarized-light", &[("surface_panel", 0xfdf6e3, 0xfdf6e3), ("surface_raised", 0xfffbf0, 0xfffbf0), ("surface_input", 0xeee8d5, 0xeee8d5), ("surface_selected", 0xd9d2ba, 0xd9d2ba), ("text_primary", 0x002b36, 0x002b36), ("text_secondary", 0x073642, 0x073642), ("text_tertiary", 0x586e75, 0x586e75), ("text_on_light", 0xfdf6e3, 0xfdf6e3), ("keycap_shell_bg", 0xeee8d5, 0xeee8d5), ("state_success", 0x859900, 0x667500), ("state_danger", 0xdc322f, 0xd72724)]),
        ("nord", &[("surface_panel", 0x2e3440, 0x2e3440), ("surface_raised", 0x3b4252, 0x3b4252), ("surface_input", 0x272c36, 0x272c36), ("surface_selected", 0x434c5e, 0x434c5e), ("text_primary", 0xeceff4, 0xeceff4), ("text_secondary", 0xe5e9f0, 0xe5e9f0), ("text_tertiary", 0xd8dee9, 0xd8dee9), ("text_on_light", 0x2e3440, 0x2e3440), ("keycap_shell_bg", 0x4c566a, 0x4c566a), ("state_success", 0xa3be8c, 0xa3be8c), ("state_danger", 0xbf616a, 0xd18d93)]),
        ("tokyo-night", &[("surface_panel", 0x24283b, 0x24283b), ("surface_raised", 0x1f2335, 0x1f2335), ("surface_input", 0x1b1e2e, 0x1b1e2e), ("surface_selected", 0x3b4261, 0x3b4261), ("text_primary", 0xc0caf5, 0xc0caf5), ("text_secondary", 0xa9b1d6, 0xa9b1d6), ("text_tertiary", 0x9aa5ce, 0x9aa5ce), ("text_on_light", 0x1b1e2e, 0x1b1e2e), ("keycap_shell_bg", 0x2c324a, 0x2c324a), ("state_success", 0x73daca, 0x73daca), ("state_danger", 0xf7768e, 0xf7768e)]),
        ("rose-pine", &[("surface_panel", 0x191724, 0x191724), ("surface_raised", 0x1f1d2e, 0x1f1d2e), ("surface_input", 0x21202e, 0x21202e), ("surface_selected", 0x403d52, 0x403d52), ("text_primary", 0xe0def4, 0xe0def4), ("text_secondary", 0x908caa, 0xaca9c0), ("text_tertiary", 0x6e6a86, 0x837f9a), ("text_on_light", 0x191724, 0x191724), ("keycap_shell_bg", 0x26233a, 0x26233a), ("state_success", 0x9ccfd8, 0x9ccfd8), ("state_danger", 0xeb6f92, 0xeb6f92)]),
        ("rose-pine-dawn", &[("surface_panel", 0xfaf4ed, 0xfaf4ed), ("surface_raised", 0xfffaf3, 0xfffaf3), ("surface_input", 0xf4ede8, 0xf4ede8), ("surface_selected", 0xcecacd, 0xcecacd), ("text_primary", 0x575279, 0x555076), ("text_secondary", 0x797593, 0x55526a), ("text_tertiary", 0x9893a5, 0x716b80), ("text_on_light", 0xfaf4ed, 0xfaf4ed), ("keycap_shell_bg", 0xf2e9e1, 0xf2e9e1), ("state_success", 0x286983, 0x286983), ("state_danger", 0xb4637a, 0xab526c)]),
        ("dracula", &[("surface_panel", 0x282a36, 0x282a36), ("surface_raised", 0x343746, 0x343746), ("surface_input", 0x21222c, 0x21222c), ("surface_selected", 0x44475a, 0x44475a), ("text_primary", 0xf8f8f2, 0xf8f8f2), ("text_secondary", 0xbfbfc4, 0xbfbfc4), ("text_tertiary", 0x6272a4, 0x8692b9), ("text_on_light", 0x282a36, 0x282a36), ("keycap_shell_bg", 0x343746, 0x343746), ("state_success", 0x50fa7b, 0x50fa7b), ("state_danger", 0xff5555, 0xff5555)]),
        ("everforest-dark", &[("surface_panel", 0x2d353b, 0x2d353b), ("surface_raised", 0x343f44, 0x343f44), ("surface_input", 0x232a2e, 0x232a2e), ("surface_selected", 0x3d484d, 0x3d484d), ("text_primary", 0xd3c6aa, 0xd3c6aa), ("text_secondary", 0x9da9a0, 0xaeb7b0), ("text_tertiary", 0x859289, 0x95a099), ("text_on_light", 0x2d353b, 0x2d353b), ("keycap_shell_bg", 0x475258, 0x475258), ("state_success", 0xa7c080, 0xa7c080), ("state_danger", 0xe67e80, 0xe67e80)]),
        ];
        let mut corrections = 0usize;
        for (theme_id, tokens) in table {
            let t = theme_by_id(theme_id).unwrap_or_else(|| panic!("no such theme: {theme_id}"));
            for (token, upstream, shipped) in *tokens {
                if upstream != shipped {
                    corrections += 1;
                }
                let actual = rgba_to_u8(match *token {
                    "surface_panel" => t.palette.surface_panel,
                    "surface_raised" => t.palette.surface_raised,
                    "surface_input" => t.palette.surface_input,
                    "surface_selected" => t.palette.surface_selected,
                    "text_primary" => t.palette.text_primary,
                    "text_secondary" => t.palette.text_secondary,
                    "text_tertiary" => t.palette.text_tertiary,
                    "text_on_light" => t.palette.text_on_light,
                    "keycap_shell_bg" => t.palette.keycap_shell_bg,
                    "state_success" => t.palette.state_success,
                    "state_danger" => t.palette.state_danger,
                    other => panic!("unpinned token name: {other}"),
                });
                assert_eq!(
                    hex_to_u8(*shipped),
                    actual,
                    "{theme_id}/{token}: pinned {shipped:#08x} but the shipped token is {actual:?}"
                );
            }
        }
        assert_eq!(
            corrections, 20,
            "the number of tokens that deviate from upstream changed — update \
             docs/evidence/themes-report.md's correction table before changing this number"
        );
    }

    /// The relationship contract, checked on every theme rather than trusted:
    /// [`build`] is the only constructor, so a theme physically cannot supply
    /// these independently — this test is what makes an edit to [`build`]
    /// itself fail loudly.
    #[test]
    fn every_theme_derives_its_relationship_tokens_from_its_base_colours() {
        for t in THEMES {
            let p = &t.palette;
            let id = t.id;

            // The translucent panel fill is the panel colour, only thinner.
            assert_eq!(rgba_to_u8(p.surface_panel_translucent), rgba_to_u8(p.surface_panel), "{id}: translucent panel is a different colour");
            assert!(p.surface_panel_translucent.a < 1.0, "{id}: panel fill is opaque, discarding the native material");
            assert_eq!(p.surface_panel.a, 1.0, "{id}: the base panel colour must be opaque");

            // The menu tint is the raised colour, only thinner — and more
            // opaque than the panel, per `menu_glass_tint`'s own reasoning.
            assert_eq!(rgba_to_u8(p.menu_glass_tint), rgba_to_u8(p.surface_raised), "{id}: menu tint is a different colour");
            assert!(p.menu_glass_tint.a < 1.0, "{id}: menu tint is opaque");

            // The promotion rule, not a colour choice.
            assert_eq!(p.text_tertiary_on_selected, p.text_secondary, "{id}: the tertiary-on-selected promotion was decoupled from text_secondary");

            // One hairline hue, two weights, strong is exactly double.
            assert_eq!(rgba_to_u8(p.border_hairline), rgba_to_u8(p.border_hairline_strong), "{id}: the two hairline weights are different colours");
            assert!((p.border_hairline_strong.a - p.border_hairline.a * 2.0).abs() < 1e-6, "{id}: border_hairline_strong is not twice border_hairline");
            assert!(p.border_hairline_strong.a <= 1.0, "{id}: doubled hairline alpha overflowed");

            // State-derived tokens keep their state colour's hue.
            assert_eq!(rgba_to_u8(p.banner_danger_bg), rgba_to_u8(p.state_danger), "{id}: the danger banner is not built from state_danger");
            assert_eq!(p.banner_danger_bg.a, BANNER_ALPHA, "{id}: banner alpha drifted");
            assert_eq!(rgba_to_u8(p.state_danger_border), rgba_to_u8(p.state_danger), "{id}: the danger border is not built from state_danger");
            assert_eq!(rgba_to_u8(p.state_success_border), rgba_to_u8(p.state_success), "{id}: the success border is not built from state_success");
            assert_eq!(p.state_danger_border.a, STATE_BORDER_ALPHA, "{id}: danger border alpha drifted");
            assert_eq!(p.state_success_border.a, STATE_BORDER_ALPHA, "{id}: success border alpha drifted");

            // The icon plate is always a low-alpha film, never opaque chrome.
            assert!(p.row_icon_socket_bg.a > 0.0 && p.row_icon_socket_bg.a < 0.5, "{id}: the row-icon socket is not a low-alpha plate");
        }
    }

    /// The icon-plate polarity rule, which is the one thing the cozy-theme
    /// study (`data/neko-cozy-theme/report.md`) found breaks silently on a
    /// light theme: a pale film on a pale panel is invisible, so a light
    /// theme's plate has to be dark. Checked as "opposite polarity to the
    /// panel", not as a fixed hex, so a future theme can pick any hue it likes
    /// as long as the plate still reads.
    #[test]
    fn every_theme_has_an_icon_plate_that_reads_against_its_own_panel() {
        for t in THEMES {
            let panel = relative_luminance(t.palette.surface_panel);
            let plate = relative_luminance(t.palette.row_icon_socket_bg);
            match t.appearance {
                Appearance::Dark => assert!(plate > panel, "{}: a dark theme needs a lighter icon plate than its panel (panel {panel:.3}, plate {plate:.3})", t.id),
                Appearance::Light => assert!(plate < panel, "{}: a light theme needs a *darker* icon plate than its panel — a pale film vanishes on a pale surface (panel {panel:.3}, plate {plate:.3})", t.id),
            }
        }
    }

    /// **A theme that ships unreadable is worse than no theme**, so this is a
    /// hard gate rather than a report: every text-on-surface pair this app
    /// actually paints, on every built-in, at WCAG AA's 4.5:1 for normal text.
    ///
    /// Two pairs are deliberately absent and both are accounted for elsewhere:
    /// `text_tertiary` on `surface_selected` fails in *every* palette checked
    /// including the shipped neutral one (3.04:1) and is solved by the
    /// `text_tertiary_on_selected` promotion — which is itself checked here,
    /// through `text_secondary` on `surface_selected`. And nothing is checked
    /// against the *translucent* panel fill composited over a live desktop:
    /// that depends on the wallpaper, and this app's own worst case was
    /// measured separately in `data/neko-native-material/report.md` §6.
    #[test]
    fn themes_all_pass_wcag_aa() {
        const AA: f32 = 4.5;
        let mut failures = Vec::new();
        for t in THEMES {
            let p = &t.palette;
            let pairs: &[(&str, Rgba, &str, Rgba)] = &[
                ("text_primary", p.text_primary, "surface_panel", p.surface_panel),
                ("text_secondary", p.text_secondary, "surface_panel", p.surface_panel),
                ("text_tertiary", p.text_tertiary, "surface_panel", p.surface_panel),
                ("text_primary", p.text_primary, "surface_raised", p.surface_raised),
                ("text_secondary", p.text_secondary, "surface_raised", p.surface_raised),
                ("text_primary", p.text_primary, "surface_input", p.surface_input),
                ("text_primary", p.text_primary, "surface_selected", p.surface_selected),
                ("text_secondary", p.text_secondary, "surface_selected", p.surface_selected),
                ("state_success", p.state_success, "surface_panel", p.surface_panel),
                ("state_danger", p.state_danger, "surface_panel", p.surface_panel),
                ("text_on_light", p.text_on_light, "text_primary", p.text_primary),
            ];
            for (fg_name, fg, bg_name, bg) in pairs {
                let ratio = contrast(*fg, *bg);
                if ratio < AA {
                    failures.push(format!("{} — {fg_name} on {bg_name}: {ratio:.2}:1", t.id));
                }
            }
        }
        assert!(failures.is_empty(), "themes below WCAG AA 4.5:1:\n  {}", failures.join("\n  "));
    }

    /// A selected row has to be *visible* as a selected row. Contrast tuning
    /// can otherwise "fix" a failing text pair by walking `surface_selected`
    /// back into `surface_panel`, which trades a readability defect for a
    /// worse usability one.
    #[test]
    fn every_theme_has_a_visible_selection_step_away_from_its_panel() {
        for t in THEMES {
            let step = contrast(t.palette.surface_selected, t.palette.surface_panel);
            assert!(step >= 1.30, "{}: selected row is only {step:.2}:1 away from the panel — effectively invisible", t.id);
        }
    }

    /// A compile-time census of [`Palette`]. Adding or removing a field breaks
    /// this destructure, which is the point: [`PALETTE_TOKEN_COUNT`], this
    /// module's doc comment and `AGENTS.md` all quote the same number.
    #[test]
    fn the_palette_has_exactly_the_documented_number_of_colour_tokens() {
        let Palette {
            surface_panel,
            surface_panel_translucent,
            surface_raised,
            menu_glass_tint,
            surface_input,
            surface_selected,
            text_primary,
            text_secondary,
            text_tertiary,
            text_tertiary_on_selected,
            text_on_light,
            keycap_shell_bg,
            state_success,
            state_danger,
            state_success_border,
            state_danger_border,
            banner_danger_bg,
            border_hairline,
            border_hairline_strong,
            row_icon_socket_bg,
        } = THEMES[0].palette;
        let all = [
            surface_panel, surface_panel_translucent, surface_raised, menu_glass_tint,
            surface_input, surface_selected, text_primary, text_secondary, text_tertiary,
            text_tertiary_on_selected, text_on_light, keycap_shell_bg, state_success,
            state_danger, state_success_border, state_danger_border, banner_danger_bg,
            border_hairline, border_hairline_strong, row_icon_socket_bg,
        ];
        assert_eq!(all.len(), PALETTE_TOKEN_COUNT);
    }

    /// The client owns the colours, the daemon owns the list and the persisted
    /// choice — so the two registries have to name the same set. Without this,
    /// adding a theme here but not there leaves a row nobody can select, and
    /// the reverse leaves a row that silently does nothing.
    #[test]
    fn themes_match_the_protocol_registry() {
        let mut ours: Vec<&str> = THEMES.iter().map(|t| t.id).collect();
        let mut theirs: Vec<&str> = neko_protocol::BUILTIN_THEMES.iter().map(|t| t.id).collect();
        ours.sort_unstable();
        theirs.sort_unstable();
        assert_eq!(ours, theirs, "theme.rs and neko_protocol::BUILTIN_THEMES disagree about which themes exist");

        for t in THEMES {
            let listed = neko_protocol::BUILTIN_THEMES.iter().find(|b| b.id == t.id).unwrap();
            assert_eq!(listed.name, t.name, "{}: display name differs between the two registries", t.id);
        }
        assert!(theme_by_id(neko_protocol::DEFAULT_THEME_ID).is_some());
        assert_eq!(DEFAULT_THEME_ID, neko_protocol::DEFAULT_THEME_ID);
    }

    #[test]
    fn theme_ids_are_unique() {
        let mut ids: Vec<&str> = THEMES.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate theme id");
    }

    #[test]
    fn the_default_theme_leads_the_list_and_is_what_a_fresh_process_renders() {
        let _guard = test_lock();
        assert_eq!(THEMES[0].id, DEFAULT_THEME_ID);
        // ACTIVE starts at 0 and other tests restore it, so this is the
        // untouched-process state.
        set_active(DEFAULT_THEME_ID);
        assert_eq!(active_theme().id, DEFAULT_THEME_ID);
        assert_eq!(*active(), THEMES[0].palette);
    }

    #[test]
    fn setting_a_known_theme_swaps_every_token_at_once() {
        let _guard = test_lock();
        assert!(set_active("gruvbox-light"));
        assert_eq!(active_theme().id, "gruvbox-light");
        assert_eq!(active_theme().appearance, Appearance::Light);
        assert_eq!(*active(), theme_by_id("gruvbox-light").unwrap().palette);
        set_active(DEFAULT_THEME_ID);
    }

    #[test]
    fn an_unknown_theme_id_changes_nothing_rather_than_blanking_the_palette() {
        let _guard = test_lock();
        set_active("catppuccin-mocha");
        assert!(!set_active("no-such-theme"));
        assert_eq!(active_theme().id, "catppuccin-mocha", "a bad persisted id must leave the live palette alone");
        set_active(DEFAULT_THEME_ID);
    }

    #[test]
    fn every_theme_keeps_the_native_material_visible() {
        for t in THEMES {
            assert!(t.keeps_translucency(), "{}: an opaque theme discards the native window material", t.id);
        }
    }
}

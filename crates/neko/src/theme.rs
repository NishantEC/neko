// This module is "the token table as a real Rust module" in full, not just
// the subset this v1 slice happens to paint: `SURFACE_RAISED` is mouse-hover
// only (rows are keyboard-first here), `STATE_SUCCESS`/`STATE_DANGER` are
// onboarding's permission-granted/denied colours, and
// `PANEL_WIDTH_WITH_DETAIL_PX` is the clipboard-detail-pane width, a later
// feature. Keeping the full table here — correct and ready — beats trimming
// it to only what's painted today and redefining it piecemeal later.
#![allow(dead_code)]

//! The frozen design tokens from `data/neko-design/report.md` §1, re-toned
//! twice now. First to "monochrome with a hint of blue" (hue 252°–257°,
//! chroma 0.012–0.025) per the captain's own instruction at the time —
//! offered three cat-derived identity directions (amber eye, jade eye,
//! copper coat), he chose none of them: *"lets do monochrome with hint of
//! blue."* Then, on direct captain instruction again (`fm/neko-mode-visual`):
//! *"let's remove the blue tint altogether... let it be just naturally
//! there."* — reversing that hint-of-blue call for true neutral. Every
//! chrome token below now sits at **chroma 0** (pure grey: R=G=B once hue
//! contributes nothing) at **exactly the same lightness (`L`) as the blue
//! ramp** — a pure hue/chroma change, not a re-tone: contrast, hierarchy, and
//! the accessibility work already done against those `L` values all carry
//! over unchanged. No accent token exists in this file any more, and none
//! should be reintroduced without a fresh captain decision. Geometry,
//! layout, and every non-colour constant below are untouched.
//!
//! **State colours are the one deliberate exception, per the design's own
//! "chrome is monochrome; state is coloured" rule** — `STATE_SUCCESS`,
//! `STATE_DANGER`, and the tokens derived from them
//! (`STATE_SUCCESS_BORDER`/`STATE_DANGER_BORDER`/`BANNER_DANGER_BG`) keep
//! their original hue and chroma untouched by this pass.
//!
//! Every `Rgba` is generated from an OKLCH triple via Björn Ottosson's
//! OKLab→linear-sRGB matrices, the same method and hand-computed precision
//! the original report used. GPUI has no live `color-mix()`/OKLCH
//! interpolation (see report §6), so every token here is a plain constant,
//! the same shape `comet`'s `Theme` module uses (read for architecture only,
//! not copied — see `AGENTS.md`). The test at the bottom of this file checks
//! every value against an independently-implemented OKLCH→sRGB conversion.

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

const fn rgba_const(hex: u32, a: f32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a,
    }
}

pub const SURFACE_PANEL: Rgba = rgb_const(0x0d0d0d);
/// `SURFACE_PANEL`'s own RGB, at reduced alpha — the panel's fill when a
/// native material (`material.rs`) is behind the window. This is what
/// makes the material visible at all: GPUI renders this `div`'s fill on
/// top of the native background view in window z-order, so a fully opaque
/// fill (`SURFACE_PANEL`) would paint over it completely regardless of the
/// native view's own z-position. **Reasoned, not screenshot-verified
/// against a busy desktop** — this task's standing capture-safety rule
/// (`AGENTS.md`, "Window material") rules out the only capture mechanism
/// that could show it composited with a real/synthetic backdrop. Chosen
/// high (82%) specifically so the final on-screen color stays dominated by
/// this near-black tint rather than whatever the material blurs in behind
/// it: `data/neko-native-material/report.md` §6 measured every material's
/// own *worst case* (no panel tint on top at all) at 5.15:1 against a busy
/// backdrop, comfortably above WCAG AA's 4.5:1 floor; at 82% opacity the
/// backdrop's contribution to the final pixel is a fifth of that already-
/// comfortable case, so contrast here should sit close to `SURFACE_PANEL`'s
/// own fully-opaque, already-OKLCH-verified numbers (this file's own test,
/// below). Re-verify with a real screenshot once a safe capture path exists
/// for this machine, rather than trusting this reasoning indefinitely.
pub const SURFACE_PANEL_TRANSLUCENT: Rgba = rgba_const(0x0d0d0d, 0.82);
pub const SURFACE_RAISED: Rgba = rgb_const(0x161616);
/// The `⌘K` actions menu's own fill when the native menu-overlay material
/// installed (`material::install_menu_overlay`) — `SURFACE_RAISED`'s own
/// RGB, translucent, so the native blur genuinely shows through wherever the
/// menu overlaps content GPUI itself left translucent (nearly all of an
/// unselected row's own footprint — see `panel::render_row`), the same
/// "the panel's own fill has to be translucent for a material to be visible
/// at all" rule `SURFACE_PANEL_TRANSLUCENT` already establishes for the
/// whole window (`AGENTS.md`, "Window material"). Noticeably *more* opaque
/// than `SURFACE_PANEL_TRANSLUCENT` (0.82) — calibrated against comet's own
/// reference menu tint (`oklch(0.33 0 0 / 34%)`, read from
/// `data/helm/refs/comet/crates/ui/src/theme.rs` for inspiration only, not
/// copied) scaled up for legibility: this app has no real backdrop-blur
/// primitive behind that tint (see `material::install_menu_overlay`'s own
/// doc comment on the gap), so the fill alone carries more of the contrast
/// duty a true blur would otherwise share. Unused when the menu overlay
/// material didn't install (`panel::Root::menu_frost` false) — the menu
/// keeps its original, fully-opaque `SURFACE_RAISED` fill instead.
pub const MENU_GLASS_TINT: Rgba = rgba_const(0x161616, 0.62);
/// The clipboard-history detail pane's own recessed preview box
/// (`data/neko-design/mockups/12-first-clipboard-use.html`'s
/// `--surface-input`). Same `L` as the blue ramp's own `SURFACE_INPUT`
/// (`oklch(0.13 0.012 255)`), chroma taken to zero for the neutral re-tone —
/// not a fresh colour pick, so it doesn't reopen the closed colour-identity
/// question (see this module's own doc comment).
pub const SURFACE_INPUT: Rgba = rgb_const(0x070707);
pub const SURFACE_SELECTED: Rgba = rgb_const(0x3a3a3a);
pub const TEXT_PRIMARY: Rgba = rgb_const(0xe8e8e8);
pub const TEXT_SECONDARY: Rgba = rgb_const(0xa9a9a9);
/// `L=0.615`, unchanged from the blue ramp — only chroma moved to zero.
/// `data/neko-native-material/report.md` §6 measured this app's own
/// worst-case material (the Popover fallback, `material.rs`) reducing
/// placeholder-text contrast to ~94% of opaque (6.08:1 vs. 6.46:1 baseline
/// in that report's own sampling). The original warm tertiary sat at
/// exactly 4.53:1 against the panel — barely over WCAG AA's 4.5:1 floor
/// already, so a ~6% translucency haircut would have put it under AA on the
/// real (non-Glass) fallback path; the blue ramp's re-tone raised `L` to
/// 0.615 specifically to clear that with room to spare, and this neutral
/// pass preserves that exact `L` for the same reason — chroma is the only
/// axis this task was asked to change. This token measures **5.23:1** opaque
/// (this file's own test), which survives that same worst-case reduction
/// with room to spare (~4.9:1) while still reading distinctly dimmer than
/// `text_secondary`'s contrast — the hierarchy between the two tiers is
/// preserved.
pub const TEXT_TERTIARY: Rgba = rgb_const(0x848484);
/// State colour — deliberately untouched by the neutral re-tone (this
/// module's own doc comment: "chrome is monochrome; state is coloured").
pub const STATE_SUCCESS: Rgba = rgb_const(0x61bd67);
/// State colour — deliberately untouched by the neutral re-tone, same
/// reasoning as `STATE_SUCCESS` above.
pub const STATE_DANGER: Rgba = rgb_const(0xe96e50);

/// A selected row promotes its accessory text from `text_tertiary` to
/// `text_secondary` — the one scoped exception called out in the report
/// (`text_tertiary` on `surface_selected` measures 3.04:1, failing AA). The
/// promoted colour measures **4.84:1**, a genuine AA pass — unchanged in
/// substance from the blue ramp's own 4.81:1 (the tiny drift is chroma
/// leaving the luminance calculation, not an `L` change): `surface_selected`
/// keeps the blue ramp's `L=0.35`, chosen there specifically to clear this
/// pair for real rather than carry forward the original warm ramp's 4.20:1
/// near-miss. Not a new token, a paint-path rule.
pub const TEXT_TERTIARY_ON_SELECTED: Rgba = TEXT_SECONDARY;

/// Decorative dividers only, per the report's own note on this token —
/// never a load-bearing boundary. Onboarding's dialog chrome (header/footer
/// hairlines, permission-row outlines, keycap borders) is the first paint
/// path in this codebase to use these as named tokens rather than an inline
/// `rgba(0xffffff__)` literal. `L≈0.90`, unchanged from the blue ramp;
/// chroma taken to zero for the neutral re-tone, same as every other token
/// in this file.
pub const BORDER_HAIRLINE: Rgba = rgba_const(0xdedede, 0.08);
pub const BORDER_HAIRLINE_STRONG: Rgba = rgba_const(0xdedede, 0.16);

/// Text sitting on a `TEXT_PRIMARY` fill (the primary button's own label).
pub const TEXT_ON_LIGHT: Rgba = rgb_const(0x0f0f0f);

/// Onboarding's keycap chip background (`keycap_shell()` in `view.rs`) —
/// previously an inline `rgba(0x2a221aff)` literal never routed through
/// this module. `L=0.255`, unchanged from the blue ramp (`oklch(0.255 0.020
/// 255)` there; chroma zeroed here), sitting between `SURFACE_RAISED` and
/// `SURFACE_SELECTED` in lightness, matching the same relationship the
/// original warm-ramp literal had to its own equivalent surfaces.
pub const KEYCAP_SHELL_BG: Rgba = rgb_const(0x232323);

/// The accessibility-declined banner's background (`render_accessibility_banner`
/// in `panel.rs`) — `STATE_DANGER`'s own hue at ~7.8% alpha. State colour,
/// deliberately untouched by the neutral re-tone (this module's own doc
/// comment: "chrome is monochrome; state is coloured").
pub const BANNER_DANGER_BG: Rgba = rgba_const(0xe96e50, 0.078_431_37);

/// Status-pill borders (`status_pill` in `onboarding/view.rs`) — state
/// colours at ~34.9% alpha, deliberately untouched by the neutral re-tone,
/// same reasoning as `BANNER_DANGER_BG` above.
pub const STATE_SUCCESS_BORDER: Rgba = rgba_const(0x61bd67, 0.349_019_6);
pub const STATE_DANGER_BORDER: Rgba = rgba_const(0xe96e50, 0.349_019_6);

pub const PANEL_RADIUS_PX: f32 = 16.0;
pub const ROW_RADIUS_PX: f32 = 8.0;
pub const DIALOG_RADIUS_PX: f32 = 14.0;
pub const BTN_RADIUS_PX: f32 = 8.0;
pub const CHIP_RADIUS_PX: f32 = 5.0;

pub const INPUT_ROW_HEIGHT_PX: f32 = 56.0;
pub const RESULT_ROW_HEIGHT_PX: f32 = 40.0;
pub const SECTION_HEADER_HEIGHT_PX: f32 = 28.0;
pub const FOOTER_HEIGHT_PX: f32 = 44.0;
pub const PANEL_WIDTH_PX: f32 = 680.0;
pub const PANEL_WIDTH_WITH_DETAIL_PX: f32 = 760.0;
/// Half the gap between the two panel widths above — the horizontal inset
/// the root list's own 680px content sits at within the real `NSWindow`,
/// which is now always `PANEL_WIDTH_WITH_DETAIL_PX` wide (see `AGENTS.md`,
/// "Mode view resize seam"). Centering the narrower panel inside the fixed
/// window this way, rather than resizing the window itself for a mode
/// transition, is what keeps the root list landing in the exact same
/// on-screen position `upper_third_offset`'s own 680px-based formula placed
/// it at before that fix — `(680 - 760) / 2` cancels back out once this
/// inset is added on top of the window's own 760px-based centering.
pub const PANEL_ROOT_INSET_PX: f32 = (PANEL_WIDTH_WITH_DETAIL_PX - PANEL_WIDTH_PX) / 2.0;
/// The two-column mode view's fixed left (list) column width —
/// `data/neko-design/mockups/12-first-clipboard-use.html`'s
/// `.panel-list.with-detail { flex: 0 0 264px }`. The detail pane takes
/// whatever's left of `PANEL_WIDTH_WITH_DETAIL_PX`.
pub const MODE_LIST_COLUMN_WIDTH_PX: f32 = 264.0;
pub const ROW_ICON_PX: f32 = 22.0;
pub const ROW_ICON_RADIUS_PX: f32 = 6.0;
/// The row-icon slot's own backing plate — `design.css`'s `.row-icon`
/// background, `TEXT_PRIMARY`'s hex at 6% alpha (also reused for
/// `.type-tag`'s background, the same token). Applied behind every app
/// icon, not just the not-yet-cached placeholder: real macOS icon PNGs
/// (`NSWorkspace.iconForFile`) commonly bake in their own transparent
/// padding at wildly different ratios per app, so a raw icon floats at an
/// inconsistent visual size with nothing to anchor it against the dark
/// panel. A shared, low-alpha, tinted socket (not a neutral white) gives
/// every icon the same backing plate regardless of how much of its own
/// padding shows through — the fix for icons reading as "bright stamps"
/// mismatched in shape and brightness against the rest of the list.
pub const ROW_ICON_SOCKET_BG: Rgba = rgba_const(0xe8e8e8, 0.06);

/// The results list's own scroll edge-fade band height
/// (`edge_fade.rs`) — tall enough to read as a gradual dissolve rather
/// than a hard cutoff at `RESULT_ROW_HEIGHT_PX` (40), short enough that it
/// never covers more than about half of one row.
pub const EDGE_FADE_BAND_PX: f32 = 18.0;

/// Onboarding window geometry (design report §3): the same panel width as
/// the summoned popup, a static header standing in for the input row, a
/// fixed content area sized for the tallest step (01's two permission
/// rows), and the same footer height — matching `panel.rs`'s own "one
/// fixed size, not dynamic per-content resize" v1 simplification rather
/// than reopening that already-settled call for a second window.
pub const ONBOARDING_CONTENT_HEIGHT_PX: f32 = 420.0;
pub const ONBOARDING_HEIGHT_PX: f32 =
    INPUT_ROW_HEIGHT_PX + ONBOARDING_CONTENT_HEIGHT_PX + FOOTER_HEIGHT_PX;

/// Left padding the onboarding header reserves so its own content (the neko
/// glyph + wordmark) never sits under macOS's real traffic-light cluster,
/// inset at `TitlebarOptions::traffic_light_position` (14px, 14px) — same
/// clearance comet's own `titlebar_cluster_start` reserves for a windowed
/// mac titlebar. Fullscreen hides the lights, so the reservation collapses
/// to comet's fullscreen value there instead (see `view.rs`).
pub const ONBOARDING_TRAFFIC_LIGHT_CLEARANCE_PX: f32 = 88.0;
pub const ONBOARDING_TRAFFIC_LIGHT_CLEARANCE_FULLSCREEN_PX: f32 = 12.0;
pub const ONBOARDING_HEADER_BASE_PADDING_PX: f32 = 20.0;

/// Björn Ottosson's OKLab↔linear-sRGB matrices
/// (<https://bottosson.github.io/posts/oklab/>), implemented independently
/// from the published formulas — not ported from any reference app — to
/// cross-check the constants above against this module's own OKLCH
/// derivation in a test, per the original report's own instruction: "If any
/// value in that table looks wrong to you, report it — do not silently
/// correct it." (None did; see the test below.)
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

    /// The neutral-re-tone base-palette table (`fm/neko-mode-visual`): every
    /// chrome token's `L` carried over unchanged from the blue ramp, chroma
    /// taken to zero — `h` is left at the blue ramp's own value for direct
    /// diffability against that table (and this module's own history), but
    /// is mathematically inert at `c=0.0`. `state_success`/`state_danger`
    /// keep their original hue *and* chroma — state colours, not chrome (see
    /// this module's own doc comment). If this test ever fails, this
    /// module's hex and its own OKLCH triple disagree — that's the "report
    /// it" case, not a silent-fix case.
    #[test]
    fn base_palette_matches_the_frozen_oklch_table() {
        let table: &[(&str, f32, f32, f32, Rgba)] = &[
            ("surface_panel", 0.16, 0.0, 255.0, SURFACE_PANEL),
            ("surface_raised", 0.20, 0.0, 255.0, SURFACE_RAISED),
            ("surface_input", 0.13, 0.0, 255.0, SURFACE_INPUT),
            ("surface_selected", 0.35, 0.0, 255.0, SURFACE_SELECTED),
            ("text_primary", 0.93, 0.0, 257.0, TEXT_PRIMARY),
            ("text_secondary", 0.735, 0.0, 255.0, TEXT_SECONDARY),
            ("text_tertiary", 0.615, 0.0, 252.0, TEXT_TERTIARY),
            ("state_success", 0.72, 0.150, 145.0, STATE_SUCCESS),
            ("state_danger", 0.68, 0.160, 35.0, STATE_DANGER),
            ("text_on_light", 0.17, 0.0, 255.0, TEXT_ON_LIGHT),
            ("keycap_shell_bg", 0.255, 0.0, 255.0, KEYCAP_SHELL_BG),
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

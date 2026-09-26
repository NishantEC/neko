//! The substrate every vendored `gpui-component` file expects, re-expressed
//! against neko's own palette and geometry.
//!
//! **Why this exists at all.** `gpui-component` cannot be a dependency here:
//! it publishes against the crates.io `gpui`, neko runs the `wingleeio` fork,
//! and forcing them onto one gpui produces 81 compile errors of genuine API
//! drift (`Corner` and `Timer` do not exist in the fork; `ScrollHandle::
//! max_offset` returns `Point` rather than `Size`; `focus` takes two arguments
//! rather than one; `BoxShadow` gained an `inset` field). So its components are
//! vendored file by file and repaired — and every one of them reaches for the
//! same handful of library-private traits, which are supplied here once instead
//! of being patched out of each file separately.
//!
//! **A vendored component never learns which theme is active.** It asks
//! `cx.theme()` for a colour exactly as it did upstream; [`ThemeShim`] answers
//! from [`theme::active`], so all seventeen palettes drive vendored components
//! with no per-theme asset and no `if themed` branch — the same rule the rest
//! of this app's paint sites already follow.
//!
//! **Mapping is deliberate, not mechanical.** `gpui-component`'s theme carries
//! roles neko's palette has no token for, and inventing tokens to match a
//! vendored library's vocabulary would be the tail wagging the dog. Those roles
//! are *derived* here instead — `warning` is the midpoint of the success→danger
//! ramp neko already computes in OKLCH, the scrollbar colours are `text_primary`
//! at two alphas — so a theme still supplies values and can never redefine what
//! a token means.

// Upstream's full public API is kept even where this app calls only part of it,
// so a `curl`-and-diff against the pinned upstream version stays meaningful.
#![allow(dead_code)]
use gpui::{App, Div, Hsla, Pixels, SharedString, Styled as _, div, px};

use crate::theme;

/// When a scrollbar is visible.
///
/// Upstream this lives on the theme and is serialised; here it is a plain enum
/// because neko has no user-facing setting for it and `serde`/`schemars` on a
/// three-variant enum would be the only reason those crates appeared in this
/// module's dependency list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarShow {
    /// Visible while scrolling, fading out once idle.
    #[default]
    Scrolling,
    /// Visible while the pointer is over the scrollable region.
    Hover,
    /// Always visible.
    Always,
}

impl ScrollbarShow {
    pub fn is_hover(&self) -> bool {
        matches!(self, Self::Hover)
    }

    pub fn is_always(&self) -> bool {
        matches!(self, Self::Always)
    }
}

/// The colour and geometry surface a vendored component reads.
///
/// Returned by value rather than by reference: [`theme::active`] hands back a
/// `&'static Palette` whose *contents* change when the theme does, and a shim
/// holding a reference would have to be rebuilt on every swap anyway. Building
/// it is ~40 field copies with no allocation, which is cheaper than the atomic
/// bookkeeping a cached alternative would need.
#[derive(Debug, Clone)]
pub struct ThemeShim {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted_foreground: Hsla,
    pub primary: Hsla,
    pub primary_foreground: Hsla,
    pub secondary: Hsla,
    pub secondary_foreground: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub popover: Hsla,
    pub popover_foreground: Hsla,
    pub danger: Hsla,
    pub danger_foreground: Hsla,
    pub success: Hsla,
    pub success_foreground: Hsla,
    pub warning: Hsla,
    pub warning_foreground: Hsla,
    pub info: Hsla,
    pub info_foreground: Hsla,
    pub red: Hsla,
    /// Always fully transparent. neko draws no box shadows: both of the ones it
    /// used to draw were measured spilling into the panel's own margin and were
    /// removed (`AGENTS.md`, "The panel shadow tent"). A vendored component that
    /// asks for a shadow colour gets nothing rather than reopening that.
    pub shadow: Hsla,
    pub skeleton: Hsla,
    pub scrollbar: Hsla,
    pub scrollbar_thumb: Hsla,
    pub scrollbar_thumb_hover: Hsla,
    pub scrollbar_show: ScrollbarShow,
    pub switch: Hsla,
    pub switch_thumb: Hsla,
    pub slider_bar: Hsla,
    pub slider_thumb: Hsla,
    pub progress_bar: Hsla,
    pub accordion: Hsla,
    pub accordion_hover: Hsla,
    pub transparent: Hsla,
    pub radius: Pixels,
    pub radius_lg: Pixels,
    pub is_dark: bool,
    pub font_family: SharedString,
}

/// Alpha for a resting scrollbar thumb, and for a hovered one.
///
/// Two values rather than two palette tokens: a scrollbar thumb is
/// `text_primary` seen through more or less of the surface behind it, which is
/// a rendering rule rather than a colour a theme should be able to disagree
/// about — the same reasoning `snap_guide_muted` follows in being derived from
/// `snap_guide` rather than authored beside it.
const THUMB_ALPHA: f32 = 0.24;
const THUMB_HOVER_ALPHA: f32 = 0.44;

fn alpha(color: gpui::Rgba, a: f32) -> Hsla {
    gpui::Rgba { a, ..color }.into()
}

impl ThemeShim {
    fn current() -> Self {
        let p = theme::active();
        // Neither role exists in neko's palette, and neither should: `warning`
        // is exactly "half way between fine and not fine", which is what the
        // meter's own ramp already computes, in OKLCH, for every theme.
        let warning = theme::ramp(p.state_success, p.state_danger, 0.5);
        Self {
            background: p.surface_panel.into(),
            foreground: p.text_primary.into(),
            muted_foreground: p.text_secondary.into(),
            // neko has no accent token — it was removed during the palette
            // re-tone and its absence is deliberate ("chrome is monochrome;
            // state is coloured"). `primary` is therefore the strongest
            // foreground, not a hue.
            primary: p.text_primary.into(),
            primary_foreground: p.text_on_light.into(),
            secondary: p.surface_raised.into(),
            secondary_foreground: p.text_primary.into(),
            border: p.border_hairline.into(),
            input: p.surface_input.into(),
            popover: p.surface_raised.into(),
            popover_foreground: p.text_primary.into(),
            danger: p.state_danger.into(),
            danger_foreground: p.text_on_light.into(),
            success: p.state_success.into(),
            success_foreground: p.text_on_light.into(),
            warning: warning.into(),
            warning_foreground: p.text_on_light.into(),
            info: p.text_secondary.into(),
            info_foreground: p.text_on_light.into(),
            red: p.state_danger.into(),
            shadow: gpui::Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }
            .into(),
            skeleton: p.row_icon_socket_bg.into(),
            // The track stays invisible: the panel is translucent over a live
            // native material, and a filled track would be an opaque stripe
            // through it.
            scrollbar: gpui::Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }
            .into(),
            scrollbar_thumb: alpha(p.text_primary, THUMB_ALPHA),
            scrollbar_thumb_hover: alpha(p.text_primary, THUMB_HOVER_ALPHA),
            scrollbar_show: ScrollbarShow::default(),
            switch: p.surface_selected.into(),
            switch_thumb: p.text_primary.into(),
            slider_bar: p.text_primary.into(),
            slider_thumb: p.text_primary.into(),
            progress_bar: p.text_primary.into(),
            accordion: p.surface_panel.into(),
            accordion_hover: p.surface_selected.into(),
            transparent: gpui::Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }
            .into(),
            radius: px(theme::ROW_RADIUS_PX),
            radius_lg: px(theme::DIALOG_RADIUS_PX),
            is_dark: is_dark(p.surface_panel),
            font_family: SharedString::from(theme::MONOSPACE_FAMILY),
        }
    }
}

/// Whether a surface is dark enough that a vendored component should treat the
/// theme as dark.
///
/// Relative luminance rather than a flag on the palette, for the same reason
/// `every_theme_has_an_icon_plate_that_reads_against_its_own_panel` measures
/// luminance rather than pinning a hex: a theme declaring itself dark while
/// shipping a cream panel would be believed.
pub fn is_dark(surface: gpui::Rgba) -> bool {
    fn linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    let l = 0.2126 * linear(surface.r) + 0.7152 * linear(surface.g) + 0.0722 * linear(surface.b);
    l < 0.5
}

/// `cx.theme()`, exactly as a vendored component calls it upstream.
pub trait ActiveTheme {
    fn theme(&self) -> ThemeShim;
}

impl ActiveTheme for App {
    fn theme(&self) -> ThemeShim {
        ThemeShim::current()
    }
}

impl ActiveTheme for gpui::Window {
    fn theme(&self) -> ThemeShim {
        ThemeShim::current()
    }
}

/// A row that centres its children on the cross axis.
pub fn h_flex() -> Div {
    div().flex().flex_row().items_center()
}

/// A column.
pub fn v_flex() -> Div {
    div().flex().flex_col()
}

/// The styling helpers vendored components chain onto their own elements.
pub trait StyledExt: gpui::Styled + Sized {
    /// Merge a caller-supplied `StyleRefinement` over what the component set.
    ///
    /// This is how a vendored component honours `.w_full()`/`.h_4()` written by
    /// its caller: the component stores the refinement and applies it last.
    fn refine_style(mut self, style: &gpui::StyleRefinement) -> Self {
        use gpui::Refineable as _;
        self.style().refine(style);
        self
    }

    fn h_flex(self) -> Self {
        self.flex().flex_row().items_center()
    }

    fn v_flex(self) -> Self {
        self.flex().flex_col()
    }
}

impl<E: gpui::Styled> StyledExt for E {}

/// The size scale vendored components size themselves against.
///
/// `Size::Size(..)` keeps upstream's own name because vendored files construct
/// it by that name; renaming it here would make every future diff noise.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Size {
    Size(Pixels),
    XSmall,
    Small,
    #[default]
    Medium,
    Large,
}

impl From<Pixels> for Size {
    fn from(value: Pixels) -> Self {
        Self::Size(value)
    }
}

pub trait Sizable: Sized {
    fn with_size(self, size: impl Into<Size>) -> Self;

    fn xsmall(self) -> Self {
        self.with_size(Size::XSmall)
    }

    fn small(self) -> Self {
        self.with_size(Size::Small)
    }

    fn large(self) -> Self {
        self.with_size(Size::Large)
    }
}

/// `Axis::is_vertical()` / `is_horizontal()`, which the fork's `Axis` lacks.
pub trait AxisExt {
    fn is_vertical(&self) -> bool;
    fn is_horizontal(&self) -> bool;
}

impl AxisExt for gpui::Axis {
    fn is_vertical(&self) -> bool {
        matches!(self, gpui::Axis::Vertical)
    }

    fn is_horizontal(&self) -> bool {
        matches!(self, gpui::Axis::Horizontal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_theme_produces_a_usable_shim() {
        // A vendored component reads `cx.theme()` on every paint. A palette
        // that produced a panicking or degenerate shim would take out whichever
        // surface happened to use a vendored component first, and only in that
        // theme — the exact shape of bug that survives a review.
        let _guard = theme::test_lock();
        for t in theme::THEMES {
            assert!(theme::set_active(t.id));
            let shim = ThemeShim::current();
            assert!(shim.radius > px(0.0), "{}: radius", t.id);
            assert_eq!(shim.shadow.a, 0.0, "{}: neko draws no box shadow", t.id);
            assert_eq!(shim.scrollbar.a, 0.0, "{}: the track stays invisible", t.id);
            assert!(
                shim.scrollbar_thumb_hover.a > shim.scrollbar_thumb.a,
                "{}: hovering a thumb must strengthen it",
                t.id
            );
        }
        theme::set_active(theme::DEFAULT_THEME_ID);
    }

    #[test]
    fn a_light_panel_is_not_reported_as_dark() {
        // Luminance, not a flag: a theme declaring itself dark while shipping a
        // cream panel would otherwise be believed.
        assert!(is_dark(gpui::Rgba {
            r: 0.1,
            g: 0.1,
            b: 0.1,
            a: 1.0
        }));
        assert!(!is_dark(gpui::Rgba {
            r: 0.98,
            g: 0.96,
            b: 0.92,
            a: 1.0
        }));
    }
}

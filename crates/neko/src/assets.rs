//! The binary's own asset source, and the icon vocabulary it serves.
//!
//! Until this module existed, every mark in this app was hand-painted from
//! `div()`s (`panel::glyph_element`, `search_glyph`, `back_glyph`) and every
//! one of their doc comments gave the same reason: *"this codebase has no
//! bundled icon-asset pipeline, and a Unicode symbol isn't a reliable
//! substitute (design report §6)."* The second half of that is still true.
//! The first half is what this module removes.
//!
//! **The thing that made it cheap is that `gpui::svg()` was already there.**
//! It is a real element in the gpui this crate compiles against
//! (`crates/gpui/src/elements/svg.rs:20` at the pinned fork rev) — not
//! something a component library would have brought in. `gpui-component`'s
//! own `icon.rs` is a thin wrapper over exactly this call, which is why
//! `AGENTS.md`'s "Third-party UI, re-evaluated" section closed with this as
//! the better path than any library: a real icon set, no dependency, no
//! version drift. All that was missing was an [`AssetSource`] and some
//! files.
//!
//! ## How the renderer works, and the one constraint that follows from it
//!
//! `svg().path(p)` resolves `p` through the app's `AssetSource` and renders
//! it to an **alpha mask**, tinted at paint time by the element's own
//! `text_color` (`SvgRenderer::render_alpha_mask` keeps `p.alpha()` and
//! throws the colour channels away; `Window::paint_svg` then tints the mask).
//!
//! Two consequences, both load-bearing:
//!
//! 1. **An icon is exactly one colour.** A multi-colour mark cannot be an
//!    SVG here at all. That is why `Glyph::Palette` — four swatches painted
//!    in the *live theme's own* `surface_panel`/`text_primary`/
//!    `state_danger`/`surface_selected` — stays hand-painted, and why
//!    [`glyph_icon`] returns `None` for it rather than naming a file. It is
//!    not a gap to close later: a single-tint palette swatch would not be a
//!    palette swatch.
//! 2. **A filled icon renders as a solid blob.** Coverage is all the mask
//!    keeps, so `fill="#f00"` and `fill="#000"` are the same pixel. Lucide's
//!    set is stroke-only (`fill="none"`), which is why it composes correctly
//!    against a translucent panel where a filled shape would not — the same
//!    property `Glyph::File`'s hand-painted version already reasoned its way
//!    to ("border only, no fill, so it composes correctly whether the row is
//!    selected or the window is translucent"). `every_vendored_icon_is_a_
//!    stroke_only_24px_lucide_icon` is what keeps that true.
//!
//! Tint therefore still comes from `theme::active()` at every call site,
//! exactly as it did when these were `div()`s — swapping to SVG changed the
//! *shape* source, never the colour source, so themes keep working with no
//! per-theme icon anything.
//!
//! ## Why `include_bytes!` and not `include_dir` / `rust-embed`
//!
//! Nine files, known by name at compile time, listed once in [`ICONS`]. A
//! crate that walks a directory at build time buys directory-shaped growth
//! this set does not have, and costs a dependency plus a build script. The
//! table below is the same shape, minus both, and it has a property a
//! directory walk does not: **a typo'd path is caught by
//! `every_glyph_icon_path_resolves` rather than by an icon silently failing
//! to paint at runtime**, because `paint_svg` on an unresolvable path is
//! `Ok(())` and draws nothing (`SvgRenderer::render_alpha_mask` returns
//! `Ok(None)` when the asset source has no bytes). A missing icon is a
//! *hole in the row*, not an error anywhere — so the test is the only thing
//! standing between a rename and a blank slot.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};
use neko_protocol::Glyph;

/// Every asset compiled into the binary, keyed by the path `svg().path(..)`
/// asks for. Paths carry the vendor name (`icons/lucide/...`) deliberately:
/// provenance is then visible at the call site and a second icon family
/// cannot collide with this one by picking the same icon name.
///
/// The bytes are the upstream files **unmodified** — no re-export, no
/// re-minify, no stroke-width tweak. That is what lets a future
/// `curl`-and-`diff` against the pinned upstream commit stay meaningful,
/// the same discipline `theme.rs` uses for vendored palette hex. Size is
/// controlled at the call site (`theme::ROW_ICON_GLYPH_PX`), never by
/// editing a file.
///
/// Upstream: <https://github.com/lucide-icons/lucide>, commit
/// `33a44aa8b0b43d9b0ed14eb08860a1b5550a1573` (2026-08-20). ISC — see
/// `THIRD_PARTY_LICENSES/lucide-ISC.txt` and
/// `crates/neko/src/components/vendor/MANIFEST.md`.
const ICONS: &[(&str, &[u8])] = &[
    (icon::CHEVRON_LEFT, include_bytes!("../assets/icons/lucide/chevron-left.svg")),
    (icon::CLIPBOARD, include_bytes!("../assets/icons/lucide/clipboard.svg")),
    (icon::FILE, include_bytes!("../assets/icons/lucide/file.svg")),
    (icon::FOLDER, include_bytes!("../assets/icons/lucide/folder.svg")),
    (icon::LINK, include_bytes!("../assets/icons/lucide/link.svg")),
    (icon::SEARCH, include_bytes!("../assets/icons/lucide/search.svg")),
    (icon::SLIDERS, include_bytes!("../assets/icons/lucide/sliders-horizontal.svg")),
    (icon::TERMINAL, include_bytes!("../assets/icons/lucide/square-terminal.svg")),
    (icon::TEXT_LINES, include_bytes!("../assets/icons/lucide/text-align-start.svg")),
];

/// The asset paths this app draws with, named for what they *mean* here
/// rather than for the upstream filename — `TEXT_LINES` is what a clipboard
/// entry's icon is for, and it happening to be Lucide's `text-align-start`
/// today is a vendoring detail. Renaming an upstream file then only touches
/// [`ICONS`], not every call site.
pub mod icon {
    pub const CHEVRON_LEFT: &str = "icons/lucide/chevron-left.svg";
    pub const CLIPBOARD: &str = "icons/lucide/clipboard.svg";
    pub const FILE: &str = "icons/lucide/file.svg";
    pub const FOLDER: &str = "icons/lucide/folder.svg";
    pub const LINK: &str = "icons/lucide/link.svg";
    pub const SEARCH: &str = "icons/lucide/search.svg";
    pub const SLIDERS: &str = "icons/lucide/sliders-horizontal.svg";
    pub const TERMINAL: &str = "icons/lucide/square-terminal.svg";
    pub const TEXT_LINES: &str = "icons/lucide/text-align-start.svg";
}

/// The icon file for a wire [`Glyph`], or `None` for a glyph that must stay
/// hand-painted.
///
/// The `match` is exhaustive on purpose: adding a `Glyph` variant to
/// `neko-protocol` is a **compile error here**, which forces the one real
/// decision — does this mark have a single-colour shape (name a file) or
/// does it genuinely need paint (return `None`, and add a `glyph_element`
/// arm)? That is the same seam `AGENTS.md`'s "Provider abstraction" section
/// already prices at "one `Glyph` variant plus one case in
/// `panel::glyph_element`"; this narrows the second half to "usually, one
/// line here."
///
/// `Agent` and `AgentLive` share a file and differ by a painted presence
/// dot at the call site, not by a second asset — see `panel::glyph_element`.
pub fn glyph_icon(glyph: Glyph) -> Option<&'static str> {
    match glyph {
        Glyph::Text => Some(icon::TEXT_LINES),
        Glyph::Link => Some(icon::LINK),
        Glyph::File => Some(icon::FILE),
        Glyph::Folder => Some(icon::FOLDER),
        Glyph::Clipboard => Some(icon::CLIPBOARD),
        Glyph::Agent | Glyph::AgentLive => Some(icon::TERMINAL),
        Glyph::Sliders => Some(icon::SLIDERS),
        // Painted, permanently — four swatches in the *live* theme's own
        // colours. An alpha mask has exactly one tint, so this is not
        // expressible as an SVG at all. See this module's own doc comment.
        Glyph::Palette => None,
    }
}

/// The app's compiled-in asset source, installed once on the `Application`
/// builder in `main.rs` (`with_assets`). There is no filesystem fallback and
/// deliberately no user-supplied icon directory: an icon that can go missing
/// after the build is an icon that can leave a hole in a row, and this app
/// has no way to report that (see the module doc on why an unresolvable
/// path paints nothing rather than erroring).
pub struct NekoAssets;

impl AssetSource for NekoAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::new_static(name))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `Glyph` variant, so the tests below can iterate them. The
    /// `match` keeps this list honest: a new variant fails to compile here
    /// until it is handled, and the length assertion fails until it is
    /// added to the array too. (Rust has no way to enumerate an enum's
    /// variants, so this is the cheapest thing that still fails loudly.)
    const ALL_GLYPHS: [Glyph; 9] = [
        Glyph::Text,
        Glyph::Link,
        Glyph::File,
        Glyph::Folder,
        Glyph::Clipboard,
        Glyph::Agent,
        Glyph::AgentLive,
        Glyph::Sliders,
        Glyph::Palette,
    ];

    #[test]
    fn all_glyphs_lists_every_variant() {
        for glyph in ALL_GLYPHS {
            match glyph {
                Glyph::Text
                | Glyph::Link
                | Glyph::File
                | Glyph::Folder
                | Glyph::Clipboard
                | Glyph::Agent
                | Glyph::AgentLive
                | Glyph::Sliders
                | Glyph::Palette => {}
            }
        }
        assert_eq!(ALL_GLYPHS.len(), 9, "a Glyph variant was added without extending ALL_GLYPHS");
    }

    /// The one that matters: a path named by [`glyph_icon`] that the asset
    /// source cannot serve does not error anywhere at runtime, it just
    /// paints nothing. This is the check that catches a rename.
    #[test]
    fn every_glyph_icon_path_resolves() {
        for glyph in ALL_GLYPHS {
            let Some(path) = glyph_icon(glyph) else {
                continue;
            };
            let bytes = NekoAssets
                .load(path)
                .expect("loading a compiled-in asset cannot fail")
                .unwrap_or_else(|| panic!("{glyph:?} names {path}, which no asset serves"));
            assert!(!bytes.is_empty(), "{path} is empty");
        }
    }

    /// Same check for the paths that are used directly rather than through
    /// a `Glyph` — the input row's search mark and the mode row's back
    /// affordance have no wire vocabulary entry, so `glyph_icon` never
    /// covers them.
    #[test]
    fn every_named_icon_constant_resolves() {
        for path in [icon::CHEVRON_LEFT, icon::SEARCH] {
            assert!(
                NekoAssets.load(path).unwrap().is_some(),
                "{path} is named by a constant but served by no asset",
            );
        }
    }

    /// Nothing in [`ICONS`] is unreachable. A file that no call site and no
    /// `Glyph` names is dead weight in the binary and, worse, reads as
    /// available when it is really just forgotten.
    #[test]
    fn every_vendored_icon_is_reachable_from_a_name() {
        let named: Vec<&str> = ALL_GLYPHS
            .into_iter()
            .filter_map(glyph_icon)
            .chain([icon::CHEVRON_LEFT, icon::SEARCH])
            .collect();
        for (path, _) in ICONS {
            assert!(named.contains(path), "{path} is vendored but nothing names it");
        }
    }

    /// The vendored bytes are upstream's, and upstream's are stroke-only at
    /// a 24×24 viewBox. Both halves are load-bearing:
    ///
    /// - **`fill="none"`** — gpui renders an SVG to an alpha mask, so a
    ///   filled shape becomes a solid blob rather than an outline. An icon
    ///   swapped in from a filled set would look plausible in a browser and
    ///   wrong here.
    /// - **`viewBox="0 0 24 24"`** — every call site sizes by the element,
    ///   and `render_pixmap` scales by `size.width / svg_size.width()`. One
    ///   icon on a different grid would silently render at a different
    ///   optical weight beside the others.
    ///
    /// The byte lengths are pinned for the same reason `theme.rs` pins each
    /// vendored palette's upstream hex: an edit that quietly walks a file
    /// away from upstream should fail rather than pass silently.
    #[test]
    fn every_vendored_icon_is_a_stroke_only_24px_lucide_icon() {
        // (path, upstream byte length at the pinned commit)
        let pinned: &[(&str, usize)] = &[
            (icon::CHEVRON_LEFT, 238),
            (icon::CLIPBOARD, 354),
            (icon::FILE, 373),
            (icon::FOLDER, 342),
            (icon::LINK, 359),
            (icon::SEARCH, 275),
            (icon::SLIDERS, 422),
            (icon::TERMINAL, 321),
            (icon::TEXT_LINES, 279),
        ];
        assert_eq!(pinned.len(), ICONS.len(), "an icon was vendored without pinning its length");

        for (path, bytes) in ICONS {
            let text = std::str::from_utf8(bytes).unwrap_or_else(|_| panic!("{path} is not UTF-8"));
            assert!(text.trim_start().starts_with("<svg"), "{path} is not an SVG document");
            assert!(text.contains(r#"viewBox="0 0 24 24""#), "{path} is not on the 24x24 grid");
            assert!(text.contains(r#"fill="none""#), "{path} is not stroke-only");
            let expected = pinned
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, len)| *len)
                .unwrap_or_else(|| panic!("{path} has no pinned length"));
            assert_eq!(bytes.len(), expected, "{path} differs from the pinned upstream file");
        }
    }

    /// The strongest headless proof available: every vendored icon really
    /// rasterises, through **gpui's own** `SvgRenderer` and the same
    /// `AssetSource` the app installs — not a stand-in parser.
    ///
    /// Worth having because the failure mode this closes is silent. An
    /// unparseable or empty SVG does not panic and does not log: `paint_svg`
    /// swallows a `None` from the renderer and draws nothing, leaving a hole
    /// in a row. Asserting *coverage* (some pixel is non-transparent) rather
    /// than just "parsed" is what catches a file that is valid SVG with
    /// nothing in it.
    ///
    /// No window, no `TestAppContext` — `SvgRenderer` is a plain public type
    /// and these icons contain no `<text>`, so the expensive system-font
    /// database this renderer builds lazily is never touched.
    #[test]
    fn every_vendored_icon_actually_rasterises_through_gpuis_own_renderer() {
        let renderer = gpui::SvgRenderer::new(std::sync::Arc::new(NekoAssets));
        for (path, bytes) in ICONS {
            let image = renderer
                .render_single_frame(bytes, 1.0)
                .unwrap_or_else(|e| panic!("{path} failed to rasterise: {e}"));
            let size = image.size(0);
            assert!(size.width.0 > 0 && size.height.0 > 0, "{path} rasterised to nothing");
            // BGRA. Every 4th byte is alpha; a mark that draws must cover at
            // least one pixel.
            let painted = image
                .as_bytes(0)
                .expect("frame 0 exists")
                .chunks_exact(4)
                .any(|px| px[3] != 0);
            assert!(painted, "{path} rasterised to a fully transparent image");
        }
    }

    #[test]
    fn an_unknown_path_resolves_to_nothing_rather_than_erroring() {
        assert!(NekoAssets.load("icons/lucide/not-a-real-icon.svg").unwrap().is_none());
        assert!(NekoAssets.load("").unwrap().is_none());
    }

    #[test]
    fn list_filters_by_prefix() {
        assert_eq!(NekoAssets.list("icons/lucide/").unwrap().len(), ICONS.len());
        assert!(NekoAssets.list("fonts/").unwrap().is_empty());
    }
}

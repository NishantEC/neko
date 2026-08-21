//! Native macOS window material: `NSGlassEffectView` ("Liquid Glass") where
//! the OS ships it, a custom `.popover` `NSVisualEffectView` beneath that,
//! plain opaque as the last resort if installing either one errors. This
//! implements `data/neko-native-material/report.md` (firstmate home) — a
//! feasibility investigation that proved every part of this chain working
//! in running Rust on this exact machine; §8 is the recommendation this
//! follows, §9 the reusable code path.
//!
//! # The one invariant that matters
//!
//! Background material must be inserted into `window.contentView()` — the
//! real `NSWindow`'s true root view — positioned *below*, as a sibling.
//! **Never** into the `NSView` that `raw-window-handle` hands out directly:
//! that view is GPUI's own rendering view, and a subview renders *above*
//! its superview's own layer-drawn content, so inserting there silently
//! eats every pixel GPUI draws, text included (the investigation hit this
//! exact bug first — report §2). `root_content_view` below is the one
//! function that walks from the rendering view up to its owning `NSWindow`
//! and back down to the window's true content view; both install functions
//! insert there, never into what `window.window_handle()` returns.

use gpui::{Window, WindowBackgroundAppearance};

/// The window background GPUI itself should use — `Transparent`, always:
/// every material below (including the final opaque fallback) works by
/// installing or omitting a native background view behind an otherwise
/// transparent window, not by GPUI's own `Blurred` (which hard-codes
/// `NSVisualEffectMaterial.Selection` with no public choice of material —
/// see `AGENTS.md`, "Window material"). If `install` returns `Err`, the
/// caller flips this back to `Opaque` explicitly via
/// `Window::set_background_appearance`.
pub fn window_background() -> WindowBackgroundAppearance {
    WindowBackgroundAppearance::Transparent
}

/// Which chain step actually got installed. `panel.rs` only needs to know
/// whether *some* translucent material is behind the window — both
/// variants get the same panel tint — the distinction otherwise exists for
/// logging and for the fallback-forcing hook below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installed {
    /// `NSGlassEffectView`, style `.regular` — macOS 26+.
    Glass,
    /// Custom `NSVisualEffectView`, material `.popover`.
    Popover,
}

/// Installs the real material chain behind `window`. Call once, right
/// after the window opens — this window is resident for the process
/// lifetime (only ever hidden, never closed — see `AGENTS.md`, "Onboarding"
/// / the daemon/client split notes), so one install covers every future
/// summon; there is no per-summon cost.
///
/// On `Err`, nothing was installed. The caller is expected to fall back to
/// `WindowBackgroundAppearance::Opaque` — the plain opaque panel with
/// shadow and hairline border is still correct and shipped, just without a
/// material behind it.
#[cfg(target_os = "macos")]
pub fn install(window: &Window) -> Result<Installed, String> {
    macos::install(window)
}

#[cfg(not(target_os = "macos"))]
pub fn install(_window: &Window) -> Result<Installed, String> {
    Err("native window material is only implemented on macOS".to_string())
}

/// Test/evidence-only, for the `NEKO_BENCH` latency loop — see
/// `macos::order_front_regardless`'s own doc comment.
#[cfg(target_os = "macos")]
pub fn order_front_regardless(window: &Window) -> Result<(), String> {
    macos::order_front_regardless(window)
}

#[cfg(target_os = "macos")]
pub fn order_out(window: &Window) -> Result<(), String> {
    macos::order_out(window)
}

#[cfg(target_os = "macos")]
pub fn window_number(window: &Window) -> Result<isize, String> {
    macos::window_number(window)
}

/// Reads back, from the real `NSWindow`, whether this window is currently
/// the system's **key** window — i.e. whether real keystrokes typed on this
/// machine right now would land in it.
///
/// This is the load-bearing readback behind `AGENTS.md`'s "an evidence
/// window must never become the key window" rule: `evidence.rs` prints it
/// next to the window number on every capture, so a capture is
/// self-evidencing about focus rather than merely asserting it. Same
/// "verified, not trusted" pattern as `verify_installed`/
/// `verify_shadow_disabled` — a native readback, not a cached flag.
#[cfg(target_os = "macos")]
pub fn is_key_window(window: &Window) -> Result<bool, String> {
    macos::is_key_window(window)
}

#[cfg(not(target_os = "macos"))]
pub fn is_key_window(_window: &Window) -> Result<bool, String> {
    Err("key-window readback is only implemented on macOS".to_string())
}

/// A non-visual proof that `install`'s `Ok(Installed::_)` claim is real:
/// re-derives `window.contentView()` fresh, reads back the actual live
/// class, its material-specific properties, and its position in the
/// content view's subview list — the same "verified-not-assumed" pattern
/// the investigation report used for its own Sidebar/UnderWindowBackground
/// finding (§2), rather than trusting that a setter call took effect. Called
/// unconditionally by `main.rs` right after a successful `install`; logs
/// the readback to stderr and returns `Err` with exactly what didn't match
/// on any mismatch, rather than panicking — this runs on every real launch,
/// not just an evidence run.
#[cfg(target_os = "macos")]
pub fn verify_installed(window: &Window, installed: Installed) -> Result<String, String> {
    macos::verify_installed(window, installed)
}

#[cfg(not(target_os = "macos"))]
pub fn verify_installed(_window: &Window, _installed: Installed) -> Result<String, String> {
    Err("native window material is only implemented on macOS".to_string())
}

/// `NSWindowStyleMask` bit 0, `NSTitledWindowMask`.
///
/// The summon panel has no titlebar and never wanted one — `main.rs` passes
/// `titlebar: None` — but gpui's mac backend reads that as "no
/// `TitlebarOptions`" and falls into a branch that sets
/// `NSTitledWindowMask | NSFullSizeContentViewWindowMask` anyway
/// (`gpui_macos/src/window.rs`, `MacWindow::open`); `titlebar: Some(..)`
/// takes the *other* branch, which also sets `NSTitledWindowMask`. There is
/// no `WindowOptions` value that produces an untitled window, which is why
/// this is cleared natively after the fact rather than at creation.
///
/// AppKit draws its own ~1pt top-edge highlight on a **titled** window,
/// above everything the app itself paints — the captain's thrice-reported
/// "line across the top of the panel". See `AGENTS.md`, "The top line"
/// (and the two earlier, wrong attributions it corrects).
const TITLED_STYLE_MASK_BIT: u64 = 1 << 0;

/// `NSWindowStyleMaskNonactivatingPanel` (bit 7) — set by gpui for a
/// `WindowKind::PopUp` window and load-bearing for both the summon path and
/// every `evidence.rs` hook (`AGENTS.md`, "an evidence window must never
/// become the key window"). `verify_titled_cleared` asserts it survives.
const NONACTIVATING_PANEL_STYLE_MASK_BIT: u64 = 1 << 7;

/// `NSWindowStyleMaskFullSizeContentView` (bit 15) — what lets the app's own
/// content occupy the window's entire frame. Also asserted to survive.
const FULL_SIZE_CONTENT_VIEW_STYLE_MASK_BIT: u64 = 1 << 15;

/// Pure bit arithmetic, factored out so "what should the mask become" is
/// unit-testable without a real `NSWindow` — the same split
/// `spaces::joins_all_spaces_and_fullscreen_auxiliary` already uses.
/// Clears **only** bit 0; every other bit is preserved exactly.
pub(crate) fn style_mask_without_titled(bits: u64) -> u64 {
    bits & !TITLED_STYLE_MASK_BIT
}

/// Whether `bits` is a mask this app is happy with: `Titled` clear, and
/// both the non-activating-panel and full-size-content-view bits still set.
pub(crate) fn style_mask_is_untitled_panel(bits: u64) -> bool {
    bits & TITLED_STYLE_MASK_BIT == 0
        && bits & NONACTIVATING_PANEL_STYLE_MASK_BIT != 0
        && bits & FULL_SIZE_CONTENT_VIEW_STYLE_MASK_BIT != 0
}

/// The live `NSWindow.styleMask`, as raw bits — the "before" half of the
/// readback `main.rs` logs on every launch.
#[cfg(target_os = "macos")]
pub fn style_mask_bits(window: &Window) -> Result<u64, String> {
    macos::style_mask_bits(window)
}

#[cfg(not(target_os = "macos"))]
pub fn style_mask_bits(_window: &Window) -> Result<u64, String> {
    Err("style-mask readback is only implemented on macOS".to_string())
}

/// Clears `NSTitledWindowMask` on the real `NSWindow`, leaving every other
/// bit alone — see `TITLED_STYLE_MASK_BIT` for why this is needed and why it
/// cannot be done through `WindowOptions`.
///
/// Called once, at window creation, *before* `install` — a `setStyleMask:`
/// call makes AppKit rebuild the window's frame view, so doing it first
/// means every native view this module installs afterwards is installed
/// into the final one, and `disable_native_shadow`'s own readback happens
/// after the mask is already settled.
///
/// Nothing else about the window is touched: rounding is drawn by GPUI and
/// by the material view's own `cornerRadius` (never by AppKit's titled
/// frame), the collection behavior that carries Spaces/full-screen
/// reachability is a separate property, and `canBecomeKeyWindow` is
/// overridden to `YES` by gpui's own window subclass regardless of style
/// mask. All three are read back live anyway rather than assumed —
/// `spaces::verify`, `verify_shadow_disabled`, and `verify_titled_cleared`.
#[cfg(target_os = "macos")]
pub fn clear_titled_style_mask(window: &Window) -> Result<(), String> {
    macos::clear_titled_style_mask(window)
}

#[cfg(not(target_os = "macos"))]
pub fn clear_titled_style_mask(_window: &Window) -> Result<(), String> {
    Err("style-mask clearing is only implemented on macOS".to_string())
}

/// A non-visual proof `clear_titled_style_mask`'s claim is real, and that
/// it cost nothing else — same "verified, not trusted" pattern
/// `verify_installed`/`verify_shadow_disabled`/`spaces::verify` establish.
/// Returns the live mask bits on success so the caller can log the "after"
/// half of the readback.
#[cfg(target_os = "macos")]
pub fn verify_titled_cleared(window: &Window) -> Result<u64, String> {
    macos::verify_titled_cleared(window)
}

#[cfg(not(target_os = "macos"))]
pub fn verify_titled_cleared(_window: &Window) -> Result<u64, String> {
    Err("style-mask verification is only implemented on macOS".to_string())
}

// `set_background_frame` (a direct `NSView` frame set repositioning the
// whole-window background material for a mode transition) was deleted here
// — see `AGENTS.md`, "One constant panel width": the panel `div` is now
// always exactly the window's own width, so `install`'s own full-
// `contentView`-bounds install already matches it in every mode, and
// nothing ever needs to reposition it again.

/// Disables AppKit's own automatic window drop shadow — the fix for the
/// "two nested rounded rectangles" defect (`fm/neko-double-panel`,
/// `docs/evidence/double-panel-shadow-fix-report.md`). This window is
/// non-opaque (`window_background()` is `Transparent`, and even the
/// opaque-fallback branch below fills only the panel `div`, not the whole
/// `NSWindow`), with its entire visible surface drawn by a single
/// Metal-layer-backed `NSView` (GPUI's own rendering view) — AppKit cannot
/// inspect that layer's alpha channel to shape a shadow around the actual
/// painted content, so its default automatic shadow instead follows the
/// **whole `NSWindow` frame rectangle**, unconditionally.
///
/// Before the fixed-width-window fix (`AGENTS.md`, "Mode view resize
/// seam"), the real window's own frame always matched whatever width the
/// visible panel was (it resized with every mode transition), so this was
/// never visible — the frame-shaped shadow and the panel's own edge were
/// the same edge by construction. Once the window became permanently
/// `theme::PANEL_WIDTH_WITH_DETAIL_PX` wide regardless of the narrower
/// root-list panel then drawn inside it (a two-width layout later replaced
/// by "One constant panel width" — `AGENTS.md`), this same automatic
/// shadow started extending past the real panel edge on both sides, at
/// rest — a second, correctly-rounded (AppKit applies the window's own
/// corner radius to this shadow too) but wrongly-sized "surface" around the
/// real one. Confirmed by a single-variable live test (screenshot
/// before/after this one call, nothing else changed) — see the evidence
/// report.
///
/// The panel `div`'s own `.shadow_lg()` (`panel.rs`) is unaffected — it's
/// GPUI's own explicit box-shadow, drawn as real pixels in the same Metal
/// frame as everything else, already correctly sized to whatever width the
/// panel actually is in every mode. It was always the only shadow this app
/// needed; AppKit's own was redundant even when it happened to be
/// correctly shaped, and actively wrong once the window and the panel's
/// own width could diverge.
#[cfg(target_os = "macos")]
pub fn disable_native_shadow(window: &Window) -> Result<(), String> {
    macos::disable_native_shadow(window)
}

/// Put the real `NSWindow` into the appearance the active theme belongs to.
///
/// **This is the one part of a theme that a token swap genuinely cannot
/// do.** Everything GPUI paints is under this app's own control, but the
/// surface *behind* the panel is not: `install`'s `NSGlassEffectView` (or the
/// `NSVisualEffectView(.popover)` fallback) samples the desktop and tints it
/// according to the window's `NSAppearance`. Leave a light theme in
/// `darkAqua` and the blur reads dark everywhere the cream panel is thin — a
/// muddy halo around the panel's own translucency, which
/// `data/neko-cozy-theme/report.md` called out as the specific reason a light
/// direction "needs real native work" rather than only new token values.
///
/// Setting it on the window (rather than on the material view) is deliberate:
/// appearance inherits down the view tree, so this covers whichever material
/// actually installed, plus the menu overlay, without either of them having
/// to be found again or re-installed.
pub fn set_window_appearance(
    window: &Window,
    appearance: crate::theme::Appearance,
) -> Result<(), String> {
    macos::set_window_appearance(window, appearance)
}

/// Reads the live `NSAppearance` name back off the real window — the same
/// "verified, not trusted" pattern as `verify_installed`/
/// `verify_shadow_disabled`/`spaces::verify`, and the only way to tell a
/// `setAppearance:` that took effect from one that silently did not. Logged by
/// `main.rs` at startup alongside the existing material/shadow/style-mask
/// readbacks.
pub fn window_appearance_name(window: &Window) -> Result<String, String> {
    macos::window_appearance_name(window)
}

#[cfg(not(target_os = "macos"))]
pub fn disable_native_shadow(_window: &Window) -> Result<(), String> {
    Err("native window material is only implemented on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn set_window_appearance(
    _window: &Window,
    _appearance: crate::theme::Appearance,
) -> Result<(), String> {
    Err("native window material is only implemented on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn window_appearance_name(_window: &Window) -> Result<String, String> {
    Err("native window material is only implemented on macOS".to_string())
}

/// A non-visual proof `disable_native_shadow`'s claim is real — same
/// "verified, not trusted" pattern `verify_installed`/`spaces::verify`
/// already establish in this file/`spaces.rs`. Called unconditionally by
/// `main.rs` right after `disable_native_shadow`, on every real launch.
#[cfg(target_os = "macos")]
pub fn verify_shadow_disabled(window: &Window) -> Result<(), String> {
    macos::verify_shadow_disabled(window)
}

#[cfg(not(target_os = "macos"))]
pub fn verify_shadow_disabled(_window: &Window) -> Result<(), String> {
    Err("native window material is only implemented on macOS".to_string())
}

/// The class name of the window's live first responder — the object AppKit
/// delivers `keyDown:` to. `"GPUIView"` means keystrokes reach GPUI; anything
/// else means they do not. Read-only, for `evidence.rs`'s typing proof.
#[cfg(target_os = "macos")]
pub fn first_responder_name(window: &Window) -> Result<String, String> {
    macos::first_responder_name(window)
}

#[cfg(not(target_os = "macos"))]
pub fn first_responder_name(_window: &Window) -> Result<String, String> {
    Err("native window material is only implemented on macOS".to_string())
}

/// The `⌘K` actions menu's own, smaller frost surface — a *second* native
/// material view, installed once (hidden, zero-sized) right after the
/// whole-window `install`/`verify_installed` succeed, then shown/repositioned
/// on each real menu open (`show_menu_overlay`) and hidden on each close
/// (`hide_menu_overlay`) rather than being created and torn down per open —
/// no native view allocation on the menu-open path — the same "install
/// once" shape `install`'s own whole-window backdrop already uses.
///
/// **Same sibling-below invariant as the whole-window material — see this
/// module's own top doc comment.** This view is a second child of
/// `window.contentView()`, inserted *above* the whole-window background view
/// but still *below* GPUI's own rendering view (`install_below_rendering_view`
/// in the macOS impl) — never above it, for the exact reason stated there:
/// a subview above GPUI's rendering view would silently eat every pixel GPUI
/// draws inside the menu itself (its border, its row text).
///
/// **The honest limitation this carries, disclosed here because it is easy
/// to assume the opposite from the name "frost surface":** because GPUI owns
/// exactly one rendering `NSView` for the whole window (there is no way to
/// insert a native layer *between* two portions of GPUI's own single paint
/// pass), this view — like the whole-window one — can only ever reveal
/// what's genuinely behind *the window*, not GPUI's own already-painted
/// content (the result rows) sitting in front of it in the very same scene.
/// Wherever the menu overlaps a row that GPUI painted with an opaque fill
/// (the selected-row highlight is the one real case in this app —
/// `theme::active().surface_selected`), the visible result is the menu's own
/// translucent fill blended with that opaque pixel *within GPUI's own draw
/// pass* — a translucent tint, not a blur of it. Wherever the menu overlaps
/// anything GPUI left translucent (true for nearly all of an unselected
/// row's own footprint — see `panel::render_row`), this view's real blur
/// genuinely shows through. Closing this gap for good would mean either a
/// forked `gpui` with a real in-scene backdrop-blur primitive (reopening the
/// closed GPL question — `AGENTS.md`, "The GPUI dependency decision") or a
/// custom Metal-level compositing pass inside neko itself — a compositor
/// feature, not a UI pattern, per `data/neko-comet-design/report.md` §1's
/// identical conclusion about comet's own `frost.rs`.
#[cfg(target_os = "macos")]
pub fn install_menu_overlay(window: &Window) -> Result<Installed, String> {
    macos::install_menu_overlay(window)
}

#[cfg(not(target_os = "macos"))]
pub fn install_menu_overlay(_window: &Window) -> Result<Installed, String> {
    Err("native window material is only implemented on macOS".to_string())
}

/// Non-visual proof `install_menu_overlay`'s claim is real — same
/// "verified, not trusted" pattern `verify_installed` already establishes
/// for the whole-window material. Called unconditionally by `main.rs` right
/// after a successful `install_menu_overlay`.
#[cfg(target_os = "macos")]
pub fn verify_menu_overlay_installed(window: &Window, installed: Installed) -> Result<String, String> {
    macos::verify_menu_overlay_installed(window, installed)
}

#[cfg(not(target_os = "macos"))]
pub fn verify_menu_overlay_installed(_window: &Window, _installed: Installed) -> Result<String, String> {
    Err("native window material is only implemented on macOS".to_string())
}

/// Shows and positions the menu overlay at `bounds` — GPUI's own paint-time
/// `Bounds<Pixels>` for the menu card, in GPUI's coordinate space (origin
/// top-left of the window's content area, y increasing downward). Converted
/// internally to AppKit's unflipped `contentView` coordinate space (origin
/// bottom-left, y increasing upward) — see the macOS impl's own doc comment
/// for why this conversion is necessary here: the menu overlay is a genuine
/// sub-rectangle of the window, unlike the whole-window backdrop `install`
/// sets up once (which always spans the full content height, where the
/// conversion would be a no-op).
///
/// Called from `panel.rs`'s own paint-time element (`MenuFrostSync`) every
/// frame the menu is actually painted — so the native view always tracks
/// GPUI's real, laid-out position exactly, including a future clamped/
/// anchored position (`AGENTS.md`), never a value this module or `panel.rs`
/// computed independently. Best-effort: an `Err` (no raw window handle, the
/// overlay not installed) is logged by the caller and otherwise ignored, the
/// same as every other native call in this module.
#[cfg(target_os = "macos")]
pub fn show_menu_overlay(window: &Window, x_px: f32, y_px: f32, width_px: f32, height_px: f32) -> Result<(), String> {
    macos::show_menu_overlay(window, x_px, y_px, width_px, height_px)
}

#[cfg(not(target_os = "macos"))]
pub fn show_menu_overlay(
    _window: &Window,
    _x_px: f32,
    _y_px: f32,
    _width_px: f32,
    _height_px: f32,
) -> Result<(), String> {
    Err("native window material is only implemented on macOS".to_string())
}

/// Hides the menu overlay — called whenever the actions menu closes (every
/// site that sets `Root::actions_menu` back to `None`), so a closed menu
/// never leaves a stale blurred patch on screen. Cheap and idempotent
/// (`NSView.setHidden`, not a view teardown), safe to call even if the menu
/// was never shown this session.
#[cfg(target_os = "macos")]
pub fn hide_menu_overlay(window: &Window) -> Result<(), String> {
    macos::hide_menu_overlay(window)
}

#[cfg(not(target_os = "macos"))]
pub fn hide_menu_overlay(_window: &Window) -> Result<(), String> {
    Err("native window material is only implemented on macOS".to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2::runtime::AnyClass;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSAutoresizingMaskOptions, NSGlassEffectView, NSGlassEffectViewStyle, NSView, NSWindow,
        NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
        NSVisualEffectView, NSWindowOrderingMode, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use objc2_quartz_core::kCACornerCurveContinuous;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::{
        Installed, Window, style_mask_is_untitled_panel, style_mask_without_titled,
    };

    /// Matches the design token (`theme::PANEL_RADIUS_PX`) — the
    /// background view's own rounding has to agree with GPUI's
    /// `.rounded()` on the panel `div`, or the material's square corners
    /// show past the edge of GPUI's rounded content at the four corners of
    /// the (rectangular) window frame. Free once a native background view
    /// exists at all (report §5) — no custom squircle needed.
    const CORNER_RADIUS_PT: f64 = crate::theme::PANEL_RADIUS_PX as f64;

    /// Forces a specific fallback branch for verification, without needing
    /// an older machine or a genuinely broken install: see
    /// `docs/evidence/` for the screenshots taken with each value. Never
    /// set in normal operation.
    const FORCE_ENV_VAR: &str = "NEKO_FORCE_MATERIAL";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ForcedFallback {
        /// Skip the `NSGlassEffectView` branch even though the class is
        /// available on this OS — exercises exactly the same code the real
        /// "class lookup failed" branch would run, since that lookup is
        /// the only thing gating it.
        Popover,
        /// Fail `install` outright — exercises the final opaque fallback
        /// without needing a material install to genuinely error.
        Opaque,
    }

    fn parse_forced_fallback(value: Option<&str>) -> Option<ForcedFallback> {
        match value? {
            "popover" => Some(ForcedFallback::Popover),
            "opaque" => Some(ForcedFallback::Opaque),
            _ => None,
        }
    }

    fn forced_fallback() -> Option<ForcedFallback> {
        parse_forced_fallback(std::env::var(FORCE_ENV_VAR).ok().as_deref())
    }

    pub fn install(window: &Window) -> Result<Installed, String> {
        if forced_fallback() == Some(ForcedFallback::Opaque) {
            return Err(format!(
                "forced via {FORCE_ENV_VAR}=opaque, for fallback-chain verification"
            ));
        }

        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "material install attempted off the main thread".to_string())?;
        let content_view = root_content_view(window)?;

        if forced_fallback() != Some(ForcedFallback::Popover) && glass_class_available() {
            install_glass(&content_view, mtm);
            Ok(Installed::Glass)
        } else {
            install_popover(&content_view, mtm);
            Ok(Installed::Popover)
        }
    }

    /// A runtime class lookup, not an OS version-string parse, so this
    /// degrades correctly on any future OS too (report §3/§8) — including
    /// ones this codebase was never updated for.
    fn glass_class_available() -> bool {
        AnyClass::get(c"NSGlassEffectView").is_some()
    }

    /// Walks from the `NSView` `raw-window-handle` hands out (GPUI's own
    /// rendering view) up to its owning `NSWindow` — the shared first step
    /// both `root_content_view` (for material install) and the bench-only
    /// ordering helpers below need.
    fn native_window(window: &Window) -> Result<Retained<NSWindow>, String> {
        // `Window` also has its own inherent `window_handle()` (returning
        // GPUI's `AnyWindowHandle`, an unrelated type) that would otherwise
        // shadow the trait method below — UFCS picks the right one.
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|e| format!("no raw window handle: {e}"))?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return Err("not a macOS AppKit window handle".to_string());
        };

        // SAFETY: `appkit.ns_view` is GPUI's own live rendering `NSView`
        // for as long as this `Window` exists — neko's summon window is
        // resident for the process lifetime (hidden, never closed), and
        // the retained pointer below is used only to walk up to its
        // owning `NSWindow` within this call, never stored past it.
        let rendering_view: Retained<NSView> =
            unsafe { Retained::retain(appkit.ns_view.as_ptr().cast()) }
                .ok_or_else(|| "raw-window-handle returned a null NSView".to_string())?;

        rendering_view
            .window()
            .ok_or_else(|| "GPUI's rendering view has no owning NSWindow yet".to_string())
    }

    /// GPUI's own rendering `NSView` — the same one `native_window` walks up
    /// from, returned directly. This is the view AppKit delivers `keyDown:`
    /// to, and therefore the one that has to be the window's first responder
    /// for anything typed to reach `TextField` at all.
    fn rendering_view(window: &Window) -> Result<Retained<NSView>, String> {
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|e| format!("no raw window handle: {e}"))?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return Err("not a macOS AppKit window handle".to_string());
        };
        // SAFETY: identical to `native_window`'s own — GPUI's rendering view
        // is live for as long as this resident `Window` exists.
        unsafe { Retained::retain(appkit.ns_view.as_ptr().cast()) }
            .ok_or_else(|| "raw-window-handle returned a null NSView".to_string())
    }

    fn root_content_view(window: &Window) -> Result<Retained<NSView>, String> {
        native_window(window)?
            .contentView()
            .ok_or_else(|| "NSWindow has no contentView".to_string())
    }

    /// Test/evidence-only: orders the real native window front or out
    /// directly, bypassing `Window::activate_window`/`cx.hide()` (which
    /// activate or deactivate the whole app). Used by `main.rs`'s
    /// `NEKO_BENCH` loop so a repeated timing run doesn't repeatedly steal
    /// focus from whatever else is on screen — the same reason the
    /// investigation's own benchmark avoided `cx.activate(true)` (report
    /// §4). Not part of the material chain itself; kept here only because
    /// it needs the same raw-window-handle plumbing as `native_window`.
    pub fn order_front_regardless(window: &Window) -> Result<(), String> {
        native_window(window)?.orderFrontRegardless();
        Ok(())
    }

    /// See `order_front_regardless`.
    pub fn order_out(window: &Window) -> Result<(), String> {
        native_window(window)?.orderOut(None);
        Ok(())
    }

    /// Test/evidence-only: the real `NSWindow`'s `windowNumber`, for
    /// driving `screencapture -l<windowid>` (window-scoped capture) from
    /// outside the process — see `evidence.rs`.
    pub fn window_number(window: &Window) -> Result<isize, String> {
        Ok(native_window(window)?.windowNumber())
    }

    /// See the public `is_key_window` wrapper's own doc comment. Reads
    /// `-[NSWindow isKeyWindow]` off the live window, plus
    /// `-[NSApplication isActive]` is deliberately *not* consulted: key
    /// status is what decides where a keystroke is delivered, and a
    /// window can be in an active app without being key.
    pub fn is_key_window(window: &Window) -> Result<bool, String> {
        Ok(native_window(window)?.isKeyWindow())
    }

    /// The corner radius the `⌘K` actions menu's own overlay material
    /// installs at — matches the menu card's own `.rounded(px(theme::
    /// ROW_RADIUS_PX))` (`panel.rs`), the same "the background view's own
    /// rounding has to agree with GPUI's `.rounded()`" rule
    /// `CORNER_RADIUS_PT` states for the whole-window case, just at the
    /// menu's own (smaller) radius rather than the panel's.
    const MENU_CORNER_RADIUS_PT: f64 = crate::theme::ROW_RADIUS_PX as f64;

    fn install_glass(content_view: &NSView, mtm: MainThreadMarker) {
        let glass = make_glass_view(mtm, content_view.bounds(), CORNER_RADIUS_PT);
        glass.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        content_view.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Below, None);
    }

    fn install_popover(content_view: &NSView, mtm: MainThreadMarker) {
        let effect_view = make_popover_view(mtm, content_view.bounds(), CORNER_RADIUS_PT);
        effect_view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        content_view.addSubview_positioned_relativeTo(&effect_view, NSWindowOrderingMode::Below, None);
    }

    fn make_glass_view(mtm: MainThreadMarker, frame: NSRect, corner_radius: f64) -> Retained<NSGlassEffectView> {
        let glass = NSGlassEffectView::new(mtm);
        glass.setFrame(frame);
        glass.setStyle(NSGlassEffectViewStyle::Regular);
        // No separate CALayer step: unlike `NSVisualEffectView` below,
        // `NSGlassEffectView` takes a corner radius directly (report §3/§5).
        glass.setCornerRadius(corner_radius);
        glass
    }

    fn make_popover_view(mtm: MainThreadMarker, frame: NSRect, corner_radius: f64) -> Retained<NSVisualEffectView> {
        let effect_view = NSVisualEffectView::new(mtm);
        effect_view.setFrame(frame);
        // `.popover`, not the better-measuring `.sidebar`/
        // `.underWindowBackground`: those two render pixel-identically to
        // each other under `BehindWindow` blending on this OS for reasons
        // the investigation couldn't explain, so it declined to bet a
        // shipped fallback on either — report §2/§8.
        effect_view.setMaterial(NSVisualEffectMaterial::Popover);
        effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect_view.setState(NSVisualEffectState::Active);
        apply_continuous_corner_mask(&effect_view, corner_radius);
        effect_view
    }

    /// See `super::install_menu_overlay`'s own doc comment for the
    /// invariant and the honest limitation this carries. Installed hidden,
    /// zero-sized, positioned `Above` the whole-window background view
    /// (`subviews()[0]`, already established as that view's own index by
    /// `install`/`verify_installed`) but still below GPUI's rendering view —
    /// `contentView.subviews()` is therefore always `[whole-window
    /// background, menu overlay, GPUI's rendering view, ...]` after this
    /// call, and `show_menu_overlay`/`hide_menu_overlay` below rely on the
    /// menu overlay always being at index 1.
    pub fn install_menu_overlay(window: &Window) -> Result<Installed, String> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "menu overlay install attempted off the main thread".to_string())?;
        let content_view = root_content_view(window)?;
        let subviews = content_view.subviews().to_vec();
        let background = subviews
            .first()
            .ok_or_else(|| "contentView has no subviews — whole-window material not installed yet".to_string())?;

        let zero_frame = NSRect { origin: NSPoint { x: 0.0, y: 0.0 }, size: NSSize { width: 0.0, height: 0.0 } };
        if forced_fallback() != Some(ForcedFallback::Popover) && glass_class_available() {
            let glass = make_glass_view(mtm, zero_frame, MENU_CORNER_RADIUS_PT);
            glass.setHidden(true);
            content_view.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Above, Some(background));
            Ok(Installed::Glass)
        } else {
            let effect_view = make_popover_view(mtm, zero_frame, MENU_CORNER_RADIUS_PT);
            effect_view.setHidden(true);
            content_view.addSubview_positioned_relativeTo(&effect_view, NSWindowOrderingMode::Above, Some(background));
            Ok(Installed::Popover)
        }
    }

    /// See `super::verify_menu_overlay_installed`'s doc comment.
    pub fn verify_menu_overlay_installed(window: &Window, installed: Installed) -> Result<String, String> {
        let content_view = root_content_view(window)?;
        let subviews = content_view.subviews().to_vec();
        if subviews.len() < 3 {
            return Err(format!(
                "expected at least 3 subviews of contentView (whole-window background, \
                 menu overlay, GPUI's own rendering view), found {}",
                subviews.len()
            ));
        }
        let overlay = subviews[1].clone();
        let hidden = overlay.isHidden();
        if !hidden {
            return Err("expected the freshly-installed menu overlay to start hidden, readback visible".to_string());
        }
        match installed {
            Installed::Glass => {
                let glass = overlay.downcast::<NSGlassEffectView>().map_err(|v| {
                    format!(
                        "expected NSGlassEffectView at contentView subview index 1, found {}",
                        v.class().name().to_string_lossy()
                    )
                })?;
                let radius = glass.cornerRadius();
                if (radius - MENU_CORNER_RADIUS_PT).abs() > 0.01 {
                    return Err(format!("expected menu overlay cornerRadius {MENU_CORNER_RADIUS_PT}, readback {radius}"));
                }
                Ok(format!("NSGlassEffectView menu overlay at contentView.subviews()[1], hidden, cornerRadius={radius}"))
            }
            Installed::Popover => {
                let effect = overlay.downcast::<NSVisualEffectView>().map_err(|v| {
                    format!(
                        "expected NSVisualEffectView at contentView subview index 1, found {}",
                        v.class().name().to_string_lossy()
                    )
                })?;
                let Some(layer) = effect.layer() else {
                    return Err("expected the menu overlay to have a backing CALayer, found none".to_string());
                };
                let radius = layer.cornerRadius();
                if (radius - MENU_CORNER_RADIUS_PT).abs() > 0.01 {
                    return Err(format!("expected menu overlay layer.cornerRadius {MENU_CORNER_RADIUS_PT}, readback {radius}"));
                }
                Ok(format!("NSVisualEffectView menu overlay at contentView.subviews()[1], hidden, layer.cornerRadius={radius}"))
            }
        }
    }

    /// See `super::show_menu_overlay`'s doc comment for the calling
    /// convention. `y_px` is GPUI's own top-left-origin, y-down coordinate —
    /// converted here to `contentView`'s unflipped (bottom-left-origin,
    /// y-up) AppKit coordinate space by reading `contentView`'s own live
    /// height and flipping around it: `appkit_y = content_height -
    /// (y_px + height_px)`. The whole-window backdrop `install` sets up once
    /// never needed this conversion, since it always spans the *entire*
    /// content height (`y_px` and `content_height - height_px` are always
    /// both `0` there); a sub-rectangle genuinely needs it.
    pub fn show_menu_overlay(window: &Window, x_px: f32, y_px: f32, width_px: f32, height_px: f32) -> Result<(), String> {
        let _mtm = MainThreadMarker::new()
            .ok_or_else(|| "menu overlay frame update attempted off the main thread".to_string())?;
        let content_view = root_content_view(window)?;
        let subviews = content_view.subviews().to_vec();
        let overlay = subviews
            .get(1)
            .ok_or_else(|| "contentView has fewer than 2 subviews — menu overlay not installed yet".to_string())?;
        let content_height = content_view.bounds().size.height as f32;
        let appkit_y = content_height - (y_px + height_px);
        overlay.setFrame(NSRect {
            origin: NSPoint { x: x_px as f64, y: appkit_y as f64 },
            size: NSSize { width: width_px as f64, height: height_px as f64 },
        });
        overlay.setHidden(false);
        Ok(())
    }

    /// See `super::hide_menu_overlay`'s doc comment.
    pub fn hide_menu_overlay(window: &Window) -> Result<(), String> {
        // Same gate `show_menu_overlay`/`install` use: a
        // headless `#[gpui::test]` window's own `HasWindowHandle::
        // window_handle` panics rather than erroring, so this has to fail
        // gracefully *before* reaching it (via `root_content_view` below),
        // not inside its own `Err` path.
        let _mtm = MainThreadMarker::new()
            .ok_or_else(|| "menu overlay hide attempted off the main thread".to_string())?;
        let content_view = root_content_view(window)?;
        let subviews = content_view.subviews().to_vec();
        let overlay = subviews
            .get(1)
            .ok_or_else(|| "contentView has fewer than 2 subviews — menu overlay not installed yet".to_string())?;
        overlay.setHidden(true);
        Ok(())
    }

    /// `NSVisualEffectView`, unlike `NSGlassEffectView`, has no
    /// `cornerRadius` property of its own — native rounding has to go
    /// through its backing `CALayer` instead (report §5). Necessary, not
    /// cosmetic: without it, this view's square corners show past the edge
    /// of GPUI's own `.rounded()` panel content.
    fn apply_continuous_corner_mask(view: &NSView, corner_radius: f64) {
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            return;
        };
        layer.setCornerRadius(corner_radius);
        layer.setMasksToBounds(true);
        // SAFETY: reading an `extern "C"` static is unsafe only because
        // every extern-static read is, in Rust; `kCACornerCurveContinuous`
        // is a QuartzCore symbol AppKit itself provides on every macOS
        // version neko targets (13+).
        let continuous = unsafe { kCACornerCurveContinuous };
        layer.setCornerCurve(continuous);
    }

    /// See `super::verify_installed`'s doc comment.
    pub fn verify_installed(window: &Window, installed: Installed) -> Result<String, String> {
        let content_view = root_content_view(window)?;
        let subviews = content_view.subviews().to_vec();
        if subviews.len() < 2 {
            return Err(format!(
                "expected at least 2 subviews of contentView (the background material \
                 view plus GPUI's own rendering view), found {}",
                subviews.len()
            ));
        }
        // `positioned: Below, relativeTo: nil` (both install functions)
        // means bottommost — index 0 — among contentView's own subviews.
        // GPUI's own rendering view is a sibling, further up this same
        // list, never the background view's superview.
        let background = subviews[0].clone();

        match installed {
            Installed::Glass => {
                let glass = background.downcast::<NSGlassEffectView>().map_err(|v| {
                    format!(
                        "expected NSGlassEffectView at contentView subview index 0, found {}",
                        v.class().name().to_string_lossy()
                    )
                })?;
                let style = glass.style();
                let radius = glass.cornerRadius();
                if style != NSGlassEffectViewStyle::Regular {
                    return Err(format!("expected style .regular, readback {style:?}"));
                }
                if (radius - CORNER_RADIUS_PT).abs() > 0.01 {
                    return Err(format!("expected cornerRadius {CORNER_RADIUS_PT}, readback {radius}"));
                }
                Ok(format!(
                    "NSGlassEffectView at contentView.subviews()[0] (below GPUI's rendering \
                     view, {} total subviews) — readback style={style:?} cornerRadius={radius}",
                    subviews.len()
                ))
            }
            Installed::Popover => {
                let effect = background.downcast::<NSVisualEffectView>().map_err(|v| {
                    format!(
                        "expected NSVisualEffectView at contentView subview index 0, found {}",
                        v.class().name().to_string_lossy()
                    )
                })?;
                let material = effect.material();
                let blending = effect.blendingMode();
                let state = effect.state();
                if material != NSVisualEffectMaterial::Popover {
                    return Err(format!("expected material .popover, readback {material:?}"));
                }
                if blending != NSVisualEffectBlendingMode::BehindWindow {
                    return Err(format!("expected blendingMode .behindWindow, readback {blending:?}"));
                }
                if state != NSVisualEffectState::Active {
                    return Err(format!("expected state .active, readback {state:?}"));
                }
                let Some(layer) = effect.layer() else {
                    return Err("expected a backing CALayer (setWantsLayer(true) was called), found none".to_string());
                };
                let layer_radius = layer.cornerRadius();
                let masks = layer.masksToBounds();
                if (layer_radius - CORNER_RADIUS_PT).abs() > 0.01 {
                    return Err(format!(
                        "expected layer.cornerRadius {CORNER_RADIUS_PT}, readback {layer_radius}"
                    ));
                }
                if !masks {
                    return Err("expected layer.masksToBounds true, readback false".to_string());
                }
                Ok(format!(
                    "NSVisualEffectView at contentView.subviews()[0] (below GPUI's rendering \
                     view, {} total subviews) — readback material={material:?} \
                     blendingMode={blending:?} state={state:?} layer.cornerRadius={layer_radius} \
                     layer.masksToBounds={masks}",
                    subviews.len()
                ))
            }
        }
    }

    /// See `super::disable_native_shadow`'s doc comment.
    pub fn disable_native_shadow(window: &Window) -> Result<(), String> {
        native_window(window)?.setHasShadow(false);
        Ok(())
    }

    /// See `super::set_window_appearance`'s doc comment.
    pub fn set_window_appearance(
        window: &Window,
        appearance: crate::theme::Appearance,
    ) -> Result<(), String> {
        let native = native_window(window)?;
        // SAFETY: both names are AppKit's own documented, always-present
        // appearance constants; `appearanceNamed:` returns `nil` only for a
        // name that names nothing, which is why the `None` arm below is an
        // error rather than an unwrap.
        let name = unsafe {
            match appearance {
                crate::theme::Appearance::Light => NSAppearanceNameAqua,
                crate::theme::Appearance::Dark => NSAppearanceNameDarkAqua,
            }
        };
        let Some(value) = NSAppearance::appearanceNamed(name) else {
            return Err(format!("AppKit has no appearance named {name:?}"));
        };
        native.setAppearance(Some(&value));
        Ok(())
    }

    /// See `super::window_appearance_name`'s doc comment.
    pub fn window_appearance_name(window: &Window) -> Result<String, String> {
        let native = native_window(window)?;
        match native.appearance() {
            Some(a) => Ok(a.name().to_string()),
            // Never set, or explicitly cleared — the window inherits the
            // application's own appearance. Reported as such rather than as
            // an error, since it is a real, valid state (it is what every
            // build before this one was in).
            None => Ok("<inherited>".to_string()),
        }
    }

    /// See `super::style_mask_bits`'s doc comment.
    pub fn style_mask_bits(window: &Window) -> Result<u64, String> {
        Ok(native_window(window)?.styleMask().bits() as u64)
    }

    /// See `super::clear_titled_style_mask`'s doc comment.
    pub fn clear_titled_style_mask(window: &Window) -> Result<(), String> {
        let native = native_window(window)?;
        let current = native.styleMask().bits() as u64;
        let wanted = style_mask_without_titled(current);
        if wanted == current {
            // Already untitled — nothing to do, and no `setStyleMask:` call
            // (which would rebuild the frame view) made for no reason.
            return Ok(());
        }
        native.setStyleMask(NSWindowStyleMask::from_bits_retain(
            wanted as usize as _,
        ));

        // `setStyleMask:` makes AppKit rebuild the window's frame view, and
        // that **resets the window's first responder** — measured live, not
        // inferred: GPUI's `GPUIView` before the call, the panel itself
        // (`NSKVONotifying_GPUIPanel`) after it. GPUI only ever calls
        // `makeFirstResponder:` once, at window creation
        // (`gpui_macos/src/window.rs`), so nothing restores it on its own,
        // and with the panel as first responder `keyDown:` never reaches
        // GPUI at all — the search field silently stops accepting input
        // while the window still looks and behaves correct in every other
        // way. That is exactly the regression `98522de` shipped. Re-make
        // GPUI's rendering view the first responder here, in the same call
        // that disturbed it, so the two can never drift apart.
        //
        // Note this is *not* a key-window problem: the fork's `GPUIPanel`
        // overrides `canBecomeKeyWindow` to return `YES` unconditionally,
        // regardless of style mask.
        let view = rendering_view(window)?;
        if !native.makeFirstResponder(Some(&view)) {
            return Err(
                "NSWindow refused to restore GPUI's rendering view as first responder                  after the style-mask change"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// See `super::verify_titled_cleared`'s doc comment.
    pub fn verify_titled_cleared(window: &Window) -> Result<u64, String> {
        // (see below — the responder check is the load-bearing half)
        let native = native_window(window)?;
        // The regression `98522de` shipped was invisible to a style-mask-only
        // check, so this readback covers the responder too — see
        // `clear_titled_style_mask`.
        let responder = native.firstResponder();
        let is_rendering_view = responder
            .as_deref()
            .zip(rendering_view(window).ok().as_deref())
            .is_some_and(|(a, b)| std::ptr::eq(a as *const _ as *const (), b as *const _ as *const ()));
        if !is_rendering_view {
            return Err(format!(
                "expected GPUI's rendering view to be the window's first responder after                  the style-mask change, found {:?} — keyboard input would not reach the                  search field",
                responder.map(|r| format!("{:?}", r.class()))
            ));
        }
        let bits = native.styleMask().bits() as u64;
        if !style_mask_is_untitled_panel(bits) {
            return Err(format!(
                "expected NSTitledWindowMask clear with NSNonactivatingPanelMask and \
                 NSFullSizeContentViewWindowMask still set, readback bits={bits:#x}"
            ));
        }
        Ok(bits)
    }

    /// See `super::verify_shadow_disabled`'s doc comment.
    pub fn verify_shadow_disabled(window: &Window) -> Result<(), String> {
        let has_shadow = native_window(window)?.hasShadow();
        if has_shadow {
            return Err("expected NSWindow.hasShadow false, readback true".to_string());
        }
        Ok(())
    }

    /// See `super::first_responder_name`'s doc comment.
    pub fn first_responder_name(window: &Window) -> Result<String, String> {
        Ok(native_window(window)?
            .firstResponder()
            .map_or_else(|| "<none>".to_string(), |r| r.class().name().to_string_lossy().into_owned()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn no_env_var_means_no_forced_fallback() {
            assert_eq!(parse_forced_fallback(None), None);
        }

        #[test]
        fn recognizes_both_forced_branches() {
            assert_eq!(parse_forced_fallback(Some("popover")), Some(ForcedFallback::Popover));
            assert_eq!(parse_forced_fallback(Some("opaque")), Some(ForcedFallback::Opaque));
        }

        #[test]
        fn an_unrecognized_value_is_not_a_forced_fallback() {
            // Fails open to the real detection logic rather than silently
            // misinterpreting a typo as a specific forced branch.
            assert_eq!(parse_forced_fallback(Some("glass")), None);
            assert_eq!(parse_forced_fallback(Some("")), None);
        }
    }
}

#[cfg(test)]
mod style_mask_tests {
    use super::*;

    /// The exact mask gpui produces for this window, read back live off the
    /// captain's own running client (`data/neko-truth-pass/report.md` §2.4):
    /// `NSTitledWindowMask | NSNonactivatingPanelMask |
    /// NSFullSizeContentViewWindowMask`.
    const OBSERVED_MASK: u64 = 0x8081;

    #[test]
    fn clearing_titled_leaves_every_other_bit_alone() {
        let cleared = style_mask_without_titled(OBSERVED_MASK);
        assert_eq!(cleared, 0x8080);
        assert_eq!(cleared & TITLED_STYLE_MASK_BIT, 0);
        assert_ne!(cleared & NONACTIVATING_PANEL_STYLE_MASK_BIT, 0);
        assert_ne!(cleared & FULL_SIZE_CONTENT_VIEW_STYLE_MASK_BIT, 0);
    }

    #[test]
    fn clearing_an_already_untitled_mask_is_a_no_op() {
        assert_eq!(style_mask_without_titled(0x8080), 0x8080);
    }

    #[test]
    fn the_observed_mask_is_rejected_before_the_fix_and_accepted_after() {
        assert!(!style_mask_is_untitled_panel(OBSERVED_MASK));
        assert!(style_mask_is_untitled_panel(style_mask_without_titled(OBSERVED_MASK)));
    }

    #[test]
    fn losing_the_non_activating_panel_bit_fails_verification() {
        // The bit every `evidence.rs` hook and the summon path depend on —
        // a `setStyleMask:` that dropped it would be a worse regression
        // than the line this change removes.
        assert!(!style_mask_is_untitled_panel(0x8000));
    }

    #[test]
    fn losing_the_full_size_content_view_bit_fails_verification() {
        assert!(!style_mask_is_untitled_panel(0x0080));
    }
}

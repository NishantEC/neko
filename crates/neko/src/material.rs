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

#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2::runtime::AnyClass;
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSGlassEffectView, NSGlassEffectViewStyle, NSView, NSWindow,
        NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
        NSVisualEffectView, NSWindowOrderingMode,
    };
    use objc2_quartz_core::kCACornerCurveContinuous;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::{Installed, Window};

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

    fn install_glass(content_view: &NSView, mtm: MainThreadMarker) {
        let glass = NSGlassEffectView::new(mtm);
        glass.setFrame(content_view.bounds());
        glass.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        glass.setStyle(NSGlassEffectViewStyle::Regular);
        // No separate CALayer step: unlike `NSVisualEffectView` below,
        // `NSGlassEffectView` takes a corner radius directly (report §3/§5).
        glass.setCornerRadius(CORNER_RADIUS_PT);
        content_view.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Below, None);
    }

    fn install_popover(content_view: &NSView, mtm: MainThreadMarker) {
        let effect_view = NSVisualEffectView::new(mtm);
        effect_view.setFrame(content_view.bounds());
        effect_view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        // `.popover`, not the better-measuring `.sidebar`/
        // `.underWindowBackground`: those two render pixel-identically to
        // each other under `BehindWindow` blending on this OS for reasons
        // the investigation couldn't explain, so it declined to bet a
        // shipped fallback on either — report §2/§8.
        effect_view.setMaterial(NSVisualEffectMaterial::Popover);
        effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect_view.setState(NSVisualEffectState::Active);
        apply_continuous_corner_mask(&effect_view);
        content_view.addSubview_positioned_relativeTo(&effect_view, NSWindowOrderingMode::Below, None);
    }

    /// `NSVisualEffectView`, unlike `NSGlassEffectView`, has no
    /// `cornerRadius` property of its own — native rounding has to go
    /// through its backing `CALayer` instead (report §5). Necessary, not
    /// cosmetic: without it, this view's square corners show past the edge
    /// of GPUI's own `.rounded()` panel content.
    fn apply_continuous_corner_mask(view: &NSView) {
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            return;
        };
        layer.setCornerRadius(CORNER_RADIUS_PT);
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

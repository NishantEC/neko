//! Repositions the resident summon window onto the display under the
//! cursor before every summon, instead of always the primary display —
//! see `AGENTS.md`, "Window behavior" / the neko-audit report Part 4 item
//! 9. The window opens once at process start and is only ever hidden and
//! shown after that (`AGENTS.md`'s "resident window" design), so
//! `main.rs`'s own `upper_third()` — which only runs at that one initial
//! open — never gets a chance to place it correctly for a summon that
//! happens while the captain is looking at a different display than
//! whichever one was primary at launch.
//!
//! **Why the display under the cursor, not "the display owning the active
//! window":** the active window's owning display isn't cheaply or
//! reliably available without Accessibility (`AXUIElement`, the same
//! permission machinery `accessibility.rs` already gates the hotkey on,
//! and a cross-process query at that) — `NSEvent.mouseLocation` is a
//! synchronous, always-available class method needing no special
//! permission and no run-loop pump (unlike `NSWorkspace
//! .frontmostApplication`'s documented staleness off a spinning run loop —
//! see `AGENTS.md`, "Clipboard history" — this call doesn't have that
//! problem even off one). It also matches what a person actually expects
//! from a "summon where I'm looking" launcher: the cursor is where their
//! attention already is.
//!
//! **Why a native `NSWindow` call, not a GPUI API:** GPUI 0.2.2 has no
//! public way to move an already-open window — `Window::resize` exists,
//! but there is no `set_bounds`/`set_origin` (the same limitation
//! `AGENTS.md`'s window-material section documents for title-bar drag).
//! `WindowOptions::display_id` only takes effect at `open_window` time.
//! So this reaches for the real `NSWindow` the same way `material.rs`
//! does — `raw-window-handle` walked up from GPUI's own rendering view —
//! and calls the real, public `setFrameTopLeftPoint:`, not a private API.
//! Moving the real window this way is safe: gpui's own mac backend wires
//! `windowDidMove:`/`windowDidChangeScreen:` into `Window::bounds_changed`
//! (`gpui-0.2.2/src/window.rs`), which refreshes `scale_factor` from the
//! live `NSWindow` on every such native move — the same live rescale the
//! icon-cache renderer already depends on for a single display (see
//! `AGENTS.md`, "Icons": "GPUI re-samples the same cached bitmap at the
//! window's live `scale_factor()` every frame already"), so a captain
//! moving from the built-in Retina display to the external 4K panel gets
//! correctly re-rasterized icons with no extra code here.

use gpui::{Pixels, Point, Size, Window, point};

/// The design report's §2 panel geometry: "positioned upper-third, not
/// vertically centered", horizontally centered. Shared by `main.rs`'s
/// initial-open placement (`upper_third`) and the native re-placement
/// below, so both always agree on the exact same offsets — panel geometry
/// itself is frozen by captain decision and must not drift between the
/// two call sites.
pub(crate) fn upper_third_offset(display_size: Size<Pixels>, panel_size: Size<Pixels>) -> Point<Pixels> {
    let x = (display_size.width - panel_size.width) / 2.0;
    let y = display_size.height / 3.0 - panel_size.height / 4.0;
    point(x, y)
}

/// A screen's frame in native AppKit global coordinates (points, y-up,
/// origin at the bottom-left of the primary display) — a plain, testable
/// stand-in for `NSScreen.frame` so the "which screen contains this point"
/// logic below doesn't need a real `NSScreen`/AppKit to unit-test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScreenFrame {
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
}

/// The index of the first screen in `screens` whose frame contains
/// `(point_x, point_y)`, or `None` if no screen does (rare — e.g. a
/// display was unplugged in the instant between reading the cursor
/// position and reading the screen list; the caller falls back to the
/// main screen).
pub(crate) fn pick_screen_for_point(screens: &[ScreenFrame], point_x: f64, point_y: f64) -> Option<usize> {
    screens.iter().position(|f| {
        point_x >= f.origin_x
            && point_x < f.origin_x + f.width
            && point_y >= f.origin_y
            && point_y < f.origin_y + f.height
    })
}

#[cfg(target_os = "macos")]
pub fn reposition_to_cursor_display(window: &Window, panel_size: Size<Pixels>) -> Result<String, String> {
    macos::reposition(window, panel_size)
}

#[cfg(not(target_os = "macos"))]
pub fn reposition_to_cursor_display(_window: &Window, _panel_size: Size<Pixels>) -> Result<String, String> {
    Err("multi-display repositioning is only implemented on macOS".to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEvent, NSScreen, NSView, NSWindow};
    use objc2_foundation::NSPoint;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::{Pixels, ScreenFrame, Size, Window, pick_screen_for_point, upper_third_offset};

    /// Walks from the `NSView` `raw-window-handle` hands out (GPUI's own
    /// rendering view) up to its owning `NSWindow` — the same walk
    /// `material.rs::native_window` does, duplicated rather than shared
    /// because that function is private to `material`'s own submodule and
    /// this module deliberately doesn't touch `material.rs` (its material
    /// *selection* is settled by captain decision — see `AGENTS.md`,
    /// "Design tokens" / "Window material").
    fn native_window(window: &Window) -> Result<Retained<NSWindow>, String> {
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|e| format!("no raw window handle: {e}"))?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return Err("not a macOS AppKit window handle".to_string());
        };
        // SAFETY: `appkit.ns_view` is GPUI's own live rendering `NSView`
        // for as long as this `Window` exists — the summon window is
        // resident for the process lifetime — and the retained pointer is
        // used only to walk up to its owning `NSWindow` within this call.
        let rendering_view: Retained<NSView> =
            unsafe { Retained::retain(appkit.ns_view.as_ptr().cast()) }
                .ok_or_else(|| "raw-window-handle returned a null NSView".to_string())?;
        rendering_view
            .window()
            .ok_or_else(|| "GPUI's rendering view has no owning NSWindow yet".to_string())
    }

    fn screen_frame(screen: &NSScreen) -> ScreenFrame {
        let frame = screen.frame();
        ScreenFrame {
            origin_x: frame.origin.x,
            origin_y: frame.origin.y,
            width: frame.size.width,
            height: frame.size.height,
        }
    }

    /// The real `NSScreen` under the cursor right now, falling back to the
    /// main screen if the cursor is (rarely — e.g. mid display-hotplug)
    /// outside every screen's frame, and to the first screen if even that
    /// fails.
    fn screen_under_cursor(mtm: MainThreadMarker) -> Option<Retained<NSScreen>> {
        let cursor = NSEvent::mouseLocation();
        let screens: Vec<Retained<NSScreen>> = NSScreen::screens(mtm).to_vec();
        let frames: Vec<ScreenFrame> = screens.iter().map(|s| screen_frame(s)).collect();
        if let Some(idx) = pick_screen_for_point(&frames, cursor.x, cursor.y) {
            return screens.get(idx).cloned();
        }
        NSScreen::mainScreen(mtm).or_else(|| screens.first().cloned())
    }

    pub fn reposition(window: &Window, panel_size: Size<Pixels>) -> Result<String, String> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "display reposition attempted off the main thread".to_string())?;
        let native = native_window(window)?;
        let screen = screen_under_cursor(mtm).ok_or_else(|| "no NSScreen is available".to_string())?;
        let frame = screen.frame();
        let offset = upper_third_offset(
            gpui::size(gpui::px(frame.size.width as f32), gpui::px(frame.size.height as f32)),
            panel_size,
        );
        let top_left = NSPoint {
            x: frame.origin.x + offset.x.to_f64(),
            y: frame.origin.y + frame.size.height - offset.y.to_f64(),
        };
        native.setFrameTopLeftPoint(top_left);
        Ok(format!(
            "screen frame origin=({}, {}) size=({}, {}), top-left set to ({}, {})",
            frame.origin.x, frame.origin.y, frame.size.width, frame.size.height, top_left.x, top_left.y
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};

    #[test]
    fn upper_third_offset_centers_horizontally_and_sits_at_a_third_down() {
        let display = size(px(3456.0), px(2234.0));
        let panel = size(px(680.0), px(448.0));
        let offset = upper_third_offset(display, panel);
        assert_eq!(offset.x, px((3456.0 - 680.0) / 2.0));
        assert_eq!(offset.y, px(2234.0 / 3.0 - 448.0 / 4.0));
    }

    #[test]
    fn upper_third_offset_matches_across_different_display_sizes() {
        // The formula must not silently assume the primary display's own
        // dimensions — a captain's external 4K-presenting-as-1920x1080
        // display must center against its own size, not the built-in's.
        let external = size(px(1920.0), px(1080.0));
        let panel = size(px(680.0), px(448.0));
        let offset = upper_third_offset(external, panel);
        assert_eq!(offset.x, px((1920.0 - 680.0) / 2.0));
        assert_eq!(offset.y, px(1080.0 / 3.0 - 448.0 / 4.0));
    }

    #[test]
    fn pick_screen_for_point_finds_the_screen_containing_the_cursor() {
        // Built-in on the left (origin 0,0), external to the right —
        // a common two-display arrangement.
        let screens = [
            ScreenFrame { origin_x: 0.0, origin_y: 0.0, width: 3456.0, height: 2234.0 },
            ScreenFrame { origin_x: 3456.0, origin_y: 0.0, width: 1920.0, height: 1080.0 },
        ];
        assert_eq!(pick_screen_for_point(&screens, 100.0, 100.0), Some(0));
        assert_eq!(pick_screen_for_point(&screens, 4000.0, 500.0), Some(1));
    }

    #[test]
    fn pick_screen_for_point_handles_a_display_positioned_with_negative_origin() {
        // A secondary display placed above/left of the primary in System
        // Settings' Displays arrangement gets negative-origin coordinates
        // in AppKit's shared global space — must still resolve correctly.
        let screens = [
            ScreenFrame { origin_x: 0.0, origin_y: 0.0, width: 3456.0, height: 2234.0 },
            ScreenFrame { origin_x: -1920.0, origin_y: 1000.0, width: 1920.0, height: 1080.0 },
        ];
        assert_eq!(pick_screen_for_point(&screens, -1000.0, 1500.0), Some(1));
    }

    #[test]
    fn pick_screen_for_point_returns_none_when_the_cursor_is_outside_every_screen() {
        let screens = [ScreenFrame { origin_x: 0.0, origin_y: 0.0, width: 3456.0, height: 2234.0 }];
        assert_eq!(pick_screen_for_point(&screens, -50.0, -50.0), None);
    }
}

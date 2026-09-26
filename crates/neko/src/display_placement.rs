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

use crate::snap::{self, Rect};

/// The design report's §2 panel geometry: "positioned upper-third, not
/// vertically centered", horizontally centered. Shared by `main.rs`'s
/// initial-open placement (`upper_third`) and the native re-placement
/// below, so both always agree on the exact same offsets — panel geometry
/// itself is frozen by captain decision and must not drift between the
/// two call sites.
pub(crate) fn upper_third_offset(
    display_size: Size<Pixels>,
    panel_size: Size<Pixels>,
) -> Point<Pixels> {
    let x = (display_size.width - panel_size.width) / 2.0;
    let y = display_size.height / 3.0 - panel_size.height / 4.0;
    point(x, y)
}

/// The index of the first screen in `screens` whose frame contains
/// `(point_x, point_y)`, or `None` if no screen does (rare — e.g. a
/// display was unplugged in the instant between reading the cursor
/// position and reading the screen list; the caller falls back to the
/// main screen).
///
/// Screens are [`snap::Rect`]s — native AppKit global coordinates (points,
/// y-up, origin at the bottom-left of the primary display). That type used
/// to be a local `ScreenFrame` struct here; it moved to `snap.rs` when the
/// drag needed the identical rectangle, rather than leaving two structurally
/// identical rectangles in one crate for a future reader to wonder about.
pub(crate) fn pick_screen_for_point(screens: &[Rect], point_x: f64, point_y: f64) -> Option<usize> {
    screens.iter().position(|f| f.contains(point_x, point_y))
}

#[cfg(target_os = "macos")]
pub fn reposition_to_cursor_display(
    window: &Window,
    panel_size: Size<Pixels>,
) -> Result<String, String> {
    macos::reposition(window, panel_size)
}

#[cfg(not(target_os = "macos"))]
pub fn reposition_to_cursor_display(
    _window: &Window,
    _panel_size: Size<Pixels>,
) -> Result<String, String> {
    Err("multi-display repositioning is only implemented on macOS".to_string())
}

/// Both frames of the display the cursor is on, in one read.
///
/// `full` and `visible` answer two different questions and the drag needs
/// both: a summon is placed against the **full** frame
/// ([`upper_third_offset`], and therefore [`home_origin`]), while a drag
/// snaps and clamps against the **visible** one, which excludes the menu bar
/// and the Dock. Read together, from one `NSScreen`, so the two can never
/// describe different displays because the cursor moved between two calls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorScreen {
    pub full: Rect,
    pub visible: Rect,
}

/// Where a summon puts a panel of `panel` size on a display whose full frame
/// is `full`, as a y-up bottom-left origin.
///
/// Goes through [`upper_third_offset`] rather than restating its arithmetic:
/// that function is already the single source of the summon position for both
/// `main.rs`'s initial open and [`reposition_to_cursor_display`], and a drag
/// that offered a "home" a few points away from where summon actually lands
/// would be worse than offering none. The only work here is the coordinate
/// flip — `upper_third_offset` returns a top-left, y-down offset, and
/// everything in the drag is bottom-left, y-up.
pub(crate) fn home_origin(full: Rect, panel: snap::Size) -> snap::Point {
    let offset = upper_third_offset(
        gpui::size(gpui::px(full.width as f32), gpui::px(full.height as f32)),
        gpui::size(gpui::px(panel.width as f32), gpui::px(panel.height as f32)),
    );
    snap::Point {
        x: full.x + offset.x.to_f64(),
        y: full.max_y() - offset.y.to_f64() - panel.height,
    }
}

/// The cursor's position in AppKit global coordinates — see `snap.rs`'s
/// module doc comment for the space, and this module's own for why
/// `NSEvent.mouseLocation` is the permission-free way to ask.
#[cfg(target_os = "macos")]
pub fn cursor_position() -> Result<snap::Point, String> {
    macos::cursor_position()
}

#[cfg(not(target_os = "macos"))]
pub fn cursor_position() -> Result<snap::Point, String> {
    Err("reading the cursor position is only implemented on macOS".to_string())
}

/// See [`CursorScreen`].
#[cfg(target_os = "macos")]
pub fn screen_under_cursor() -> Result<CursorScreen, String> {
    macos::cursor_screen()
}

#[cfg(not(target_os = "macos"))]
pub fn screen_under_cursor() -> Result<CursorScreen, String> {
    Err("reading the screen under the cursor is only implemented on macOS".to_string())
}

/// The real `NSWindow`'s live frame. Read rather than derived from
/// `theme::PANEL_WIDTH_WITH_DETAIL_PX`/`panel::PANEL_HEIGHT_PX` so a drag can
/// never be computed against a size the window does not actually have.
#[cfg(target_os = "macos")]
pub fn window_frame(window: &Window) -> Result<Rect, String> {
    macos::window_frame(window)
}

#[cfg(not(target_os = "macos"))]
pub fn window_frame(_window: &Window) -> Result<Rect, String> {
    Err("reading a window frame is only implemented on macOS".to_string())
}

/// Moves the real `NSWindow`'s bottom-left corner to `origin`.
///
/// `setFrameOrigin:` rather than `setFrameTopLeftPoint:` (which
/// [`reposition_to_cursor_display`] uses) purely because the drag already
/// works in bottom-left origins throughout; both are the same public AppKit
/// API and neither cares that the window was created `is_movable: false` —
/// that flag governs *user* dragging, and every placement this app has ever
/// done has been programmatic.
#[cfg(target_os = "macos")]
pub fn move_window_origin(window: &Window, origin: snap::Point) -> Result<(), String> {
    macos::move_window_origin(window, origin)
}

#[cfg(not(target_os = "macos"))]
pub fn move_window_origin(_window: &Window, _origin: snap::Point) -> Result<(), String> {
    Err("moving a window is only implemented on macOS".to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEvent, NSScreen, NSView, NSWindow};
    use objc2_foundation::NSPoint;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::{Pixels, Rect, Size, Window, pick_screen_for_point, snap, upper_third_offset};

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

    fn screen_frame(screen: &NSScreen) -> Rect {
        let frame = screen.frame();
        Rect {
            x: frame.origin.x,
            y: frame.origin.y,
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
        let frames: Vec<Rect> = screens.iter().map(|s| screen_frame(s)).collect();
        if let Some(idx) = pick_screen_for_point(&frames, cursor.x, cursor.y) {
            return screens.get(idx).cloned();
        }
        NSScreen::mainScreen(mtm).or_else(|| screens.first().cloned())
    }

    pub fn reposition(window: &Window, panel_size: Size<Pixels>) -> Result<String, String> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "display reposition attempted off the main thread".to_string())?;
        let native = native_window(window)?;
        let screen =
            screen_under_cursor(mtm).ok_or_else(|| "no NSScreen is available".to_string())?;
        let frame = screen.frame();
        let offset = upper_third_offset(
            gpui::size(
                gpui::px(frame.size.width as f32),
                gpui::px(frame.size.height as f32),
            ),
            panel_size,
        );
        let top_left = NSPoint {
            x: frame.origin.x + offset.x.to_f64(),
            y: frame.origin.y + frame.size.height - offset.y.to_f64(),
        };
        native.setFrameTopLeftPoint(top_left);
        Ok(format!(
            "screen frame origin=({}, {}) size=({}, {}), top-left set to ({}, {})",
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
            top_left.x,
            top_left.y
        ))
    }

    /// `NSEvent.mouseLocation` — a class method, no `MainThreadMarker` and no
    /// permission, the same call `screen_under_cursor` above already relies
    /// on. Read fresh on every drag tick rather than taken from GPUI's own
    /// `MouseMoveEvent`, which carries a *window-relative* position: during a
    /// drag the window is being moved out from under the cursor, so a
    /// window-relative reading would be measuring against a moving datum. This
    /// one is absolute, which also makes a dropped tick cost nothing — the
    /// next one lands the panel in exactly the right place regardless of how
    /// many were missed.
    pub fn cursor_position() -> Result<snap::Point, String> {
        let p = NSEvent::mouseLocation();
        Ok(snap::Point { x: p.x, y: p.y })
    }

    pub fn cursor_screen() -> Result<super::CursorScreen, String> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "screen lookup attempted off the main thread".to_string())?;
        let screen =
            screen_under_cursor(mtm).ok_or_else(|| "no NSScreen is available".to_string())?;
        let visible = screen.visibleFrame();
        Ok(super::CursorScreen {
            full: screen_frame(&screen),
            visible: Rect {
                x: visible.origin.x,
                y: visible.origin.y,
                width: visible.size.width,
                height: visible.size.height,
            },
        })
    }

    pub fn window_frame(window: &Window) -> Result<Rect, String> {
        let frame = native_window(window)?.frame();
        Ok(Rect {
            x: frame.origin.x,
            y: frame.origin.y,
            width: frame.size.width,
            height: frame.size.height,
        })
    }

    pub fn move_window_origin(window: &Window, origin: snap::Point) -> Result<(), String> {
        native_window(window)?.setFrameOrigin(NSPoint {
            x: origin.x,
            y: origin.y,
        });
        Ok(())
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
            Rect {
                x: 0.0,
                y: 0.0,
                width: 3456.0,
                height: 2234.0,
            },
            Rect {
                x: 3456.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
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
            Rect {
                x: 0.0,
                y: 0.0,
                width: 3456.0,
                height: 2234.0,
            },
            Rect {
                x: -1920.0,
                y: 1000.0,
                width: 1920.0,
                height: 1080.0,
            },
        ];
        assert_eq!(pick_screen_for_point(&screens, -1000.0, 1500.0), Some(1));
    }

    #[test]
    fn pick_screen_for_point_returns_none_when_the_cursor_is_outside_every_screen() {
        let screens = [Rect {
            x: 0.0,
            y: 0.0,
            width: 3456.0,
            height: 2234.0,
        }];
        assert_eq!(pick_screen_for_point(&screens, -50.0, -50.0), None);
    }

    /// The drag's "home" snap target has to be the position a summon actually
    /// lands at, or the guide lies. Both are derived from
    /// `upper_third_offset`; this pins the coordinate flip between them —
    /// `reposition` writes a **top-left, y-up** point, `home_origin` returns a
    /// **bottom-left, y-up** one, and they must describe the same rectangle.
    #[test]
    fn home_origin_is_the_same_place_reposition_puts_a_summoned_panel() {
        let full = Rect {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        };
        let panel = snap::Size {
            width: 760.0,
            height: 420.0,
        };

        let home = home_origin(full, panel);

        // `reposition`'s own arithmetic, restated here as the reference:
        // `top_left = (frame.origin.x + offset.x, frame.origin.y +
        // frame.height − offset.y)`.
        let offset = upper_third_offset(
            gpui::size(px(full.width as f32), px(full.height as f32)),
            gpui::size(px(panel.width as f32), px(panel.height as f32)),
        );
        let top_left_x = full.x + offset.x.to_f64();
        let top_left_y = full.y + full.height - offset.y.to_f64();

        assert_eq!(home.x, top_left_x);
        assert_eq!(
            home.y + panel.height,
            top_left_y,
            "home is that same top edge, measured from the bottom"
        );
    }

    /// The one number a display's own menu bar/Dock insets must **not** change:
    /// home is computed from the full frame, so two displays of the same size
    /// with different Dock settings still call the same place home.
    #[test]
    fn home_origin_ignores_the_visible_frame_and_tracks_the_display_it_is_given() {
        let panel = snap::Size {
            width: 760.0,
            height: 420.0,
        };
        let primary = home_origin(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1440.0,
                height: 900.0,
            },
            panel,
        );
        let secondary = home_origin(
            Rect {
                x: 1440.0,
                y: 0.0,
                width: 1440.0,
                height: 900.0,
            },
            panel,
        );
        assert_eq!(secondary.x - primary.x, 1440.0);
        assert_eq!(secondary.y, primary.y);
    }
}

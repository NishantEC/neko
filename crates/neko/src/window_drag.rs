//! Dragging the panel by hand, with snap targets and the guides that show
//! them — everything in the feature that genuinely needs AppKit. The part
//! that decides *where the panel lands* is `snap.rs`, which has none of this
//! in it and is where the tests are.
//!
//! # Why this app owns the drag loop instead of AppKit
//!
//! The first version of drag handed the whole gesture to
//! `Window::start_window_move()` → `performWindowDragWithEvent:`, which was
//! correct and about four lines. It cannot survive snapping, and that is a
//! property of the API rather than a detail: `performWindowDragWithEvent:`
//! runs **AppKit's own modal event loop** for the duration of the gesture and
//! does not return until the mouse comes up. There is no moment inside it at
//! which this app could measure how close the panel is to a target, decide
//! one, or draw anything. Snapping requires owning the loop, so the loop is
//! owned here.
//!
//! # The assumption the whole approach rests on, and how it was checked
//!
//! Once the cursor can leave the panel (it does, the instant a snap holds the
//! window still while the hand keeps moving), the drag depends on **mouse
//! events continuing to arrive at the panel's window anyway**. They do, for
//! two independent reasons, both confirmed by reading the gpui revision this
//! crate actually compiles against rather than assumed:
//!
//! * **AppKit's implicit capture.** `mouseDragged:`/`mouseUp:` go to the view
//!   that received `mouseDown:`, wherever the cursor has since gone.
//!   `gpui_macos/src/events.rs` turns `NSLeftMouseDragged` into a
//!   `MouseMoveEvent` with `pressed_button: Some(Left)` and no bounds check of
//!   any kind, and `gpui/src/window.rs`'s `dispatch_mouse_event` runs every
//!   listener registered with `Window::on_mouse_event` for every event, with
//!   position only ever consulted by the element-level wrappers (a `div`'s own
//!   `on_mouse_move` is gated on `hitbox.is_hovered`, which is exactly why
//!   this uses the window-level API instead).
//! * **gpui's own synthetic drag.** `gpui_macos/src/window.rs`'s
//!   `synthetic_drag` re-emits the last drag event every 16ms for as long as
//!   the button is held. So ticks keep coming even when the mouse is
//!   physically still.
//!
//! **Not verified by performing a real drag** — synthesising mouse input is
//! forbidden in this repo (`AGENTS.md`), so no agent can press the button.
//! What removes most of the risk is that the position is never accumulated:
//! every tick reads `NSEvent.mouseLocation` afresh and computes an absolute
//! origin, so a tick that never arrives costs nothing at all, and the drag
//! cannot drift.
//!
//! # The guides live in their own window
//!
//! A guide is drawn at the edge or the centre of the *screen*, which is
//! outside the panel — and an element cannot paint outside its own window. So
//! the guides are a second, transparent, click-through
//! (`NSWindow.ignoresMouseEvents`) `WindowKind::PopUp` window, ordered
//! **below** the panel so the thing being positioned stays on top of the lines
//! describing where it will go. It is opened **lazily**, the first time a
//! guide actually has to be drawn, and closed on every path that ends a drag:
//! a plain click on the input row, or a drag that never approaches a target,
//! never creates a window at all.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, Window, div, prelude::*, px};

use crate::snap::{self, Guide, GuideAxis};
use crate::theme;

/// How the panel reaches the drag. Injected into [`crate::panel::Root`] for
/// the same reason [`crate::panel::PreferencesOpener`] and
/// [`crate::panel::AppearanceSetter`] are: every method here ends in a native
/// window call, and **GPUI's test platform `unimplemented!()`s — panics — on
/// `window_handle()` and `open_window`**, so an unconditional call from a
/// mouse handler would take every headless panel test down with it.
///
/// Deliberately four verbs rather than one "handle this event": the panel
/// knows *what happened* (a press, a move, a release, an escape) and this
/// knows what a drag is. Nothing about grab offsets, screens or guides
/// crosses that line.
pub trait PanelDrag {
    /// Records where the cursor grabbed the panel. `false` if a drag could
    /// not be started at all (no window handle, no screen — every native read
    /// here can fail), in which case the panel stays exactly as it was and
    /// nothing further is sent.
    fn start(&self, window: &Window, cx: &mut App) -> bool;
    /// One tick: follow the cursor, snap, move the window, update the guides.
    fn update(&self, window: &Window, cx: &mut App);
    /// Mouse released — keep the panel where it is, tear the overlay down.
    fn finish(&self, window: &Window, cx: &mut App);
    /// Escape, or anything else that abandons the gesture — put the panel back
    /// where it was picked up from, tear the overlay down.
    fn cancel(&self, window: &Window, cx: &mut App);
}

/// A drag that does nothing, for tests and for any platform that is not macOS
/// — the same `#[cfg]` shape `accessibility::FakeAccessibilityChecker` uses,
/// so a real macOS build carries none of it.
#[cfg(any(test, not(target_os = "macos")))]
pub fn disabled() -> Rc<dyn PanelDrag> {
    Rc::new(NoDrag)
}

#[cfg(any(test, not(target_os = "macos")))]
struct NoDrag;

#[cfg(any(test, not(target_os = "macos")))]
impl PanelDrag for NoDrag {
    fn start(&self, _window: &Window, _cx: &mut App) -> bool {
        false
    }
    fn update(&self, _window: &Window, _cx: &mut App) {}
    fn finish(&self, _window: &Window, _cx: &mut App) {}
    fn cancel(&self, _window: &Window, _cx: &mut App) {}
}

/// The real thing.
#[cfg(target_os = "macos")]
pub fn native() -> Rc<dyn PanelDrag> {
    Rc::new(NativeDrag::default())
}

#[cfg(not(target_os = "macos"))]
pub fn native() -> Rc<dyn PanelDrag> {
    disabled()
}

/// One live gesture. Two things are frozen at mouse-down and never
/// recomputed:
///
/// * `grab` — where inside the panel it was picked up, so the panel keeps
///   that point under the cursor rather than jumping its corner to it. Frozen
///   is what makes a snap recoverable: while a snap holds the window still,
///   the cursor drifts away from the grab point, and moving back reverses it
///   exactly.
/// * `start_origin` — for [`PanelDrag::cancel`].
struct Session {
    grab: snap::Point,
    start_origin: snap::Point,
    overlay: Option<Overlay>,
}

struct Overlay {
    handle: gpui::WindowHandle<GuideOverlay>,
    /// The visible frame this overlay was sized to. A drag that crosses onto
    /// another display needs a differently-sized window, and resizing a live
    /// gpui window is the one thing this codebase has learned not to do
    /// (`AGENTS.md`, "Mode view resize seam"), so the overlay is closed and
    /// reopened instead.
    screen: snap::Rect,
}

#[derive(Default)]
struct NativeDrag {
    session: RefCell<Option<Session>>,
}

impl PanelDrag for NativeDrag {
    fn start(&self, window: &Window, cx: &mut App) -> bool {
        let _ = cx;
        let cursor = match crate::display_placement::cursor_position() {
            Ok(cursor) => cursor,
            Err(e) => {
                eprintln!("neko: could not read the cursor to start a panel drag: {e}");
                return false;
            }
        };
        let frame = match crate::display_placement::window_frame(window) {
            Ok(frame) => frame,
            Err(e) => {
                eprintln!("neko: could not read the panel's own frame to start a drag: {e}");
                return false;
            }
        };
        *self.session.borrow_mut() = Some(Session {
            grab: snap::Point { x: cursor.x - frame.x, y: cursor.y - frame.y },
            start_origin: frame.origin(),
            overlay: None,
        });
        true
    }

    fn update(&self, window: &Window, cx: &mut App) {
        // Borrowed for the whole tick, and nothing inside re-enters this type
        // — the overlay is a different entity entirely.
        let mut held = self.session.borrow_mut();
        let Some(session) = held.as_mut() else {
            return;
        };

        let (cursor, screen, frame) = match (
            crate::display_placement::cursor_position(),
            crate::display_placement::screen_under_cursor(),
            crate::display_placement::window_frame(window),
        ) {
            (Ok(cursor), Ok(screen), Ok(frame)) => (cursor, screen, frame),
            _ => return,
        };

        let panel = frame.size();
        let geometry = snap::Geometry {
            visible: screen.visible,
            panel,
            home: crate::display_placement::home_origin(screen.full, panel),
        };
        let desired = snap::Point { x: cursor.x - session.grab.x, y: cursor.y - session.grab.y };
        let resolved = snap::resolve(&geometry, desired);

        if let Err(e) = crate::display_placement::move_window_origin(window, resolved.origin) {
            eprintln!("neko: could not move the panel during a drag: {e}");
            return;
        }
        sync_overlay(session, window, cx, screen.visible, &resolved.guides);
    }

    fn finish(&self, window: &Window, cx: &mut App) {
        let _ = window;
        if let Some(session) = self.session.borrow_mut().take() {
            close_overlay(session.overlay, cx);
        }
    }

    fn cancel(&self, window: &Window, cx: &mut App) {
        let Some(session) = self.session.borrow_mut().take() else {
            return;
        };
        if let Err(e) = crate::display_placement::move_window_origin(window, session.start_origin) {
            eprintln!("neko: could not put the panel back after cancelling a drag: {e}");
        }
        close_overlay(session.overlay, cx);
    }
}

/// Opens, moves, refills or leaves alone the guide window, whichever this tick
/// calls for. Every failure here is logged and swallowed: a drag with no
/// guides is a worse drag, but a drag that stops moving because a window could
/// not be opened is a broken one.
fn sync_overlay(
    session: &mut Session,
    panel_window: &Window,
    cx: &mut App,
    screen: snap::Rect,
    guides: &[Guide],
) {
    // A drag that never comes near a target never opens a window.
    if guides.is_empty() && session.overlay.is_none() {
        return;
    }

    // Crossed onto another display: the overlay is sized to a screen, so it
    // has to be replaced rather than resized.
    if let Some(overlay) = &session.overlay
        && overlay.screen != screen
    {
        close_overlay(session.overlay.take(), cx);
    }

    if session.overlay.is_none() {
        match open_overlay(panel_window, cx, screen, guides) {
            Ok(overlay) => session.overlay = Some(overlay),
            Err(e) => eprintln!("neko: could not open the drag-guide overlay: {e}"),
        }
        return;
    }

    if let Some(overlay) = &session.overlay {
        let local = local_guides(screen, guides);
        let _ = overlay.handle.update(cx, |view, _window, cx| view.set_guides(local, cx));
    }
}

fn close_overlay(overlay: Option<Overlay>, cx: &mut App) {
    if let Some(overlay) = overlay {
        let _ = overlay.handle.update(cx, |_view, window, _cx| window.remove_window());
    }
}

/// Opens the guide window over `screen`.
///
/// **Sized at creation, positioned natively straight afterwards.** gpui reads
/// `WindowOptions::window_bounds`' origin relative to the *main* screen
/// (`MacDisplay::bounds()` reports every display at origin zero — the same
/// gap `display_placement` exists for), so the honest way to land this window
/// on a specific display is the one the summon panel already uses: open it at
/// the right size, then set the real `NSWindow`'s origin. The size is fixed
/// for this window's whole life.
fn open_overlay(
    panel_window: &Window,
    cx: &mut App,
    screen: snap::Rect,
    guides: &[Guide],
) -> Result<Overlay, String> {
    let local = local_guides(screen, guides);
    let size = gpui::size(px(screen.width as f32), px(screen.height as f32));
    let handle = cx
        .open_window(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
                    origin: gpui::point(px(0.), px(0.)),
                    size,
                })),
                titlebar: None,
                // Same level as the panel (`NSPopUpWindowLevel`), so the
                // guides float above other applications' windows rather than
                // being buried by whatever happens to be on screen —
                // `WindowKind::Normal` would put them underneath. Ordered
                // below the panel itself immediately after opening.
                kind: gpui::WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                // Never key, never focused: this window exists to be looked
                // at. `focus: false` with `show: true` is gpui's own
                // `orderFront:` path, not `makeKeyAndOrderFront:`.
                focus: false,
                show: true,
                window_background: gpui::WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|_cx| GuideOverlay {
                    guides: local,
                    size: snap::Size { width: screen.width, height: screen.height },
                })
            },
        )
        .map_err(|e| format!("open_window failed: {e}"))?;

    let panel_number = crate::material::window_number(panel_window).ok();
    let click_through = handle
        .update(cx, |_view, window, _cx| {
            // Read straight back off the live window rather than trusted —
            // the same discipline as `material::verify_installed`, and for a
            // sharper reason than usual: this window sits at the panel's own
            // level, over every other application, and if it silently failed
            // to take `ignoresMouseEvents` it would eat the captain's clicks
            // with nothing visible on screen to explain why. So a failure
            // here closes the window rather than shipping that.
            let click_through = match crate::material::set_ignores_mouse_events(window, true)
                .and_then(|()| crate::material::ignores_mouse_events(window))
            {
                Ok(through) => through,
                Err(e) => {
                    eprintln!("neko: could not make the drag-guide overlay click-through: {e}");
                    false
                }
            };
            if !click_through {
                window.remove_window();
                return false;
            }
            // AppKit computes a window's automatic drop shadow from what the
            // window actually paints, so on a transparent window that is
            // *every guide line* — measured at up to 44/255 of black bleeding
            // outward from a line into otherwise-empty pixels. A measuring
            // line with a halo is not a measuring line, and this is the same
            // call and the same reasoning as the panel's own
            // (`AGENTS.md`, "The double-panel shadow defect"); the guide's
            // own `surface_panel` outline is the contrast it needs.
            if let Err(e) = crate::material::disable_native_shadow(window) {
                eprintln!("neko: could not disable the drag-guide overlay's window shadow: {e}");
            }
            if let Err(e) = crate::display_placement::move_window_origin(window, screen.origin()) {
                eprintln!("neko: could not place the drag-guide overlay on its display: {e}");
            }
            if let Some(number) = panel_number
                && let Err(e) = crate::material::order_below(window, number)
            {
                eprintln!("neko: could not order the drag-guide overlay under the panel: {e}");
            }
            true
        })
        .map_err(|e| format!("could not configure the drag-guide overlay: {e}"))?;
    if !click_through {
        return Err("the drag-guide overlay would not become click-through".to_string());
    }

    Ok(Overlay { handle, screen })
}

/// Turns global guide positions into coordinates inside the overlay window —
/// the only coordinate conversion in the whole feature (see `snap.rs`'s module
/// doc comment). Vertical guides keep their direction; horizontal ones flip,
/// because AppKit measures y up from the bottom and gpui measures it down from
/// the top.
fn local_guides(screen: snap::Rect, guides: &[Guide]) -> Vec<LocalGuide> {
    guides
        .iter()
        .map(|guide| LocalGuide {
            axis: guide.axis,
            active: guide.active,
            position: match guide.axis {
                GuideAxis::Vertical => (guide.position - screen.x) as f32,
                GuideAxis::Horizontal => (screen.max_y() - guide.position) as f32,
            },
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
struct LocalGuide {
    axis: GuideAxis,
    position: f32,
    active: bool,
}

/// The guide window's whole view: some lines on nothing.
struct GuideOverlay {
    guides: Vec<LocalGuide>,
    size: snap::Size,
}

impl GuideOverlay {
    fn set_guides(&mut self, guides: Vec<LocalGuide>, cx: &mut gpui::Context<Self>) {
        if self.guides == guides {
            return;
        }
        self.guides = guides;
        cx.notify();
    }
}

impl gpui::Render for GuideOverlay {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let thickness = theme::SNAP_GUIDE_THICKNESS_PX;
        let palette = theme::active();
        let width = self.size.width as f32;
        let height = self.size.height as f32;

        div().size_full().relative().children(self.guides.iter().map(|guide| {
            // A guide sitting exactly on a screen edge would otherwise be
            // drawn half outside the window and clipped to a hairline.
            let start = |along: f32, extent: f32| (along - thickness / 2.0).clamp(0.0, extent - thickness);
            let line = div()
                .absolute()
                .bg(if guide.active { palette.snap_guide } else { palette.snap_guide_muted })
                // The panel's own surface colour as a hairline outline, so a
                // light theme's guide still reads against a light wallpaper
                // and a dark theme's against a dark one. Reuses an existing
                // token rather than inventing a third guide colour.
                .border_1()
                .border_color(palette.surface_panel)
                .rounded_full();
            match guide.axis {
                GuideAxis::Vertical => line
                    .top_0()
                    .h(px(height))
                    .left(px(start(guide.position, width)))
                    .w(px(thickness)),
                GuideAxis::Horizontal => line
                    .left_0()
                    .w(px(width))
                    .top(px(start(guide.position, height)))
                    .h(px(thickness)),
            }
        }))
    }
}

/// What [`demo_overlay`] managed to do, for the evidence hook to print.
pub struct DemoOverlay {
    /// For `screencapture -l<windowid>` — the only capture form allowed on
    /// this machine (`AGENTS.md`, "Window material").
    pub window_number: isize,
    pub guides: usize,
    /// How long `open_window` plus the native configuration actually took. The
    /// number the lazy-open decision above rests on: this cost is paid once
    /// per drag, at the moment the first guide appears.
    pub open_took: std::time::Duration,
    /// Every input and output of the `snap::resolve` call this overlay is
    /// drawing, printed alongside the window number so a screenshot can be
    /// checked *against the numbers that produced it* rather than eyeballed.
    /// Without this a capture only proves that some lines were drawn
    /// somewhere.
    pub detail: String,
}

/// Verification-only (`evidence.rs`, `NEKO_SHOW_DRAG_GUIDES`): opens the guide
/// overlay for a snap the panel is *not* actually being dragged into, so the
/// guides can be photographed and the open cost measured without synthesising
/// a single mouse event.
///
/// It runs the real path — the same [`snap::resolve`], the same
/// [`open_overlay`] — with the cursor's own screen and the panel's own live
/// frame. The only fiction is the desired origin, which is chosen to put one
/// guide on the left edge and one on home so both axes are visible at once.
/// The window is deliberately left open and never closed: the evidence run
/// exits with it on screen, which is the point.
pub fn demo_overlay(panel_window: &Window, cx: &mut App) -> Result<DemoOverlay, String> {
    let screen = crate::display_placement::screen_under_cursor()?;
    let frame = crate::display_placement::window_frame(panel_window)?;
    let panel = frame.size();
    let geometry = snap::Geometry {
        visible: screen.visible,
        panel,
        home: crate::display_placement::home_origin(screen.full, panel),
    };
    let resolved = snap::resolve(
        &geometry,
        snap::Point { x: screen.visible.x + 4.0, y: geometry.home.y - 5.0 },
    );

    let started = std::time::Instant::now();
    let overlay = open_overlay(panel_window, cx, screen.visible, &resolved.guides)?;
    let open_took = started.elapsed();

    let window_number = overlay
        .handle
        .update(cx, |_view, window, _cx| crate::material::window_number(window))
        .map_err(|e| format!("could not read the overlay's window number: {e}"))??;

    let mut detail = format!(
        "visible={:?} panel={:?} home={:?} desired-origin snapped to {:?}",
        geometry.visible, panel, geometry.home, resolved.origin
    );
    for (guide, local) in resolved.guides.iter().zip(local_guides(screen.visible, &resolved.guides)) {
        detail.push_str(&format!(
            "\n  guide {:?} {:?} global={:.1} local={:.1} active={}",
            guide.target, guide.axis, guide.position, local.position, guide.active
        ));
    }

    Ok(DemoOverlay { window_number, guides: resolved.guides.len(), open_took, detail })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> snap::Rect {
        snap::Rect { x: 100.0, y: 50.0, width: 1440.0, height: 800.0 }
    }

    #[test]
    fn a_vertical_guide_keeps_its_direction_and_a_horizontal_one_is_flipped() {
        let screen = screen();
        let guides = [
            Guide {
                target: snap::Target::LeftEdge,
                axis: GuideAxis::Vertical,
                position: screen.x,
                active: true,
            },
            Guide {
                target: snap::Target::TopEdge,
                axis: GuideAxis::Horizontal,
                position: screen.max_y(),
                active: false,
            },
        ];
        let local = local_guides(screen, &guides);
        assert_eq!(local[0].position, 0.0, "the screen's left edge is the overlay's own left edge");
        assert_eq!(local[1].position, 0.0, "the screen's *top* edge is y=0 in gpui's own y-down space");
        assert!(local[0].active && !local[1].active);
    }

    #[test]
    fn a_guide_in_the_middle_of_the_screen_lands_in_the_middle_of_the_overlay() {
        let screen = screen();
        let guides = [Guide {
            target: snap::Target::VerticalCenter,
            axis: GuideAxis::Horizontal,
            position: screen.y + screen.height / 2.0,
            active: true,
        }];
        let local = local_guides(screen, &guides);
        assert_eq!(local[0].position, (screen.height / 2.0) as f32);
    }
}

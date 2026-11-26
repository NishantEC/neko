//! Where the window was, kept between runs (docs/knowledge/app-shell.md).
//!
//! A Mac app reopens its window where it was closed, at the size it was. Rune gives the parts and no autosave
//! (`WindowHandle::placement` to read, `WindowOptions::{position, physical_size}` to open at, the displays the
//! window factory is handed), so this module is the rest, as pure functions with no window:
//!
//! * [`Frame::from_placement`] turns what the window reports into a frame in points. A window that is full
//!   screen, zoomed or minimised gives none: its normal frame is the one saved before.
//! * [`restore`] puts a saved frame on the displays there are now: the display holding most of it, else the
//!   primary one; never larger than that display, never smaller than [`MIN`], and moved so all of it is on it.
//! * [`Keeper`] decides when to write: once the frame has been still for half a second, and at quit.
//!
//! Coordinates are points in the desktop's top-left space (CoreGraphics' `CGDisplayBounds`, which Rune's
//! `Display::native_bounds` gives on macOS). AppKit's window creation divides the physical position and size it
//! is given by the main display's scale, so [`restore`] multiplies by that one.

use serde::{Deserialize, Serialize};

/// The smallest window (points), which `AppSpec::min_size` sets too: 760 wide, where the sidebar has folded by itself.
pub const MIN: (f64, f64) = (760.0, 560.0);
/// The first launch's size, and the size of a window with nothing saved.
pub const DEFAULT: (f64, f64) = (1200.0, 800.0);
/// How long the frame must be still before it is written.
pub const STILL: f64 = 0.5;
/// The prefs key.
pub const KEY: &str = "window";

/// A window's normal frame, in points: its top-left and its content size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A display as [`restore`] needs it: its bounds in points, the physical pixels per point, and whether it has the menu bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub scale: f64,
    pub primary: bool,
}

/// Where and how big the window opens, in physical pixels (`WindowOptions::position`, `physical_size`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Open {
    pub position: (i32, i32),
    pub size: (u32, u32),
}

impl Frame {
    /// The frame to keep from what the window reports: None while it is full screen, zoomed or minimised, or has no size.
    pub fn from_placement(p: &rune::WindowPlacement) -> Option<Frame> {
        if p.full_screen || p.maximized || p.minimized || p.scale <= 0.0 || p.content_size.0 == 0 || p.content_size.1 == 0 {
            return None;
        }
        let s = p.scale;
        Some(Frame { x: f64::from(p.position.0) / s, y: f64::from(p.position.1) / s, w: f64::from(p.content_size.0) / s, h: f64::from(p.content_size.1) / s })
    }

    /// Whether a value read from the prefs file is usable at all (a hand-edited or broken file gives None from `restore`).
    fn sane(&self) -> bool {
        [self.x, self.y, self.w, self.h].iter().all(|v| v.is_finite()) && self.w > 0.0 && self.h > 0.0
    }
}

impl Screen {
    /// A Rune display: its bounds in points (`native_bounds`: CoreGraphics' points on macOS) and its scale.
    pub fn of(d: &rune::Display) -> Screen {
        let [x, y, w, h] = d.native_bounds();
        Screen { x, y, w, h, scale: if d.scale > 0.0 { d.scale } else { 1.0 }, primary: d.primary }
    }

    fn overlap(&self, f: &Frame) -> f64 {
        let w = (self.x + self.w).min(f.x + f.w) - self.x.max(f.x);
        let h = (self.y + self.h).min(f.y + f.h) - self.y.max(f.y);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    }
}

/// Puts `saved` wholly on one of `screens`, in points: the display with most of it, else the primary (else the first).
/// None when nothing is saved, nothing is usable, or there are no displays: the window then opens at its default, centred.
pub fn place(saved: Option<Frame>, screens: &[Screen]) -> Option<Frame> {
    let f = saved.filter(Frame::sane)?;
    let best = screens.iter().filter(|s| s.w > 0.0 && s.h > 0.0).max_by(|a, b| a.overlap(&f).total_cmp(&b.overlap(&f)))?;
    let on = if best.overlap(&f) > 0.0 { best } else { screens.iter().find(|s| s.primary && s.w > 0.0).unwrap_or(best) };
    // The size: no larger than the display, no smaller than the floor (a display smaller than the floor keeps the floor).
    let w = f.w.min(on.w).max(MIN.0);
    let h = f.h.min(on.h).max(MIN.1);
    // The place: moved in until the whole window is on the display (its top-left corner wins when it is larger).
    let x = f.x.min(on.x + on.w - w).max(on.x);
    let y = f.y.min(on.y + on.h - h).max(on.y);
    Some(Frame { x, y, w, h })
}

/// [`place`], in the physical pixels the window is created with: AppKit divides by the main display's scale.
pub fn restore(saved: Option<Frame>, screens: &[Screen]) -> Option<Open> {
    let f = place(saved, screens)?;
    let scale = screens.iter().find(|s| s.primary).or(screens.first()).map_or(1.0, |s| s.scale);
    Some(Open { position: ((f.x * scale).round() as i32, (f.y * scale).round() as i32), size: ((f.w * scale).round() as u32, (f.h * scale).round() as u32) })
}

/// When to write: the frame is written once it has not moved for [`STILL`], and at quit ([`Keeper::flush`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Keeper {
    /// What was last written (or read at launch).
    pub written: Option<Frame>,
    /// The frame the last look saw, and when it first saw it.
    pub seen: Option<(Frame, f64)>,
}

impl Keeper {
    pub fn new(written: Option<Frame>) -> Keeper {
        Keeper { written, seen: None }
    }

    /// One look at time `t` (seconds) with the frame the window reports now (None: full screen, zoomed, minimised or closed:
    /// nothing changes). Returns the frame to write now, if any, and when to look again for a frame still waiting.
    pub fn look(&mut self, t: f64, now: Option<Frame>) -> (Option<Frame>, Option<f64>) {
        let Some(now) = now else { return (None, None) };
        if self.written == Some(now) {
            self.seen = None;
            return (None, None);
        }
        match self.seen {
            Some((f, since)) if f == now => {
                if t - since >= STILL {
                    self.written = Some(now);
                    self.seen = None;
                    (Some(now), None)
                } else {
                    (None, Some(since + STILL + 0.05))
                }
            }
            _ => {
                self.seen = Some((now, t));
                (None, Some(t + STILL + 0.05))
            }
        }
    }

    /// At quit or close: the frame to write if it differs from what was written, waiting or not.
    pub fn flush(&mut self, now: Option<Frame>) -> Option<Frame> {
        let now = now.or(self.seen.map(|(f, _)| f))?;
        self.seen = None;
        if self.written == Some(now) {
            return None;
        }
        self.written = Some(now);
        Some(now)
    }
}

/// The frame to keep from what the window reports, `full_screen` being Rune's own flag (`Cx::is_full_screen`), which turns on as the
/// Space transition starts: while the window grows into full screen it is not full screen yet to AppKit, and that frame is not the
/// window's normal one either.
pub fn normal_frame(placement: Option<&rune::WindowPlacement>, full_screen: bool) -> Option<Frame> {
    if full_screen {
        return None;
    }
    placement.and_then(Frame::from_placement)
}

thread_local! {
    /// The main window, once made (`App::on_window`): its frame is read from it on the UI thread.
    static WINDOW: std::cell::RefCell<Option<rune::WindowHandle>> = const { std::cell::RefCell::new(None) };
}

/// Keeps the main window's handle (each time Rune makes the window).
pub fn remember(handle: &rune::WindowHandle) {
    WINDOW.with(|w| *w.borrow_mut() = Some(handle.clone()));
}

/// The main window's normal frame now: None before it is made, after it closed, or while it is full screen (or going there: `full_screen`),
/// zoomed or minimised.
pub fn current(full_screen: bool) -> Option<Frame> {
    normal_frame(WINDOW.with(|w| w.borrow().as_ref().and_then(rune::WindowHandle::placement)).as_ref(), full_screen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(x: f64, y: f64, w: f64, h: f64, primary: bool) -> Screen {
        Screen { x, y, w, h, scale: 2.0, primary }
    }
    fn laptop() -> Screen {
        screen(0.0, 0.0, 1512.0, 982.0, true)
    }
    fn frame(x: f64, y: f64, w: f64, h: f64) -> Frame {
        Frame { x, y, w, h }
    }

    #[test]
    fn nothing_saved_opens_at_the_default() {
        assert_eq!(place(None, &[laptop()]), None);
        assert_eq!(restore(None, &[laptop()]), None);
        assert_eq!(place(Some(frame(10.0, 10.0, 1200.0, 800.0)), &[]), None, "no displays: the default, centred");
        assert_eq!(place(Some(frame(f64::NAN, 0.0, 1200.0, 800.0)), &[laptop()]), None, "a broken value is ignored");
    }

    #[test]
    fn a_frame_that_fits_comes_back_exactly() {
        let f = frame(120.0, 80.0, 1200.0, 760.0);
        assert_eq!(place(Some(f), &[laptop()]), Some(f));
        // AppKit divides by the main display's scale (2): the physical values are twice the points.
        assert_eq!(restore(Some(f), &[laptop()]), Some(Open { position: (240, 160), size: (2400, 1520) }));
    }

    #[test]
    fn a_frame_on_an_unplugged_monitor_comes_back_whole_on_the_primary() {
        // Saved on a monitor to the right of the laptop; only the laptop is left.
        let f = frame(1800.0, 100.0, 1400.0, 900.0);
        let p = place(Some(f), &[laptop()]).unwrap();
        assert_eq!((p.w, p.h), (1400.0, 900.0), "it still fits, so it keeps its size");
        assert!(p.x >= 0.0 && p.x + p.w <= 1512.0 && p.y >= 0.0 && p.y + p.h <= 982.0, "wholly on the laptop: {p:?}");
        // With the monitor still there it opens on it, where it was.
        let monitor = screen(1512.0, -200.0, 2560.0, 1440.0, false);
        assert_eq!(place(Some(f), &[laptop(), monitor]), Some(f));
    }

    #[test]
    fn a_frame_partly_off_screen_is_moved_on() {
        let p = place(Some(frame(-300.0, 700.0, 1200.0, 760.0)), &[laptop()]).unwrap();
        assert_eq!((p.x, p.y, p.w, p.h), (0.0, 982.0 - 760.0, 1200.0, 760.0));
        let p = place(Some(frame(900.0, -50.0, 1200.0, 760.0)), &[laptop()]).unwrap();
        assert_eq!((p.x, p.y), (1512.0 - 1200.0, 0.0));
    }

    #[test]
    fn a_display_smaller_than_the_saved_size_shrinks_it_but_never_below_the_floor() {
        let small = screen(0.0, 0.0, 1280.0, 720.0, true);
        let p = place(Some(frame(0.0, 0.0, 2400.0, 1300.0)), &[small]).unwrap();
        assert_eq!((p.x, p.y, p.w, p.h), (0.0, 0.0, 1280.0, 720.0));
        let tiny = screen(0.0, 0.0, 700.0, 500.0, true);
        let p = place(Some(frame(50.0, 50.0, 1000.0, 600.0)), &[tiny]).unwrap();
        assert_eq!((p.w, p.h), MIN, "the floor holds");
        assert_eq!((p.x, p.y), (0.0, 0.0), "its top-left stays on the display");
        let p = place(Some(frame(10.0, 10.0, 300.0, 200.0)), &[laptop()]).unwrap();
        assert_eq!((p.w, p.h), MIN, "a frame smaller than the floor grows to it");
    }

    #[test]
    fn the_display_with_most_of_the_window_wins() {
        let left = screen(-1920.0, 0.0, 1920.0, 1080.0, false);
        let f = frame(-400.0, 100.0, 1200.0, 760.0); // 400 on the left display, 800 on the laptop
        let p = place(Some(f), &[laptop(), left]).unwrap();
        assert_eq!((p.x, p.y), (0.0, 100.0), "moved onto the laptop, which held most of it");
    }

    #[test]
    fn full_screen_zoomed_and_minimised_windows_save_nothing() {
        let normal = rune::WindowPlacement { position: (200, 100), content_size: (2400, 1600), scale: 2.0, ..Default::default() };
        assert_eq!(Frame::from_placement(&normal), Some(frame(100.0, 50.0, 1200.0, 800.0)));
        for p in [
            rune::WindowPlacement { full_screen: true, ..normal },
            rune::WindowPlacement { maximized: true, ..normal },
            rune::WindowPlacement { minimized: true, ..normal },
            rune::WindowPlacement { content_size: (0, 0), ..normal },
        ] {
            assert_eq!(Frame::from_placement(&p), None, "{p:?}");
        }
    }

    #[test]
    fn a_window_going_full_screen_saves_nothing_before_appkit_says_so() {
        let normal = rune::WindowPlacement { position: (200, 100), content_size: (2400, 1600), scale: 2.0, ..Default::default() };
        assert_eq!(normal_frame(Some(&normal), false), Some(frame(100.0, 50.0, 1200.0, 800.0)));
        assert_eq!(normal_frame(Some(&normal), true), None, "Rune says full screen as the transition starts: the growing frame is not kept");
        assert_eq!(normal_frame(Some(&rune::WindowPlacement { maximized: true, ..normal }), false), None, "a zoomed frame is not kept");
        assert_eq!(normal_frame(None, false), None, "no window");
        // So a window that was full screen (or zoomed) when the app quit comes back at the normal frame written before.
        let mut k = Keeper::new(Some(frame(100.0, 50.0, 1200.0, 800.0)));
        let big = rune::WindowPlacement { position: (0, 0), content_size: (3024, 1964), scale: 2.0, ..Default::default() };
        assert_eq!(k.look(0.0, normal_frame(Some(&big), true)), (None, None));
        assert_eq!(k.look(1.0, normal_frame(Some(&big), true)), (None, None));
        assert_eq!(k.flush(normal_frame(Some(&rune::WindowPlacement { full_screen: true, ..big }), true)), None, "quit in full screen writes nothing");
        assert_eq!(k.written, Some(frame(100.0, 50.0, 1200.0, 800.0)));
    }

    #[test]
    fn the_keeper_writes_once_the_frame_is_still_and_at_quit() {
        let a = frame(0.0, 0.0, 1200.0, 800.0);
        let b = frame(10.0, 0.0, 1200.0, 800.0);
        let mut k = Keeper::new(Some(a));
        assert_eq!(k.look(0.0, Some(a)), (None, None), "the frame read at launch is not written again");
        let (w, again) = k.look(1.0, Some(b));
        assert_eq!(w, None);
        assert!(again.is_some_and(|at| at >= 1.5), "it asks to look again after the wait: {again:?}");
        assert_eq!(k.look(1.3, Some(b)).0, None, "not still long enough");
        assert_eq!(k.look(1.6, Some(b)).0, Some(b), "still for half a second: written");
        assert_eq!(k.look(2.0, Some(b)), (None, None), "and only once");
        // A move restarts the wait.
        let (c, d) = (frame(20.0, 20.0, 1000.0, 700.0), frame(30.0, 20.0, 1000.0, 700.0));
        k.look(3.0, Some(c));
        k.look(3.3, Some(d));
        assert_eq!(k.look(3.6, Some(d)).0, None, "the wait restarted at 3.3");
        assert_eq!(k.look(3.85, Some(d)).0, Some(d));
        // Full screen (None) changes nothing; quit writes what is waiting.
        assert_eq!(k.look(3.9, None), (None, None));
        k.look(4.0, Some(c));
        assert_eq!(k.flush(None), Some(c), "quit writes the frame still waiting");
        assert_eq!(k.flush(Some(c)), None, "nothing new to write");
    }
}

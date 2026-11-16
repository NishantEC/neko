//! The widths of the three resizable regions and whether each shows (docs/decisions/0002):
//! the sessions column on the left, the context panel beside the session, and the Neko column on
//! the right. The session view in the middle takes what is left. State only; `view.rs` draws it.
//!
//! Defaults are sensible for a 1280-point window (300 + a middle of about 660 + 300 for Neko, the
//! context panel closed because a session with nothing to show has no panel). Every width is
//! clamped to a range, and dragging a pane's edge below half its minimum collapses it, as
//! AppKit's split views do (`PaneState::drag_to`). Widths are not saved in P0.

use rune_kit::shell::PaneState;

/// The Neko column when collapsed to an icon.
pub const NEKO_STRIP: f32 = 44.0;
/// Below this window width Neko starts as its icon (the person can open it).
pub const NEKO_AUTO_WIDTH: f32 = 1100.0;
/// The least the session view keeps; below this the window is too small for all the regions.
pub const MIDDLE_MIN: f32 = 320.0;
/// How wide the draggable edge between two regions is.
pub const GRIP: f32 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Sessions,
    Context,
    Neko,
}

/// What [`Layout::fit`] decides shows in a given window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// The context panel's width, 0 when it is hidden or closed.
    pub context: f32,
    pub neko_open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub sessions: PaneState,
    pub context: PaneState,
    pub neko: PaneState,
    /// Nobody has opened or closed Neko yet: a window under [`NEKO_AUTO_WIDTH`] shows its icon.
    pub neko_auto: bool,
}

/// What is kept between runs (Rune's `prefs` store): each region's width and whether it shows.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Saved {
    pub sessions: f32,
    pub context: f32,
    pub neko: f32,
    pub context_open: bool,
    pub neko_open: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            sessions: PaneState::new(300.0).range(240.0, 420.0),
            context: PaneState::new(420.0).range(320.0, 640.0).collapsed(),
            neko: PaneState::new(340.0).range(280.0, 480.0),
            neko_auto: true,
        }
    }
}

impl Layout {
    /// What to keep for the next run.
    pub fn saved(&self) -> Saved {
        Saved { sessions: self.sessions.width(), context: self.context.width(), neko: self.neko.width(), context_open: self.context.is_open(), neko_open: self.neko.is_open() }
    }

    /// The layout the last run left (widths clamp to their ranges).
    pub fn restored(saved: Saved) -> Layout {
        let mut l = Layout::default();
        l.sessions.drag_to(saved.sessions);
        l.context.drag_to(saved.context);
        l.neko.drag_to(saved.neko);
        l.context.set_open(saved.context_open);
        l.neko.set_open(saved.neko_open);
        l.neko_auto = false;
        l
    }

    /// Opens or closes Neko by choice: from then on the window's width no longer decides.
    pub fn set_neko_open(&mut self, open: bool) {
        self.neko.set_open(open);
        self.neko_auto = false;
    }

    pub fn pane(&self, region: Region) -> &PaneState {
        match region {
            Region::Sessions => &self.sessions,
            Region::Context => &self.context,
            Region::Neko => &self.neko,
        }
    }

    pub fn pane_mut(&mut self, region: Region) -> &mut PaneState {
        match region {
            Region::Sessions => &mut self.sessions,
            Region::Context => &mut self.context,
            Region::Neko => &mut self.neko,
        }
    }

    /// A drag of a region's edge, `travel` points from where it started at `start` wide. The
    /// sessions column's edge is on its right (dragging right widens it); the other two have
    /// theirs on the left (dragging left widens them).
    pub fn drag(&mut self, region: Region, start: f32, travel: f32) {
        let width = match region {
            Region::Sessions => start + travel,
            Region::Context | Region::Neko => start - travel,
        };
        self.pane_mut(region).drag_to(width);
        if region == Region::Neko {
            self.neko_auto = false;
        }
    }

    /// What shows in a window `window` wide so the session keeps [`MIDDLE_MIN`]: the context panel
    /// narrows toward its minimum first, then the Neko column collapses to its icon, then the
    /// context panel hides. Nothing is changed in the state: widening the window brings it all back.
    pub fn fit(&self, window: f32) -> Fit {
        let mut fit = Fit { context: self.context.shown_width(), neko_open: self.neko.is_open() && !(self.neko_auto && window < NEKO_AUTO_WIDTH) };
        let middle = |f: &Fit| window - self.sessions.width() - f.context - if f.neko_open { self.neko.width() } else { NEKO_STRIP } - 3.0 * GRIP;
        if middle(&fit) < MIDDLE_MIN && fit.context > 0.0 {
            fit.context = (fit.context - (MIDDLE_MIN - middle(&fit))).max(self.context.min_width());
        }
        if middle(&fit) < MIDDLE_MIN && fit.neko_open {
            fit.neko_open = false;
        }
        if middle(&fit) < MIDDLE_MIN {
            fit.context = 0.0;
        }
        fit
    }

    /// Points the three regions take of a window `window` wide, and what is left for the session.
    pub fn middle(&self, window: f32) -> f32 {
        let neko = if self.neko.is_open() { self.neko.width() } else { NEKO_STRIP };
        window - self.sessions.width() - self.context.shown_width() - neko - 3.0 * GRIP
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_leave_the_session_most_of_a_1280_window() {
        let l = Layout::default();
        assert!(l.sessions.is_open() && l.neko.is_open());
        assert!(!l.context.is_open(), "the context panel is closed until a session has something to show");
        assert_eq!((l.sessions.width(), l.context.width(), l.neko.width()), (300.0, 420.0, 340.0), "the decided defaults");
        let middle = l.middle(1280.0);
        assert!(middle > 600.0, "{middle}");
        assert!(middle > l.sessions.width() && middle > l.neko.width());
        // Even with the context panel open, the session keeps its minimum at 1440.
        let mut open = l;
        open.context.set_open(true);
        assert!(open.middle(1440.0) >= MIDDLE_MIN, "{}", open.middle(1440.0));
    }

    #[test]
    fn dragging_resizes_within_the_range() {
        let mut l = Layout::default();
        l.drag(Region::Sessions, 300.0, 60.0);
        assert_eq!(l.sessions.width(), 360.0);
        l.drag(Region::Sessions, 300.0, 900.0);
        assert_eq!(l.sessions.width(), 420.0, "capped at its maximum");
        l.drag(Region::Neko, 340.0, -80.0);
        assert_eq!(l.neko.width(), 420.0, "dragging a right-hand region's edge left widens it");
        l.drag(Region::Neko, 340.0, 60.0);
        assert_eq!(l.neko.width(), 280.0);
        l.drag(Region::Context, 420.0, -900.0);
        assert_eq!(l.context.width(), 640.0);
    }

    #[test]
    fn dragging_far_enough_collapses_a_region_and_dragging_back_opens_it() {
        let mut l = Layout::default();
        l.drag(Region::Neko, 340.0, 340.0 - 100.0);
        assert!(!l.neko.is_open(), "below half its minimum it collapses to the icon");
        l.drag(Region::Neko, 340.0, -40.0);
        assert!(l.neko.is_open());
        assert_eq!(l.neko.width(), 380.0);
    }

    #[test]
    fn a_small_window_gives_way_in_order_and_the_session_keeps_its_minimum() {
        let mut l = Layout::default();
        l.context.set_open(true);
        l.neko_auto = false;
        assert_eq!(l.fit(1600.0), Fit { context: 420.0, neko_open: true });
        assert_eq!(l.fit(1440.0), Fit { context: 420.0, neko_open: true }, "the session still has its minimum");
        // Tighter: the context panel narrows first, down to its minimum.
        let narrower = l.fit(1320.0);
        assert!(narrower.context < 420.0 && narrower.context >= 320.0 && narrower.neko_open, "{narrower:?}");
        // Tighter still: Neko collapses to its icon, then the context panel hides.
        assert_eq!(l.fit(1100.0), Fit { context: 320.0, neko_open: false });
        assert_eq!(l.fit(1010.0), Fit { context: 320.0, neko_open: false });
        assert_eq!(l.fit(960.0), Fit { context: 0.0, neko_open: false });
        for w in [840.0, 960.0, 1000.0, 1060.0, 1180.0, 1280.0, 1600.0] {
            let f = l.fit(w);
            let middle = w - l.sessions.width() - f.context - if f.neko_open { l.neko.width() } else { NEKO_STRIP } - 3.0 * GRIP;
            assert!(middle >= MIDDLE_MIN, "{w}: {middle}");
        }
        // The state is untouched.
        assert!(l.neko.is_open() && l.context.is_open());
    }

    #[test]
    fn neko_starts_as_its_icon_under_1100_until_the_person_opens_it() {
        let mut l = Layout::default();
        assert!(!l.fit(1099.0).neko_open && l.fit(1100.0).neko_open);
        l.set_neko_open(true);
        assert!(l.fit(1099.0).neko_open || l.middle(1099.0) < MIDDLE_MIN, "an open Neko stays open while the session has room");
        assert!(!l.neko_auto);
    }

    #[test]
    fn widths_and_what_shows_come_back_from_the_saved_state() {
        let mut l = Layout::default();
        l.drag(Region::Sessions, 300.0, 40.0);
        l.context.set_open(true);
        l.set_neko_open(false);
        let back = Layout::restored(l.saved());
        assert_eq!((back.sessions.width(), back.context.is_open(), back.neko.is_open()), (340.0, true, false));
        assert!(!back.neko_auto, "a saved layout is the person's choice");
        let json = serde_json::to_string(&l.saved()).unwrap();
        assert_eq!(serde_json::from_str::<Saved>(&json).unwrap(), l.saved());
    }

    #[test]
    fn the_context_panel_opens_at_its_own_width() {
        let mut l = Layout::default();
        l.context.set_open(true);
        assert_eq!(l.context.width(), 420.0);
        assert_eq!(l.context.shown_width(), 420.0);
        l.context.set_open(false);
        assert_eq!(l.context.shown_width(), 0.0);
    }
}

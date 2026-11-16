//! The widths of the three resizable regions and whether each shows (docs/decisions/0002):
//! the sessions column on the left, the context panel beside the session, and the Neko column on
//! the right. The session view in the middle takes what is left. State only; `view.rs` draws it.
//!
//! Defaults are sensible for a 1280-point window (260 + a middle of about 700 + 300 for Neko, the
//! context panel closed because a session with nothing to show has no panel). Every width is
//! clamped to a range, and dragging a pane's edge below half its minimum collapses it, as
//! AppKit's split views do (`PaneState::drag_to`). Widths are not saved in P0.

use rune_kit::shell::PaneState;

/// The Neko column when collapsed to an icon.
pub const NEKO_STRIP: f32 = 48.0;
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub sessions: PaneState,
    pub context: PaneState,
    pub neko: PaneState,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            sessions: PaneState::new(260.0).range(220.0, 420.0),
            context: PaneState::new(360.0).range(280.0, 760.0).collapsed(),
            neko: PaneState::new(300.0).range(240.0, 480.0),
        }
    }
}

impl Layout {
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
        let middle = l.middle(1280.0);
        assert!(middle > 650.0, "{middle}");
        assert!(middle > l.sessions.width() && middle > l.neko.width());
        // Even with the context panel open, the session keeps its minimum at 1280.
        let mut open = l;
        open.context.set_open(true);
        assert!(open.middle(1280.0) >= MIDDLE_MIN, "{}", open.middle(1280.0));
    }

    #[test]
    fn dragging_resizes_within_the_range() {
        let mut l = Layout::default();
        l.drag(Region::Sessions, 260.0, 60.0);
        assert_eq!(l.sessions.width(), 320.0);
        l.drag(Region::Sessions, 260.0, 900.0);
        assert_eq!(l.sessions.width(), 420.0, "capped at its maximum");
        l.drag(Region::Neko, 300.0, -80.0);
        assert_eq!(l.neko.width(), 380.0, "dragging a right-hand region's edge left widens it");
        l.drag(Region::Neko, 300.0, 60.0);
        assert_eq!(l.neko.width(), 240.0);
    }

    #[test]
    fn dragging_far_enough_collapses_a_region_and_dragging_back_opens_it() {
        let mut l = Layout::default();
        l.drag(Region::Neko, 300.0, 300.0 - 100.0);
        assert!(!l.neko.is_open(), "below half its minimum it collapses to the icon");
        l.drag(Region::Neko, 300.0, -40.0);
        assert!(l.neko.is_open());
        assert_eq!(l.neko.width(), 340.0);
    }

    #[test]
    fn the_context_panel_opens_at_its_own_width() {
        let mut l = Layout::default();
        l.context.set_open(true);
        assert_eq!(l.context.width(), 360.0);
        assert_eq!(l.context.shown_width(), 360.0);
        l.context.set_open(false);
        assert_eq!(l.context.shown_width(), 0.0);
    }
}

//! Where a dragged panel actually lands, and which guides say so.
//!
//! **Pure, and deliberately GPUI-free.** Nothing in this file knows about a
//! window, a screen, an element or a frame — it takes three rectangles of
//! `f64` and returns an origin plus a list of lines to draw. That is the whole
//! reason it exists as its own module: the correctness of a drag lives here
//! and is unit-testable, while `window_drag.rs` keeps only the parts that
//! genuinely need AppKit (where the cursor is, where the window is, moving
//! it, and painting the guides).
//!
//! # One coordinate space, AppKit's
//!
//! Every number in this module is in **AppKit's global screen space**:
//! points (not device pixels), **y-up**, origin at the bottom-left of the
//! primary display, so a second display placed above or to the left has
//! negative coordinates. A [`Point`] used as an origin is a rectangle's
//! **bottom-left** corner, exactly like `NSWindow.frame.origin` and
//! `NSScreen.visibleFrame.origin`.
//!
//! This is not the space the rest of this crate renders in (GPUI is y-down
//! from a window's top-left), and the mismatch is the single easiest thing to
//! get wrong here. It is resolved by never converting inside the drag at all:
//! `window_drag.rs` reads the cursor from `NSEvent.mouseLocation` and the
//! window from `NSWindow.frame`, both already in this space, does the whole
//! computation here, and writes back through `NSWindow.setFrameOrigin:`, also
//! in this space. The only conversion in the feature is the last one — turning
//! a guide's global position into a coordinate inside the overlay window that
//! draws it.
//!
//! # The targets, and why these
//!
//! Four things a person actually means when they let go of a launcher panel:
//!
//! * **The edges of the usable screen** (`visibleFrame`, so the menu bar and
//!   the Dock are already excluded — a panel snapped against "the edge" must
//!   not end up under the Dock). Parking a panel against an edge is the one
//!   placement that is stable across every window arrangement underneath it.
//! * **The centre**, horizontal and vertical, independently. Centring by hand
//!   is the placement that is most obviously *nearly* right and most annoying
//!   to actually hit.
//! * **Home** — the exact position every summon puts the panel at
//!   ([`Geometry::home`], computed by the caller from
//!   `display_placement::upper_third_offset`, the one function that decides
//!   where a summon lands). Without it, a drag is a one-way trip: the next
//!   summon resets the position anyway, but during *this* session there would
//!   be no way back to the placement the app itself chose.
//!
//! Home is passed in rather than computed here for two reasons: this module
//! must stay free of the panel's own geometry constants, and the summon
//! position is derived from the display's **full** frame while snapping works
//! against its **visible** frame. Recomputing it from the visible frame would
//! produce a "home" guide that lies by the height of the menu bar.
//!
//! Two of the seven targets usually coincide: home's x *is* the horizontal
//! centre of the display, which equals the centre of the visible frame on
//! every machine whose Dock is at the bottom (or hidden). [`resolve`] collapses
//! candidates that resolve to the same origin, so the usual case offers three
//! x targets, not four; a left- or right-side Dock genuinely separates them by
//! half the Dock's width and both are then offered.

/// How close the panel has to get before a target takes it, in points.
///
/// **16pt, and the number is a trade, not a taste.** Two constraints bracket
/// it:
///
/// * It has to be reachable by a hand that is not being careful. A deliberate
///   positioning gesture moves the cursor a few points between frames, so a
///   band narrower than about a dozen points is one a fast drag can step
///   straight over — the snap then feels unreliable, which is worse than not
///   having it.
/// * It has to leave the screen mostly free of pull. At 16pt, the three x
///   targets on a 1440pt-wide display occupy 96pt of 1440 — under 7% of the
///   width has any snapping behaviour at all, so a deliberate placement
///   anywhere else is never fought.
///
/// It is deliberately *not* wide enough to make two targets indistinguishable
/// in the common case, but it is not so narrow that overlap is impossible
/// either: on a 1280×800 display the home and vertical-centre origins sit
/// 28.3pt apart (`800/3 − 420/4 = 161.7` versus `(800 − 420)/2 = 190`), so a
/// 3.7pt band exists where both are within reach. That case is why guides
/// distinguish active from inactive at all rather than only ever drawing one.
pub const SNAP_THRESHOLD_PT: f64 = 16.0;

/// A point in the global screen space this module's doc comment defines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// A rectangle in the global screen space, `origin` being its bottom-left
/// corner — the shape `NSWindow.frame`, `NSScreen.frame` and
/// `NSScreen.visibleFrame` all already have.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }

    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }

    pub fn size(&self) -> Size {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    pub fn origin(&self) -> Point {
        Point {
            x: self.x,
            y: self.y,
        }
    }

    /// Whether `(x, y)` falls inside this rectangle, half-open at the far
    /// edges so two abutting displays never both claim the same cursor.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.max_x() && y >= self.y && y < self.max_y()
    }
}

/// Which way a guide *line* runs, not which coordinate it constrains — the
/// distinction that makes this readable at the paint site.
///
/// A [`GuideAxis::Vertical`] line is a vertical stroke down the screen; it
/// marks an **x** position. A [`GuideAxis::Horizontal`] line is a horizontal
/// stroke across the screen; it marks a **y** position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideAxis {
    Vertical,
    Horizontal,
}

/// One place the panel can be taken to. Each one owns two things: the origin
/// it would put the panel at, and which edge (or centre line) of the panel the
/// guide should be drawn along, so a guide always marks exactly what the panel
/// will line up with rather than an abstract screen coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    LeftEdge,
    HorizontalCenter,
    RightEdge,
    /// The x of the panel's summon position. Collapses into
    /// [`Target::HorizontalCenter`] unless the Dock is on a side — see the
    /// module doc comment.
    HomeCenter,
    BottomEdge,
    VerticalCenter,
    TopEdge,
    /// The y of the panel's summon position — the upper-third placement
    /// `display_placement::upper_third_offset` produces. Its guide runs along
    /// the panel's **top** edge, which is the edge that reads as "the panel is
    /// back where it starts".
    HomeTop,
}

impl Target {
    pub fn axis(self) -> GuideAxis {
        match self {
            Target::LeftEdge
            | Target::HorizontalCenter
            | Target::RightEdge
            | Target::HomeCenter => GuideAxis::Vertical,
            Target::BottomEdge | Target::VerticalCenter | Target::TopEdge | Target::HomeTop => {
                GuideAxis::Horizontal
            }
        }
    }

    /// Where the line for this target is drawn, given the panel origin it
    /// produces and the panel's extent along the same axis.
    fn line(self, origin: f64, extent: f64) -> f64 {
        match self {
            Target::LeftEdge | Target::BottomEdge => origin,
            Target::HorizontalCenter | Target::HomeCenter | Target::VerticalCenter => {
                origin + extent / 2.0
            }
            Target::RightEdge | Target::TopEdge | Target::HomeTop => origin + extent,
        }
    }
}

/// A line to draw while the drag is live. `position` is a global x for a
/// [`GuideAxis::Vertical`] guide and a global y for a horizontal one.
///
/// `active` is the whole point of returning more than one: every guide in a
/// [`Resolution`] is within reach, but exactly one per axis is the one the
/// panel has actually been taken to, and that is the one drawn strongly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    pub target: Target,
    pub axis: GuideAxis,
    pub position: f64,
    pub active: bool,
}

/// Everything about the drag that does not change while the cursor moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// The usable area of the display the cursor is on — `NSScreen`'s
    /// `visibleFrame`, never `frame`: a panel snapped to "the bottom edge"
    /// must sit above the Dock, not behind it.
    pub visible: Rect,
    /// The real `NSWindow`'s own size, read live rather than taken from
    /// `theme::PANEL_WIDTH_WITH_DETAIL_PX`, so this cannot drift from what is
    /// actually on screen.
    pub panel: Size,
    /// Where a summon would put this panel on this display. See the module
    /// doc comment for why the caller computes it.
    pub home: Point,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    pub origin: Point,
    pub guides: Vec<Guide>,
}

/// Where the panel goes for a cursor that wants its origin at `desired`, and
/// which guides to draw for it.
///
/// Order of operations, and each step matters:
///
/// 1. **`desired` is clamped into the visible frame first.** A drag that runs
///    off the side of the screen leaves the panel against that edge instead of
///    somewhere it cannot be picked up again. The panel is a transient tool
///    with no title bar and no window-list entry — half of it hanging off a
///    display has no upside, and "you can lose it" is a real failure mode, so
///    this is a hard clamp rather than a "keep 100pt visible" rule.
/// 2. **Every candidate is clamped the same way**, so a target that would sit
///    off-screen (a home position computed for a display too short to hold the
///    panel at it) still resolves to a reachable origin — and its guide is
///    then drawn from that *clamped* origin, so the line never promises a
///    position the panel will not take.
/// 3. **The two axes are independent.** Snapping x to the left edge while y
///    stays exactly where the cursor put it is the common case, not an edge
///    case.
pub fn resolve(geometry: &Geometry, desired: Point) -> Resolution {
    let (min_x, max_x) = axis_range(
        geometry.visible.x,
        geometry.visible.width,
        geometry.panel.width,
    );
    let (min_y, max_y) = axis_range(
        geometry.visible.y,
        geometry.visible.height,
        geometry.panel.height,
    );

    let desired = Point {
        x: desired.x.clamp(min_x, max_x),
        y: desired.y.clamp(min_y, max_y),
    };

    let x_candidates = candidates(
        [
            (Target::LeftEdge, geometry.visible.x),
            (
                Target::HorizontalCenter,
                geometry.visible.x + (geometry.visible.width - geometry.panel.width) / 2.0,
            ),
            (
                Target::RightEdge,
                geometry.visible.max_x() - geometry.panel.width,
            ),
            (Target::HomeCenter, geometry.home.x),
        ],
        min_x,
        max_x,
    );
    let y_candidates = candidates(
        [
            (Target::BottomEdge, geometry.visible.y),
            (
                Target::VerticalCenter,
                geometry.visible.y + (geometry.visible.height - geometry.panel.height) / 2.0,
            ),
            (
                Target::TopEdge,
                geometry.visible.max_y() - geometry.panel.height,
            ),
            (Target::HomeTop, geometry.home.y),
        ],
        min_y,
        max_y,
    );

    let (x, mut guides) = snap_axis(desired.x, &x_candidates, geometry.panel.width);
    let (y, y_guides) = snap_axis(desired.y, &y_candidates, geometry.panel.height);
    guides.extend(y_guides);

    Resolution {
        origin: Point { x, y },
        guides,
    }
}

/// The inclusive range an origin may take along one axis so the panel stays
/// fully inside the visible frame.
///
/// A panel wider (or taller) than the screen has no such range at all: the
/// range collapses to the visible frame's own origin, which pins the panel's
/// top-left corner on screen and lets the overflow fall off the far side. Not
/// hypothetical — a 420pt-tall panel on a display running at a scaled
/// resolution with a large Dock is close enough that the arithmetic must not
/// produce an inverted range (`clamp` panics on `min > max`).
fn axis_range(visible_origin: f64, visible_extent: f64, panel_extent: f64) -> (f64, f64) {
    let slack = visible_extent - panel_extent;
    if slack <= 0.0 {
        (visible_origin, visible_origin)
    } else {
        (visible_origin, visible_origin + slack)
    }
}

/// Clamps each candidate origin into the legal range and drops duplicates,
/// keeping the first (the declaration order above is the priority order).
fn candidates(raw: [(Target, f64); 4], min: f64, max: f64) -> Vec<(Target, f64)> {
    let mut out: Vec<(Target, f64)> = Vec::with_capacity(raw.len());
    for (target, origin) in raw {
        let origin = origin.clamp(min, max);
        // Half a point: below anything a person could see or a display could
        // render, so two targets this close are one target with two names.
        if out
            .iter()
            .any(|(_, existing)| (existing - origin).abs() < 0.5)
        {
            continue;
        }
        out.push((target, origin));
    }
    out
}

/// The nearest candidate within [`SNAP_THRESHOLD_PT`], plus a guide for every
/// candidate that is within it — the nearest one marked active.
///
/// Ties go to the earlier candidate, which is why [`candidates`] keeps its
/// declaration order: with two targets exactly equidistant, an edge beats a
/// centre beats home, deterministically, rather than flickering between them
/// as the cursor jitters.
fn snap_axis(desired: f64, candidates: &[(Target, f64)], panel_extent: f64) -> (f64, Vec<Guide>) {
    let within: Vec<(Target, f64)> = candidates
        .iter()
        .copied()
        .filter(|(_, origin)| (origin - desired).abs() <= SNAP_THRESHOLD_PT)
        .collect();

    let Some((_, nearest)) = within
        .iter()
        .copied()
        .min_by(|(_, a), (_, b)| (a - desired).abs().total_cmp(&(b - desired).abs()))
    else {
        return (desired, Vec::new());
    };

    let guides = within
        .iter()
        .map(|&(target, origin)| Guide {
            target,
            axis: target.axis(),
            position: target.line(origin, panel_extent),
            active: (origin - nearest).abs() < f64::EPSILON,
        })
        .collect();

    (nearest, guides)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1440×900 display with the menu bar and a bottom Dock taken out, and
    /// this app's real panel size — the shape almost every assertion below is
    /// made against.
    fn geometry() -> Geometry {
        let visible = Rect {
            x: 0.0,
            y: 70.0,
            width: 1440.0,
            height: 805.0,
        };
        let panel = Size {
            width: 760.0,
            height: 420.0,
        };
        // What `display_placement::upper_third_offset` produces for the full
        // 1440×900 frame: centred horizontally, `900/3 − 420/4 = 195` down
        // from the top, expressed as a y-up bottom-left origin.
        let home = Point {
            x: (1440.0 - 760.0) / 2.0,
            y: 900.0 - 195.0 - 420.0,
        };
        Geometry {
            visible,
            panel,
            home,
        }
    }

    fn active(resolution: &Resolution, axis: GuideAxis) -> Option<Guide> {
        resolution
            .guides
            .iter()
            .copied()
            .find(|g| g.axis == axis && g.active)
    }

    #[test]
    fn a_position_far_from_every_target_is_left_exactly_where_the_cursor_put_it() {
        let g = geometry();
        let desired = Point { x: 300.0, y: 200.0 };
        let resolved = resolve(&g, desired);
        assert_eq!(resolved.origin, desired);
        assert!(
            resolved.guides.is_empty(),
            "no target is in reach, so nothing should be drawn"
        );
    }

    #[test]
    fn approaching_the_left_edge_snaps_to_it_and_draws_one_guide_along_the_panel_edge() {
        let g = geometry();
        let resolved = resolve(
            &g,
            Point {
                x: g.visible.x + 9.0,
                y: 200.0,
            },
        );
        assert_eq!(resolved.origin.x, g.visible.x);
        assert_eq!(
            resolved.origin.y, 200.0,
            "the y axis is untouched by an x snap"
        );

        let guide = active(&resolved, GuideAxis::Vertical).expect("an active vertical guide");
        assert_eq!(guide.target, Target::LeftEdge);
        assert_eq!(
            guide.position, g.visible.x,
            "the line marks the panel's own left edge"
        );
        assert_eq!(resolved.guides.len(), 1);
    }

    #[test]
    fn approaching_the_right_edge_snaps_the_panels_right_edge_to_the_screens() {
        let g = geometry();
        let right_origin = g.visible.max_x() - g.panel.width;
        let resolved = resolve(
            &g,
            Point {
                x: right_origin - 5.0,
                y: 200.0,
            },
        );
        assert_eq!(resolved.origin.x, right_origin);

        let guide = active(&resolved, GuideAxis::Vertical).unwrap();
        assert_eq!(guide.target, Target::RightEdge);
        assert_eq!(guide.position, g.visible.max_x());
    }

    #[test]
    fn the_centre_guide_marks_the_panels_centre_line_not_its_edge() {
        let g = geometry();
        let centre_origin = g.visible.x + (g.visible.width - g.panel.width) / 2.0;
        let resolved = resolve(
            &g,
            Point {
                x: centre_origin + 4.0,
                y: 200.0,
            },
        );
        assert_eq!(resolved.origin.x, centre_origin);

        let guide = active(&resolved, GuideAxis::Vertical).unwrap();
        assert_eq!(guide.position, g.visible.x + g.visible.width / 2.0);
    }

    #[test]
    fn home_is_reachable_again_after_a_drag_and_its_guide_runs_along_the_panels_top_edge() {
        let g = geometry();
        let resolved = resolve(
            &g,
            Point {
                x: 300.0,
                y: g.home.y - 7.0,
            },
        );
        assert_eq!(resolved.origin.y, g.home.y);

        let guide = active(&resolved, GuideAxis::Horizontal).unwrap();
        assert_eq!(guide.target, Target::HomeTop);
        assert_eq!(guide.position, g.home.y + g.panel.height);
    }

    #[test]
    fn homes_x_collapses_into_the_horizontal_centre_when_the_dock_is_not_on_a_side() {
        // The common case: one guide, not two stacked on the same pixel.
        let g = geometry();
        let centre_origin = g.visible.x + (g.visible.width - g.panel.width) / 2.0;
        assert_eq!(
            g.home.x, centre_origin,
            "the fixture itself must have them coincide"
        );

        let resolved = resolve(
            &g,
            Point {
                x: centre_origin + 2.0,
                y: 200.0,
            },
        );
        assert_eq!(resolved.guides.len(), 1);
        assert_eq!(resolved.guides[0].target, Target::HorizontalCenter);
    }

    #[test]
    fn a_dock_on_the_side_genuinely_separates_home_from_the_visible_centre() {
        // A 64pt Dock on the left: the visible frame starts at x=64, so its
        // centre is 32pt right of the display's own centre, which is where a
        // summon actually puts the panel.
        let g = Geometry {
            visible: Rect {
                x: 64.0,
                y: 0.0,
                width: 1440.0 - 64.0,
                height: 875.0,
            },
            panel: Size {
                width: 760.0,
                height: 420.0,
            },
            home: Point {
                x: (1440.0 - 760.0) / 2.0,
                y: 900.0 - 195.0 - 420.0,
            },
        };
        let visible_centre = g.visible.x + (g.visible.width - g.panel.width) / 2.0;
        assert!(
            (visible_centre - g.home.x).abs() > 0.5,
            "the fixture must actually separate them"
        );

        // Halfway between the two, and both are inside the threshold.
        let between = (visible_centre + g.home.x) / 2.0;
        let resolved = resolve(
            &g,
            Point {
                x: between,
                y: 200.0,
            },
        );
        let vertical: Vec<Target> = resolved
            .guides
            .iter()
            .filter(|g| g.axis == GuideAxis::Vertical)
            .map(|g| g.target)
            .collect();
        assert_eq!(vertical.len(), 2, "both are in reach, so both are drawn");
        assert!(vertical.contains(&Target::HomeCenter));
    }

    #[test]
    fn two_targets_within_reach_draw_two_guides_with_exactly_one_active() {
        // The 1280×800 case named in `SNAP_THRESHOLD_PT`'s own doc comment:
        // home and the vertical centre are 28.3pt apart, so a cursor between
        // them is within reach of both.
        let g = Geometry {
            visible: Rect {
                x: 0.0,
                y: 0.0,
                width: 1280.0,
                height: 800.0,
            },
            panel: Size {
                width: 760.0,
                height: 420.0,
            },
            home: Point {
                x: (1280.0 - 760.0) / 2.0,
                y: 800.0 - (800.0 / 3.0 - 105.0) - 420.0,
            },
        };
        let centre_origin = (800.0 - 420.0) / 2.0;
        assert!((g.home.y - centre_origin).abs() < 2.0 * SNAP_THRESHOLD_PT);

        let between = (g.home.y + centre_origin) / 2.0;
        let resolved = resolve(
            &g,
            Point {
                x: 300.0,
                y: between + 1.0,
            },
        );
        let horizontal: Vec<Guide> = resolved
            .guides
            .iter()
            .copied()
            .filter(|g| g.axis == GuideAxis::Horizontal)
            .collect();
        assert_eq!(horizontal.len(), 2);
        assert_eq!(horizontal.iter().filter(|g| g.active).count(), 1);
        // Nudged toward home, so home is the one that takes it.
        assert!(horizontal.iter().find(|g| g.active).unwrap().target == Target::HomeTop);
    }

    #[test]
    fn the_threshold_is_inclusive_at_its_own_boundary_and_dead_one_point_past_it() {
        let g = geometry();
        let at = resolve(
            &g,
            Point {
                x: g.visible.x + SNAP_THRESHOLD_PT,
                y: 200.0,
            },
        );
        assert_eq!(
            at.origin.x, g.visible.x,
            "exactly at the threshold still snaps"
        );

        let past = resolve(
            &g,
            Point {
                x: g.visible.x + SNAP_THRESHOLD_PT + 1.0,
                y: 200.0,
            },
        );
        assert_eq!(past.origin.x, g.visible.x + SNAP_THRESHOLD_PT + 1.0);
        assert!(past.guides.is_empty());
    }

    #[test]
    fn a_drag_that_runs_off_the_screen_leaves_the_panel_fully_on_it() {
        let g = geometry();
        let far_off = resolve(
            &g,
            Point {
                x: -5000.0,
                y: -5000.0,
            },
        );
        assert_eq!(
            far_off.origin,
            Point {
                x: g.visible.x,
                y: g.visible.y
            }
        );

        let far_off_other_way = resolve(
            &g,
            Point {
                x: 9000.0,
                y: 9000.0,
            },
        );
        assert_eq!(
            far_off_other_way.origin,
            Point {
                x: g.visible.max_x() - g.panel.width,
                y: g.visible.max_y() - g.panel.height
            }
        );
    }

    #[test]
    fn a_panel_bigger_than_the_screen_pins_to_the_origin_rather_than_producing_an_inverted_range() {
        let g = Geometry {
            visible: Rect {
                x: 100.0,
                y: 50.0,
                width: 600.0,
                height: 300.0,
            },
            panel: Size {
                width: 760.0,
                height: 420.0,
            },
            home: Point { x: 100.0, y: 50.0 },
        };
        let resolved = resolve(&g, Point { x: 400.0, y: 400.0 });
        assert_eq!(resolved.origin, Point { x: 100.0, y: 50.0 });
        assert!(resolved.origin.x.is_finite() && resolved.origin.y.is_finite());
    }

    #[test]
    fn both_axes_can_snap_at_once_and_each_reports_its_own_guide() {
        let g = geometry();
        let resolved = resolve(
            &g,
            Point {
                x: g.visible.x + 3.0,
                y: g.visible.y + 3.0,
            },
        );
        assert_eq!(
            resolved.origin,
            Point {
                x: g.visible.x,
                y: g.visible.y
            }
        );
        assert!(active(&resolved, GuideAxis::Vertical).is_some());
        assert!(active(&resolved, GuideAxis::Horizontal).is_some());
        assert_eq!(resolved.guides.len(), 2);
    }

    #[test]
    fn a_rect_owns_a_cursor_on_its_low_edge_but_not_its_high_one() {
        // Two abutting displays must never both claim the same cursor, which
        // is what `pick_screen_for_point` relies on.
        let left = Rect {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        };
        let right = Rect {
            x: 1440.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        assert!(left.contains(0.0, 0.0));
        assert!(!left.contains(1440.0, 0.0));
        assert!(right.contains(1440.0, 0.0));
    }
}

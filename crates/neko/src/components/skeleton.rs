//! Placeholder rows for a list that is still being fetched.
//!
//! **This exists because an empty list was saying the wrong thing.** A mode
//! renders `ModeChrome::empty_line` whenever it holds no rows — "No agents
//! running", "No providers signed in" — which is a *statement of fact* about
//! a finished search. During a fetch it is simply false, and the modes where
//! the wait is real are exactly the ones backed by a network round trip:
//! Usage fans out to three vendor APIs, Terminals and Schedules go over MCP.
//! So the first thing those surfaces did was assert there was nothing there,
//! and then contradict themselves a moment later.
//!
//! ## It does not animate itself
//!
//! `gpui-component`'s own `skeleton.rs` is 59 lines and all of them are an
//! `Animation::new(..).repeat()`, which this project forbids after comet's
//! measured incident — one repeating element pinned a window at 120Hz and
//! 36% CPU. It was declined for exactly that reason and rebuilt here on
//! [`motion::PulseClock`], the one sanctioned repeating drive: a shared
//! 12.5Hz tick that stops dead when nothing is painting a skeleton, and that
//! returns a fixed midpoint under reduce-motion so the placeholder still
//! reads as a placeholder without moving.

use gpui::{Div, ParentElement, Styled, div, prelude::FluentBuilder as _, px};

use crate::theme;

/// How many placeholder rows to draw.
///
/// Enough to read as a list rather than as one stray element, few enough that
/// the real answer replacing three rows with two is not a visible collapse.
/// Every mode this appears in returns a handful of rows.
pub const SKELETON_ROWS: usize = 3;

/// How far the shimmer travels, as a fraction of the plate's own alpha.
///
/// Deliberately narrow. A placeholder's job is to say "this is coming", and a
/// hard pulse between invisible and solid reads as something failing rather
/// than something loading.
const SHIMMER: f32 = 0.45;

/// Bar widths, as a fraction of the row's text column.
///
/// Varied per row on purpose: three identical bars read as a table with
/// missing data, where an uneven stack reads as text that has not arrived.
const TITLE_WIDTHS: [f32; SKELETON_ROWS] = [0.42, 0.58, 0.33];
const SUBTITLE_WIDTHS: [f32; SKELETON_ROWS] = [0.66, 0.48, 0.72];

/// One shimmering plate.
fn plate(intensity: f32) -> Div {
    let mut color = theme::active().row_icon_socket_bg;
    // The clock's intensity is a smooth 0..1 breath; this maps it onto a
    // narrow band around the plate's resting alpha rather than onto the whole
    // range, so the row never blinks.
    color.a *= 1.0 - SHIMMER / 2.0 + SHIMMER * intensity;
    div().bg(color).rounded(px(4.))
}

/// What the rows being waited for will actually look like.
///
/// **A placeholder whose shape does not match what arrives is a different
/// kind of jump, not a fix.** Usage renders meters — a title over a note on
/// the left, a fixed-width column of ticks with numbers beneath on the right
/// — and standing a generic two-bar row in for one only moves the layout
/// shift from "empty to full" to "wrong shape to right shape". The mode says
/// which shape it expects, the same way it already says whether it has a
/// detail pane or is a transcript: chrome is the client's own layout
/// vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkeletonShape {
    /// Icon, title, subtitle — the ordinary list row.
    Row,
    /// `panel::render_meter`'s two columns.
    Meter,
    /// Chat bubbles, alternating sides.
    Bubble,
}

/// Placeholder rows in the shape of whatever is being fetched.
///
/// `intensity` comes from [`crate::motion::PulseClock`], so every skeleton on
/// screen breathes in step and none of them owns a timer.
pub fn skeleton_list(shape: SkeletonShape, intensity: f32) -> Div {
    let mut column = div().flex().flex_col().px(px(theme::CONTENT_INSET_PX));
    for row in 0..SKELETON_ROWS {
        column = column.child(match shape {
            SkeletonShape::Row => row_skeleton(row, intensity),
            SkeletonShape::Meter => meter_skeleton(row, intensity),
            SkeletonShape::Bubble => bubble_skeleton(row, intensity),
        });
    }
    column
}

fn row_skeleton(row: usize, intensity: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .h(px(theme::RESULT_ROW_HEIGHT_PX))
        .px_3()
        // The icon socket, at the size a real row's artwork occupies, so
        // nothing shifts sideways when the answer lands.
        .child(
            plate(intensity)
                .flex_shrink_0()
                .size(px(theme::ROW_ICON_PX)),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(
                    plate(intensity)
                        .h(px(9.))
                        .w(gpui::relative(TITLE_WIDTHS[row])),
                )
                .child(
                    plate(intensity)
                        .h(px(7.))
                        .w(gpui::relative(SUBTITLE_WIDTHS[row])),
                ),
        )
}

/// `panel::render_meter`'s own two columns, in plates.
///
/// The right column is `METER_COLUMN_WIDTH_PX` and holds `METER_TICK_COUNT`
/// ticks at their real height — the same fixed width the real meters use so
/// every reading's bar starts at the same x, which is the property that would
/// be most obviously broken by a placeholder that guessed.
fn meter_skeleton(row: usize, intensity: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap_7()
        .px_3()
        .py_2p5()
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    plate(intensity)
                        .h(px(11.))
                        .w(gpui::relative(TITLE_WIDTHS[row])),
                )
                .child(
                    plate(intensity)
                        .h(px(8.))
                        .w(gpui::relative(SUBTITLE_WIDTHS[row])),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(theme::METER_COLUMN_WIDTH_PX))
                .child(div().flex().gap(px(theme::METER_TICK_GAP_PX)).children(
                    (0..theme::METER_TICK_COUNT).map(|_| {
                        plate(intensity)
                            .flex_1()
                            .h(px(theme::METER_TICK_HEIGHT_PX))
                            .rounded(px(theme::METER_TICK_RADIUS_PX))
                    }),
                ))
                .child(
                    div()
                        .mt(px(6.))
                        .child(plate(intensity).h(px(8.)).w(gpui::relative(0.45))),
                ),
        )
}

/// Alternating chat bubbles — the user's right-aligned, the agent's not.
fn bubble_skeleton(row: usize, intensity: f32) -> Div {
    let from_user = row.is_multiple_of(2);
    div()
        .flex()
        .py(px(6.))
        .when(from_user, |el| el.justify_end())
        .child(
            plate(intensity)
                .h(px(30.))
                .w(gpui::relative(if from_user { 0.38 } else { 0.62 }))
                .rounded(px(10.)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_usage_mode_expects_meters_and_a_conversation_expects_bubbles() {
        // The whole point of matching the shape: a generic row here would
        // only move the layout shift from "empty to full" to "wrong shape to
        // right shape".
        let usage = crate::modes::chrome_for("usage").expect("the usage mode");
        assert_eq!(usage.skeleton, SkeletonShape::Meter);
        let chat = crate::modes::chrome_for("conversation").expect("the conversation mode");
        assert_eq!(chat.skeleton, SkeletonShape::Bubble);
        let plain = crate::modes::chrome_for("schedule").expect("the schedules mode");
        assert_eq!(
            plain.skeleton,
            SkeletonShape::Row,
            "an ordinary list stays a row"
        );
    }

    #[test]
    fn the_shimmer_never_reaches_either_extreme() {
        // A placeholder that blinks to invisible reads as something failing.
        // Both ends of the clock's range have to stay inside the plate's own
        // alpha, not swing across it.
        let resting = theme::active().row_icon_socket_bg.a;
        for intensity in [0.0_f32, 0.5, 1.0] {
            let scale = 1.0 - SHIMMER / 2.0 + SHIMMER * intensity;
            let alpha = resting * scale;
            assert!(alpha > 0.0, "never invisible at intensity {intensity}");
            assert!(alpha <= resting * 1.25, "never brighter than a real plate");
        }
    }

    #[test]
    fn no_two_placeholder_rows_are_the_same_width() {
        // Identical bars read as a table with missing data; uneven ones read
        // as text that has not arrived yet.
        let mut widths = TITLE_WIDTHS.to_vec();
        widths.sort_by(f32::total_cmp);
        widths.dedup();
        assert_eq!(widths.len(), SKELETON_ROWS);
    }
}

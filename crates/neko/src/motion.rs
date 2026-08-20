//! A small, throttled motion catalog — the comet craft study's recommendation
//! 2 (`data/neko-comet-design/report.md`, firstmate home), reimplemented from
//! scratch against the published `gpui = "0.2.2"` neko actually compiles
//! against, not comet's own git-fork `gpui`. Comet's `crates/ui/src/motion.rs`
//! was read for the *shape* (a `CubicBezier`/`MotionSpec` pair, a handful of
//! named specs, thin element helpers over `.with_animation(...)`) — every
//! line below is neko's own, and every numeric value was chosen for neko's
//! own places motion applies, not copied from comet's table.
//!
//! **Deliberately small: two specs, three call sites** — the actions menu's
//! open transition (`panel::Root::render_actions_menu`), a mode's content
//! reveal on entry (`panel::Root::render_mode_content`), and the "still
//! searching" tell for a query that hasn't returned yet
//! (`panel::Root::render_input_row`). Per the launch brief's own tie-break
//! ("motion is worth less than correctness — if the catalog has to be three
//! specs and two call sites to stay honest, make it three and two"), nothing
//! here was added speculatively:
//! - The actions menu's *close* is an instant cut, not animated — a real
//!   fade-out needs the menu's state kept alive past the frame it logically
//!   closes (comet's own `Popup<T>`, `closing_since` + a reap timer), which
//!   is exactly the kind of state-machine surface the trigger-press race
//!   guard (`panel.rs`'s own doc comments on `menu_open_before_this_press`)
//!   most needs to stay simple and provably correct. The brief's own
//!   priority ("the actions menu is the one the captain touches; get its
//!   behaviour exactly right, including the race") settles this: correctness
//!   for the close path, not a matching fade.
//! - Summon itself is never animated — the launch brief says so explicitly,
//!   and warm summon is ~4-6ms (`AGENTS.md`, "Summon latency"); fading a
//!   panel that's already there before the eye can register motion would
//!   make a fast thing read as slow.
//! - Row selection, hover, and every other steady-state interaction stay
//!   instant cuts — the brief names exactly three places worth fixing, not a
//!   blanket "animate everything."
//!
//! **Reduced motion**: `App::reduce_motion()`/`set_reduce_motion()` do not
//! exist in published `gpui-0.2.2` (confirmed by grep against the vendored
//! source — see the design report's §1), so there is no automatic snap the
//! way comet gets one for free. [`system_reduce_motion`] reads the real OS
//! setting directly (`NSWorkspace.accessibilityDisplayShouldReduceMotion`,
//! the same AppKit-read pattern `material.rs`/`spaces.rs` already establish
//! for other native state) and every element helper below takes that flag
//! explicitly, skipping `with_animation` entirely rather than merely forcing
//! its output to the end state — a skipped animation schedules zero extra
//! frames; a forced-but-still-running one would still request frames for its
//! whole nominal duration for no visual benefit.
//!
//! **No repeating animation exists in this catalog.** Both specs are
//! one-shot (`Animation::new(..)`, never `.repeat()`) — `gpui`'s own
//! `AnimationElement::request_layout` (confirmed by reading
//! `gpui-0.2.2/src/elements/animation.rs`) stops calling
//! `window.request_animation_frame()` the instant a one-shot's delta passes
//! 1.0, so a mounted-and-finished fade schedules nothing further on its own;
//! an *unmounted* one (the common case — these fades only exist for ~120-
//! 150ms around a state transition) schedules nothing at all. If a future
//! change to this catalog ever adds a repeating animation (a spinner, a
//! pulse), it must go through a shared, explicitly-throttled clock — never
//! `with_animation(...).repeat()` directly — per comet's own `PulseClock`
//! (`motion.rs:44-118` there) and its own recorded incident: one repeating
//! element pinned a whole window at 120Hz, measured 36% CPU. Nothing in
//! neko's catalog needs this today, so no clock exists yet; this paragraph
//! is the seam for the day one does.

use gpui::{AnimationExt, Div, IntoElement, Styled, px};

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function (endpoints fixed at
/// (0,0)/(1,1)) — evaluated by Newton–Raphson with a bisection fallback, the
/// standard `UnitBezier` approach (independently implemented here; not
/// ported from comet's own `CubicBezier`, which uses the identical
/// well-known technique for the identical well-known problem).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl CubicBezier {
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn coefficients(a: f32, b: f32) -> (f32, f32, f32) {
        let c = 3.0 * a;
        let bb = 3.0 * (b - a) - c;
        let aa = 1.0 - c - bb;
        (aa, bb, c)
    }

    fn sample_x(&self, t: f32) -> f32 {
        let (a, b, c) = Self::coefficients(self.x1, self.x2);
        ((a * t + b) * t + c) * t
    }

    fn sample_y(&self, t: f32) -> f32 {
        let (a, b, c) = Self::coefficients(self.y1, self.y2);
        ((a * t + b) * t + c) * t
    }

    fn sample_x_derivative(&self, t: f32) -> f32 {
        let (a, b, c) = Self::coefficients(self.x1, self.x2);
        (3.0 * a * t + 2.0 * b) * t + c
    }

    fn solve_t_for_x(&self, x: f32) -> f32 {
        let mut t = x;
        for _ in 0..8 {
            let err = self.sample_x(t) - x;
            if err.abs() < 1e-6 {
                return t;
            }
            let d = self.sample_x_derivative(t);
            if d.abs() < 1e-6 {
                break;
            }
            t -= err / d;
        }
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..32 {
            let mid = (lo + hi) / 2.0;
            if self.sample_x(mid) < x { lo = mid } else { hi = mid }
        }
        (lo + hi) / 2.0
    }

    /// Eased output for input progress `x ∈ [0,1]` (clamped both ends —
    /// `f32` rounding can push `sample_y` a hair past 1.0 near the tail of a
    /// curve, and `gpui`'s own animation element asserts its delta stays in
    /// `[0,1]`, so this is a real correctness requirement, not defensive
    /// padding).
    pub fn eval(&self, x: f32) -> f32 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        self.sample_y(self.solve_t_for_x(x)).clamp(0.0, 1.0)
    }
}

/// The standard Material/CSS "decelerate" curve — starts fast, settles in
/// gently. Chosen for every spec in this catalog: everything here is a
/// *reveal* (a menu appearing, content fading in), not an exit or a
/// symmetric transition, and a decelerating curve is what reads as
/// "arriving," not "sliding past." One curve for the whole catalog is
/// deliberate — two specs don't need two curves to feel distinct from each
/// other; their duration difference already does that.
pub const EASE_OUT: CubicBezier = CubicBezier::new(0.0, 0.0, 0.2, 1.0);

/// One catalog entry: a duration over [`EASE_OUT`]. No delay field, unlike
/// comet's own `MotionSpec` — nothing in this catalog needs a staggered
/// start, and adding the plumbing for a case that doesn't exist yet is
/// exactly the premature generality this task's own brief argues against
/// elsewhere in this codebase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSpec {
    duration_ms: u64,
}

impl MotionSpec {
    const fn new(duration_ms: u64) -> Self {
        Self { duration_ms }
    }

    fn duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.duration_ms)
    }

    /// A one-shot `gpui::Animation` for this spec — never `.repeat()`, see
    /// this module's own doc comment on why nothing here loops.
    fn animation(&self) -> gpui::Animation {
        let curve = EASE_OUT;
        gpui::Animation::new(self.duration()).with_easing(move |d| curve.eval(d))
    }
}

/// The actions menu's own open transition — a small, local element close to
/// the cursor's likely position, so it wants to read as snappy rather than
/// as a considered content reveal. 120ms: fast enough to feel like part of
/// the same gesture that opened it (comet's own equivalent, `MENU_IN`, is
/// 140ms; neko's summon ethos ("simply *there*," per the launch brief's own
/// "do not animate the summon" instruction) is a hair quicker throughout, so
/// this stays under that).
pub const MENU_FADE: MotionSpec = MotionSpec::new(120);

/// A content region appearing — a mode's list/detail pane on entry, or the
/// "still searching" tell. Slightly longer than [`MENU_FADE`]: it's read
/// over more of the panel at once, so a little more time keeps it feeling
/// deliberate rather than flickery, while still resolving well within the
/// ~150ms window past which a UI transition starts to read as sluggish
/// rather than immediate.
pub const CONTENT_FADE: MotionSpec = MotionSpec::new(150);

/// The actions menu's entrance: opacity 0→1 plus a small upward drift (4px),
/// the same "settling into place" shape comet's `menu_in` uses (translateY
/// via a relative `top` inset — `gpui` divs have no scale/translate
/// transform at this pinned version, matching the same limitation comet's
/// own module doc comment records). `reduced` skips `with_animation`
/// entirely rather than forcing its output to the end state — see this
/// module's own doc comment for why that distinction matters for idle CPU.
pub fn menu_fade_in(id: &'static str, reduced: bool, element: Div) -> gpui::AnyElement {
    if reduced {
        return element.into_any_element();
    }
    element
        .with_animation(id, MENU_FADE.animation(), |el, t| el.opacity(t).relative().top(px(4.0 * (1.0 - t))))
        .into_any_element()
}

/// A plain opacity-only reveal (no drift) — mode-content entry and the
/// "still searching" tell. Kept distinct from [`menu_fade_in`] because
/// neither of those two call sites has a natural "coming from" direction the
/// way a menu popping out of its trigger does; an unmotivated drift would
/// just be motion for its own sake.
pub fn fade_in(id: &'static str, reduced: bool, element: Div) -> gpui::AnyElement {
    if reduced {
        return element.into_any_element();
    }
    element
        .with_animation(id, CONTENT_FADE.animation(), |el, t| el.opacity(t))
        .into_any_element()
}

/// Reads the real, live OS "reduce motion" accessibility setting — see this
/// module's own doc comment for why this exists instead of a `gpui`-provided
/// global. Read fresh at each call site rather than cached: every call site
/// here fires once per state transition (a menu opening, a mode entering,
/// a search starting), not once per frame, so re-reading is a handful of
/// extra Objective-C message sends per interaction, not a per-frame cost —
/// and a cached value would go stale the moment the captain flips the
/// setting in System Settings without restarting neko.
#[cfg(target_os = "macos")]
pub fn system_reduce_motion() -> bool {
    use objc2_app_kit::NSWorkspace;
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

#[cfg(not(target_os = "macos"))]
pub fn system_reduce_motion() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32, tol: f32, ctx: &str) {
        assert!((actual - expected).abs() <= tol, "{ctx}: got {actual}, expected {expected} ±{tol}");
    }

    #[test]
    fn ease_out_endpoints_and_clamping() {
        assert_eq!(EASE_OUT.eval(0.0), 0.0);
        assert_eq!(EASE_OUT.eval(1.0), 1.0);
        assert_eq!(EASE_OUT.eval(-0.5), 0.0);
        assert_eq!(EASE_OUT.eval(1.5), 1.0);
    }

    #[test]
    fn ease_out_is_monotonic() {
        let mut last = 0.0;
        for i in 0..=100 {
            let y = EASE_OUT.eval(i as f32 / 100.0);
            assert!(y >= last - 1e-4, "monotonicity violated at {i}");
            last = y;
        }
    }

    #[test]
    fn ease_out_decelerates_faster_than_linear_at_the_start() {
        // A decelerate curve front-loads progress: well past the halfway
        // point in output by the time input is only 30% of the way there.
        let y = EASE_OUT.eval(0.3);
        assert!(y > 0.3, "expected front-loaded progress, got {y}");
    }

    #[test]
    fn eval_never_escapes_unit_interval_dense_sweep() {
        // Same regression class comet's own suite guards against: f32
        // rounding pushing `sample_y` a hair past 1.0 near the curve's tail,
        // which would trip `gpui`'s own `delta ∈ [0,1]` assert.
        for i in 0..=100_000u32 {
            let x = i as f32 / 100_000.0;
            let y = EASE_OUT.eval(x);
            assert!((0.0..=1.0).contains(&y), "eval({x}) = {y} escaped [0,1]");
        }
        for x in [0.999_999f32, 0.999_999_9, 1.0 - f32::EPSILON] {
            let y = EASE_OUT.eval(x);
            assert!((0.0..=1.0).contains(&y), "eval({x}) = {y} escaped [0,1]");
        }
    }

    #[test]
    fn catalog_durations() {
        // The menu is deliberately the snappier of the two — see
        // `MENU_FADE`'s own doc comment.
        assert_eq!(MENU_FADE.duration_ms, 120);
        assert_eq!(CONTENT_FADE.duration_ms, 150);
    }

    #[test]
    fn known_value_sanity_check() {
        // Independently computed reference for `EASE_OUT` at its midpoint —
        // cross-checks the Newton/bisection solver against a hand-derived
        // value rather than only checking shape properties.
        let y = EASE_OUT.eval(0.5);
        assert_close(y, 0.839, 5e-3, "ease-out midpoint");
    }
}

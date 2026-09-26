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

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{AnimationExt, Div, IntoElement, Rgba, SharedString, Styled, Window, px};
use gpui::{App, AppContext as _, Context, Entity, Global};

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
            if self.sample_x(mid) < x {
                lo = mid
            } else {
                hi = mid
            }
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
    /// **Added when the catalog grew past reveals.** The paragraph above still
    /// holds for everything that *is* a reveal — they all take [`EASE_OUT`] and
    /// [`new`](Self::new) still gives it to them for free. But a hover wash and
    /// a scroll glide are not reveals: a wash is a symmetric colour transition
    /// that has to feel identical entering and leaving, and a glide is a
    /// journey with a start and a landing. Those needed their own curves, and
    /// a spec that could not name one would have meant hard-coding the curve
    /// at each call site instead — which is exactly the drift a catalog exists
    /// to prevent.
    curve: CubicBezier,
}

impl MotionSpec {
    const fn new(duration_ms: u64) -> Self {
        Self {
            duration_ms,
            curve: EASE_OUT,
        }
    }

    const fn with_curve(duration_ms: u64, curve: CubicBezier) -> Self {
        Self { duration_ms, curve }
    }

    fn duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.duration_ms)
    }

    /// Eased progress for a raw `0..1` delta. Pure, so a caller driving its own
    /// tween from wall time (the hover fades, the scroll glide) rides the same
    /// curve a `with_animation` element would.
    pub fn progress(&self, raw: f32) -> f32 {
        self.curve.eval(raw.clamp(0.0, 1.0))
    }

    /// A one-shot `gpui::Animation` for this spec — never `.repeat()`, see
    /// this module's own doc comment on why nothing here loops.
    fn animation(&self) -> gpui::Animation {
        let spec = *self;
        gpui::Animation::new(self.duration()).with_easing(move |d| spec.progress(d))
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

/// CSS `cubic-bezier(0.4, 0, 0.2, 1)` — Tailwind's default transition curve,
/// and therefore what `transition-colors` rides on nearly every web interface
/// anybody has built a sense of "normal" against. Used only by [`HOVER_FADE`].
///
/// **Symmetric on purpose**, unlike [`EASE_OUT`]: a wash has to feel the same
/// arriving and leaving, because the pointer crosses the same boundary in both
/// directions and an asymmetric curve makes leaving feel like a different
/// gesture from entering.
pub const EASE_TAILWIND: CubicBezier = CubicBezier::new(0.4, 0.0, 0.2, 1.0);

/// CSS `ease-in-out` — a gentle start, a cruise, a gentle landing. The shape a
/// browser's own smooth scroll uses, which is what [`SCROLL_GLIDE`] is imitating.
pub const EASE_IN_OUT: CubicBezier = CubicBezier::new(0.42, 0.0, 0.58, 1.0);

/// Every interactive hover wash: 150ms over [`EASE_TAILWIND`].
///
/// **gpui's `.hover()` snaps by construction** — the style applies on the frame
/// the pointer enters — so this is not reachable through `with_animation` at
/// all. See [`HoverFades`] for the tween that drives it.
pub const HOVER_FADE: MotionSpec = MotionSpec::with_curve(150, EASE_TAILWIND);

/// Keyboard-driven scrolling: 250ms over the whole distance.
///
/// **Fixed duration over the whole distance, never percent-of-remaining.** A
/// proportional glide accelerates when the jump is long and crawls at the end,
/// which reads as the list resisting; a fixed span means one Down and ten Downs
/// both land in the same beat, which is what makes a held arrow key feel like
/// one continuous movement instead of a stutter.
///
/// Shorter than comet's own 500ms: that one glides a transcript somebody is
/// reading, this one keeps up with a key being held down, and 500ms of travel
/// per keypress would fall behind immediately.
pub const SCROLL_GLIDE: MotionSpec = MotionSpec::with_curve(250, EASE_IN_OUT);

/// A selection moving between fixed positions — the Preferences tab bar.
///
/// **A crossfade, not a slide, and the name says so.** comet's own equivalent
/// is `TAB_SLIDE`, and sliding would mean one indicator element positioned by a
/// measured x — which means measuring every tab's width, which this app has no
/// way to do before layout. Fading the old fill out while the new one comes up
/// reads as the fill moving without pretending to geometry nobody has.
pub const SELECTION_FADE: MotionSpec = MotionSpec::new(150);

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
        .with_animation(id, MENU_FADE.animation(), |el, t| {
            el.opacity(t).relative().top(px(4.0 * (1.0 - t)))
        })
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

/// A keyboard-driven scroll in flight.
///
/// **`ScrollHandle` moves instantly and has no animated variant** — the whole
/// public surface is `scroll_to_item` and `set_offset`, both of which take
/// effect on the next paint with no travel. So a glide is a tween this app
/// drives itself: ask the handle where it *would* land, put it back, and walk
/// it there over [`SCROLL_GLIDE`].
#[derive(Debug, Clone, Copy)]
pub struct ScrollGlide {
    from: gpui::Point<gpui::Pixels>,
    to: gpui::Point<gpui::Pixels>,
    /// What this glide last wrote to the handle. If the handle no longer reads
    /// back as this, something else moved it — a wheel gesture, a fresh search
    /// resetting the list — and the glide has been overtaken and should stop
    /// rather than drag the view back to a destination nobody wants any more.
    last_written: gpui::Point<gpui::Pixels>,
    started: Instant,
}

impl ScrollGlide {
    pub fn new(
        from: gpui::Point<gpui::Pixels>,
        to: gpui::Point<gpui::Pixels>,
        now: Instant,
    ) -> Self {
        Self {
            from,
            to,
            last_written: from,
            started: now,
        }
    }

    /// Where the view should sit at `now`, and whether the glide is finished.
    pub fn sample(&self, now: Instant) -> (gpui::Point<gpui::Pixels>, bool) {
        let duration = SCROLL_GLIDE.duration();
        let elapsed = now.saturating_duration_since(self.started);
        if duration.is_zero() || elapsed >= duration {
            return (self.to, true);
        }
        let t = SCROLL_GLIDE.progress(elapsed.as_secs_f32() / duration.as_secs_f32());
        let at = gpui::point(
            gpui::px(lerp(
                self.from.x.to_f64() as f32,
                self.to.x.to_f64() as f32,
                t,
            )),
            gpui::px(lerp(
                self.from.y.to_f64() as f32,
                self.to.y.to_f64() as f32,
                t,
            )),
        );
        (at, false)
    }

    /// Whether `current` is still where this glide left the handle.
    pub fn still_owns(&self, current: gpui::Point<gpui::Pixels>) -> bool {
        // Exact equality: this compares a value the glide itself wrote against
        // what the handle reports, with no arithmetic in between, so any
        // difference at all is somebody else's write rather than rounding.
        current == self.last_written
    }

    pub fn record_write(&mut self, at: gpui::Point<gpui::Pixels>) {
        self.last_written = at;
    }
}

// ---------------------------------------------------------------------------
// Hover washes
//
// Adapted from `comet`'s own `crates/ui/src/motion.rs` (MIT, `refs/comet`) —
// the `HoverFades` store, its frame-counter staleness rule, and the
// premultiplied `mix`. Reimplemented against this crate's own types and
// re-tested here; see `components/vendor/MANIFEST.md`.
// ---------------------------------------------------------------------------

/// Linear interpolation. Named because the alternative is writing it out at
/// four call sites and getting one of them backwards.
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Blend two colours the way a browser transitions them: component
/// interpolation in sRGB with **premultiplied** alpha.
///
/// Premultiplied is the whole point rather than a detail. Every hover wash in
/// this app fades in from fully transparent, and a straight component mix from
/// `rgba(r,g,b,0)` interpolates the *hidden* colour channels too — so a wash
/// rising out of transparent black passes visibly through grey on its way to
/// its real hue. Premultiplying makes it simply brighten.
pub fn mix(from: Rgba, to: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return from;
    }
    if t >= 1.0 {
        return to;
    }
    let a = lerp(from.a, to.a, t);
    if a <= f32::EPSILON {
        // Both endpoints effectively transparent: carry the target's hue so a
        // subsequent fade *out of* this state starts from the right colour.
        return Rgba { a: 0.0, ..to };
    }
    Rgba {
        r: lerp(from.r * from.a, to.r * to.a, t) / a,
        g: lerp(from.g * from.a, to.g * to.a, t) / a,
        b: lerp(from.b * from.a, to.b * to.a, t) / a,
        a,
    }
}

/// The shortest gap between two frame-counter advances. Below one frame at
/// 120Hz, so a real frame is never merged into its predecessor.
const MIN_TICK_INTERVAL: Duration = Duration::from_millis(4);

/// One element's hover progress.
#[derive(Debug, Clone, Copy)]
struct FadeEntry {
    /// Where this fade started from — **not always 0 or 1.** A pointer that
    /// leaves mid-fade re-anchors here at whatever the wash had actually
    /// reached, so reversing direction is continuous rather than a jump back
    /// to the far end.
    origin: f32,
    target: f32,
    started: Instant,
    /// Frame counter at the last read. See [`HoverFades::tick_at`].
    seen: u64,
}

impl FadeEntry {
    fn value(&self, now: Instant, duration: Duration) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        if duration.is_zero() || elapsed >= duration {
            return self.target;
        }
        let raw = elapsed.as_secs_f32() / duration.as_secs_f32();
        lerp(self.origin, self.target, HOVER_FADE.progress(raw))
    }

    fn settled(&self, now: Instant, duration: Duration) -> bool {
        self.origin == self.target || now.saturating_duration_since(self.started) >= duration
    }
}

/// Hover progress per element key.
///
/// **This exists because `gpui`'s `.hover()` cannot fade.** It applies its
/// style on the frame the pointer enters and removes it on the frame the
/// pointer leaves — there is no interpolation and no hook to add one, so a
/// wash that fades has to be driven by hand from wall time.
///
/// Deliberately **not** `with_animation`: that keys its clock on an element id
/// and restarts from zero whenever the element remounts, which for a result row
/// is every single keystroke — the wash would replay under a stationary pointer
/// on every character typed.
///
/// The core takes `now` explicitly so the whole state machine is testable
/// without a window or a clock.
#[derive(Default)]
pub struct HoverFades {
    entries: HashMap<String, FadeEntry>,
    frame: u64,
    /// When the frame counter last advanced. See [`tick_at`](Self::tick_at).
    last_tick: Option<Instant>,
}

impl HoverFades {
    /// One duration for the whole store.
    ///
    /// [`HOVER_FADE`] and [`SELECTION_FADE`] are both 150ms, which is not a
    /// coincidence worth relying on silently: a wash and a selection crossfade
    /// are the same kind of event to a person — "this thing is now the one" —
    /// and giving them different lengths would make a tab bar feel unlike every
    /// row above it. If they ever need to differ, this becomes a per-entry
    /// field rather than two stores.
    fn duration() -> Duration {
        debug_assert_eq!(HOVER_FADE.duration_ms, SELECTION_FADE.duration_ms);
        HOVER_FADE.duration()
    }

    /// The pointer entered or left the element behind `key`.
    ///
    /// `reduced` snaps to the endpoint rather than skipping the entry, so a
    /// reduce-motion user still gets the hover *state* — only the travel is
    /// removed. Colour is the feedback here; motion is the garnish.
    pub fn set_at(&mut self, key: &str, hovered: bool, reduced: bool, now: Instant) {
        let target = if hovered { 1.0 } else { 0.0 };
        let duration = Self::duration();
        let current = self
            .entries
            .get(key)
            .map(|e| e.value(now, duration))
            .unwrap_or(0.0);
        if target == 0.0 && !self.entries.contains_key(key) {
            // A never-hovered element reporting a leave. Recording it would
            // create an entry whose only purpose is to be pruned.
            return;
        }
        let origin = if reduced { target } else { current };
        let seen = self.frame;
        self.entries.insert(
            key.to_string(),
            FadeEntry {
                origin,
                target,
                started: now,
                seen,
            },
        );
    }

    /// Like [`set_at`](Self::set_at), but a no-op when the fade is already
    /// heading where it is being told to go.
    ///
    /// **This is what makes the store usable from `render`.** A hover arrives
    /// as an event, so `set_at` runs once per flip; a *selection* is state
    /// re-derived on every frame, and calling `set_at` with it would re-anchor
    /// the fade every frame and freeze it at its first step forever.
    pub fn set_target_at(&mut self, key: &str, on: bool, reduced: bool, now: Instant) {
        let target = if on { 1.0 } else { 0.0 };
        if self.entries.get(key).is_some_and(|e| e.target == target) {
            return;
        }
        self.set_at(key, on, reduced, now);
    }

    /// Progress for `key`, stamping it as still alive.
    pub fn value_at(&mut self, key: &str, now: Instant) -> f32 {
        let frame = self.frame;
        match self.entries.get_mut(key) {
            Some(entry) => {
                entry.seen = frame;
                entry.value(now, Self::duration())
            }
            None => 0.0,
        }
    }

    /// Once-per-frame bookkeeping. Returns whether any fade is still moving,
    /// which is what tells the caller to keep asking for frames.
    ///
    /// **The frame counter is a liveness stamp, and it is load-bearing.** An
    /// element that unmounts while hovered never receives its leave event — a
    /// result row does exactly this on every keystroke — so without pruning,
    /// the next element to reuse that key would inherit a stale full-strength
    /// wash and appear hovered when nothing is under the pointer. Going a whole
    /// frame unread is the only available proof that an element is gone.
    pub fn tick_at(&mut self, now: Instant) -> bool {
        // **At most one advance per real frame, however many surfaces call.**
        // The store is one process-wide map but this app has two windows that
        // render independently, and the counter is what decides liveness — so
        // two ticks inside one frame would advance it twice while each surface
        // had only stamped its own entries once, and each window would prune
        // the other's live hovers. Rate-limiting the advance makes the rule
        // hold for any number of surfaces without any of them knowing about
        // the others. The threshold is below one frame at 120Hz, so a genuine
        // frame is never skipped.
        let advance = self
            .last_tick
            .is_none_or(|last| now.saturating_duration_since(last) >= MIN_TICK_INTERVAL);
        if advance {
            self.frame += 1;
            self.last_tick = Some(now);
        }
        let frame = self.frame;
        let duration = Self::duration();
        let mut active = false;
        self.entries.retain(|_, entry| {
            if entry.seen + 1 < frame {
                return false;
            }
            let settled = entry.settled(now, duration);
            if !settled {
                active = true;
            }
            // Settled at rest is indistinguishable from absent, so drop it and
            // keep the map the size of what is actually hovered.
            !(settled && entry.target == 0.0)
        });
        active
    }
}

thread_local! {
    /// Main-thread only, and a `thread_local` rather than a gpui `Global` so a
    /// free-standing element builder can blend a colour without a `cx` — every
    /// reader here is an element builder, a mouse listener, or the render tail,
    /// all of which run on the UI thread.
    static HOVER_FADES: RefCell<HoverFades> = RefCell::new(HoverFades::default());
}

/// Hover progress for `key` this frame.
pub fn hover_t(key: &str) -> f32 {
    HOVER_FADES.with(|fades| fades.borrow_mut().value_at(key, Instant::now()))
}

/// Record a hover flip for `key`.
pub fn set_hover(key: &str, hovered: bool, reduced: bool) {
    HOVER_FADES.with(|fades| {
        fades
            .borrow_mut()
            .set_at(key, hovered, reduced, Instant::now())
    });
}

/// The `.on_hover` listener for `key` — pair it with [`hover_blend`] on the
/// same key in the same element.
///
/// `window.refresh()` rather than `request_animation_frame`: this runs in
/// event-dispatch context, where the latter resolves against the current view
/// and is draw-phase-only. Refresh marks the window dirty, the render pass
/// re-evaluates the blend, and its tail keeps frames coming while anything is
/// still moving.
pub fn hover_listener(
    key: impl Into<SharedString>,
) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    let key = key.into();
    move |hovered, window, cx| {
        set_hover(&key, *hovered, cx.reduce_motion());
        window.refresh();
    }
}

/// Call **once per window frame**, from the render tail. True while any wash is
/// mid-flight, which is the signal to request another frame.
pub fn hover_fades_active() -> bool {
    HOVER_FADES.with(|fades| fades.borrow_mut().tick_at(Instant::now()))
}

/// Record a *state* for `key` — see [`HoverFades::set_target_at`].
pub fn set_state(key: &str, on: bool, reduced: bool) {
    HOVER_FADES.with(|fades| {
        fades
            .borrow_mut()
            .set_target_at(key, on, reduced, Instant::now())
    });
}

/// `off` → `on` at `key`'s current progress, for a state re-derived every
/// frame rather than delivered as an event. Records the target and blends in
/// one call, because the two must not drift apart.
pub fn state_blend(key: &str, on: bool, reduced: bool, off_color: Rgba, on_color: Rgba) -> Rgba {
    set_state(key, on, reduced);
    mix(off_color, on_color, hover_t(key))
}

/// `rest` → `hover` at `key`'s current progress. The one call an element makes.
pub fn hover_blend(key: &str, rest: Rgba, hover: Rgba) -> Rgba {
    mix(rest, hover, hover_t(key))
}

/// The one-line form every call site uses: a wash that fades.
///
/// **Replaces `gpui`'s own `.hover(..)` rather than sitting beside it**, because
/// the two cannot be combined — `.hover()` applies a style on the frame the
/// pointer enters, so an element carrying both would snap to the end colour and
/// then fade a second, invisible one underneath it.
///
/// `key` must be stable across re-renders and unique across the window. A row's
/// own identity works; its list index does not, since the row under a stationary
/// pointer changes index whenever the list above it does.
pub trait HoverWash: gpui::StatefulInteractiveElement + Styled + Sized {
    /// Fade the background from `rest` to `hovered`.
    fn hover_bg(self, key: impl Into<SharedString>, rest: Rgba, hovered: Rgba) -> Self {
        let key = key.into();
        let blended = hover_blend(&key, rest, hovered);
        self.bg(blended).on_hover(hover_listener(key))
    }

    /// Fade the text colour from `rest` to `hovered` — for a control whose
    /// affordance is the label itself, where a background wash would read as a
    /// button appearing where there was none.
    fn hover_text(self, key: impl Into<SharedString>, rest: Rgba, hovered: Rgba) -> Self {
        let key = key.into();
        let blended = hover_blend(&key, rest, hovered);
        self.text_color(blended).on_hover(hover_listener(key))
    }

    /// Fade the border colour — for a control already using its fill to mean
    /// something (a switch), where a second fill on top is unreadable.
    fn hover_border(self, key: impl Into<SharedString>, rest: Rgba, hovered: Rgba) -> Self {
        let key = key.into();
        let blended = hover_blend(&key, rest, hovered);
        self.border_color(blended).on_hover(hover_listener(key))
    }
}

impl<E: gpui::StatefulInteractiveElement + Styled> HoverWash for E {}

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

    #[test]
    fn a_state_re_derived_every_frame_does_not_restart_its_own_fade() {
        // A hover arrives as an event; a selection is recomputed on every
        // render. Calling the event form with it would re-anchor the fade every
        // frame and freeze it one step from the start, forever.
        let mut fades = HoverFades::default();
        let t0 = Instant::now();
        fades.set_target_at("tab", true, false, t0);
        let quarter = t0 + HOVER_FADE.duration() / 4;
        fades.set_target_at("tab", true, false, quarter);
        fades.set_target_at("tab", true, false, quarter);
        let half = t0 + HOVER_FADE.duration() / 2;
        fades.set_target_at("tab", true, false, half);

        let done = t0 + HOVER_FADE.duration();
        assert_eq!(
            fades.value_at("tab", done),
            1.0,
            "the fade kept its original start and finished on time"
        );
    }

    #[test]
    fn a_state_flip_still_reverses() {
        let mut fades = HoverFades::default();
        let t0 = Instant::now();
        fades.set_target_at("tab", true, false, t0);
        assert_eq!(fades.value_at("tab", t0 + HOVER_FADE.duration()), 1.0);
        fades.set_target_at("tab", false, false, t0 + HOVER_FADE.duration());
        assert_eq!(fades.value_at("tab", t0 + HOVER_FADE.duration() * 2), 0.0);
    }

    #[test]
    fn a_glide_starts_where_it_was_and_lands_where_it_was_told() {
        let t0 = Instant::now();
        let from = gpui::point(gpui::px(0.), gpui::px(0.));
        let to = gpui::point(gpui::px(0.), gpui::px(-120.));
        let glide = ScrollGlide::new(from, to, t0);

        let (at, done) = glide.sample(t0);
        assert_eq!(at, from);
        assert!(!done);

        let (at, done) = glide.sample(t0 + SCROLL_GLIDE.duration());
        assert_eq!(at, to, "and lands exactly, not near");
        assert!(done);
    }

    #[test]
    fn a_glide_that_something_else_overtook_gives_up_its_claim() {
        // A wheel gesture or a fresh search moves the handle out from under a
        // glide. Continuing would drag the view back to a destination nobody
        // wants any more — the same rule that makes a superseded search abandon
        // rather than finish.
        let t0 = Instant::now();
        let from = gpui::point(gpui::px(0.), gpui::px(0.));
        let mut glide = ScrollGlide::new(from, gpui::point(gpui::px(0.), gpui::px(-120.)), t0);
        assert!(glide.still_owns(from));

        glide.record_write(gpui::point(gpui::px(0.), gpui::px(-40.)));
        assert!(glide.still_owns(gpui::point(gpui::px(0.), gpui::px(-40.))));
        assert!(
            !glide.still_owns(gpui::point(gpui::px(0.), gpui::px(-300.))),
            "somebody else wrote to the handle"
        );
    }

    #[test]
    fn a_glide_is_monotonic_so_the_view_never_backs_up_mid_travel() {
        let t0 = Instant::now();
        let glide = ScrollGlide::new(
            gpui::point(gpui::px(0.), gpui::px(0.)),
            gpui::point(gpui::px(0.), gpui::px(-200.)),
            t0,
        );
        let mut previous = f32::MAX;
        for step in 0..=20 {
            let at = glide
                .sample(t0 + SCROLL_GLIDE.duration().mul_f32(step as f32 / 20.0))
                .0;
            let y = at.y.to_f64() as f32;
            assert!(
                y <= previous + 1e-3,
                "step {step}: went back from {previous} to {y}"
            );
            previous = y;
        }
    }

    #[test]
    fn a_wash_starts_at_rest_and_arrives_at_full() {
        let mut fades = HoverFades::default();
        let t0 = Instant::now();
        assert_eq!(fades.value_at("k", t0), 0.0, "never hovered is rest");
        fades.set_at("k", true, false, t0);
        assert_eq!(fades.value_at("k", t0), 0.0, "and it starts there");
        let done = t0 + HOVER_FADE.duration();
        assert_eq!(fades.value_at("k", done), 1.0);
    }

    #[test]
    fn reversing_mid_fade_is_continuous_rather_than_a_jump() {
        // A pointer that leaves halfway must fade back from where the wash
        // actually got to. Re-anchoring at the endpoint instead would snap the
        // colour to full and then fade down, which reads as a flash.
        let mut fades = HoverFades::default();
        let t0 = Instant::now();
        fades.set_at("k", true, false, t0);
        let half = t0 + HOVER_FADE.duration() / 2;
        let mid = fades.value_at("k", half);
        assert!(mid > 0.0 && mid < 1.0, "mid-flight, got {mid}");

        fades.set_at("k", false, false, half);
        let after = fades.value_at("k", half);
        assert!(
            (after - mid).abs() < 1e-3,
            "the reversal starts from {mid}, not from an endpoint (got {after})"
        );
    }

    #[test]
    fn reduced_motion_keeps_the_state_and_drops_only_the_travel() {
        // Colour is the feedback; movement is the garnish. A reduce-motion user
        // still needs to know what the pointer is on.
        let mut fades = HoverFades::default();
        let t0 = Instant::now();
        fades.set_at("k", true, true, t0);
        assert_eq!(fades.value_at("k", t0), 1.0);
    }

    #[test]
    fn an_element_that_unmounts_mid_hover_does_not_leave_its_wash_behind() {
        // A result row unmounts on every keystroke and never receives its leave
        // event. Without pruning, the next element to take that key would paint
        // as hovered with nothing under the pointer.
        let mut fades = HoverFades::default();
        let t0 = Instant::now();
        fades.set_at("row", true, false, t0);
        let settled = t0 + HOVER_FADE.duration();

        // One frame where it still renders: read, then tick.
        fades.value_at("row", settled);
        fades.tick_at(settled + MIN_TICK_INTERVAL);
        assert_eq!(
            fades.value_at("row", settled),
            1.0,
            "still mounted, still hovered"
        );

        // A frame where it does not render at all.
        fades.tick_at(settled + MIN_TICK_INTERVAL * 2);
        fades.tick_at(settled + MIN_TICK_INTERVAL * 3);
        assert_eq!(
            fades.value_at("row", settled),
            0.0,
            "gone, and its wash with it"
        );
    }

    #[test]
    fn two_surfaces_ticking_in_one_frame_do_not_prune_each_other() {
        // One process-wide store, two windows rendering independently. Two
        // advances inside one frame would let each window prune the other's
        // live hovers, so the counter is rate-limited rather than each surface
        // being told about the others.
        let mut fades = HoverFades::default();
        let t0 = Instant::now();
        fades.set_at("panel-row", true, false, t0);
        fades.value_at("panel-row", t0);

        // Both surfaces tick within the same real frame.
        fades.tick_at(t0);
        fades.tick_at(t0);
        fades.tick_at(t0);

        assert_eq!(
            fades.value_at("panel-row", t0 + HOVER_FADE.duration()),
            1.0,
            "the panel's own live hover survived the other window's tick"
        );
    }

    #[test]
    fn a_settled_wash_stops_asking_for_frames() {
        // The tail requests another frame whenever this is true, so a wash that
        // never reported settled would hold the window at frame rate forever —
        // the exact failure `PulseClock` exists to avoid, arriving by a
        // different road.
        let mut fades = HoverFades::default();
        let t0 = Instant::now();
        fades.set_at("k", true, false, t0);
        fades.value_at("k", t0);
        assert!(
            fades.tick_at(t0 + MIN_TICK_INTERVAL),
            "mid-flight keeps frames coming"
        );

        let done = t0 + HOVER_FADE.duration() + MIN_TICK_INTERVAL;
        fades.value_at("k", done);
        assert!(!fades.tick_at(done), "settled asks for nothing");
    }

    #[test]
    fn a_wash_rising_out_of_transparent_never_passes_through_grey() {
        // The premultiplied blend is the whole reason `mix` is not a plain
        // component lerp: a straight mix interpolates the hidden channels of a
        // fully transparent colour too, so a light wash fading in over a dark
        // panel visibly darkens on its way up.
        let from = Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
        let to = Rgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.06,
        };
        let mid = mix(from, to, 0.5);
        assert!(
            (mid.r - 1.0).abs() < 1e-4,
            "hue is the target's throughout, got {}",
            mid.r
        );
        assert!(
            (mid.a - 0.03).abs() < 1e-4,
            "only the alpha travels, got {}",
            mid.a
        );
    }

    #[test]
    fn mix_returns_its_endpoints_exactly() {
        let a = Rgba {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 1.0,
        };
        let b = Rgba {
            r: 0.9,
            g: 0.8,
            b: 0.7,
            a: 1.0,
        };
        assert_eq!(mix(a, b, 0.0).r, a.r);
        assert_eq!(mix(a, b, 1.0).r, b.r);
        assert_eq!(mix(a, b, -5.0).r, a.r, "clamped");
        assert_eq!(mix(a, b, 5.0).r, b.r, "clamped");
    }

    fn assert_close(actual: f32, expected: f32, tol: f32, ctx: &str) {
        assert!(
            (actual - expected).abs() <= tol,
            "{ctx}: got {actual}, expected {expected} ±{tol}"
        );
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

// ---------------------------------------------------------------------
// The shared pulse clock
// ---------------------------------------------------------------------

/// Ticks 12.5 times a second (every 80ms): fast enough that the pulse reads
/// as a pulse, and roughly a tenth of the frame rate that caused the
/// incident described above.
pub const PULSE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(80);
/// A full breath in and out.
const PULSE_PERIOD_MS: f32 = 1600.0;

/// One repeating clock for the whole app, and the only sanctioned way to
/// drive a repeating animation here.
///
/// **This is the seam this module's own doc comment reserved.** The rule it
/// exists to enforce, from comet's recorded incident: a single
/// `with_animation(..).repeat()` element pinned a whole window at 120Hz and
/// measured 36% CPU. Nothing in this catalog may repeat on its own; anything
/// that needs to must read a phase from here instead.
///
/// Three properties make that safe, and all three are load-bearing:
///
/// 1. **It ticks at [`PULSE_INTERVAL`], not at frame rate.** A breathing dot
///    conveys "alive"; it does not need 120 steps a second to do it.
/// 2. **It stops completely when nothing is using it.** `set_running(false)`
///    ends the task, so the resting cost of this module is exactly zero —
///    the panel is hidden most of the time, and a clock that kept ticking
///    behind a hidden window would be the same defect in a slower disguise.
/// 3. **It never starts under reduce-motion.** `set_running` is a no-op then,
///    and [`PulseClock::intensity`] returns a fixed value, so a live row
///    still reads as live without moving.
pub struct PulseClock {
    elapsed_ms: f32,
    running: bool,
}

struct GlobalPulseClock(Entity<PulseClock>);

impl Global for GlobalPulseClock {}

impl PulseClock {
    /// The one clock. Created on first use, never more than one.
    pub fn global(cx: &mut App) -> Entity<PulseClock> {
        if !cx.has_global::<GlobalPulseClock>() {
            let clock = cx.new(|_| PulseClock {
                elapsed_ms: 0.0,
                running: false,
            });
            cx.set_global(GlobalPulseClock(clock));
        }
        cx.global::<GlobalPulseClock>().0.clone()
    }

    /// `0.0..=1.0`, a smooth breath. Callers interpolate whatever they like
    /// between two values with it rather than each inventing a waveform.
    ///
    /// Fixed at its midpoint while stopped or under reduce-motion, so a
    /// caller never has to branch: the dot is simply steady instead of
    /// breathing.
    pub fn intensity(&self) -> f32 {
        if !self.running {
            return 0.5;
        }
        let phase = (self.elapsed_ms % PULSE_PERIOD_MS) / PULSE_PERIOD_MS;
        // A cosine rather than a triangle: a linear ramp reverses with a
        // visible corner, which reads as a blink rather than a breath.
        0.5 - 0.5 * (phase * std::f32::consts::TAU).cos()
    }

    #[cfg(test)]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Starts or stops ticking. Idempotent — calling it every frame with the
    /// same value, which is exactly what a render pass will do, costs one
    /// comparison and spawns nothing.
    pub fn set_running(&mut self, running: bool, cx: &mut Context<Self>) {
        if running == self.running {
            return;
        }
        if running && system_reduce_motion() {
            return;
        }
        self.running = running;
        if !running {
            // The loop below sees this on its next tick and returns, so the
            // task ends on its own rather than needing to be cancelled.
            self.elapsed_ms = 0.0;
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PULSE_INTERVAL).await;
                let keep_going = this.update(cx, |clock, cx| {
                    if !clock.running {
                        return false;
                    }
                    clock.elapsed_ms += PULSE_INTERVAL.as_millis() as f32;
                    cx.notify();
                    true
                });
                match keep_going {
                    Ok(true) => {}
                    // Stopped, or the clock itself is gone with the app.
                    _ => break,
                }
            }
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod pulse_tests {
    use super::*;

    /// The clock's waveform is pure arithmetic on `elapsed_ms`, so it can be
    /// checked without a running task or a window.
    fn clock_at(elapsed_ms: f32, running: bool) -> PulseClock {
        PulseClock {
            elapsed_ms,
            running,
        }
    }

    #[test]
    fn a_stopped_clock_reports_a_fixed_midpoint_so_callers_never_branch_on_it() {
        assert_eq!(clock_at(0.0, false).intensity(), 0.5);
        assert_eq!(
            clock_at(12345.0, false).intensity(),
            0.5,
            "a stopped clock does not drift"
        );
    }

    #[test]
    fn intensity_stays_inside_the_unit_interval_across_a_dense_sweep() {
        for step in 0..2000 {
            let i = clock_at(step as f32 * 7.3, true).intensity();
            assert!(
                (0.0..=1.0).contains(&i),
                "intensity {i} out of range at step {step}"
            );
        }
    }

    #[test]
    fn the_waveform_breathes_rather_than_blinking() {
        // Starts dark, peaks at the half-period, returns — a corner here
        // would read as a blink, which is what the cosine is for.
        let start = clock_at(0.0, true).intensity();
        let peak = clock_at(PULSE_PERIOD_MS / 2.0, true).intensity();
        let end = clock_at(PULSE_PERIOD_MS, true).intensity();
        assert!(start < 0.01, "starts dark, got {start}");
        assert!(peak > 0.99, "peaks mid-period, got {peak}");
        assert!(end < 0.01, "returns, got {end}");
    }

    #[test]
    fn a_clock_reports_whether_it_is_running_so_the_stop_path_is_observable() {
        assert!(!clock_at(0.0, false).is_running());
        assert!(clock_at(0.0, true).is_running());
    }

    #[test]
    fn the_tick_rate_stays_far_below_frame_rate() {
        // The whole reason this clock exists: comet measured a repeating
        // element pinning a window at 120Hz for 36% CPU. A regression here
        // would be silent, so it is asserted rather than trusted to review.
        assert!(
            PULSE_INTERVAL.as_millis() >= 50,
            "a shared clock ticking faster than 20Hz defeats its own purpose"
        );
    }
}

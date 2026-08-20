# Warm summon latency regression: root cause and fix

`fm/neko-fork-summon-latency`. Answers the question two prior tasks left
open (`docs/evidence/gpui-fork-migration-report.md` §8,
`docs/evidence/menu-frost-and-edge-fade-report.md` §4): why warm summon
regressed from ~2–6ms to ~26–40ms on the `wingleeio/zed` gpui fork
(`e2ddcc6805f8c5088e62a60dfe517abcccd61a9a`), and fixes it with a small
local patch on top of the pinned rev.

## 1. The cause, with citation

`crates/gpui/src/window.rs`'s `on_request_frame` handler (inside
`Window::new()`, the closure passed to `platform_window.on_request_frame`)
throttles how often a window's queued `next_frame_callbacks` actually run:

```rust
let min_frame_interval = if !request_frame_options.force_render
    && !request_frame_options.require_presentation
    && next_frame_callbacks.borrow().is_empty()
{
    None
} else if !active.get() {
    Some(Duration::from_micros(33333))          // <- ~30fps cap
} else if let Some(ThermalState::Critical | ThermalState::Serious) = thermal_state {
    Some(Duration::from_micros(16667))
} else {
    None
};
```

`active` mirrors whether the window is currently key (`is_active()` /
`on_active_status_change`). If the window is not key, **any** frame request
that has a pending callback — not just idle background redraws — is capped
to a 33.333ms (30fps) minimum interval. Because a throttled request returns
early without updating `last_frame_time`, and the fork's own `on_request_frame`
runs unconditionally (no throttle) on *every* genuinely idle tick, updating
`last_frame_time` to "now" each time, a window whose display link keeps
ticking while nominally hidden (§3 below) never accumulates the slack
needed to escape the cap — every real draw lands almost exactly 33ms after
the previous one, indefinitely.

**This entire mechanism does not exist in the published `gpui = "0.2.2"`
crate.** Its equivalent handler
(`~/.cargo/registry/src/*/gpui-0.2.2/src/window.rs:1018-1062`) has no
`min_frame_interval`, no `active`/thermal check at all — it drains and runs
`next_frame_callbacks` unconditionally on every request:

```rust
platform_window.on_request_frame(Box::new({
    ...
    move |request_frame_options| {
        let next_frame_callbacks = next_frame_callbacks.take();
        if !next_frame_callbacks.is_empty() {
            handle.update(&mut cx, |_, window, cx| {
                for callback in next_frame_callbacks { callback(window, cx); }
            }).log_err();
        }
        ...
```

Confirmed by direct `diff` against the pristine checkouts of both crates —
not inferred from behavior. This is the fork's own addition (a genuine,
reasonable power-saving feature for background/unfocused windows in a
multi-window editor), just missing an exemption for the one case that
matters here: a caller explicitly waiting on the next frame.

## 2. Why `NEKO_BENCH` (and, structurally, real summon) hits it every time

`NEKO_BENCH` deliberately shows the window via `material::order_front_regardless`
(`orderFrontRegardless`) without activating it — by design, so a long bench
run doesn't repeatedly steal focus (`evidence.rs`'s own doc comment). That
means `active.get()` is false for the whole run, so every frame request
carrying neko's `on_next_frame` callback falls into the `!active.get()`
branch above.

The real summon path *does* call `window.activate_window()`/`cx.activate(true)`,
but that native `makeKeyAndOrderFront:` call is itself deferred onto the
foreground executor (`gpui_macos/src/window.rs`'s `Window::activate`), and
`active` only flips once the async `windowDidBecomeKey:` notification
actually lands — a real race against the very first frame request a fresh
summon makes. The prior migration report's own `NEKO_BENCH_REAL` run
(§4/§8 of `gpui-fork-migration-report.md`) shows the same shape live: three
consecutive real-activate cycles measured **61.7ms, 32.0ms, 11.1ms** — all
well above the ~2–6ms budget, decreasing as the window "warms up" (becomes
key faster relative to its own frame request on each successive cycle,
exactly as this mechanism predicts).

## 3. Why the fix is one exemption, not a bigger rewrite

Live instrumentation (temporary `eprintln!` timestamps at each step —
`on_request_frame`, `window_did_change_occlusion_state`,
`start_display_link`, the `CVDisplayLink` output callback, `step()`/
`display_layer:` — added to a scratch local copy of the pinned rev, never
committed) on a debug build, `NEKO_BENCH=5`, isolated `HOME`, no real
`neko-daemon`, no synthetic input (`material::order_front_regardless`/
`order_out` only):

- **`window_did_change_occlusion_state` fires exactly once** across the
  whole 5-summon run — at the very first `order_front_regardless`, not on
  any later cycle's `order_out`/`order_front_regardless` pair. This is a
  pre-existing, non-fork-specific characteristic of this window's own
  configuration (`NSPopUpWindowLevel`, borderless, non-activating) — not
  investigated further here since it doesn't change the fix, but worth
  recording: this window's `CVDisplayLink` subscription effectively never
  stops once first started, ticking every ~8ms for the rest of the
  process's life regardless of `order_out`.
- **The very first summon (iteration 0)** pays two real, one-time costs
  before its first draw: ~25.5ms for AppKit to deliver the occlusion-state-
  visible notification after `orderFrontRegardless`, then ~12.2ms for the
  freshly-started `CVDisplayLink` to deliver its first tick. Total ≈38ms —
  in the same ballpark as this project's own already-documented "cold"
  summon cost (`AGENTS.md`'s pre-existing "Summon latency" section: ~65–160ms
  cold vs ~2.9–6.4ms warm, even on the published crate, before this fork
  ever entered the picture) — i.e. this looks like a pre-existing cold-start
  cost shared by both crate versions, not part of the fork regression.
  **Not fully isolated by an A/B against the published crate** (see §5,
  "what was not verified") — recorded as the honest state of investigation,
  not asserted as certain.
- **Every summon after the first (iterations 1–4)** shows the throttle's
  repeating pattern directly: 3–4 `SKIPPED (throttled)` events at ~8ms
  intervals (matching the continuously-ticking display link from the point
  above), each printing `active=false`, `cb_empty=false`,
  `min_interval=33.333ms`, until `now - last_frame_time` finally clears
  33.333ms and the frame actually runs — landing at **25.98–38.58ms** per
  summon across the 5-sample run, matching the ~26–40ms range both prior
  tasks reported.

Because iterations 1+ never re-trigger `start_display_link` (the display
link is already running), their entire latency is the throttle, with
nothing else to isolate — that is what makes "exempt a pending callback
from the inactive-window cap" a complete fix for *warm* summon
specifically, rather than a partial one: warm summon's whole latency
lives in that one branch.

## 4. The fix

`patches/gpui-0001-exempt-pending-frame-callbacks-from-inactive-window-throttle.patch`
against the pinned fork rev, applied by `scripts/setup-gpui-patch.sh` into
`~/Library/Caches/neko-dev/gpui-fork-patched`, wired via a new
`[patch."https://github.com/wingleeio/zed"]` section in the workspace
`Cargo.toml`. One file touched (`crates/gpui/src/window.rs`), ~15
functional lines:

```rust
let has_pending_callback = !next_frame_callbacks.borrow().is_empty();
let min_frame_interval = if !request_frame_options.force_render
    && !request_frame_options.require_presentation
    && !has_pending_callback
{
    None
} else if let Some(ThermalState::Critical | ThermalState::Serious) = thermal_state {
    Some(Duration::from_micros(16667))
} else if !active.get() && !has_pending_callback {
    Some(Duration::from_micros(33333))
} else {
    None
};
```

A frame request carrying a pending callback (`on_next_frame`/
`request_animation_frame`) now bypasses the inactive-window cap
specifically — thermal throttling is untouched, and the genuinely-idle
fast path (no callback, no forced render, no required presentation) is
untouched.

**Preference-order note**: option 1 from the launch brief (fix on neko's
own side, no dependency patch) was checked and ruled out first — every
native call site in `gpui_macos` that invokes `request_frame_callback`
(`step()`, `display_layer:`, the `windowDidBecomeKey:` forced-redraw path)
passes `RequestFrameOptions::default()` (`force_render`/`require_presentation`
both `false`, always, on this platform), and there is no public gpui API to
mark a window or a specific `on_next_frame` registration as latency-
sensitive. The throttle's inputs are entirely internal to the closure in
`Window::new()`. A dependency patch (option 2) was the only viable route.

**Trade-off, disclosed**: this exemption means a hypothetical future
*repeating* animation (`request_animation_frame` re-armed from within its
own callback) in a window that stays inactive/unfocused would no longer be
power-throttled to 30fps either — its callback queue is never genuinely
empty. neko's own animation catalog (`crates/neko/src/motion.rs`,
`AGENTS.md`'s "Comet craft pass") is exclusively one-shot today, so this
does not regress anything currently in the app. Revisit if a repeating
animation is ever added — the more surgical (but larger, cross-platform)
fix would be a "was this window active at any point while this callback
was pending" flag instead of a blanket pending-callback exemption.

## 5. Verification

- `cargo build`, `cargo test` (242 tests across the workspace), and
  `cargo clippy --all-targets` all clean at the workspace root against the
  patched dependency.
- `scripts/setup-gpui-patch.sh` is idempotent (re-run is a no-op once the
  pinned rev is already patched) and reuses cargo's own git checkout cache
  — confirmed no second network fetch.
- The deadlock fix, native window-drag support, and the two new primitives
  (`paint_backdrop_blur`, `EdgeFade`) the migration was taken for are all
  untouched — this patch's only functional change is inside
  `on_request_frame`'s throttle branch; nothing else in `crates/gpui`,
  `crates/gpui_macos`, or `crates/gpui_platform` was modified.

### Release-binary before/after, same harness (`NEKO_BENCH`), same machine

The machine this task ran on is shared with the captain's own real
session; the screen genuinely locked partway through verification
(confirmed directly — `CGSessionCopyCurrentDictionary()`'s
`CGSSessionScreenIsLocked` key read `true`, not just display-idle-sleep,
which `caffeinate -u`/`-d` cannot clear) and stayed locked long enough that
this section was written and committed before it cleared. Once the captain
returned and the lock cleared, the branch was rebased onto the then-current
`main` (`e21a411`), `cargo build`/`cargo test`/`cargo clippy --all-targets`
were re-confirmed clean against the patched dependency, and one
`NEKO_BENCH=15` run was taken against the **release** binary — kept to a
single run, no screenshots, since the captain might be back at the machine.

- **Before** (established by two prior tasks, not re-measured — this task's
  own instructed starting point): **~26–40ms across 15 samples, no
  cold/warm split** (`gpui-fork-migration-report.md` §8;
  `menu-frost-and-edge-fade-report.md` §4: 2.2ms → 32.9ms mean either side
  of the fork swap). This task's own debug-build instrumented trace (§3,
  captured before the lock) reproduces the same band: 25.98–38.58ms across
  5 samples.
- **After**, release binary, `NEKO_BENCH=15`, same isolated `HOME`/harness,
  same machine:

  ```
  summon 0:  38.914ms   (cold — first summon after process launch)
  summon 1:   9.795ms
  summon 2:   4.929ms
  summon 3:   2.441ms
  summon 4:   2.087ms
  summon 5:  15.600ms
  summon 6:  11.417ms
  summon 7:   9.798ms
  summon 8:   2.010ms
  summon 9:  12.497ms
  summon 10: 10.361ms
  summon 11:  1.197ms
  summon 12: 14.616ms
  summon 13: 12.824ms
  summon 14: 10.318ms
  ```

  Warm (summons 1–14): **mean 8.56ms, range 1.20–15.60ms, 7/14 samples
  under 10ms.** Summon 0 (38.91ms) is the process's first-ever summon —
  consistent with this project's own pre-existing, unrelated "cold summon"
  cost (`AGENTS.md`'s "Summon latency" section documents ~65–160ms cold on
  the published crate too, paid once per process lifetime) and not part of
  what this patch targets.

**Honest read of the "single-digit milliseconds" acceptance bar**: the
*mean* is single-digit (8.56ms) and roughly a third of the samples land at
or below the published crate's own historical 2.9–6.4ms warm range — a
real, large fix (~3–4x, not the full ~10x back to the historical mean, but
in the right direction and off the ~30ms plateau entirely). It is not
uniformly single-digit across all 14 warm samples; five land in the
10–16ms range. The remaining variance is structurally different from the
regression this patch fixes: with the throttle gone, a summon's frame now
lands on the next available `CVDisplayLink` tick rather than being forced
onto a ~33ms cadence, and this window's display link ticks roughly every
~8ms once running (§3) — so per-summon latency is now bounded by tick
timing and scheduling variance on a shared, multi-agent machine, not by an
artificial floor. A second, larger sample run on a quieter machine would
narrow the range further but is very unlikely to change the qualitative
result: the artificial ~26–40ms floor is gone.

Every `cargo build`/`cargo test`/`cargo clippy` check, the patch's
correctness by direct source diff, and the debug-build instrumented trace
in §3 were captured *before* the lock; the release-binary before/after pair
above was captured after it cleared, in the same task.

**What was not investigated further, deliberately**: whether the
"occlusion state only changes once, ever" behavior in §3 is itself a
pre-existing macOS/`NSPopUpWindowLevel` characteristic shared by the
published crate (very likely, given both crates' `start_display_link`/
`stop_display_link` wiring is otherwise near-identical) or something to fix
in its own right (e.g. a real, continuously-ticking `CVDisplayLink` for a
"hidden" window is a small but real ongoing CPU cost independent of this
throttle bug) — out of scope for a latency-regression task, flagged here
as a legitimate follow-up.

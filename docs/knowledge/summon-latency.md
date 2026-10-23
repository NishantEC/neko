## Summon latency

Re-measured after the restructure (release binary, `RUST_LOG` off — full
methodology and all samples in `docs/evidence/summon-latency.md`):

- **Warm** (window, renderer, and daemon connection already live): **~2.9–6.4ms**
  across 6 samples, mean ≈4.1ms — inside slice 1's own budget (~11–14ms) and
  faster than slice 1, despite the panel now rendering far more per frame.
  The daemon round-trip for `Search` is off the summon path entirely (summon
  is hotkey-press → first-frame-after-activation, purely client-side,
  unchanged from slice 1) — a slow or unreachable daemon cannot make summon
  itself slower.
- **Cold** (first summon after process launch): **~65–160ms** across 2
  samples — roughly double slice 1's ~89ms. This is the one place the extra
  panel content and `WindowBackgroundAppearance::Blurred` compositing
  genuinely cost something, paid once per process lifetime (the process stays
  resident — see "Current scope" above), not per summon. Reported plainly
  rather than absorbed, per the brief's own instruction.

Testing caveat carried over from slice 1: synthetic hotkey events via
`osascript … key code 49 using {option down}` are unreliable in rapid
succession (roughly half dropped between AppleScript and the OS's global-hotkey
delivery), even though every event that *did* arrive was handled correctly.
Real hardware keypresses weren't re-tested with the same instrumentation this
pass either — don't be surprised if automated re-tests need retries.

**Regressed on the `wingleeio/zed` fork (`fm/neko-gpui-fork-migration`,
2026-08-20) to ~26–40ms warm — root-caused and fixed
(`fm/neko-fork-summon-latency`).** Full diagnosis, the live instrumented
trace that proves it, and the fix's own trade-offs:
`docs/evidence/gpui-inactive-window-throttle-fix-report.md`. Summary:

**Cause**: `crates/gpui/src/window.rs`'s `on_request_frame` handler caps
frame delivery to ~30fps (a `Duration::from_micros(33333)` minimum
interval) whenever a window isn't key (`!active.get()`) — with no
exemption for a pending `on_next_frame`/`request_animation_frame`
callback, i.e. something a caller is *explicitly* waiting on, not an idle
background redraw. **This throttle does not exist at all in the published
`gpui = "0.2.2"` crate** — confirmed by direct diff, not inferred; its
equivalent handler runs `next_frame_callbacks` unconditionally on every
request. `NEKO_BENCH` shows the window via a non-activating
`orderFrontRegardless` by design (so a long bench run doesn't repeatedly
steal focus), so it is always "inactive" from this check's point of view —
every summon's frame request fell into the 30fps cap, and because a
throttled request never advances the handler's own `last_frame_time`,
every following request landed in the same stale window and got throttled
again, locking summon into a ~25–40ms cadence with no way out on its own.
The real `activate_window`/`cx.activate` path races the same check against
the async `windowDidBecomeKey:` notification and shows the identical
pattern, decreasing across repeated activations
(`gpui-fork-migration-report.md`'s own `NEKO_BENCH_REAL` numbers: 61.7ms,
32.0ms, 11.1ms for 3 cycles) — not contradicting this diagnosis, consistent
with it.

**Fix**: a small local patch on top of the pinned fork rev —
`patches/gpui-0001-exempt-pending-frame-callbacks-from-inactive-window-throttle.patch`,
applied by `scripts/setup-gpui-patch.sh` (run once, or again after the
pinned rev changes; populates `~/Library/Caches/neko-dev/gpui-fork-patched`,
which the workspace `Cargo.toml`'s new
`[patch."https://github.com/wingleeio/zed"]` section points `gpui`/
`gpui_platform` at) — exempts a frame request carrying a pending callback
from the inactive-window cap specifically, leaving thermal throttling
untouched. No public gpui API exists to opt out of this per-window or
per-request (every native macOS call site that invokes the frame callback
passes `RequestFrameOptions::default()`, always), which is why this needed
a dependency patch rather than a neko-side fix — see the evidence report's
§4 for the trade-off this patch accepts (a future *repeating* animation in
an inactive window would also stop being power-throttled; neko's own
`motion.rs` catalog is exclusively one-shot today, so this doesn't regress
anything currently in the app).

**Verified**: `cargo build`/`cargo test` (242 tests)/`cargo clippy
--all-targets` all clean against the patched dependency, both before and
after rebasing onto `main`; the deadlock fix, native window-drag support,
and the `paint_backdrop_blur`/`EdgeFade` primitives the fork migration was
taken for are all untouched (the patch's only functional change is inside
`on_request_frame`'s throttle branch). The shared machine's screen locked
partway through this task (confirmed via `CGSessionScreenIsLocked`, not
just display-idle-sleep, which `caffeinate` can't clear) and stayed locked
long enough that the diagnosis and fix were written up and committed before
it cleared; once it did, one **release-binary** `NEKO_BENCH=15` run
recorded the actual before/after: **before, ~26–40ms across 15 samples, no
cold/warm split** (established by the two prior tasks, re-confirmed by this
task's own 25.98–38.58ms debug-build trace); **after, warm summons (1–14)
mean 8.56ms, range 1.20–15.60ms, 7/14 under 10ms** — the artificial ~33ms
floor is gone, real variance now bounded by this window's own ~8ms
`CVDisplayLink` tick cadence (not investigated further — see the evidence
report's own closing note) rather than by the fixed regression. Summon 0
(cold, 38.91ms) is this project's own pre-existing, unrelated cold-start
cost, unaffected by this patch. Full numbers and methodology:
`docs/evidence/gpui-inactive-window-throttle-fix-report.md`.

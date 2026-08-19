# The mode-view resize seam — reproduced, root-caused, fixed

`fm/neko-mode-resize`, `main` at `b790796`. Two prior tasks
(`docs/evidence/mode-view-and-neutral-palette-report.md`) tried and failed to
reproduce the captain's screenshot; this task reproduced it on the first
attempt at the sequence they hadn't tried, root-caused it to a real `gpui`
internal staleness (not a neko-side geometry bug), and fixed it by removing
the operation that triggered it rather than working around it.

All verification below ran on the real `target/release` binaries under an
isolated `HOME` (`/tmp/neko-mode-resize-home`), never the captain's own
daemon (pid 21793) or client. Clipboard fixtures were seeded directly into
the isolated SQLite DB, never the real pasteboard. Every capture is
`screencapture -l<windowID>` (window-scoped). An obscure hotkey
(⌘⌥⌃⇧F13) was committed to the isolated daemon before launching the isolated
client.

## Reproducing it — the sequence that mattered

The prior task's own `confirm_for_evidence` repro entered the mode
*immediately* after the window's first-ever appearance — the one case that
reliably works. The captain's real sequence is different: the window is
summoned and dismissed some number of times first, then, from a *later*
summon, a mode is entered. `evidence.rs` gained `NEKO_REAL_CYCLES_BEFORE_SHOW=<n>`
to model exactly that — `n` real order-front/order-out paint cycles
(`material::order_front_regardless`/`order_out`, chosen over the real
`activate_window`/`cx.activate` path specifically to dodge the *other*,
already-documented `windowDidBecomeKey:` deadlock in "A known,
upstream-fixed-but-unreleased deadlock in the real summon path" — hitting
that one live, mid-investigation, is what ruled it out) — followed by one
real final summon (`activate_window`/`cx.activate(true)`, kept key/visible
through query + `Enter`), matching "he summons it, types, and presses Enter
while looking at it."

Two real order-front/order-out cycles, then one real summon, one query
("clipboard", matching the `CommandsProvider`'s "Clipboard History" row —
seeded with a fixture deliberately missing the letter `c` so the raw
clipboard entry itself can't also match and steal the top slot), then
`Enter`: the seam reproduced on the very first run, and every run after.

**Measured, before the fix** (`mode-resize-seam-before.png`):
panel border x≈68–1588 (width 1520px @2x = 760pt, matching `window.bounds()`);
drawn content stops dead at x≈1428 (1428−68 = 1360px = 680pt).
1360/1520 = **0.895** — matching the captain's own screenshot ratio
(1360/1515 = 0.898) almost exactly, and exactly `PANEL_WIDTH_PX /
PANEL_WIDTH_WITH_DETAIL_PX`. Text (the preview paragraph, the `Application`/
`Content Type`/`Copied` values, the footer's `Actions ⌘K`) was clipped at
the identical boundary, not just the backdrop — matching every symptom the
captain's report described.

## Root cause — confirmed by direct readback, not inferred

The previous task's own measurements (window bounds, background-view
`CGRect`, drawable size) were correct and are not contradicted here — they
just weren't the whole picture. Direct native readback immediately after
the same `setContentSize`/`setFrameTopLeftPoint` resize this task started
from confirmed, every time: `contentView`'s frame, GPUI's own rendering
`NSView`'s frame, its backing `CAMetalLayer`'s `bounds`, and the Metal
drawable's own `drawableSize` **all correctly updated to the new (wider)
size**. What did not update: `gpui::Window::viewport_size()` (public,
read-only) — the size `Window::draw_roots` (`gpui-0.2.2/src/window.rs`)
passes as the root element's own available space every frame, and (per
direct testing) also what governs how much of that frame's scene actually
gets painted to screen, independent of what width any individual `div`
declares. `viewport_size` is a private field, updated only from
`Window::bounds_changed`, itself only reachable via the platform layer's
`on_resize`/`on_moved`/`on_active_status_change` native callbacks — all
private wiring, no public hook to force a resync.

**Four independent workarounds were tried and all failed to force that
resync**, confirmed via `window.viewport_size()` reads taken well after
(seconds, many real frames) each attempt, not just synchronously:

1. `window.refresh()` (`gpui`'s own public "mark dirty, redraw next frame") —
   fixed the *drawable*/background paint (no more transparent gap — the
   `Root` panel `div`'s own explicit width already paints correctly
   regardless of `viewport_size`) but not the clipping — text/right-aligned
   values still cut at the old boundary.
2. `window.resize()` (`gpui`'s own public async resize, the same
   `setContentSize:` call scheduled a tick later) — no change.
3. A direct `-[NSView setFrameSize:]` "nudge" (target size ±1pt, then the
   real target, guaranteeing a genuine value change so `set_frame_size`'s
   own early-return-on-same-size guard couldn't be the reason) on GPUI's
   real rendering view, using the same raw-window-handle plumbing
   `material.rs`/`display_placement.rs` already use — no change.
4. `-[NSView setNeedsLayout:]` + `-layoutSubtreeIfNeeded` +
   `-displayIfNeeded` forcing any deferred AppKit layout pass to run
   synchronously before the nudge above — no change.
5. A second, fully independent resize (exit the mode, re-enter it) from a
   *separate* top-level closure, not nested in the original call at all —
   also no change; `viewport_size` stayed at the stale `680` through all
   three resizes in that run.

No error was ever logged by `gpui`'s own `.log_err()`-guarded callback
paths, and this reliably reproduced only after the window had already gone
through order-front/order-out cycling — a fresh, never-cycled window's
first-ever mode-entry resize (the prior task's own repro shape) reliably
works. The exact reason the native resize/move callbacks stop firing after
that cycling could not be pinned down further without instrumenting `gpui`
itself, which is out of scope (published-crate pin, see "The GPUI
dependency decision") — but the practical conclusion is solid, repeated
five separate ways: **a runtime `NSWindow` resize, once a window has been
shown/hidden a few times, cannot be relied on to keep `gpui`'s own paint
viewport in sync, and no public API forces the resync.**

## The fix — never resize the real window for a mode transition

Per the task brief's own candidate #3, applied precisely rather than as a
blunt fallback: the real `NSWindow` is now created once at
`PANEL_WIDTH_WITH_DETAIL_PX` (760) and **never resized again for the rest
of the process** — `display_placement::resize_and_recenter` is deleted
outright, not just unused. A mode transition instead:

- `panel::Root::render`'s own stage element is `w(PANEL_WIDTH_WITH_DETAIL_PX)
  .flex().justify_center()`, always — the narrower root-list `div` centers
  inside it via ordinary flexbox, the same "one query, two result types,
  same list" `Render::render` already builds every frame regardless of
  `viewport_size`'s own tracking (never subject to the staleness above,
  since the window's real size genuinely never changes any more).
- `material::set_background_frame` (new) sets the installed background
  view's own frame directly — `x = (760 − new_width) / 2`, `width =
  new_width`, `height` unchanged — a plain, synchronous `-[NSView
  setFrame:]` call, no window resize, no dependency on the broken
  callback chain at all.
- `panel::Root::update_background_bounds` (replacing `resize_panel`) calls
  the above from `enter_mode`/`exit_mode`/`reset_for_summon`, with the
  identical centering formula `Render::render`'s `justify_center()`
  produces — the two are proven to agree algebraically (both center a
  `new_width`-wide box inside a fixed 760-wide one), not just by
  eyeballing a screenshot.
- `main.rs` narrows the background to `PANEL_WIDTH_PX` once, right after
  `material::install`, since the window now opens wide by default and the
  resting state is the narrow root list.

**The cost, measured, not assumed**: the real `NSWindow`'s own footprint is
now 760pt wide at rest instead of 680pt — an internal fact, not a visible
one. `upper_third_offset`'s centering, now computed against the window's
own fixed 760pt size, places the window's left edge 40pt further left than
before; centering the 680pt root-list panel *inside* that window (the same
40pt each side `PANEL_ROOT_INSET_PX` names) lands its own visible left edge
back at exactly `(display_width − 680) / 2` — algebraically identical to
where the old 680pt-wide window's own left edge sat. Verified live, not
just by the algebra: `mode-resize-seam-rootlist-after.png` — panel border
x≈192–1552 (680pt), window border x≈112–1630 (760pt), left inset 192−112 =
80px = 40pt each side, matching `PANEL_ROOT_INSET_PX` exactly. The
40pt margin outside the visible panel carries no material and no shadow of
its own (macOS's native window shadow follows the actual opaque/painted
pixels for a transparent-backed window, not the raw window rect — visible
directly in the same screenshot, the soft shadow hugs the panel's own edge,
not the wider window's) — there is no visible seam, halo, or shape change
in the root list's resting state.

## Verified after the fix

- **The measured drawn width, mode open**: panel border x≈112–1630 (1518px
  @2x ≈ 759–760pt), drawn content extends to the same right edge — no
  seam, matching the window's own width exactly (`mode-resize-seam-after.png`).
- **Preview text, all three info rows, and the footer**: all fully visible,
  no clipping — `Application: IsolatedFixtureApp`, `Content Type: Text`,
  `Copied: 59m`, footer `Clipboard History | Paste ↵ | Actions ⌘K`, all
  complete (contrast the identical fixture in `mode-resize-seam-before.png`,
  where none of the three info rows even had a visible value).
- **Exit and re-enter the mode**: `mode-resize-seam-after-cycle.png` —
  `NEKO_CYCLE_MODE_ONCE` (new evidence hook) exits the mode
  (`Root::dismiss_for_evidence`, the same `Escape` path) and re-enters it
  once more before capture; identical, correct rendering, no residue. The
  fix has no native-callback dependency to degrade across cycles in the
  first place, unlike the mechanism it replaced.
- **The root list's resting state**: `mode-resize-seam-rootlist-after.png`
  — unchanged in size, shape, and on-screen position from before this task,
  per the inset math above.
- `cargo build`, `cargo test --workspace` (209 tests, all crates), and
  `cargo clippy --workspace --all-targets` — all clean.
- `NEKO_BENCH=8` warm summon latency, same isolated `HOME`: 0.98–9.4ms
  across 7 warm samples (one cold-start sample at 40.6ms) — consistent
  with the previously documented ~2.9–6.4ms warm range; nothing on the
  summon path changed (this fix touches mode transitions only, which fire
  well after summon completes). Daemon idle memory was not touched by
  this change (no daemon-side code in this diff) and was not re-measured.

## What this did not verify

Two real displays (this sandbox has one physical display, same limitation
every other multi-display verification in this repo already discloses);
the captain's own exact machine/session (this ran in an isolated sandbox,
per the task's own constraints) — though the reproduction sequence and
measured ratio match his report closely enough that this is treated as the
same bug, not a coincidentally similar one.

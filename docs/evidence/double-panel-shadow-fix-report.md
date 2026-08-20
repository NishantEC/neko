# The double-panel defect — root-caused to AppKit's own window shadow, not a second painted surface

`fm/neko-double-panel`, `main` at `235bf88`. The captain reported the root
list (not the mode view) drawing two concentric rounded rectangles: an
outer surface at ~1515px wide, an inner one holding the real content at
~1355px wide — ratio 0.894, matching `PANEL_WIDTH_PX / PANEL_WIDTH_WITH_DETAIL_PX`
(680/760 = 0.895) almost exactly.

## Investigation — ruling candidates out with live readbacks, not inference

Four candidates were named in the launch brief. Each was checked against a
live, isolated instance (never the captain's own daemon/client — see
"Verification setup" below) rather than assumed from source alone:

1. **The native backdrop material view** (`NSGlassEffectView`/
   `NSVisualEffectView`, installed by `material::install`, narrowed by
   `material::set_background_frame`). A temporary readback
   (`contentView.subviews()[0].frame()`, the same technique
   `verify_installed` already uses) taken immediately after the startup
   narrowing call, again after `reset_for_summon`, and again at rest after
   the window's first real `activate_window()`/`cx.activate(true)` all
   showed the identical, correct frame: `origin.x=40, width=680,
   height=420` inside a `760×421` `contentView`. **This was never the
   stray surface** — the narrowing code does exactly what its own doc
   comments claim, at every point checked.
2. **The stage element** (`Root::render`'s outer `div`). Read directly:
   `w(theme::PANEL_WIDTH_WITH_DETAIL_PX).flex()`, no `.bg()`, no
   `.border_1()`, no `.rounded()` — confirmed clean by inspection, and its
   absence from the rendered pixels was separately confirmed by the pixel
   probe below (nothing paints in that region once the real cause is
   fixed).
3. **The margin divs** (`render_dismiss_margin`, added by `235bf88`).
   `div().w(px(width)).h_full().flex_shrink_0().on_mouse_down(...)` — no
   `.bg()` call, so nothing paints there either. Confirmed clean by
   inspection; `235bf88`'s own diff touches only `panel.rs`'s render tree,
   never `material.rs` or `main.rs`, so it could not have changed what the
   native backdrop or window itself does.
4. **`main.rs`'s startup narrowing call** (~line 188). Runs, with the
   correct arguments, confirmed by the same readback as (1) above.

None of the four is the stray surface. A live window-scoped screenshot
(`double-panel-before.png`) reproduced the defect on the very first fresh
summon of an isolated instance — no repeated cycling needed to see it,
unlike the earlier mode-resize-seam bug. A pixel probe of that screenshot's
alpha channel (not just eyeballing it) showed the "outer surface" is not a
solid fill at all: alpha rises to only ~15–44 (of 255) near the window's
own edge, then drops, then rises again approaching the real 680pt panel
edge where it reaches 255. Two distinct, overlapping soft-edged regions,
not one hard-edged second panel — consistent with two independent shadow
sources rather than a second painted surface.

## Root cause — AppKt's own automatic window shadow, confirmed by a single-variable test

`crates/neko/src/main.rs` opens this window with `window_background:
material::window_background()`, which is `WindowBackgroundAppearance::Transparent`.
`gpui-0.2.2`'s own `set_background_appearance` (`platform/mac/window.rs`)
implements "transparent" as a *near-invisible but non-zero* alpha
background color (`rgba(0,0,0,0.0001)`) with an explicit comment: *"Not
using `+[NSColor clearColor]` to avoid broken shadow."* Combined with the
window's entire visible surface being drawn by one Metal-layer-backed
`NSView` (GPUI's own rendering view) — which AppKit cannot introspect
pixel-by-pixel to shape an automatic shadow around the actually-painted
content — the practical effect is that AppKit's automatic window drop
shadow is computed against the **whole `NSWindow` frame rectangle**,
unconditionally, with the window's own corner radius applied to it (which
is why it reads as "rounded" rather than a plain rectangular blur).

Before the mode-resize-seam fix (`82261a0`), the real window's own frame
always matched whatever width the visible panel was — the window itself
resized on every mode transition — so the frame-shaped shadow and the
panel's real edge were the same edge by construction, and the defect was
invisible. `82261a0` made the window a permanent
`theme::PANEL_WIDTH_WITH_DETAIL_PX` (760px) regardless of the narrower
panel actually drawn inside it for the root list — from that commit
onward, AppKit's own shadow started extending `theme::PANEL_ROOT_INSET_PX`
(40pt) past the real 680pt panel on both sides, at rest. `235bf88`
(the commit the launch brief names) didn't introduce this — its own diff
never touches `material.rs`/`main.rs` — it just landed after the real
regression and was the most recent commit when the captain noticed.

**Confirmed with a single-variable live test**, not just this
explanation: calling `NSWindow.setHasShadow(false)` — nothing else changed
— on the exact same reproduction removed the outer surface completely
(`double-panel-after-rootlist.png`). This is the fix.

## The fix

`material::disable_native_shadow` (`crates/neko/src/material.rs`) calls
`NSWindow.setHasShadow(false)` once, right after `material::install`, in
`main.rs`'s window-creation closure — the same place the startup background
narrowing already runs. `material::verify_shadow_disabled` reads
`NSWindow.hasShadow()` back immediately after and logs the result to
stderr, the same "verified, not trusted" pattern `verify_installed`/
`spaces::verify` already establish in this codebase; `main.rs` calls it
unconditionally on every real launch, not just in an evidence run.

The panel `div`'s own `.shadow_lg()` (`panel.rs`, unchanged by this task)
is unaffected — it's GPUI's own explicit box-shadow, painted as real pixels
in the same Metal frame as everything else, already correctly sized to
whatever width the panel actually is in every mode. It was always the only
shadow this app needed; AppKit's automatic one was redundant even when it
happened to be correctly shaped (before `82261a0`) and actively wrong once
the window and the visible panel's width could diverge.

**Nothing about the fixed-width-window architecture changed.** The real
`NSWindow` is still created once at `PANEL_WIDTH_WITH_DETAIL_PX` and never
resized at runtime; `Root::render`'s stage element still centers the
narrower root-list panel inside it via the explicit margin divs; the
native backdrop is still narrowed the same way via
`material::set_background_frame`. The only change is telling AppKit not to
draw its own shadow around a frame that no longer matches the visible
content.

## Measured, before and after

All measurements from window-scoped screenshots (`screencapture
-l<windowID>`), pixel-probed programmatically (alpha channel per-pixel),
not eyeballed. Display backing scale on the verification machine: 2.0x
(confirmed: a `760×421`pt window captures as an exactly `1520×842`px PNG
once the OS shadow-blur padding is gone).

**Before** (`double-panel-before.png`, fresh isolated instance, first
summon): outer surface (low-alpha region hugging AppKit's own window-frame
shadow) extends to the full captured canvas including shadow-blur bleed
(`1656×978`px, i.e. the OS added ~68px of blur padding on each side beyond
the window's own `1520×842`px frame); inner opaque panel spans x≈160–1500px
(2x) — 680pt, matching `PANEL_WIDTH_PX`.

**After** (`double-panel-after-rootlist.png`, same isolated instance,
identical build otherwise): captured canvas is exactly `1520×842`px — the
window's own frame, zero shadow-blur bleed beyond it, because
`setHasShadow(false)` removed the shadow that was causing that bleed.
Opaque panel: x=80 to x=1439 inclusive (1360px = 680pt), left/right margin
80px = 40pt each side, matching `theme::PANEL_ROOT_INSET_PX` exactly. One
panel, no second border, no halo.

**Stable across repeated summon/dismiss cycling**
(`double-panel-after-4-cycles.png`, `NEKO_REAL_CYCLES_BEFORE_SHOW=4`): same
measurement, byte-for-byte-equivalent panel bounds, after four real
order-front/order-out cycles before the captured summon — the captain's
own real usage pattern that turned up the mode-resize-seam bug is checked
here too, and finds no residue.

**Mode view still renders complete after real cycling**
(`double-panel-after-modeview-cycled.png`): `NEKO_REAL_CYCLES_BEFORE_SHOW=3`
(three real summon/dismiss cycles) → query "clipboard history" → confirm →
`NEKO_CYCLE_MODE_ONCE=1` (exit the mode, re-enter it once more) → capture.
Full preview text, all three Information rows (Application: Terminal,
Content Type: Text, Copied: 6m) populated, footer's "Actions ⌘K" visible.
Panel spans the entire `1520×842`px window (no margin in detail mode, as
designed) — no shadow artifact possible here since panel width == window
width, but confirmed anyway.

**Warm summon latency**: `NEKO_BENCH=8` on the same isolated instance,
release binary: 0.8–24.9ms across 8 samples (first sample includes normal
one-time JIT/cache warmup, matching every prior bench run's own shape) —
consistent with the previously-measured ~2.9–6.4ms warm / ~65–160ms cold
range (`AGENTS.md`, "Summon latency"), no regression. `disable_native_shadow`
is a one-time call at window creation, never on the summon path itself.
**Daemon idle memory**: not re-measured — this task touched no daemon-side
code at all (`neko-core`, `neko-daemon`), only `crates/neko/src/material.rs`
and `main.rs`.

## Click-outside-dismiss margin — unaffected, confirmed by code, not a live click

Per the launch brief's own standing rule (no synthetic input of any kind),
this was not re-tested with a real click — same limitation `235bf88`'s own
commit message already disclosed. Confirmed instead that nothing in this
task's diff touches `panel.rs` at all (`git diff --stat` against `main`
shows only `crates/neko/src/material.rs` and `crates/neko/src/main.rs`
changed, plus the new verification harness below) — `render_dismiss_margin`
and its `on_mouse_down` handler are byte-for-byte what `235bf88` shipped.
`NSWindow.setHasShadow(false)` has no bearing on hit-testing; it only
affects whether AppKit draws a shadow layer, a purely visual effect
orthogonal to which `NSView` mouse events land on.

## Verification setup — never the captain's real daemon

The captain's real `neko-daemon`/`neko` (pids 32503/32649, `/tmp/neko-target/release/`)
were never touched. Per this task's own new standing rule — the real
`neko-daemon` binary must never be launched for verification, even under an
isolated `HOME`, because its clipboard capture loop polls the *systemwide*
pasteboard regardless of `HOME` — a small permanent harness,
`crates/neko-daemon/src/bin/verify_harness.rs`, hosts the exact same
`server` module (real Spotlight-backed app index, real providers, real
socket protocol) `neko-daemon`'s own `main.rs` does, but never starts
`neko_core::clipboard::run_capture_loop`. A clipboard fixture was seeded
directly into the isolated SQLite database
(`Db::record_clipboard_entry`) instead of a real capture. An obscure
hotkey (⌘⌥⌃⇧F13) was committed to the isolated instance
(`neko_core::hotkey::set_hotkey`) before the client was ever launched.

**One real near-miss during this task, caught and corrected**: an early
`NEKO_BENCH` run was launched without the harness occupying the socket
first, so `neko`'s own `daemon_launcher::ensure_daemon_running()` spawned a
real (if freshly-built, not the captain's) `neko-daemon` binary for about
four seconds before this was noticed. It was killed by exact pid
immediately (never a broad `pkill`), and the isolated SQLite database was
checked afterward — only the one deliberately-seeded fixture row exists,
confirming nothing from the real system pasteboard was captured in that
window. Every verification run after this point launched `verify_harness`
first, unconditionally, before the client. Worth restating as a standing
practice: **always confirm the harness (or nothing) already holds the
daemon socket before launching `neko` for any reason**, since
`ensure_daemon_running` will otherwise spawn the real binary silently.

All captures were `screencapture -l<windowID>` (window-scoped), never
region or full-screen, per the standing rule in `AGENTS.md`'s "Window
material" section.

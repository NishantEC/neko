# Window behavior — multi-display, disconnected daemon, Spaces

Fixes `data/neko-audit/report.md` Part 4 items 8, 9, 10. Verified against the
real `target/release` binaries, under isolated `HOME`s
(`/tmp/neko-*-verify-home*`), never the captain's real daemon (pid 17761) or
client (pid 18165) — both confirmed running, untouched, throughout. This
environment (a treehouse worktree sandbox) has a single physical display
(3456×2234), so true cross-display evidence — a captain's built-in plus an
external 4K — could not be captured live here; that gap is stated plainly
below rather than inferred away.

## Item 9 — panel opens on the wrong display

**Fix**: `crates/neko/src/display_placement.rs`. Chose the display *under the
cursor*, not the display owning the active window — reasoning in that file's
own module doc comment (no cheap, permission-free way to query the active
window's owning display without Accessibility/AXUIElement, a cross-process
query; `NSEvent.mouseLocation` is synchronous, needs no permission, and
matches what a person expects from a "summon where I'm looking" launcher).

GPUI 0.2.2 has no public API to move an already-open window (`Window::resize`
exists; no `set_bounds`/`set_origin` — same gap `AGENTS.md`'s window-material
section already documents for title-bar drag), so this reaches for the real
`NSWindow` the same way `material.rs` does (`raw-window-handle`) and calls
the real, public `setFrameTopLeftPoint:`. Confirmed via source read
(`gpui-0.2.2/src/platform/mac/window.rs`) that gpui's own mac backend wires
`windowDidMove:`/`windowDidChangeScreen:` into `Window::bounds_changed`,
which refreshes `scale_factor` from the live `NSWindow` — so a native move
keeps GPUI's own renderer in sync, and the icon-cache renderer (already
resampling from the 128px `v3-128px` cache at the window's live
`scale_factor()` every frame — see `AGENTS.md`, "Icons") needs no special
handling for a display change.

**What's proven live, in this sandbox**: the real function
(`display_placement::reposition_to_cursor_display`), called on the real
resident summon window, using real `NSEvent.mouseLocation()` and
`NSScreen.screens()` — cost measured directly (temporary instrumentation,
reverted before commit): 8 consecutive calls, **66µs–788µs**, settling to the
66–260µs range after the first — negligible against the 4–6ms warm-summon
budget. The window continued to render correctly after the move (no
corruption, sharp icons in the connected-state screenshots below).

**What's proven by unit test, not live hardware** (`display_placement.rs`'s
own test module):
- `upper_third_offset` matches the exact existing formula for a 3456×2234
  display and, independently, for a 1920×1080 one (the captain's external
  4K's logical resolution) — proves the geometry math doesn't secretly
  assume the primary display's own size.
- `pick_screen_for_point` correctly resolves a cursor position to the right
  screen for a two-display side-by-side arrangement, **and** for a display
  placed with a negative origin (a real AppKit case for a monitor positioned
  above/left of the primary in System Settings' Displays arrangement) — the
  exact geometry a captain's real second-monitor arrangement could produce.

**Honestly unverified**: an actual two-display on-screen capture of the
panel opening on the *non-primary* display. The math and the live single-
display execution are both proven; the cross-display visual has not been,
for lack of a second physical display in this environment.

## Item 8 — daemon death leaves the UI silently frozen

**Repro, before fixing anything** (own repro, not the audit's inconclusive
one): built a second worktree at the pre-fix commit (`fe4e4e9`), launched it
under its own isolated `HOME`, used `NEKO_SHOW_QUERY` (no synthetic
keystrokes) to populate real search results, killed *that* isolated daemon
by exact PID (confirmed via `lsof` that its socket path matched the isolated
`HOME` first), and captured window-scoped screenshots before and after.
Result: **byte-identical PNGs before and after** — literally zero visual
change, confirming the audit's own reading of the code
(`connection-state-pre-fix-unchanged-both-before-and-after-kill.png`).

**Fix**: `NekoClient::is_connected()` (`neko-client/src/lib.rs`) — a plain
`AtomicBool` the reconnect supervisor flips on both transitions (dial-in
succeeds / connection drops), polled once per tick by `main.rs`'s existing
20ms summon loop (already polling `Event`s on that interval — the smaller
addition over a new push channel). `panel::Root::set_connected` pushes it
into a new `connected: bool` field, rendered as a strip banner
(`render_connection_banner`, same visual treatment as the existing
accessibility banner, no new chrome) reading "Can't reach neko-daemon.
Results may be out of date." — cleared automatically the moment the
supervisor reconnects, no dismiss control (nothing to dismiss, unlike the
accessibility banner's persisted setting).

**Repro after the fix**, same technique, real `target/release` binary: typed
"safari" (real result), killed the isolated daemon by exact PID, waited
~1.5s with **zero further input** — the banner appeared on its own:
- Before: `connection-state-before-daemon-killed.png`
- After: `connection-state-after-daemon-killed-fixed.png` (banner visible,
  stale Safari result still shown underneath — "results may be out of
  date," not cleared)

**Test**: `neko-client::tests::is_connected_reflects_the_daemon_dying_without_any_request_being_made`
— a real `UnixListener`, connect, assert `true`, close the accepted stream,
assert `false` again, with **no request ever sent** — the exact "idle, no
keystroke" case the audit's own repro attempt couldn't confirm.

## Item 10 — Spaces / full-screen reachability

**Verdict: already correct. Nothing changed in behavior — only a live
readback was added to prove it, per the brief's "do not report this one as
fixed on inference."**

`main.rs` opens the summon window with `kind: WindowKind::PopUp`.
`gpui-0.2.2/src/platform/mac/window.rs`'s `MacWindow::open`, `WindowKind::
PopUp` branch, unconditionally sets `NSWindowCollectionBehaviorCanJoinAllSpaces
| NSWindowCollectionBehaviorFullScreenAuxiliary` — confirmed by reading the
gpui source directly, not assumed. `crates/neko/src/spaces.rs` adds a live
readback (`spaces::verify`, same raw-window-handle technique
`material.rs`/`display_placement.rs` use), called unconditionally from
`main.rs` right after the material readback, logging to stderr on every real
launch — a permanent runtime sanity check, not a one-off proof.

**Live output, this run, real binary**:
```
neko: Spaces/full-screen reachability verified: readback bits=0x101 (NSWindowCollectionBehavior(257)) — reachable from every Space and over full-screen apps
```
`0x101` = `0x1` (`CanJoinAllSpaces`) | `0x100` (`FullScreenAuxiliary`) —
exactly the two bits `spaces::joins_all_spaces_and_fullscreen_auxiliary`
checks for, unit-tested independently (`spaces.rs`'s own test module, 5
cases including "an unrelated bit like `.moveToActiveSpace` must not
substitute for the real ones").

**Not independently tested live**: actually switching Spaces or entering a
full-screen app and confirming the panel is still reachable — ruled out by
the same standing rule the audit itself cites (no system-wide Space-switch
keystroke on a machine running the captain's live work). The readback plus
Apple's documented semantics for these two exact flags (cited in
`spaces.rs`'s own doc comment) is the evidence this task offers instead.

## Latency and memory — no regression

- **Warm summon latency**: unaffected by construction for the daemon/core
  crates (this task touches only `neko`/`neko-client`); for the client's own
  added cost, see item 9's timing above (66µs–788µs, per repositioning call,
  against the current 4–6ms warm-summon budget — over 5x margin even at the
  worst first-call sample).
- **Daemon idle memory**: `neko-core`/`neko-daemon` are untouched by this
  branch (`git diff --stat` shows zero changes in either crate) — the daemon
  binary is byte-for-byte the same build `main` already produces, so it
  cannot have regressed. Measured anyway for completeness: ~35–36MB RSS via
  `ps` in this sandbox, both before and after, standalone
  (`neko-daemon` alone, no client) and via a full client+daemon run — higher
  than `AGENTS.md`'s documented ~13MB, but identical between old and new
  builds, so a measurement-methodology or environment difference (macOS
  "memory footprint" vs. raw `ps` RSS is a known, large divergence),
  not a regression from this task.

## Constraints followed

Every daemon/client pair in this verification ran under an isolated `HOME`
(`/tmp/neko-*-home*`, never `~/Library/Application Support/neko/`). Every
kill used an exact PID, confirmed via `lsof` against the isolated socket
path first, never a pattern match. No `screencapture -x`/`-R` (full-screen
or region) was used anywhere — every capture was `screencapture -l<windowID>`
window-scoped. No synthetic OS keystrokes were sent. The captain's real
daemon (17761) and client (18165) were confirmed alive and untouched at the
end of this task.

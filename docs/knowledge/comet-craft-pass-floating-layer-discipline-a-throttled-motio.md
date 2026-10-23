## Comet craft pass: floating-layer discipline, a throttled motion catalog, `paint_layer`

`neko-craft-pass`, working from a read-only design study of `zeronsh/comet`
(MIT; `data/neko-comet-design/report.md` in the firstmate home, vendored
comet read at `refs/comet`) commissioned specifically to find what
neko's interaction craft could learn from comet's, on the captain's own
instruction to "read and write as close as possible" — read for pattern,
never copied. The report's own load-bearing finding shaped what was
buildable: **comet depends on a git fork of Zed's `gpui`, not the published
crate neko is deliberately pinned to** (see "The GPUI dependency decision"
below), so three of comet's primitives are simply absent from what neko
compiles against — confirmed by direct grep of the vendored `gpui-0.2.2`
source, not inferred from comet's `Cargo.toml`: `window.paint_backdrop_blur`/
`BackdropBlur` (comet's `frost.rs`), `gpui::EdgeFade` (comet's
`edge_fade.rs`), and `App::reduce_motion()`/`set_reduce_motion()` (comet's
own automatic reduced-motion snap). None of the three was built here, or
worked around by reaching for the fork — the report's own explicit
instruction, honored.

**The `⌘K` actions menu now has the same floating-layer discipline comet's
`popover.rs:395-416` (`anchored_menu`) demonstrates**, reimplemented against
neko's own geometry in `panel::Root::render_actions_menu`/
`render_actions_trigger`:
- `deferred(anchored().anchor(Anchor::BottomRight)
  .snap_to_window_with_margin(px(8.0)).child(card))` — its own floating
  paint layer above everything else painted that frame, clamped to stay
  inside the real window when the trigger sits near an edge. Genuinely
  reachable in this app, not hypothetical: the clipboard mode's own detail
  view runs the panel at the full `PANEL_WIDTH_WITH_DETAIL_PX` with no side
  margin, putting the trigger right at the window's own edge.
- `.occlude()` on the card, so a click on the card's own dead space can't
  fall through to whatever's under the floating layer.
- `.on_mouse_down_out(...)` dismisses the menu on any click outside the
  card — deliberately not "any click anywhere," which would double-fire
  with the trigger's own click.

**The trigger-click-while-open race, reproduced from comet's own finding
and fixed the same shape, in neko's own terms.** The footer trigger sits
outside the menu card, so clicking it while the menu is open fires the
card's own `on_mouse_down_out` (capture phase, on mouse-down) *and* the
trigger's own `on_click` (bubble phase, on mouse-up) from the same physical
press — a naive toggle reads `actions_menu` after the outside-close handler
already ran, finds it `None`, and reopens a fresh menu instead of leaving
the captain's dismiss click alone. `Root::menu_open_before_this_press`
(a snapshot taken by a capture-phase listener,
`note_actions_menu_mouse_down`, registered on `render`'s own outer panel
div so it always runs before any descendant's own capture-phase handler)
is the fix: `handle_actions_menu_trigger_click` checks whether the menu was
open *before this press's capture phase ran at all*, not its current state.
Regression-tested directly against the three real handler methods in
dispatch order (`panel.rs`'s
`clicking_the_trigger_while_the_menu_is_open_does_not_reopen_it`), plus the
normal-path counterpart proving the guard doesn't suppress a genuine open.
**Escape closes the menu before the panel** — `handle_dismiss`'s existing
early-return structure already gave modes this shape; the menu branch was
added ahead of it, tested at `escape_closes_the_actions_menu_before_exiting_an_active_mode`
(the panel-hide case for a *root-list* Escape isn't observable headlessly,
the same pre-existing limitation `escape_exits_the_mode_instead_of_hiding_when_one_is_active`
already had before this task). The destructive delete action's two-step
confirm (`AGENTS.md`, "Commands and modes") is untouched by any of this.
`reset_for_summon` now also closes a stale open menu unconditionally, not
just inside the mode-exit branch — a window losing activation with the menu
open on a *root-list* row used to leave it silently open into the next
summon, since that path never routed through `handle_dismiss`.

**`components::layered::layered(...)`** wraps the menu card's whole paint
(background, border, rows) in one `Window::paint_layer` call — comet's
`frost.rs` recommendation 3, reimplemented from scratch (no backdrop-blur
concept, since that needs the fork-only primitive above). **The rule this
exists to make easy to follow, for any future floating/overlaid chrome**:
if it needs to guarantee its own internal paint order (a tint under
content, a control over a thumbnail, anything layered), wrap it in
`layered(...)` rather than relying on sibling `.child()` order staying
stable — cheap, and it closes off a class of bug this codebase hasn't hit
yet (comet's own `frost.rs` module comment: a hover repaint elsewhere
reassigning a card's own quads to the wrong relative paint order). Honest
scope, stated in `layered.rs`'s own doc comment: this is a `gpui`
scene-graph-level guarantee — it would not have prevented either the
double-panel shadow defect (AppKit's own window shadow, a level below
`gpui`'s scene graph) or the mode-resize-seam bug (a `gpui`-internal
`viewport_size` cache going stale, not a paint-order issue).

**A small, throttled `motion.rs` catalog** — two named specs
(`MENU_FADE` 120ms, `CONTENT_FADE` 150ms) over one shared decelerate
`CubicBezier`, plus `menu_fade_in`/`fade_in` element helpers wrapping
`.with_animation(...)`. Three call sites, no more, per the launch brief's
own tie-break ("motion is worth less than correctness"): the actions
menu's open transition (opacity + a 4px settle drift), a mode's content
reveal on entry, and a new "still searching" tell
(`panel::Root::render_searching_tell`) for a query that hasn't returned
within `SEARCHING_TELL_DELAY_MS` (150ms) — closing a real, previously-silent
gap where `files.rs`'s up-to-1.5s worst case (`AGENTS.md`, "Search and
ranking") left stale results on screen with zero indication a new answer
was coming. **The menu's own close, and summon itself, are deliberately
never animated** — see `motion.rs`'s own doc comment for why each is an
instant cut. **No repeating animation exists anywhere in this catalog** —
both specs are one-shot `gpui::Animation`s, which `gpui-0.2.2`'s own
`AnimationElement` stops scheduling frames for the instant they finish, so
neither a mounted-and-finished nor an unmounted fade costs anything at
rest. This is the seam this task's brief asked to leave for the day a
repeating animation (a spinner, a pulse) is actually needed: it must go
through a shared, explicitly-throttled clock — never
`with_animation(...).repeat()` directly — per comet's own `PulseClock`
finding, restated in `motion.rs`'s own doc comment: one repeating element
pinned a whole window at 120Hz, measured 36% CPU. Nothing here needed that
clock, so none was built.

**Reduced motion, read live, not assumed.** `motion::system_reduce_motion()`
reads `NSWorkspace.accessibilityDisplayShouldReduceMotion()` directly (the
same AppKit-read pattern `material.rs`/`spaces.rs` already established for
other native state), since `App::reduce_motion()` doesn't exist in
published `gpui-0.2.2`. Every element helper takes the flag explicitly and
skips `with_animation` entirely when it's set, rather than forcing the
animation's output to its end state — a skipped animation schedules zero
extra frames; a forced-but-still-running one would keep requesting frames
for its nominal duration for no visual benefit.

**Verified live, on the release binaries, under the same isolated-harness
discipline "Daemon concurrency" below established** (`verify_harness`, a
seeded clipboard fixture, an obscure committed hotkey, no real
`neko-daemon` launch): idle CPU with the panel visible and nothing
happening averaged **~0.16% over a 15s sample** (`ps -p <pid> -o %cpu`,
polled once per second) — indistinguishable from a resting GPUI app's
baseline (a blink-cursor timer and occasional wakeups, nothing from this
task's own catalog, since nothing here repeats). Warm summon latency
(`NEKO_BENCH=20`) measured **1.3–5.5ms, mean ≈4.2ms** — matching "Summon
latency" below's existing ~2.9–6.4ms/mean≈4.1ms figure inside measurement
noise, confirming no regression. Daemon-side RSS (the harness standing in
for `neko-daemon`, same `server` module) held at ~13MB idle — this task
touched no daemon-side code at all, so no change was expected or found.
Window-scoped screenshots (`screencapture -l<windowid>`, per the standing
rule below): `docs/evidence/actions-menu-clipboard-mode-edge-clamp.png`
(the menu open inside clipboard mode, its right edge clamped inside the
window by `snap_to_window_with_margin`) and
`docs/evidence/actions-menu-root-list-open.png` (the menu open on a
root-list row, anchored above the trigger with room to spare). **One
environment note worth recording for the next agent doing evidence capture
on this machine**: `screencapture -l<windowid>` can fail with "could not
create image from window" for a reason unrelated to TCC permissions or the
window itself — `system_profiler SPDisplaysDataType` showing `Display
Asleep: Yes` (this machine's own `displaysleep` idle timer, since no human
is physically at the keyboard during an unattended agent run) makes
`ScreenCaptureKit` unable to start a capture stream at all (confirmed via
`log stream --predicate 'process == "screencapture"'`: "Failed to start
stream due to audio/video capture failure"). `caffeinate -u -t <seconds>`
before launching the client (not just before capturing — a window's
content painted while the display was asleep can stay stale in the
compositor even after the display wakes, until some new state change
triggers a fresh paint) is the standing fix, and is itself non-invasive
(no synthetic input, just a display wake assertion).

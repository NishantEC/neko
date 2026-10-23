## Mode view resize seam

`neko-mode-resize`, fixing a real defect two prior tasks
(`docs/evidence/mode-view-and-neutral-palette-report.md`) tried and failed
to reproduce: the clipboard mode's window widened to 760pt but only 680pt
was actually drawn into, leaving a transparent strip (the desktop visibly
showing through) and clipped text on the right. Full diagnosis, the four
workarounds tried and ruled out, and the before/after measurements:
`docs/evidence/mode-resize-seam-fix-report.md`. Summary:

**The reproduction the prior tasks were missing**: entering a mode
*immediately* after the window's first-ever appearance (their own repro
shape) always worked. The captain's real sequence — summon and dismiss the
panel some number of times first, *then*, from a later summon, enter a
mode — reliably reproduced the seam on the very first attempt.
`evidence.rs` gained `NEKO_REAL_CYCLES_BEFORE_SHOW=<n>` to drive that real
sequence (see its own doc comment for why it uses
`order_front_regardless`/`order_out`, not the real `activate_window`/
`cx.activate` path, to avoid colliding with the unrelated, already-
documented `windowDidBecomeKey:` deadlock below) — a permanent addition to
this repo's evidence tooling, not a one-off script.

**Root cause, confirmed by direct native readback, not inferred**: the old
`display_placement::resize_and_recenter`'s raw `NSWindow` resize genuinely
worked — `contentView`, GPUI's own rendering `NSView`, its `CAMetalLayer`,
and the Metal drawable itself all correctly tracked the new size, every
time, confirmed live. What silently didn't track it was `gpui::Window`'s
own private `viewport_size` — the size `draw_roots` (`gpui-0.2.2/src/
window.rs`) actually paints and clips the frame against — which only
resyncs via native `on_resize`/`on_moved`/`on_active_status_change`
callbacks that, per direct testing, stop firing reliably on this window
once it's been shown/hidden a few times. **Four different public-API
workarounds were tried and all failed** to force that resync (`window.
refresh()`, `window.resize()`, a real `-setFrameSize:` value-change nudge,
and forcing AppKit's own deferred layout via `-layoutSubtreeIfNeeded`/
`-displayIfNeeded`) — `viewport_size` stayed stale through all of them,
confirmed by reading the public `window.viewport_size()` accessor well
after each attempt, not just synchronously. This is a real `gpui-0.2.2`
staleness this codebase cannot reach around from outside the crate (the
field and the resync method are both private), not a bug in this repo's
own resize/positioning math — which the prior tasks' own readbacks had
already shown correct.

**The fix removes the operation that triggered the staleness, rather than
working around it**: the real `NSWindow` is now created once at
`theme::PANEL_WIDTH_WITH_DETAIL_PX` (760) and never resized again for the
rest of the process — `resize_and_recenter` is deleted, not just unused.
`panel::Root::render`'s own stage element is always `w(PANEL_WIDTH_WITH_DETAIL_PX)
.flex().justify_center()`; the narrower root-list panel centers inside it
via ordinary GPUI flexbox, provably unaffected by `viewport_size`'s own
tracking since the window's real size genuinely never changes any more.
`material::set_background_frame` (new) moves/resizes the installed native
backdrop directly — one synchronous `-[NSView setFrame:]` call, the same
centering formula `justify_center()` produces, no window resize and no
dependency on the broken callback chain. `panel::Root::update_background_bounds`
(replacing `resize_panel`) is the one call site, from `enter_mode`/
`exit_mode`/`reset_for_summon`.

**Disclosed cost, verified rather than assumed**: the real window's own
footprint is 760pt at rest now, not 680pt — an internal fact only.
`theme::PANEL_ROOT_INSET_PX` (40pt, half the width gap) centers the 680pt
root-list panel inside that wider window, landing its own visible left
edge at exactly the same on-screen position the old 680pt-wide window's
left edge sat at — proven algebraically and confirmed live
(`docs/evidence/mode-resize-seam-fix-report.md`'s own pixel measurements).
The 40pt margin outside the visible panel carries no material and no
shadow of its own (macOS's native shadow follows the actual painted pixels
for a transparent-backed window, confirmed in the same screenshots) — the
root list's resting state is visually unchanged.

**That 40pt margin is still real, clickable `NSWindow` frame, though — a
second real defect this task's own fix introduced, caught in review before
landing, not by the original acceptance criteria.** The window is always
`PANEL_WIDTH_WITH_DETAIL_PX` now, in *every* mode, not just detail mode; a
click landing in the margin (present on all four sides whenever the panel
is narrower than the window — i.e. the common root-list case, not just
detail mode) used to hit nothing (`justify_center()`'s implicit gap has no
gpui element covering it at all). Checked directly against `gpui-0.2.2`'s
own source (no `ignoresMouseEvents`, no custom `-hitTest:` override
anywhere in the crate) rather than assumed: standard AppKit hit-testing
means that click is captured by neko's own window (already key/frontmost,
`NSPopUpWindowLevel`) regardless — it neither reaches whatever's behind
the window on the desktop (no real click-through without
`ignoresMouseEvents`, which is a whole-window property and would also make
the *visible* panel unclickable) nor dismisses the panel (`main.rs`'s
click-outside-dismiss, `cx.observe_window_activation`, only fires on an
actual activation *change* — a click that stays inside neko's own window,
margin included, is never that). Two real, silent regressions from one fix:
a click there did nothing at all, where before this task there was no such
region for a click to land in.

Fixed by giving the margin its own real gpui presence:
`panel::Root::render_dismiss_margin` renders two explicit divs (left/right,
each `(PANEL_WIDTH_WITH_DETAIL_PX − panel_width) / 2`, `0`-width and so
harmless whenever the panel already fills the window) in place of
`justify_center()`'s implicit gap, each with an `on_mouse_down` that calls
`cx.hide()` — restoring the click-outside *behavior* the captain already
relies on, for a region that can no longer be true click-through short of
a second overlay window or patching `gpui`'s own hit-testing (neither
undertaken here — see the launch brief's own "whichever is cleaner"
framing between the two, and `ignoresMouseEvents`' whole-window scope
above for why it wasn't the one picked). **Live click confirmation
(pixel-position mouse-down/up, either via `System Events` or a
`CGEventPostToPid`-scoped synthetic event) was attempted and abandoned partway**:
a `System Events "click at"` probe against unverified absolute screen
coordinates landed on a real, unrelated app on this shared machine before
its risk was fully appreciated (harmless as far as could be confirmed —
an accessibility-hierarchy read, not a destructive action — but a real
lesson: never post a synthetic click at raw screen coordinates without
first confirming exactly what occupies them); switching to
`CGEventPostToPid` (confirmed safe — scoped to one process's own windows
by the OS, cannot address a different app regardless of coordinates) hit a
*separate*, real rendering staleness in this same long test session — data
proven correct via direct daemon queries and in-process debug reads, but
the on-screen frame never updated to reflect it, reproducible even on
completely unmodified, non-margin-related search-result rendering — that
made on-screen confirmation unreliable within the session and wasn't
chased further (a session/environment issue, not this fix's own
correctness, but not run to ground either). The fix itself rests on the
`gpui`-source-level hit-testing analysis above, not a live click; the next
person to touch this area should get a real, interactive click on the
margin from a clean session before trusting this description of the
mechanism as the final word.

**Correction to this section's own earlier claim, found by
`neko-double-panel`**: the text above ("The 40pt margin outside the visible
panel carries no material and no shadow of its own … the root list's
resting state is visually unchanged") is wrong. It does carry a shadow —
AppKit's own automatic window drop shadow, which (for this window's
Metal-layer-backed, near-invisible-alpha-background construction — see
"Window material" below) is computed against the **whole `NSWindow` frame**
rather than the actually-painted content, and was invisible only because
every screenshot taken by this section's own task happened to be judged by
eye against a low-alpha, blurred region rather than pixel-probed. Once the
window became permanently wider than the visible root-list panel, this
produced a real, captain-reported "two nested rounded rectangles" defect —
see "The double-panel shadow defect" below for the root cause, the fix
(`NSWindow.setHasShadow(false)`, since the panel `div`'s own `.shadow_lg()`
was always the shadow this app actually needed), and the corrected
measurements.

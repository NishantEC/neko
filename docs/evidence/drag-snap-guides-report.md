# Dragging the panel: snap targets and highlighted guides

`fm/neko-drag-snap`. Replaces the AppKit-owned drag that landed one commit
earlier (`a0d8716`) with one this app drives itself, because the two are
mutually exclusive and the reason is structural rather than a matter of effort.

## 1. Why the previous drag had to go

`a0d8716` dragged the panel with `Window::start_window_move()`, which on the
`wingleeio/zed` fork is a real `performWindowDragWithEvent:`. That call **runs
AppKit's own modal event loop** for the whole gesture and does not return until
the mouse is released. There is no point inside it at which this app can

- read where the cursor is relative to a snap target,
- decide which target wins,
- or draw anything at all.

Snapping requires owning the loop. So `panel::Root` now owns it:
`begin_window_drag` records the grab, window-level mouse listeners tick it,
`snap.rs` decides where the panel goes, and `window_drag.rs` moves the real
`NSWindow` and paints the guides. `main.rs` went back to `is_movable: false`
along with it — that flag governs *user* dragging via the API no longer used;
every placement neko has ever done is programmatic (`setFrameOrigin:`), which
it has never affected. Multi-display repositioning worked with it `false` for
the whole project's life, which is the proof.

## 2. The assumption everything rests on, and exactly how far it was checked

Once a snap holds the window still while the hand keeps moving, **the cursor
leaves the panel** — so the drag depends on mouse events still arriving at the
panel's window. Two independent mechanisms in the gpui revision this crate
compiles against say they do. Both were read in the pinned source at
`~/Library/Caches/neko-dev/gpui-fork-patched`, not assumed:

| mechanism | where | what it says |
| --- | --- | --- |
| AppKit implicit capture | `gpui_macos/src/events.rs:288` | `NSLeftMouseDragged` → `MouseMoveEvent { pressed_button: Some(Left) }`, no bounds check anywhere in the conversion |
| window-level dispatch | `gpui/src/window.rs`, `dispatch_mouse_event` | every listener registered by `Window::on_mouse_event` runs for every event; position is only ever consulted by the element wrappers (`div`'s own `on_mouse_move` is gated on `hitbox.is_hovered`, `gpui/src/elements/div.rs:305`) |
| synthetic drag | `gpui_macos/src/window.rs`, `synthetic_drag` | re-emits the last drag event every 16ms while the button is held, so ticks continue even when the mouse is physically still |

That last one is why the listeners are registered **unconditionally** every
frame rather than only while dragging: there is then no frame between the
mouse-down and the next paint in which a move could arrive unheard.

**Not verified by performing a drag.** Synthesising mouse input is forbidden in
this repo, so no agent can press the button. The design removes most of the
consequence: the position is never accumulated. Every tick reads
`NSEvent.mouseLocation` afresh and computes an absolute origin, so a dropped,
delayed or coalesced tick costs nothing and the drag cannot drift. **The first
person to drag this panel is doing the one test that was not possible here.**

## 3. The snap engine

`crates/neko/src/snap.rs`, pure `f64`, no gpui, 13 unit tests. One coordinate
space throughout — AppKit global points, y-up, bottom-left origins — chosen so
the drag never converts anything: `NSEvent.mouseLocation` and `NSWindow.frame`
are already in it, and `NSWindow.setFrameOrigin:` writes back into it. The only
conversion in the feature is the last one, turning a guide's global position
into a coordinate inside the overlay window (`window_drag::local_guides`, its
own two tests).

**Targets (7, of which one pair usually collapses to 6):** left/right edges,
horizontal centre, and home's x; bottom/top edges, vertical centre, and home's
y. "Home" is where a summon puts the panel, computed by
`display_placement::home_origin` through the *same* `upper_third_offset` that
places every real summon — passed into `snap::resolve` rather than recomputed
there, because the summon position derives from the display's **full** frame
while snapping works against its **visible** frame, and a home guide computed
the wrong way would lie by the height of the menu bar.
`home_origin_is_the_same_place_reposition_puts_a_summoned_panel` pins the
coordinate flip between the two.

**Threshold, 16pt**, bracketed rather than picked: wide enough that a hand not
being careful reaches it (a fast drag steps over a band of a few points), narrow
enough that three x targets occupy under 7% of a 1440pt-wide display. Full
reasoning in `SNAP_THRESHOLD_PT`'s doc comment, including the real case where
two targets overlap — on a 1280×800 display home and the vertical centre sit
28.3pt apart, so a 3.7pt band exists where both are in reach. That case is why
guides distinguish active from inactive at all.

**Off-screen is a hard clamp**, not a "keep 100pt visible" rule: `resolve`
clamps the desired origin into the visible frame before anything else, and
clamps every candidate the same way so a guide can never promise a position the
panel will not take. A panel bigger than the screen pins to the visible origin
rather than producing an inverted range (`clamp` panics on `min > max`) — tested.

## 4. The guides are a second window

An element cannot paint outside its own window, and every guide is at a screen
edge or centre. So: a transparent, click-through
(`NSWindow.ignoresMouseEvents`, **read back** rather than trusted — a window at
`NSPopUpWindowLevel` that silently failed to take it would eat clicks over every
other application, so a failure closes the window instead), non-key
(`focus: false`, which is gpui's `orderFront:` path rather than
`makeKeyAndOrderFront:`), `WindowKind::PopUp` window ordered **below** the panel.

It is opened **lazily** — the first time a guide actually has to be drawn — and
closed on every path that ends a drag. A plain click on the input row, or a drag
that never approaches a target, never creates a window at all.

## 5. Live verification

Release binaries, isolated `HOME`, `verify_harness` (never the real
`neko-daemon`), empty `PATH` so `daemon_launcher` could not spawn one,
`screencapture -l<windowid>` only, no synthetic input. `key window false`
logged at every capture point on every run.

`NEKO_SHOW_DRAG_GUIDES=1` (new, `evidence.rs`) runs the genuine path — the same
`snap::resolve`, the same `open_overlay` — with only the cursor's contribution
replaced by a fixed desired origin, and prints every input and output so the
picture can be checked **against the numbers that produced it**:

```
neko: drag-guide overlay window number 9874 (2 guides, opened in 26.846583ms)
visible=Rect { x: 0.0, y: 0.0, width: 1496.0, height: 938.0 } panel=Size { width: 760.0, height: 421.0 }
home=Point { x: 368.0, y: 328.9166564941406 } desired-origin snapped to Point { x: 0.0, y: 328.9166564941406 }
  guide LeftEdge Vertical global=0.0 local=0.0 active=true
  guide HomeTop Horizontal global=749.9 local=188.1 active=true
```

`docs/evidence/drag-snap-guides-overlay.png` is that window, and every pixel
agrees:

| claim | measured in the capture |
| --- | --- |
| the overlay is exactly the visible frame | 2992×1876 device px at 2x = **1496×938pt** |
| the left-edge guide sits at local x = 0 | opaque device columns **0–7** = 0–4pt |
| a guide is 4pt: 1pt outline, 2pt core, 1pt outline | `(13,13,13,255)`, `(233,233,233,229)`, `(13,13,13,255)` — `surface_panel`, then `text_primary` at `SNAP_GUIDE_ALPHA` 0.9 |
| the home guide sits at local y = 188.1 | opaque device rows **372–379**, centred on 188.1pt |
| nothing else is painted | alpha is exactly 0 everywhere outside the two lines |

`drag-snap-guides-panel.png` is the panel from the same run — unchanged, and
still rendering normally with the overlay open beneath it.

**A real defect found by that measurement, and fixed.** The first capture came
back 3128×2012 with a soft alpha gradient rising to **44/255** either side of
each line: AppKit's automatic window shadow, computed from what a transparent
window actually paints, which here is the guide lines themselves. A measuring
line with a halo is not a measuring line. `material::disable_native_shadow` on
the overlay — the same call and the same reasoning as the panel's own
(`AGENTS.md`, "The double-panel shadow defect") — is the fix, and the table
above is the after: alpha exactly 0 outside the lines, and the capture is now
exactly the window's own size.

**Warm summon latency, same harness, `NEKO_BENCH=15`:** summons 1–14 ranged
**0.84–9.19ms, mean ≈4.96ms** (cold summon 0: 55.1ms). That brackets the
existing documented figure (mean 8.56ms, range 1.20–15.60ms, `AGENTS.md`
"Summon latency") — the two extra window-level listeners and the zero-sized
canvas registering them cost nothing measurable.

## 6. What was NOT verified

- **No real drag was ever performed.** Everything above about following the
  cursor, snapping on release, and guides appearing *as you approach* rests on
  the source-level argument in §2 plus the unit tests, not on a gesture.
- **The muted (inactive) guide weight was never photographed.** Producing one
  needs two targets within 16pt of each other on one axis, which this machine's
  geometry does not offer (home and the vertical centre are 70.4pt apart on its
  visible frame). It is unit-tested
  (`two_targets_within_reach_draw_two_guides_with_exactly_one_active`) and is a
  gated palette derivation (`snap_guide_muted` is exactly half `snap_guide`'s
  alpha, asserted for every one of the seventeen themes).
- **Multi-display was not exercised.** One physical display here, the same gap
  `docs/evidence/window-behavior-report.md` already records. The cross-display
  path (close the overlay, reopen it sized to the new screen) is written and
  compiles; it has never run.
- **The ~27ms overlay open was not optimised.** It is paid once per drag, at the
  moment the first guide appears, and the obvious way to remove it — keeping one
  overlay alive between drags — is precisely what "torn down on mouse-up, every
  path" rules out. Measured, disclosed, left.
- **No light theme was captured**, and the guide's legibility over a genuinely
  busy wallpaper was not tested: `screencapture -l` composites the window alone,
  which is the same standing limitation every material capture in this repo has.

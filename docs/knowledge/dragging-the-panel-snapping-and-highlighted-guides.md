## Dragging the panel: snapping and highlighted guides

`fm/neko-drag-snap`. Full account, the pixel measurements, and what was
**not** verified: `docs/evidence/drag-snap-guides-report.md`. **Read
`crates/neko/src/snap.rs`'s module doc comment before touching anything
about where the panel sits** — it is the normative statement of the
coordinate space and the target set.

**The panel is dragged by the input row or the footer, and this app owns
the gesture.** One commit shipped the four-line version first
(`a0d8716`: `Window::start_window_move()` → `performWindowDragWithEvent:`)
and it had to be replaced wholesale, for a structural reason worth keeping:
**`performWindowDragWithEvent:` runs AppKit's own modal event loop** and
does not return until the mouse comes up. There is no moment inside it at
which proximity to a snap target can be measured, a target chosen, or a
guide drawn. Snapping requires owning the loop. `main.rs` went back to
`is_movable: false` with it — that flag governs *user* dragging through the
API no longer used, and has never affected the programmatic
`setFrameOrigin:`/`setFrameTopLeftPoint:` placement this app has always
done (multi-display repositioning worked with it `false` for the project's
whole life).

**Three pieces, split so the correctness is testable:**

- `crates/neko/src/snap.rs` — **pure, no gpui, 13 tests.** Given the visible
  frame, the panel's size, home, and a desired origin, it returns the
  snapped origin and the guides. All of the correctness lives here.
- `crates/neko/src/window_drag.rs` — everything native: reading the cursor,
  moving the real `NSWindow`, and the guide overlay window. Injected into
  `panel::Root` as `Rc<dyn PanelDrag>` for the same reason
  `AppearanceSetter`/`PreferencesOpener` are — **GPUI's test platform panics
  on `window_handle()` and `open_window`**, so a native call from a mouse
  handler would take every headless panel test down with it.
- `panel::Root` — four verbs (`start`/`update`/`finish`/`cancel`) and one
  `dragging: bool`. Nothing about grab offsets, screens or guides crosses
  into it.

**One coordinate space, AppKit's** (points, y-up, bottom-left origins),
chosen so the drag never converts anything: `NSEvent.mouseLocation` and
`NSWindow.frame` are already in it and `setFrameOrigin:` writes back into
it. The single conversion in the feature is turning a guide's global
position into a coordinate inside the overlay window.

**The event is a clock, not a position.** `MouseMoveEvent::position` is
window-relative, and this drag moves the window out from under the cursor —
measuring against it would be measuring against a datum that moves with the
thing being measured. Every tick reads `NSEvent.mouseLocation` afresh and
computes an **absolute** origin, which is also what makes a dropped tick
free.

**The assumption the whole approach rests on, and how far it was checked.**
Once a snap holds the window still while the hand keeps moving, the cursor
leaves the panel, so the drag needs mouse events to keep arriving anyway.
They do, confirmed by reading the pinned gpui rev (not assumed): AppKit's
implicit capture sends `mouseDragged:`/`mouseUp:` to the mouse-down view,
`gpui_macos/src/events.rs` converts it with no bounds check, and
`dispatch_mouse_event` runs every `Window::on_mouse_event` listener for
every event — **which is why the listeners are window-level and not on a
`div`**: a `div`'s own `on_mouse_move` is gated on `hitbox.is_hovered`.
gpui's own `synthetic_drag` re-emits the last drag event every 16ms while
the button is held, so ticks continue even when the mouse is still.
**Nobody has performed a real drag** — synthesising mouse input is
forbidden here — so that argument plus the unit tests is the whole
verification. The absolute-position design is what keeps the consequence
small.

**Targets, and the threshold.** Screen edges and centres (against
`visibleFrame`, so "the bottom edge" is above the Dock, not behind it), plus
**home** — where a summon puts the panel. Home is computed by
`display_placement::home_origin` through the *same* `upper_third_offset`
every real summon uses and passed *into* `snap::resolve`, never recomputed
there: the summon position derives from the display's **full** frame while
snapping works against its **visible** one, so recomputing it the wrong way
would produce a home guide that lies by the height of the menu bar. Home's x
usually collapses into the horizontal centre; a left- or right-side Dock
genuinely separates them and both are then offered.
`SNAP_THRESHOLD_PT` is **16pt**, bracketed rather than picked — see its own
doc comment, which also names the real case where two targets overlap
(1280×800, home and vertical centre 28.3pt apart).

**Off-screen is a hard clamp, applied before snapping and to every
candidate.** The panel has no title bar and no window-list entry; half of it
hanging off a display has no upside and "you can lose it" is a real failure
mode. Escape during a drag puts the panel back where it was picked up
(`handle_dismiss`, ahead of the menu and mode branches — while the panel is
physically moving, Escape can only mean that), and `reset_for_summon`
cancels unconditionally so a drag can never outlive its summon.

**The guides are a second window, and it is opened lazily.** An element
cannot paint outside its own window and every guide is at a screen edge or
centre. So: transparent, click-through (`NSWindow.ignoresMouseEvents`, read
back rather than trusted — a window at `NSPopUpWindowLevel` that silently
failed to take it would eat clicks over every other app, so a failure closes
it), never key (`focus: false` is gpui's `orderFront:` path), `PopUp` level
so guides float above other applications, ordered **below** the panel.
Opened the first time a guide actually has to be drawn and closed on every
path that ends a drag — a plain click, or a drag that never nears a target,
opens no window at all. Measured cost when it does open: **~27ms**, paid
once per drag; the obvious way to remove it (keep one alive between drags)
is exactly what "torn down on mouse-up, every path" rules out.
**Its AppKit window shadow is disabled too** — a transparent window's
automatic shadow is computed from what it actually paints, which here is the
guide lines, measured bleeding 44/255 of black either side of each line
before the fix. Same call, same reasoning as the panel's own ("The
double-panel shadow defect").

**Two new palette tokens**, `snap_guide` and `snap_guide_muted` — the
theme's own `text_primary` at `SNAP_GUIDE_ALPHA` and at exactly half it,
derived in `build()` like every other dependent token, so a theme supplies a
hue and can never decouple the two weights. `PALETTE_TOKEN_COUNT` is 23 —
`surface_tile` (the theme's foreground at `TILE_ALPHA`) joined them when the
agent tiles had to stop being opaque: the panel paints glass now, so an
opaque tile on top of it read as a block pasted on rather than part of the
same pane.
The guide is drawn with a `surface_panel` hairline outline so it reads over
a wallpaper of either polarity. `SNAP_GUIDE_THICKNESS_PX` is geometry and
stays `const`.

**Evidence hook: `NEKO_SHOW_DRAG_GUIDES=1`** (with `NEKO_SHOW_ON_LAUNCH`)
opens the overlay for a snap the panel is not actually being dragged into —
the real `snap::resolve` and the real overlay, with only the cursor's
contribution replaced — and prints every input and output beside the window
number, so a capture is checked against the numbers that produced it rather
than eyeballed. Focus-neutral; the panel's own window state is untouched.

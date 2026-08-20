# The untitled window that stopped accepting keystrokes

`fm/neko-revert-titled`. `98522de` cleared `NSTitledWindowMask` to remove
AppKit's top-edge rim (correctly — the line went away, and the captain wants
that kept). Within minutes of it landing on his running app he reported *"I'm
not able to type in the prompt."*

## What actually broke — proven, not inferred

**Not key-window eligibility.** The launch hypothesis was the standard macOS
rule that an untitled window cannot become key unless `canBecomeKeyWindow`
allows it. That is not the mechanism here: the fork's own window classes
override it unconditionally.

```
crates/gpui_macos/src/window.rs:365  build_window_class(...)
    decl.add_method(sel!(canBecomeKeyWindow), yes as extern "C" fn(...) -> BOOL);
crates/gpui_macos/src/window.rs:133-134
    WINDOW_CLASS = build_window_class("GPUIWindow", class!(NSWindow));
    PANEL_CLASS  = build_window_class("GPUIPanel",  class!(NSPanel));
```

`yes` (line 1976) returns `YES` with no style-mask test anywhere. neko's summon
window is `WindowKind::PopUp` → `PANEL_CLASS`, so it is covered.

**The real cause: `setStyleMask:` resets the window's first responder.** AppKit
rebuilds the window's frame view for a style-mask change, and the responder
goes with it. gpui calls `makeFirstResponder:` exactly once, at window creation
(`gpui_macos/src/window.rs:985`), so nothing puts it back.

Measured live on the real binary, before/after a single `setStyleMask:` call:

```
mask 0x8081->0x8080  firstResponder GPUIView -> NSKVONotifying_GPUIPanel
```

With the panel itself as first responder, `keyDown:` never reaches `GPUIView`,
so no character ever reaches `TextField`. Every other property stayed correct,
which is why the original task's checks (corners, Spaces, shadow, the
non-activating bit, the style mask itself) all passed on a build that could not
be typed into.

## The fix

`material::clear_titled_style_mask` re-makes gpui's rendering view the first
responder in the same call that disturbs it, so the two cannot drift apart. No
gpui patch needed — this is reachable from neko's own code through the same
`raw-window-handle` walk `material.rs` and `spaces.rs` already use.

`material::verify_titled_cleared` now asserts the responder alongside the mask
bits, so this regression cannot recur silently.

## Verification

Release binaries, isolated `HOME`, no real `neko-daemon`, non-activating window
(`key window false` throughout — no evidence window ever took focus), no
synthetic OS input.

| Run | style mask | first responder | typed "safari" → field |
|---|---|---|---|
| Control: titled (pre-`98522de`) | `0x8081` | `GPUIView` | `"safari"` |
| `98522de` as shipped (responder restore disabled) | `0x8080` | `NSKVONotifying_GPUIPanel` | see caveat |
| **Shipping fix** | `0x8080` | `GPUIView` | `"safari"` |

Also verified unchanged on the shipping build: `NSGlassEffectView` installed and
read back with `cornerRadius=16` (rounded corners), Spaces/full-screen
reachability `0x101`, window shadow disabled, non-activating panel and
full-size-content-view bits both still set (`0x8080`).

### `NEKO_PROVE_TYPING`, and what it does and does not prove

New evidence hook (`evidence.rs`): types a string through
`Window::dispatch_keystroke` — gpui's own real key dispatch, the path a physical
keypress takes *after* AppKit hands the event over — then reads the field back.

It covers gpui's dispatch, this app's bindings, and `TextField`'s editing model.
It does **not** cover the AppKit half, and the negative-control row above is the
honest demonstration of that limit: with the responder restore disabled the
typing proof still reported `PASSED`, because `dispatch_keystroke` enters below
the layer that was broken. **The first-responder readback is the check that
catches this regression**, and it did — `verify_titled_cleared` failed loudly on
that same run.

Delivering a real `NSEvent` instead was tried and abandoned. gpui routes
printable keys through `-[NSTextInputContext handleEvent:]`, which does nothing
for a window that is not key, and an evidence window must never be made key.
Confirmed by control: a synthesized `NSEvent` was swallowed identically on an
ordinary *titled* window, so that harness could not have distinguished a working
build from a broken one. Both `-[NSWindow sendEvent:]` and a direct
`keyDown:` to the live first responder behaved the same way.

### Not verified

- **Click-outside dismissal.** It runs off `cx.observe_window_activation`, which
  needs a real activation change driven by a real click. Synthetic input is off
  the table and no evidence window may take focus, so this was not exercised.
  Nothing in this change touches activation or collection behaviour — the
  non-activating panel bit is intact and `canBecomeKeyWindow` is unconditional —
  but that is reasoning, not a measurement. Worth one interactive check.
- **Escape.** Same GPUI dispatch path the typing proof exercises, but its effect
  (`cx.hide()`) is not observable headlessly.
- Warm summon latency and daemon memory: unchanged code paths, not re-measured.

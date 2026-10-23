## The top line — AppKit's titled-window rim

`fm/neko-top-line-titled`, the third and actual fix for the symptom the two
corrections above misattribute. Root-caused by `data/neko-truth-pass/
report.md` §2.4 (four escalating single-variable tests); applied and
verified here. Full before/after pixel measurements, the survives-checks,
and the latency/memory numbers: `docs/evidence/top-line-titled-window-fix-report.md`.

**Cause**: gpui's mac backend creates this window **titled** no matter what
`WindowOptions.titlebar` says — `titlebar: None` falls into an `else` branch
setting `NSTitledWindowMask | NSFullSizeContentViewWindowMask`, and
`titlebar: Some(..)` sets `NSTitledWindowMask` too. **No `WindowOptions`
value produces an untitled window.** AppKit draws its own ~1pt top-edge
highlight on a titled window, composited above everything the app paints.
The truth pass proved neko doesn't paint it two independent ways: the
brightening is identical under all three material paths (so not the
material, and not Liquid Glass's specular rim), and it survives replacing
the panel `div` with a flat opaque black rect.

**Fix**: `material::clear_titled_style_mask` clears **only** bit 0 on the
real `NSWindow`, via the same `raw-window-handle` walk this module and
`spaces.rs` already use, with `material::verify_titled_cleared` reading the
live mask back and asserting the bit is gone *and* that
`NSNonactivatingPanelMask` (bit 7) and `NSFullSizeContentViewWindowMask`
(bit 15) both survive — same "verified, not trusted" pattern as
`verify_installed`/`verify_shadow_disabled`/`spaces::verify`. `0x8081` →
`0x8080`, both halves logged on every launch next to the existing
readbacks. **Called first in `main.rs`'s window-init closure, before
`material::install`** — `setStyleMask:` makes AppKit rebuild the window's
frame view, so every native view installed afterwards goes into the final
one.

**What survived, each checked live rather than assumed** (the real risk in
mutating a live style mask): rounded corners — unchanged, since rounding
here is GPUI's `.rounded()` plus the material view's own `cornerRadius=16`,
never AppKit's titled frame; Spaces/full-screen reachability — `0x101`,
a separate property; the non-activating panel bit — still set, and gpui's
own window subclass overrides `canBecomeKeyWindow` to `YES` regardless of
style mask anyway; the deliberately-disabled window shadow; and the whole
material chain. Warm summon latency and RSS both unchanged.

**It broke keyboard input, and was fixed forward rather than reverted
(`fm/neko-revert-titled`).** `setStyleMask:` makes AppKit rebuild the window's
frame view, and **the window's first responder is reset with it** — measured
live, `GPUIView` before the call, `NSKVONotifying_GPUIPanel` after. gpui calls
`makeFirstResponder:` exactly once, at window creation
(`gpui_macos/src/window.rs:985`), so nothing restores it: `keyDown:` stopped
reaching GPUI at all and the search field silently accepted nothing, while
every property the original task checked still read back correct. **It was
never a key-window problem** — the fork's `GPUIPanel` overrides
`canBecomeKeyWindow` to return `YES` unconditionally, with no style-mask test
(`gpui_macos/src/window.rs:365`), so notes elsewhere predicting an untitled
window cannot become key do not apply here. `clear_titled_style_mask` now
re-makes gpui's rendering view first responder in the same call that disturbs
it, and `verify_titled_cleared` asserts the responder alongside the mask bits.
No gpui patch was needed. A revert commit is kept on that branch as a
fallback but is not the shipping state. Full A/B (including the negative
control), and what the new `NEKO_PROVE_TYPING` hook does and does not prove:
`docs/evidence/titled-window-first-responder-fix-report.md`.

**Anything that mutates this window's style mask must re-assert the first
responder afterwards, and any window-level change must be verified by
actually typing** — property readbacks cannot see this failure. Note the
converse trap the same investigation found: `Window::dispatch_keystroke`
enters *below* AppKit's responder chain, so it reports success on a build
that a real keypress cannot reach. The first-responder readback is the check
that catches it.

**Three overlapping causes, one symptom — the durable lesson.** The panel's
top edge had *three* independent bright/dark artifacts at different times
(AppKit's automatic window shadow, GPUI's own `.shadow_lg()`, and this
titled-window rim), each invisible until the fixed-width window opened a
margin for it, and each fix correctly removed one while leaving the others
standing. A captain re-reporting the same symptom after a verified fix is
evidence of *another* source, not of the fix having failed — measure the
edge again with a single-variable test before attributing.

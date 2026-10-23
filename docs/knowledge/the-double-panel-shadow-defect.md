## The double-panel shadow defect

`fm/neko-double-panel`, fixing the regression the correction just above
describes. Full investigation (four candidates checked and ruled out by
live readback before the real cause was found, the single-variable test
that confirmed it, before/after pixel measurements, and the verification
methodology — including a near-miss where the real `neko-daemon` was
briefly, accidentally spawned and how that was caught): `docs/evidence/
double-panel-shadow-fix-report.md`. Summary:

**Root cause**: not a second painted surface — every candidate named in
this bug's own launch brief (the native backdrop material view, the stage
`div`, the margin divs, `main.rs`'s startup narrowing call) was confirmed
clean by live readback. The real cause is AppKit's own automatic window
drop shadow, which this window's construction (`WindowBackgroundAppearance::
Transparent`, a single Metal-layer-backed rendering `NSView` covering the
whole window) forces AppKit to compute against the **entire `NSWindow`
frame** rather than the narrower content actually painted inside it. Before
"Mode view resize seam" above, the real window's own frame always matched
the visible panel's width, so this was invisible by construction; once the
window became permanently `PANEL_WIDTH_WITH_DETAIL_PX` regardless of a
narrower root-list panel, the shadow started extending
`theme::PANEL_ROOT_INSET_PX` past the real panel edge on both sides, read
by the captain as a second, correctly-rounded but wrongly-sized surface.

**The fix**: `material::disable_native_shadow` calls
`NSWindow.setHasShadow(false)` once, at window creation (`main.rs`, right
after `material::install`), with `material::verify_shadow_disabled` reading
it back immediately after — the same "verified, not trusted" pattern
`verify_installed`/`spaces::verify` already establish. The panel `div`'s
own `.shadow_lg()` (`panel.rs`, untouched) is unaffected and remains the
only shadow this app draws, now correctly the *only* shadow visible too.
No change to the fixed-width-window architecture itself — the real
`NSWindow` still never resizes at runtime.

**Verification tooling this task added, permanently**:
`crates/neko-daemon/src/bin/verify_harness.rs` — a small second binary in
the `neko-daemon` crate that hosts the real `server` module (real app
index, real providers, real socket protocol) without ever starting
`neko_core::clipboard::run_capture_loop`. This is now the standing way to
verify anything that needs a live daemon connection without risking a real
capture of the captain's actual pasteboard — the real `neko-daemon` binary
must never be launched for verification, even under an isolated `HOME`, per
the report's own near-miss account. Seed fixtures via
`Db::record_clipboard_entry` directly; commit an obscure hotkey via
`neko_core::hotkey::set_hotkey` before launching a client against it.

**Correction to this section's own claim above, found by
`fm/neko-panel-shadow-tent`**: "the panel `div`'s own `.shadow_lg()` … is
unaffected … remains the only shadow this app draws, now correctly the
*only* shadow visible too" is wrong, in the same shape as the correction
this section's own fix already made to the section above it. `.shadow_lg()`
is GPUI's own box-shadow, painted as real scene pixels — a second,
independent source, not "the same shadow, now unblocked." It paints a few
px of soft, low-alpha black *outside* the panel `div`'s own bounds, and the
same `theme::PANEL_ROOT_INSET_PX` margin this section's own fix left
untouched (because it was never the AppKit auto-shadow's doing) gave that
blur real, otherwise-empty transparent window to bleed into — invisible
before `235bf88` for the identical reason the AppKit shadow was, and
visible after it for the identical reason too. The captain kept reporting
the same "black tent" symptom on a binary that already contained this
task's fix because this task closed one of the two overlapping shadow
sources, not both. See "The panel shadow tent — the second overlapping
shadow source" below for the real fix and the single-variable test that
confirms it.

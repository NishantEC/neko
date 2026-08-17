# Native material readback verification

Non-visual, in-process proof that `material::install`'s `Ok(Installed::_)`
claim is real: `material::verify_installed` re-derives `window.contentView()`
fresh after install, reads back the actual live view AppKit is holding
(concrete class via downcast, its material-specific properties, and its
position in `contentView.subviews()`), and asserts every value — rather than
trusting that the setter calls silently took effect. See `AGENTS.md`,
"Window material", for why this replaces a screenshot-based compositing
proof (a `screencapture -l<windowid>` window-scoped capture cannot show
`BehindWindow` vibrancy at all — a real OS limitation, not a missing flag —
and this task's standing rule after a near-miss forbids the full-screen/
region capture that could show it).

Both runs below are the real `neko` release binary
(`target/release/neko`), driven by `NEKO_SHOW_ON_LAUNCH=1`
(`crates/neko/src/evidence.rs`) — the same `install`/`verify_installed` call
main.rs makes on every real launch, not a special test harness.

## Primary: `NSGlassEffectView` (default, no env override — this machine is
macOS 26.5.1, so the runtime class lookup succeeds)

```
$ NEKO_SHOW_ON_LAUNCH=1 ./target/release/neko
neko: window material installed: Glass
neko: material verified: NSGlassEffectView at contentView.subviews()[0] (below GPUI's rendering view, 2 total subviews) — readback style=NSGlassEffectViewStyle(0) cornerRadius=16
neko: window number 3402
neko: window rect 408px 218px 680px 421px
```

`NSGlassEffectViewStyle(0)` is `.regular` (`NSGlassEffectViewStyle::Regular`
constant value). `cornerRadius=16` matches `theme::PANEL_RADIUS_PX`. 2 total
subviews of `contentView` = the installed background view (index 0) plus
GPUI's own rendering view — confirming the background view is a *sibling* of
GPUI's view under the real content view, not a subview of it (the invariant
`material.rs`'s own module doc comment calls out).

## Fallback: custom `NSVisualEffectView`, forced via `NEKO_FORCE_MATERIAL=popover`

```
$ NEKO_FORCE_MATERIAL=popover NEKO_SHOW_ON_LAUNCH=1 ./target/release/neko
neko: window material installed: Popover
neko: material verified: NSVisualEffectView at contentView.subviews()[0] (below GPUI's rendering view, 2 total subviews) — readback material=NSVisualEffectMaterial(6) blendingMode=NSVisualEffectBlendingMode(0) state=NSVisualEffectState(1) layer.cornerRadius=16 layer.masksToBounds=true
neko: window number 3419
neko: window rect 408px 218px 680px 421px
```

`NSVisualEffectMaterial(6)` = `.popover`, `NSVisualEffectBlendingMode(0)` =
`.behindWindow`, `NSVisualEffectState(1)` = `.active` (the raw integer
values match `objc2_app_kit`'s own published constants). `layer.cornerRadius`
and `layer.masksToBounds` confirm the `CALayer`-based corner mask (the path
`NSVisualEffectView` needs, unlike `NSGlassEffectView`'s own `cornerRadius`
property) was actually applied, not just requested.

## Final fallback: opaque, forced via `NEKO_FORCE_MATERIAL=opaque`

No material is installed in this branch (`install` returns `Err` before
attempting either view), so there is nothing to read back — the proof here
is the window-scoped screenshot
(`docs/evidence/material-opaque-fallback-window-scoped.png`) showing the
plain opaque panel with its hairline border, plus the `install` error being
handled without a panic:

```
$ NEKO_FORCE_MATERIAL=opaque NEKO_SHOW_ON_LAUNCH=1 ./target/release/neko
neko: native window material install failed, falling back to opaque: forced via NEKO_FORCE_MATERIAL=opaque, for fallback-chain verification
neko: window number 3015
```

## What this does and does not prove

Proves: the correct native class was actually instantiated and installed
(not merely requested), configured with the exact properties `install`
intended, and positioned correctly relative to GPUI's own rendering view —
the part this task's implementation owns.

Does not re-prove: that a live `BehindWindow` material genuinely composites
with real screen content behind it (shifts hue with a red vs. blue
backdrop, with an opaque control that doesn't move). That mechanism is
already independently proven for this exact `NSVisualEffectView`/
`NSGlassEffectView` code path by `data/neko-native-material/report.md` §2's
own differential test (firstmate verified those numbers directly) — cited
here rather than re-run, per the standing capture-safety rule in
`AGENTS.md`.

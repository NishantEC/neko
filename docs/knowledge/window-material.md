## Window material

Real native material, not GPUI's own `WindowBackgroundAppearance::Blurred`
(which hard-codes `NSVisualEffectMaterial.Selection`, no public choice of
material). `crates/neko/src/material.rs` implements
`data/neko-native-material/report.md`'s (firstmate home) recommendation: a
three-step chain, `install(window: &gpui::Window) -> Result<Installed,
String>`, called once from `main.rs` right after the summon window opens
(the window is resident for the process lifetime — only ever hidden, never
closed — so one install covers every future summon):

1. **`NSGlassEffectView`** ("Liquid Glass"), style `.regular`, corner radius
   16pt via its own `setCornerRadius` — macOS 26+, detected with a runtime
   class lookup (`AnyClass::get(c"NSGlassEffectView")`), never an OS
   version-string parse.
2. **Fallback**: a custom `NSVisualEffectView`, material `.popover`,
   `blendingMode: .behindWindow`, `state: .active`, corner radius 16pt via
   its backing `CALayer` (`setCornerRadius`/`setMasksToBounds`/
   `setCornerCurve(kCACornerCurveContinuous)` — `NSVisualEffectView` has no
   `cornerRadius` property of its own, unlike `NSGlassEffectView`). Deliberately
   `.popover`, not the better-measuring `.sidebar`/`.underWindowBackground` —
   report §2 found those two render pixel-identically to each other under
   `BehindWindow` blending on this OS for reasons it couldn't fully explain.
3. **Final fallback**: plain opaque. Only reached if `install` returns `Err`
   (in practice: no raw window handle, or no `NSWindow`/`contentView` yet —
   the two custom-view installs themselves are infallible). The caller
   (`main.rs`) then calls `window.set_background_appearance(Opaque)`
   explicitly, since the window opens `Transparent` (`material::
   window_background()`) to let any installed material show through.

**The one invariant that matters, stated in `material.rs`'s own module doc
comment because getting it backwards is silent and severe:** background
views must be inserted into `window.contentView()` — the real `NSWindow`'s
true root view, reached by walking up from the `NSView` `raw-window-handle`
hands out (GPUI's own rendering view) via `.window()` — never as a subview
of that rendering view itself. A subview renders *above* its superview's
own layer-drawn content, so inserting there silently eats every pixel GPUI
draws, text included. `material::macos::native_window` is the one function
that does this walk; every install path and the bench-only window-ordering
helpers below go through it.

**The panel's own fill has to be translucent for a material to be visible
at all.** GPUI renders the panel `div`'s background on top of the native
material view in window z-order — a fully opaque fill paints over it
completely regardless of the native view's own z-position. `panel::Root`
now carries a `translucent: bool` (set once, from `material::install`'s
result, threaded through `Root::new`): `true` picks
`theme::SURFACE_PANEL_TRANSLUCENT` (`SURFACE_PANEL`'s own RGB at reduced
alpha); `false` (the final opaque fallback) picks full-alpha `SURFACE_PANEL`
plus a hairline border (`theme::BORDER_HAIRLINE_STRONG`) for edge definition
against an arbitrary desktop, per the design report's own explicit fallback.

**Licensing/dependency footprint**: `objc2`, `objc2-app-kit`,
`objc2-quartz-core`, `raw-window-handle` added as direct dependencies of the
`neko` crate (macOS-only target block) — all four already transitive
dependencies of `gpui`/`neko-core` at these exact versions, so this added
**zero new crate versions** to the tree at the time this task ran. Every one
of *these four* is MIT/Apache/Zlib. **"The tree stays GPL-free" is no
longer true as of the 2026-08-20 fork migration** ("The GPUI dependency
decision" below) — unrelated to this task's own four crates, but
`docs/evidence/cargo-tree.txt`/`cargo-license.txt` now reflect the current,
post-migration tree (GPL-3.0-or-later present via `gpui`'s own dependency
chain), not the GPL-free state this paragraph describes.

**Blur is binary and belongs to the material; transparency is continuous and
belongs to the theme.** `NSGlassEffectViewStyle` has exactly two values —
`.regular` and `.clear` — so there is no "a bit more frosted" at the material
level, and `.clear` was tried live and read as far too little. Everything
between is `panel_alpha` on the palette. The shipped `neutral` value is
**0.0**: the panel paints no fill of its own at all, and what you see is
purely `NSGlassEffectView`. The rounded shape survives because it comes from
the material view's own `cornerRadius`, not from the `div`'s fill.

**The consequence, stated rather than discovered later:** at zero fill,
nothing but the glass sits between the desktop and the text, and
`themes_all_pass_wcag_aa` **cannot see this** — it measures text against the
opaque `surface_panel`, which no longer renders. Legibility over a light or
busy wallpaper is now entirely the material's job. If it ever fails there,
the fix is a small non-zero alpha (0.08–0.12 reads as no tint but restores a
floor), not a different material.

**Forcing a specific fallback branch for verification** (never set in
normal operation): `NEKO_FORCE_MATERIAL=popover` skips the
`NSGlassEffectView` branch even though the class is available (runs the
exact same code the real "class lookup failed" branch would); `=opaque`
fails `install` outright. Both are read by `material.rs` itself
(`parse_forced_fallback`, unit-tested).

**Verification/evidence-only tooling**, `crates/neko/src/evidence.rs`, each
hook gated on its own unset-by-default env var — none of this runs in
normal operation: `NEKO_SHOW_ON_LAUNCH=1` shows the summon panel
immediately (skips hotkey/onboarding) and prints the real `NSWindow`'s
`windowNumber` and on-screen point rect to stderr, for driving
`screencapture -l<windowid>` (window-scoped capture) from outside the
process; `NEKO_BENCH=<n>` re-measures warm summon latency `n` times using
`material::order_front_regardless`/`order_out` (direct `NSWindow` ordering,
bypassing `Window::activate_window`/`cx.hide()` so a long run doesn't
repeatedly steal focus) instead of synthetic OS keystrokes (unreliable —
see "Summon latency" below) or repeated `cx.activate(true)`;
`NEKO_BACKDROP_IMAGE=<path>` opens a second, full-display window at
`NSNormalWindowLevel` (strictly below the summon panel's own
`NSPopUpWindowLevel`, so it never needs explicit ordering) showing the given
image — for local, interactive checking, never for unattended evidence
capture (see the standing rule below).

**Standing rule, decided after a real near-miss and binding on every future
task in this repo: never run `screencapture -x` (full-screen) or
`screencapture -R<rect>` (region) on this machine, for any reason,
including "just to prove compositing."** `screencapture -l<windowid>`
(strictly window-scoped — the OS composites only that window's own layer
tree, nothing behind it) is the only screen-capture form that's safe here.
This machine runs the captain's live private work and other agents'
sessions concurrently with whatever task is running; a region/full-screen
grab reads the real, literal screen buffer, and there is no reliable
in-process way to guarantee your own synthetic content is actually the
frontmost thing painted there at the exact instant you capture — a
same-machine attempt at this during this task proved it isn't safe even
with a synthetic full-display backdrop window and a pre-capture delay: one
`-R` capture landed mid-race and briefly wrote an unrelated real window's
private content to a temp file instead (deleted immediately; nothing
committed). No amount of extra polling before the capture makes this an
acceptable risk to take for evidence — treat it as categorically off the
table, not a tradeoff to re-litigate per task.

**This is exactly why `screencapture -l<windowid>` cannot be used to prove
live `BehindWindow` vibrancy compositing** (report §6, confirmed
independently): it renders the window's own content in isolation, none of
the live backdrop blend — a real technical limitation, not a missing flag.
**The resolution used here instead of a screenshot: a non-visual, in-process
readback.** `material::verify_installed(window, installed)` re-derives
`window.contentView()` fresh after `install` returns `Ok`, reads back the
*actual* live view AppKit is holding — its concrete class (downcast,
`Retained<NSView>::downcast::<NSGlassEffectView>`/`::<NSVisualEffectView>`,
not a name-string guess), its material-specific properties (`style`/
`cornerRadius` for Glass; `material`/`blendingMode`/`state`/
`layer.cornerRadius`/`layer.masksToBounds` for Popover), and its position in
`contentView.subviews()` (index 0 — bottommost, confirming it's a sibling of
GPUI's own rendering view, not swallowed by or swallowing it) — and asserts
every value against what `install` was supposed to have set, returning
`Err` with exactly what didn't match on any mismatch rather than trusting
the setter calls silently took effect. Called unconditionally from
`main.rs` after every real `install`, not just in an evidence run, logging
the readback to stderr (`neko: material verified: …` /
`neko: material readback verification FAILED: …`) — this is a permanent
runtime sanity check now, not one-off proof. **The compositing mechanism
itself** — that a genuinely live `BehindWindow` material shifts hue with
whatever is behind it, with an opaque control that doesn't move — **is
independently proven for this exact code path by
`data/neko-native-material/report.md` §2's own red/blue differential test**
(firstmate verified those numbers directly); that result is cited here, not
re-run, per the standing rule above. What *is* fresh for this task: the
window-scoped screenshots in `docs/evidence/material-{glass,popover-
fallback,opaque-fallback}-window-scoped.png` (each fallback branch renders
correctly, no crash, correct corner radius, correct hairline border on the
opaque fallback) and the readback verification transcripts.

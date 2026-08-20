# The "black tent" halo — root-caused to the panel div's own `.shadow_lg()`, not the native window shadow

`fm/neko-panel-shadow-tent`, `main` at `08a07a1`. Third report of the same
symptom: *"It still seems like a black tent that used to be there, still
there, dude."* The captain was seeing this on a binary that already
contains `fm/neko-double-panel`'s fix (`material::disable_native_shadow`,
verified live by readback) — so either that fix never addressed the real
cause, or there were two overlapping dark surfaces and only one went away.
It was the latter.

## Hypothesis

`panel::Root::render`'s panel `div` carries `.shadow_lg()` — GPUI's own
box-shadow, painted into the scene, entirely separate from the native
`NSWindow` shadow `fm/neko-double-panel` disabled. `git log -S "shadow_lg"`
shows it's been there since the original spine commit (`8888e33`), and for
most of the project's life the real `NSWindow` was exactly the panel's own
width, so the shadow's blur had nowhere to spill (the window's own edge cut
it off). Since `235bf88` ("Mode view resize seam") made the window
permanently `PANEL_WIDTH_WITH_DETAIL_PX` (760) while the root-list panel
stays the narrower `PANEL_WIDTH_PX` (680), there's `theme::
PANEL_ROOT_INSET_PX` (40pt) of real, otherwise-empty transparent window on
each side for that same shadow to bleed into.

## Single-variable test — the falsifiable prediction

Commented out `.shadow_lg()`, changed nothing else, rebuilt, and compared a
window-scoped capture of the empty-query root panel against a plain dark
backdrop, before and after, pixel-for-pixel.

**Setup**: `crates/neko-daemon/src/bin/verify_harness.rs` (never the real
`neko-daemon` — its clipboard capture loop polls the systemwide pasteboard
regardless of `HOME`) under an isolated `HOME`, an obscure hotkey committed
before any client connected, and `crates/neko/src/evidence.rs`'s own
`NEKO_SHOW_ON_LAUNCH=1` + `NEKO_BACKDROP_IMAGE=<solid dark PNG>` hooks — no
query typed, no results, chrome only, exactly the "clean read" the launch
brief asked for. Captured with `screencapture -l<windowID>` (window-scoped
only, per the standing rule). No synthetic mouse/keyboard input anywhere in
this investigation.

**Method**: rather than eyeball the composited image (a translucent glass
panel over a near-black backdrop makes a faint halo hard to judge by eye),
the screenshot's own **alpha channel** was read directly. The window is
`WindowBackgroundAppearance::Transparent`, so a window-scoped PNG capture
preserves per-pixel alpha: `0` where nothing painted (the true, invisible
margin), `255` where the opaque panel fill is, and anything in between is
exactly the shadow's own blur/spread — no ambiguity from whatever the
backdrop image happens to look like.

### Before — `.shadow_lg()` present

Window rect (from the evidence hook): origin `368,218`, size `760×421`pt.
Capture: `1520×842`px (confirms the real `2.0` backing scale factor).
`docs/evidence/panel-shadow-tent-before.png`; alpha channel alone, boosted
6x for visibility, in `panel-shadow-tent-before-alpha-boosted.png`.

- At mid-height, alpha rises smoothly from `0` (window's own left edge) to
  `17/255` (~6.7%) immediately next to the panel's own opaque left edge
  (`x=80px = 40pt`, exactly `PANEL_ROOT_INSET_PX`) — a soft gradient, not a
  hard clip, filling roughly the outer two-thirds of the 40pt margin
  (nonzero from ~12pt out to the panel's own edge).
- Peak alpha anywhere in the margin (excluding the 1-2px window-edge
  anti-aliasing artifact at `y=0`): `15/255` (~5.9%) at a representative
  interior point; corner regions run a little higher, consistent with two
  overlapping blur radii (`shadow_lg`'s two `BoxShadow` layers) pooling
  near the rounded corner.
- This is faint by the numbers, but real, low-alpha black tinting is
  exactly a soft ambient halo — over the captain's own real desktop
  (unlike this test's uniform dark backdrop), a smooth ~6% black gradient
  framing an otherwise-invisible margin reads as a dark, tent-shaped aura
  around the panel, which is exactly the "black tent" description.

### After — `.shadow_lg()` removed

Identical setup, one line changed. `docs/evidence/panel-shadow-tent-after.png`;
alpha in `panel-shadow-tent-after-alpha-boosted.png`.

- Alpha is **exactly `0`** for every pixel in `x ∈ [0, 79]` at mid-height
  (and confirmed elsewhere in the margin) — a hard, clean jump straight
  from `0` to `255` at `x=80px`. No gradient, no partial alpha anywhere
  outside the panel's own opaque fill.

**The prediction held.** This confirms the hypothesis: `.shadow_lg()` was
the real, remaining source of the halo. `fm/neko-double-panel`'s own fix
(disabling AppKit's automatic window shadow) was correct and necessary —
it removed a real, independent surface — but it was never sufficient on
its own, because GPUI's own box-shadow, drawn as real scene pixels rather
than a native window property, was a second, overlapping source the same
architectural change (`235bf88`'s wider, permanently-fixed window) exposed.

## The fix

`.shadow_lg()` is removed outright, not shrunk or replaced. Reasoning,
weighed against the three options the launch brief posed:

- **Re-enabling the native window shadow, constrained to the panel** was
  already ruled out by `fm/neko-double-panel`'s own investigation — AppKit
  computes its automatic shadow from the whole `NSWindow` frame, and it
  cannot be shaped around the panel's own alpha since this window's entire
  visible surface is one Metal-layer-backed `NSView` with no per-region
  shadow API. Nothing about this task changes that.
- **Shrinking `shadow_lg` further** doesn't fix the actual problem.
  `shadow_lg` was already fully contained within the 40pt margin before
  this fix (alpha reaches `0` well inside the window's own edge, per the
  measurements above) — it was never *too big*, it was *present at all* in
  a region every other decision in this codebase already treats as
  strictly, invisibly transparent. The native backdrop material is
  deliberately narrowed to the panel's own width, not the window's
  (`material::set_background_frame`); the native auto-shadow was disabled
  specifically because it painted into this same margin
  (`docs/evidence/double-panel-shadow-fix-report.md`). A smaller version of
  `shadow_lg` would still be the one remaining thing painting there,
  however faint — still visible, just a smaller tent.
- **Dropping it entirely** is the only option consistent with that
  invariant, and it doesn't leave the panel undefined: the translucent
  Liquid Glass material (`self.translucent`, installed by `material::
  install`) already reads as a distinct, elevated surface through real
  vibrancy/refraction against the backdrop — genuine native material, not a
  drawn approximation of one. The opaque fallback path (native material
  install failed) keeps its existing `.border_1().border_color(theme::
  BORDER_HAIRLINE_STRONG)`, which was already there specifically for edge
  definition and has zero spill risk (a 1px border painted at the panel's
  own edge, no blur). Neither rendering path loses a "this is a distinct
  surface" cue.

The frozen mockups' own `--shadow-panel` CSS token (`data/neko-design/
mockups/design.css`, in the firstmate home) specifies a large ambient
shadow (`0 24px 70px -12px rgba(0,0,0,.65), 0 6px 20px -4px rgba(0,0,0,.5)`)
— but that token was authored for a static HTML mockup with no real OS
window boundary at all, where the shadow simply blends into an
infinite page background. It predates, and doesn't account for, the real
window's own fixed-width architecture and the deliberate "margin paints
nothing" invariant that architecture requires (`235bf88`, `fm/neko-double-
panel`). Once real native vibrancy was confirmed reachable and shipped
(`AGENTS.md`, "Window material" — Direction 1, "System Vibrancy," is what's
actually built), that native material became the elevation cue the frozen
mockup's synthetic box-shadow was always standing in for.

## Both modes checked

- **Root list (680pt panel, 40pt margin each side)** — the case this whole
  investigation is about. Confirmed fixed by the alpha-channel test above.
- **Clipboard mode (760pt panel, 0pt margin)** — `panel::Root::render`
  computes `margin_width = (PANEL_WIDTH_WITH_DETAIL_PX - panel_width) / 2.0`;
  when `panel_width == PANEL_WIDTH_WITH_DETAIL_PX` (detail mode,
  `mode.chrome.has_detail`), this is exactly `0`. `render_dismiss_margin`
  renders a genuinely `0`-width div in that mode — there has never been
  room for `shadow_lg` to spill in clipboard mode, before or after this
  fix, for the same reason the whole app was immune to this bug before
  `235bf88` (window width == panel width, nowhere for the blur to go). The
  fix removes `shadow_lg` unconditionally, with no mode-specific branch, so
  clipboard mode is affected identically to the root list: it had no
  shadow-driven halo before this fix (nothing to regress) and has no drawn
  shadow at all after it (same as the root list). This was confirmed
  against the daemon directly — a raw `Request::Search` for `"clipboard"`
  against the isolated harness returns the `Clipboard History` command as
  the top result with `enters_mode: "clipboard"`, exactly as `AGENTS.md`'s
  "Commands and modes" section documents — so the mode-entry *logic* this
  reasoning depends on is real and correct, not assumed.

**A live window-scoped screenshot of clipboard mode specifically was
attempted and not obtained this task**, for a reason unrelated to this
fix: `evidence.rs`'s `NEKO_SHOW_QUERY`/`NEKO_SHOW_CONFIRM` path (the only
non-synthetic-input hook capable of driving a mode transition) held the
summon window in an *active* state for several seconds longer than the
plain `NEKO_SHOW_ON_LAUNCH`-only path does, and on this shared, multi-agent
machine that longer exposure reliably lost real window activation to
contention before the capture — confirmed directly, not inferred: a
temporary diagnostic (`window.is_window_active()`, reverted before commit)
read `false` at capture time on every attempt, including after an explicit
re-`activate_window()`/`cx.activate(true)` immediately before capture,
which also read `false` moments later. A second temporary diagnostic
(logging the daemon's own `Response::Search`, also reverted) confirmed the
underlying application state was completely correct throughout — `"Safari"`
resolved to the `Safari` app as the top match, and `"clipboard"` resolved
to the `Clipboard History` command — the loss is specific to the OS-level
window-activation/paint-compositing step this shared environment couldn't
reliably hold long enough for a screenshot, not to any application logic
this task touched. Ruled out before concluding this: display sleep (kept
awake with `caffeinate -u` for the client's entire lifecycle, not just
around the capture), timer starvation (confirmed the same result after a
25-second wait), and the backdrop window (reproduced identically with it
absent). Neither `evidence.rs` nor any other file besides `panel.rs` was
changed by this task — the diagnostic edits described above were made,
tested, and reverted (`git checkout -- crates/neko/src/evidence.rs`)
in-session, never committed.

## Verification

- `cargo build`, `cargo test --workspace` (242 tests, 0 failed), `cargo
  clippy --workspace --all-targets` all clean at the workspace root, on the
  release binaries.
- Single-variable test above: falsifiable prediction stated in advance,
  confirmed by measurement (alpha channel, not eyeballing).
- Verified on `target/release/neko` against `verify_harness` (never the
  real `neko-daemon`), isolated `HOME`, obscure committed hotkey, no
  synthetic input anywhere.
- Not attempted: full live re-measurement of warm summon latency / daemon
  idle memory. This fix touches one style call on one `div` in the client's
  render path — no daemon code, no window sizing, no allocation pattern —
  so no latency or memory regression is expected, but it was not
  re-measured this task given the same shared-machine contention documented
  above made even the simpler bench hooks (`NEKO_BENCH`) show every
  `on_next_frame` callback silently not firing during this session (a
  symptom of the same activation/compositing contention, not evidence of
  an actual regression — `AGENTS.md`'s own "Summon latency" section already
  tracks the fork's real, separate ~26–40ms regression under a parallel
  task, `neko-fork-summon-latency`, which owns re-measurement).

# Menu frost backdrop + results-list edge fade — `fm/neko-frost`

Scope: a native frosted backdrop for the `⌘K` actions menu, and a real
scroll edge fade for the results list — both requested directly by the
captain after seeing comet's `frost.rs`/`edge_fade.rs`. Built against the
published `gpui = "0.2.2"` crate (no `paint_backdrop_blur`/`EdgeFade`
primitives available — confirmed by direct grep of the vendored crate
source, same finding `data/neko-comet-design/report.md` §1 already made).
Rebased onto `fm/neko-craft-pass`'s actions-menu floating-layer landing
(`a19cffb`, `deferred`/`anchored`/`snap_to_window_with_margin`/`occlude`)
per the launch brief's own instruction to hook into that work rather than
invent a second positioning path.

## 1. The menu frost backdrop

**Mechanism**: a second, menu-scoped native material view
(`material::install_menu_overlay`) — same `NSGlassEffectView` →
`NSVisualEffectView(.popover)` fallback chain as the whole-window material
(`material.rs`'s pre-existing `install`), same sibling-below-GPUI's-
rendering-view invariant. Installed once, hidden and zero-sized, right
after the whole-window material install/verify succeed; shown and
repositioned every time the menu opens (`material::show_menu_overlay`,
called from `menu_frost::MenuFrostSync` — a thin `Element` wrapper that
reads the menu card's own real, finished `Bounds<Pixels>` at **paint**
time, after `anchored()`/`snap_to_window_with_margin` have already resolved
its position, and pushes that straight to the native view) and hidden on
every close path (`Root::close_actions_menu`, now the single place
`actions_menu` is ever cleared — audited: `reset_for_summon`, `enter_mode`,
`exit_mode`, `confirm_menu_action` (both branches), `handle_dismiss`, and
`close_actions_menu_from_outside_click` all route through it).

Corner radius (`theme::ROW_RADIUS_PX`, 8pt) is baked in at the overlay
view's own creation, matching the menu card's `.rounded(px(theme::
ROW_RADIUS_PX))` — the same "background view's rounding must agree with
GPUI's `.rounded()`" rule `material.rs`'s whole-window `CORNER_RADIUS_PT`
already established, just at the menu's own smaller radius.

**Y-axis flip, new to this task**: GPUI's paint-time `Bounds<Pixels>` are
top-left-origin, y-down; AppKit's `contentView` (the parent both the
whole-window and menu-overlay material views are inserted into) is
unflipped, bottom-left-origin, y-up. The whole-window backdrop never needed
to convert (it always spans the full content height, so `y=0` is correct in
both systems by construction — `set_background_frame`'s own doc comment
already noted this). The menu overlay is a genuine sub-rectangle, so
`show_menu_overlay` does the real conversion: `appkit_y = content_height -
(gpui_y + height)`, read back live off `contentView.bounds()` rather than a
hard-coded constant.

**The menu's own GPUI fill** switches from opaque `SURFACE_RAISED` to a
translucent `theme::MENU_GLASS_TINT` (`SURFACE_RAISED`'s RGB at 0.62 alpha
— calibrated against comet's own reference tint, `oklch(0.33 0 0 / 34%)`,
read from `refs/comet/crates/ui/src/theme.rs` for inspiration
only, scaled up since this app has no true blur behind that tint to share
the contrast duty) whenever the overlay installed (`Root::menu_frost`).
Falls back to the original fully-opaque fill whenever it didn't — either
because the whole-window material itself failed (`translucent == false`,
`NEKO_FORCE_MATERIAL=opaque`) or the menu-specific overlay install itself
errored independently.

### The honest limitation — read before extending this

GPUI exposes **exactly one** rendering `NSView` for the whole window.
There is no way to insert a native compositing step *between* two portions
of GPUI's own single paint pass. This backdrop — like the whole-window one
before it — can only ever reveal what's genuinely **behind the window**
(the desktop, or another window below it in the OS's own z-order), never
GPUI's own already-painted content sitting in front of it in the same
scene. Concretely:

- Wherever the menu overlaps content GPUI left translucent (true for
  nearly all of an unselected row's own footprint — `panel::render_row`
  gives no `.bg(...)` to an unselected row at all, letting the panel's own
  translucent fill show through already), the native material genuinely
  shows through and the result is a real, if uniform, frosted-glass look.
- Wherever the menu overlaps something GPUI painted opaquely (the one real
  case in this app: `theme::SURFACE_SELECTED`, a selected row's highlight),
  the visible result is the menu's own translucent fill blended with that
  *already-rasterized* pixel, within GPUI's own draw pass — a translucent
  tint of it, not a gaussian blur of it. The native view underneath is
  irrelevant there; GPUI's own opaque output fully occludes it at the OS
  compositing level regardless of what the native view is doing.

This is exactly the gap `data/neko-comet-design/report.md` §1 already
named: comet's actual mechanism (`window.paint_backdrop_blur`, a real
in-scene sample-and-blur primitive) is a renderer feature, not a UI
pattern, and isn't in the published `gpui` crate neko depends on. Closing
it for real needs either that primitive (which means the git fork and its
GPL-3.0 consequence — see `AGENTS.md`'s "The GPUI dependency decision") or
a custom Metal-level compositing pass inside neko itself. Neither was
attempted here, per the brief's own "get as close as possible to the
result without the mechanism" framing.

**Known follow-up, explicitly flagged, not lost**: a parallel task
(`fm-neko-gpui-fork-migration`) is evaluating a move to a `gpui` fork
(`wingleeio/zed`) that *does* ship `paint_backdrop_blur`. If that migration
lands, a future task should compare this native-AppKit approach against
using that primitive directly and keep whichever reads closer to comet's
own result — this file's own honest-limitation section above is the
starting point for that comparison, not a reason to consider this task's
approach obsolete on its own.

## 2. The results-list edge fade

`edge_fade.rs` — `scroll_edge_fade` wraps the clipboard-mode list (now
genuinely scrolling, `ScrollHandle`/`track_scroll`, replacing the old
budget-fit-and-truncate `fit_mode_list`) with a paint-time overlay
gradient, gated each frame on the scroll handle's own `offset()`/
`max_offset()` — never a render-time decision, since the child's own
`prepaint` (called first) is what clamps the offset for a frame where the
list's content just shrank (e.g. a `⌘K` delete near the bottom of a
scrolled list). Two plain `window.paint_quad` calls with a
`gpui::linear_gradient` fill — no scene primitive needed, no hitbox, so it
never intercepts a row click.

Verified visually (see screenshots below) that the fade is a genuine
top-to-bottom dissolve of the row directly under it (icon glyph and badge
both visibly dimmer at the very top edge) — subtle by design (the fade
color is the same translucent panel background the row would otherwise sit
against, not a bright highlight), matching "rows dissolve rather than
being sliced by a hard edge," not a decorative smudge.

## 3. Screenshots (window-scoped, `screencapture -l<windowID>`)

- `menu-frost-root-list-open.png` — actions menu open on a clipboard row in
  the root list (680px panel), normal position. Frost is visibly distinct
  from the flat panel background — a soft vertical texture/vignette inside
  the menu card that the surrounding opaque rows don't have.
- `menu-frost-clipboard-mode-edge-clamp.png` — actions menu open inside the
  wider clipboard-history mode (760px panel, no side margin), clamped by
  `snap_to_window_with_margin` against the real window edge — visibly
  overlapping the detail pane's own "Copied"/"Application" text at its
  clamped position, same frosted treatment.
- `edge-fade-scrolled-top-fade-visible.png` — the mode list scrolled to its
  own bottom; the top row visibly dissolves into the background (only the
  top fade shows, since there's nothing below to scroll to).
- `edge-fade-short-list-no-fade.png` — a one-entry mode list, at rest — no
  fade at either edge, since there's nothing scrolled out of view.
- `menu-frost-fallback-opaque.png` — `NEKO_FORCE_MATERIAL=opaque` (the
  whole-window material install forced to fail): the panel falls back to
  its plain opaque fill with a hairline border, and the actions menu falls
  back to its original fully-opaque `SURFACE_RAISED` card — no menu-overlay
  install is even attempted (`main.rs` gates it on `translucent`), and no
  error is logged for it, confirmed by the client's own stderr.

Captured via `verify_harness` (never the real `neko-daemon` — its clipboard
capture loop polls the systemwide pasteboard regardless of `HOME`) under an
isolated `HOME=/tmp/neko-frost-verify-home`, an obscure committed hotkey,
and `evidence.rs`'s existing `NEKO_SHOW_QUERY`/`NEKO_SHOW_CONFIRM`/
`NEKO_SHOW_ACTIONS_MENU`/`NEKO_SCROLL_MODE_LIST_TO_BOTTOM` hooks (the first
three are `fm/neko-craft-pass`'s own; the fourth this task's) — no synthetic
input anywhere. `verify_harness`'s `NEKO_VERIFY_SEED_CLIPBOARD_COUNT=<n>`
(added this task) seeds enough distinct clipboard fixtures for the mode
list to genuinely overflow its viewport, which is what makes the top-edge
fade screenshot non-decorative.

**A real, reproduced flakiness while capturing, worth recording rather than
silently retried away**: on this shared machine (multiple concurrent agent
sessions, load average 11.84 over 15 min at the time), a `NEKO_SHOW_CONFIRM`
run intermittently printed `neko: summon window lost activation, hiding`
early in the sequence and the captured frame showed the pre-confirm state
even though `show_once`'s own re-activation-before-capture logic ran and
printed a window number — reproduced with `NEKO_SHOW_CONFIRM` alone (no
actions-menu hook involved). `AGENTS.md`'s "Comet craft pass" section
already documents the most likely root cause for exactly this class of
capture flakiness on an unattended machine: the display's own idle sleep
timer can leave `ScreenCaptureKit` unable to start a capture stream at all,
and a window's content can stay stale in the compositor even after the
display wakes until some new state change repaints it —
`caffeinate -u -t <seconds>` before launching the client is the documented
fix. Not confirmed as the specific cause here (not re-diagnosed via `log
stream` this pass), but consistent with every symptom observed — so this is
a pre-existing capture-path condition on this machine, not something this task's
changes introduced. A retry of the identical command succeeded cleanly
every time. Worth noting for whoever next captures evidence on a busy
machine: if a `NEKO_SHOW_CONFIRM` capture shows the pre-confirm state
despite `show_once` completing normally, just retry.

## 4. Idle CPU and warm summon latency

Measured twice: first against the published `gpui = "0.2.2"` crate (before
`main` migrated to the `wingleeio/zed` fork, §6), then re-measured against
the fork build after rebasing, since a dependency swap that size can and
did move this number — see below. Both passes: release binary,
`verify_harness` backing it, isolated `HOME`, on a shared machine with
several other concurrent agent sessions running (load average ~11.8 over
15 min at the first measurement — noted since it inflates both numbers
somewhat above what a quiet machine would show).

- **Idle CPU** (panel visible via `NEKO_SHOW_ON_LAUNCH`, nothing open, no
  menu, no scrolling — 8 samples of `ps -o %cpu`, 2s apart): **0.5%–0.9%**
  on the published-crate build, **0.1%–1.0%** on the fork build after
  rebasing (mean ≈0.64% / ≈0.44%) — no regression either way. No new
  per-frame work exists in either feature at idle: the menu overlay is only
  ever touched from `MenuFrostSync::paint`, which is only mounted while
  `actions_menu.is_some()`; the edge fade is only ever mounted inside
  `render_mode_list`, which only exists while a mode is active. Neither is
  in the tree at all in the idle/default state, so idle cost is unchanged
  from before this task on either dependency.
- **Warm summon latency** (`NEKO_BENCH`, `order_front_regardless`/
  `order_out`, excluding the first/cold summon): **1.2–4.6ms** (mean ≈2.2ms,
  11 samples) on the published-crate build — within the existing 4–6ms
  baseline, no regression from this task. **25.8–40.6ms (mean ≈32.9ms, 15
  samples) on the fork build after rebasing** — this is *not* a regression
  this task introduced: it's the exact, already-documented,
  not-yet-root-caused regression `AGENTS.md`'s own "Summon latency" section
  records for the fork migration itself (`fm/neko-gpui-fork-migration`,
  measured there at ~26–40ms, confirmed not to be shared-machine noise by
  an interleaved A/B against the published crate on the same machine). This
  task's own before/after pair on identical code either side of the
  dependency swap — 2.2ms mean → 32.9ms mean, no code of this task's own
  changed between those two measurements — independently reproduces that
  same finding rather than just citing it. Unlike the published-crate build
  (cold summon ~45ms, warm 1.2–4.6ms — a clear cold/warm split), the fork
  build's own first sample (31.3ms) sits inside the same 25.8–40.6ms band as
  every later one — no distinct cold-start spike, matching `AGENTS.md`'s own
  "no warm-up trend" description of this regression.

## 5. Where this still differs from comet, and what closing it costs

- **No true blur** — see §1's "honest limitation" above. Comet samples and
  blurs its own already-composited scene content; this reveals native
  AppKit material wherever GPUI leaves a spot translucent, and a plain
  alpha tint everywhere else. The visual result is close for the common
  case (menu over unselected rows) and diverges for the one opaque case
  (menu over a selected row's highlight). Cost to close: a `gpui` fork with
  `paint_backdrop_blur` (GPL-3.0 consequence, `AGENTS.md`) or a custom
  Metal compositing pass in neko itself (a compositor feature).
- **No animated open/close for the frost specifically** — the menu's own
  fade-in (`motion::menu_fade_in`, `fm/neko-craft-pass`) already covers the
  card's appearance; the *native* material view has no animation of its
  own (`setHidden(false)`/`setFrame(...)` are instant). Comet's forked gpui
  can animate `paint_backdrop_blur`'s own parameters directly; a
  native-AppKit view could in principle be animated too (`NSAnimationContext`),
  not attempted here since the card's own fade already reads as the "open"
  motion and a second, separately-timed animation risked looking busy
  rather than more polished.
- **Uniform tint, not scene-aware** — comet's blur genuinely differs based
  on what's behind the card each frame (a busy backdrop blurs more visibly
  than a plain one); this native material always renders the *same*
  material regardless of GPUI's own content, since it can't see it. Not
  fixable without the same missing primitive.

## 6. Rebased onto the gpui fork migration (`a83d5da`) — still shipping the native path

Mid-task, `main` moved from the published `gpui = "0.2.2"` crate to
`wingleeio/zed`'s fork (`AGENTS.md`, "The GPUI dependency decision," rewritten
by that migration). **This task's own decision, confirmed by the captain: keep
shipping the native AppKit frost built here, do not switch to the fork's
`paint_backdrop_blur` in this task.** Rebased cleanly onto it; two small API
drifts fixed (`ScrollHandle::max_offset()` now returns `Point<Pixels>` instead
of `Size<Pixels>` — `edge_fade.rs` reads `.y` instead of `.height`; the
`NEKO_SCROLL_MODE_LIST_TO_BOTTOM` evidence hook still used the pre-migration
free `Timer::after`/a `let _ = cx.update(...)` the rest of `evidence.rs` had
already converted to `cx.background_executor().timer(...)`/a bare
`cx.update(...)`, per the fork's `AsyncApp::update` returning `R` directly now
instead of `Result<R>`). `cargo build`/`test`/`clippy` all clean afterward;
material install, menu-overlay install, and their readback verifications all
still succeed identically on the fork build (confirmed live, release binary,
`docs/evidence/menu-frost-fork-build-verification.png` — pixel-identical to
the pre-migration capture). One capture-time observation: `screencapture -l`
was intermittently unable to grab the window on this specific build on the
first few attempts (`could not create image from window`) despite the process
being alive and the window number valid, clearing on retry — plausibly the
same kind of shared-machine/activation-timing flakiness §3 already documents,
not reproduced as a *build*-specific regression (a plain, non-evidence launch
stayed capturable throughout a 15s poll on the same build).

**The concrete follow-up this section exists to set up**: now that the fork
is `main`'s own dependency, `window.paint_backdrop_blur` is real and present
at `crates/gpui/src/window.rs:3992` (its own doc comment: "everything already
painted beneath `bounds` is snapshotted and painted back gaussian-blurred...
macOS Metal only... content painted after this call composites on top of the
blur") and `EdgeFade` at `crates/gpui/src/window.rs:682` (its own doc
comment names exactly the problem this task's `edge_fade.rs` also solved:
"Built for scroll-edge fades over translucent/blurred window backgrounds,
where a backdrop-colored gradient overlay cannot exist"). A future task
should build the `paint_backdrop_blur` version of the menu frost and compare
it against this native-AppKit one head-to-head — genuine scene-aware blur
that reacts to whatever's actually behind the menu (including GPUI's own
opaque content, §1's "honest limitation" this native approach can't reach)
vs. this task's zero-new-dependency, works-on-either-crate native material —
and keep whichever reads closer to comet's own result. `gpui::EdgeFade`
could similarly replace `edge_fade.rs`'s hand-rolled paint-time gradient
outright; that swap is far lower-risk than the blur one (no native bridging
involved either way) and could reasonably happen as part of the same
follow-up or independently.

## 7. Verification

- `cargo build`, `cargo test`, `cargo clippy` — clean at the workspace root
  (`cargo build --workspace`, `cargo test --workspace`, `cargo clippy
  --workspace --all-targets`, all after rebasing onto `fm/neko-craft-pass`'s
  landing).
- Fallback path exercised live (`NEKO_FORCE_MATERIAL=opaque`) and shown to
  degrade gracefully — §3 above.
- Verified on release binaries (`cargo build --release --workspace`).

## Menu frost backdrop and the results-list edge fade

`fm/neko-frost`, requested directly by the captain after seeing comet's
`frost.rs`/`edge_fade.rs`: *"comet's frost is much more elegant."* Rebased
onto `neko-craft-pass`'s floating-layer landing above (its own `deferred`/
`anchored`/`.occlude()` menu positioning), then again onto the `gpui` fork
migration below (two small API drifts — `ScrollHandle::max_offset()` now
`Point<Pixels>` not `Size<Pixels>`; `Timer::after` → `cx.background_executor()
.timer(...)`). Full writeup, screenshots, idle CPU/latency numbers, and the
honest gap vs. comet: `docs/evidence/menu-frost-and-edge-fade-report.md`.
Summary:

**The results-list edge fade is straightforward** — `edge_fade.rs`'s
`scroll_edge_fade` wraps the now-genuinely-scrolling clipboard-mode list
(`ScrollHandle`/`track_scroll`, replacing the old budget-fit-and-truncate
`fit_mode_list`) with a paint-time overlay gradient (two `window.paint_quad`
calls, `linear_gradient`), gated each frame on the scroll handle's own
`offset()`/`max_offset()` so it only ever shows where content is genuinely
scrolled out of view. No `gpui` primitive needed for this one.

**The menu frost backdrop is the load-bearing invariant to understand before
touching this area again.** GPUI (both the published crate this app used to
depend on and, as of the fork migration below, the one it depends on now)
exposes exactly **one** rendering `NSView` for the whole window — there is no
way to insert a native compositing step between two portions of GPUI's own
single paint pass. `material::install_menu_overlay` is a *second*,
menu-scoped native material view (same `NSGlassEffectView` →
`NSVisualEffectView(.popover)` chain as the whole-window one, same
sibling-**below**-GPUI's-rendering-view placement — the invariant
`material.rs`'s own top doc comment already states and that has cost this
project real time before), sized/positioned every frame to the menu card's
*real, finished* paint-time bounds (`menu_frost::MenuFrostSync`, a
layout-transparent `Element` wrapper — reads `Bounds<Pixels>` at `paint()`
time, after `anchored()`/`snap_to_window_with_margin` have already resolved
the card's position, never a value computed independently). Because it's
still sibling-below, it carries the **same fundamental limitation** the
whole-window material always had: it can only reveal what's genuinely
**behind the window** (the desktop, or another window below it), never
GPUI's own already-painted opaque content sitting in front of it in the same
scene (concretely: a selected row's `SURFACE_SELECTED` highlight under the
menu blends as a translucent tint within GPUI's own draw pass, not a true
blur of it — everywhere else, where GPUI left content translucent, as most
of an unselected row's own footprint already is, the native material
genuinely shows through). Closing that gap needs either a real in-scene
backdrop-blur primitive or a custom Metal compositing pass — see the fork
note just below for where the former now actually exists.

**Every close path for the `⌘K` menu goes through one function,
`Root::close_actions_menu`** — the same discipline `AGENTS.md`'s other
single-chokepoint patterns already establish elsewhere in this codebase —
specifically so the native overlay is always hidden in lockstep with
`actions_menu` being cleared; a menu that closes by unmounting
`menu_frost::MenuFrostSync` (its own paint stops running, so it has no
"hide" branch to fall into) would otherwise leave a stale blurred patch on
screen. If you add a new way to close the menu, route it through this
function, not a bare `self.actions_menu = None`.

**Concrete follow-up, not just a note-to-self**: `main` now depends on the
`wingleeio/zed` `gpui` fork (below) specifically *because* it ships real
backdrop-blur/edge-fade primitives — `window.paint_backdrop_blur`
(`crates/gpui/src/window.rs:3992`, its own doc comment: "everything already
painted beneath `bounds` is snapshotted and painted back gaussian-blurred...
macOS Metal only") and `gpui::EdgeFade` (`crates/gpui/src/window.rs:682`,
its own doc comment names this exact problem: "Built for scroll-edge fades
over translucent/blurred window backgrounds, where a backdrop-colored
gradient overlay cannot exist"). This task's own launch brief explicitly
scoped switching to either one *out* — "keep the edge fade, stop the native
blur" was a steer meant for a different session instance and never reached
this one; the captain's later, explicit call once both were in flight: ship
the native path now (built, tested, working today), and have a **future
task build the `paint_backdrop_blur` version of the menu frost and compare
it head-to-head against this native-AppKit one, keeping whichever reads
closer to comet's own result.** `gpui::EdgeFade` replacing `edge_fade.rs`'s
hand-rolled gradient is the lower-risk half of that same follow-up (no
native bridging on either side of that swap).

# neko — project knowledge

neko is a hotkey-summoned, GPU-rendered launcher for macOS, written from scratch
in Rust on [GPUI](https://gpui.rs) (Zed's UI framework). See `README.md` for how
to run it and `data/dim/plan.md` (in the firstmate home, not this repo) for the
full product plan.

## Documentation written for people

Four documents cover the shape of this project for a human reader. Point
someone at those first; they are shorter and they are current.

| Document | Covers |
| --- | --- |
| `README.md` | What neko is and is not, build and run, the licence position |
| `docs/architecture.md` | Crates, the daemon/client split, the wire protocol, providers, ranking, modes, themes, the window decisions |
| `docs/adding-a-provider.md` | Adding a result type through the `Provider` seam |
| `docs/adding-a-theme.md` | Adding a theme: the token contract and the tests that gate it |

**This file stays the authority.** Those four are summaries chosen for a first
read; every one of them drops nuance this file keeps — what was measured, what
was tried and ruled out, and which fixes were wrong before they were right.
When one of them disagrees with this file, this file is the record and the code
is the truth. Nothing was removed from here to write them.

Keep them in sync when a documented shape changes: the crate boundary, the
`Provider` trait, `search::allocate`'s passes, the theme token contract, the
build steps, or any of the window decisions in `architecture.md`'s last two
sections. A behaviour change that leaves one of those pages wrong is not
finished.

## Current scope: the spine, clipboard history, the first-run onboarding arc, and the provider abstraction

Slice 1 was a single-crate proof that GPUI works at all. A follow-up task
restructured that proof into the plan's real five-crate workspace and built
the first end-to-end feature: **summon with a hotkey, type, see your real
applications ranked sensibly, press enter, the app launches** — on the frozen
visual design, over real system window vibrancy. The task after that added
**clipboard history as a second result type in that same list**: the daemon
polls the pasteboard, persists distinct copies, and they show up under their
own `Clipboard` section header, filterable by the same search field,
restorable to the pasteboard with enter. See "Clipboard history" below. A
third task built the frozen 14-screen first-run arc (design report §3): the
real Accessibility permission ask, the clipboard-history ask, choosing/testing
the summon hotkey, and graceful degradation when Accessibility is declined.
See "Onboarding" below for the architecture. A fourth task replaced the
original hard-coded five-directory app scan with Spotlight as the primary
discovery source, kept live without polling, and filtered to what a person
would actually launch — see "Application discovery" below. A fifth task
replaced the `WindowBackgroundAppearance::Blurred` placeholder with real
native window material — `NSGlassEffectView` ("Liquid Glass") where the OS
ships it, an honest fallback chain beneath it — see "Window material"
below. A sixth task re-toned the palette from the original warm ramp to a
cool monochrome-with-a-hint-of-blue one, on direct captain instruction —
pure colour values, no geometry/layout/copy change — see "Design tokens"
below. A seventh task fixed two real defects in the app-icon cache: icons
that finished background extraction after a search response had already
landed never appeared until the client was relaunched, and every cached icon
was stored at up to 1024x1024 (188MB across a real 147-app index) for
artwork displayed at 22px — see "Icons" below. An eighth task replaced the
two hard-coded result types (apps, clipboard) with a real `Provider` trait
every result type implements, and proved the seam by adding **file and
folder search** as the third provider through it — the captain's own "fix
the indexing of stuff and also make it extensible," and the plan's own
prescribed order: earn extensibility by using the seam for a real feature,
not by designing the WASM extension host first. See "Provider abstraction"
below for the trait, the cross-provider ranking algorithm, the now-generic
wire protocol, and what a fourth provider actually has to touch. Still not
built: the WASM extension system, agent capability (which is now exactly
"implement `Provider`," per that section). See "Seams for follow-up work"
below for exactly where each remaining piece plugs in. A ninth task fixed
the seventh task's own icon-cache *size* choice: 64px was picked as "big
enough" rather than checked against what macOS actually ships, which made
extraction itself lossy (a non-native target size) on top of the
renderer's own necessary downscale — see "Icons" below,
`docs/evidence/icon-cache-128px-report.md`, for the diagnosis and the fix
(128px, a real representation size). A tenth task fixed search ranking so
applications win queries obviously about them — the captain's own daily
complaint that searching for an app returned source files instead — with a
deliberate, gated per-provider category weight for `AppsProvider` and a
demotion (never exclusion) of compiled/source-code file extensions in
`FileProvider`; both constants were re-tuned after live queries against the
captain's own file corpus caught real regressions a unit fixture wouldn't
have. See "Search and ranking" below, `docs/evidence/ranking-before-after.md`.
An eleventh task (`neko-p0-fixes`) fixed four more captain-reported
defects in one pass: Finder (and Installer/Siri/Game Center/Screen Time)
missing from the app index, the summoned panel not dismissing on an
outside click, activation failures closing the panel silently instead of
saying why, and the search field ignoring standard macOS line/word editing
shortcuts (⌘⌫/⌥⌫/⌘←/⌥← and their forward counterparts) — see "Application
discovery" below for the index fix, "Click-outside dismissal and inline
activation errors" below for the other two panel-level fixes, and "Text
field editing shortcuts" below for the fourth. A twelfth task added System
Settings pane search — `neko_core::settings::SettingsProvider`, the fourth
`Provider` and the second proof (after file search) that the seam is cheap
to extend — so that "displays", "bluetooth", "sound" etc. find the
individual System Settings pane rather than nothing. See "System Settings
pane search" below. A thirteenth task (`neko-window`), running in parallel,
fixed three window-behavior defects an exhaustive review
(`data/neko-audit/report.md`, Part 4 items 8–10) found real or left
genuinely unverified: the summon window always opened on the primary
display regardless of which one the captain was looking at, a dead daemon
left the panel silently frozen with zero indication anything was wrong, and
Spaces/full-screen collection behavior had never been checked at all (only
grepped for and found absent). See "Window placement, disconnection, and
Spaces" below — one of the three (Spaces) turned out to already be correct,
fixed by nothing this task did except adding the live readback that proves
it. A fourteenth task (`neko-ranking-2`) fixed two more captain-reported
ranking defects, live on the System Settings pane provider `b39eb25` had
just shipped: an exact pane match (e.g. "Sound") scored below unrelated
files and clipboard entries because `settings.rs` deliberately left it
unboosted, and a long clipboard paste's near-unbounded match score could
flood the result list (nine of ten rows for one real query) once it was the
only provider left with supply for the shared budget. See "Search and
ranking" below, `docs/evidence/settings-and-clipboard-ranking.md` — the
fixes are `search::settings_category_score` (the same per-provider bonus
`app_category_score` established) and `search::clipboard_max_slots` (a cap
on clipboard's own share of the greedy phase); the same evidence file also
records a structural finding worth knowing before touching ranking again:
`allocate`'s output order is fixed by provider *registration* order, never
resorted by score — true at the time this task ran; a seventeenth task
(`neko-section-order`, below) later made section order follow content
strength instead. A fifteenth task (`neko-detail-pane`) built commands and
modes — the audit's top-priority finding, that the captain expects to
search for a *thing to do* and enter it, not just get a row — proved with
clipboard history as the one real command/mode pair: a fifth provider
(`neko_core::commands::CommandsProvider`) whose rows enter a client-side
mode instead of performing a daemon action, and the mode concept itself
(`crate::modes`, `panel::Root`) that transition drives — the reserved
two-column `PANEL_WIDTH_WITH_DETAIL_PX` layout, the `⌘K` actions menu
(previously a dead label), and a real one-time window resize on mode
entry/exit. See "Commands and modes" below for the concept, the wire
protocol's three small additive touches, and — since the captain was using
this machine during this task — exactly what could be verified headlessly
(`#[gpui::test]`, no client launch) versus what still needs a live window.
A sixteenth task (`neko-mode-visual`) closed exactly that gap, live, on the
release binaries under an isolated `HOME`: `resize_and_recenter` was
confirmed correct (verified twice — a fresh mode entry and an exit/re-entry
cycle both land at the real 760px), and a real, reproduced defect was found
and fixed instead — the mode list's rows reused the root list's row
renderer unmodified, which rendered `subtitle`/`accessory` text that has no
room in the mode list's narrower 264px column and visibly collided with the
truncated title. The same task also reversed the captain's own prior
"monochrome with a hint of blue" palette decision to true neutral (chroma
0 on every chrome token, lightness unchanged) on his direct instruction.
See "Mode-view row anatomy and the neutral re-tone" below.
A seventeenth task (`neko-section-order`) fixed the structural gap
`neko-ranking-2` had already found and deliberately deferred: `allocate`'s
section order was fixed by provider registration order, so a query naming
one thing exactly — "clipboard history" — still rendered the **Commands**
section beneath the always-reserved **Clipboard** one, and "displays"
rendered **Settings** beneath unrelated **Files**, regardless of how
decisively the right section actually matched. Sections now order by their
own top candidate's score (the same score already used for row-level
ranking), with a documented, narrowly-scoped correction so a clipboard
entry's recency boost — legitimate for ordering *rows* within Clipboard —
can't be mistaken for query relevance when deciding which *section* leads.
See "Search and ranking" below, `docs/evidence/section-order-report.md`.
An eighteenth task (`neko-mode-resize`), running in parallel with the
seventeenth, finally reproduced and fixed the mode-view seam the
sixteenth task's own `resize_and_recenter` verification had — correctly,
by every measurement taken at the time — found no evidence of: entering a
mode from a window that had already been summoned and dismissed a few
times first (not the fresh-window case the sixteenth task tested)
reproduced it reliably, and it traced to a real `gpui-0.2.2` internal
staleness (`Window::viewport_size` not resyncing after a real resize, with
no public API able to force it) rather than anything wrong in this repo's
own geometry code. The fix removes runtime window resizing for a mode
transition entirely — the real `NSWindow` is now fixed at
`PANEL_WIDTH_WITH_DETAIL_PX` for the process's whole lifetime, with the
narrower root-list panel centered inside it — rather than working around
the staleness. See "Mode view resize seam" below,
`docs/evidence/mode-resize-seam-fix-report.md`. A nineteenth task fixed a
second defect the eighteenth's own fix introduced: the 40pt margin
`justify_center()` left around a narrower root-list panel inside the now
permanently-wide window was still real, clickable `NSWindow` frame with no
gpui element covering it, silently swallowing clicks that should have
dismissed the panel — fixed with two explicit margin `div`s carrying their
own `on_mouse_down` dismiss handler. See "Mode view resize seam" below. A
twentieth task (`neko-double-panel`) fixed a third, captain-reported defect
from the same architecture change: the root list visibly drew two nested
rounded rectangles at rest. Root-caused, after ruling out every candidate
named in its own launch brief by live readback, to AppKit's own automatic
window drop shadow — computed against the now-permanently-wide `NSWindow`
frame rather than the narrower panel actually painted inside it — and
fixed with one call, `NSWindow.setHasShadow(false)`, since the panel
`div`'s own explicit `.shadow_lg()` was always the only shadow this app
needed. See "The double-panel shadow defect" below,
`docs/evidence/double-panel-shadow-fix-report.md`. A twenty-first task
(`neko-craft-pass`) raised neko's interaction craft to match zeronsh/comet's
as closely as published `gpui = "0.2.2"` allows, per a captain-commissioned
design study (`data/neko-comet-design/report.md` in the firstmate home) that
read comet's own source for patterns and reimplemented every line from
scratch. Two pieces: the `⌘K` actions menu got real floating-layer
discipline (`deferred`+`anchored().snap_to_window_with_margin`+`.occlude()`,
plus a state-machine fix for the trigger-click-while-open race comet's own
`popover.rs` documents), and a small, throttled `motion.rs` catalog
(two one-shot fade specs, no repeating animation, gated on the real OS
reduce-motion setting) replaced three previously-instant state changes that
read as broken rather than fast. See "Comet craft pass" below,
`docs/evidence/`'s `actions-menu-clipboard-mode-edge-clamp.png`/
`actions-menu-root-list-open.png`. Explicitly not attempted: comet's
`frost.rs` backdrop blur and `EdgeFade` (both need `gpui` fork-only
primitives absent from the published crate) and comet's automatic
`App::reduce_motion()` snap (same reason) — see that section for what was
built instead of each. A twenty-second task (`neko-gpui-fork-migration`)
reversed the standing decision that same section's own "Explicitly not
attempted" line depended on: on direct captain instruction, `crates/neko`
now depends on the `wingleeio/zed` fork of `gpui` (pinned by rev, same
commit `refs/comet` pins), accepting a real GPL-3.0-or-later
dependency-chain exposure the captain was shown and explicitly chose to
accept — "its okay lets do it, i dont care about licence." This makes
`paint_backdrop_blur`/`BackdropBlur` and `EdgeFade`/`with_edge_fade`
genuinely available for the first time (confirmed by citation, not used by
this task — left to `fm/neko-frost`'s own follow-up), and empirically
confirms (not just reads in source) that this fork also fixes the
long-standing `windowDidBecomeKey:` self-deadlock. See "The GPUI dependency
decision" below for the full reversal record and its licence consequence,
and `docs/evidence/gpui-fork-migration-report.md` for the migration itself
— the API-drift compile fixes it forced, the four-primitive availability
citations, build-cost numbers, and a real, measured warm-summon-latency
regression this task found and did not root-cause (see "Summon latency"
below). A twenty-third task (`neko-frost`), running in parallel with the
fork migration and rebased onto it as it landed, built a native frosted
backdrop for the `⌘K` actions menu (a second, menu-scoped AppKit material
view, sibling-below GPUI's rendering view exactly like the whole-window one)
and a real scroll edge fade for the clipboard-mode list — both requested
directly by the captain after seeing comet's own `frost.rs`/`edge_fade.rs`,
deliberately on the native route rather than the fork's own now-available
`paint_backdrop_blur`/`EdgeFade` primitives, per the captain's own explicit
choice to ship the tested, working native path now and leave a head-to-head
comparison against the fork's primitives to a future task. See "Menu frost
backdrop and the results-list edge fade" below,
`docs/evidence/menu-frost-and-edge-fade-report.md`. A twenty-fourth task
(`neko-panel-shadow-tent`) fixed the same "black tent" halo the captain had
now reported three times — `fm/neko-double-panel`'s own fix (disabling
AppKit's automatic window shadow) was correct but only ever addressed one
of *two* overlapping shadow sources spilling into the same 40pt margin the
fixed-width window (`235bf88`) opened up: the panel `div`'s own
`.shadow_lg()`, GPUI's box-shadow drawn as real scene pixels, was the
other, and the one still standing on the binary the captain was looking
at. Confirmed with a single-variable test (alpha-channel measurement of a
window-scoped capture, not eyeballing) before removing it outright. See
"The panel shadow tent — the second overlapping shadow source" below,
`docs/evidence/panel-shadow-tent-fix-report.md`. A twenty-fifth task
(`neko-fork-summon-latency`) root-caused and fixed the warm-summon-latency
regression the fork migration task had found but not root-caused: a
fork-only addition to `crates/gpui/src/window.rs`'s `on_request_frame`
handler throttles any window that isn't key to ~30fps, with no exemption
for a pending `on_next_frame` callback — absent entirely from the published
`gpui = "0.2.2"` crate, confirmed by direct diff. Fixed with a small local
patch on top of the pinned rev (`patches/`, applied by
`scripts/setup-gpui-patch.sh`), not a neko-side change, since no public
gpui API exists to opt a window or request out of that throttle. See
"Summon latency" below, `docs/evidence/gpui-inactive-window-throttle-fix-report.md`.
A twenty-sixth task (`fm/neko-footer-hairline`) unified the panel to one
constant width (760px, root list and clipboard mode alike, overriding the
frozen design's original two-width rule) on captain instruction, which let
a real chunk of now-dead centering/margin/native-backdrop-repositioning
machinery come out. Its first pass also removed the footer's own
horizontal hairline on a wrong inference about which "top line" the
captain meant — corrected and restored once his own follow-up screenshot
showed the real line sits **outside the panel's top-left edge, in the
transparent margin**, not on the footer; that real line turned out to
already be fixed by the width unification (a soft, low-alpha ghost of the
panel's own top-edge highlight bleeding past its rounded corner into the
680-vs-760 margin the width fix removes) — not, as first hypothesized, a
Liquid Glass–specific specular rim (ruled out live: the same top-edge
brightening persists on the `NSVisualEffectView(.popover)` fallback too).
See "One constant panel width, and the real top line" below,
`docs/evidence/footer-hairline-and-panel-width-report.md`. A twenty-seventh
task (`fm/neko-textinput`) closed the one seam "Text field editing
shortcuts" below had explicitly deferred: real keyboard-driven text
selection (⇧←/⇧→, ⇧⌥←/⇧⌥→, ⇧⌘←/⇧⌘→, ⌘A) and real clipboard (⌘C/⌘X/⌘V against
the actual `NSPasteboard`) in the search field, closing the one place a
peer launcher study (`data/neko-loungy-study/report.md` in the firstmate
home) had found neko clearly behind. See "Text field editing shortcuts"
below for the implementation, the rendered-highlight token reuse, and a
real pasteboard-test race this task found and fixed along the way.
A twenty-eighth task (`fm/neko-instant-search`) made results render as
you type: one `Request::Search` used to run all five providers and return
one combined response, so nothing rendered until file search's `mdfind`
round-trip finished — measured 55–990ms, bounded at 1.5s — even though the
other four were ready in microseconds. A search can now be answered by
**two** frames (the fast providers immediately, the full re-allocated set
when the slow one lands), a superseded query is genuinely abandoned with
its child process killed rather than left competing with the query the
captain actually wants, and late results append below what is already on
screen rather than reordering it or displacing the selection. Keystroke to
first render: **median 108.50ms → 0.36ms** on release binaries. See
"Two-phase search: results as you type" below,
`docs/evidence/instant-search-report.md`. A twenty-ninth task
(`fm/neko-top-line-titled`) removed the line across the top of the panel
that the captain had reported three times and that two prior tasks had
attributed wrongly (first the AppKit window shadow, then the transparent
margin the fixed-width window opened up — both real, separate defects with
real fixes, neither this one). It is AppKit's own titled-window rim: gpui
creates this window titled regardless of `WindowOptions.titlebar`, and no
value of that field produces an untitled one. See "The top line — AppKit's
titled-window rim" below, `docs/evidence/top-line-titled-window-fix-report.md`.
A thirtieth task (`fm/neko-themes`) made the app themeable and shipped
seventeen palettes. The real work was not the colours: `theme.rs`'s 20
colour tokens stopped being `pub const`s (constants cannot change at
runtime) and became fields on one swappable `Palette` that every paint site
reads through `theme::active()` — one relaxed atomic load and a slice index,
no lock and no allocation on the render path — with every *dependent* token
derived in one `const fn` so a theme supplies values and can never redefine
what a token means. Geometry, spacing and type stayed `const`: a theme is
colour and surface only. On top of that seam, a `Themes` command entering a
`theme` mode (the second command/mode pair, costing exactly what
`modes.rs`'s own accounting promised) gives live preview as the selection
moves, Escape to revert, Enter to keep and persist. Ships the captain's own
neutral palette as the unchanged default plus Ember/Catnap, Catppuccin ×4,
Gruvbox ×2, Solarized ×2, Nord, Tokyo Night, Rosé Pine ×2, Dracula and
Everforest — every vendored palette MIT, verified from its own source, with
20 of 187 vendored values lightness-corrected to clear WCAG AA on this app's
own pairs (a hard test, not a report) and both the upstream and the shipped
hex pinned so a departure stays visible. Light themes also move the real
`NSWindow`'s `NSAppearance`, because the native material behind the panel
renders in it. See "Themes" below,
`docs/evidence/themes-report.md`. A later task (`fm/neko-icons`) gave the
app real icons, closing the "still open" note this file had been carrying
since the gpui-component evaluation: `gpui::svg()` was already in the gpui
this crate compiles against, so nine vendored Lucide SVGs plus a
compile-time `AssetSource` (`crates/neko/src/assets.rs`) replaced the
hand-composed `div()` marks — with one deliberate, permanent exception,
`Glyph::Palette`, which paints four swatches in the live theme's own colours
and therefore cannot be an alpha mask. See "Icons: real SVGs, and the one
that stays painted" below, `docs/evidence/icons-svg-report.md`. A later task (`fm/neko-drag-snap`) made
the panel draggable with Raycast-style snapping — screen edges, centres and
the summon position, with guides that appear only within reach and highlight
the one that will take it — which meant replacing the AppKit-owned drag that
had shipped one commit earlier, since `performWindowDragWithEvent:` runs its
own modal loop and cannot be snapped. See "Dragging the panel: snapping and
highlighted guides" below, `docs/evidence/drag-snap-guides-report.md`.

## Crate layout

```
crates/
  neko-protocol/  pure wire types + length-prefixed JSON framing, no logic
  neko-core/      daemon-owned logic: SQLite, app index, search ranking,
                  launching, hotkey-setting persistence, the Provider seam
  neko-daemon/    thin binary hosting neko-core behind a Unix socket
  neko-client/    SDK: persistent auto-reconnecting connection to the daemon
  neko/           the GPUI app — window, panel, live OS hotkey capture, onboarding
```

Boundary rule the crate split exists to enforce: `neko-protocol` depends on
nothing but `serde`. `neko` (the app) depends on `neko-client` and
`neko-protocol` only — never `neko-core` — so it physically cannot reach past
the daemon boundary to touch SQLite or the app index directly.

## The daemon/client split, and why the hotkey is registered where it is

The daemon is resident, owns SQLite as the single writer (`neko_protocol::database_path()`,
`~/Library/Application Support/neko/neko.db`) and the application index, and
survives the client being killed — a fresh client process just opens a new Unix
socket connection (`neko_protocol::socket_path()`) to the same live daemon, no
handshake state required. `neko`'s `daemon_launcher` module spawns
`neko-daemon` unconditionally on client startup; the daemon's own
`bind_singleton` (in `neko-daemon/src/server.rs`) makes a redundant spawn a
no-op — it pings the existing socket and exits quietly if something already
answers, or takes over a stale socket file if nothing does.

Note for concurrent dev testing: `socket_path()`/`database_path()` are fixed
per-user paths (`~/Library/Application Support/neko/`), not per-worktree —
correct for a real single install, but if another agent/worktree building
this same repo is testing its own `neko`/`neko-daemon` at the same time on
the same machine, you'll transparently be talking to *their* daemon (or vice
versa) via the singleton check above, and a crash report for "neko" in
Console/`~/Library/Logs/DiagnosticReports/` may not be yours. Check
`pgrep -fl neko-daemon` and match the binary's actual path before assuming a
daemon-related failure is this worktree's bug.

**The live OS-level hotkey registration happens in the client, not the
daemon**, even though the daemon owns and persists the *setting*
(`neko_core::hotkey`, a `settings` KV table). This looked like it could go
either way; it can't, for two reasons: (1) `global-hotkey`'s registration
needs a live run loop, and the client is the process with GPUI's foreground
executor already polling `GlobalHotKeyEvent::receiver()` — routing the actual
keypress through the daemon over IPC would add a socket round-trip to the
exact path the ~11–14ms warm-summon budget is measured on; (2) only a live
registration attempt can prove a conflict (see below), and only the client can
make one. `neko::hotkey_client::HotkeyController` owns this; `neko_core::hotkey`
never touches AppKit/Carbon.

## The hotkey is a runtime-configurable setting, not a constant

Added mid-task, on top of the original single-crate proof's hard-coded
`⌥Space`. The daemon persists the current `HotkeyCombo` and serves it over
`neko-protocol::Request::{GetHotkey, CheckHotkeyConflict, CommitHotkey}`; on a
successful `CommitHotkey` it broadcasts `Event::HotkeyChanged` to every
connected client. `neko::hotkey_client::HotkeyController::rebind` is the real
capture mechanism: it attempts a live OS registration of the candidate first,
and only unregisters the old combo once the new one is proven live — a failed
rebind never loses the working hotkey. `neko_core::hotkey::check_known_conflict`
is a fast, side-effect-free pre-check against well-known reserved combos
(Spotlight, Mission Control, screenshot shortcuts, ...) — real but a heuristic,
not a guarantee; only a live registration attempt can catch a *third-party*
app's custom binding.

`rebind` is now called from exactly one place: onboarding's step 09
(`onboarding::view::OnboardingRoot::try_rebind`) — captures the next raw
keydown while "Use a different combination" is active, validates it locally
(`onboarding::state::validate_candidate`), checks
`Request::CheckHotkeyConflict` for a human-readable reason to show if the live
attempt fails, then calls `rebind` and on success `Request::CommitHotkey`. See
`hotkey_client.rs`'s own doc comments and test suite (`FakeRegistrar`) for the
state machine `rebind` itself guarantees.

## Onboarding (design report §3)

The 14-screen first-run arc, `crates/neko/src/onboarding/`. Split the same
way `hotkey_client.rs` splits from `main.rs`: `state.rs` is a pure,
side-effect-free step machine (`Flow`, unit-tested — the state-transition
rules, not the GPUI rendering); `view.rs` is the GPUI window and all the real
I/O (accessibility polling, daemon requests, live hotkey capture) that drives
it. Step 03 (the real macOS system dialog) has no code of its own — it's what
macOS draws natively the instant `AccessibilityChecker::request_prompt` is
called; step 08 (the post-onboarding refusal banner) isn't part of `Flow`
either, since it isn't a step in the wizard — it's a strip `panel.rs` renders
inside the summoned panel's own content area, gated live on
`AXIsProcessTrusted()` plus a persisted dismissed flag, not on anything
onboarding remembers about how it ended.

**Real permission, real gating.** `crates/neko/src/accessibility.rs` wraps
`AXIsProcessTrusted`/`AXIsProcessTrustedWithOptions` (via `accessibility-sys`
+ `core-foundation`, both MIT/Apache — no objc2 needed in this crate for it).
`main.rs` only calls `HotkeyController::apply_initial` if
`AccessibilityChecker::is_trusted()` is already true at startup — a rejected
registration is expected and silent, not a panic, whenever it isn't (that
replaces slice 1's `.unwrap_or_else(|e| panic!(...))`, which predates any
permission awareness in this codebase). If accessibility flips to granted
*during* an onboarding run, `OnboardingRoot` registers the hotkey live at
that moment too (`ensure_hotkey_registered`), so step 09's "press it to
dismiss" works without a restart.

**One `HotkeyController`, shared.** Onboarding's step 09 and `main.rs`'s
summon loop both hold the *same* `Rc<RefCell<HotkeyController<SystemRegistrar>>>`
(`onboarding::SharedHotkeyController`) — a rebind proven live during
onboarding is the exact registration summon uses afterward, not a second one.

**Persisted lifecycle state**, daemon-owned settings, same shape as the
hotkey combo: `neko_core::onboarding` (`onboarding_completed`,
`accessibility_banner_dismissed`, `clipboard_history_enabled`), served over
`neko_protocol::Request::{GetOnboardingState, SetOnboardingComplete,
DismissAccessibilityBanner, GetClipboardHistoryEnabled,
SetClipboardHistoryEnabled}`. `clipboard_history_enabled` is the seam the
clipboard-history capture task reads before it starts watching
`NSPasteboard` — onboarding owns the ask (steps 06-07), not the watcher.
Reset for testing: `NEKO_RESET_ONBOARDING=<anything>` (`README.md`) clears
`onboarding_completed` before the client's first `GetOnboardingState` call.

**The Dock-icon "way back."** `App::on_reopen` (registered on the
`Application` builder, *before* `.run()`, since it needs to exist before any
window does — reads a `ReopenTargets` `Global` set once the summon window and
`onboarding::SharedOnboardingSlot` exist) routes a Dock-icon click to
whichever is live: the onboarding window if `SharedOnboardingSlot` is
`Some`, otherwise the summon panel. This is the real, load-bearing "never a
dead end" path for a captain who declined Accessibility and has no live
hotkey — not decorative.

**Deviations from the mockups, documented where they happen in code** (also
see `docs/evidence/onboarding-verification.md`): no embedded/registered
fonts — matches `panel.rs`'s own pre-existing choice, GPUI's default system
font throughout, sized to the mockup's values (report §6: fonts are real
packaging work, orthogonal to onboarding); permission-row/status icons are
tinted squares, not traced paths (only the ⌥ glyph is traced —
`components::glyphs::opt_glyph`/`neko_wordmark_glyph`, the one the report
specifically flags as unreliable as font text, both via GPUI's
`PathBuilder`/`window.paint_path`, no SVG asset needed); step 08's banner
renders as a strip inside the existing fixed 680px upper-third panel rather
than the mockup's repositioned/narrowed 420px variant, because there's no
menu-bar icon (see below) for it to sit next to and `panel.rs` already has a
documented fixed-size/positioning decision this task didn't reopen; the
hotkey-recording sub-state (after "Use a different combination") has no
mockup at all — it reuses the existing dialog/keycap/status-pill components.

**Menu bar icon: not built.** The mockups' simulated OS chrome shows one in
steady state and step 08, but `gpui` 0.2.2's `platform/mac/status_item.rs`
exists in the published crate's source and is never wired into a `mod`
declaration — dead code, not a usable API. Building a real `NSStatusItem`
would be raw AppKit bridging, its own scoped task (the exact kind of
non-trivial native-bridging effort the design report itself flags for
material work in §5). The Dock-icon `on_reopen` path above is the real,
functioning substitute. Relatedly, GPUI hardcodes
`NSApplicationActivationPolicyRegular` (`platform/mac/platform.rs`) — there is
no supported way to hide the Dock icon via GPUI's public API, so "no dock
icon" (mockup step 13's copy) isn't achievable without patching GPUI itself.

**Window chrome: no system title bar, live captain decision, supersedes the
mockups.** The captain ran onboarding for the first time and rejected the
standard macOS title bar (traffic lights + a "neko" title in a grey strip) —
this overrides the design report's implication that onboarding gets
*ordinary* system chrome; it is still a real, movable, closable `WindowKind::
Normal` window that appears in the Dock, just without that strip.
`onboarding::view::open_window`'s `WindowOptions` now sets `titlebar:
Some(TitlebarOptions { title: None, appears_transparent: true,
traffic_light_position: Some(point(px(14.), px(14.))) })` — frameless inset
chrome, comet's own convention (`refs/comet`, this repo's own
read-only reference clones). `render_header` is the real chrome now, not content sitting under a
system bar: it reserves `theme::ONBOARDING_TRAFFIC_LIGHT_CLEARANCE_PX` (88px)
of left padding so the neko glyph/wordmark never collides with the real
traffic-light cluster — a real, captain-reported defect on the first pass,
fixed by matching comet's own `titlebar_cluster_start` clearance value
exactly rather than guessing one. Collapses to
`ONBOARDING_TRAFFIC_LIGHT_CLEARANCE_FULLSCREEN_PX` (12px) when
`window.is_fullscreen()` — mac hides the lights there.

**The header does not actually drag the window — a known, deliberate gap,
not an oversight.** comet and waku both drag their custom chrome via
`app_owns_titlebar_drag: true` plus GPUI's `WindowControlArea::Drag` hit-test
wiring, but both depend on a **git fork** of Zed's gpui — this project is
pinned to the published `gpui = "0.2.2"` crate specifically to avoid the
GPL-3.0 taint on git-main (see "The GPUI dependency decision" below), and
that published crate has neither of those: `WindowOptions` has no
`app_owns_titlebar_drag` field at all, and `Window::start_window_move()` is
gpui's own no-op default (`platform.rs`) — mac never overrides it, only
Wayland/X11 do, per its own doc comment ("Tells the compositor to take
control of window movement (Wayland and X11)"). `render_header` still sets
`window_control_area(WindowControlArea::Drag)` and calls
`window.start_window_move()` from its mouse-down/move handlers — the correct
code for GPUI's public API, and forward-compatible with a future gpui bump
that wires mac support for it — but confirmed live (synthetic `CGEvent`
click-drag on the built release binary, window position unchanged
before/after) to do nothing on mac today. **This is a real regression from
the old system title bar**, which *was* natively draggable (a full-size
content view swallows the click before AppKit's own title-bar drag ever
sees it) — do not "fix" the regression by silently restoring
`appears_transparent: false`; that brings back the grey strip the captain
explicitly rejected. Two real fixes exist, both deferred by captain
decision: bump gpui to a version with mac drag support (reopens the GPL
question this project deliberately closed), or add a small, targeted native
call (`raw-window-handle` + `NSWindow.performWindowDragWithEvent:` from the
header's mouse-down handler, calling it from inside GPUI's own native
mouseDown dispatch so `[NSApp currentEvent]` is still the right event) —
scoped, low-risk, and the captain's own choice to make since it means a new
dependency.

**Two corrections to the paragraph above, both from later work.** The fork
this project now depends on *does* implement `start_window_move()` on mac
("The GPUI dependency decision"), so the gap is a wiring question rather
than an API one — nothing has wired it up for the onboarding window. And the
summon panel, which now genuinely drags, deliberately does **not** use it:
`performWindowDragWithEvent:` cannot be snapped, for the structural reason
in "Dragging the panel: snapping and highlighted guides" below. An
onboarding window that wants drag should decide which of the two it wants
before reaching for the shorter one.

**"Skip setup": a persistent, always-available way past the whole arc.**
Added the same day, for a captain re-testing the app who doesn't want to
walk all 14 screens every time — distinct from `NEKO_RESET_ONBOARDING`
(that's for *getting back into* onboarding; this is for getting *out* of it
quickly). `Flow::skip_onboarding` (`onboarding::state`) sets `finished` from
any step, unlike `Flow::finish` (gated on reaching `LearnHotkey` first), and
deliberately does not retroactively grant or enable anything — whatever
`accessibility_granted`/`clipboard_enabled` already are at the moment it's
clicked is exactly what persists. `OnboardingRoot::skip_onboarding` (view.rs)
shares `close_and_persist_completion` with the normal finish path, so the
graceful-degradation state is identical to declining accessibility from step
02: no live hotkey if it was never granted, and `App::on_reopen`'s Dock-icon
path remains the way back in, unchanged — never a dead end. Rendered as a
small "Skip setup" text control in the footer, next to the "Setup · N of 4"
label — deliberately not styled like `link_button` (used for the heavier,
in-content "Use a different combination" link), since this should read as
quiet and secondary, not a primary alternative to the step-by-step flow.

## Application discovery

`neko_core::apps` (`scan_applications`, `watch_applications`). Replaced the
original hard-coded five-directory scan after the captain reported apps
missing that Spotlight itself finds. Full reasoning, with the machine
evidence behind each choice, is in the module's own doc comment — read that
before touching this file again. Summary:

- **Spotlight's metadata index (`mdfind`) is the primary source**, not a
  directory list — it's what Spotlight's own UI queries, has no depth limit
  (fixes the old `depth < 1` cap that missed nested installs like
  `~/Applications/CrossOver/Steam/*.app`), and isn't tied to any hard-coded
  root. No public LaunchServices call enumerates "every registered app"
  (checked against the SDK header directly), which is why this shells out to
  `mdfind` rather than linking `NSMetadataQuery`/`MDQuery`.
- **Unioned with a plain scan of three sealed-system-volume directories**
  (`/System/Applications` and its two siblings) — verified on the dev
  machine that `mdfind` returns zero results there (`mdutil -s /` reports
  indexing disabled for that whole volume) even though every app is really
  there. That scan runs once, not live — the volume can't change without an
  OS update, which restarts the daemon anyway.
- **A fourth sealed-volume location, `/System/Library/CoreServices` itself
  (not just its `Applications` subdirectory), gets a named allowlist of 5
  bundle names instead of a fourth recursive scan — `neko-p0-fixes`,
  correcting this section's own prior claim.** Finder/Installer/Siri/Game
  Center/Screen Time all live loose in that top-level directory, which is
  why the captain's own "Finder is missing" report traced here — but so do
  ~112 macOS background agents (`Dock.app`, `PowerChime.app`,
  `CoreLocationAgent.app`, `SystemUIServer.app`, …), and a first version of
  this fix scanned the whole directory on the assumption that
  `LSBackgroundOnly` (below) would drop them, landing 259 apps instead of
  ~152. It doesn't: verified against a wide sample that **none of the
  leaked agents set `LSBackgroundOnly`**, and no other static
  `Info.plist`/`lsregister`-flag/launchd-registration/nib-presence signal
  checked separates the 5 wanted bundles from the ~112 agents either (both
  classes mix every candidate property inconsistently) — the full
  investigation, each signal tried and why it failed, is in
  `apps.rs`'s `CORE_SERVICES_ALLOWED_APPS` doc comment. `Finder.app`'s own
  `Info.plist` also needed a second, narrower fix either way:
  `CFBundlePackageType` is the legacy `"FNDR"` value there, not `"APPL"` —
  `read_app_bundle` accepts both now, the one general rule change this task
  made. Regression test:
  `apps::tests::scanning_the_real_machine_excludes_a_verified_background_agent`
  (asserts `PowerChime` absent, `Finder` present).
- **Live updates via `mdfind -live` as a change signal, not a data source**
  — it only ever reports a match *count*, never the updated paths (confirmed
  against its man page and by experiment), so an update triggers a fresh
  one-shot query rather than being parsed directly. Requires
  `NSUnbufferedIO=YES` in the child's environment or its stdout sits fully
  buffered and invisible for a minute-plus even though Spotlight already has
  the change (confirmed live). Debounced 300ms so a burst of filesystem
  churn collapses into one rescan.
- **Filtering rule: `LSBackgroundOnly`, never `LSUIElement`.** `LSUIElement`
  only hides the Dock icon — Raycast, Rectangle, Tailscale, Docker, and
  Amphetamine all set it and are all meant to be launchable (verified: none
  of the five set `LSBackgroundOnly`). `LSBackgroundOnly` means no UI
  surface at all exists to bring forward, which is what actually
  distinguishes a helper (verified against `~/Applications/Claude Code URL
  Handler.app`, a real example from the captain's machine). Also filtered:
  bundles nested inside another `.app`/`.framework` (login items, XPC
  services, framework-embedded helpers), Xcode/CI build products
  (`DerivedData`, `ios/build`, ...), installer staging directories, and
  Script Editor's template stubs (a separate rule from the nesting one —
  they live loose in a shared `Templates` directory on this machine, not
  inside `Script Editor.app` itself).
- **Known limitation, stated rather than engineered around**: if Spotlight
  indexing is disabled machine-wide, `mdfind` still runs successfully but
  returns nothing, and a successful empty result is trusted rather than
  triggering the directory-scan fallback (that fallback only fires if
  `mdfind` can't be run at all) — per "the metadata index must be the
  primary source." The sealed-system-volume apps still show either way.
- **A bundle name resolves to `None`, not an empty string, if every
  candidate is blank.** `bundle_display_name` (`apps.rs`) chains
  `CFBundleDisplayName` → `CFBundleName` → the `.app` filename's own stem,
  but falls through past a candidate that's *present and blank* too, not
  just a missing key — `plist::Value::as_string()` returns `Some("")` for an
  empty `<string></string>`, which the original `.unwrap_or_else(...)` chain
  treated as "found," producing a real row with a cached icon and a
  completely empty title (hit at 147 real Spotlight-indexed apps). A bundle
  where even the filename stem is blank is dropped from the index entirely
  — never render an icon with no title, per the same rule `panel.rs`'s row
  layout assumes throughout.

## Click-outside dismissal and inline activation errors

Two more `neko-p0-fixes` defects, both in `main.rs`/`panel.rs`.

**Click-outside dismissal.** `main.rs` registers
`cx.observe_window_activation` once, on the one resident summon window,
right after `open_window` — `window.is_window_active()` false calls
`cx.hide()`, exactly the same hide `confirm()` already used for the
launched-a-result case. Registered only on that window, never on the
separate onboarding window entity, so onboarding is unaffected by
construction. **Fixing this also fixed a latent bug it would otherwise have
made worse**: the hotkey-press handler used to toggle an `Rc<Cell<bool>>`
`visible` flag rather than checking real window state, which desynced the
moment the window was hidden by anything other than that exact branch (a
`confirm()` hide, or this new click-outside hide) — the *next* hotkey press
would then silently no-op instead of re-summoning. `main.rs` now checks
`window.is_window_active()` live at the point of each hotkey press instead
of trusting a cached flag.

**Inline activation errors.** `panel::Root::confirm()` used to discard
`client.request(Request::Activate{..})`'s result and unconditionally
`cx.hide()` — an app moved/deleted since it was indexed, or the daemon
unreachable, closed the panel exactly as if the launch had worked, with no
indication anything failed. `confirm()` now matches the real outcome:
`Response::Error{message}` or a transport `Err` both set a new
`Root::activation_error: Option<String>` field and **do not** hide the
panel; only a real non-error response hides it. `render_footer()` swaps its
normal title/verb content for `"Couldn't open — {message}"` in
`theme::STATE_DANGER` (the same danger token `render_accessibility_banner`
already established — no new toast surface, no geometry change), and
`run_search()` clears `activation_error` as its first line so it never
outlives the query that produced it. Window-scoped GUI evidence (a real
file, indexed, then deleted before Enter, captured via the
`NEKO_SHOW_CONFIRM` evidence hook below):
`docs/evidence/p0-3-activation-error.png`.

## Text field editing shortcuts

`neko-p0-fixes`'s fourth fix. `TextField` (`crates/neko/src/text_field.rs`)
had only `Backspace`/`Left`/`Right` before this task; it now also binds (in
`main.rs`'s `cx.bind_keys`) the standard macOS line/word editing set:
`cmd-backspace`/`cmd-delete` (delete to line start/end), `alt-backspace`/
`alt-delete` (delete previous/next word), `cmd-left`/`cmd-right` (jump to
line start/end), `alt-left`/`alt-right` (jump to previous/next word start).
Word boundaries go through `unicode_segmentation::UnicodeWordIndices`
(`TextField::word_start_before`/`word_end_after`), not a byte-level
`char::is_whitespace` scan — verified against `"café 東京 test"`: each CJK
ideograph is its own word-boundary stop (UAX#29 has no dictionary-based
segmentation), accented Latin stays within one word. Already a transitive
dependency via `global-hotkey -> keyboard-types`, so this added zero new
crate versions to the tree. One unit test per shortcut plus a dedicated
Unicode-awareness test, all against the editing model directly (no `Window`
needed, consistent with this file's existing test convention).

**Selection and clipboard, closing the seam this section used to describe —
`fm/neko-textinput`.** `TextField` now carries `selection_anchor:
Option<usize>`; `cursor` is always the moving end, `selection_anchor` the
fixed one. `selection_range()` normalizes the two into a `start..end` byte
range, `None` whenever there's no anchor or the two coincide (an empty
selection is the same as no selection). Bound in `main.rs`'s `cx.bind_keys`:
`shift-left`/`shift-right` (`SelectLeft`/`SelectRight`, one char at a time),
`shift-alt-left`/`shift-alt-right` (by word, reusing `word_start_before`/
`word_end_after`), `shift-cmd-left`/`shift-cmd-right` (to line start/end),
`cmd-a` (`SelectAll`, always re-anchors at the true start rather than
extending from wherever the cursor sits), `cmd-c`/`cmd-x`/`cmd-v`
(`crates/neko/src/pasteboard.rs`, the real `NSPasteboard`, every AppKit call
autoreleasepool-wrapped — same discipline as `neko_core::clipboard::
pasteboard` and `material.rs`, since an unpooled pasteboard call site is
exactly the defect class that took the daemon's own capture loop to 14+ GB
resident, see "Clipboard capture memory" above). A non-shift movement key
clears the selection (`selection_anchor = None`); any real edit clears it
too, inside `commit_edit`, the one chokepoint every content mutation goes
through. Typing a character or invoking any delete shortcut with a
selection active replaces/deletes exactly the selection first
(`edit_target_range()`), never falling through to its own direction-specific
range. The rendered highlight (`TextFieldElement::prepaint`/`paint`) reuses
`theme::SURFACE_SELECTED` — the same token `panel.rs`'s row-selection
highlight already uses — painted behind the shaped text so selected
characters stay legible, deliberately no new palette token. `⌘K` is bound
only at the `"Panel"` key context (`main.rs`), never `"TextField"`, so it
was never at risk of being shadowed by anything this task added.

**Every AppKit pasteboard test hits the same real, systemwide
`NSPasteboard`, with no per-test isolation — this raced under `cargo test`'s
default multithreading and had to be fixed with a lock, not skipped.**
`text_field::tests::pasteboard_test_lock()` is a single `static
Mutex<()>` every copy/cut/paste test holds for its whole body, so two such
tests can never interleave their real pasteboard writes/reads — confirmed
live: without it, 5 of the file's pasteboard tests failed intermittently
(wrong fixture string read back) whenever `cargo test --workspace` happened
to schedule more than one of them concurrently; with it, dozens of repeat
runs were clean. **The general lesson**: any test suite that touches a
real, unpartitioned OS-level shared resource (the systemwide pasteboard
here; a fixed socket path is `neko-client`'s own equivalent, per that
crate's own `temp_socket_path()`) needs its own explicit serialization if
more than one test in the suite touches it — `cargo test`'s default
parallelism doesn't know or care that two tests share state outside the
process.

Verified on the release binaries, window-scoped, without synthetic input —
same isolated-`HOME`/`verify_harness` discipline "Daemon concurrency" above
established (a real `neko-daemon` binary was never spawned: the isolated
`neko` client ran from a directory with no `neko-daemon` sibling and an
empty `PATH`, so `daemon_launcher::ensure_daemon_running`'s spawn attempt
failed cleanly rather than transiently starting the real capture loop).
`evidence.rs` gained `NEKO_SHOW_SELECTION=1` (only read alongside
`NEKO_SHOW_QUERY`) — drives `Root::select_query_for_evidence` →
`TextField::select_all_for_evidence`, the exact logic ⌘A's real handler
uses, factored out so it doesn't need a live `Window` — for a rendered
selection highlight without a synthetic keystroke, this repo's standing
rule. `docs/evidence/text-field-selection-highlight.png`: a real, isolated
summon window with `SURFACE_SELECTED` visibly highlighting the whole query
text, driven by this hook alone. Copy/cut/paste were verified the same way
the rest of this section already was — real `NSPasteboard` round-trips
through `pasteboard.rs`'s actual read/write functions in `text_field.rs`'s
own test suite, own fixture strings only, never the captain's real
clipboard — not by a live ⌘V keystroke, since synthesizing one is exactly
the kind of synthetic OS input this repo's standing rule (`AGENTS.md`
throughout, and this task's own launch brief) rules out.

## Two-phase search: results as you type

`fm/neko-instant-search`, fixing the captain's own daily complaint —
*"the lag between the stuff that are typed and the stuff that actually
shows up… it should be instantaneous."* Full before/after numbers,
methodology, and window-scoped evidence:
`docs/evidence/instant-search-report.md`. Measured on release binaries,
keystroke to first render: **median 108.50ms → 0.36ms** (n=24 / n=22).

**The cause, and why it was structural rather than a tuning problem.** One
`Request::Search` ran all five providers and returned one combined
response, so nothing rendered until the slowest finished — and the slowest,
`files::FileProvider`, shells out to `mdfind` (measured 55–990ms on a real
corpus, bounded at `files::QUERY_TIMEOUT` = 1.5s). Apps, clipboard,
settings and commands were all ready in microseconds and waited anyway.

**A search can now be answered by more than one frame.**
`Response::SearchResults` carries `complete: bool`;
`Response::ends_request()` is the one place the "a request may be answered
more than once" rule is written down. `neko-daemon`'s `handle_request`
partitions its providers by `Provider::defers_for(query)` (defaulted
`false`; only `FileProvider` overrides it, and only once the query is long
enough that it will really run `mdfind`), answers the fast group
immediately with `complete: false`, then answers the whole set with
`complete: true`. **A query nobody defers for is still exactly one frame** —
answering a short query in two would cost a wire frame and a second client
render for a byte-identical result, which is why `defers_for` takes the
query rather than being a fixed property of the provider.

**Why two frames rather than one response per provider.** Section ordering
and reservation (`search::allocate`) are inherently cross-provider
decisions — "which section leads", "does every provider with a match get a
slot" — that cannot be answered one provider at a time. Per-provider
responses would have pushed that logic into the client, across the crate
boundary this repo exists to protect (`neko` must never depend on
`neko-core`). Two frames keep `allocate` where it belongs and simply run it
twice: over the fast providers alone, then over everything. Both frames are
tested to honour the reservation
(`the_partial_and_final_frames_both_honour_allocates_own_reservation_rules`),
not assumed to.

**Client side**: `NekoClient::request_streaming` returns a `ResponseStream`
yielding every frame for one request id until `ends_request`. A plain
`request()` entry is *left in place* when a partial arrives and discards
it, so every pre-existing caller still resolves with the complete answer
and never learns the daemon answered in two parts — that includes anything
that reaches for `request(Request::Search { .. })` in future.

**A superseded query is abandoned, not merely ignored.** Each connection
cancels its own previous in-flight `Search` when the next arrives
(`server::supersede_previous_search`) — no new wire message was needed,
since a client sending its next search on the same socket *is* the
statement that it has moved on, and no client ever wants two of its own
searches answered at once. `neko_core::Cancel` carries the signal to
`Provider::search_cancellable` (defaulted to delegate to `search`, so a
provider with nothing interruptible still implements exactly one method).
Inside `files.rs`, the single blocking `recv_timeout(QUERY_TIMEOUT)` — a
wait that structurally cannot notice a flag flipping halfway through it —
is now a poll loop over the same deadline, and **the `mdfind` child is
killed, not left to finish with its output discarded**. That distinction is
the whole point: an abandoned query that keeps running competes with the
one the captain actually wants, so the faster they type the slower the
current answer gets. `run_bounded_child` returns the reaped child's exit
status specifically so the test can assert `SIGKILL` directly rather than
infer the kill from timing.

**Late results append; they never reorder and never displace the
selection — the hard part, and the one most likely to feel worse than the
lag if got wrong.** `search::allocate` orders sections by content strength,
so its authoritative answer can legitimately put a decisive Files match
*above* the Applications section already on screen: correct as a one-shot
answer, a visible reshuffle under the captain's eyes as a late one.
`panel::merge_late_results` therefore keeps what is rendered in its exact
order and appends only what the deferred provider introduced (necessarily
its own rows — a fast provider cannot gain candidates between the two
frames). Two consequences, both deliberate and both stated in that
function's own doc comment: the result can differ in order from what a
single-shot response would have produced (self-correcting on the next
keystroke, which has no anchor to preserve), and **if making room for the
late section would cost the selected row, the late section is not shown at
all** until the next keystroke — a captain who has arrowed down to row
seven is about to press Enter, and `fit_within_budget` can only take a new
section's header-plus-row from the tail of an earlier one.

**The "still searching" tell now reads real pending state**, not elapsed
time: `Root::partial_generation` is set when a partial frame lands and
cleared when the complete one does. The 150ms delay before revealing it
stays, now as the anti-flicker threshold it actually is.

**Verification hooks this added, all unset by default**: `NEKO_BENCH_SEARCH=<query>`
(types the query one character at a time through the panel's own real edit
path — never synthetic input — for one keystroke-to-first-render sample per
character), `NEKO_LOG_SEARCH_LATENCY=1` (implied by the former; prints one
`neko: search-latency` line per applied frame, which is what tells you which
phase a captured screenshot actually shows), and
`NEKO_FILE_SEARCH_DELAY_MS=<ms>` in `neko-core/src/files.rs` (same pattern as
`NEKO_ICON_EXTRACT_DELAY_MS` — real hardware answers a prefix query in tens
of milliseconds, genuinely too fast to screenshot the intermediate state
that the whole change exists to produce; it is itself cancellable, which
makes the abandon path live-verifiable too).

**One environment fact worth keeping for any future evidence run here:
Spotlight does not index a path with a hidden component**, so a fixture
corpus placed anywhere under this repo's own `~/.treehouse` worktree is
invisible to `mdfind` no matter how long you wait or how you `mdimport` it.
An evidence `HOME` that needs real file-search results has to live outside
it.

## Search and ranking

`neko_core::search::fuzzy_score` is a small, dependency-free subsequence
scorer (fzf-shaped: contiguous-run and word-boundary bonuses, a length
penalty), shared by every provider — see "Provider abstraction" below for
where each provider's own matching/scoring lives now and how their scores
are merged into one ranked response (`neko_core::search::allocate`,
superseding the old `rank_apps`/`rank_clipboard`-plus-hard-coded-reservation
shape this section used to describe). The daemon caps results server-side
(`Request::Search { limit, .. }`, the client asks for 8) — the client never
renders more than that, which is why the result list isn't virtualized (see
"gpui-component" below).

**No provider can crowd another out of a response entirely when there's a
matching entry — a first pass at this (apps vs. clipboard, before the
provider abstraction existed) got that wrong, twice.** The very first
version gave apps the full `limit` and filled whatever was left with
clipboard matches, so a query matching `limit` or more apps returned zero
clipboard results even with a real match — silent, not an error, easy to
miss (see "v1 simplification" below: the client's own first fix hit the
same shape and initially "fixed" it by dropping the whole clipboard section
rather than reserving it room, which is exactly backwards from the brief's
"same list" requirement). That was fixed by reserving clipboard's top match
a slot before apps ever ran. The provider-abstraction task then found a
*third* variant of the same bug, live rather than in a unit test — see
"Provider abstraction" below, `search::allocate`'s doc comment, for the
full account of why a reservation has to apply to *every* provider,
including whichever one is registered first.

**`cx.observe` fires on *any* notification, not just the one you meant —
this made keyboard navigation unusable.** `panel::Root` used to re-run
search via `cx.observe(&text_field, |root, _, cx| root.run_search(cx))`,
and `TextField` notifies on every cursor blink tick (~2/sec, forever —
that's how the cursor's own render updates) as well as on real edits.
`cx.observe` can't tell those apart, so search re-ran and `selected` reset
to `0` roughly twice a second, independent of typing — pressing Down looked
like it "went back to the first selection" because it did, within half a
second, every time. Fixed with GPUI's typed-event path instead of raw
notify: `text_field::ContentChanged` is emitted only from `TextField::
commit_edit` (the one real content-mutation chokepoint; blink's own
`cx.notify()` is untouched, since the cursor still needs to visibly blink),
and `Root` subscribes to that event type (`cx.subscribe`) rather than
observing the entity generically. **This is the general lesson, not just
this bug**: any entity that calls `cx.notify()` for a render-only reason
(a blink timer, a hover-state pulse, anything cosmetic) can silently drive
`cx.observe` listeners elsewhere in the app; if a re-render signal and a
"something meaningful changed" signal need to be distinguished, that's
what a typed event + `cx.subscribe` is for, not `cx.observe`. Selection
across a genuine re-search is preserved by identity (`(kind, id)`, not
index) via `panel::resolve_selection` when the previously-highlighted item
is still present in the new results, and reset to the top only when it
isn't — covered in `text_field.rs`'s and `panel.rs`'s own test suites
(`#[gpui::test]` needs `gpui`'s `test-support` feature, added as a
dev-dependency only).

**An exact app-name match only barely outscored a same-prefixed file — a
razor-thin length-penalty spread, not a reservation bug — so "terminal"
surfaced junk files, and a vendor-prefixed app ("Google Chrome") lost to a
plain file entirely.** Fixed with two scoped, deliberate additions — neither
touches `fuzzy_score` itself (still the same 12 original tests, unmodified):
`search::app_category_score` gives the `"app"` provider's own candidates a
per-word rescore (so "chrome" matches "Chrome" inside "Google Chrome" at
full strength, not just the whole string) plus a flat `APP_CATEGORY_BONUS`,
**gated on a real prefix match** (whole title or a significant word must
*start with* the query) — an unconditional version of this bonus was tried
first and caught live promoting scattered, coincidental app matches
("Xcode" for query "code") above genuinely relevant files, which is exactly
backwards. `files::is_source_artifact` demotes (never excludes) compiled/
source-code extensions' `fuzzy_score` by `SOURCE_ARTIFACT_DEMOTION`; the
first value tried (`0.5`) was too aggressive and, combined with the app
bonus above, let unboosted scattered app matches outrank real, relevant
source files — `0.85` was the value that survived live verification against
the captain's own file corpus. Full before/after numbers, the two live
regressions the first-tried constants caused and why, and the resolution of
a previously-unresolved "duplicate `finders.py`" report (two genuinely
different files vendored inside a Python virtualenv's `site-packages` — now
filtered as noise, the same category `node_modules`/`vendor` already are):
`docs/evidence/ranking-before-after.md`.

**`allocate`'s section order is decided by content strength, not provider
registration order — this was true through `neko-ranking-2` (below) and was
deliberately left for a later task; `neko-section-order` is that task.**
Through `neko-ranking-2`, the final `flat_map` iterated `providers` in
exactly the order `AppState::new` registered them (`app`, `file`,
`clipboard`, `settings`, `command`), always — a provider's score, however
boosted, could change *how many* rows it got but never move its section
earlier or later. That was a real, captain-reported defect on its own:
typing "clipboard history" — a phrase the `command` provider's own alias
table matches almost perfectly — still rendered **Commands** beneath the
always-reserved **Clipboard** section, purely because `command` registers
last. `search::allocate` now adds a third pass after reservation and greedy
interleave: sections are stable-sorted by their own top surviving
candidate's score, descending, reusing the exact score already established
as "comparable enough" for the row-level greedy interleave rather than
inventing a second cross-provider scale — ties resolve to registration
order for free, since a stable sort with no secondary key leaves
equal-scoring providers exactly where `providers`' input order already put
them. `panel::fit_within_budget` needed no code change for this — it
already just spends the pixel budget on whatever order `results` arrives
in, so "the first section gets the most generous budget" was already
correct once "first" means "most relevant" instead of "app" specifically.
**One second-order defect this surfaced, live, not assumed**: reusing the
row-level score as-is let a clipboard entry copied at the same instant as
the query out-rank a decisive command match purely on its recency boost
(up to `clipboard::CLIPBOARD_RECENCY_BOOST_CEILING`, 8.0) — freshness is a
legitimate *row*-ordering signal within Clipboard, not a *section*-strength
signal against other providers. `allocate`'s section-ordering pass
discounts a clipboard section's top score by that same constant (shared
with `clipboard.rs`, not re-declared, so the two can't drift apart) only
for this one comparison — `candidate.score` itself, and therefore every
row-level ranking, is untouched. Any future per-provider category bonus
still changes *which* candidates win the shared greedy budget as before;
it now also feeds section placement, both deliberately. Full before/after
query traces (all four captain-reported shapes, plus the recency-discount
finding) and the verification methodology (why the real daemon was never
launched for this): `docs/evidence/section-order-report.md`.
`docs/evidence/settings-and-clipboard-ranking.md` still has the full trace
for the registration-order-only era described above.

**System Settings pane matches scored too low, and clipboard could flood
the list — two more captain-reported defects, `neko-ranking-2`.**
`settings.rs`'s original choice to leave pane candidates completely
unboosted (reasoning: plain `fuzzy_score` already keeps a pane from
crowding out a genuine app match) overcorrected — an exact pane title also
lost to *everything else*, files and clipboard included. `search::
settings_category_score` extends `app_category_score`'s exact shape (a
shared `category_score` helper factors out the prefix-gate/per-word-rescore
logic both now use) to a fourth provider, `"settings"`, with a smaller,
deliberately-sized bonus (`SETTINGS_CATEGORY_BONUS = 1.5`, half of
`APP_CATEGORY_BONUS`) so a genuine app match for an app-shaped query
("Bluetooth File Exchange" for "bluetooth") still wins by a full 1.5-point
margin. Separately, `ClipboardProvider::search` scored a whole pasted
paragraph with the same unmodified `fuzzy_score` plus up to `+8.0` of
recency boost — `fuzzy_score`'s own length penalty is sized for
title-length strings and barely dents a match found once inside hundreds of
characters of text, so a long, largely-irrelevant paste routinely
outscored everything else and (correctly, by the greedy phase's own design)
won every contested slot once it was the last provider with remaining
supply: nine of ten rows for one real "wallpaper" query, and a privacy
concern given the captain's real clipboard holds invoices and client
correspondence. Fixed with two independent layers: `clipboard::
clipboard_length_normalization` scales a candidate's raw match score down
proportionally once its content passes a title-like length threshold
(`CLIPBOARD_TITLE_LIKE_CHARS = 60`), applied before the recency boost;
`search::clipboard_max_slots` caps clipboard's own greedy-phase intake at
half of `limit` (rounded up) whenever at least one other provider also has
a candidate — scoped to `"clipboard"` specifically, the same per-provider
gating the category bonuses use, since a provider whose *individual*
candidates are all genuinely relevant winning most of the shared budget is
the greedy phase working as intended, not a bug. The pre-existing "clipboard
always gets at least one slot" guarantee (from `allocate`'s unconditional
reservation pass) is untouched — the cap only bounds what the *greedy*
phase can additionally hand it. Full before/after numbers (a clean
`git checkout <prior-commit> -- <files>` A/B on identical fixture data, not
two separate runs), the real `fuzzy_score` values both bonuses were tuned
against, daemon idle/loaded RSS measurements, and an investigated-but-not-
reproduced finding (the original report's "displays"/"bluetooth" pane
*totally* absent, not just last — mathematically impossible from
`allocate`'s reservation pass alone if the provider returned any real
candidate, so likely a `SettingsProvider` data/environment difference on
the captain's own machine, not a ranking bug): `docs/evidence/
settings-and-clipboard-ranking.md`. That task's own hard constraint — no
client launch, no window, no `NEKO_BENCH`, no screenshot, because the
captain was actively using this machine — means warm summon latency was
not re-measured; the evidence file states that gap plainly rather than
assuming the last documented figure still holds.

## Icons

`neko_core::icons` extracts each app's real icon via `NSWorkspace::iconForFile`
(not a loose `.icns`-file parse, which silently fails on modern
asset-catalog-only apps) and caches it as a PNG under
`~/Library/Caches/neko/icons/v3-128px/`. Runs once per app on daemon startup, in a
background thread *after* the app index is already searchable, so it never
delays the first search.

**Cached at `ICON_CACHE_PX` (128px) — a real macOS icon-representation
size, not a value picked to be "big enough."** The 1024px→64px pass (below)
fixed a genuine disk-space problem but picked its replacement size as
"roughly 3x the 22pt render slot," without checking what macOS actually
ships representations at (16, 32, 128, 256, 512, 1024, each at 1x/2x pixel
density). That made 64 a *non-native* target: `extract_icon_png`'s own
`drawInRect` call resampled once to reach a non-native 64x64, and GPUI's
renderer (a plain bilinear `min_filter: linear` sampler, no mipmap chain —
`gpui-0.2.2/src/platform/mac/shaders.metal`) resampled a second time down
to the display's physical pixels (44px at the captain's confirmed 2x
backing scale on both his displays) — two compounding lossy steps for
artwork whose whole problem was already "looks soft." Caching at 128
instead — a real representation size — lets extraction's draw call copy
instead of resample for most icons, leaving exactly one resample in the
whole pipeline. Full diagnosis, before/after screenshots (including a raw
cached-PNG nearest-neighbor blow-up that isolates extraction quality from
rendering), and the display-move reasoning (nothing app-side has to
special-case it — GPUI re-samples the same cached bitmap at the window's
live `scale_factor()` every frame already):
`docs/evidence/icon-cache-128px-report.md`. Storage, same real 146-app
index: 1.10MB at 64px → 2.52MB at 128px — both trivially far from the
original 188MB/1024px problem below.

**Cached at `ICON_CACHE_PX`, not whatever size the source representation
happened to be — the original, 1024px→64px fix.** A first pass here
trusted `NSImage.setSize` + `TIFFRepresentation` to produce a small bitmap;
it doesn't — a modern asset-catalog icon's source representation can be as
large as 1024x1024, `setSize` only changes the image's reported *drawing*
size, and `TIFFRepresentation` serializes the untouched source
representation. Measured on a real 147-app index: 188MB across 142 files
(`com.apple.calculator.png` alone 1.4MB) for icons displayed at 22px.
`extract_icon_png` now draws into an explicitly `ICON_CACHE_PX`-sized
`NSBitmapImageRep` via `NSGraphicsContext::graphicsContextWithBitmapImageRep`
(the non-deprecated replacement for `NSImage.lockFocus`/`unlockFocus` —
works from a background thread with no window or run loop, same as every
other AppKit call site in this daemon) instead. `CACHE_GENERATION`
(`"v3-128px"`) is a versioned subdirectory, not a flat rename:
`ensure_cached_icon` only checks whether *a* file exists at its cache path,
not what size it is, so bumping this constant is what makes a future
pixel-size change regenerate rather than keep serving a stale file forever.
`purge_stale_icon_cache` (called once from `neko-daemon`'s startup
icon-extraction thread) is the cleanup for caches written under any
previous generation — deletes loose `*.png` files sitting directly in
`icons/` (the pre-versioning layout) *and* any versioned subdirectory other
than the current `CACHE_GENERATION` (e.g. `v2-64px` after the 128px
upgrade), so it's a no-op once it's caught up. No code reads old-generation
files as a fallback; a captain who never runs the daemon again after
upgrading keeps the old bytes on disk until this cleanup pass runs once,
which happens automatically on the next `neko-daemon` start.

**A client already showing search results does not, on its own, ever find
out an icon it was missing has since been extracted — a real defect, not
hypothetical.** `AppsProvider::search` (`apps.rs`) sets `SearchItem::icon`
to `Icon::Placeholder` until it finds the file on disk
(`icons::cached_icon_path(...).filter(|p| p.exists())`, re-checked fresh on
every `Request::Search`, then `Icon::Image(path)`), but a client's results
are a one-time snapshot: nothing re-runs `run_search` on its own after that
snapshot lands, and background extraction (real AppKit work, run once after
the daemon's own startup — see above) can easily still be in flight when a
captain's very first summon after a fresh install or a daemon restart
renders. Confirmed live: summon immediately after clearing the cache and
restarting the daemon left every icon socket blank even once extraction had
finished in the background, until the client process itself was relaunched.
Fixed with a push, not a poll: `Event::IconsUpdated` (`neko_protocol`)
broadcasts once after each background extraction pass finishes (the startup
pass and each `watch_applications`-triggered incremental pass —
`server::notify_icons_updated`, `neko-daemon/src/main.rs`); `neko`'s
daemon-event loop (`main.rs`) routes it to `panel::Root::refresh_icons`,
which re-runs the *current* query (not `reset_for_summon` — this can fire
mid-search, and clearing what the captain already typed would be wrong) so
already-visible rows pick up icons that just finished, live, in the same
window, no relaunch and no per-frame filesystem check anywhere on the client
side (the daemon is the only place that stats an icon file, once per search
it already has to run). `panel::app_icon_placeholder_glyph` fills a
not-yet-resolved app row's `Icon::Placeholder` slot with a small
hand-painted neutral mark instead of an
empty hole, self-healing to the real icon in place once `refresh_icons`
re-renders it — see "Provider abstraction" below for `Icon`/`Glyph`, the
wire-level rendering vocabulary this and every other provider's row now
draws from instead of a client-side `match` on result type. **It stays a
painted `div` mark even now that there is an icon-asset pipeline**
(`fm/neko-icons`, "Icons: real SVGs, and the one that stays painted"
below) — reading as a *different* kind of mark from the real glyph
vocabulary is its whole job.
`NEKO_ICON_EXTRACT_DELAY_MS` (`neko-daemon/src/main.rs`) is a
verification-only, unset-by-default hook (same pattern as `evidence.rs`'s
`NEKO_BENCH`/`NEKO_FORCE_MATERIAL`) that stretches out the startup
extraction pass — real hardware finishes 146 icons in well under a second
even at 128px, too fast to reliably land a screenshot mid-pass without it.
Before/after window-scoped screenshots of the same summon (no relaunch, cold
→ resolved): `docs/evidence/icon-cache-cold-before.png` /
`icon-cache-warm-after.png`.

**The above is the daemon-side, on-disk PNG cache. Client-side, GPUI's own
image cache is separately bounded — `crates/neko/src/row_icon_cache.rs`.**
A static, source-only investigation
(`data/neko-leak-audit/report.md` in the firstmate home, §2 alternative #2 /
§4 item 3) found that `gpui`'s sprite atlas (`MetalAtlas::remove`,
`gpui-0.2.2/src/platform/mac/metal_atlas.rs`) only returns a whole texture
to the free list once every tile inside it has been individually removed,
and that this crate never called the one caller-facing entry point that
removes a tile at all (`Window::drop_image`/`image_cache(...)`) — every row
icon (`panel::render_row`'s `Icon::Image` case) went through GPUI's default,
never-evicted per-`App` asset cache instead, an unbounded hole for as long
as the process ran. **The report explicitly does not believe this was the
primary driver of a captain-reported 20 GB resident-memory growth** — it
ranks a GPUI window-activation code path (`windowDidBecomeKey:`'s forced
synchronous redraw, gated on key-window transitions) above it, and that
cause remains unconfirmed; this fix closes a real, documented gap this
crate owns outright, not the 20 GB investigation itself.

`row_icon_cache::RowIconCache` is a bounded-LRU `ImageCache`
(`ROW_ICON_CACHE_CAPACITY`, 256 — comfortably above the real, closed
row-icon identity space of ~150 apps plus one shared System Settings icon;
file-search and clipboard rows use painted `Glyph`s, never `img()`, so they
never touch this cache at all), installed once on the results-list
container (`panel::Root::render_content_area`, `.image_cache(...)`) rather
than per row. Adapted from `gpui`'s own shipped
`examples/image_gallery.rs::SimpleLruCache` (Apache-2.0, the same license
`gpui` is already vendored under — not one of this project's GPL-licensed
reference apps), with the recency/eviction bookkeeping split into its own
`Window`-free `RecencyOrder` type specifically so the bound could be unit
tested without a live GPUI window.

**Deliberately not cleared at `reset_for_summon`/`refresh_icons`, even
though those are the two natural "the result set changed" boundaries.** A
full clear on every fresh summon would drop icons about to be shown again
immediately, forcing a redundant disk reload and a visible blank-then-appear
flash on almost every summon — the same "evicted too eagerly" regression
class this section's cold-cache and placeholder fixes above already closed
once each. The bounded LRU evicts continuously instead, only when a
genuinely new icon identity is requested while already at capacity, which a
fresh summon's query (or a `Placeholder`→`Image` promotion) naturally
triggers on its own — see `row_icon_cache.rs`'s and `panel.rs`'s own doc
comments on `reset_for_summon`/`refresh_icons` for the full reasoning.

**No new filesystem work on the summon path.** The actual `fs::read` of a
cached icon PNG already happened asynchronously, off the GPUI-frame path,
before this task — through GPUI's own `ImageAssetLoader` via the default
per-`App` asset cache. This task changes *which* cache holds the decoded
result (and adds real eviction), not when or how the file is read; summon
latency (`AGENTS.md`'s own "Summon latency" section) was not remeasured as
part of this task since nothing on that path changed.

**What this did not verify, and why.** The task brief that produced this
section explicitly ruled out launching the real client, opening a window, or
capturing a screenshot (the captain was using this machine at the time), so
this was verified with `cargo test`'s headless `#[gpui::test]` machinery
only (`TestAppContext`/`TestWindow` — the same mechanism already used by
`text_field.rs`'s pre-existing tests; no OS window, no screen pixels) plus
plain `#[test]`s for the eviction-order bookkeeping itself. **Not verified**:
that this bound has any measurable effect on the captain's real, reported
20 GB growth (the report's own position, restated above); that a real
summon's visible icons actually come from a warm cache hit in practice
rather than incidentally re-loading (both are correct either way, but only
one is "free"); and end-to-end confirmation via `vmmap`/real memory sampling
that `drop_image` calls here actually shrink resident atlas memory — the
report's own §6 names the exact `vmmap` methodology that would settle that,
not attempted here per the brief's "no rendering" constraint.

## Clipboard history

`neko_core::clipboard` — capture, storage, and restore all live in the
daemon, nothing in the client (`neko`) crate touches AppKit for this at all;
it's just another `Request`/`Response` pair, per the search section above.

- **Capture is a background thread the daemon spawns unconditionally**
  (`run_capture_loop`, started in `neko-daemon/src/main.rs` next to the
  icon-extraction thread), polling `NSPasteboard.generalPasteboard`'s
  `changeCount` every 400ms (`clipboard::POLL_INTERVAL`). Like
  `icons.rs`'s `NSWorkspace::iconForFile`, `NSPasteboard` needs no run loop
  for the read/write calls themselves — see the frontmost-app caveat below
  for the one place in this module that *does* need one.
- **Only `NSPasteboardTypeString` is read** — plain text and, heuristically
  or via `NSPasteboardTypeURL`, links. Images are an explicit non-goal;
  `clipboard.rs`'s module doc comment records the seam (an `Image`
  `ClipboardContentKind`, a PNG cache under `~/Library/Caches/neko/clipboard/`
  mirroring `icons.rs`, no half-built variant sitting unused in the meantime).
- **Privacy**: pasteboard content flagged `org.nspasteboard.ConcealedType` or
  `org.nspasteboard.TransientType` (the convention 1Password and other
  password managers use) is never recorded — `clipboard::is_privacy_marked`,
  pure and unit-tested, plus verified live against the real pasteboard with a
  JXA script setting those exact marker types (no password manager was
  installed on the verification machine to test against directly; say so
  rather than claim more than was checked).
- **Dedup and bound**: `content` is the SQLite primary key
  (`clipboard_entries` table), so a repeat copy is an `ON CONFLICT DO UPDATE`
  that moves it to the top rather than a new row. Bounded to
  `clipboard::HISTORY_LIMIT` (200) by count, not age — pruned after every
  insert. Both behaviors are covered in `db.rs`'s tests.
- **"Paste" writes the pasteboard, it does not simulate ⌘V.**
  `ClipboardProvider::activate` (routed there from `Request::Activate {
  kind: "clipboard", id }` — `id` is the entry's own content; see "Provider
  abstraction" above for why this is a generic request rather than a
  clipboard-specific one) calls `clipboard::write_to_pasteboard` and nothing
  else — the frontmost app after neko hides still needs a real ⌘V from the
  user. Decided this way (not
  answered by the plan or design report) because a synthetic keystroke would
  need the same accessibility-permission machinery the brief explicitly
  scoped out ("do not build onboarding or permission-request UI"), and
  "puts it back on the pasteboard" is the brief's own literal wording.
  Bonus: because writing back is a real pasteboard change, the capture loop
  notices it on its own next tick and re-records it — a paste is
  indistinguishable from an ordinary re-copy, including the recency bump.
- **Source-app attribution is best-effort, not a guarantee**, in two
  independent ways: (1) it's whatever `NSWorkspace.frontmostApplication`
  says at capture time, which can be wrong if the real copying app and the
  poll tick that notices it race (bounded by `POLL_INTERVAL`); (2) **a
  process without a spinning run loop gets a stale, frozen answer from
  `frontmostApplication`, not a live one** — confirmed with a standalone
  repro (a plain background-thread binary, no `NSApplication`/`CFRunLoop`
  anywhere, read `frontmostApplication` 16 times over 8 seconds while the
  real frontmost app was switched four times via `osascript`; every read
  returned the *first* app's name). The fix, in `clipboard::pasteboard::
  frontmost_app_name`, is a `CFRunLoop::run_in_mode(kCFRunLoopDefaultMode,
  0.05, true)` pump immediately before reading the property — confirmed live
  in the daemon binary afterward (`docs/evidence/summon-panel-clipboard-
  results.png`'s "Copied from Arc" row). **Any future code in this daemon
  that reads live AppKit/Workspace state from a background thread should
  expect the same staleness and pump a run loop first** — this is not
  specific to clipboard, it's a property of any notification-backed
  `NSWorkspace`/`NSRunningApplication` query made off a real run loop.
- **Row icon is a painted content-type glyph, not a per-entry icon.**
  Clipboard entries have no favicon/thumbnail fetching in this slice, so the
  row-icon slot would otherwise render the same empty placeholder square
  `render_row` already uses for an app with no cached icon yet — fine as a
  *transient* state for apps (self-heals once the icon-extraction pass
  catches up), wrong as a *permanent* one for clipboard, where nothing will
  ever fill it in. `panel::glyph_element` fills the slot instead: a
  three-bar mark for `Glyph::Text`, two overlapping rounded-square rings for
  `Glyph::Link` — hand-painted from plain `div`s, the same pattern
  `search_glyph` already established (no bundled SVG-asset pipeline existed
  in this codebase then, and per the design report's §6 finding, a Unicode
  symbol isn't a reliable substitute either). **Both are vendored Lucide
  SVGs now** — `fm/neko-icons` built the asset pipeline whose absence this
  sentence gives as the reason; the design report's §6 finding still
  stands, since an SVG asset is not a font glyph. See "Icons: real SVGs,
  and the one that stays painted" below. See "Provider abstraction"
  below for `Icon`/`Glyph`, the same rendering vocabulary every other
  provider's rows draw from now, and `glyph_element`'s own `File`/`Folder`
  cases the file-search provider added.
- **No two-column detail pane.** The design's screen 12 (`12-first-clipboard-
  use.html`) is a dedicated, wider (760px, `theme::PANEL_WIDTH_WITH_DETAIL_PX`)
  clipboard-only mode with a preview pane. This task renders clipboard rows
  inline in the one shared 680px list instead (screen 11's pattern: "one
  query, two result types, same list" — apps and clipboard each under their
  own section header, row-level type tag / source subtitle / relative-time
  accessory matching screen 12's own row treatment). The two-column mode
  stays a follow-up; `PANEL_WIDTH_WITH_DETAIL_PX` is still unused, still
  reserved.

## Clipboard capture memory: every AppKit call site must be pool-wrapped

The capture thread ran forever, on a plain background thread with no
`NSApplication`/`CFRunLoop`, calling `NSPasteboard`/`NSWorkspace` every
400ms without ever pushing an autorelease pool — measured taking the daemon
to 14+ GB resident (and 21+ GB of swap) in the wild. Root-caused,
fixed, and measured in `docs/evidence/clipboard-autoreleasepool-leak.md` —
read that file for the full diagnosis and the before/after numbers (same
stress, same machine, back to back: unpatched hit 19 GB resident and
climbing in 90 seconds; patched oscillated in a ~1-2.4 GB band over the
same window with no growth trend).

**The fix is structural, not just "remembered to wrap it this time."**
`clipboard.rs`'s `pasteboard` module exposes exactly two public entry
points into AppKit — `poll` and `write_string` — each wrapping its entire
body in `objc2::rc::autoreleasepool`; the raw `NSPasteboard`/`NSWorkspace`
calls are private functions only reachable from inside that pooled closure.
There is no unpooled path left into this module's AppKit calls for a future
change to fall into by accident — if you add a new AppKit call anywhere in
this daemon, it needs the same treatment, and the "audited every objc2 call
site" list in the evidence doc is where to add it. `icons.rs`'s
`extract_icon_png` (one-shot per app, not a forever-loop, so its own
version of this bug was bounded rather than unbounded) got the identical
wrap for the same reason. `apps.rs`'s Spotlight-based discovery (see
"Application discovery" above) has zero `objc2` usage — it shells out to
`mdfind` — so it was never part of this defect class; confirmed by grep,
not assumed. `crates/neko/src/material.rs` does call AppKit but runs in the
*client* process inside a real Cocoa run loop that drains its own pool
every cycle, and is a different task's file — out of scope here, not
touched.

## Provider abstraction

`neko_core::provider::Provider` is the seam every result type in the search
list is built on — app search, clipboard history, and file search (this
task's own proof) all `impl Provider`; agent capability, whenever it's
built, is a fourth. Object-safe by construction (no generics, no `Self`
return types), so the daemon holds a plain `Vec<Box<dyn Provider>>`:

```rust
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn section_label(&self) -> &'static str;
    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate>;
    fn activate(&self, id: &str) -> Result<(), ProviderError>;
}
```

**This supersedes the old `neko_core::agent::AgentProvider` seam** (an
earlier task's placeholder trait — `id`/`query` only, no score, no
activation, registered nowhere) rather than living alongside it.
`agent.rs` is deleted. The two overlapping provider-shaped traits would
have left a future agent-capability task choosing between them, which
defeats the point of having one seam — and once a second provider
(clipboard) and a third (files) existed to design against, the shape that
actually falls out needs a normalized `score` (for cross-provider ranking)
and an `activate` method (to *perform* a result's action, not just produce
it), neither of which the placeholder had. A future agent-capability
provider is just another `impl Provider`, exactly like the three below.

**Registering a fourth provider** is `Box::new(FourthProvider::new(...))`
added to the `Vec` in `AppState::new` (`neko-daemon/src/server.rs`) plus
the `impl Provider` itself — nothing else. Concretely, it does *not* need
to touch: `neko-protocol` (no new `Request`/`Response`/enum variant —
`Request::Activate { kind, id }` is generic, routed by `kind` at runtime,
not matched per provider), `panel.rs` (no `match` on provider identity
anywhere — see "the wire protocol is now provider-agnostic" below), or
`search::allocate` (every provider is already treated symmetrically). It
*does* need its own `impl Provider` (matching/scoring against whatever its
own index is) and, if its rows want a genuinely new painted shape instead
of a cached raster, one `Glyph` variant plus one case in `panel::
glyph_element` — a data addition and a paint function, not a rendering
`match` keyed on which provider produced the row.

**The wire protocol is now provider-agnostic — `ResultKind` (a closed
two-variant enum) is gone.** `SearchItem::kind` is a plain `String` (the
producing provider's own `id()`), used for exactly two things: grouping a
contiguous run of results under one section header, and routing
`Request::Activate` back to the right provider. Every actual rendering
decision that used to be a client-side `match` on `ResultKind` is now data
the provider sets directly on the `SearchItem`: `section_label` (the header
text), `action_label` (the footer verb, e.g. `"Open  ↵"`/`"Paste  ↵"`), and
`icon: Icon` (`Image(path)` for a cached raster, `Glyph(Glyph)` for a
painted built-in shape, or `Placeholder`). `Icon`/`Glyph` stay a closed enum
on purpose — they're a bounded rendering vocabulary (what the client
actually knows how to paint), not a provider-identity dispatch; the
distinction that matters is "does the panel need to know *which provider*
produced this row to render it" (no, not anymore) vs. "does the panel need
a small closed set of paintable primitives" (yes, and always will, short of
the WASM host's own declarative UI vocabulary, explicitly out of scope
here). `panel.rs` itself has zero knowledge of "apps", "clipboard", or
"files" as concepts — `render_row`, `render_footer`, and the section-header
loop all just read data off `SearchItem`.

**`Request::Launch`/`Request::Paste` are gone too, replaced by one generic
`Request::Activate { kind: String, id: String }`.** The daemon routes it to
`state.providers.iter().find(|p| p.id() == kind)` and calls that provider's
`activate`. This is what actually makes "adding a provider" cheap in the
sense the brief means: if activation still needed a new per-kind `Request`
variant, a fourth provider would touch the protocol every time.

### Cross-provider ranking (`neko_core::search::allocate`)

Each provider's `search()` returns `Candidate { score, item }` — a
provider owns its own matching and scoring entirely; the only
cross-provider contract is "higher score is better, on roughly the scale
`fuzzy_score` produces" (every built-in provider's score is ultimately
built from it, so this held without extra normalization work — see "If you
have to choose" in the launch brief: the smaller abstraction that genuinely
works beats a general normalization scheme for imagined future providers).
`allocate` merges every provider's candidates into one bounded response in
two passes, replacing the old apps-vs-clipboard-only "give apps the full
limit, cap it to `limit - 1` if clipboard matched" special case:

1. **Reservation.** Every provider with at least one candidate reserves one
   slot before anything else is allocated.
2. **Greedy interleave.** Whatever's left of `limit` is spent one slot at a
   time on the single highest-scoring not-yet-taken candidate across every
   provider — a provider whose matches are more relevant to this query
   earns more of the shared budget than one that only barely cleared its
   floor.

**The reservation pass has to include the first-registered provider too,
not just the ones after it — caught live, not by a unit test.** The first
version only reserved a floor for providers after the first ("apps get
whatever's left, clipboard/files each get a floor"), reasoning that the
first-registered provider would naturally win most of the greedy phase
anyway. A window-scoped verification screenshot (`docs/evidence/
provider-search-three-result-types.png`'s first draft, not the committed
one) proved that reasoning wrong the moment a provider whose matches score
*consistently* higher was registered: file search, on a clean prefix match,
easily outscores an app's scattered subsequence match, and it won every
single contested slot — the **Applications** section rendered with zero
rows for a query that had real app matches. `search::allocate`'s reservation
loop now runs over every provider, first included; `search.rs`'s
`the_first_provider_cannot_be_crowded_out_by_a_consistently_higher_scoring_one_either`
test pins the exact shape of this bug so it can't come back silently. The
general lesson: a "primary provider always wins ties" assumption is itself
a form of hard-coding one provider's identity into the ranking, exactly
what this task was asked to remove — symmetry (every provider gets a floor)
is what "no provider crowds another out" has to mean once there's more than
two.

`panel::fit_within_budget`/`fit_section` do the equivalent reservation for
*pixels*, not slots, generalized the same way: `group_into_sections` splits
`results` into contiguous same-`kind` runs (order-preserving), the first
section gets whatever's left of the fixed content-area budget after every
*other* section reserves one header-plus-one-row, and an earlier section
using less than its share rolls the difference forward to later ones — the
exact 2-provider math this function has always done, now provider-count-
agnostic. Both layers (`allocate` for slots, `fit_within_budget` for pixels)
exist because a response with room for a provider's match is necessary but
not sufficient for a screen with room to render it.

### File search (`neko_core::files::FileProvider`)

The proof provider. Source of truth is the same one `apps.rs` established
for application discovery: `mdfind`, not a hand-rolled directory walk —
see `files.rs`'s own module doc comment for the full reasoning, which
includes two real, measured findings this task turned up (not
assumptions):

- **Prefix matching (`kMDItemFSName == 'query*'cd`), not the app
  provider's fuzzy subsequence matching.** A leading-wildcard query
  (`'*query*'cd`, matching anywhere in the filename) was the natural first
  attempt; a single-character version of it matched 55,191 files under a
  real, repository-heavy `~/Documents` and took **~13 seconds** to
  complete, because a leading wildcard forces Spotlight off its index onto
  a slow per-item scan. The identical query as a prefix (no leading
  wildcard) returned in well under a second — Spotlight can serve a prefix
  query from its index directly. This means file search only matches names
  that *start with* the query, not names that merely contain it (Spotlight's
  own live-typing UI makes the same trade). Below `files::MIN_QUERY_LEN`
  (2 characters), `FileProvider::search` returns without querying at all —
  a 1-character prefix is both too broad to be useful and too slow even
  with the bound below.
- **`mdfind` needs `NSUnbufferedIO=YES` here too, not just in `apps.rs`'s
  `-live` watcher.** Without it, a fast, large result set can sit fully
  buffered on the read end well past this query's own deadline even though
  `mdfind` itself would have finished quickly — confirmed the same way
  `apps.rs` confirmed it originally: a query that completed in well under a
  second at the shell produced zero results through this module's reader
  before the fix.

**Bounded regardless of query breadth.** `query_spotlight_paths` reads at
most `files::MAX_RAW_RESULTS` (40) lines through a background reader
thread (the same spawn-plus-channel shape `apps.rs`'s `run_live_watch`
uses), and the caller gives that thread at most `files::QUERY_TIMEOUT`
wall-clock time via `recv_timeout` either way, killing the `mdfind` child
the instant either limit is hit. **`QUERY_TIMEOUT` is 1.5s, measured, not
guessed**: even with the buffering fix above, *time to first streamed
result* for a broad 2–4 character prefix against this machine's real
`~/Documents`/`~/Desktop`/`~/Downloads` varied from well under 100ms to a
little over 1.2s across repeated runs of the identical query — real
variance in Spotlight's own query-planning time for a broad predicate that
this module can't smooth out. A tighter 900ms bound (the first value tried)
cut off real, useful results often enough in that range to be a regression,
not a rare tail.

**Scope: `~/Documents`, `~/Desktop`, `~/Downloads` only** (`files::
default_scope_dirs`, each checked with `.is_dir()` so a missing one is
silently skipped) — "the captain's own documents and common working
locations," per the brief, not the whole disk. Not configurable in this
task; the seam for a follow-up mirrors `clipboard_history_enabled`'s
existing daemon-setting pattern (a new `Request::{Get,Set}FileSearchScope`
pair, or a list-of-paths setting) rather than anything structural. Noise
inside those directories — `node_modules`, `target`, `.git`, build output,
anything hidden, and `.app` bundles (already covered by `AppsProvider`, so
showing the same bundle twice would be confusing, not additive) — is
filtered by `files::is_noisy` *after* the query, since `mdfind` has no
directory-exclusion flag to push it into the query itself.

**Icon: a painted `Glyph::File`/`Glyph::Folder`, not a real per-file
icon, in this task.** `icons.rs`'s `ensure_cached_icon` is generic enough
to work on any file path, not just app bundles, and reusing it was the
obvious next step — but real icon extraction is genuinely expensive
AppKit work (documented in `icons.rs`'s own module comment as "tens of ms
each"), which is why `apps.rs` only ever does it in a background pass
*after* the index is searchable, for a small, bounded (~150) app list. A
file provider has no equivalent bounded set to pre-warm — the set of
possibly-matched files is unbounded — so extracting synchronously on the
search path would reintroduce exactly the latency problem the rest of this
module works to avoid, and a background lazy-extraction pass is real,
separate design work this task didn't do. It's also **exactly the surface
a parallel task (`neko-icon-cache`) owns** (`icons.rs` and the panel's icon
slot, fixing cold-cache icons and oversized files) — inventing a second,
divergent file-icon-extraction path here risked conflicting with whatever
that task lands. The painted glyph is the honest, zero-risk choice for
this task, in the same spirit clipboard rows already use one (no per-entry
icon, "no favicon fetching in this slice" predates this task). Wiring real
file icons through `icons::ensure_cached_icon` in a background pass, once
the icon-cache task's own fixes are in, is the seam.

**Enter opens the item — same "Open  ↵" verb apps use, not a separate
"reveal in Finder" action.** `/usr/bin/open` (already `neko_core::launch::
launch_app`, reused as-is — `FileProvider::activate` is a one-line call
into it) launches a file in its default app and opens a folder as a Finder
window, matching what Enter does in Spotlight's own UI. A dedicated
"reveal in Finder" would be a real secondary action (Raycast's own
convention is ⌘-Enter for exactly this), but there is no secondary-action
affordance anywhere in this panel yet — adding one speculatively, for one
provider, ahead of any real need, is exactly the kind of imagined-future
generality the brief's "prefer the smaller abstraction" guidance rules
out.

### System Settings pane search (`neko_core::settings::SettingsProvider`)

The fourth provider, and the second proof (after file search) that the
seam holds for a genuinely new capability rather than being designed in
the abstract. Full reasoning, with the machine evidence behind every
choice, is in the module's own doc comment — read that before touching
this file again, and `docs/evidence/settings-provider-report.md` for the
discovery narrative, the real-pane-opened proof, and the before/after
latency/memory numbers. Summary:

- **Enumeration source: `/System/Library/ExtensionKit/Extensions/*.appex`,
  not the old `.prefPane` layout**, which is a dead stub on this OS (no
  `Info.plist` at all in e.g. `Displays.prefPane`). A pane is real when its
  `Info.plist` declares `EXAppExtensionAttributes.
  EXExtensionPointIdentifier == "com.apple.Settings.extension.ui"` and
  `SettingsExtensionAttributes.allowsXAppleSystemPreferencesURLScheme ==
  true` (50 of 241 total `.appex` bundles on this machine).
- **The URL target is the extension's own `CFBundleIdentifier`, never
  `legacyBundleIdentifier`** — the legacy field is missing on a third of
  panes and, where present, is sometimes shared by two different panes
  (Siri and Spotlight declare the identical legacy list), so it can't be
  the primary key. The modern bundle identifier is unique and was verified
  live to resolve every pane tested, alone.
- **One-time scan at daemon construction, no live watcher** — the same
  "sealed system volume, an OS update restarts the daemon anyway" argument
  `apps.rs` already established for `/System/Applications`, since
  `/System/Library/ExtensionKit/Extensions` sits on the same sealed
  volume. Enumeration never runs on a client's search path.
- **Display names come from each bundle's localized `InfoPlist.loctable`
  (`"en"` key), not the raw, often-internal-target-named `Info.plist`** —
  falls back to the raw plist, then a two-entry override table for the
  two panes (Battery, Headphones) that have no localized name anywhere in
  their bundle.
- **Icon: the System Settings app's own icon, shared by every pane row**
  — a per-pane icon would need Apple's private iconography stack (each
  pane names an `ISGraphicIconConfiguration.ISTypeIdentifier`, not
  resolvable through the public `NSWorkspace.iconForFile` this daemon's
  icon cache already uses everywhere else). Extracted once, in the same
  background pass `main.rs` already runs for every app icon, through the
  same `icons::ensure_cached_icon` — no second icon pipeline.
- **No settings-specific ranking bonus.** Unlike `AppsProvider`'s gated
  `app_category_score`, this provider's candidates are plain
  `fuzzy_score` — the launch brief's own requirement ("must not crowd out
  genuine application matches for app-shaped queries") is already
  satisfied without one, verified against `fe4e4e9`'s own ranking test
  queries ("code", "terminal", "chrome") producing identical top results.
- **The seam held.** Touched: one new file (`settings.rs`), one new
  function in `launch.rs` (`open_url`, `launch_app`'s identical shape
  typed on `&str` for a URL rather than a `Path`), one `pub mod` line,
  and one provider-registration line in `server.rs`. Untouched:
  `neko-protocol` (no new `Request`/`Response` variant), `panel.rs` (no
  `match` on provider identity — section label, icon, and action verb are
  already data on `SearchItem`), and `search::allocate` (already
  provider-count-agnostic).

### Daemon concurrency (why `handle_connection` now spawns a thread per request)

`FileProvider`'s `mdfind` round-trip (up to 1.5s on a broad query) exposed
a real risk in the daemon's original request-handling shape: one
persistent connection per client, one thread reading frames in a loop and
handling each synchronously before reading the next. A slow `Search` would
have queued every subsequent request on that connection — every future
keystroke, a hotkey change, an onboarding check — behind it, visibly
degrading the whole app's responsiveness rather than just that one search.
`neko-daemon/src/server.rs`'s `handle_connection` now spawns one thread per
*request*, not per connection; `handle_request` (`Search` specifically)
also fans its providers out across `std::thread::scope` so the daemon
round-trip is `max(provider times)`, not their sum. **`max(provider times)`
is no longer what a client waits to see anything, though** — see "Two-phase
search: results as you type" above: a search is answered in two frames, the
first at `max(fast provider times)`, and one thread per request is what
lets a superseded search be cancelled independently of the one replacing
it. Responses can complete
out of order relative to requests as a result — safe by construction,
since `neko-client`'s `Shared::pending` map (`neko-client/src/lib.rs`)
already matched a response to its caller by the request's own `id`, never
by arrival order. The other necessary half: `AppState`'s per-connection
writer moved from a bare `UnixStream` to a shared `Arc<Mutex<UnixStream>>`
— two request-handling threads on the same connection can now genuinely
write concurrently, and since a `UnixStream::try_clone()` shares the
underlying socket fd, two unsynchronized `write_frame` calls (each two
separate `write_all`s: a length prefix, then the payload) could interleave
mid-frame and corrupt the stream without it. Every writer for a connection
— a request's own response, and any `Event` broadcast to it — now goes
through that one lock.

### Evidence-capture hook: `NEKO_SHOW_QUERY`

`crates/neko/src/evidence.rs` gained one more env-gated hook, same pattern
as the existing `NEKO_SHOW_ON_LAUNCH`/`NEKO_BENCH`: `NEKO_SHOW_QUERY=<text>`
types `<text>` into the search field before the screenshot (via
`TextField::set_content_for_evidence`, a real edit through the same
`ContentChanged` path a keystroke takes — not a rendering shortcut), for
capturing real, non-empty search results without synthetic OS keystrokes.
**Synthetic keystrokes turned out unreliable here beyond just the hotkey
case `AGENTS.md`'s "Testing caveat" already documented** — a `System
Events` `keystroke` sent to the frontmost process right after this
window's own `activate_window()` call landed on the wrong process in
practice (confirmed live: the query field stayed empty).

Timing, precisely: when a query is set, `show_once` prints `neko: ready
for evidence setup` immediately (a deterministic "go" signal an outside
script can watch for — the pasteboard is systemwide, not scoped to any
per-run isolated `HOME`, so this is the moment to seed a real clipboard
entry), waits 2s, then applies the query; because that re-runs search the
same way a keystroke does, a second 3.5s wait follows before the
window-number line prints (the actual "now capture" signal), covering
`files::QUERY_TIMEOUT`'s worst case plus real scheduling variance.

**The recipe above is proven deterministic, not just plausible — a
five-run repeat, each in a fresh isolated `HOME`, each copying a unique
token to the clipboard the instant `ready for evidence setup` appears in
the log, passed 5/5**, all three sections (Applications, Files, Clipboard)
rendering correctly every time. The investigation that produced this
proof is itself worth recording: an earlier round of ad-hoc verification
runs saw the Clipboard section intermittently missing, and two rounds of
debugging chased it as a possible timing race before the actual cause
surfaced — the *verification token itself* (`"racetok1_...", "diagtok_...`)
didn't contain the query's characters in order, so `fuzzy_score` correctly
never matched it; a harness bug, not a product one. The lesson generalizes
beyond this one hook: when a search-driven test fails intermittently,
verify the query is capable of matching the fixture *before* suspecting
the ranking or timing code — a `python3 -c` one-liner reimplementing
`fuzzy_score`'s subsequence check against the exact fixture string is
faster than another round of live capture, and would have caught this
immediately.

### Evidence-capture hook: `NEKO_SHOW_CONFIRM`, and a live-hotkey collision it exposed

`neko-p0-fixes`'s own evidence hook, for capturing the inline activation-
error footer (see "Click-outside dismissal and inline activation errors"
above) without synthetic OS keystrokes — same reasoning as `NEKO_SHOW_QUERY`
above. Read alongside it: once the query's results render,
`panel::Root::confirm_for_evidence` drives the exact same `confirm()` path a
real Enter keystroke takes (`self.confirm(&Confirm, window, cx)` —
`Confirm` is a plain constructible unit struct, GPUI's `actions!` macro
output), then `show_once` waits for the real `Request::Activate` round-trip
before printing the window-number "now capture" line.

**Real finding while using this hook, worth recording for any future
evidence run on this machine, not just this one hook**: `main.rs` registers
a live OS hotkey unconditionally at startup whenever `AXIsProcessTrusted()`
is already true for the binary being run — `NEKO_SHOW_ON_LAUNCH` bypasses
the onboarding *screen* but not this registration. If the captain's real
daemon/client doesn't currently hold the default `⌥Space` combo (e.g. he's
rebound it), an isolated-`HOME` evidence client's registration attempt can
*succeed* — and then genuinely receive real physical `⌥Space` presses meant
for whatever the captain was actually doing, visibly resetting the evidence
run's own query field via `reset_for_summon` mid-capture (confirmed live:
three unexplained `neko: summon latency …` / `lost activation` cycles during
one capture attempt turned out to be three real hotkey presses landing on
the evidence process instead of the captain's). **Mitigation, now routine
for any evidence run that leaves `NEKO_SHOW_ON_LAUNCH`'s hotkey path live**:
before starting the client, send the isolated daemon a `Request::CommitHotkey`
for an obscure combo (e.g. all four modifiers plus `F13`) so nothing anyone
is realistically pressing can land on it. This is a persisted daemon-side
setting (`neko_core::hotkey::set_hotkey`, no live registration attempt of
its own — see "The hotkey is a runtime-configurable setting" above), so it
can be set with one request against the isolated socket before the client
process even starts. **Superseded structurally as of
`fm/neko-evidence-no-activate`**: `main.rs` now skips hotkey registration
entirely for any evidence run (`evidence::evidence_run_active`), so this
mitigation is belt-and-braces rather than the thing standing between an
evidence client and the captain's real keypresses. See "Standing safety
rule: an evidence window must never become the key window" above — that
section's own incident is what proved a remember-to-do-it mitigation
insufficient.

## Commands and modes

The audit's top-priority finding: the captain expects to search for a
*thing to do*, not just a row — "search for Clipboard History and enter
it," the way Raycast's own root list mixes commands in with applications.
Built as two small, orthogonal pieces on top of the existing seams rather
than a new abstraction layer: **a command** is a fifth `Provider`
(`neko_core::commands::CommandsProvider`) whose rows carry a new
`SearchItem::enters_mode` field instead of doing a daemon-side action;
**a mode** is `neko`'s own client-side UI state (`crate::modes`, `panel::
Root`) that a confirmed command transitions the panel into. Full concept
writeup, including the exact "what does a second command have to
implement" accounting the launch brief asked for (the same shape
`settings.rs`'s own doc comment gives for a fourth provider), lives in
`crates/neko/src/modes.rs`'s module doc comment — read that before adding a
second command or touching this area again.

**The wire protocol touches, and why each one is additive, not a
workaround.** Three changes, all backward-compatible field additions, no
new `Request`/`Response` variant beyond what's noted:
- `SearchItem` gained four fields: `enters_mode: Option<String>` (the mode
  id a confirmed row enters, checked by `panel::Root::confirm` *before*
  ever building a `Request::Activate` — a command's own `activate` is never
  actually called, and errors if it somehow is), `group_label: Option<String>`
  (a mode's own "Today"/"Yesterday" time-grouping, orthogonal to
  `section_label`'s provider grouping — the root list never reads it),
  `actions: Vec<ItemAction>` (the `⌘K` menu's own secondary actions, empty
  for the three providers that have nothing beyond their one primary
  action), and `source: Option<String>` (a bare "where this came from"
  value for a detail pane's own labeled field, distinct from `subtitle`'s
  already-composed sentence). All four are exactly the same "provider
  describes it, client just renders it" pattern `action_label`/`badge`/
  `icon` already established — no new per-kind `match` anywhere.
- `Request::Search` gained `provider: Option<String>` — `Some(id)` scopes
  the search to exactly that one provider's own `search()`, sorted by
  score, no cross-provider `allocate()` at all. This is the actual mode
  mechanism: entering a mode means every keystroke searches with this field
  set to the mode's own provider id instead of `None`. The daemon-side
  branch is a straight `if let`/`else` in `handle_request`
  (`neko-daemon/src/server.rs`) before the existing multi-provider path,
  which is completely untouched.
- `Request::Activate` gained `query: String` much later
  (`fm/neko-new-agent`) — the search field's contents at the moment Enter was
  pressed, for the one kind of row whose action takes an argument rather than
  naming a thing to open. Received through a defaulted
  `Provider::activate_with_query`, so every other provider is untouched. See
  "Starting an agent: the New Agent command" for the two silently-wrong
  alternatives it replaces.
- `Request::Activate` gained `action: Option<String>` — `None` is the
  existing single primary action (`Provider::activate`, unchanged);
  `Some(action_id)` routes to a new, defaulted `Provider::perform_action`
  trait method (`provider.rs`) instead. The default errors "no such
  action," so the three providers with nothing beyond their primary action
  (apps, files, settings, commands) need zero code changes at all;
  `ClipboardProvider` is the one override, for its three menu actions
  below.

**Commands (`neko_core::commands`).** One built-in command today,
`CommandsProvider::search` (id `"command"`, section label "Commands"),
returning `"Clipboard History"` with `badge: Some("COMMAND")` (pre-
uppercased on the wire, same convention `ClipboardProvider`'s `"TEXT"`/
`"LINK"` badges already use — this client never CSS-transforms text) and
`enters_mode: Some("clipboard")`. **The multi-word matching limit
documented in `docs/evidence/settings-provider-report.md`** ("keyboard
shortcuts" never matches a pane titled "Keyboard" — `fuzzy_score` needs
every query character, including the space, to appear as an in-order
subsequence of *one* title) would otherwise block "clipboard manager" (no
`m`/`a`/`n`/`g`/`e`/`r` exist anywhere in "Clipboard History" after
"Clipboard "). Fixed inside this provider, per the launch brief's own
instruction not to touch the shared `fuzzy_score` (owned by a parallel
task's `search.rs`): each command carries a short list of alias phrases
(`"Clipboard History"`, `"Clipboard Manager"`, `"Clipboard"`), each scored
independently, best score wins. "clipboard" and "clipboard history" already
match the literal title as a plain subsequence with no alias needed —
verified in `commands.rs`'s own test that pins exactly this distinction.

**Modes (`crate::modes`, `neko`).** Split the same way `onboarding/state.rs`
splits from `onboarding/view.rs`: `modes.rs` is pure, GPUI-free chrome data
(`ModeChrome`: id, scoped provider id, footer title, placeholder text,
whether it has a detail pane) plus a lookup function, unit-tested with
plain `#[test]`s; `panel::Root` is the view layer. Confirming a command row
(`Root::confirm`) calls `enter_mode`, which saves the current query, clears
the field to start the mode's list fresh (a real edit through
`TextField::set_content`, which re-runs search — by then `active_mode` is
already `Some`, so `run_search` takes the provider-scoped branch
immediately), swaps the placeholder, and — if `ModeChrome::has_detail` —
widens the *visible* panel. `Escape` (a new `Root::handle_dismiss`, registered
as a window-level `on_action` listener on `Root`'s own div so it intercepts
`DismissWindow` before `main.rs`'s global `cx.hide()` fallback ever sees it
— GPUI's own action dispatch runs window listeners in the bubble phase
before global ones, and a handled action stops propagating there by
default) or the input row's own back-arrow glyph both call `exit_mode`,
which restores the saved query verbatim and narrows the panel back.
`reset_for_summon` also force-exits any active mode — a mode is
per-summon-session state, not something that survives the panel being
hidden and re-shown.

**The real window resize this originally described (`display_placement::
resize_and_recenter`, a raw, synchronous `NSWindow` `setContentSize:`/
`setFrameTopLeftPoint:` call pair) was removed by `neko-mode-resize` and
does not exist any more — see "Mode view resize seam" below.** It resized
correctly by every measurement taken at the time; the real window, its
rendering surface, and its backing drawable all tracked the new size. What
didn't reliably track it, discovered only once a session that had summoned
and dismissed the panel a few times first was reproduced, was `gpui::
Window`'s own private `viewport_size` — the size its root element is
actually laid out and painted against every frame. A mode transition now
resizes nothing: the real `NSWindow` is a fixed `PANEL_WIDTH_WITH_DETAIL_PX`
for the process's entire lifetime, and `enter_mode`/`exit_mode` instead call
`panel::Root::update_background_bounds` (a direct native-view frame set,
`material::set_background_frame`) to move/resize the material backdrop
*within* that fixed window, in lockstep with `Render::render`'s own
`justify_center()` centering of the panel `div`.

**The two-column clipboard-history view**
(`data/neko-design/mockups/12-first-clipboard-use.html`, the frozen mockup
— not just the launch brief's prose description of the captain's Raycast
screenshots, which mentions Dimensions/Image size fields this app has no
data for and this mockup deliberately doesn't include) — left column fixed
at `theme::MODE_LIST_COLUMN_WIDTH_PX` (264px, a new geometry token, the
mockup's own `.panel-list.with-detail` width — not a change to any
*existing* frozen token), right column the preview (the entry's raw stored
content, `SearchItem::id`, not the list row's own truncated/quoted `title`)
plus three info fields: Application (`source`), Content Type (`badge`,
title-cased for display), Copied. **One disclosed deviation from the
mockup's literal copy**: "Copied" shows this app's existing relative-time
label ("12m", "3h" — `clipboard::relative_time`, already used for the row's
own accessory) rather than an absolute local timestamp like the mockup's
"Today, 9:50 AM" — no date/time-formatting dependency exists anywhere in
this codebase, and adding one (`chrono` or equivalent) for one label wasn't
judged worth it. The "Today"/"Yesterday" list-section grouping
(`clipboard::day_bucket_label`) is real but **UTC-calendar-day
arithmetic, not local-timezone-aware**, for the identical reason — a copy
made shortly before/after local midnight can land in the "wrong" UTC-day
bucket; disclosed in that function's own doc comment rather than silently
assumed correct. **One new color token**, `theme::SURFACE_INPUT` (the
preview box's recessed background) — derived by the exact same
warm-to-cool hue-swap rule every existing base-neutral token in `theme.rs`
already used for the palette re-tone (same L/C, hue swapped from ~70° to
255°), not a fresh color pick, so it doesn't reopen the closed
colour-identity question. **Not built**: the mockup's "All Types" filter
control on the right of the mode's input row — there is no real type-filter
logic behind it (clipboard has exactly two content kinds today, already
visible per-row via the badge), and a control with nothing real to do would
be a fake affordance; the back-arrow half of that row is real and built.

**The `⌘K` actions menu** (no mockup exists for this — the launch brief's
own note) is `panel::Root`'s own `ActionsMenuState`, populated from the
selected row's `SearchItem::actions` (no daemon round-trip to open it — the
data already arrived with the last search response, the same "provider
describes it" pattern the rest of this file uses). `ClipboardProvider`
offers three: "Paste" and "Copy" both call the identical `activate`
(writing to the pasteboard — see "Clipboard history" above on why neko
never simulates a keystroke, which makes these two genuinely the same
operation today, not just similarly labeled), "Delete"
(`Db::delete_clipboard_entry`) is `destructive: true`. **A destructive
action needs a second, explicit Enter to actually run** — the first only
"arms" it (`ActionsMenuState::confirm_armed`, rendered as "Confirm Delete —
↵ again"); moving the menu selection at all disarms it again, so a
confirmation can't survive being scrolled past and back. This satisfies the
brief's "a mis-keyed action cannot silently destroy an entry" requirement
without a separate modal dialog. A primary-row Enter still hides the panel
on success (matching every pre-existing `Activate` call site, mode or not);
the menu deliberately never does, on either action — reviewing/managing
entries is exactly what opening the menu is for.

**Verification: headless only, by explicit instruction — the captain was
using this machine.** No client launch, no real window, no screenshots this
pass. Covered instead: `commands.rs`/`clipboard.rs`/`db.rs`/`provider.rs`
plain `#[test]`s (command matching and aliasing, `perform_action` routing,
day-bucket labeling, delete), `server.rs` integration tests against
`handle_request` directly (provider-scoped search, the new `action` field
routing), and — the load-bearing ones for the client side —
`panel.rs`'s own `#[gpui::test]`s using `TestAppContext`/`FakeAccessibilityChecker`/a
real `NekoClient::connect` against a socket nobody's listening on (the same
headless-GPUI, no-OS-window mechanism `row_icon_cache.rs`'s own tests
already established for this crate): confirming a command row really does
enter the mode and save the prior query, `Escape` exiting it and restoring
that query, opening the actions menu populated vs. inert, and the
destructive-delete double-confirm/disarm-on-navigate behavior — all
directly on `Root`'s real methods, not reimplemented test doubles. **What
this could not verify at the time**: the real `NSWindow` resize/recenter
call, the two-column layout's actual on-screen appearance, warm summon
latency, and daemon idle memory — all needed a live client/window/daemon
this task was explicitly told not to launch. **Since closed by
`neko-mode-visual`** (see "Mode-view row anatomy and the neutral re-tone"
below): `resize_and_recenter` was verified live, twice (a fresh mode entry
and an exit/re-enter cycle), through the real `Root::confirm` path — it
correctly resizes to 760px both times. Warm summon latency and daemon idle
memory were still not re-measured by that task either, since neither the
resize path nor the daemon changed — only a client-side row-rendering
conditional and a palette constant table did.

## Preferences: a real window, and the two rejected shapes before it

`fm/neko-preferences`, from the captain's own *"it's better to have a
settings or preference as well... we haven't built the settings panel yet,
right?"*. Three settings, chosen by him: **the summon hotkey, launch at
login, and the file-search folders**. Clipboard-history on/off was
deliberately left out even though it is the cheapest possible row.

**Read this section before proposing a surface for anything settings-like.**
This shipped in three shapes in one sitting, and the two that were discarded
are the useful part of the record:

1. **In-panel mode with drill-in sub-screens.** A `preference` mode plus
   `preference.hotkey` and `preference.folders` as modes of their own. Forced
   `active_mode: Option<ActiveMode>` to become a `mode_stack: Vec<ActiveMode>`
   so Escape could pop one level.
2. **In-panel sidebar with in-pane editing.** `has_detail: true` like
   clipboard history, and the captain's own follow-up — *"they don't have to
   open their own window as well, right?"* — replaced the drill-ins with a
   `PaneFocus` model: Enter moved the keyboard into the detail pane, Escape
   brought it back. **The mode stack was reverted here**, because with
   nothing nesting it was generality nothing asked for.
3. **A real window** (shipped), after the captain compared it against
   Raycast's own settings window.

**Why the window is right, stated as a rule rather than a preference.** A
launcher panel is a *transient* surface: it hides on click-outside, its field
is a query, its Enter means "do the thing and get out of the way", and — the
load-bearing one — it is a **non-activating** `NSPopUpWindowLevel` panel.
Settings are the opposite of transient. They want a surface you can leave
open beside the thing you are configuring, and they need controls (a path
you type, a control that records raw key presses) that only make sense in a
window that can genuinely become key. Both earlier shapes were fighting that
one fact.

**Nothing daemon-side changed across all three shapes.** The settings live in
`neko_core::preferences` behind the ordinary `Provider` seam, and the window
reads and writes them through exactly the requests the panel used: a scoped
`Request::Search` per list, `Request::Activate` per change. That is the seam
working — the surface is the client's own business. `neko-protocol` gained
exactly one thing across the whole task: a `Glyph::Sliders` variant plus its
paint case, the documented cost of a new painted shape.

**The window** is `crate::preferences`, split `state.rs` (pure, unit-tested:
which tab, and what a recorded key press means) / `view.rs` (GPUI and all
I/O), the same split `crate::onboarding` uses. Chrome matches onboarding's
exactly — frameless inset title bar, native traffic lights repositioned into
it, `WindowKind::Normal`, fixed 720×520, opaque (it is read for minutes at a
time, so legibility beats the vibrancy that makes a transient overlay feel
light). Three tabs: **General** (summon hotkey, launch at login), **Search**
(the folder list), **About**. **There is no AI tab** — a tab bar padded out
with tabs that say "nothing here yet" is worse than a small one.

- **It is opened through an injected closure**, `panel::PreferencesOpener =
  Rc<dyn Fn(&Window, &mut App)>`, for the same two reasons `AppearanceSetter`
  is: the pieces it needs (client, live registrar, single-window slot) belong
  to `main.rs` rather than to a list of results, and **GPUI's test platform
  `unimplemented!()`s — panics — on `open_window`, `App::hide`, and every
  native window call**, so an unconditional call from `confirm` takes every
  panel test down with it. Dismissing the panel lives *inside* that closure:
  it is part of "open Preferences", not a second thing a caller might forget.

- **Dismissing the panel here is `material::order_out`, never `cx.hide()` —
  a second window changes what "hide" has to mean.** `App::hide` is
  `[NSApp hide:]`: it hides *every* window this app owns. That was
  indistinguishable from "hide the panel" for as long as the panel was the
  only window that could be active, and stopped being so the moment
  Preferences became a real one — the first version of this genuinely called
  `cx.hide()` right after opening the window, i.e. asked macOS to hide the
  window it had just opened. **The same trap is in
  `main.rs`'s click-outside observer**, which hides on the panel losing
  activation: opening Preferences *is* the panel losing activation. It now
  asks `cx.active_window()` first — "did focus go to another neko window, or
  out of neko entirely?" — and only hides the app in the second case, which
  is also the only case where macOS has a previous app to restore focus to.
  That question needs no state of our own, which is why it is preferred over
  tracking whether a Preferences window happens to be open.
- **`SharedPreferencesSlot` keeps it singular.** Confirming the row again
  focuses the live window; two settings windows could disagree on screen
  about what a setting currently is.
- **Every value is re-read from the daemon after every change**, never
  mutated optimistically — installing a LaunchAgent can genuinely fail, and a
  settings window showing a value it merely *hopes* is true is the exact
  defect `set_launch_at_login` is written to avoid.
- **`Response::Error`'s message is shown verbatim.** The daemon already
  phrases these for a person ("not a folder: /nope"); rewording loses detail.

**The rebind goes through the *same* `HotkeyController` the summon loop
uses**, exactly as onboarding step 09 does — a combination proven live in
Preferences is the registration that summons afterwards, not a second one.
`hotkey_client::HotkeyRebinder` is an injected trait held in a **deferred
slot** (`SharedRebinder = Rc<RefCell<Option<...>>>`), because the controller
cannot exist until the daemon has answered with the current combo, which is
after the window and `panel::Root` are created; `main.rs` fills it then.
`None` renders as "neko is still starting up" rather than silently ignoring a
press. Order matters and matches onboarding: **register live first, persist
only on success**, so a combination the OS refuses never reaches storage and
a working hotkey is never lost. The daemon's `CheckHotkeyConflict` heuristic
supplies the human-readable *reason*, since a Carbon failure is an opaque
status code. `onboarding::state::canonicalize_key_name` moved from `view.rs`
to `state.rs` and is shared by both callers, so the two can never disagree
about what a physical key is called. `state::candidate_from_press` **ignores
a bare modifier press rather than rejecting it** — gpui reports those as
ordinary key presses, and treating one as a candidate would flash "no key"
every time somebody started holding ⌘.

**Two providers, one of them mode-only — a new registry in `AppState`.**
`neko_core::preferences::PreferencesProvider` (id `"preference"`, section
"Preferences") is registered in the root list on purpose: typing "hotkey"
reaches the setting itself. It **cannot** be called `"settings"` —
`neko_core::settings::SettingsProvider` already owns that id for macOS System
Settings panes, which is also why the command is titled "Preferences" with
`"Settings"` only as an alias (verified live: a root query for "settings"
returns macOS's own **System Settings** app *and* neko's **Preferences**
command, unambiguously). Its sibling `FolderScopeProvider` (id
`"folder-scope"`) is registered in a **second** list,
`AppState::mode_providers`: reachable by an explicitly scoped search and by
`Request::Activate`, never by a root-list query, because a configured
search-folder path is not a result anybody wants back from the root list.
`AppState::all_providers()` is the union, used by the scoped-search and
activate paths only. **This is the seam for any future list that only means
something inside its own surface.**

**Launch at login writes a `~/Library/LaunchAgents` plist, not
`SMAppService`.** `SMAppService::mainApp` registers *the calling app's
bundle*, and neko is a bare Mach-O (`target/release/neko`), not a `.app` —
there is no bundle to register. The plist names the **client** binary
(resolved as the sibling `neko` next to the daemon's own `current_exe()`),
never the daemon: the client spawns the daemon itself, while starting the
daemon alone would leave no window to summon. `launchctl load -w`/`unload -w`
rather than `bootstrap`/`bootout` purely to avoid a `libc` dependency for
`getuid()`. **If neko is ever packaged as a `.app`, move this to
`SMAppService`** — the plist route needs the binary to stay at one path,
which a real install guarantees and a `cargo build` output directory does
not. The flag and the agent on disk are always written together, so "On" can
never mean "persisted true, nothing installed".

**File-search scope is now live-configurable.** `files::FileProvider` holds a
`Scope` (`Fixed` for tests, `Configured(Arc<Mutex<Db>>)` for the real daemon)
and re-reads `preferences::get_search_folders` on every query rather than
caching, so a folder added in the window takes effect on the next keystroke
with no restart and no invalidation message — one KV point query per search,
far below the `mdfind` round-trip the same call is about to make. An unset
key falls back to `files::default_scope_dirs()`, so an untouched install
behaves exactly as before; a persisted **empty** list is honoured as a real
choice ("search nothing"), and a corrupted value degrades to the default, the
same rule `themes.rs` applies.

**An empty root query must not reach every provider —
`Provider::answers_empty_root_query`, caught live from a captain
screenshot.** With nothing typed, the root list was returning seventeen
**Themes** rows ahead of everything else and a **Preferences → Launch at
Login** row underneath them. Neither is a useful answer to "I have not asked
for anything yet", while apps (top apps) and clipboard (recent entries)
genuinely are.

**Clipboard opted out of the empty root query later, on captain
instruction, and the reason generalises.** Apps are a useful thing to be
shown unprompted; the most recently copied thing very often is not. A
password manager's payload is already filtered (`is_privacy_marked`), but
an invoice, a client email or a chunk of somebody's source is not, and
rendering it the instant the panel opens puts it on screen in front of
whoever happens to be standing there. Typing is the signal that it was
actually wanted — and clipboard search itself is untouched, as is the
`Clipboard History` mode, which scopes to the provider explicitly and so
never passes through the root-list guard at all.

The fix is a defaulted trait method, not a guard inside `search()`, and the
reason is worth keeping: **`search("")` serves two opposite questions**. It
is both the root-list search with nothing typed *and* the call a surface
makes when it deliberately scopes to one provider and wants its whole list —
the `Themes` mode's browsable palette list, and the **Preferences window
loading its own values**. A guard inside `search()` would have silently
broken the window, which is exactly what a first attempt at this did before
that call site was noticed. `themes.rs`'s own doc comment had named this
distinction for a long time without anything being able to act on it; there
was no way for a provider to express it until this method existed. The
daemon applies it in `handle_request`'s **root-list branch only** — the
scoped branch returns earlier and never sees the filter.
Pinned by `server.rs`'s own
`an_empty_root_query_never_returns_settings_or_theme_rows_but_a_scoped_one_still_does`,
which asserts both halves.

**Verified over the real wire protocol against a real daemon**, not just by
unit test: an empty root query returns Applications / Commands / Clipboard
and nothing else; "hotkey" in the root list leads with **Preferences → Summon
Hotkey ⌥Space**; the scoped `preference` search returns all three rows with
live accessories; the scoped `folder-scope` search returns the configured
folders; `folder-scope` is absent from a root query for "documents". Both
mutations were exercised and then **reverted, leaving no residue** — the
launch-at-login toggle really did write and then remove
`~/Library/LaunchAgents/com.neko.launcher.plist` with the correct client path
inside it, and a folder add/remove round-tripped with a bogus path correctly
rejected inline (`not a folder: /nope/not/real`).

### Three real defects in one control, none visible from reading the code

The Summon Hotkey recorder said "Listening…" and recorded nothing. Three
separate causes, found one at a time, each only by a live readback — two
wrong hypotheses were killed by logs before the third stuck. **The general
lesson: a control that takes keyboard input has three independent
preconditions, and failing any one of them looks identical from the
outside.**

1. **The window has to be key, and one opened from the summon panel is
   not.** The panel is `NSNonactivatingPanelMask` by design (style-mask
   readback `0x8080`, bit 7) — clicking the launcher must not steal
   activation from whatever you were working in — so a window opened from it
   inherits an inactive app: visible, clickable, never key. **AppKit
   delivers clicks to a non-key window but not key events**, which is
   exactly the "the button responds, typing does nothing" shape.
   `open_window` now calls `cx.activate(true)` (make *neko* frontmost) *and*
   `window.activate_window()` (make *this window* key), with
   `material::is_key_window` read back and logged — same "verified, not
   trusted" discipline as `verify_installed`.
2. **Being key is not being focused.** `window.focus(...)` is GPUI's own
   internal focus: it draws the caret and routes actions, and **cannot pull
   real OS keystrokes into a window the OS does not consider key**. Both are
   required, and they fail independently. (This one turned out *not* to be
   the remaining bug — the readback said `focused before=true` — but the
   distinction is what the first two rounds of debugging got wrong.)
3. **A key recorder must listen in the capture phase.** GPUI matches key
   bindings and dispatches actions **between** the capture and bubble
   phases, so a bubble-phase `on_key_down` only ever sees keystrokes nothing
   else wanted. Measured live: an unmodified **Escape reached a bubble
   handler while ⌃⇧K never did** — and a hotkey is made entirely of modified
   keystrokes. `capture_key_down` is the fix, and this repo already knew it:
   the earlier in-panel version of this same control used capture phase
   deliberately and said why. The knowledge was lost moving the control into
   a window, because onboarding's structure was copied instead — and
   onboarding binds almost nothing, so the phase never mattered there.

**Not verified: anything rendered on screen.** No window-scoped screenshot
was taken, and the recorder has never been driven by a real keypress —
synthesizing one is forbidden here. The window's layout, the tab bar's
traffic-light clearance, the painted toggle switch and the `Glyph::Sliders`
mark are all unproven visually. **The traffic-light clearance is the most
likely thing to be wrong**, since it reuses onboarding's constant against a
different header layout.

## Agents: what is running right now

`fm/neko-preferences`, from the captain's own *"I also want the feasibility
of seeing all the agents that are running on my device."*
`neko_core::agents::AgentsProvider` (id `"agent"`, section "Agents"), the
eighth provider.

**Source of truth is Paseo's own on-disk state**, `~/.paseo/agents/
<workspace>/<uuid>.json` — one plain JSON document per agent. All three
candidate sources were checked against the same machine before choosing:

| source | what it reported | verdict |
| --- | --- | --- |
| the JSON files | 2 running, 53 idle, 172 closed | **chosen** |
| `ps` (process table) | 2 live `claude` processes | agreed with the files |
| Paseo's MCP `list_agents` | **1** running | under-reported — not built on |
| the `paseo` CLI | correct, but a subprocess per search | nothing to buy; the data is a file |

A source that disagrees with the process table about what is running is not
the one to build on. `apps.rs`/`files.rs` pay a subprocess cost for `mdfind`
because Spotlight has no in-process API; here there is no such excuse.

**Every live agent on the machine was Paseo-hosted** — both running `claude`
processes had `Paseo Daemon` as their parent and **no controlling terminal at
all** (`tty: ??`). That is why the captain's own "if it's a claude session in
a Ghostty tab, focus that tab" is **not built**: there was not one real
instance to build against, and terminal-tab focusing needs Accessibility APIs
and a process→tab mapping that would have been written entirely on
speculation. `agents::Backend` is the enum a second host slots into.

**`Provider::answers_empty_root_query` returns `true` here, unlike themes and
preferences.** "What is running right now" is exactly what is worth seeing
the moment the panel opens, and it is self-limiting — two rows on a real
machine, not two hundred. Idle agents need a query, because they are context
rather than news, and they outnumber live ones roughly 25:1.

**Two settings, an Agents tab, and a `Backend` seam that exists because it
was asked for.** The captain's *"Paseo is something we use currently, maybe I
change to something else later"* is a stated requirement, not an imagined
one — so `Backend` is an enum with one variant, and adding a second costs a
variant, a read function, and one arm. It is deliberately **not** a plugin
system: each backend reads a different tool's own on-disk format, and there
is nothing generic to abstract until a second exists to compare against. The
tab shows the source as a **statement, not a picker** — a dropdown with one
entry is a promise the app cannot keep — along with a live census
(`2 running · 53 idle · read from ~/.paseo/agents`), because a source of
truth you cannot see is one you cannot debug when the list looks wrong.

**Unverified, and shipped anyway: the deep link.**
`activate` opens `paseo:/h/local/agent/<id>`. The route shape is read from
Paseo's own bundled `@getpaseo/protocol/agent-deep-link`
(`buildAgentDeepLinkRoute`); `local` as the server id is **inferred** from
`=== "local"` comparisons in the same bundle and has never been opened. If
Enter on an agent row does nothing, this is the first thing to check.

**Spawning agents is now built** — see "Starting an agent: the New Agent
command" immediately below; this section's own prediction ("`paseo run
<prompt>` would be a command row") turned out to be exactly the shape.
**Still not built**: transcript sources (`~/.codex/sessions`,
`~/.claude/projects`, `~/.grok/sessions`). That one is deliberate —
`jazzyalex/agent-sessions` prices each at ~1,000 lines in its own
`docs/adding-a-session-source.md`, and it answers "what did an agent do", not
"what is running".

**The grid above the search field.** Running agents render as tiles above the
input row rather than as rows in the list, because they answer a different
question: the list is "what did you ask for", the grid is "what is happening
without you". Two things about it are load-bearing:

- **Its height comes out of the row budget, it does not grow the panel.**
  `PANEL_HEIGHT_PX` is fixed for the process's whole lifetime and the real
  `NSWindow` is never resized ("Mode view resize seam"), so anything drawn
  above the input row is space the rows no longer have. Getting this wrong
  does not look like a layout bug — it looks like the last row being clipped
  by `overflow_hidden`, the exact defect `fit_within_budget` exists to
  prevent. `panel::Root::agent_grid_height` returns `0.0` when there is
  nothing to show, so a machine with no agents running loses no space at all,
  and a `const _: () = assert!(...)` makes a grid taller than the content
  area a **build** error rather than a runtime surprise.
- **`split_agent_tiles` moves live agents out of `results`, never copies
  them.** The same agent as both a tile and a row is two things to press
  Enter on for one agent. It keys on the `LIVE` badge the provider already
  sets rather than on `kind == "agent"`, so an idle agent stays an ordinary
  row. Never applied inside a mode — a mode is one provider's own list.

**Tiles are keyboard-reachable, using the same Up/Down as the list.** That is
forced rather than chosen: Left and Right are bound to the search field's own
cursor movement (`main.rs`'s `cx.bind_keys`, `"TextField"` context), so a grid
claiming them would break typing to reach it. Treating the tiles as rows that
happen to sit above the input costs no new keys at all — `Root::grid_selected`
is `Some` while the selection is up in the grid, Up from the first result
walks into it (landing on the tile *nearest* the list, so the selection moves
by one visually rather than across the whole strip), and Down off the last
tile walks back out. Up at the top stays put rather than wrapping to the
bottom of the list, which would read as the selection teleporting. A focused
tile owns Enter, is cleared by `reset_for_summon` like every other
per-summon state, and is **clamped** when a response arrives with fewer tiles
— an agent finishing between two responses must not leave the selection
pointing at nothing.

## Starting an agent: the New Agent command

`fm/neko-new-agent`, closing the gap the section above listed as not built.
`neko_core::new_agent` is the write half of `agents.rs`'s read half — **read
that module's own doc comment before touching this area**; it is the normative
statement of everything summarised here.

**The shape cost exactly what `modes.rs`'s accounting promised, plus one wire
field.** One `CommandSpec` (`New Agent`), one `ModeChrome` (`new-agent`,
`has_detail: false`), one `Provider` registered in `AppState::mode_providers`
(not the root list — a directory to start an agent in is not an answer to a
root query). No change to `enter_mode`/`exit_mode`/`run_search`'s scoping
branch, and no new `Request`/`Response` variant.

**The one protocol touch, and why the two cheaper alternatives are wrong.**
`Request::Activate` gained `query: String` — the search field's contents at the
moment Enter was pressed — and `Provider::activate_with_query` is the defaulted
trait method that receives it (delegating to `activate`, exactly the
arrangement `search_cancellable` has with `search`, so every other provider
implements nothing). It exists because this is the first row whose action takes
an **argument**: the row is a working directory, the query is the prompt.
Both alternatives were considered and are silently wrong:
- **Fold the prompt into `SearchItem::id`.** The id then changes on every
  keystroke, and `panel::resolve_selection` follows the highlight by
  `(kind, id)` — so a captain who picks a project and types one more word is
  returned to the first row, and Enter starts the agent in the wrong
  repository. Silent, and it produces a *wrong action* rather than an error.
- **Let the provider remember the last query it was searched with.** That races
  the daemon's own documented out-of-order request completion
  (`handle_connection`), which can leave a stale prompt behind — same class of
  failure.

**Two things are chosen, never inferred, and both are shown on the row.**
- **Where.** Rows are the projects Paseo already knows about,
  `~/.paseo/projects/projects.json` (8 real entries, one per root, ordered by
  recency) — deliberately **not** `workspaces.json` (200 entries, 21 live, the
  same directory repeated up to four times plus transient worktrees). The
  daemon's own `cwd` is meaningless (it inherits whatever started it, often
  `/`), `$HOME` is worse than nothing, and a hidden preference would put the
  most consequential input behind a settings window.
- **Which tool.** `paseo run` **requires** `--provider` — verified live
  (`MISSING_PROVIDER`), with no default anywhere in `~/.paseo/config.json`. So
  the provider is the one the most recent real agent used *in that directory*
  (`agents::provider_usage`, which reads closed agents too — 172 of 227 on this
  machine), falling back to the most recent anywhere. A machine with no agent
  history cannot be answered honestly: the row says "Start one from Paseo
  first" and Enter refuses, rather than this app deciding which coding tool
  somebody uses.

**The query is the prompt, not a filter — the only provider here where typing
does not narrow the list.** Every project is returned for every query, in the
same order, which is also what keeps the row ids stable. Known gap, stated
rather than hidden: there is no way to reach a directory Paseo has never seen.

**`--cwd` is not authoritative when the CLI can see it was launched from
inside another agent — found live, and it is a real production defect, not a
test artifact.** The one verification spawn ran with `--cwd /tmp/…` and
produced an agent whose real `Cwd` was `/Users/nish/Documents/neko`: the
inherited `PASEO_AGENT_CWD`. The CLI's own bundled source states the rule
(`resolveRunWorkspace` in `app.asar`): workspace precedence is `--workspace`,
then `$PASEO_AGENT_ID` ("daemon resolves the caller's workspace"), then
`$PASEO_WORKSPACE_ID` ("exported by workspace terminals"), and only then a
workspace minted for `cwd`. `--cwd` *is* passed in the agent-scoped case and
still loses, because the daemon resolves it server-side from `callerAgentId`.
`neko-daemon` inherits the environment of whatever started it, so a captain
running neko from an agent session or a Paseo workspace terminal would have hit
this on every spawn. The child's environment is now stripped of all three
variables (`new_agent::AGENT_SCOPING_ENV`), which drops the CLI to the
mint-a-workspace-for-cwd case. **Not re-verified live** — that would have cost
a second real agent, and the brief allowed one.

**Failure is inline and never silent.** Activation waits for the CLI (bounded
at `CONFIRM_TIMEOUT`, 20s) and returns `Response::Error`, which
`panel::Root::activation_error` renders in the footer with the panel left
open. Three findings shaped that:
- **The CLI reports errors as JSON on `stdout`, not `stderr`**, exit code 1 —
  reading stderr alone produced a useless "exited with status 1".
  `json_error_message` reads `error.message` + `error.details`.
- **A timeout claims neither outcome** ("may still be starting; check Paseo")
  and, unlike `files.rs`'s `mdfind`, **does not kill the child** — the agent
  may already exist, and killing it while reporting failure is the worse lie.
- **It blocks its own request thread for the CLI's duration** (~1s of Electron
  boot; the one measured end-to-end run took **2.88s**). That is not the daemon
  blocking — `handle_connection` gives every request its own thread, and
  nothing here touches the `Db` mutex or the reader loop. The cost buys the
  honest answer. A "starting…" tell in the panel, or Paseo's own local RPC port
  (private and undocumented, deliberately not reverse-engineered), are the two
  ways to remove it later.

**Verification.** Hermetic tests only, plus **one** real agent, created once and
immediately archived and deleted (agent count 31 → 32 → 31, temp directory
removed). The spawn seam is injected (`new_agent::AgentSpawner`, the same shape
as `agents::AgentsProvider::with_root` and `panel::AppearanceSetter`) — there is
no env var and no dry-run flag, so the only way to reach the real CLI is to hold
a `PaseoCli`, and no test does. The live run also proved the read half against
the captain's own machine: 8 real project rows, each naming the tool actually
used there. **Not verified**: anything on screen (no client launch, no
screenshot), and the env-stripping fix above.

## The shared pulse clock, and the repeating-animation rule

`motion::PulseClock` — the seam `motion.rs`'s own doc comment reserved, now
built for the LIVE agent badge. **It is the only sanctioned way to drive a
repeating animation in this app**; nothing in the catalog may repeat on its
own. The rule comes from comet's recorded incident: one
`with_animation(..).repeat()` element pinned a window at 120Hz and measured
36% CPU.

Three properties, all load-bearing:

1. **Ticks at `PULSE_INTERVAL` (80ms, 12.5Hz), not at frame rate.** Asserted
   by a test rather than left to review — a regression to frame rate here
   would be silent.
2. **Stops completely when unused**, and is driven **from `render`**, not
   from `run_search`. Only the render pass knows whether a live row was
   actually *painted* (`fit_within_budget` drops what does not fit) and
   whether the window is even on screen — which the panel is not, almost all
   of the time. A clock ticking behind a hidden window is the same defect in
   a slower disguise.
3. **Never starts under reduce-motion**; `intensity()` returns a fixed
   midpoint so callers never branch and a live row still reads as live
   without moving.

Measured: **resting CPU with the panel hidden, 0.52%** — unchanged from
before the clock existed, which is the number that matters. The badge
interpolates *alpha* rather than swapping colours, so it reads as one thing
brightening rather than two states flipping.

## Third-party UI, re-evaluated: gpui-component cannot be a dependency here

The captain asked twice about component libraries. Recorded so nobody
re-runs it:

- **`rust-ui.com` (Leptos) and `dioxus.rust-ui.com`** are Tailwind/DOM
  component registries. neko has no DOM and no CSS engine — `div()` here is a
  layout struct compiled to Metal draw calls. Adopting either means putting a
  webview under neko. **Rule: a UI library is only a candidate for this repo
  if it targets GPUI.** That one line rules out nearly all of them.
- **`longbridge/gpui-component`** (Apache-2.0, 13.3k stars) does target GPUI,
  and **still cannot be used** — measured both ways, not argued:
  - added as-is → `error[E0308]`, with the compiler's own note *"there are
    multiple different versions of crate `gpui` in the dependency graph"*.
    It depends on **crates.io** `gpui`; neko runs the **wingleeio fork**.
    A plain `cargo check` **passes** in this state and is a false positive —
    it only fails once something actually calls a component.
  - forced onto one gpui via `[patch.crates-io]` → **13 × `error[E0432]`**,
    `no Corner in the root`, `no Timer in the root`. It is written against
    crates.io gpui 0.2.2 and the fork has drifted.

  The only remaining route is abandoning the fork, which costs
  `paint_backdrop_blur`, `EdgeFade`, native window drag, the
  `windowDidBecomeKey:` deadlock fix and the ~30ms summon patch. Not taken.

**`components/keycap.rs` is what came of it.** gpui-component's `kbd.rs` was
read and declined on evidence rather than on principle: ~250 of its 324 lines
are `format(Keystroke) -> String` and `binding_for_action` lookups, both of
which neko already has or does not need, and the valuable ~30 lines are a
styled div that must be rewritten against this app's tokens anyway.
Importing mostly-duplicate code to obtain a div is the worse outcome. neko's
version renders **one cap per key**, which is what Raycast does and what
makes `⌃⇧K` read as three keys rather than one string.

**Built — see "Icons: real SVGs, and the one that stays painted" below.** This
paragraph used to read "still open, and the better path than any library:
real icons via `gpui::svg()` … Lucide is MIT." The route was right and it was
taken. **Two corrections it forces on this section's own text**: Lucide is
**ISC**, not MIT (verified from its own `LICENSE`; permissive either way, but
the record was wrong), and gpui-component's published crate ships **zero**
`.svg` files — its `icon.rs` names paths a consuming app's `AssetSource` must
resolve, so adopting it would not have supplied icons even if the gpui-version
mismatch above had not already ruled it out.

## Icons: real SVGs, and the one that stays painted

`fm/neko-icons`. Full record, including what was and was not verified live:
`docs/evidence/icons-svg-report.md`. **Read `crates/neko/src/assets.rs`'s
module doc comment before touching anything icon-shaped** — it is the
normative statement of the constraints below.

**What changed**: `panel::glyph_element`'s hand-composed `div()` stacks,
`search_glyph` and `back_glyph` are now vendored Lucide SVGs drawn with
`gpui::svg()`. The wire vocabulary is untouched — `neko-protocol`'s `Icon`/
`Glyph` are exactly as they were, and no provider changed. This is a
client-side rendering change only.

**What made it free**: `gpui::svg()` was already in the gpui this crate
compiles against; all that was missing was an `AssetSource`. `assets::
NekoAssets` is a compile-time `include_bytes!` table (nine files, ~3KB total,
no `include_dir`/`rust-embed` — a directory walk buys growth this closed set
does not have and costs a build script), installed on the `Application`
builder via `with_assets` **before `run`**, because that call is also what
rebuilds gpui's `SvgRenderer` around the source.

**The one constraint everything else follows from: gpui renders an SVG to an
alpha mask, tinted by the element's own `text_color`.** Three consequences,
all load-bearing:

1. **Colour still comes from `theme::active()`**, at paint time, at every
   call site — so a theme change re-tints every icon with no per-theme asset,
   no cache to invalidate, and no `if themed` branch. Swapping to SVG changed
   the *shape* source, never the colour source.
2. **An icon can only be one colour.** `Glyph::Palette` — four swatches in
   the *live* theme's own colours — therefore **stays hand-painted,
   permanently, not pending an asset**; a single-tint palette swatch is not a
   palette swatch. `assets::glyph_icon` returns `None` for exactly this one
   variant, and that `None` is what routes to `panel::palette_glyph`.
   `Glyph::Agent`/`AgentLive` keep a painted presence dot composited over the
   SVG for the same reason (two tints, one mask) — preserving the documented
   "same mark either way, live ones still pick themselves out" behaviour. The
   dot moved to the slot's bottom-right corner because Lucide's
   `square-terminal` puts its prompt caret where the old dot sat.
3. **A filled icon renders as a solid blob**, since coverage is all the mask
   keeps. Lucide's set is stroke-only, and a test asserts every vendored file
   is `fill="none"` on a 24×24 grid rather than trusting that.

**A missing icon is silent — this is why the tests look the way they do.**
`Window::paint_svg` on a path the asset source cannot serve returns `Ok(())`
and draws nothing: no panic, no log, just a hole in a row. So `assets.rs`
resolves every named path, asserts no vendored file is unreachable from a
name, and — the load-bearing one — **rasterises every file through gpui's own
`SvgRenderer` and asserts the result has non-zero alpha coverage**, which
catches a file that is valid SVG containing nothing. `glyph_icon`'s `match` is
exhaustive, so a new `Glyph` variant is a compile error that forces the real
decision: name a file, or return `None` and paint it.

**Vendored byte-for-byte, pinned two ways.** Files live in
`crates/neko/assets/icons/lucide/`, unmodified — size and colour are applied
at the call site (`theme::ROW_ICON_GLYPH_PX`), never by editing a file, so a
`curl`-and-`diff` against the pinned upstream commit
(`33a44aa8b0b43d9b0ed14eb08860a1b5550a1573`) stays meaningful. Byte lengths
are pinned in the test suite, the same discipline `theme.rs` uses for
vendored palette hex.

**Licence: ISC, verified from Lucide's own `LICENSE` on 2026-08-23 — not a
badge, and not the "MIT" this file previously recorded.** Four of the nine
(`chevron-left`, `clipboard`, `link`, `search`) additionally carry Feather's
MIT grant. Both notices are reproduced verbatim in
`THIRD_PARTY_LICENSES/lucide-ISC.txt`; `NOTICE` and
`crates/neko/src/components/vendor/MANIFEST.md` carry the rest of the
paperwork. **Icon assets are the same audited category vendored palette
values already are** (see "Licence rule" below): no third-party *code* was
vendored, and gpui-component's `icon.rs` was read and declined on evidence
(372 lines of enum-to-string `match` plus a sizing wrapper — nothing this app
does not already have three lines of).

**What stays painted, deliberately, besides `Glyph::Palette`**:
`components::glyphs::opt_glyph`/`neko_wordmark_glyph` (⌥ is in no
general-purpose icon set; the wordmark is neko's own identity) and
`panel::app_icon_placeholder_glyph` (its whole job is reading as a *different*
kind of mark — "a real per-app raster is still warming", not "this row has no
artwork").

**A real defect found on the way**: `search_glyph`'s own comment claimed "a
circle + a diagonal stroke", but the code was a bare `.rounded_full()
.border_2()` — a ring with no handle, reading as a dot. `div()` has no
rotation, so the diagonal was presumably dropped as undrawable and the comment
never corrected. That is the general cost of assembling a mark from layout
primitives, and it is what this change removes.

**Verified live** on the release binaries under the standing isolated-`HOME`/
`verify_harness`/window-scoped-capture discipline, `key window false` at every
capture: `search`, `clipboard`, `text-align-start`, `square-terminal`+dot,
`sliders-horizontal`, `chevron-left`, and `Glyph::Palette` still painting each
theme row in its own palette (`docs/evidence/icons-svg-*.png`). **Not seen on
screen**: `file`, `folder`, `link` — Spotlight cannot index the isolated
`HOME`, and the harness seeds no URL entry; all three rasterise headlessly and
take the identical code path, but that is an argument, not a picture. **Not
measured**: summon latency and memory — `paint_svg` keys the sprite atlas on
`(path, size)` and rasterises only on a miss, so the per-frame cost should be
nil after first paint, but nothing was benchmarked.

## Mode-view row anatomy and the neutral re-tone

`neko-mode-visual`, prompted by a captain screenshot of the clipboard mode
view looking broken. Full account, live evidence, and the WCAG contrast
recomputation: `docs/evidence/mode-view-and-neutral-palette-report.md`.

**The window resize — and the native material tracking it — are both
correct, verified live three independent ways, not just read.** The
captain's report described the mode view as if the window never actually
widened to `theme::PANEL_WIDTH_WITH_DETAIL_PX` (clipped preview, Information
rows with labels but no values, a truncated footer). A pixel measurement of
his own screenshot narrowed this further: panel width ~757pt (matching a
760pt window), but drawn content stopped at ~680pt — an exact 80pt unbacked
strip, his desktop visible straight through it — pointing at `material.rs`'s
native background view (`NSGlassEffectView`/`NSVisualEffectView`) not
tracking the window's resize, rather than the window itself. **Tested
directly and not reproduced.** `install_glass`/`install_popover`
(`material.rs`) already set `NSAutoresizingMaskOptions::ViewWidthSizable |
ViewHeightSizable` on the background view at install time; a temporary
diagnostic read back the real `CGRect` of `contentView.subviews()[0]`
(the actual background view) immediately after a real mode-entry resize and
found it exactly matching the window's new content size — `760.0 x 420.0`
— on *both* the Glass and the forced-Popover fallback path. A pixel scan of
the reproduction screenshot independently confirms fully opaque content
extending symmetrically to the window's own shadow margin on both edges, no
asymmetric gap. This ran on the same physical machine and macOS build
(26.5.1) the captain's own session was on, ruling out an OS-version
explanation. Per this investigation's own explicit instruction, **no fix was
forced onto a mechanism three independent measurements found working** — no
change was made to `display_placement::resize_and_recenter`,
`panel::enter_mode`/`exit_mode`, or `material.rs`. One scenario remains
genuinely untested: many real, spontaneous summon/dismiss cycles (not just
mode enter/exit) before a mode entry — a diagnostic attempting to model this
via repeated programmatic `cx.activate`/`cx.hide` calls stalled in the
harness itself after two cycles, almost certainly a harness artifact rather
than a reproduction, so this was abandoned rather than reported as a
finding. If the captain's original symptom recurs, treat it as a fresh,
unreproduced report — the next useful evidence is either a screenshot
timestamped at the exact instant of the real keypress (a single-frame race
this task's ~800ms settle window wouldn't catch), or a repro after a long
real session with many genuine summon/dismiss cycles first.

**The real, reproduced defect: `panel::render_row` was one function shared,
unmodified, between two rows of very different width.** The root list
(680px, plenty of room) and the mode list's own column
(`theme::MODE_LIST_COLUMN_WIDTH_PX`, 264px) both rendered `item.subtitle`
and `item.accessory` — for a clipboard entry, `"Copied from {app}"` plus a
relative-time stamp — cramped in beside an already-truncating title, badge,
and icon. Reproduced live with real fixtures: `"remove con  Co   TEXT
now"`, the title cut off mid-word colliding with the start of its own
subtitle. `data/neko-design/mockups/12-first-clipboard-use.html`'s own row
anatomy settles this as a real bug, not a taste call — the mode list's rows
never carry a subtitle or accessory at all; `Application`/`Copied` already
have a dedicated, unhurried home in the detail pane. Fixed with a `compact:
bool` parameter on `render_row` (root list passes `false`, unchanged; the
mode list passes `true`, dropping `subtitle`/`accessory`) — no geometry,
spacing, or type change, only which optional fields a narrow row is allowed
to draw. **The general lesson**: a row renderer shared across two
differently-sized containers needs to know which container it's in if the
design's own row anatomy differs between them — sharing the function is
right (no per-provider knowledge either way), sharing every field it draws
isn't automatically right too.

**The neutral re-tone: chroma to zero, `L` untouched, on every chrome
token.** Reverses the captain's own earlier "monochrome with a hint of
blue" call (`37b9800`) per a second direct instruction — see "Design
tokens" above for the full before/after and the recomputed WCAG contrast
table (every figure holds within rounding; chroma's effect on relative
luminance at fixed OKLCH `L` is negligible at these lightness levels).
`STATE_SUCCESS`/`STATE_DANGER` and their derived tokens are untouched —
state colours, not chrome. `data/neko-design/report.md` §1's own OKLCH
table (the firstmate home, not this repo) was left as-is — it describes the
frozen mockup HTML/CSS files verbatim, which are still warm and were not
edited either re-tone — with an added note pointing at this repo's own
evidence files as the current source of truth for the shipped app's actual
colours.

## Mode view resize seam

`neko-mode-resize`, fixing a real defect two prior tasks
(`docs/evidence/mode-view-and-neutral-palette-report.md`) tried and failed
to reproduce: the clipboard mode's window widened to 760pt but only 680pt
was actually drawn into, leaving a transparent strip (the desktop visibly
showing through) and clipped text on the right. Full diagnosis, the four
workarounds tried and ruled out, and the before/after measurements:
`docs/evidence/mode-resize-seam-fix-report.md`. Summary:

**The reproduction the prior tasks were missing**: entering a mode
*immediately* after the window's first-ever appearance (their own repro
shape) always worked. The captain's real sequence — summon and dismiss the
panel some number of times first, *then*, from a later summon, enter a
mode — reliably reproduced the seam on the very first attempt.
`evidence.rs` gained `NEKO_REAL_CYCLES_BEFORE_SHOW=<n>` to drive that real
sequence (see its own doc comment for why it uses
`order_front_regardless`/`order_out`, not the real `activate_window`/
`cx.activate` path, to avoid colliding with the unrelated, already-
documented `windowDidBecomeKey:` deadlock below) — a permanent addition to
this repo's evidence tooling, not a one-off script.

**Root cause, confirmed by direct native readback, not inferred**: the old
`display_placement::resize_and_recenter`'s raw `NSWindow` resize genuinely
worked — `contentView`, GPUI's own rendering `NSView`, its `CAMetalLayer`,
and the Metal drawable itself all correctly tracked the new size, every
time, confirmed live. What silently didn't track it was `gpui::Window`'s
own private `viewport_size` — the size `draw_roots` (`gpui-0.2.2/src/
window.rs`) actually paints and clips the frame against — which only
resyncs via native `on_resize`/`on_moved`/`on_active_status_change`
callbacks that, per direct testing, stop firing reliably on this window
once it's been shown/hidden a few times. **Four different public-API
workarounds were tried and all failed** to force that resync (`window.
refresh()`, `window.resize()`, a real `-setFrameSize:` value-change nudge,
and forcing AppKit's own deferred layout via `-layoutSubtreeIfNeeded`/
`-displayIfNeeded`) — `viewport_size` stayed stale through all of them,
confirmed by reading the public `window.viewport_size()` accessor well
after each attempt, not just synchronously. This is a real `gpui-0.2.2`
staleness this codebase cannot reach around from outside the crate (the
field and the resync method are both private), not a bug in this repo's
own resize/positioning math — which the prior tasks' own readbacks had
already shown correct.

**The fix removes the operation that triggered the staleness, rather than
working around it**: the real `NSWindow` is now created once at
`theme::PANEL_WIDTH_WITH_DETAIL_PX` (760) and never resized again for the
rest of the process — `resize_and_recenter` is deleted, not just unused.
`panel::Root::render`'s own stage element is always `w(PANEL_WIDTH_WITH_DETAIL_PX)
.flex().justify_center()`; the narrower root-list panel centers inside it
via ordinary GPUI flexbox, provably unaffected by `viewport_size`'s own
tracking since the window's real size genuinely never changes any more.
`material::set_background_frame` (new) moves/resizes the installed native
backdrop directly — one synchronous `-[NSView setFrame:]` call, the same
centering formula `justify_center()` produces, no window resize and no
dependency on the broken callback chain. `panel::Root::update_background_bounds`
(replacing `resize_panel`) is the one call site, from `enter_mode`/
`exit_mode`/`reset_for_summon`.

**Disclosed cost, verified rather than assumed**: the real window's own
footprint is 760pt at rest now, not 680pt — an internal fact only.
`theme::PANEL_ROOT_INSET_PX` (40pt, half the width gap) centers the 680pt
root-list panel inside that wider window, landing its own visible left
edge at exactly the same on-screen position the old 680pt-wide window's
left edge sat at — proven algebraically and confirmed live
(`docs/evidence/mode-resize-seam-fix-report.md`'s own pixel measurements).
The 40pt margin outside the visible panel carries no material and no
shadow of its own (macOS's native shadow follows the actual painted pixels
for a transparent-backed window, confirmed in the same screenshots) — the
root list's resting state is visually unchanged.

**That 40pt margin is still real, clickable `NSWindow` frame, though — a
second real defect this task's own fix introduced, caught in review before
landing, not by the original acceptance criteria.** The window is always
`PANEL_WIDTH_WITH_DETAIL_PX` now, in *every* mode, not just detail mode; a
click landing in the margin (present on all four sides whenever the panel
is narrower than the window — i.e. the common root-list case, not just
detail mode) used to hit nothing (`justify_center()`'s implicit gap has no
gpui element covering it at all). Checked directly against `gpui-0.2.2`'s
own source (no `ignoresMouseEvents`, no custom `-hitTest:` override
anywhere in the crate) rather than assumed: standard AppKit hit-testing
means that click is captured by neko's own window (already key/frontmost,
`NSPopUpWindowLevel`) regardless — it neither reaches whatever's behind
the window on the desktop (no real click-through without
`ignoresMouseEvents`, which is a whole-window property and would also make
the *visible* panel unclickable) nor dismisses the panel (`main.rs`'s
click-outside-dismiss, `cx.observe_window_activation`, only fires on an
actual activation *change* — a click that stays inside neko's own window,
margin included, is never that). Two real, silent regressions from one fix:
a click there did nothing at all, where before this task there was no such
region for a click to land in.

Fixed by giving the margin its own real gpui presence:
`panel::Root::render_dismiss_margin` renders two explicit divs (left/right,
each `(PANEL_WIDTH_WITH_DETAIL_PX − panel_width) / 2`, `0`-width and so
harmless whenever the panel already fills the window) in place of
`justify_center()`'s implicit gap, each with an `on_mouse_down` that calls
`cx.hide()` — restoring the click-outside *behavior* the captain already
relies on, for a region that can no longer be true click-through short of
a second overlay window or patching `gpui`'s own hit-testing (neither
undertaken here — see the launch brief's own "whichever is cleaner"
framing between the two, and `ignoresMouseEvents`' whole-window scope
above for why it wasn't the one picked). **Live click confirmation
(pixel-position mouse-down/up, either via `System Events` or a
`CGEventPostToPid`-scoped synthetic event) was attempted and abandoned partway**:
a `System Events "click at"` probe against unverified absolute screen
coordinates landed on a real, unrelated app on this shared machine before
its risk was fully appreciated (harmless as far as could be confirmed —
an accessibility-hierarchy read, not a destructive action — but a real
lesson: never post a synthetic click at raw screen coordinates without
first confirming exactly what occupies them); switching to
`CGEventPostToPid` (confirmed safe — scoped to one process's own windows
by the OS, cannot address a different app regardless of coordinates) hit a
*separate*, real rendering staleness in this same long test session — data
proven correct via direct daemon queries and in-process debug reads, but
the on-screen frame never updated to reflect it, reproducible even on
completely unmodified, non-margin-related search-result rendering — that
made on-screen confirmation unreliable within the session and wasn't
chased further (a session/environment issue, not this fix's own
correctness, but not run to ground either). The fix itself rests on the
`gpui`-source-level hit-testing analysis above, not a live click; the next
person to touch this area should get a real, interactive click on the
margin from a clean session before trusting this description of the
mechanism as the final word.

**Correction to this section's own earlier claim, found by
`neko-double-panel`**: the text above ("The 40pt margin outside the visible
panel carries no material and no shadow of its own … the root list's
resting state is visually unchanged") is wrong. It does carry a shadow —
AppKit's own automatic window drop shadow, which (for this window's
Metal-layer-backed, near-invisible-alpha-background construction — see
"Window material" below) is computed against the **whole `NSWindow` frame**
rather than the actually-painted content, and was invisible only because
every screenshot taken by this section's own task happened to be judged by
eye against a low-alpha, blurred region rather than pixel-probed. Once the
window became permanently wider than the visible root-list panel, this
produced a real, captain-reported "two nested rounded rectangles" defect —
see "The double-panel shadow defect" below for the root cause, the fix
(`NSWindow.setHasShadow(false)`, since the panel `div`'s own `.shadow_lg()`
was always the shadow this app actually needed), and the corrected
measurements.

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

## The panel shadow tent — the second overlapping shadow source

`fm/neko-panel-shadow-tent`, fixing exactly the gap the correction just
above describes: the captain reported the same "black tent" a third time,
on a binary that already contained `fm/neko-double-panel`'s fix. Full
investigation, the single-variable test, alpha-channel measurements, and
the verification methodology: `docs/evidence/panel-shadow-tent-fix-report.md`.
Summary:

**Root cause**: `panel::Root::render`'s panel `div` also carried
`.shadow_lg()` — GPUI's own box-shadow, drawn as real, blurred, low-alpha
black pixels in the same scene, entirely independent of the AppKit window
shadow the prior task disabled. It's been in this file since the original
spine commit (`8888e33`); for most of the project's life the real `NSWindow`
was exactly the panel's own width, so the blur had nowhere to spill and was
invisible by construction — the identical reason the AppKit shadow was
invisible before `235bf88`. Once the window became permanently
`PANEL_WIDTH_WITH_DETAIL_PX` while the root-list panel stayed the narrower
`PANEL_WIDTH_PX`, both shadow sources gained the same `theme::
PANEL_ROOT_INSET_PX` margin to bleed into. Disabling the AppKit shadow
removed one of the two; this task's own subject was the one still standing.

**Confirmed by a single-variable test, not assumed**: `.shadow_lg()`
commented out, nothing else changed, window-scoped captures of the empty-
query root panel over a dark backdrop, before/after, alpha channel read
directly (this window is genuinely transparent, so alpha `0` means nothing
painted there at all). Before: a soft gradient from `0` up to `~17/255`
(~6.7%) right next to the panel's own edge, filling roughly the outer
two-thirds of the 40pt margin. After: alpha is exactly `0` everywhere
outside the panel's own opaque fill — a hard, clean edge. Prediction stated
in advance, confirmed by measurement.

**The fix**: `.shadow_lg()` is removed outright, not shrunk. It was already
fully contained within the 40pt margin (never spilling past the window's
own edge) — the problem was never its size, it was that it painted
anything at all into a region every other decision in this codebase
already treats as strictly transparent (the native backdrop material is
deliberately narrowed to the panel's own width; the AppKit auto-shadow was
disabled for painting into this exact margin). The translucent Liquid
Glass material already reads as an elevated surface through real vibrancy
with no drawn shadow needed; the opaque fallback keeps its pre-existing
`.border_1()` for edge definition, which has no spill risk. Clipboard mode
(760pt panel, 0pt margin — `render_dismiss_margin`'s `margin_width` is
exactly `0` there) was never able to leak this shadow either way, before or
after; the fix is unconditional, so both modes are affected identically.

**Scope note added by `fm/neko-top-line-titled`**: this fix and
`fm/neko-double-panel`'s are both correct and both stand — between them they
removed the two shadow sources spilling into the margin. Neither, however,
was the **top line** the captain has separately reported three times, even
though the two were sometimes discussed as the same complaint: that line is
AppKit's titled-window rim on the panel's own top edge, still present on
binaries containing both fixes. See "The top line — AppKit's titled-window
rim" below.

**Not obtained this task**: a live window-scoped screenshot of clipboard
mode itself. `evidence.rs`'s only non-synthetic-input hook that can drive a
mode transition (`NEKO_SHOW_QUERY`+`NEKO_SHOW_CONFIRM`) holds the summon
window active several seconds longer than a plain `NEKO_SHOW_ON_LAUNCH`
capture does, and on this shared, multi-agent machine that longer exposure
reliably lost real window activation to contention before capture —
confirmed directly with a temporary, reverted diagnostic
(`window.is_window_active()` read `false` at capture time on every
attempt, including immediately after an explicit re-`activate_window()`),
and separately confirmed the underlying application/search state was
completely correct throughout the same attempts (a second temporary,
reverted diagnostic on the daemon's own `Response::Search`). Ruled out
before concluding this: display sleep, timer starvation, and the backdrop
window's presence. The evidence report above has the full account, including
what was tried; the code-level argument above (a provably `0`-width margin
in that mode) stands in for the missing screenshot. `evidence.rs` itself
was not changed by this task.

## One constant panel width, and the real top line

`fm/neko-footer-hairline`. Full before/after screenshots and the pixel-level
diagnosis: `docs/evidence/footer-hairline-and-panel-width-report.md`.

**A wrong inference, corrected.** The captain reported "a top line, maybe we
can remove that" on the current build; this task's first pass inferred that
meant the footer's own horizontal hairline (`render_footer`'s
`.border_t_1().border_color(theme::BORDER_HAIRLINE)` — the only horizontal
rule anywhere in the panel) and removed it. Wrong: his own follow-up
screenshot showed the line he meant sits **outside the panel's top-left
edge, in the transparent margin**, roughly level with the panel's top edge,
ending where the rounded corner begins. **The footer hairline is restored,
unchanged from before this task** — the vertical divider inside the footer
(between `Open ↵`/`Paste ↵` and `Actions ⌘K`, named explicitly in the
frozen design, `data/neko-design/report.md` §2) was never touched either
way.

**The panel is now one constant width, 760px, root list and clipboard mode
alike** — a captain override of the frozen design's original two-width rule
(680px list-only / 760px only with a detail pane; `data/neko-design/
report.md` §2, updated in place to record the override). It has to be 760,
not 680: the real `NSWindow` was already permanently fixed at 760px for the
process's whole lifetime for the unrelated reason "Mode view resize seam"
above documents (gpui's own paint viewport goes stale after a show/hide
cycle, with no public API to resync it), so one constant width has to match
the window, not the other way around. `panel::Root::render` no longer
varies width by `active_mode`.

**What this let the implementation delete, since it was now genuinely
dead**: `render`'s centering stage element and its two margin `div`s
(`render_dismiss_margin`, including their click-to-dismiss handlers — no
margin once the panel always fills the window, so no dead zone to
compensate for); `theme::PANEL_ROOT_INSET_PX`; `Root::
update_background_bounds` and its three call sites
(`reset_for_summon`/`enter_mode`/`exit_mode`) — the native backdrop
`material::install` already puts in place at the window's own full
`contentView` bounds, once, at startup, which now matches the panel in
every mode with nothing left to reposition; `main.rs`'s startup call
narrowing that backdrop to 680px before the first summon; and
`material::set_background_frame` itself (the public wrapper and its macOS
impl), whose only two callers were the two removed just above. **What
stayed, and why**: `theme::PANEL_WIDTH_PX` (680) — `onboarding/view.rs`
still uses it for the onboarding window's own, unrelated size.

**The real top line, diagnosed rather than guessed at.** The leading
hypothesis handed off with the correction was Liquid Glass's own specular
edge rim (`NSGlassEffectView`). Tested directly with
`NEKO_FORCE_MATERIAL=popover` (skips `NSGlassEffectView`, forces the
`NSVisualEffectView` fallback): the same ~2px, fully-opaque, brighter-
than-fill top row exists under the panel with *either* material — so it is
not Liquid Glass–specific, and that line of investigation stops there per
the correction's own instruction. What actually produced a visible line
**outside** the panel, before this task's width fix, was a secondary,
softer artifact: that same top-edge brightening has a low-alpha falloff
past the panel's own rounded top corners, and with the old two-width
layout there was a real 40pt transparent margin between the 680px panel
and the 760px window for that soft falloff to become visible in —
confirmed by direct alpha-channel sampling of a pre-width-fix build
(`ccdbc64`, via a temporary `git worktree`, never disturbing this branch):
`rgba(248,248,248,38)` at the margin's top edge vs. `rgba(0,0,0,6)` a few
rows down, present nowhere else in the margin's height. **The width fix
above is also the fix for this**: with no margin left for the age to fall
into, a same-settings before/after top-left-corner capture confirms the
line is simply gone — no separate code change was needed or made.
`screencapture -l<windowID>`'s own documented limitation (real alpha, but
no live compositing with what's behind the window — "Window material"
above) is why this used the window's own alpha channel directly rather
than a synthetic backdrop image.

**Correction to this section's own claim, found by `fm/neko-top-line-titled`
— this is the second wrong attribution for the same symptom, after the
window shadow one two sections up.** "The width fix above is also the fix
for this … the line is simply gone" is wrong. The width unification removed
a *margin-side* artifact (the soft falloff of the panel's own top-edge
brightening, past its rounded corner, into the 40pt margin), which was real
and is genuinely gone — but the line the captain kept reporting is a
separate, harder one **on the panel's own top edge**, and it survived this
task untouched. Measured on `cda3765` (the build after this fix landed):
the top 2 device px read `(66,66,66)` and `(43,43,43)` against a
`(19,19,19)` fill, with every other edge flat `(19,19,19)`. The line-of-
investigation this section did correctly close stays closed — it is not
Liquid Glass's specular rim, confirmed here twice — but the reason it isn't
material-specific is that **it isn't painted by neko at all**. See "The top
line — AppKit's titled-window rim" immediately below.

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

## Comet craft pass: floating-layer discipline, a throttled motion catalog, `paint_layer`

`neko-craft-pass`, working from a read-only design study of `zeronsh/comet`
(MIT; `data/neko-comet-design/report.md` in the firstmate home, vendored
comet read at `refs/comet`) commissioned specifically to find what
neko's interaction craft could learn from comet's, on the captain's own
instruction to "read and write as close as possible" — read for pattern,
never copied. The report's own load-bearing finding shaped what was
buildable: **comet depends on a git fork of Zed's `gpui`, not the published
crate neko is deliberately pinned to** (see "The GPUI dependency decision"
below), so three of comet's primitives are simply absent from what neko
compiles against — confirmed by direct grep of the vendored `gpui-0.2.2`
source, not inferred from comet's `Cargo.toml`: `window.paint_backdrop_blur`/
`BackdropBlur` (comet's `frost.rs`), `gpui::EdgeFade` (comet's
`edge_fade.rs`), and `App::reduce_motion()`/`set_reduce_motion()` (comet's
own automatic reduced-motion snap). None of the three was built here, or
worked around by reaching for the fork — the report's own explicit
instruction, honored.

**The `⌘K` actions menu now has the same floating-layer discipline comet's
`popover.rs:395-416` (`anchored_menu`) demonstrates**, reimplemented against
neko's own geometry in `panel::Root::render_actions_menu`/
`render_actions_trigger`:
- `deferred(anchored().anchor(Anchor::BottomRight)
  .snap_to_window_with_margin(px(8.0)).child(card))` — its own floating
  paint layer above everything else painted that frame, clamped to stay
  inside the real window when the trigger sits near an edge. Genuinely
  reachable in this app, not hypothetical: the clipboard mode's own detail
  view runs the panel at the full `PANEL_WIDTH_WITH_DETAIL_PX` with no side
  margin, putting the trigger right at the window's own edge.
- `.occlude()` on the card, so a click on the card's own dead space can't
  fall through to whatever's under the floating layer.
- `.on_mouse_down_out(...)` dismisses the menu on any click outside the
  card — deliberately not "any click anywhere," which would double-fire
  with the trigger's own click.

**The trigger-click-while-open race, reproduced from comet's own finding
and fixed the same shape, in neko's own terms.** The footer trigger sits
outside the menu card, so clicking it while the menu is open fires the
card's own `on_mouse_down_out` (capture phase, on mouse-down) *and* the
trigger's own `on_click` (bubble phase, on mouse-up) from the same physical
press — a naive toggle reads `actions_menu` after the outside-close handler
already ran, finds it `None`, and reopens a fresh menu instead of leaving
the captain's dismiss click alone. `Root::menu_open_before_this_press`
(a snapshot taken by a capture-phase listener,
`note_actions_menu_mouse_down`, registered on `render`'s own outer panel
div so it always runs before any descendant's own capture-phase handler)
is the fix: `handle_actions_menu_trigger_click` checks whether the menu was
open *before this press's capture phase ran at all*, not its current state.
Regression-tested directly against the three real handler methods in
dispatch order (`panel.rs`'s
`clicking_the_trigger_while_the_menu_is_open_does_not_reopen_it`), plus the
normal-path counterpart proving the guard doesn't suppress a genuine open.
**Escape closes the menu before the panel** — `handle_dismiss`'s existing
early-return structure already gave modes this shape; the menu branch was
added ahead of it, tested at `escape_closes_the_actions_menu_before_exiting_an_active_mode`
(the panel-hide case for a *root-list* Escape isn't observable headlessly,
the same pre-existing limitation `escape_exits_the_mode_instead_of_hiding_when_one_is_active`
already had before this task). The destructive delete action's two-step
confirm (`AGENTS.md`, "Commands and modes") is untouched by any of this.
`reset_for_summon` now also closes a stale open menu unconditionally, not
just inside the mode-exit branch — a window losing activation with the menu
open on a *root-list* row used to leave it silently open into the next
summon, since that path never routed through `handle_dismiss`.

**`components::layered::layered(...)`** wraps the menu card's whole paint
(background, border, rows) in one `Window::paint_layer` call — comet's
`frost.rs` recommendation 3, reimplemented from scratch (no backdrop-blur
concept, since that needs the fork-only primitive above). **The rule this
exists to make easy to follow, for any future floating/overlaid chrome**:
if it needs to guarantee its own internal paint order (a tint under
content, a control over a thumbnail, anything layered), wrap it in
`layered(...)` rather than relying on sibling `.child()` order staying
stable — cheap, and it closes off a class of bug this codebase hasn't hit
yet (comet's own `frost.rs` module comment: a hover repaint elsewhere
reassigning a card's own quads to the wrong relative paint order). Honest
scope, stated in `layered.rs`'s own doc comment: this is a `gpui`
scene-graph-level guarantee — it would not have prevented either the
double-panel shadow defect (AppKit's own window shadow, a level below
`gpui`'s scene graph) or the mode-resize-seam bug (a `gpui`-internal
`viewport_size` cache going stale, not a paint-order issue).

**A small, throttled `motion.rs` catalog** — two named specs
(`MENU_FADE` 120ms, `CONTENT_FADE` 150ms) over one shared decelerate
`CubicBezier`, plus `menu_fade_in`/`fade_in` element helpers wrapping
`.with_animation(...)`. Three call sites, no more, per the launch brief's
own tie-break ("motion is worth less than correctness"): the actions
menu's open transition (opacity + a 4px settle drift), a mode's content
reveal on entry, and a new "still searching" tell
(`panel::Root::render_searching_tell`) for a query that hasn't returned
within `SEARCHING_TELL_DELAY_MS` (150ms) — closing a real, previously-silent
gap where `files.rs`'s up-to-1.5s worst case (`AGENTS.md`, "Search and
ranking") left stale results on screen with zero indication a new answer
was coming. **The menu's own close, and summon itself, are deliberately
never animated** — see `motion.rs`'s own doc comment for why each is an
instant cut. **No repeating animation exists anywhere in this catalog** —
both specs are one-shot `gpui::Animation`s, which `gpui-0.2.2`'s own
`AnimationElement` stops scheduling frames for the instant they finish, so
neither a mounted-and-finished nor an unmounted fade costs anything at
rest. This is the seam this task's brief asked to leave for the day a
repeating animation (a spinner, a pulse) is actually needed: it must go
through a shared, explicitly-throttled clock — never
`with_animation(...).repeat()` directly — per comet's own `PulseClock`
finding, restated in `motion.rs`'s own doc comment: one repeating element
pinned a whole window at 120Hz, measured 36% CPU. Nothing here needed that
clock, so none was built.

**Reduced motion, read live, not assumed.** `motion::system_reduce_motion()`
reads `NSWorkspace.accessibilityDisplayShouldReduceMotion()` directly (the
same AppKit-read pattern `material.rs`/`spaces.rs` already established for
other native state), since `App::reduce_motion()` doesn't exist in
published `gpui-0.2.2`. Every element helper takes the flag explicitly and
skips `with_animation` entirely when it's set, rather than forcing the
animation's output to its end state — a skipped animation schedules zero
extra frames; a forced-but-still-running one would keep requesting frames
for its nominal duration for no visual benefit.

**Verified live, on the release binaries, under the same isolated-harness
discipline "Daemon concurrency" below established** (`verify_harness`, a
seeded clipboard fixture, an obscure committed hotkey, no real
`neko-daemon` launch): idle CPU with the panel visible and nothing
happening averaged **~0.16% over a 15s sample** (`ps -p <pid> -o %cpu`,
polled once per second) — indistinguishable from a resting GPUI app's
baseline (a blink-cursor timer and occasional wakeups, nothing from this
task's own catalog, since nothing here repeats). Warm summon latency
(`NEKO_BENCH=20`) measured **1.3–5.5ms, mean ≈4.2ms** — matching "Summon
latency" below's existing ~2.9–6.4ms/mean≈4.1ms figure inside measurement
noise, confirming no regression. Daemon-side RSS (the harness standing in
for `neko-daemon`, same `server` module) held at ~13MB idle — this task
touched no daemon-side code at all, so no change was expected or found.
Window-scoped screenshots (`screencapture -l<windowid>`, per the standing
rule below): `docs/evidence/actions-menu-clipboard-mode-edge-clamp.png`
(the menu open inside clipboard mode, its right edge clamped inside the
window by `snap_to_window_with_margin`) and
`docs/evidence/actions-menu-root-list-open.png` (the menu open on a
root-list row, anchored above the trigger with room to spare). **One
environment note worth recording for the next agent doing evidence capture
on this machine**: `screencapture -l<windowid>` can fail with "could not
create image from window" for a reason unrelated to TCC permissions or the
window itself — `system_profiler SPDisplaysDataType` showing `Display
Asleep: Yes` (this machine's own `displaysleep` idle timer, since no human
is physically at the keyboard during an unattended agent run) makes
`ScreenCaptureKit` unable to start a capture stream at all (confirmed via
`log stream --predicate 'process == "screencapture"'`: "Failed to start
stream due to audio/video capture failure"). `caffeinate -u -t <seconds>`
before launching the client (not just before capturing — a window's
content painted while the display was asleep can stay stale in the
compositor even after the display wakes, until some new state change
triggers a fresh paint) is the standing fix, and is itself non-invasive
(no synthetic input, just a display wake assertion).

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

## Third-party UI code: evaluated, then narrowly vendored

`longbridge/gpui-component` (Apache-2.0, crates.io) was evaluated as a
dependency for its virtualized list, text input, and keyboard/focus handling,
and rejected as a dependency (757 resolved packages for three mechanics, a
second `cocoa`/`cocoa-foundation` Obj-C bridging stack alongside the `objc2`
one this repo already uses, and 60+ components' worth of styling opinions to
fight against the frozen bespoke tokens). One genuinely self-contained,
non-trivial piece — cursor-blink timing — was vendored instead, adapted, and
attributed: `crates/neko/src/components/vendor/gpui_component/blink_cursor.rs`.
Everything else evaluated (the virtualized list, the rope-based rich-text
input engine) was declined with reasons recorded in
`crates/neko/src/components/vendor/MANIFEST.md` — read that file before
reaching for either of those again; the reasoning (list virtualization solves
a problem this app's server-capped result list doesn't have; the input engine
is a full multi-line/LSP-integrated editor, an order of magnitude more than a
single-line search field needs) is still valid unless those constraints
change. Apache-2.0 attribution: `NOTICE`, `THIRD_PARTY_LICENSES/gpui-component-APACHE-2.0.txt`.

## Design tokens

**Superseded in shape, not in content, by "Themes" immediately below**:
`theme.rs`'s colour tokens are no longer `pub const`s — they are fields on a
swappable `Palette`, read through `theme::active()`. Everything this section
says about *which* colours the shipped default has, and why, is still exactly
right: the `neutral` theme is byte-for-byte the palette described here.
Geometry/spacing/type constants in that file are untouched and still `const`.

`crates/neko/src/theme.rs` holds the token table, cross-checked in a test
(`theme::tests::base_palette_matches_the_frozen_oklch_table`)
against an independently-implemented OKLCH→sRGB conversion. **The palette
has been re-toned twice now, both on direct captain instruction, and is
currently true neutral — chroma 0 on every chrome token.** First from
`data/neko-design/report.md` §1's original warm ramp (hue 65°–75°) to a
monochrome-with-a-hint-of-blue ramp (hue 252°–257°, chroma 0.012–0.025):
offered three cat-derived identity directions (amber eye, jade eye, copper
coat), the captain picked none of them — *"lets do monochrome with hint of
blue."* Then, on a second instruction (`fm/neko-mode-visual`) reversing that
one — *"let's remove the blue tint altogether... let it be just naturally
there"* — every chrome token's chroma was taken to exactly `0.0`, with `L`
held bit-for-bit unchanged from the blue ramp (a pure hue/chroma change, not
a re-tone: contrast and hierarchy carry over, recomputed and confirmed in
`docs/evidence/mode-view-and-neutral-palette-report.md`). **`STATE_SUCCESS`/
`STATE_DANGER` and everything derived from them
(`STATE_SUCCESS_BORDER`/`STATE_DANGER_BORDER`/`BANNER_DANGER_BG`) are
untouched by either re-tone** — state colours, not chrome, per this file's
own "chrome is monochrome; state is coloured" rule. There is **no accent
token in this file any more** (`ACCENT` was removed during the first
re-tone — it was unused by any paint path, and its only reason to exist, an
unmade identity-accent pick, no longer applies).
`docs/evidence/palette-retone-report.md` (warm→blue) and
`docs/evidence/mode-view-and-neutral-palette-report.md` (blue→neutral) have
the full before/after OKLCH/sRGB/contrast tables — read the latter before
touching palette values again, since it's the current state.

**Row/icon rendering drifted from `design.css` once real data (147 Spotlight
apps, mixed-padding icon assets) exercised it** — `docs/evidence/panel-craft-pass.md`
has the full before/after value table (icon corner radius and backing-plate
token, row/section/input-row/footer padding, footer hairline tokens). The
durable lesson: `theme::ROW_ICON_SOCKET_BG` (`TEXT_PRIMARY`'s hex at 6%
alpha) is the row-icon slot's backing plate for *every* icon, cached or not
— real macOS icon assets bake in wildly different amounts of transparent
padding per app, and a consistent tinted socket behind all of them is what
makes a list of them read as one system instead of "mismatched
brightness/shapes." Any future icon-slot work should keep painting this
background rather than reverting to a bare `img()`. `panel.rs` and
`onboarding/view.rs` should have zero raw `rgba(0x......)` colour literals —
if you find one, it's a fresh instance of the same drift the palette re-tone
task cleaned up (`docs/evidence/palette-retone-report.md` §4); move it into
`theme.rs` as a named token rather than leaving it inline.

## Themes

`fm/neko-themes`, from the captain's own *"can we suport like themes section
where people can select?"*. **Read `crates/neko/src/theme.rs`'s module doc
comment before touching anything colour-related** — it is the normative
statement of the contract below. `docs/evidence/themes-report.md` has the
licence audit, the full per-theme contrast table, the twenty AA corrections
with before/after numbers, all seventeen window-scoped screenshots, and the
A/B measurements.

### The token-table contract

Two kinds of token, behaving differently on purpose:

- **Geometry, spacing and type stay `pub const`.** A theme is colour and
  surface only — it cannot move a row, change a radius, or resize the panel,
  and nothing reads those through a theme.
- **Colour lives on `theme::Palette`**, one struct of `PALETTE_TOKEN_COUNT`
  (22) `Rgba` fields. Every paint site reads the active one:
  `theme::TEXT_PRIMARY` became `theme::active().text_primary`, unchanged
  otherwise. **There is no `if themed` branch anywhere**, and adding one would
  be the wrong fix for anything.

**A theme supplies values; it never redefines what a token means.** Enforced
structurally, not by convention: a built-in is authored as a private `Spec` of
*independent* colours, and `theme::build()` — one `const fn`, one place —
derives every dependent token (`surface_panel_translucent` from
`surface_panel`; `menu_glass_tint` from `surface_raised`;
`text_tertiary_on_selected` *is* `text_secondary`, the promotion rule;
`border_hairline_strong` is the hairline hue at exactly double alpha;
`banner_danger_bg`/`state_*_border` from the state colours;
`row_icon_socket_bg` from one socket hue). A theme that wanted the strong
hairline *weaker* than the plain one cannot express it. `theme::tests::
every_theme_derives_its_relationship_tokens_from_its_base_colours` is what
makes an edit to `build()` itself fail loudly.

**Reading the active theme costs nothing per frame**: `theme::active()` is one
`Ordering::Relaxed` `AtomicUsize` load plus a slice index into
`&'static [Theme]` — no allocation, no lock, no `Arc` clone. Relaxed is
sufficient because the data it indexes is immutable rodata, so no reader can
observe a half-published theme however the load and store are reordered; a
`Mutex` would put a lock on the render path and an `Arc<Palette>` swap would
put an atomic refcount increment there, both for no correctness gain. **Do not
"harden" this to `SeqCst` or a lock** without a concrete reason the doc comment
does not already answer.

### The seventeen built-ins, and the licence rule they live under

`neutral` (the default — byte-identical to the pre-`fm/neko-themes` palette, so
nobody's app changes appearance on upgrade), `ember` and `catnap` (this
project's own, from `data/neko-cozy-theme/mockups/`), plus Catppuccin ×4,
Gruvbox ×2, Solarized ×2, Nord, Tokyo Night (Storm), Rosé Pine ×2, Dracula,
Everforest Dark.

**Every vendored palette is MIT, verified from its own source on 2026-08-21 —
not assumed, and not taken from a badge.** The tree stays GPL-free. Full table
with links and *how each was checked* is in `docs/evidence/themes-report.md` §2;
the two entries worth knowing without opening it:

- **Gruvbox has no `LICENSE` file at all.** It declares MIT in its `README.md`
  ("License — MIT/X11") and in `package.json` (`"license": "MIT"`). Recorded
  that way rather than implying a licence file that does not exist.
- **Tokyo Night is vendored from the original
  `tokyo-night/tokyo-night-vscode-theme` (MIT), deliberately not from
  `folke/tokyonight.nvim`** — that popular port is Apache-2.0, fine but it
  would have been the only non-MIT entry for no gain.

Adding a theme means: one `Spec` in `theme.rs`'s `THEMES`, one `BuiltinTheme`
in `neko_protocol::BUILTIN_THEMES`, its licence row in the report, and its hex
pinned in `vendored_themes_match_their_pinned_upstream_and_shipped_hex`.
`theme::tests::themes_match_the_protocol_registry` fails if you do one side and
not the other.

### Contrast is a gate, not a report

`theme::tests::themes_all_pass_wcag_aa` fails the build if any of eleven
text-on-surface pairs in any built-in drops below **4.5:1**. All seventeen
pass all eleven today. Two pairs are deliberately outside the gate and both are
accounted for in the report: `text_tertiary` on `surface_selected` (fails in
*every* palette checked including the neutral one, 3.04:1 — solved by the
promotion rule, whose promoted pair *is* gated), and anything composited over a
live desktop through the translucent fill (wallpaper-dependent; measured
separately in `data/neko-native-material/report.md` §6).

`every_theme_has_a_visible_selection_step_away_from_its_panel` (≥1.30:1) exists
because contrast tuning otherwise "fixes" a failing text pair by walking
`surface_selected` back into `surface_panel` — an earlier pass of this task
produced exactly that (`rose-pine` at `#272532` on a `#191724` panel: AA text on
an invisible selection row). **The tuning policy, if you ever re-tune: move the
text, not the surface** — neko's own neutral palette already solves its one
failing pair that way — and only move a surface when the text has run out of
headroom toward white/black.

**20 of 187 vendored token values are lightness-corrected** (hue and chroma
untouched) because a terminal scheme is designed against a terminal's pairs, not
this app's. The pin test asserts *both* the upstream and the shipped hex and
that the count is exactly 20, so a future edit that quietly walks another value
away from upstream fails.

### Light themes: two things a token swap cannot do

- **The icon plate inverts.** `row_icon_socket_bg` was `text_primary` at 6% — a
  pale film, invisible on a pale surface (`data/neko-cozy-theme/report.md`
  called this out precisely). The *rule* is unchanged ("a low-alpha plate of the
  opposite polarity to the panel"); a light `Spec` supplies a dark socket hue.
  `every_theme_has_an_icon_plate_that_reads_against_its_own_panel` checks
  relative luminance rather than pinning a hex.
- **`NSAppearance` has to follow the theme.** The panel is translucent over a
  real `NSGlassEffectView`, and that view renders in the *window's* appearance —
  a cream panel over a `darkAqua` blur reads as a dark halo leaking through
  wherever the fill is thin. `material::set_window_appearance` sets it on the
  real `NSWindow` (so it inherits to whichever material installed *and* to the
  `⌘K` menu overlay); `material::window_appearance_name` reads it back, logged
  at every launch beside the existing material/shadow/style-mask readbacks.

`panel::Root::sync_window_appearance` reconciles it **once per frame in
`render`, one enum compare**, with the AppKit call only on a real change —
deliberately not at each of the four theme-change call sites (preview, commit,
`ThemeChanged` broadcast, startup read). The setter is *injected*
(`panel::AppearanceSetter`, the same shape `accessibility` already uses) for two
reasons, the second not optional: it makes the behaviour headlessly testable,
and **GPUI's test-platform window `unimplemented!()`s (panics, rather than
returning `Err`) on `window_handle()`**, so any unconditional native call from
`render` takes every panel test down with it. Every other native call in this
crate lives in `main.rs`, which tests never run; this is the first one on the
render path.

**Every built-in keeps translucency** (panel alpha 0.82–0.90) —
`every_theme_keeps_the_native_material_visible` asserts it. `Theme::
keeps_translucency` is the seam for an opaque theme; nothing uses it.

### The `Themes` command and mode

The second command/mode pair, and the proof `modes.rs`'s own "what a second
command has to implement" accounting was honest: it cost one `CommandSpec`
(`neko_core::commands`), one `ModeChrome` (`crate::modes`), and one `Provider`
(`neko_core::themes::ThemesProvider`, id `"theme"`). **Nothing in
`enter_mode`/`exit_mode`/`run_search`'s scoping branch or the wire protocol
changed to accommodate it.**

- **Live preview is the feature.** Arrowing, or typing to filter, applies the
  palette to the whole panel as the selection lands (`Root::
  preview_selected_theme`). `ActiveMode::restore_theme` holds what to put back;
  Escape restores it, Enter clears it and persists. `restore_theme` is `None`
  for a mode that does not preview themes — deliberately not "every mode
  records and restores it, which is a harmless no-op", because that turns
  leaving *any* mode into a global palette write.
- **Entering lands on the theme already in use**, not on whatever sorts first,
  so opening the list never repaints the app on its own. Gated on "the
  previously-highlighted row was not itself a theme", which is exactly the
  entering case (`enter_mode` does not clear `results`).
- **`has_detail: false`**, a real choice: the preview *is* the panel, so a
  second column would take 496px away from the thing being previewed. A mode
  with no detail pane now renders its list full-width with full rows
  (`render_mode_list(has_detail)`); the clipboard mode's 264px column is
  unchanged.
- **Each row's swatch previews its own palette** — `glyph_element` takes the
  row's `SearchItem::id` and resolves `Glyph::Palette` through
  `theme::theme_by_id`, falling back to the live palette for a row whose id
  names no theme (the `Themes` command itself). A lookup keyed on data already
  on the row, not a `match` on which provider produced it.
- **Persistence** rides the existing `settings` KV table
  (`neko_core::themes::{get_theme,set_theme}`), read at client startup via
  `Request::GetTheme` and fanned out by `Event::ThemeChanged`. **There is no
  `SetTheme` request** — `Request::Activate { kind: "theme", id }` already means
  "do this row's thing", which for a theme row is "remember it".
  A persisted id naming no built-in reads as the default rather than erroring;
  `theme::set_active` likewise returns `false` and leaves the live palette
  alone, so a corrupted cosmetic setting degrades to "keep what's on screen".

### Evidence hook, and the race it exposed

`NEKO_SHOW_THEME=<id>` (`evidence.rs`) renders a capture in a specific built-in
by calling the same `theme::set_active` live preview calls — the real paint
path, not a capture-only shortcut. Driving the real mode per screenshot would
need synthetic Down-arrows, which this repo forbids. Focus-neutral: it touches
no window state.

**`main.rs`'s startup `Request::GetTheme` reply yields when that hook is set.**
Not defensive coding — caught live: two of the first seventeen screenshots came
back in the default palette because the daemon's reply landed hundreds of
milliseconds after the hook had applied its theme, and won.

**Verifying a themed screenshot actually rendered its theme**: sample the
*selected row* (painted at full alpha, so its pixels are the token value) and
classify against every built-in's `surface_selected`. Do not sample the panel
background — the translucent fill composited over the material's own tint sits
4–16 units off the nominal hex, which is enough to misclassify. One genuine
near-collision exists (Dracula `#44475a` vs Catppuccin Mocha `#45475a`); the
`surface_panel` sample separates those two unambiguously.

### Anything that calls `theme::set_active` in a test must hold `theme::test_lock()`

`ACTIVE` is process-global. **One lock for the whole crate**, not one per
module: `cargo test` will happily run a `theme.rs` test and a `panel.rs`
theme-mode test at the same instant, and two separate mutexes do not stop them
stepping on each other's palette (confirmed the hard way — this task shipped two
mutexes first and chased the resulting flake). Same discipline
`text_field::tests::pasteboard_test_lock` established for the systemwide
`NSPasteboard`.

## Standing safety rule: an evidence window must never become the key window

Binding on every future task in this repo, alongside the no-synthetic-input
rule and the window-scoped-capture-only rule (see "Window material" below).
Full incident account, the per-hook audit table, and the live readbacks:
`docs/evidence/evidence-key-window-safety-report.md`.

**The incident.** `evidence.rs::show_once` used to show its window with
`window.activate_window()` + `cx.activate(true)`, making a throwaway
evidence panel the system's real **key** window. On a machine the captain is
actively working on, his next keystrokes then land in that panel's search
field instead of wherever he is typing. Not hypothetical: a design-review
worker captured a screenshot and found the words `fix it` already in the
query field — the captain's own typing, on a run with no query hook set.
That capture was deleted immediately, nothing was persisted, and the worker
reported it rather than quietly continuing.

**Why the pre-existing mitigation didn't help.** This file already
prescribed committing an obscure hotkey to an isolated instance (see
"Evidence-capture hook: `NEKO_SHOW_CONFIRM`"), after an earlier incident
where a real `⌥Space` press landed on an evidence client. That was in force
here and did nothing, because the hazard is **key-window focus**, not
hotkey collision. This repo's standing rules covered synthetic input going
*out* of the app and screen content going *out* of the machine; nothing
covered the app **taking real input**, which is the same hazard from the
other side.

**The rule, and how it's enforced structurally rather than by memory.**

- **Every hook in `evidence.rs` is non-activating by default.** The window
  is shown with `material::order_front_regardless`, which "structurally
  cannot reach `windowDidBecomeKey:` at all" — it paints normally and is
  fully `screencapture -l<windowID>`-able while never taking keyboard
  focus. Forgetting a flag must fail *safe*; an opt-in safe path is exactly
  the mitigation shape that already failed once here.
- **Activation is one explicit opt-in, `NEKO_EVIDENCE_ACTIVATE=1`**
  (`evidence::activation_opt_in`), and it warns loudly on stderr when set.
  Only two hooks consult it. **`NEKO_BENCH_REAL` is the one legitimate
  exception** — measuring the real summon path *is* measuring
  `activate_window()`, and the non-activating stand-in already exists and
  is `NEKO_BENCH` — so it keeps its activation but **refuses to start**
  without the opt-in rather than being reachable by typing one env var
  that doesn't say what it does. Do not run it on a machine someone is
  using.
- **Focus state is read back, not asserted.** `material::is_key_window`
  (`-[NSWindow isKeyWindow]`, same "verified, not trusted" pattern as
  `verify_installed`/`verify_shadow_disabled`) drives a `neko: key window
  <bool>` line at every point that matters, so a capture is
  self-evidencing about focus. A run that ends up key without opting in
  prints a `SAFETY WARNING`.
- **An evidence run never registers a live OS hotkey.** `main.rs` skips
  `apply_initial` whenever `evidence::evidence_run_active()` — closing the
  other half of the same "an evidence process must not intercept real
  input" hazard structurally, instead of relying on each agent remembering
  to commit an obscure combo to the isolated daemon first.
- `window.focus(&root.focus_handle(cx), cx)` stays on both paths and is
  safe: it's GPUI-internal focus only (the caret, this process's own
  action routing) and cannot pull real OS keystrokes into a window that
  isn't key.

**If you add a hook to `evidence.rs`, add its row to that file's own
per-hook focus table** (can it take key focus, does it, why) — the table is
the audit, and an unlisted hook is an unaudited one.

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
hue and can never decouple the two weights. `PALETTE_TOKEN_COUNT` is 22.
The guide is drawn with a `surface_panel` hairline outline so it reads over
a wallpaper of either polarity. `SNAP_GUIDE_THICKNESS_PX` is geometry and
stays `const`.

**Evidence hook: `NEKO_SHOW_DRAG_GUIDES=1`** (with `NEKO_SHOW_ON_LAUNCH`)
opens the overlay for a snap the panel is not actually being dragged into —
the real `snap::resolve` and the real overlay, with only the cursor's
contribution replaced — and prints every input and output beside the window
number, so a capture is checked against the numbers that produced it rather
than eyeballed. Focus-neutral; the panel's own window state is untouched.

## Window placement, disconnection, and Spaces

Three fixes from an exhaustive audit (`data/neko-audit/report.md`, Part
4 items 8–10), full evidence and methodology in
`docs/evidence/window-behavior-report.md`.

**Multi-display: opens on the display under the cursor, not always
primary.** `crates/neko/src/display_placement.rs`. The window opens once at
process start and is only ever hidden/shown after that (see "Onboarding"
above, "resident window"), so `main.rs`'s `upper_third()` — which only runs
at that one initial `open_window` call — can never place a *later* summon
correctly; every call after the first needs its own repositioning.
`reposition_to_cursor_display` (called from both real summon entry points —
the hotkey-press branch and `App::on_reopen` — right before
`window.activate_window()`) reads `NSEvent.mouseLocation()` (a synchronous,
permission-free class method — chosen over "the active window's owning
display" specifically because that needs `AXUIElement`, the same
Accessibility permission the hotkey itself is gated on, and a cross-process
query at that), picks the `NSScreen` containing it, and moves the real
`NSWindow` there via `setFrameTopLeftPoint:` — the real, public AppKit API,
reached the same `raw-window-handle` way `material.rs` reaches the content
view, since GPUI 0.2.2 has no public way to move an already-open window
(`Window::resize` exists; no `set_bounds`/`set_origin` — same gap as the
title-bar-drag limitation `AGENTS.md`'s window-material section already
documents). `upper_third_offset` — the exact "horizontally centered,
`height/3 - panel_height/4` from the top" formula — is factored out of
`main.rs`'s own `upper_third()` into this module so the initial-open
placement and every later re-placement can never drift apart. **Icons stay
sharp across a display move for free**: gpui's own mac backend wires
`windowDidMove:`/`windowDidChangeScreen:` into a `scale_factor` refresh
(`gpui-0.2.2/src/window.rs::bounds_changed`), the same live rescale the
128px icon cache (`v3-128px`, see "Icons" above) already depends on for a
single display — nothing app-side has to special-case a display's different
backing scale. Real two-display evidence wasn't obtainable in the sandbox
this task ran in (one physical display) — see the evidence report for
exactly what was and wasn't verified live.

**Daemon disconnection: a real, live "can't reach neko-daemon" banner,
not silence.** `run_search`'s response handling (`panel.rs`) used to be
`let Ok(Response::SearchResults{items}) = response else { return; };` —
every error, including a dead connection, was a silent no-op; results just
stayed exactly as they were, forever, with zero indication. Fixed with one
new client-visible signal: `NekoClient::is_connected()`
(`neko-client/src/lib.rs`) — a plain `AtomicBool` the reconnect supervisor
flips on both transitions (dial-in succeeds / connection drops), polled
once per tick by `main.rs`'s summon loop (it already polls `Event`s on a
fixed 20ms interval — the smaller addition over a new push channel or
folding this into `neko_protocol::Event`, which would incorrectly imply the
*daemon* originates a concept that's actually pure client-local knowledge).
`panel::Root::set_connected` pushes the transition into a `connected: bool`
field; `render_connection_banner` shows the same strip treatment
`render_accessibility_banner` already established (frozen design, no new
chrome) — "Can't reach neko-daemon. Results may be out of date." — with no
dismiss control, since it clears itself the instant the supervisor
reconnects, live, no keystroke needed to notice either the failure or the
recovery. `run_search`'s own request error is still deliberately not
surfaced there — connection state is owned entirely by the poll, results
state by `run_search`; that separation is what makes "results stay stale,
not wiped" and "banner shows immediately, even fully idle" both true at
once.

**Spaces / full-screen: already correct, now proven, not inferred.**
`data/neko-audit/report.md` explicitly declined to test this live (it would
have needed a system-wide Space-switch keystroke on a machine also running
the captain's real work) and only grepped — zero hits for
`NSWindowCollectionBehavior` anywhere in the crate, flagged as plausibly
broken. It wasn't: `main.rs` opens the summon window with `kind:
WindowKind::PopUp`, and `gpui = "0.2.2"`'s own mac backend
(`MacWindow::open`, the `WindowKind::PopUp` branch) unconditionally sets
`NSWindowCollectionBehaviorCanJoinAllSpaces |
NSWindowCollectionBehaviorFullScreenAuxiliary` on any window of that kind —
confirmed by reading gpui's own source, not assumed. `crates/neko/src/
spaces.rs` adds the live readback the audit couldn't take: same
raw-window-handle technique `material.rs`'s `verify_installed` already
uses, called unconditionally from `main.rs` right after the material
readback, logging the real bits back off the live `NSWindow` on every
launch — a permanent runtime sanity check, not a one-off proof, matching
the precedent `material.rs` already set for "don't just trust the setter
call took effect." No behavior changed here; only the missing verification
was added.

## v1 simplification: fixed-size window, not dynamic per-keystroke resize

`gpui::Window::resize` exists, but this task had no safe way to verify its
resize-anchor behavior on a borderless always-on-top popup window before
committing to it on the hotkey-latency-critical path. `neko::panel::Root`
instead renders at one fixed size (680×448) with the footer pinned to the true
bottom via a flex-grow content area, so short result lists leave quiet space
above the footer rather than the window growing or shrinking. Documented in
`panel.rs`'s own module doc comment. Real dynamic resizing is a follow-up, not
attempted here specifically because it was untested territory on the one path
this task's acceptance criteria measures a hard number against. The
*height* (448) is still exactly this: fixed, never resized at runtime. The
*width* no longer varies at the real `NSWindow` level at all either, since
"Mode view resize seam" below — the window is a fixed
`PANEL_WIDTH_WITH_DETAIL_PX`, and only the panel `div`'s own centering
changes for a mode transition.

**A fixed content area with two possible section headers (apps, clipboard)
means the row count that fits isn't always `RESULT_LIMIT` (8) — a header eats
real space nothing budgeted for before clipboard existed.** Rendering
whatever the daemon returned and relying on `overflow_hidden()` to clip the
excess produced a real defect: the last row would render half-visible,
jammed against the footer, instead of being fully shown or fully absent.
`panel::fit_within_budget` (called on every search response, before results
are stored in `Root`, so a truncated-but-out-of-sync `selected` index can
never point at a row that isn't rendered) fixes that — but an earlier
version fixed it by dropping the *entire* clipboard section whenever apps
alone filled the budget, which just moved the "Search and ranking" crowd-out
bug into the client instead of removing it (six or more app matches made
clipboard invisible again, silently). The corrected version reserves one
section header plus one row for clipboard *before* deciding how many apps
fit — `fit_section` spends whatever budget is actually left, in order, so
apps only give up rows they don't strictly need, and clipboard always gets
at least its reservation plus any unused app budget. Still never a partial
row, never a header with zero rows under it. Covered by `panel.rs`'s own
tests, including the exact 6-apps-plus-1-clipboard-row shape that exposed
both bugs in turn, and a 20-apps case proving the reservation holds well
past the original repro's app count. **Generalized from exactly two
sections to however many providers a response actually contains** by the
provider-abstraction task — see "Provider abstraction" above, "Cross-
provider ranking" subsection, for the current N-section algorithm; the
reservation-and-rollover behavior described above is still exactly what it
does for the 2-section case.

## Summon latency

Re-measured after the restructure (release binary, `RUST_LOG` off — full
methodology and all samples in `docs/evidence/summon-latency.md`):

- **Warm** (window, renderer, and daemon connection already live): **~2.9–6.4ms**
  across 6 samples, mean ≈4.1ms — inside slice 1's own budget (~11–14ms) and
  faster than slice 1, despite the panel now rendering far more per frame.
  The daemon round-trip for `Search` is off the summon path entirely (summon
  is hotkey-press → first-frame-after-activation, purely client-side,
  unchanged from slice 1) — a slow or unreachable daemon cannot make summon
  itself slower.
- **Cold** (first summon after process launch): **~65–160ms** across 2
  samples — roughly double slice 1's ~89ms. This is the one place the extra
  panel content and `WindowBackgroundAppearance::Blurred` compositing
  genuinely cost something, paid once per process lifetime (the process stays
  resident — see "Current scope" above), not per summon. Reported plainly
  rather than absorbed, per the brief's own instruction.

Testing caveat carried over from slice 1: synthetic hotkey events via
`osascript … key code 49 using {option down}` are unreliable in rapid
succession (roughly half dropped between AppleScript and the OS's global-hotkey
delivery), even though every event that *did* arrive was handled correctly.
Real hardware keypresses weren't re-tested with the same instrumentation this
pass either — don't be surprised if automated re-tests need retries.

**Regressed on the `wingleeio/zed` fork (`fm/neko-gpui-fork-migration`,
2026-08-20) to ~26–40ms warm — root-caused and fixed
(`fm/neko-fork-summon-latency`).** Full diagnosis, the live instrumented
trace that proves it, and the fix's own trade-offs:
`docs/evidence/gpui-inactive-window-throttle-fix-report.md`. Summary:

**Cause**: `crates/gpui/src/window.rs`'s `on_request_frame` handler caps
frame delivery to ~30fps (a `Duration::from_micros(33333)` minimum
interval) whenever a window isn't key (`!active.get()`) — with no
exemption for a pending `on_next_frame`/`request_animation_frame`
callback, i.e. something a caller is *explicitly* waiting on, not an idle
background redraw. **This throttle does not exist at all in the published
`gpui = "0.2.2"` crate** — confirmed by direct diff, not inferred; its
equivalent handler runs `next_frame_callbacks` unconditionally on every
request. `NEKO_BENCH` shows the window via a non-activating
`orderFrontRegardless` by design (so a long bench run doesn't repeatedly
steal focus), so it is always "inactive" from this check's point of view —
every summon's frame request fell into the 30fps cap, and because a
throttled request never advances the handler's own `last_frame_time`,
every following request landed in the same stale window and got throttled
again, locking summon into a ~25–40ms cadence with no way out on its own.
The real `activate_window`/`cx.activate` path races the same check against
the async `windowDidBecomeKey:` notification and shows the identical
pattern, decreasing across repeated activations
(`gpui-fork-migration-report.md`'s own `NEKO_BENCH_REAL` numbers: 61.7ms,
32.0ms, 11.1ms for 3 cycles) — not contradicting this diagnosis, consistent
with it.

**Fix**: a small local patch on top of the pinned fork rev —
`patches/gpui-0001-exempt-pending-frame-callbacks-from-inactive-window-throttle.patch`,
applied by `scripts/setup-gpui-patch.sh` (run once, or again after the
pinned rev changes; populates `~/Library/Caches/neko-dev/gpui-fork-patched`,
which the workspace `Cargo.toml`'s new
`[patch."https://github.com/wingleeio/zed"]` section points `gpui`/
`gpui_platform` at) — exempts a frame request carrying a pending callback
from the inactive-window cap specifically, leaving thermal throttling
untouched. No public gpui API exists to opt out of this per-window or
per-request (every native macOS call site that invokes the frame callback
passes `RequestFrameOptions::default()`, always), which is why this needed
a dependency patch rather than a neko-side fix — see the evidence report's
§4 for the trade-off this patch accepts (a future *repeating* animation in
an inactive window would also stop being power-throttled; neko's own
`motion.rs` catalog is exclusively one-shot today, so this doesn't regress
anything currently in the app).

**Verified**: `cargo build`/`cargo test` (242 tests)/`cargo clippy
--all-targets` all clean against the patched dependency, both before and
after rebasing onto `main`; the deadlock fix, native window-drag support,
and the `paint_backdrop_blur`/`EdgeFade` primitives the fork migration was
taken for are all untouched (the patch's only functional change is inside
`on_request_frame`'s throttle branch). The shared machine's screen locked
partway through this task (confirmed via `CGSessionScreenIsLocked`, not
just display-idle-sleep, which `caffeinate` can't clear) and stayed locked
long enough that the diagnosis and fix were written up and committed before
it cleared; once it did, one **release-binary** `NEKO_BENCH=15` run
recorded the actual before/after: **before, ~26–40ms across 15 samples, no
cold/warm split** (established by the two prior tasks, re-confirmed by this
task's own 25.98–38.58ms debug-build trace); **after, warm summons (1–14)
mean 8.56ms, range 1.20–15.60ms, 7/14 under 10ms** — the artificial ~33ms
floor is gone, real variance now bounded by this window's own ~8ms
`CVDisplayLink` tick cadence (not investigated further — see the evidence
report's own closing note) rather than by the fixed regression. Summon 0
(cold, 38.91ms) is this project's own pre-existing, unrelated cold-start
cost, unaffected by this patch. Full numbers and methodology:
`docs/evidence/gpui-inactive-window-throttle-fix-report.md`.

## Hotkey scoping (must never leak into other apps)

`global-hotkey`'s macOS backend registers through Carbon's
`RegisterEventHotKey`, which is an OS-level *consuming* registration — a
registered combo's keystroke is delivered to the registering process's
callback and does not also reach the focused application. This was already
true in slice 1; nothing in this task's daemon/client split or the
configurable-hotkey addition changes which API performs the registration.
Manually verified this pass: typed `BASELINE-` into a focused TextEdit
document, pressed neko's live hotkey (which correctly summoned the panel),
typed `-AFTER` back into TextEdit — result `BASELINE—AFTER` with nothing
(not even a space) inserted between the two typed strings. Screenshot:
`docs/evidence/hotkey-no-leak-textedit.png`.

## Licence rule

**Every line in this repo is written fresh**, with one documented exception:
see "Third-party UI code" above (`gpui-component`'s `blink_cursor.rs`,
Apache-2.0, attributed). **Vendored *assets* are a separate, audited
category** — nothing executes, nothing is copied into a `.rs` file, and the
upstream is pinned so a drift stays visible. Two things are in it:

- **Colour values.** Seventeen built-in themes draw on eight upstream
  projects, every one MIT, each verified from its own source; see "Themes"
  above and `docs/evidence/themes-report.md` §2 for the table and the
  per-palette verification method. No third-party palette *code* is
  vendored, only values re-expressed in `theme.rs`'s own `Spec` form.
- **Icon geometry.** Nine Lucide SVGs, **ISC** (not MIT — verified from
  Lucide's own `LICENSE`, 2026-08-23), vendored byte-for-byte and pinned to
  one upstream commit; four of them additionally carry Feather's MIT grant.
  See "Icons: real SVGs, and the one that stays painted" above,
  `THIRD_PARTY_LICENSES/lucide-ISC.txt`, and the icon table in
  `crates/neko/src/components/vendor/MANIFEST.md`. gpui-component's own
  `icon.rs` was read and declined; no icon *code* is vendored. `refs/` (this repo's
own read-only reference clones, gitignored — see `refs/README.md` for the
licence table and the standing rule) holds five reference GPUI apps — `comet`
(MIT), `loungy` (MIT), `t3code` (MIT), `waku` (GPL-3.0), `codux` (GPL-3.0) —
plus GPUI's own bundled `examples/` (Apache-2.0). All read for
architecture and API shape, never copied. The aim is MIT end to end, plus the
one attributed Apache-2.0 file — true of every line of *this repo's own
source*, still. **It does not describe a built, distributed binary as of
2026-08-20** — see "The GPUI dependency decision" immediately below for why
a linked dependency can attach obligations this rule doesn't cover.

## The GPUI dependency decision

**Reversed 2026-08-20, by explicit captain decision — this section
previously chose the opposite route specifically to avoid the cost this
reversal accepts. Read this whole section, not just the summary below, if
you're touching this dependency again.**

**Current state**: `crates/neko` depends on the `wingleeio/zed` fork of
`gpui` (plus its own `gpui_platform`), pinned by git rev
`e2ddcc6805f8c5088e62a60dfe517abcccd61a9a` — the same rev
`refs/comet` pins, chosen specifically so the primitives this
migration was taken for are known to exist at that exact commit. Pinned by
rev, never by branch. Full migration record, build-cost numbers, and the
four-primitive availability confirmation:
`docs/evidence/gpui-fork-migration-report.md`.

**What was decided, in the captain's own words**: shown verified evidence
that this fork's `gpui` unconditionally links three GPL-3.0-or-later crates
(`gpui → sum_tree → ztracing`, plus `ztracing`'s own `zlog`/`ztracing_macro`
dependencies) with no Cargo feature that avoids it, and that this is not
fork-specific — vanilla Zed's own `gpui` carries the identical chain,
tracked upstream as `zed-industries/zed#55470`, still open, unfixed — the
captain's response was *"its okay lets do it, i dont care about licence."*
Full evidence and decision record (outside this repo, firstmate home):
`data/neko-gpui-fork-licence/report.md`,
`data/neko-gpui-fork-licence/decision-adopt-fork.md`.

**Why the original decision (below) chose the opposite route**: the
published `gpui = "0.2.2"` crate on crates.io is genuinely Apache-2.0 with
no GPL edge — `sum_tree`/`ztracing` aren't part of what crates.io publishes
under that version, only present via a git dependency on the fork/mainline.
Depending on the published crate was the whole point: get gpui without the
GPL chain. This reversal gives that up on purpose, for capabilities the
published crate doesn't have and won't get without waiting on an unknown
future release.

**What it buys, concretely** (see `docs/evidence/gpui-fork-migration-report.md`
§4 for citations): `window.paint_backdrop_blur`/`BackdropBlur` (a real
Metal backdrop-blur scene primitive — `fm/neko-frost`'s own follow-up to
build a native frosted backdrop with, not attempted by the migration task
itself); `gpui::EdgeFade`/`window.with_edge_fade` (a scoped edge-fade
primitive, likewise left to `fm/neko-frost`); `Window::start_window_move()`
gaining a real mac implementation (`NSWindow.performWindowDragWithEvent:`)
instead of gpui's cross-platform no-op default — closes the "Window chrome"
section's own documented header-drag gap, though nothing wires it up yet;
and, empirically confirmed by driving the real summon/dismiss path 3 times
without a hang (not just read in source), a fix for the
`windowDidBecomeKey:` self-deadlock documented below in "A known,
upstream-fixed-but-unreleased deadlock in the real summon path" —
`window_did_change_key_status` (`gpui_macos/src/window.rs`) now drops its
state lock before calling back into AppKit's `resignKeyWindow`.

**The licence consequence, stated plainly**: a **built, distributed**
`neko` binary is no longer shippable under a clean MIT grant — GPL-3.0
obligations (including source disclosure) attach to the combined work once
it's distributed. This repo's own source remains MIT (see "Licence rule"
below — nothing here was relicensed), and the workspace `Cargo.toml`'s
`license = "MIT"` field and `NOTICE` both now carry an explicit caveat
saying so, rather than silently describing something no longer true of a
release binary.

**The obligation is real but currently dormant — GPL-3.0 triggers on
distribution, not on use.** neko ships **local-only**: no remote, no
releases, built and run on the captain's own machine, for himself. While
that stays true there is nothing to disclose and no obligation to
discharge — today's practical exposure is essentially zero. It becomes live
the moment a `neko` binary is ever distributed to anyone else. **Before
that day, revisit this with counsel** — the open questions (whether linking
functionally-inert GPL code creates a derivative work at all; an
unreferenced Apache licence file sitting in `ztracing`'s own directory
next to its GPL manifest declaration; `-or-later` semantics) are legal
judgment calls, not settled facts, and weren't resolved by the decision
above — only accepted as a known, live risk. **`refs/comet`
shipping MIT-declared binaries is not precedent for this** — an MIT grant
on comet's own code doesn't discharge an obligation attaching to a
distributed *combined* binary, and comet's own maintainer also owns the
`wingleeio/zed` fork, so comet's licensing choice isn't an independent
validation of the arrangement being clean.

**Practical rule going forward**: fine to build, run, and develop against
locally. Do not distribute a `neko` release binary to anyone — including a
GitHub release, a download link, or handing a built binary to another
person — without first resolving the open legal questions above, with
counsel. This is a standing constraint on this repo now, not a one-time
warning.

### The original decision (2026, superseded above)

`gpui` is Apache-2.0 as published on crates.io, but git-`main` currently links
a GPL-3.0-or-later crate (`ztracing`) through an unfixed dependency edge — see
`data/dim-licence/report.md`. **Chosen route, unchanged from slice 1: depend on
the published crate, `gpui = "0.2.2"` from crates.io — not git, no `[patch]`
block.** Re-verified again after clipboard history added `objc2-core-foundation`
as a direct dependency (`Apache-2.0 OR MIT OR Zlib`, already transitively
present via `gpui`/`objc2-app-kit`) — `docs/evidence/cargo-tree.txt` and
`docs/evidence/cargo-license.txt` are current as of that change: still no
GPL/AGPL anywhere in the tree, still only `self_cell`'s dual
`Apache-2.0 OR GPL-2.0` line, used under the Apache-2.0 arm.

**This verification is stale as of the 2026-08-20 reversal above** — it
described the dependency tree before the fork switch and no longer
reflects what `cargo tree`/`cargo license` actually report. Kept here as
the historical record the original decision was based on, not as current
fact.

To re-verify (now expected to show the GPL-3.0 chain, not flag one):

```sh
cargo license 2>/dev/null | grep -iE '\bgpl\b|agpl'   # now expected to show ztracing/zlog/ztracing_macro (GPL-3.0-or-later)
cargo tree | grep -i 'ztracing\|zlog'                  # now expected to show the gpui -> sum_tree -> ztracing chain
```

## A known, upstream-fixed-but-unreleased deadlock in the real summon path

`neko-leak-confirm` set out to confirm a hypothesized ~4GB-per-summon memory
leak in gpui-0.2.2's `windowDidBecomeKey:` forced-draw path (see the
now-superseded framing in `data/neko-leak-audit/report.md`, firstmate home)
and instead found a **different, real bug in the identical code path**: five
live runs under an isolated `HOME`, driving the real `activate_window()` +
`cx.activate(true)` / `cx.hide()` cycle, showed **zero IOSurface/GPU-backed
growth** (the original leak hypothesis is not confirmed), but every run
reproducibly **self-deadlocked** the main thread after one or two real
activations — a live `sample` stack trace shows `window_did_change_key_status`
(`gpui-0.2.2/src/platform/mac/window.rs:1976`) re-entering itself via a
synchronous `resignKeyWindow` call made while still holding `window_state`'s
lock, blocking forever on the same non-reentrant mutex. This is not a novel
finding — it's an exact match (same call chain, same locking primitive) for
`zed-industries/zed#50151`, already root-caused and fixed by merged PR
`#51035`, but **that fix isn't in any published `gpui` crate** (crates.io's
newest is still 0.2.2, from six months before the fix merged) — consuming it
today would mean depending on git `zed-industries/zed`, reopening "The GPUI
dependency decision" above. No neko-side workaround was found (one
call-ordering hypothesis was tested live and disproven). Full account,
proven-vs-inferred table, and the captain's real options:
`docs/evidence/neko-leak-audit-confirmation.md`. The permanent artifact this
task left behind: `NEKO_BENCH_REAL` (`crates/neko/src/evidence.rs`), a bench
mode that drives the real summon/dismiss path (unlike `NEKO_BENCH`, which
uses `order_front_regardless`/`order_out` and — proven in this task, not
just asserted — structurally cannot reach `windowDidBecomeKey:` at all) —
use it to re-verify once a fixed `gpui` becomes consumable.

**Resolved 2026-08-20** — a fixed `gpui` did become consumable, via the
reversal in "The GPUI dependency decision" above, and `NEKO_BENCH_REAL` is
exactly what re-verified it: 3 real `activate_window()`/`cx.activate(true)`/
`cx.hide()` cycles on the `wingleeio/zed` fork's release binary completed
without a hang (`docs/evidence/gpui-fork-migration-report.md` §4) — the
fix's own source (`gpui_macos/src/window.rs`'s `window_did_change_key_status`
now drops its lock before calling `resignKeyWindow`) matches this section's
own account of what was missing. The originally-hypothesized ~4GB memory
growth remains unconfirmed either way — this task's own bench runs were
short (3 cycles) and didn't re-attempt that measurement.

## Seams for follow-up work

- **Onboarding UI**: built — see "Onboarding" above.
- **Clipboard history**: built — capture, storage, and restore all live in
  the daemon; see "Clipboard history" above. Onboarding's steps 06-07 own the
  *ask* (`clipboard_history_enabled` daemon setting). The two-column
  detail-pane mode (`PANEL_WIDTH_WITH_DETAIL_PX`, screen 12) is now also
  built — see "Commands and modes" below. Still open: image capture (seam
  documented in `clipboard.rs`'s module doc comment, and in "Commands and
  modes"'s own note on what an `Image` `SearchItem`/detail-pane variant
  would cost). No macOS permission prompt was observed gating general
  pasteboard reads on the verification machine (unlike Accessibility,
  there's no `AXIsProcessTrusted`-equivalent "is trusted" API for the
  pasteboard) — `read_current`'s SAFETY comment covers the "detect and
  degrade" reasoning for the day one shows up: a blocked read and an empty
  one both come back as `None`/`nil`, so treating `None` as "nothing to
  capture this tick" already is the honest degrade path, not a placeholder
  for a request flow still to build.
- **Commands and modes**: built — see "Commands and modes" above. One
  command exists (Clipboard History); a second command's own accounting
  (what it has to implement vs. what it gets for free) is in
  `crates/neko/src/modes.rs`'s module doc comment. Still open: the mockup's
  "All Types" filter control (no real filtering logic exists to back it —
  see "Commands and modes"), real window-scoped visual evidence and warm
  summon latency / daemon idle memory re-measurement (this task's client
  could not be launched — the captain was using this machine), and image
  entries in the clipboard-history detail pane (a real scope increase: blob
  storage, thumbnails, size bounds, a new `SearchItem`/`Icon` shape — a
  separate captain decision per the launch brief, not attempted here).
- **Dynamic window resize**: partially built — see "v1 simplification"
  below for the still-true per-keystroke case. A mode transition's own
  width change is real but, since "Mode view resize seam" above, is no
  longer a window resize at all — the real `NSWindow` is fixed at
  `PANEL_WIDTH_WITH_DETAIL_PX` for the process's whole lifetime; only the
  panel `div`'s own centering and the native backdrop's frame change.
- **`Provider` abstraction**: built — see "Provider abstraction" above.
  `agent.rs`'s old `AgentProvider` placeholder is deleted, superseded by
  this. Agent capability, whenever it's built, is `impl Provider` plus one
  line in `AppState::new` — no protocol or panel changes needed, per that
  section's "registering a fourth provider" accounting.
- **File search**: built as the provider abstraction's proof — see
  "Provider abstraction" above, "File search" subsection. Still open:
  configurable scope (currently fixed to `~/Documents`/`~/Desktop`/
  `~/Downloads`, no settings UI — the seam mirrors `clipboard_history_enabled`'s
  existing daemon-setting pattern) and real per-file icons (painted
  `Glyph::File`/`Glyph::Folder` for now — deliberately not a second
  icon-extraction path; wire through `icons::ensure_cached_icon` in a
  background pass once the parallel `neko-icon-cache` task's own fixes are
  in).
- **System Settings pane search**: built — see "Provider abstraction"
  above, "System Settings pane search" subsection. Still open: real
  per-pane icons (shares the System Settings app icon for now, same
  deliberate scope cut as file search's painted glyphs — Apple's private
  iconography stack, not `NSWorkspace.iconForFile`, is what would be
  needed) and multi-word queries ("keyboard shortcuts") — a `fuzzy_score`
  limitation shared by every provider, not specific to this one.
- **Native window material**: built — see "Window material" above. Still
  open: a real screenshot proving live compositing/legibility against a
  busy backdrop, blocked on this machine's standing capture-safety rule
  (same section) rather than on any code gap.
- **Menu frost backdrop and the results-list edge fade**: built — see "Menu
  frost backdrop and the results-list edge fade" above,
  `docs/evidence/menu-frost-and-edge-fade-report.md`. Still open, and the
  concrete next step now that `main` depends on a `gpui` fork that ships
  the real primitive: build the `window.paint_backdrop_blur` version of the
  menu frost (`crates/gpui/src/window.rs:3992`) and compare it head-to-head
  against this task's native-AppKit sibling-below material, keeping
  whichever reads closer to comet's own result; `gpui::EdgeFade`
  (`crates/gpui/src/window.rs:682`) could similarly replace `edge_fade.rs`'s
  hand-rolled gradient, lower-risk since neither side of that swap needs
  native bridging.
- **Two-phase search**: built — see "Two-phase search: results as you type"
  above. Still open: nothing on the latency itself, but two things worth
  knowing. A *second* deferred provider would work without any code change
  (`defers_for`/`search_cancellable` are per-provider and `allocate` is
  already provider-count-agnostic), but has never been exercised — the
  daemon would still emit exactly two frames, with both deferred providers
  in the second, rather than one frame each; splitting further is a real
  design choice nobody has had to make yet. And `merge_late_results`'s
  "decline the late section rather than displace the selection" rule
  currently drops those results until the next keystroke; a scrollable root
  list (see "v1 simplification" below — it is still budget-fit and
  non-scrolling) would remove the need for that trade entirely.
- **Themes**: built — see "Themes" above. Seventeen built-ins, a `Themes`
  command/mode with live preview, daemon-persisted. Still open: per-theme
  geometry (Sherbet from `data/neko-cozy-theme` wants radii 16/8/6 → 22/12/8,
  which the current contract forbids on purpose — a theme is colour and surface
  only), gradient palettes (Ember and Catnap ship as flat fills; their
  `--grad-from`/`--grad-to` tokens are unused, since a `linear_gradient` on the
  panel is a rendering change rather than a colour one), Nightlight's
  per-section hues, user-supplied palettes (would need a format, validation,
  and an answer for "what happens when a loaded theme fails contrast" —
  `themes_all_pass_wcag_aa` only gates compiled-in ones), and a window-scoped
  capture of *onboarding* in a non-default theme (it compiles and reads the
  same tokens, but was not re-screenshotted).
- **Preferences**: built — see "Preferences: neko's own settings, and the
  mode stack" above. Three settings (summon hotkey, launch at login, search
  folders). Still open: the clipboard-history toggle (deliberately left out
  of this pass though it is nearly free — one `RowSpec` over the existing
  `clipboard_history_enabled` key), a real folder picker (needs suppressing
  the click-outside dismissal while a native panel is open), a visual pass
  on the hotkey capture screen, and `SMAppService` once neko is packaged as
  a `.app`.
- **Real icons**: built — see "Icons: real SVGs, and the one that stays
  painted" above. `assets::NekoAssets` is now this app's `AssetSource`, so
  **any future asset — an icon, a bundled font, an image — has a home**;
  adding one is a row in `ICONS` and a constant, not a pipeline. Still open:
  real per-file and per-System-Settings-pane icons (both still deliberate
  scope cuts for the *daemon-side* reasons in "File search" and "System
  Settings pane search" above — extraction cost and Apple's private
  iconography stack, neither of which this change touches); a light-theme
  capture; a latency/memory measurement of the sprite-atlas cost (argued nil
  after first paint from reading `paint_svg`, never benchmarked); and
  `file`/`folder`/`link` never being seen on screen (`docs/evidence/
  icons-svg-report.md` §6). One thing worth knowing before reaching for
  `svg()` elsewhere: it is an **alpha mask**, so anything genuinely
  multi-colour (`Glyph::Palette`) has to stay painted.
- **Agents**: seeing them is built (see "Agents: what is running right now"),
  and **starting** them now is too (see "Starting an agent: the New Agent
  command"). Still open: reaching a directory Paseo has never seen (the mode
  offers registered projects only), choosing a different tool or model than the
  one last used there, live confirmation of the `PASEO_AGENT_ID`/
  `PASEO_WORKSPACE_ID` environment strip (it would cost a second real spawn),
  any on-screen evidence of the mode, and the ~1–3s the CLI's own Electron boot
  puts between Enter and the panel closing — a "starting…" tell, or Paseo's own
  local RPC port, would each remove it.
- **Dragging the panel**: built — see "Dragging the panel: snapping and
  highlighted guides" above. Still open, in order of how likely each is to
  matter: **nobody has performed a real drag** (synthetic input is forbidden
  here, so the live behaviour rests on a source-level argument plus
  `snap.rs`'s own tests); the cross-display path (close the overlay, reopen
  it sized to the new screen) has never run, this machine having one display;
  the muted guide weight has never been photographed, needing two targets
  within 16pt on one axis; the dragged position is deliberately **not**
  persisted, since `reposition_to_cursor_display` resets it on the next
  summon; and the ~27ms overlay open could be removed by keeping one window
  alive between drags, which is exactly what "torn down on mouse-up, every
  path" currently forbids.
- **A real menu-bar `NSStatusItem`**: see "Onboarding" above — GPUI 0.2.2 has
  no usable status-item API; this is raw AppKit bridging, its own task.
- **Text field selection and paste**: built — see "Text field editing
  shortcuts" above. Still open: mouse selection (click-drag) and IME
  composition (marked text) — both real, separate pieces of work this
  task's own launch brief scoped out (single-line, standard-shortcut-set
  only, no general text editor).
- **`CORE_SERVICES_ALLOWED_APPS`'s 5-name allowlist** (`apps.rs`, see
  "Application discovery" above) is pinned to this machine's OS build. A
  future macOS release could rename, remove, or add a loose bundle in
  `/System/Library/CoreServices` that a person would want launchable; no
  static signal was found this task could build a version-proof general
  rule on instead (the investigation, and why each candidate signal failed,
  is in that constant's own doc comment) — re-verify the list by hand after
  any major OS upgrade, the same way `docs/evidence/cargo-license.txt`/
  `cargo-tree.txt` get re-verified after a dependency bump.
- **Multi-display, disconnected-daemon, and Spaces window behavior**: built
  — see "Window placement, disconnection, and Spaces" above. Still open:
  real evidence on an actual two-display setup (this task's sandbox only had
  one physical display — the math and the live single-display execution
  path are both proven; the cross-display *visual* isn't).
- **The `windowDidBecomeKey:` deadlock**: fixed and empirically re-verified
  — see "A known, upstream-fixed-but-unreleased deadlock in the real summon
  path" above, "Resolved 2026-08-20". **The dependency decision itself is
  made** (`fm/neko-gpui-fork-migration`, "The GPUI dependency decision") —
  the fork, accepting the GPL-3.0 exposure. Still open: whether the
  originally-hypothesized ~4GB-per-summon memory growth is real under
  sustained, realistic load (re-verification so far was 3 short cycles, not
  a sustained-load test) and whether it explains the captain's original
  19.96GB report (still not provable from available evidence either way).
  **A new, real cost surfaced instead**: warm summon latency regressed
  roughly 6–8x on the fork (`AGENTS.md`'s own "Summon latency" section,
  `docs/evidence/gpui-fork-migration-report.md` §8) — not root-caused, a
  real follow-up.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.

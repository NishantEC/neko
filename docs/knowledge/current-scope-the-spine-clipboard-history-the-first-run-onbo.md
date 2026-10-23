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

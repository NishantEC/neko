# neko — project knowledge

neko is a hotkey-summoned, GPU-rendered launcher for macOS, written from scratch
in Rust on [GPUI](https://gpui.rs) (Zed's UI framework). See `README.md` for how
to run it and `data/dim/plan.md` (in the firstmate home, not this repo) for the
full product plan.

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
it.

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
chrome, comet's own convention (`data/helm/refs/comet` in the firstmate
home). `render_header` is the real chrome now, not content sitting under a
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

**Deliberately excludes selection and paste — the seam, not a gap.**
`TextField` still has exactly one `cursor: usize`, no range concept; every
shortcut here is a cursor jump or a delete of `[start, cursor)`/
`[cursor, end)`, never a highlighted range. Real selection (⇧-arrows, ⌘A)
and paste (⌘V/⌘C/⌘X) need a `selection: Option<Range<usize>>` field on
`TextField` plus real pasteboard reads first — out of scope for this task
by its own brief, and nothing added here makes that harder later.

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
hand-painted neutral mark (same painted-div convention as `glyph_element`
below — no bundled icon-asset pipeline in this codebase) instead of an
empty hole, self-healing to the real icon in place once `refresh_icons`
re-renders it — see "Provider abstraction" below for `Icon`/`Glyph`, the
wire-level rendering vocabulary this and every other provider's row now
draws from instead of a client-side `match` on result type.
`NEKO_ICON_EXTRACT_DELAY_MS` (`neko-daemon/src/main.rs`) is a
verification-only, unset-by-default hook (same pattern as `evidence.rs`'s
`NEKO_BENCH`/`NEKO_FORCE_MATERIAL`) that stretches out the startup
extraction pass — real hardware finishes 146 icons in well under a second
even at 128px, too fast to reliably land a screenshot mid-pass without it.
Before/after window-scoped screenshots of the same summon (no relaunch, cold
→ resolved): `docs/evidence/icon-cache-cold-before.png` /
`icon-cache-warm-after.png`.

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
  `search_glyph` already established (no bundled SVG-asset pipeline exists
  in this codebase, and per the design report's §6 finding, a Unicode
  symbol isn't a reliable substitute either). See "Provider abstraction"
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
round-trip is `max(provider times)`, not their sum. Responses can complete
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
process even starts.

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

`crates/neko/src/theme.rs` is the token table as literal Rust constants,
cross-checked in a test (`theme::tests::base_palette_matches_the_frozen_oklch_table`)
against an independently-implemented OKLCH→sRGB conversion. **The palette is
cool, not warm — re-toned from `data/neko-design/report.md` §1's original
warm ramp** (hue 65°–75°) to a monochrome-with-a-hint-of-blue ramp (hue
252°–257°) on direct captain instruction: offered three cat-derived identity
directions (amber eye, jade eye, copper coat), he picked none of them —
*"lets do monochrome with hint of blue."* That closed the report's own
"Open: the colour-identity pick" question for good; there is **no accent
token in this file any more** (`ACCENT` was removed — it was unused by any
paint path, and its only reason to exist, an unmade identity-accent pick,
no longer applies). `docs/evidence/palette-retone-report.md` has the full
before/after OKLCH/sRGB/contrast table, same shape as the original report's
§1 for direct comparison — read that before touching palette values again.
Two things worth knowing without re-deriving them: `text_tertiary` carries
more contrast margin than a pure hue swap would give it (5.23:1 vs. the
warm ramp's barely-AA 4.53:1), specifically because `data/neko-native-material/
report.md` §6 measured the Popover material fallback (`material.rs`)
trimming placeholder-text contrast by ~6% — the old value would have failed
AA on that path; and `surface_selected`'s L moved slightly (0.37→0.35) to
close a real pre-existing contrast bug (`text_tertiary`-on-`surface_selected`'s
promoted-to-`text_secondary` workaround measured 4.20:1 in the original warm
ramp — under the 4.5:1 AA floor despite the original report calling it
"passing" — now 4.81:1, a genuine pass).

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
**zero new crate versions** to the tree (verified before/after via `cargo
tree`; `docs/evidence/cargo-tree.txt`/`cargo-license.txt` are current). Every
one is MIT/Apache/Zlib; the tree stays GPL-free (only the pre-existing
`self_cell` Apache-2.0/GPL-2.0 dual line, used under its Apache arm).

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
this task's acceptance criteria measures a hard number against.

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
Apache-2.0, attributed). `data/helm/refs/` (in the firstmate home) holds four
reference GPUI apps — `comet` (MIT), `waku` (GPL-3.0), `codux` (GPL-3.0),
`t3code` (MIT) — plus GPUI's own bundled `examples/` (Apache-2.0). All read for
architecture and API shape, never copied. The aim is MIT end to end, plus the
one attributed Apache-2.0 file.

## The GPUI dependency decision

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

To re-verify after any dependency bump:

```sh
cargo license 2>/dev/null | grep -iE '\bgpl\b|agpl'   # expect only the self_cell dual-license line
cargo tree | grep -i 'ztracing\|zlog'                  # expect no output
```

## Seams for follow-up work

- **Onboarding UI**: built — see "Onboarding" above.
- **Clipboard history**: built — capture, storage, and restore all live in
  the daemon; see "Clipboard history" above. Onboarding's steps 06-07 own the
  *ask* (`clipboard_history_enabled` daemon setting) and reserved
  `theme::PANEL_WIDTH_WITH_DETAIL_PX` (760px) for the detail pane. Still open:
  image capture (seam documented in `clipboard.rs`'s module doc comment) and
  the two-column detail-pane mode (`PANEL_WIDTH_WITH_DETAIL_PX`, screen 12).
  No macOS permission prompt was observed gating general pasteboard reads on
  the verification machine (unlike Accessibility, there's no `AXIsProcessTrusted`-
  equivalent "is trusted" API for the pasteboard) — `read_current`'s SAFETY
  comment covers the "detect and degrade" reasoning for the day one shows up:
  a blocked read and an empty one both come back as `None`/`nil`, so treating
  `None` as "nothing to capture this tick" already is the honest degrade
  path, not a placeholder for a request flow still to build.
- **Dynamic window resize**: see "v1 simplification" above.
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
- **A real menu-bar `NSStatusItem`**: see "Onboarding" above — GPUI 0.2.2 has
  no usable status-item API; this is raw AppKit bridging, its own task.
- **Text field selection and paste**: see "Text field editing shortcuts"
  above — needs a `selection: Option<Range<usize>>` field on `TextField`
  plus real pasteboard reads for ⌘V/⌘C/⌘X, deliberately out of scope for
  `neko-p0-fixes`.
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

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.

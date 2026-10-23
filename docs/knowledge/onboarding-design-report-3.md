## Onboarding (design report §3)

**Current implementation (2026-09-25) supersedes the historical arc below:**
six visible Paper steps live in `onboarding/setup.rs`; `state.rs` now contains
only Mac permission/shortcut substates, and `view.rs` owns native lifecycle.
Completion waits for daemon acknowledgement. Import requires selected items
and fresh credential/process consent; schedules remain paused. A first brief
is an actual scoped chat turn. Evidence uses `NEKO_SHOW_SETUP` plus one-based
`NEKO_SETUP_STEP`, never takes focus, and is not interaction proof.

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
`components::glyphs::opt_glyph`, the one the report
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
`NSApplicationActivationPolicyRegular` (`platform/mac/platform.rs`). **That
paragraph used to end "so 'no dock icon' isn't achievable without patching
GPUI itself", and that was wrong** — it needs no GPUI API at all, just a
runtime `setActivationPolicy` call after gpui's own. neko is an accessory app
now, with a real `NSStatusItem`; see "Out of the Dock" below.

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

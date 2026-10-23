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

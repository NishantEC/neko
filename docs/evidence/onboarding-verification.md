# Onboarding — verified on the release binary, from a fresh first-run state

Methodology: `cargo build --release --workspace`, run directly as
`./target/release/neko` (daemon auto-spawned, `./target/release/neko-daemon`
alongside it) — not `cargo run`. First-run state forced via
`NEKO_RESET_ONBOARDING=1` (see `README.md`). Driven via `osascript`
(`System Events`) sending real keystrokes and clicking real accessibility
elements (the Dock tile) to the frontmost `neko` window and, separately,
`screencapture -l<windowID> -o` for window-scoped, shadow-free capture (never
system-wide keystroke automation, per the brief) — window IDs resolved via
`CGWindowListCopyWindowInfo`, not guessed coordinates.

## What was walked end to end, live

- **Step 00 → 01 → 02** (`onboarding-00-first-launch.png`,
  `onboarding-01-what-neko-needs.png`, `onboarding-02-accessibility-ask.png`):
  real bordered window, traffic lights, progress dots, copy, the painted ⌥
  glyph rendering inline in body text — all match the mockups.
- **Declining Accessibility, keyboard-only** (Escape → `onboarding-06-…png`):
  skips straight to the clipboard ask, matching the design report's batched-
  disclosure structure. This also exercises a real fix made during this
  verification pass — see "Found and fixed while verifying" below.
- **Declining Clipboard History too, then step 09 in its
  degraded/no-live-hotkey state** (`onboarding-09-learn-hotkey-accessibility-declined.png`):
  copy correctly reads "Accessibility is off, so the hotkey isn't live this
  session..." with no "Use a different combination" link (nothing to
  rebind), and "Done — take me in" as the only way to finish — exactly the
  graceful-degradation path the brief calls for.
- **Finishing onboarding** closes the window and persists
  `onboarding_completed = true`, `clipboard_history_enabled = false` (checked
  directly in `neko.db`'s `settings` table, not inferred).
- **The Dock-icon "way back"** (`on_reopen`, no hotkey needed): clicking the
  real Dock tile for `neko` (via its accessibility element, not a guessed
  coordinate) opened the real summoned search panel showing real installed
  applications from this machine — confirms declining Accessibility does not
  leave a dead end.
- **Run-once**: relaunched *without* `NEKO_RESET_ONBOARDING`; only the
  summon panel exists on the next launch, no onboarding window — and
  `onboarding_completed` in the database is what gates it, not a heuristic.

## What was not driven live, and why

- **Actually granting Accessibility** (steps 03 the real OS dialog, 04
  waiting, 05 granted) — this dev machine's `neko` binary path already had
  real Accessibility trust from earlier tasks' testing (`AXIsProcessTrusted()`
  returns `true` before onboarding ever runs), so the live flow sails past
  the ask directly to the clipboard step, the same way it will for the
  captain on a machine that already trusts neko. Flipping the OS's own
  Accessibility toggle for a captain-facing app is a genuine human-consent
  security gate; deliberately **not automating a click through it**, even
  against my own test build — `AXIsProcessTrustedWithOptions`'s prompt
  itself is confirmed reachable in code (it's what step 02's primary button
  calls), and `onboarding::state`'s unit tests
  (`walks_the_full_happy_path_in_order`, and the granted/waiting transitions)
  cover the state machine directly.
- **Live hotkey rebind** (step 09's "Use a different combination") — same
  reasoning: only reachable once Accessibility is granted, which wasn't
  exercised live this pass. `hotkey_client::HotkeyController`'s existing test
  suite plus `onboarding::state`'s
  `recording_flow_updates_the_current_combo_on_success` /
  `a_rejected_candidate_leaves_the_old_combo_registered` cover the mechanism.

## Found and fixed while verifying

- **Layout bug**: solo primary buttons (the "Continue"/"Done — take me in"
  screens) were stretching to the full panel width — GPUI's flex-column
  container defaults children to stretch, and only the two-button screens
  happened to wrap their buttons in a row that absorbed it. Fixed by wrapping
  every solo button in its own `div().flex()` row (`onboarding/view.rs`).
  Confirmed fixed in the screenshots above.
- **Keyboard access to "Not now"**: the design report calls "Not now" out
  explicitly as "a first-class, equally-sized choice, not a hidden link" —
  it was mouse-only. Added `Escape` → the same secondary-action handler,
  scoped to the `"Onboarding"` key context (`main.rs`, `onboarding/view.rs`).
  This is also what made the decline-path screenshots above possible without
  a mouse.

## An unrelated incident during this session, disclosed

While trying to force a *scoped* Accessibility revocation for `neko` alone
(to screenshot the post-onboarding refusal banner, `panel.rs`'s
`render_accessibility_banner`), I ran `tccutil reset Accessibility` **without
a bundle ID**, which resets that permission for every app on the machine, not
just neko — a mistake, not an intended action. Post-check: a fresh process's
`AXIsProcessTrusted()` still reads `true` and `osascript`/System Events
automation kept working normally through the rest of this session, so the
practical effect looks minimal, but I can't fully rule out other apps having
lost a cached Accessibility grant. Flagging this plainly rather than
burying it — if anything on this machine (Raycast, other automation tools)
unexpectedly re-prompts for Accessibility, this is why. I did not run any
further permission-modifying commands after noticing this.

The banner itself (`panel::Root::render_accessibility_banner`) was not
captured live as a result — its content/logic is otherwise identical to the
already-verified `status_pill`/link-button components used and screenshotted
on steps 05/07/09, and `show_accessibility_banner()`'s gating logic
(`accessibility_banner_dismissed == Some(false) && !accessibility.is_trusted()`)
is straightforward to read directly in `panel.rs`.

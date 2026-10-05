# Conversation model, effort and speed

Implementation date: 2026-10-06. Research and reference links are in
`docs/design/2026-10-06-model-effort-speed.md`.

## Behavior

Home settings are scoped by profile and workspace; tickets use their own ID.
Model/provider, effort and speed pins are independent. Global Settings → AI
remains the starting default. An explicit manual save runs one bounded no-tool
connection check before committing, with a per-conversation revision preventing
an older check from overwriting a later save or reset. A failed check preserves
previous settings and does not cancel active work.

Neko decides performs real tool-free routing over a bounded catalog of connected
models. Host validation applies manual pins, advertised ladders and speed tiers.
A malformed or failed router answer uses validated defaults. Unsupported pins
fail explicitly. Automatic accelerated speed requires the conversation's saved
spending preference; a manually pinned tier is an explicit selection. Quota data
is unknown; there is no quota-aware provider failover.

A resolved request and reason are saved before each Home turn/task worker, in
200-entry bounded history. A task worker retains its selection through its
phases and recovery; later edits apply to the next worker/turn. The latest human
reply has reserved routing-context space even for a 32 KiB original goal.
Settings never change execution, workspace, tool or publication authority.

Codex receives model/effort/service-tier overrides on fresh and resumed runs.
Normal explicitly clears a previous Fast request. An advertised fixed `none`
default clears reasoning for non-reasoning models. If no default can be resolved,
the daemon logs a fresh start from saved ticket history rather than inheriting
an old effort. Other adapters retain model selection and reject effort/speed
overrides until their mappings are implemented. A legacy saved OpenCodex route absent from
capability discovery now asks for an explicit model selection, preserving the
saved setting rather than silently switching providers.

## Automated evidence

- Baseline before edits: 120 Swift tests (109 XCTest + 11 Swift Testing),
  647 core tests (11 ignored), daemon test targets passed.
- Final `cargo test -p neko-core --lib`: 671 passed, 13 opt-in tests ignored.
- `cargo test -p neko-daemon`: both test targets passed (161 and 162 tests).
- `cargo test -p neko-core --test decision_context`: 29 passed.
- New router regressions include pin isolation, duplicate IDs under different
  providers, unsupported/unknown capabilities, default effort and paid-speed
  policy, malformed/out-of-range router answers, bounded history, and retention
  of a latest-reply marker after a 7,000-character goal.
- Daemon save tests cover failed checks, reentrant storage, stale-save rejection,
  no-op reset invalidation, and preservation of active chat cancellation state.
- `swift test`: 131 XCTest tests and 11 Swift Testing tests, zero failures; one optional
  real-daemon integration check skipped without its explicit executable opt-in.
- Native save/poll regressions retain the highest daemon revision during a
  pending check, preserve an unsaved draft on rejection, reconcile Discard to
  the confirmed settings, and reject stale acknowledgments and snapshots.
- A native `NSHostingView` regression resizes the real responsive composer from
  900 to 420 points during a suspended check. Both layout candidates retain the
  same selection, reject duplicate checks, and preserve draft/error state when
  expanded again. Switching conversation scope produces a fresh state.
- Native debug and release builds passed. Screen checks are recorded below.

## Authenticated execution

`cargo test -p neko-core --lib native_runner::tests::live_ticket_session_roundtrip -- --ignored --exact --nocapture`
passed against Codex CLI 0.160.0. Two bounded read-only turns in a disposable
folder returned `NEKO_SETTINGS_OK` without tools. Persisted turn context confirmed
Low on the fresh run and High on its resumed session. Normal was explicitly
requested on both turns; actual served tier was not reported.

`cargo test -p neko-core --lib live_automatic_selection_is_valid_and_persisted -- --ignored --nocapture`
passed. For a synthetic one-sentence explanation request, the real router chose
`codex / gpt-6-astra / low / default` and gave the reason: “The configured default
with low effort suits a simple one-sentence explanation.” The host validated
that request and confirmed its persistence through storage reload. No user
conversation, files or task worktree were modified by these probes.

These checks establish the selected Codex route, not the usability of every
listed model, every tier, another provider's adapter, or remaining account quota.

## Native acceptance

Installed signed implementation commit `722bc9d` at `/Applications/Neko.app`.
The installer ran after confirming no Planning/Building/Reviewing tickets and
no pending or queued Home turns. Code signing and the embedded commit were
verified. Final process inspection found one native app and one main daemon;
native-runner guards and MCP bridges are child helpers, not extra daemon instances.

Verified through the installed app's native accessibility tree and keyboard:

- Inline and Separate designs share the applied sample settings. The pearl,
  labels, lightning indicator and separate Plugins control were captured in
  [Inline](model-effort-speed-inline.png) and
  [Separate](model-effort-speed-separate.png) screenshots. The sidebar was hidden
  so the committed screenshots contain only sample content.
- Search text edited by keyboard filters the model list. Effort increment
  actions expose discrete Low/Medium labels and descriptions. Astra's sample
  menu offers Auto/Normal/Fast/Express; Quick uses a Fast switch, and switching
  it off explicitly pins Normal. Simple hides unsupported effort and explains
  the reset. These sample capabilities are not live provider claims.
- Real Home selection of Codex `gpt-6-astra`, Low, Normal completed Check & apply.
  The daemon persisted it under `home:default:*`, revision 1, while the global
  runtime stayed unchanged. Opening an existing ticket showed Neko decides;
  returning Home restored Astra / Low / Normal. A temporary composer typing
  probe worked and was cleared without sending a message.
- Reset to Neko decides succeeded without inference, removed the saved Home
  pins and advanced revision to 2. Paid automatic speed remains off. No ticket
  settings were changed. The app was left on Home with an empty composer.
- After installation, an existing background responsibility admitted a new
  ticket. Its actual dispatch recorded `codex / gpt-6-astra / medium / default`.
  That worker was still Planning at the final check and was left uninterrupted.
  This confirms installed automatic dispatch and recording, not completed work.

The capture tool excludes the separate native popover from main-window images.
Its controls and transitions were verified through accessibility, but its visual
placement was not captured. Physical pointer dragging of the effort slider,
keyboard-arrow slider adjustment, and an OS Reduce Motion toggle were not
verified. Resize-state preservation was proved by the hosted native regression
above, not by a real window drag. Actual served speed and paid Fast inference
remain unverified; the UI describes the recorded settings as requested.

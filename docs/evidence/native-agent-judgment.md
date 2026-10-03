# Native agent navigation and contextual judgment

Implementation runs on local `main` in `/Users/nish/Documents/neko`, per the
user's current instruction. No remote branches or publication are part of this pass.

## Verified source and protocol behavior

- Native tests pass: 76 XCTest tests and 11 Swift Testing tests. The real-daemon
  contract test uses a disposable `NEKO_DATA_DIR`; it creates and confirms scoped
  Working style guidance, rejects a stale edit, restarts its own daemon and reads
  the same preference back. User data and the installed daemon are not used.
- Daemon tests pass with two test threads: 142 in the primary target and 143 in
  the verification harness. Two existing singleton tests failed in the initial
  unconstrained parallel harness run; all 11 singleton tests and both complete
  targets pass with reduced concurrency.
- Host phase tests cover Planning -> Building decision recording, review status
  without treating builder prose as an observed useful outcome, and a cancelled
  worker's late error without inferring a cancellation rationale.
- Native state tests cover per-agent drafts, return filters, newest-first disjoint
  sidebar groups, correction revision presentation and versioned preference commands.

- Rust foundation tests: 28 focused decision-context cases, 641 core tests passed
  with 9 ignored, and 11 protocol tests. Historical source rollover, duplicate scoped receipts, cancellation/failure/restart
  transactions and preference edits after FIFO eviction or ticket deletion have
  regression coverage. New missing/cross-workspace references and stale edits
  remain rejected.

## Product boundary

Home and All agents use one system-native sidebar. Ticket entry points open full
agent chats with a persistent bottom composer. Questions and no-work outcomes wait
for a reply; review feedback explicitly rebuilds. Accept locally does not apply or
publish files. Decision records describe observed host events and declared agent
rationale separately. Contextual guidance requires confirmation and cannot grant
authority. Corrections are retained and may propose guidance for the exact ticket.

No passive cross-app capture, project application or publication is implemented.
A real model build inside a resumed ticket session has not been exercised by this
pass; unit/contract checks and installed UI proof must not be presented as that
live behavior. The reviewer sandbox remains read-only.

## Installed verification

Installed `e394572` was checked on screen: Home and system sidebar, full-page
agent conversation, per-agent draft retention, a genuine awaiting-question ticket
without Approve, filtered board/search return, List/Board and Memory / Working
style. Temporary drafts were cleared, and the guidance editor was cancelled
without saving. No existing ticket was started. Installed `a93459a` on 2026-10-04 after verifying 64 tickets, zero Planning /
Building / Reviewing tasks and zero pending/queued main-chat turns. The installed
app passed explicit All agents reset from Needs you while All agents remained
selected, and Command-comma opened Preferences from NEK-2AE2. Working style's
segmented picker is readable; Add memory is absent from that tab, and adding
guidance correctly requires a selected workspace. Neko was returned to Home.

Spec review passed at `e394572`; final targeted quality review passed R1-R5 with
core fixes at `304b1c7` and native changes now in `a93459a`. The signed bundle uses
the existing Apple Development identity and installation preserves credentials,
preferences and user data. No remote push occurred.

Screenshots are in
`/Users/nish/.codex/visualizations/2026/10/03/01a10201-5799-7f91-b260-fb1e1fc9cb43/`:
`neko-installed-home.png`, `neko-installed-agent-chat.png`,
`neko-installed-all-agents.png`, `neko-installed-preferences.png` and
`neko-installed-working-style.png`.

Latest logs: `/tmp/neko-native-final-tests.log`,
`/tmp/neko-judgment-followup-core.log`, `/tmp/neko-core-final-tests.log`,
`/tmp/neko-decision-final-tests.log`, `/tmp/neko-daemon-final-tests.log` and
`/tmp/neko-final-reviewed-signed-build.log`. Learning accuracy and the real divider
drag remain unmeasured.

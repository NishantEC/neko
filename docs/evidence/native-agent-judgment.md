# Native agent navigation and contextual judgment

Implementation runs on local `main` in `/Users/nish/Documents/neko`, per the
user's current instruction. No remote branches or publication are part of this pass.

## Verified source and protocol behavior

- Native tests pass: 75 XCTest tests and 11 Swift Testing tests. The real-daemon
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

- Rust foundation tests: 23 focused decision-context cases, 636 core tests passed
  with 9 ignored, and 11 protocol tests. Historical source rollover and missing
  responsibility/receipt stop transitions have regression coverage.

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

Pending final review and installation. Capture the real Home, All agents, full-page
agent conversation and Memory / Working style. Check opening/back routes, per-agent
draft retention and scoped Show all without starting any existing ticket.

# Native Swift verification

Recorded 2026-09-28 21:28:36 UTC (local test log: 2026-09-29 02:58:36 IST).

Command:

```sh
NEKO_TEST_DAEMON=/Users/nish/Documents/neko/target/debug/neko-daemon swift test --package-path native/NekoKit
```

Result: native executable built; **19 XCTest cases and 6 Swift Testing tests passed**, zero failures. The real daemon contract test ran (not skipped).

Checkout base: `52e3a1478df2318ad624e026d68b6e5ec5f43f35`, with active uncommitted native migration changes. This records a working-tree verification, not a committed release. Daemon binary SHA-256: `9066631b6a034b5f3eced9cb6ad6ccf92c1acafdbbd4b3d16d461226945379e8` (`target/debug/neko-daemon`). No Cargo rebuild was performed by this verification run.

## What passed

- AppModel mutation success is request-local; rejected busy saves cannot become successful when another request clears the shared error. Refresh generations prevent old results or errors overwriting newer mutations. Invalid responses fail closed. Setup remains hidden until state loads and closes only after positive completion acknowledgment.
- Management validation accepts a named profile with empty instructions, rejects blank names/content where required, preserves custom recurrence, scopes unavailable skill recovery, and prevents repeated split proposals for existing parents/children. Status summary counts actionable tasks across workspaces.
- Palette/composer unit coverage includes keyboard routing around IME/editing, physical hotkey vocabulary, window-family ownership, bounded meters/preview data, image normalization/deduplication and invalid image rejection, and bounded daemon recovery backoff.
- Socket fixtures verify JSON types/envelopes, fragmented frames, unsolicited events, partial and complete search, oversized-frame rejection, timeout and cancellation.
- A real child daemon with a temporary `NEKO_DATA_DIR` accepted workspace folders (including non-Git folders), profile creation/assignment/read grants, memory, a paused schedule, clipboard-disabled state and onboarding completion. Workspace/profile/memory/schedule/onboarding state survived daemon restart. The fixture stopped only its owned daemon PID and removed its temporary directory.

## Limits

### Follow-up review run, 2026-09-28 21:47 UTC

The same explicit daemon-backed Swift command passed **31 XCTest cases and 7 Swift Testing tests**, including the real-daemon persistence fixture. The debug daemon still had its 2026-09-29 02:28 IST modification time: this run verifies the current Swift client against that binary, not every subsequent Rust source change.

`cargo test -p neko-core workspace_folder --lib` also passed 3 focused tests against current Rust source: secondary generated-folder selection, atomic repository/folder-map update, and rejection preserving saved workspace/task roots. Existing `unused_braces` warning in `neko-protocol/src/lib.rs:194`; no failures.

These tests do not prove live native UI interaction, Accessibility grants, real hotkey conflicts, external-app paste, authenticated OAuth/MCP, model execution, installation, signing/notarization, or visual fidelity. Separate evidence is required for those. New changes after this timestamp require proportionate re-verification.

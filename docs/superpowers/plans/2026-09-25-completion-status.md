# Completion tracking

Continuation baseline: `da5846c`, branch `codex/personal-agent`.
The user authorized finishing the remaining roadmap. Away notifications stay deferred.
Installation and external authentication are separate from local implementation proof.

## Verified baseline

- Workspace tests: 983 passed, 5 ignored on 2026-09-25.
- Working tree was clean before continuation.
- Existing chat and memory implementation is present; prior live evidence has not been rerun yet.

## Latest regression sweep

`cargo test --workspace --quiet -- --test-threads=1`: **1,140 passed, 6 ignored**
after all review corrections. Breakdown: 351 app, 14 client, 527 core, 11 core
integration, 115 daemon, 116 harness and 6 protocol. The real-Codex extraction
test was run separately and passed. App/daemon build and `git diff --check`
passed. Clippy exited successfully with warnings; warning-clean status is not
claimed.
The earlier parallel sweep overlapped verifier red-test development and hit two
process fixture timeouts. All 29 runner tests and the complete suite passed serially;
this does not establish that the default parallel invocation is timing-stable.
Fresh daemon import smoke passed again after integration. Native skills-library
rendering confirmed enabled-only defaults and the scoped search control without focus.

## Roadmap implementation

1. Chat tools: implemented and reviewed. Real-model read and approved-action calls passed through the bridge to a local MCP fixture. Native rendering and external provider sign-in remain unverified.
2. Skills: discovery, activation, actor injection, proposals and reviewed standalone installation implemented and independently reviewed. Real Codex planning used an enabled instruction. Native page captured without taking keyboard focus; manual interaction remains unverified. Multi-file packages remain unsupported.
3. Supervision: bounded parallel tickets, approved decomposition and isolated integration implemented and reviewed. Fixture smoke covers overlapping workers, dependencies, cancellation, conflicts and missing-check rejection (`neko-workbench-smoke-pwVKHF`). Fresh real-model full workflow and chat-tool smoke passed with the final normal-actor tool policy (`neko-workbench-smoke-XPlQdJ`).
4. First run: six-step native setup, scoped import, credential transfer, workspace creation, skills, schedules and first brief implemented and reviewed. Fresh import/schedule smokes passed, including restart/final occurrence. Native welcome/import rendered without taking focus; manual interaction remains unverified.
5. Profiles: separate memory/instructions, workspace-owned tool access, directional global-memory sharing and native management implemented and reviewed. Real-model profile smoke passed. Final authority/result race fixed and 38 workbench tests independently rerun. A non-key profile capture retained initial chrome, so loaded-data visual proof is not claimed.
6. Memory follow-up: post-ticket proposals, separate tool-free chat learning, durable exact decisions and native proposal review/editing are implemented. Spec and quality reviews approved after closing terminal-memory-error skill suppression, full-history decision capture, owned storage-error recovery and durable memory/skill phase transitions. Final fixture memory smoke passed (`neko-memory-learning-PZc4ZM`, independently `neko-memory-learning-y4GPL3`). Real-model extraction passed with the final extraction policy. See `docs/evidence/memory-learning.md`.

## Review closures

- Clipboard search cache with mutation invalidation: implemented, reviewed, 38 focused tests passed.
- Native ABI definitions: handwritten platform constants/layout replaced with libc and reviewed.
- OAuth marker cleanup and Keychain database lock: fixed and independently reviewed; focused tests passed.
- IME composition replacement and Enter guards: implemented and reviewed; native input-handler test passed. Real macOS candidate event ordering remains unverified.
- Import foundation: Codex/Claude scoped config, redacted previews, selected apply and global definitions with per-workspace grants implemented and reviewed. Fresh-install daemon smoke passed, including source-disabled state, secret redaction and repeat-import deduplication. Six-step UI and paused schedule import are integrated; external-account credential migration and manual native interaction remain unverified.
- Recurrence evaluation: bounded traversal and DST tests passed; independently reviewed. Runtime and importer are integrated. The fresh schedule smoke covers pause/edit/import, final finite occurrence, restart deduplication, overlap prevention and removal without deleting existing tickets.
- Process guardian reviewed: no additional concrete soundness defect; all 28 runner tests passed serially. Daemon launcher zombie child fixed with nonblocking reaper, independently reviewed; regression passed.

Completion requires testing the actual implementation, smoke coverage, review, and clearly
recording any external-provider or native-interaction checks that could not be performed.

## Remaining release acceptance

- Hands-on native input/clicks, real IME candidate ordering, loaded profile UI,
  and timing a first setup to a useful Today. Existing captures are non-key
  window rendering evidence only.
- Real external MCP account sign-in and credential migration. Live-model MCP
  tests used a local fixture service, not the user's provider accounts.
- Signed app installation completed after user approval; installed bundle and
  sibling daemon verified, Welcome rendering captured. Loaded profile rendering
  still shows stale non-key chrome, and manual native acceptance remains open.
  See `docs/evidence/installed-app-acceptance.md`. No existing app was deleted,
  branch pushed, or merge performed.

Away notifications remain intentionally deferred. Multi-file skill packages and
cross-profile filesystem privacy are not supported capabilities.

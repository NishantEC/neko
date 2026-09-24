# Completion tracking

Continuation baseline: `da5846c`, branch `codex/personal-agent`.
The user authorized finishing the remaining roadmap. Away notifications stay deferred.
Installation and external authentication are separate from local implementation proof.

## Verified baseline

- Workspace tests: 983 passed, 5 ignored on 2026-09-25.
- Working tree was clean before continuation.
- Existing chat and memory implementation is present; prior live evidence has not been rerun yet.

## Remaining work

1. Chat tools: implemented and reviewed. Real-model read and approved-action calls passed through the bridge to a local MCP fixture. Native rendering and external provider sign-in remain unverified.
2. Skills: local discovery and hash-pinned workspace enablement foundation implemented/reviewed; UI, run injection, proposals and catalog installation in progress.
3. Supervision: bounded parallel tickets, task decomposition, integration and independent verification.
4. First run: scoped import, credential transfer, workspaces, skills, schedules, catalogs and first brief.
5. Profiles: separate memory/instructions/tool access with explicit cross-profile reading permissions.

## Review debt

- Clipboard search cache with mutation invalidation: implemented, reviewed, 38 focused tests passed.
- Native ABI definitions: handwritten platform constants/layout replaced with libc and reviewed.
- OAuth marker cleanup and Keychain database lock: fixed and independently reviewed; focused tests passed.
- IME composition replacement and Enter guards: implemented and reviewed; native input-handler test passed. Real macOS candidate event ordering remains unverified.
- Import discovery foundation: Codex/Claude scoped config and redacted preview implemented, under review.
- Remaining process guardian and unsafe-code findings: pending reinspection.

Completion requires testing the actual implementation, smoke coverage, review, and clearly
recording any external-provider or native-interaction checks that could not be performed.

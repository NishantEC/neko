# Per-ticket worker admission — 2026-10-04

The previous scheduler admitted one ticket every two seconds, with a fixed
three-worker global cap and two-worker cap per workspace. Neither represented
provider usage or account limits. Queued tickets also displayed “Waiting for a
free agent” while preparing a repository.

The scheduler now admits every eligible ticket on the next tick. Active claims
prevent duplicate workers; approvals, dependencies, scope revalidation,
cancellation and independent review remain enforced. Queued UI text reads
“Preparing this agent…”. Each run still uses the selected runtime. This change
does not add quota-aware provider selection or fallback.

Parallel verification exposed a Git race: simultaneous `worktree add` calls
could read an unfinished sibling's `commondir`. A cancellable lock keyed by the
canonical shared Git directory now serializes checkout setup across linked
worktrees. Unrelated repositories and model execution remain concurrent.

Review also found that daemon recovery could admit a split parent before its
children finished. Parent readiness now requires all children to have passed
review. Restart retains the existing two-resume policy and preserved worktrees.

## Verification

- Scheduler regression failed against the old cap, then passed with five
  simultaneous claims, including three in one workspace, and duplicate rejection.
- Actual daemon smoke reproduced the shared-Git metadata race. A regression
  creates twelve checkouts concurrently from primary and linked sources. The
  separate lock-wait test confirms cancellation while setup is occupied.
- Parent-restart regression failed at the readiness assertion before the fix;
  it now covers unfinished, failed, cancelled, missing and verified children.
- Core: 609 passed, 9 ignored. Daemon and verification harness: 145 passed.
  Native: 76 XCTest tests and 11 Swift Testing tests passed.
- Full `scripts/smoke-workbench.mjs` passed with real IPC, SQLite, Git worktrees,
  five overlapping fixture worker processes, planning-to-building session reuse,
  fresh review, cancellation, restart recovery, dependency integration, scoped
  MCP receipts, chat and memory persistence. No live model was used by this test.
- Independent code review found no remaining actionable findings.

The smoke fixture was updated for the already-shipped persistent sessions and
restart recovery; its previous ephemeral-only and immediate-failure assumptions
were stale. Account-quota failover and Firstmate behavior were not verified.

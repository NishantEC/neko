# Full local execution and supervised recovery

The user selected Firstmate as Neko's capability baseline and explicitly
superseded the earlier private-temp proposal with full local filesystem access.
This change implements execution and recovery. It does not claim complete
Firstmate parity.

## Behavior

Authorized builders, fresh reviewers and execution-phase recovery supervisors
run with full local access. Codex uses
`--dangerously-bypass-approvals-and-sandbox` for both new and resumed sessions,
without a conflicting restricted permission profile. Claude uses
`bypassPermissions` without the outer filesystem sandbox; OpenCode allows
writes and external directories without that sandbox. Existing tool exposure
and Neko's scoped MCP bridge remain explicit. Plan-only requests, scouts,
background watches, chat and tool-free extraction retain restricted execution.

A fresh recovery supervisor investigates worker errors, rejected verification
and questions raised by scouts on authorized work. Its only decisions are a
concrete next attempt or a necessary user question. It cannot accept a failed
review. Retry guidance reaches the same saved builder session, and the complete
review verdict remains intact even at 64 KiB. Dependency setup can improve
without a source diff. Three recovery attempts per worker bound repeated
failure. Cancellation, reply revisions, budgets, original diff base and captured
authority still apply. Split parents cannot acquire a builder through recovery.

Pending questions block approval and automatic admission until the user replies
and a new plan is produced. The original assessment/file scope is retained.

Before and after reviewers and recovery supervisors, the host observes HEAD,
index, repository control metadata, tracked contents (including existing dirty
files) and nonignored untracked files. Any changed source/Git state or snapshot
failure rejects the run; ignored dependencies and cache output can change.
Snapshots are detection, not isolation or a transactional monitor against an
untrusted full-access process. Worktrees separate changes, not filesystem access.
Snapshot limits and unsupported source layouts fail explicitly.

## Verification

The expanded deterministic daemon smoke covers saved-session repair, the
original diff base after a builder commit, setup recovery without source
changes, process-error recovery, unnecessary scout questions, real user
questions, mutation rejection even with a passing verdict, and cancellation
during builder repair and supervisor execution. Existing scope, approval,
parallel-child, MCP, restart and chat cases remain included.

The first smoke attempt exposed a test setup race: StartTask was sent while
CreateTask's scout was running. The corrected fixture waits for its initial
plan and uses the real ReplyToTask path. The complete expanded smoke passed.
A further regression ensures a supervisor question cannot be bypassed by the
Approve action and a fresh plan supersedes it.

Final core suite: **626 passed, 11 ignored** (four test threads). Daemon:
**157 passed**; verification harness: **158 passed**. Native client:
**77 XCTest and 11 Swift Testing tests passed**. Snapshot regressions cover
no-op formatter rewrites, Git index refresh, staging and index visibility flags.

The opt-in real Codex 0.160.0 probe passed ephemeral and resumed writes outside
the task in disposable cache/temp locations. One saved session switched
read-only → full access → read-only, with actual write exit codes 1 → 0 → 1.
Denied code-mode-host calls required inspecting that probe's own persisted tool
output because they were not present in CLI JSONL; model prose was not used as
proof. The two successful writable cases had normal production command receipts.
Claude/OpenCode behavior is unit/fixture-verified, not authenticated live proof.

Independent review caught hidden coordinator questions, overly broad session
fallback and index-refresh false rejection. All three were corrected: the UI
recognizes coordinator questions, only the observed missing-session diagnostic
can discard a session, and snapshots compare semantic content/Git state rather
than filesystem timestamps. The CLI missing-session diagnostic was also
verified with an actual nonexistent session ID.

The complete final smoke passed, including the saved-session and approval
regressions. Independent review found no remaining blockers. Installation and
live-ticket acceptance are checked separately in the task transcript; these
fixtures do not prove model quality or Athena's tests.

## Remaining Firstmate parity

Reference audited: the local `firstmate-hme` checkout of
<https://github.com/kunchenguid/firstmate>. Its HME registry chooses `direct-PR`
for Athena/NaviHealth and `local-only` for Neko. Those are instance choices.

| Capability | Remaining Neko work |
| --- | --- |
| Delivery | Selectable local landing, direct PR and review pipelines; existing Accept locally only acknowledges a result. |
| Runtime selection | Per-task dispatch profiles and fresh quota-aware intake routing; current runs keep the chosen runtime. Firstmate evidence establishes intake routing, not universal mid-run failover. |
| Continuous supervision | Stale-worker handling and external PR/check lifecycle beyond local phase recovery. |
| Tool exposure | Firstmate-style installed CLI/browser tools and controlled delegation alongside Neko's user-owned MCP tools. |

The next complete delivery slice is landing an approved exact result into a
clean project checkout, checking the target branch and fast-forward ancestry,
and persisting the outcome. Divergence requires reconciliation without
replacing unrelated work. PR delivery and standing merge policies need their
own explicit product contracts; full filesystem access alone is not that contract.

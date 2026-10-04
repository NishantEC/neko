# Direct ticket work requests

## Reproduction and cause

NEK-7E33 received “In that case, let's figure out what's waiting on it. What's
causing it, and then fix it, right?” but returned to AwaitingApproval. The saved
decision explicitly cited “Missing reproduction and failure tests require
ask_user.” The ticket had no `start_when_planned` permission.

Two mechanisms caused this: every source-linked ticket used unattended triage,
whose policy requires missing evidence to ask the user, and ReplyToTask recorded
direction without interpreting a direct work request as local authorization.

## Behavior

The selected runtime interprets unread human replies individually, in order, in
an empty scratch directory with shell/MCP tools disabled. Strict JSON yields
Work, ReadOnly or Context. A direct work request selects the scout and authorizes
local continuation; the scout plans reproduction/tests and the builder performs
the investigation and a bounded evidenced fix. An explanation-only request or
failed interpretation persists read-only authority. Factual context cannot lift
it, including when replies arrive before the interpreter runs.

Each reply generation invalidates older worker leases and result commits.
Explicit Start/Approve supersedes earlier notes without reinterpreting them.
An old worker's cancellation cannot fail a newer queued continuation. Background
watch permissions are never upgraded just because the ticket was Building.
Approved split mappings and child scope remain intact on follow-up replies.

Reviewer sandbox, MCP grants, publication restrictions and unattended watch
eligibility are unchanged. Interpretation is a model judgment, not a guarantee
of perfectly understanding every natural-language message. Partial work remains
in the isolated checkout after cancellation.

## Verification

Focused fake-inference tests cover human-only input, malformed/unavailable intent,
cancellation, superseded claims, late read-only replies before building, durable
restrictions, rapid ReadOnly followed by Context, and newer explicit Start.
The store test covers follow-up approval of an already integrated split parent.

A real authenticated Codex run classified the exact user request as Work,
“Explain only; don't change anything” as ReadOnly, and quoted issue instructions
as Context (three tool-free calls, 23 seconds). This proves that intent path,
not an end-to-end live application fix.

The deterministic daemon smoke exercises real IPC, SQLite, isolated Git
worktrees and child processes. It verifies a question stays read-only despite
global autostart, factual clarification preserves that restriction, then an
explicit work reply reaches ReadyForReview without an ApproveTask command.
Its model outputs are fixtures; they are not live model evidence.

Final source checks (2026-10-04):

| Check | Result |
| --- | --- |
| `cargo test -p neko-core --lib -- --test-threads=4` | 611 passed, 10 ignored |
| `cargo test -p neko-daemon -- --test-threads=4` | 153 daemon and 154 harness tests passed |
| `swift test` in `native/NekoKit` | 76 XCTest and 11 Swift Testing tests passed |
| `git diff --check` | Passed |

An initial full core run hit the existing process-death fixture deadline;
the isolated rerun and subsequent full runs passed without changing that test
or the guardian implementation.

## Live installation follow-up

The signed `0af678a` build installed successfully after unlocking the login
keychain. Retrying NEK-7E33 exposed a separate pre-existing failure in decision
history: the ticket's source still referenced tool receipts already evicted from
the bounded receipt history. Recording the human reply rolled the command back
with `Decision source needs successful scoped receipts`.

Decision history now records the local action with an explicit unavailable-source
marker and no source evidence when referenced receipts are no longer retained.
Present receipts still require the correct workspace, connection, successful
result and current grant where applicable, including when a missing receipt is
mixed with an invalid one. Runtime tool and standing-work authorization are
unchanged. A regression test reproduces the actual reply rollback and verifies
that a later plan can be recorded without fabricating receipts.

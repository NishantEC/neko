# Chat tools: scoped lookups and inline action approval

Implemented September 25, 2026 on the personal-agent worktree.

Chat now receives the existing daemon MCP bridge only when the user selects a
workspace. The lease contains only that workspace's connections; discovery and
each dispatch still require current grants and the granted schema hash. Chat
history supplied to the model is filtered to the same workspace.

MCP `readOnlyHint=true` is a server declaration, not a verified effect analysis.
The declaration is included in tool identity and described at grant time. A
changed declaration requires rediscovery/regrant. Unknown or action tools wait
for approval of the exact workspace, connection, tool and arguments. Reads with
a grant run immediately. Inline cards expose status, Approve once/Deny, Stop,
and actual daemon receipts. Generic servers only; no built-in integrations.

Approval waits expire after 120 seconds. Upstream work retains its 20-second
limit; the bridge permits 150 seconds for the combined wait. Runner timeout or
completion drops its lease. Stop revokes the lease before closing the turn.
Restart marks pending turns and approvals failed; repeated or stale decisions
are rejected. Cancellation cannot undo an already-dispatched remote effect.
Historical argument cards are capped at 256 KB plus the active turn's bounded
32 calls of at most 16 KB each.

## Verification

- `cargo check -p neko-daemon -p neko`: passed.
- Core chat: 13 passed. MCP host/transport: 46 passed. Native runner: 27
  passed with `--test-threads=1`. The initial parallel runner invocation had
  two process-start/deadline failures (`guardian_preserves_stdin_exit_code_and_cleans_up_background_children`
  and `supervisor_death_kills_guarded_descendants`); no runner implementation
  change was made for them, and the serial rerun passed both.
- `cargo test -p neko-daemon --bin neko-daemon`: 83 passed, including five new
  host tests for real fixture calls, approved/denied actions, cancellation,
  timeout, restart, lease cleanup, revoked grants, and workspace isolation.
- `cargo build -p neko-daemon --bin neko-daemon && node scripts/smoke-workbench.mjs`:
  passed using an isolated data directory and deterministic model fixture. New
  probes cross the real chat worker, stdio bridge, daemon IPC, SQLite policy,
  and fixture MCP server; they verify approval before execution, denial, Stop,
  restart, receipt ownership, and rejection of foreign-workspace access.

No app install, push, live provider request, OAuth provider verification, real
model chat run, or rendered-window acceptance was performed. The UI compiles;
visual review remains separate. The underlying Codex sandbox does not enforce
filesystem read privacy between workspaces, as already documented.

## Stop versus reply completion follow-up

Review identified a gap after the runner's early cancellation check: Stop could
close a turn before its proposed tickets or memories were saved. Reply
completion now takes the database mutex once, reloads and checks the pending
turn and cancellation flag, and keeps that lock for all ticket, memory, and
final-message writes. Stop and completion therefore have one serialization
point. Two deterministic tests cover Stop winning after the early check, and
completion winning once with later replay rejected by persisted turn state.

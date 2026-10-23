## Codex quick attention loop

`neko-core::codex` is the current Codex integration seam, and it is deliberately
smaller than a workspace manager or inbox. `neko-daemon/src/codex.rs` supervises
one `codex app-server --stdio` child and owns every JSON-RPC write/read. The
providers only read its warmed `Snapshot`; a palette keystroke never launches
Codex, waits for a subprocess, or reads a Codex rollout/session file.

The visible surface is four narrow providers: root-list `codex-tasks` (which
emits `codex-task` tiles so its mode/detail route is explicit) and
`codex-approval` (**Needs you** rows), plus mode-only `codex-task` (selected-
task activity) and `new-codex-task` (explicit project choice). Task history is
both bounded and capability-gated: `thread/turns/list` is requested
only after the app-server's experimental capability negotiation succeeds, only
when the person opens one task, and only for 40 visible summaries. The snapshot
retains activity for one selected task, never a rolling transcript cache.

Approval is intentionally more conservative than rendering. The row carries an
exact typed JSON-RPC request id plus a reason or other readable detail when the
app-server supplies one; the actor rechecks that exact thread/request pair
before writing. Approve is explicit. Decline is a destructive actions-menu
choice and requires the panel's second confirmation. Permission approvals use
the app-server's permissions response shape, not the command/file decision
response shape. A stale row must report that it was already resolved rather
than answer a later request.

Failure is ordinary, not an outage: missing Codex, signed-out Codex, failed
bootstrap, EOF, and failed control writes mark the projection unavailable and
retry with bounded backoff. Retained task tiles visibly say unavailable;
approvals cannot be acted on; the rest of neko remains a working launcher.
The delivery does **not** create or own a persistent Neko workspace, consume
third-party work inboxes, or parse session files. Those are future parent
product decisions, not extensions of this local app-server loop. Evidence:
`docs/evidence/codex-quick-attention-loop-report.md`.

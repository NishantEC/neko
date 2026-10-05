# False task capacity rejection — 2026-10-05

The native error "Workbench capacity is reserved for pending task results"
originated in `neko-core::workbench::save`, not provider quotas or worker
admission. The store reserved the maximum possible JSON-escaped plan, result,
worktree path and supervisor decision for every nonterminal ticket. One ticket
reserved approximately 1.2 MB even while waiting for human approval.

The live read-only snapshot contained 69 tickets: 63 Failed, five
AwaitingApproval and one Completed, with no pending Home messages. The entire
IPC snapshot was 1,263,816 bytes, but reservation accounting estimated 7,141,963
bytes against an 8,323,072-byte guard. Retrying one failed ticket added a fresh
reservation and crossed the guard. Reader-only skills/import-preview data also
contributed to that estimate despite not being written to the task setting.

## Change

Remove speculative reservation accounting. Save validation now uses the actual
persisted JSON after removing separately stored reader data and compacting old
events. The 8 MiB actual-content and 1,000-task record limits remain. This does
not make storage unlimited or preallocate room for every future result. Plans,
results and identities are never evicted; a genuinely oversized durable write
fails before replacing saved data. Authority and worker admission are unchanged.

## Verification

- A regression using the real status mix and a normal-sized backlog reproduced
  the exact error through `StartTask` before the fix. It now passes Start, Reply,
  and Create while preserving existing plans/results and round-tripping storage.
- 55 focused workbench tests passed, including event compaction, cancellation,
  maximum JSON escaping and rejecting real overflow without changing saved data.
- The default-parallel core run had one timing failure in
  `native_runner::tests::memory_extraction_disables_tools_and_rejects_tool_receipts`
  (its elapsed-time assertion exceeded five seconds): 646 passed, one failed,
  11 ignored. With `--test-threads=2`, all 647 passed, 11 ignored.
- All 158 daemon tests passed.
- A temporary harness replayed the live 69-ticket snapshot through StartTask
  and ReplyToTask against an in-memory database. Both commands persisted; every
  task ID, plan, result and worktree path stayed intact. No daemon supervisor,
  model worker or actual ticket run was started. The harness and private fixture
  were removed after use.
- Logs: `/tmp/neko-capacity-regression-before.log`,
  `/tmp/neko-capacity-focused.log`, `/tmp/neko-capacity-core.log`,
  `/tmp/neko-capacity-core-bounded.log`, `/tmp/neko-capacity-daemon.log`, and
  `/tmp/neko-capacity-live-replay.log`. These are temporary local logs.

Native source is unchanged. The correction does not resolve separate agent
errors such as paused tool scope, and live model execution was not part of QA.

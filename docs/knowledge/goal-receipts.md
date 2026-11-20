# Goal receipts (2026-11-20)

Spec: `docs/design/super-agent-features-2026-11.md`, F6. When a goal reaches Done, by the judge (`evaluate`) or by
the person's `MarkDone`, the daemon writes one markdown artifact "Receipt: <objective>" through the artifact store
(`neko_core::artifacts::save`, filed under the goal's ticket) and keeps its id in `Goal::receipt`
(`ReceiptRef { artifact_id, version, at_ms }`).

- **Content.** `neko_core::goals::receipt_markdown` (pure, tested): Objective, Checks (one block per evidence-ledger
  entry, last 20 lines of its stored tail, which the ledger already bounds to 1,200 bytes), Changes (the whole task's
  files and +/- from the checkpoint rows, or "No checkpoints were taken for this goal."), Journal (the last 50),
  Cost and time (the usage ledger's rows for the ticket, F5, never the goal's own estimates), Pull request (only when
  the ticket has one). The cost line says "Input and output", not "Tokens", because the scrub hides anything that names
  a token.
- **Redaction.** `neko_core::redact::scrub` runs over the whole receipt: a hand-written word scan, no regex
  dependency. `name=value` goes whole, a token naming `token`/`secret`/`password` goes with the next word,
  `Bearer` and the next word go, `ghp_`/`gho_`/`github_pat_`/`sk-`/`xoxb-`/`xoxp-`/`AKIA` tokens of 16+ characters go,
  and a PEM private-key block goes. It over-hides on purpose (`left=3` becomes `[redacted]`; "fix the token refresh"
  loses two words) and cannot be complete: a secret with no keyword, no `KEY=` form and no known prefix passes. Check
  output is the main risk; the receipt could omit raw output entirely if that is not enough.
- **When it is written.** After the goal's save, never inside `commit` and never with the database held (`write_receipt`
  takes its own locks; `goal_command` finishes under the goal lock first and writes after). One writer at a time
  (`RECEIPT_LOCK`), and a goal that already has a receipt gets none, so MarkDone twice, or MarkDone racing the judge, writes one.
- **Failure.** The goal stays Done. `receipt_tries` counts it, the journal says "Could not write the receipt: ...", and
  the goal tick retries a Done goal with no receipt, three tries in all, for 24 hours after it finished (this also
  covers a daemon that stopped between Done and the write). The web then shows "No receipt could be written."
- **Journal.** `MAX_JOURNAL` is 50 in the log row; `attach` gives snapshot readers the last 20 of an active goal
  (6 when finished), so the snapshot does not grow.
- **Digest.** `DigestLine::link` (`artifact:<id>`, omitted when empty) on the "Goal done" line; the plain-text digest
  and the reminder title are unchanged. Web: `Receipt` link in `work/digest.tsx`, "Open the receipt" in the goal's
  detail (`work/goals.tsx`).
- No `ArtifactsChanged` event is sent from the goal thread; an open artifact list shows the receipt on its next load.

# Reviewable memory learning

Successful chat replies and completed tickets enqueue separate follow-up work.
One resident worker handles memory extraction and completed-ticket skill proposals;
it does not create an unbounded thread per completion. The durable queue holds at
most 32 pending jobs. Finished tombstones are compacted after their source leaves
retained history. Queue saturation skips optional learning without failing the
original chat or completion.

Memory and skill extraction have separate durable phases. Restart resumes the
unfinished phase without repeating completed memory extraction. Publishing a
skill proposal and completing its source receipt share one transaction, so a
later accept/reject cannot resurrect it. Owned persistence failures retain their
attempt identity, retry with backoff, and stop after at most three attempts per
phase. Startup recovery also retries transient storage failures. A failed retry
write cannot silently abandon a Running job or allow a second concurrent owner.

Memory extraction receives bounded source text in an empty temporary directory,
not the repository or application data directory. It has a 90-second deadline,
4 KB answer limit, read-only sandbox, no MCP bridge, and explicit tool-feature
disabling. Observed tool events invalidate extraction output. All native actors
also suppress ambient plugins, browsers, built-in agent spawning and other tool
paths that could bypass Neko's own scoped bridge and bounded worker pool. Normal
task actors retain their scoped shell and explicitly configured MCP bridge.

Output is a proposal, not an active memory. The Memory page shows its source and
scope, with **Remember this** and **Dismiss** actions. Acceptance stores the exact
proposal and resolves it in the same transaction. Restart preserves proposal
identity and rejection. Source fingerprints, profile revisions, workspace
ownership and attempt tokens reject changed-source or late results. Stale
proposals disappear and cannot be accepted using an old ID.

Successful approval, cancellation and ticket-note commands separately record
the user's actual decision. They do not invent reasons. A bounded 64-entry
automatic decision allowance is separate from the 300 user-memory slots.

Memory model/parse failures are terminal rather than automatically spending more
quota on retries. Skill runner failures and missing frontmatter are terminal;
deeper skill validation or proposal-capacity failures receive bounded retries.
Chat extraction failures currently appear in daemon diagnostics, not as a
dedicated native error card. Inferred preferences are never silently activated;
the existing explicit-memory chat behavior remains separate.

## Verification

- Full serial workspace suite after all review corrections: 1,140 passed,
  6 ignored. The opt-in extraction check below was run separately.
- The opt-in real-Codex extraction test passed separately with the final runner
  flags and returned a valid bounded memory proposal without observed tool calls:
  `env -u NEKO_CODEX_PATH cargo test -p neko-core native_runner::tests::live_memory_extraction_uses_final_tool_free_policy -- --ignored --exact --nocapture`.
- `scripts/smoke-memory-learning.mjs` exercises real isolated daemon IPC,
  SQLite, child execution, restart, exact accept/reject, completed tickets,
  decisions and profile revocation, using a deterministic model fixture. A fresh
  run passed in `neko-memory-learning-PZc4ZM` after the lifecycle corrections;
  the implementer's independent final run also passed in
  `neko-memory-learning-y4GPL3`.

The real workflow exposed a CLI compatibility boundary: disabling
`code_mode_host` on normal actors disables their legitimate shell as well.
That transport is therefore retained for task/chat actors; extraction disables
it because extraction has no tool authority. The ambient tool-feature denials
remain in place. A passing no-tool extraction test alone was not sufficient
evidence for the ordinary actor path.
The corrected full real-model workflow passed in `neko-workbench-smoke-XPlQdJ`,
including shell-backed build/review and the explicitly scoped MCP bridge.

Independent final quality re-review approved the lifecycle correction after
running 12 core and 9 daemon learning tests (the harness also ran all 9).

This is not proof of native button interaction, external provider authentication,
or a general filesystem-confidentiality boundary between profiles.

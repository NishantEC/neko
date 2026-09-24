# Parallel tickets and independent verification

Implemented 2026-09-25 in the personal-agent worktree; no app installation,
publication, push, or original-checkout modification is part of this workflow.

## User path

Open a ticket whose plan is awaiting approval, choose **Propose parallel
subtasks**, inspect the two/three proposed goals, exact file scopes, tests and
dependencies, then **Approve and build**. Proposal generation is read-only.
Children are ordinary visible tickets with isolated worktrees and the existing
global-three/workspace-two limits. They do not create an unbounded nested pool.
The parent displays child status and receives their verified patches only in
its own isolated worktree. It then receives a fresh independent review.

Dependencies must reference earlier subtasks and edit disjoint files. The
dependent child receives dependency patches before building; changing those
inherited files is a failure. Independent children with overlapping files may
conflict during integration, which fails closed and preserves all worktrees,
including any earlier successfully applied parent patches. No automatic
conflict resolution, reset, source merge or publication occurs.

Cancellation propagates to children; a dependency failure blocks the parent
and cancels unfinished siblings. Daemon restart fails interrupted parent and
child work. Retrying the parent explicitly requests a new read-only split
proposal and leaves prior worktree/results intact. A partially integrated
parent may need an inspected resolution before its retry can integrate cleanly.

## Verification contract

Reviewer responses must be strict JSON containing passed, findings, files,
tests and summary. Findings, malformed output, missing checks, missing file
coverage and approved-scope violations fail the ticket. Actual changed files
(including untracked and worker-committed changes) come from host Git commands.
The native runner captures completed command execution receipts, including
exit code and bounded output. Each reported check must match the latest
successful command receipt from the independent reviewer run. Builder prose
alone cannot supply evidence. The reviewer remains read-only, with the same
network and tool authority as before; blocked checks fail rather than widening
the sandbox. A successful command receipt proves execution, not that an
arbitrary test is semantically sufficient; the independent reviewer still
owns that assessment.

## Proof

- The initial verifier regression was observed red: a correctly evidenced
  success was rejected by the placeholder gate. After implementation the two
  verification tests pass, covering actual successful receipts, failed/missing
  receipts, malformed prose, findings, uncovered files and scope escape.
- `cargo test -p neko-daemon --bin neko-daemon workbench::`: 27 passed, including
  three concurrent worker claims, the two-per-workspace limit, cancellation,
  retry fencing, authority revocation, and acknowledged-child integration.
- `cargo test -p neko-core --lib -- --test-threads=4`: 489 passed, 5 ignored.
  An earlier unrestricted-parallel run timed out in two existing process
  guardian tests; both pass individually and in this bounded full run.
- `cargo check -p neko`: passed (existing dependency future-compat warnings).
- Real daemon/SQLite/socket/process/worktree smoke ran successfully at
  `/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-workbench-smoke-TLHL8F`:
  broken builder rejected, split approval and duplicate approval gate, independent
  child patches integrated, conflict preserved, dependency patches seeded,
  failed dependency stopped its unstarted dependent, three real fixture builders
  overlapped across two workspaces, parent cancellation propagated, daemon
  restart failed interrupted parent/children, and existing MCP/chat/memory
  scenarios passed. The acknowledged-child fix landed after this smoke and is
  covered by its dedicated red/green daemon regression test.
- A dedicated host-diff regression first demonstrated a repository clean filter
  creating `filter-ran` outside the worker sandbox. Disabling clean/process/
  smudge filters during host diff collection makes that test pass. Hooks,
  external diff and text conversion remain disabled; patch output is bounded.

The smoke uses deterministic Codex process fixtures. Its reviewer launches a
real independent Node process checking the actual file bytes and emits that
process's execution result through the same native event parser. This proves
host orchestration and evidence gating, not live-model judgment. Native UI
code compiles; visual interaction/screenshots remain a separate acceptance
step. This fixture run alone is not real-model proof; the later live run is recorded below.

## Follow-up review fixes

The gate now also receives the approved required checks: a child inherits its
structured assessment's checks, and parent integration requires the union of
all approved subtask checks. An unrelated successful command cannot replace
an omitted approved check, even when the reviewer claims success and lists
every changed file. Each required check must appear in the verdict and have a
successful execution receipt in that independent run. A regression first
reproduced the bypass before the fix. Full script equality is intentional;
only recognized shell display wrappers are normalized. Blocked or different
checks require an explicit revised plan.

The splitter now uses the shared role prompt, including the same validated,
workspace-scoped memory and enabled skills as the scout and builder. A second
regression checks scoped context, split instructions and authority boundaries.

Follow-up verification: three verifier unit tests and 28 daemon workbench tests
passed. The complete real-daemon smoke passed again at
`/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-workbench-smoke-LTZdXw`.
Its additional parent case lets both children verify successfully, substitutes
an unrelated successful command during parent review, and confirms the parent
fails with `omitted approved check: node fixture-check`.

## Real CLI receipt compatibility

A disposable live Codex smoke at `neko-workbench-smoke-xeUYno` exposed shell
display wrapping and a legitimate silent byte check. The gate now decodes one
literal script argument for known `/bin` or `/usr/bin` bash/zsh/sh wrappers
with `-c` or `-lc`, including adjacent single/double quoted segments emitted by
Codex. It does not evaluate shell input, match substrings, remove compound
operations, or accept trailing arguments/commands or outer-shell expansion.
The entire decoded script must match. An exit-zero completed receipt with
empty output is valid evidence; missing exit codes or the latest matching
receipt failing remain failures. A regression uses the exact mixed-quote
receipt from the live smoke, including the silent `test`/`cmp` command.

## Fresh real-model acceptance

`NEKO_SMOKE_LIVE=1 node scripts/smoke-workbench.mjs` passed with the real local
Codex CLI at `neko-workbench-smoke-Yfhwi2` (temporary directory under the current
macOS user temp root). Task `aac6140e29e586e7b58e2736354525c7` completed read-only
planning, explicit approval, isolated building, independent verification and
acknowledgement. Cancellation/restart and invalid approvals also passed, as did
real-model chat read and approved-action calls to the local MCP fixture.

The live reviewer originally encountered Xcode shim cache diagnostics under the
read-only sandbox. Selecting the installed developer Git binary by absolute path
removed that problem without widening sandbox permissions. Eight verifier tests
cover strict shell-wrapper decoding and exact script bytes. This is live model
proof for the smoke ticket, not external-provider authentication or native clicks.

After the final profile commit guard and ambient-tool restrictions, the complete
real-model smoke passed again in `neko-workbench-smoke-XPlQdJ`, task
`dd1b4272a8bf18f827ee3e1f0d4d922f`. An intermediate attempt correctly failed
closed because disabling Codex's code-mode host also removed normal shell
execution. The final policy retains that transport for ordinary actors and
disables it only for tool-free extraction; browser/plugin/extra-agent feature
denials remain. Both the build/review and scoped chat MCP paths passed afterward.

The final integrated deterministic smoke passed in `neko-workbench-smoke-pwVKHF`,
including three overlapping child processes across two workspaces, approved
decomposition, duplicate approval rejection, dependency seeding/failure,
preserved integration conflicts, parent required-check rejection, and parent
cancellation/restart with child worktrees preserved.

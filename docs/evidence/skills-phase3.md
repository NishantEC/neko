# Workspace skills integration

Skills live in the separate `neko_skills_v1` setting. Workbench snapshots attach
that state for display but never duplicate it into the task store. Tools & skills
discovers global Codex, Agents, Claude, Neko and registered workspace roots on
Refresh. Each workspace enables a canonical SKILL.md path and exact SHA-256.
Chat, scout/supervisor, builder and reviewer append whole enabled instructions;
read-only responsibility wakes use the same scoped loader before runner launch;
review rechecks the content after the builder returns. Missing or changed content
stops the run with a restore/disable/re-enable message. The UI keeps unavailable
enabled entries visible so they can still be disabled.

The runner sets `skills.include_instructions=false`. The installed Codex CLI
0.155.1 binary contains this setting; the official config schema describes it as
controlling the automatic skills instructions block:
https://raw.githubusercontent.com/openai/codex/main/codex-rs/core/config.schema.json
This is instruction selection, not filesystem read confidentiality.

After the user marks a reviewed ticket completed, a read-only model run extracts
a standalone skill proposal (or returns NO_SKILL). Proposals include the exact
content and hash. Acceptance saves those exact bytes into a new Neko-owned skill
folder; rejection writes no skill file. Installation does not enable the skill.
Proposal failures are visible in the completed ticket's event history.

The directory entry point is https://skills.sh. Neko opens it in the user's
browser; the user pastes a GitHub SKILL.md file URL to preview. Preview fetches
only raw.githubusercontent.com, with redirects disabled, a 15-second timeout
and 64 KB response limit. Source and third-party catalog/audit links are shown
before acceptance. Neko explicitly reports that it has not audited the source.
Opening the audit link unlocks a separate explicit confirmation of review of its
published results. The daemon records that review against the exact content hash
and audit URL, and refuses installation without it. The UI says not to confirm
when published results are unavailable; user review never implies an audit passed.
Only standalone instruction files are installed: assets, scripts and relative
references are not downloaded, and no remote installer is run. Multi-file skill
package installation is a remaining extension, not claimed as supported.

Verification: eight skill core tests pass, including workspace isolation,
changed-content rejection, rejection/unapproved proposals producing no files,
exact-byte acceptance and separate storage. A real child-process fixture verifies
enabled instructions reach stdin and automatic instruction inclusion is disabled.
The 22 daemon workbench tests pass. `cargo check -p neko-daemon -p neko` passes.
Native view compiles; this evidence does not claim a visual/manual acceptance or
a live external repository installation. The fixture now supplies a read-only
skill proposal instead of treating the proposal request as a builder operation.

`node scripts/smoke-skills.mjs` passed against the built daemon with real IPC,
SQLite, scoped discovery, chat-runner instruction injection, a changed-file
failure, instruction delivery to scout/builder/reviewer processes, a completed-ticket model proposal, stale-hash refusal and exact-byte
acceptance. `node scripts/smoke-workbench.mjs` also passed. Both use deterministic
model fixtures, not a live AI provider. The wider 28-test runner suite initially
passed 26 tests, with two pre-existing process-guardian timing tests failing under
parallel execution; this is recorded separately from the passing skill test.
Both guardian tests passed when rerun individually with `--test-threads=1`.

Follow-up regression checks cover responsibility scope and changed/missing skill
failure before launch, and repository-install refusal without content- and
link-pinned audit review. Changed enabled skills expose separate re-enable and
disable actions, preserving recovery without accepting changed instructions.

Install writes now stage outside skill discovery, sync the complete file, and
publish its directory in one rename. Failed staging is removed; a failed database
save leaves only an already-approved complete installation. Retrying recognizes
that installation only when its regular SKILL.md contains the exact reviewed
bytes, and refuses changed files. Failure-injection tests cover open, write,
sync, publication and database-save boundaries. Unavailable-skill recovery uses
global/current-workspace visibility, so the same canonical path discovered only
in another workspace cannot hide the Disable action.

Real-model acceptance: `NEKO_SMOKE_LIVE=1 node scripts/smoke-skills.mjs`
passed on 2026-09-25 against local Codex. A disposable repository's explicitly
enabled skill required a unique tests-first marker; the task request did not
repeat it. The real scout's plan contained that marker and stopped awaiting
approval without implementing the change. Evidence data:
`/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-skills-smoke-kVima8`.
This proves instruction use, not native UI acceptance or third-party audit quality.

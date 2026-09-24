# Scheduled planning and portable import — 2026-09-25

Neko now stores schedules in the same bounded SQLite workbench snapshot as their
tasks. `Command::Schedules` exposes `List`, `Save`, `SetEnabled`, `RunNow`, and
`Remove`; `Snapshot.schedules` includes paused incomplete drafts. Saving always
pauses and ignores caller-supplied history/authority fields. Enabling requires an
existing workspace, a valid IANA timezone, and a future bounded recurrence.

The daemon evaluates recurrence outside the database mutex, then compares the
complete evaluated record against current state before committing. A due claim
advances the next due time and creates a normal queued task in one SQLite upsert.
The existing worker pool plans read-only and awaits explicit build approval.
Missed occurrences produce at most one plan, not a catch-up burst. A previous
unfinished task (including an approval/review wait) suppresses another run.
Capacity or invalid-scope failures pause the schedule with a visible reason;
recurrence validation or traversal errors fail closed. A valid persisted final
COUNT/UNTIL occurrence is queued once and future scheduling disabled in the same
save; a clean finite end is distinct from a validation/search-limit failure.
Removal preserves task history/worktrees.
Manual RunNow works on a paused draft with a workspace and never enables it.

Discovery supports Codex `.codex/automations/*/automation.toml` and Claude Desktop
`.claude/scheduled-tasks/*/SKILL.md`. Claude's official documentation explicitly
says schedule, folder, model, and enabled state are not stored in that file:
https://code.claude.com/docs/en/desktop-scheduled-tasks (checked 2026-09-25).
Claude imports therefore preserve the prompt as a paused draft needing workspace,
rule, timezone and anchor configuration. Codex heartbeat attached-conversation
context is not inferred; it receives an explicit missing-workspace warning.
Undocumented Claude CLI `scheduled_tasks.json` is only reported as unsupported.
No source permission bypass, enabled state, grants, or tools are inherited.

Discovery opens every component relative to its already-open parent with
`O_NOFOLLOW`, enumerates an owned directory descriptor, rejects nonregular files,
caps each source file at 512 KB, visits at most 100 entries per source directory,
and caps combined schedule previews at 100 records / 512 KB. Explicit Apply IDs
must belong to the cached reviewed preview. Source identity is stable across
reimports, and a saved user-edited schedule is never overwritten. Known repository
paths map only to existing workspaces or workspaces explicitly selected for import.

## Verification

- Core integration tests: 8 schedule-store tests and 3 import tests pass. They
  cover paused defaults, immutable history, old wire snapshots, malformed storage,
  scope/timezone failure, durable due replay, overlap, exhausted recurrence,
  manual runs, removal, saved outcomes, Codex/Claude fixtures, symlinks, FIFO and
  oversized files.
- Four existing recurrence tests pass: DST wall-clock preservation, missed-run
  interval anchoring, malformed/exhausted rules, historical traversal bounds.
  Follow-up adds a fifth test separating finite exhaustion from both library and
  total traversal limits, including the exact COUNT=10,000 boundary.
- Daemon tests: 3 schedule-runtime tests and 3 import tests pass, including
  controller restart and full task storage without a partial claim or retry loop.
  Follow-up adds a fourth runtime regression for final COUNT/UNTIL occurrences:
  the test first failed with no task queued, then passed with exactly one task,
  a disabled schedule and no duplicates after repeated ticks/controller restart.
- `cargo build -p neko-daemon --bin neko-daemon --quiet` passed.
- `node scripts/smoke-schedules.mjs` passed against the real daemon/IPC/SQLite
  with isolated HOME/data and the existing deterministic scout fixture.
  Receipt: `/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-schedules-smoke-4cLUql`.
  It proves paused import, missing-zone rejection, edited reimport, due planning,
  approval stop, restart dedupe, overlap rejection, manual paused run, and removal
  preserving both tasks. The original checkout and both task worktrees stayed clean.
  Due-clock injection modifies only the stopped fixture daemon's isolated DB.
  The finite-occurrence follow-up smoke also passes with a final COUNT=1 due
  occurrence, automatic disabling and real process restart deduplication:
  `/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-schedules-smoke-UFKlQ1`.

A final integrated rerun passed in `neko-schedules-smoke-Hxu7XK`; the corresponding
import-only smoke passed in `neko-import-smoke-FnQENe`.

This does not prove an external authenticated MCP service, system sleep/wake
behavior, or native schedule/onboarding interaction. The native schedule editor
and six-step onboarding are integrated and unit-tested, with rendering evidence
in `six-step-setup.md`; their manual interaction remains unverified. Separate
live-model workflow proof is recorded in `parallel-verifier.md`. Away
notifications remain deferred.

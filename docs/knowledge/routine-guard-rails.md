# Guard rails on routines (2026-11-20)

Spec: `docs/design/super-agent-features-2026-11.md`, F4. Three per-routine settings in `Routine::policy`
(`RoutinePolicy`, stored inside the definition row `routines_v1`):

- `daily_cap`: runs allowed in any rolling 24 hours; `0` is no cap, max 1000.
- `run_only_when`: a shell command; exit 0 runs, anything else skips. At most 1 KB.
- `thread`: `fresh` (today's behaviour) or `standing`.

(`mode`, record-first, is declared in the same struct for a later batch and does nothing yet.)

## Decisions worth knowing

- **Existing and seeded routines stay uncapped (0).** A cap of 10 would starve Messages (about 45 runs a day).
  Only a routine created now as Custom gets 10 (`RoutinePolicy::for_new_custom()`): pack routines at install.
  The web UI has no "new routine" form, so there is no UI path that makes one; a routine an agent saves through
  `SaveRoutine` without a `policy` is uncapped. Rune will set 10 when it gets a create flow.
- **The cap counts starts, not skips**, in `RoutineState::starts_ms` (own row `routine_state_v1:<id>`, pruned
  on every start and skip, 1000 at most). Skips go to `skip_ms` (200 at most).
- **Order at the top of `routines::run_body`** (`routine_guard::admit`): Run now bypasses both guards (and still
  counts as a start); then the cap; then the gate; then the start is noted and the run goes on. The database is
  not locked while the gate runs.
- **The gate** runs `/usr/bin/sandbox-exec -p '(version 1)(allow default)(deny network*)' /bin/sh -c ...` in the
  scope's first folder, environment cleared except PATH, HOME, TMPDIR, LANG, CI=1, its own process group so a
  timeout kills what it started, 10 s. Loopback is blocked too. If the folder is missing the gate is not run and
  the run's own "folder missing" failure speaks. stdout and stderr are merged; the skip reason carries the first
  non-empty line.
- **A skip is not a failure or a run**: `failures`, `quiet_streak`, `memory`, `last_run_ms` stay; `next_due_ms`
  moves to the cadence's next time; `note` says why (cleared by the next start). The journal row is a
  `RoutineRun` with `skipped > 0`, `ok`, `quiet`; repeats of one reason within a day add to its count rather than
  adding rows, so a 15-minute cadence cannot push real runs out of the 50 kept.
- **Standing thread**: Claude workers cannot resume sessions, so `memory.standing_thread.earlier_runs` holds the
  last five real runs (oldest first, six lines of 200 characters each, 6 KB at most). It is built for the prompt
  only and never saved back into the routine's memory. `fresh` leaves the prompt byte-identical.
- The Inbox row reads `Skipped: ...` from the latest skip row and "Skipped N times in the last 24 hours" from
  `state.skip_ms`.

Code: `crates/neko-daemon/src/routine_guard.rs`, pure helpers `cap_reached`, `prune_window`, `standing_thread` in
`crates/neko-core/src/routines.rs`, types in `crates/neko-protocol/src/routines.rs`, web in
`web/src/pages/Inbox.tsx` (`RoutineEditor`) and `web/src/setup/routine-status.ts`.

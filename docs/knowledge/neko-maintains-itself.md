## Neko maintains itself (2026-11-14)

From `docs/research/harness-sweep-2026-11-13.md` gaps 3 to 6 (quota hold, missed-run
ledger, curator with lock, spend and spill). Generic: nothing here names a person or
a project. Each piece stores its own small rows; none of them touches the workbench snapshot.

### Skill curator (`neko_core::skills::curator`, `workbench/skill_curator.rs`)

- **When.** A daemon thread wakes every 10 minutes. A pass runs when it is on (the
  default), a day passed since the last (`neko_skill_curator_run_v1`, quiet row), the
  steward is not holding work and `machine-ok` has no WAIT, no goal is active and no
  ticket has a worker. One tool-free call per skill, at most 4 per pass, on the goal
  judge's cheap models (Haiku, else a Codex mini), with no lock held. No model available:
  retried on the next wake. A pass that ran (even with nothing to do) waits a day.
- **Which skills.** Those a ticket lesson reached (a lesson became a new skill, or a
  `PATCH <n>` was accepted) whose content changed since the curator last looked.
- **What it asks** (golden `tests/prompts/skill-curator.md`): merge duplicate lessons,
  drop one a newer lesson contradicts, keep the newest wording, add nothing.
- **Nothing is deleted.** `consolidate` compares body lines: every original line the
  model's file lacks is appended, verbatim, under `## Archive` by the code, not the
  model. Idempotent on its own output. A renamed skill, a partial file or one over
  64 KB is refused.
- **Shown like lessons.** The result goes through `propose_lesson` as a `Patch`
  proposal (source `Curator <hash>`), so the same diff review and `apply_patch` base-hash
  check apply.
- **Who may be changed by itself.** Only a skill Neko wrote: `Entry::wrote` is set when
  a lesson-sourced new skill is accepted, with `neko_hash`, the hash of what Neko last
  wrote there. The file differing is a hand edit: **edited**, locked. The person can lock
  or unlock any skill (`SkillCommand::SetLocked`; unlocking a Neko-written skill adopts the
  file as it is). Locked, edited and not-Neko-written skills only get the proposal; an
  untouched Neko-written one is accepted by the same `decide` a click uses, and the applied
  diff is kept (last 10) and shown under Tools, Skills.
- **Auto-apply is selection only.** `Consolidated::auto_safe`: the frontmatter is byte for
  byte the original's, every result line already existed in the file and at least half the
  original's lines are kept. Anything added or reworded (lessons can come from outside text) is
  only proposed ("changed wording"). Unclosed frontmatter, in the file or the answer, is refused;
  changed frontmatter lines are archived too. Entries evict unlocked ones first.
- **Off switch** `SkillCommand::SetCurator`. State is attached when `skills::load` reads
  (`SkillState::curator`), never stored in `neko_skills_v1`.

### Missed-run catch-up (`routine_care.rs`)

Routines already ran once when overdue; what was missing was knowing why. The tick writes
`routine_alive_v1` (quiet, once a minute). A gap over 10 minutes between ticks (restart or
sleep) is the downtime window. A routine whose due times fall inside it (`core::missed_in`)
runs **once**, and the run's note says "Missed ×N since Tue 09:20; ran once to catch up."
A late routine behind another run, or an overdue one from before the downtime, is not
"missed". `Routine::catch_up: Option<bool>`: `None` means on for Messages and Digest, off
for the rest; off means the missed times are skipped, the next due time is set and a quiet
note says so. "Run now" always runs. Switch: Inbox, edit routine, "Catch up after downtime".

### Quota hold

When every runtime a run could use reports a usage limit (Claude via
`runtime_health::is_usage_limit`, Codex via `is_codex_usage_limit`; a failure of one
runtime for another reason is not a hold) the routine is parked: `RoutineState::hold`,
`next_due_ms` at the reset the error names (`limit_reset_ms`: Claude's `|<epoch>`, Codex's
`resets_in_seconds`, a clock time after "resets" or "try again at"; the earliest of the
runtimes wins), else +1 hour, at most a week. Credit or extra usage that must be topped up
(`is_resettable_limit` false) is not a window: it stays a failure with the usual notice. No failure is counted, no alert and no run is
added; the row says "Held until 14:00 (usage limit)" (`routine-status.ts`).

### Spill (`neko_core::spill`)

Where Neko itself feeds output back into a prompt, text over 8 KB is written to
`<data>/spill/<label>-<hash>.txt` (0600, same content one file) and the prompt gets the head
(2 KB), the tail (2 KB) and the path. Wired to: a goal check's output (`run_shell`, shown by
`continue_prompt`, the judge and the evidence gate), a routine's open items list, and the
crew `task_status` answer. `parking_tick` deletes spill files older than 7 days. Not applied
to an agent's own tool calls. Output between 1.2 KB and 8 KB keeps the old 1.2 KB tail.

### Spend (`neko_core::spend`, `routine_care::spend_held`)

Reuses what existed: tickets' `USAGE` events (the per-task budget in Settings, AI) and the
Usage page's per-task and per-day figures. New: a routine run records the cost Claude Code
reports (`RoutineRun::cost_micro_usd`), the `Routines` reply carries `spend_today`, the Usage
page shows "Today", and `Snapshot::daily_budget_cents` (`SetDailyBudget`, Settings, AI, off by
default) is a ceiling over the local day's tickets and routine runs together. At the ceiling a
running goal does not issue its next turn and no routine is picked (a "Run now" still goes),
and one Inbox item "Daily spend ceiling reached" is raised (its own row `neko_spend_notice_v1`,
once a day); raising the ceiling or midnight releases both and closes the item. Codex runs report
no price, so they are not counted.

### Not done

- Codex's usage-limit wording is a guess from its documented error; widen `is_codex_usage_limit` when a real one is seen.
- Routines on Codex report no cost; `cost_micro_usd` is Claude Code only.
- The curator never touches memory entries, only skills.
- The ceiling does not stop a ticket turn that is already running, nor a user's own chat.

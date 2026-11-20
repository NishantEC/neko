# One gate for every agent start (2026-11-21, hole H1)

`workbench::admission::may_start(db, Start) -> Admit | Hold(HoldReason)` is the one question every start of an agent asks
(`crates/neko-daemon/src/workbench/admission.rs`). Before it, watch wakes started runs with no machine steward and no
spend-ceiling check, and each other kind checked its own subset.

**Order of checks.** (1) the machine steward `holding`; (2) the steward `cap` against agents running now; (3) the daily spend
ceiling (`routine_care::spend_held`, which still raises its one Inbox item); (4) quiet hours, only for proactive starts
nobody asked for. Quiet hours are the phone's setting (`phone_settings_v1`, default 23:00 to 08:00).
A start the person asked for (Run now, a message to a cat) skips (4) only; it still obeys the steward and the ceiling.

**Kinds** (`StartKind`) and where each is asked:

| Kind | Site | Proactive |
| --- | --- | --- |
| Task | `Controller::tick_with` (before load pacing, so a hold does not spend burst tokens) | no |
| GoalTurn | `goals.rs` `running` and `held` through `hold_goal` | no |
| Judge | `goals.rs` `evaluate` through `hold_goal` | no |
| Routine | `routines.rs` `routine_held` | yes |
| WatchWake | `responsibilities.rs` `watch_may_wake`, before `claim_due` | yes |
| CatTurn | `cats.rs` `cat_plan` | yes, unless the person wrote |
| SkillCurator | `skill_curator.rs` `curator_tick`, `improve.rs` `tick` | yes |
| Triage, Naming | `triage.rs`, `display_names.rs` | no |
| SmallModel | memory learning, the chat summary refresh, `Controller::check_model` (the person's "check this model") | no |

**A held start is not a failure.** It stays pending and its own tick asks again. It leaves one `Held: <reason>` line where the
person looks, at most one per hold period: a task event (role `admission`), the goal's journal, a routine's `note`, a cat's
thread (an Event line), a watch's `last_result` (a `Held:` first line over what it last found; `claim_due` strips it so it is
never taken for a result). Sites with no item (curator, naming, memory learning, chat summaries) log one line to the daemon log
per hold period (`admit_background`). A watch is held before `claim_due`, so its due time and probe state are untouched and a
hold costs no writes after the first.

**Not behind the gate on purpose.**
- Runs inside a task worker the scheduler already admitted: builder, planner, reviewer, repository search, repair, publish
  conflict repair, runtime pick, reply classifier. They are the admitted `Task`.
- The Home chat. The person is waiting; it is counted in `running_agents`. H3's single launcher decides whether it is gated.
- Not agents: the Codex app-server mirror (legacy), the Spotlight `mdfind -live` helper.

**Kept as stricter local rules:** writable-per-repository cap, load pacing, `max_running_tickets`, per-task budget, the
curator and improve idle check (`idle_for_curator`, also reads the shared `machine-ok` WAIT), triage and naming skipping at
load 20+, routine `daily_cap` and `run_only_when`, the cat unattended debounce (20 s, 60 s apart, 40 a day).

**Running count.** `Controller::running_agents` is tickets + Home chat + cat turns (`TODO(H3)`: routine, watch, curator, judge
and small-model runs are not counted yet; H3 replaces it with the `RunningRuns` registry). Count it before taking the database
lock: the scheduler takes `active` then the database, and `cat_plan` used to take them the other way round. Sites with no
controller handle use `admission::running_now()`.

**Tests.** `admission.rs` loops `StartKind::ALL` for steward hold, cap, ceiling and quiet hours (proactive vs person-started),
the check order, the coalesced lines, and `no_agent_start_bypasses_may_start`: every file that builds a `RunSpec` or calls
`spawn_guarded` must be in `SITES` (gated here, within an admitted task, reached through a gated caller, or exempt with a
reason), each `Here` entry must name its `StartKind`, every kind must have a site, and nothing outside routine care reads the
spend ceiling itself. Adding a spawn path means adding it there. Site tests: scheduler (held over the ceiling, then starts),
goals (the journal line, no judge call), watch (held keeps its due time, starts once the hold clears), routine, cat, triage.
Tests that run at fixed instants turn quiet hours off in their fixture (`phone::save_settings`), or they would depend on the
time zone of whoever runs them.

## Goals that keep working (2026-11-14)

Owner: "people can use this as their super agent… something that will just
keep on working." The three research notes (`docs/research/t3code-and-zeron.md`,
`super-agent-landscape-2026-11.md`, `harness-sweep-2026-11-13.md`) agreed the
top gap was that Neko could not keep working toward a goal by itself and
prove it was done. Pieces came from Hermes (`goals.py` judge, WAIT,
`verification_stop`), Zeron (run journal, ten-minute stall watchdog) and
Hermes `quota_hold`.

### What a goal is

`neko_protocol::goals::Goal`: the person's **objective** (their words), an
optional **done_check** (a shell command run in the ticket's folder; passing
means done; "none" or empty means none), **constraints**, and a **budget**
(`max_turns` default 20, `max_wall_ms` default 12 h of *running* time, optional
`max_cost_cents` from the ticket's reported agent cost). Stored in
`neko_core::goals` as small rows, never the workbench snapshot:
`neko_goal_v1:<id>` (the record), `neko_goals_index_v1`, `neko_run_log_v1:<subject>`
(ledger and journal; the subject is the ticket id, else the goal id) and
`neko_goal_clock_v1` (wall time and last tick, **deferred**, since they change
on a timer). `load` attaches them as `Snapshot::goals` (cleared in `encode`
like skills and decision records). Three ways to make one:

- `/goal <text> [--check <command>] [--turns <n>]` in the chat (handled in
  `Controller::send_chat`, no model turn; bare `/goal` lists the open ones);
- a `neko-goal` fenced block in a reply (`ProposedGoal`, one a reply, rides the
  ticket the reply opened or the one named by `task_id`). The full-power chat
  instruction (`neko_chat::super_instruction`) says when: anything the person
  wants finished, not just started;
- `Command::Goals(GoalCommand::Create{…})` (web, `--request`), optionally with a
  `ticket_id`.

A goal always works through a ticket. A goal with no ticket gets one made in the
workspace (a Git repository is required, as for any ticket). The goal is the
person's own request for local work: `start_when_planned` is set, so it builds
without a plan approval, and nothing is published.

### The loop (`crates/neko-daemon/src/workbench/goals.rs`)

One turn is one run of the ticket, so admission, the machine steward, the
writable-per-repository cap, budgets, worktrees and review all apply unchanged.
The goal tick (3 s, its own thread) watches the ticket. When a turn ends
(ReadyForReview, Completed or Failed):

1. run `done_check` (`/bin/sh -c`, in the worktree, test-worker env, 10 minutes
   max) and file the result in the **evidence ledger**: command, exit code, the
   last 1.2 KB of output, time, who ran it;
2. ask the **judge**: `claude -p` on `haiku` through `native_runner::extract`
   (no tools), else Codex's smallest listed "mini" model, else its default.
   Prompts are `neko_core::goals::judge_prompt` (golden:
   `tests/prompts/goal-judge*.md`). The answer is JSON:
   DONE / CONTINUE / BLOCKED / WAIT, a reason, and `done` / `next` / `plan` lines;
3. `core::enforce` applies the rule the judge cannot override: **DONE needs
   evidence**. With a check, only a passing one (a judge DONE over a failing check
   becomes CONTINUE with "still fails"). With no check, `evidence` must be
   **verified** (`Env::verify`): a path inside the task's folder (no `..`, not
   absolute, not a link out, not `.`) that was created or changed since the goal
   began (a file that was already there proves nothing), a `owner/repo#12`
   reference that `gh pr view` finds, or a link that answers (a GitHub pull URL is
   checked as the pull request). BLOCKED without a question and WAIT with nothing to wait for become
   CONTINUE;
4. apply it:
   - **CONTINUE**: if a cap is reached, pause (below); otherwise
     `store::goal_turn` queues the ticket again with a note carrying the failing
     output and the judge's reason (`continue_prompt`), same session;
   - **BLOCKED**: one Inbox item with one question and a default answer; the goal
     waits, ticks cost nothing. Answer from the goal line or the Inbox
     (`GoalCommand::Answer`; empty text takes the default);
   - **WAIT**: parked until a time, a PR state (`gh pr view`, polled at most once a
     minute) or a pid exits, with no turn and no judge;
   - **DONE**: finished.
   - A judge error, or an answer that is not the JSON, is **CONTINUE** (fail
     open); the turn cap still bounds it.

`goal_turn` is deliberately not `ReplyToTask`: a reply goes through
`supervision::reply_intent`, a model call that could read a machine-written
message as a read-only question. A goal turn grants nothing the goal did not.

### Evidence-gated finish for code work

In `execute_with_worktree`, after the independent review passes and before
ReadyForReview, `goals::evidence_gate` files this run's receipts in the ledger
and calls `core::gate`. Only **real checks** are recorded (`core::real_check`: the
goal's own check, the ticket's approved checks, or a recognised test, type-check
or lint runner; never a read such as `cat src/test/x.rs`, a listing, a plain
build, or anything piped or chained). Only the **latest** result of each command
counts, and once the independent review ran only its results do (it ran after
the last edit, so a builder pass from before a later edit proves nothing). A ticket whose changed files include code
(`core::needs_check`; markdown, text, images, lockfiles and licences are prose)
needs a passing ledger entry. Missing: up to `GATE_EXTRA_TURNS` = 2 extra "run the
checks and fix" builder turns (`gate_instruction`, sent as part of the next
prompt), then the ticket fails with `Blocked: …` plus the failing output; a goal
on it turns that into one Inbox question. Split children skip the gate (their
parent integrates verified work).

### Keeping going through trouble

- **Run journal.** Every turn appends `{turn, plan, done, next, session_id, note}`
  (20 kept). `core::context` renders the goal, journal and ledger tail; it is
  added to the builder's prompt for any ticket with an active goal
  (`goals::prompt_context`), so a fresh session starts informed. The saved
  session id lives in `task_sessions`; a runtime that can resume it
  (`supports_resume`) does, else it starts fresh from the context.
- **Resume.** `Controller::start` runs `store::recover_interrupted` (tickets are
  requeued, at most twice) and then `goals::resume_sweep`: goals whose turn was in
  flight get a "Resumed after a restart" journal line naming the session and a
  fresh clock (the time the daemon was down is not spent). A ticket that failed
  after its restarts is judged like any failed turn and continued.
- **Stall watchdog.** Codex already ended a run after ten silent minutes with no
  command running (`StallGuard`). Claude Code now does too: `cli_workers` counts
  each tool in flight (`tool_use` to `tool_result`, subagents included), so a long
  build is never a stall. OpenCode reports a tool only when it ends, so it stays
  bounded by the wall-clock timeout only. The error contains "looked stalled";
  the goal resumes it **once** (not counted as a turn), then treats the next one
  as a failed turn.
- **Quota hold.** A failed turn whose error `runtime_health::is_usage_limit`
  recognises parks the goal as **Held** until `core::hold_until_ms`: the reset
  time in the error (`…usage limit reached|<unix seconds>` or `resets 3pm`, local)
  plus 30 s, else one hour. No check, judge or turn meanwhile; then the turn
  resumes (not counted). The Codex fallback is untouched.
- **Budget.** Turns, running time or reported cost: the goal pauses with its state
  and raises one Inbox item, "Extend by N turns?" (N = half the cap, at least 5),
  linked `goal:<id>`. Extend (from the goal line, the right-click menu or the Inbox
  button) adds the turns, moves a time or cost cap that stopped it, closes the
  item and runs on.
- **Machine.** `machine::latest().holding` (disk, memory, load, night and the
  shared `machine-ok` WAIT) postpones checks, judging and new turns; the
  ticket's own admission holds its run.

### What a goal may and may not do

- Only the person sets a goal that runs a check: `/goal`, the app, or Neko's own
  `--request`. A `neko-goal` block in a reply is only honoured in full-power chat
  (the read-only prompt does not offer it) and the goal it makes is **paused**
  ("Neko suggested this goal and its check. Read the check, then Resume"): its
  shell check is never run, and it asks for no plan skip, until the person resumes.
- A goal never widens a ticket. `store::goal_turn` keeps the ticket's assessment
  (so the scope check still applies), its read-only state and approval state;
  a read-only ticket cannot carry a goal or take a goal turn.
- A ticket's dependency folders are not pruned, and its folder is not parked,
  while an unfinished goal works it (the check runs there; a parked folder is
  restored first).
- `create` refuses a second goal on a ticket before it changes anything.
- Snapshot readers get no clocks (`wall_ms` / `last_tick_ms` are zero there):
  running time moves on a timer, in its own deferred row, written only by running
  goals every 15 s.

### Inbox

Inbox items belong to routines. A goal's items live on a hidden, never-running
routine `goals:<workspace_id>` (`routines::raise_goal_item`), so the phone push
and the age-out already apply. The web Inbox hides that routine and shows the
Extend / Stop / Resume buttons and the answer box on items whose link starts
`goal:`.

### UI (DESIGN.md §8) — `web/src/work/goals.tsx`

One line per goal: status glyph, objective, "turn 4/20 · check failing (exit 101)".
It opens to the evidence ledger and the journal. Pause, Resume, Extend, Mark done
and Stop are row actions on hover and focus, and each is in the right-click menu.
The line is above the conversation on a task (sheet and full page) and in Home's
details; goals set in Home's chat show under the crew line, which counts active
goals ("2 goals"; paused ones do not count). `scripts/verify/goals.mjs` drives it
against the harness's three seeded goals.

### Not done

- Split parents skip the evidence gate (their children are gated and integrated);
  a ticket that ends "nothing to change" has no changed code to check.
- A goal with no workspace Git repository cannot make a ticket; there is no
  ticketless goal that runs on its own.
- The judge is asked once per ended turn; there is no per-turn plan file the agent
  rewrites (the journal is written by Neko from the judge's lines).
- No native (Swift) view of goals: the snapshot field is ignored there.
- OpenCode has no quiet-stall check.
- Cost caps count only runtimes that report a price (Claude Code, OpenCode).
- Goal checks run on the host with the person's own shell environment, like the
  review's checks; a hostile `done_check` is the person's own command.

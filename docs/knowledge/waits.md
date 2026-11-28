# Timers and watches (2026-11-28)

A thread's agent can rest until something happens instead of waiting inside its turn: "I'll check again at 18:00", "wake me when
`build/deploy.log` changes", "wake me when `gh pr checks 12` passes". It sets a **timer** or a **watch** with Neko's own tools, ends
its turn and rests as Waiting with no process; Neko wakes it with `--resume` when the time comes or the thing happens. Protocol 25,
store 20.

Code: `neko-protocol/src/waits.rs` (the shapes, the limits, path patterns), `neko-core/src/waits.rs` (rows, reading times and paths,
what is due, the words the agent reads), `neko-daemon/src/waits.rs` (the tools, the scheduler's pass, checks, Stop watching),
`neko-daemon/src/fsevents.rs` (files), the app's `waits.rs` (words and lines).

## The tools

Offered to a thread's run (not a kitten's: a kitten's report is its end, and a kitten waiting on a clock would hold its parent with no
report). Each answers at once and tells the agent to end its turn.

| Tool | In | Does |
| --- | --- | --- |
| `wake_me` | `at` (`18:00`: the next time the Mac's clock reads it; `2026-11-29 18:00`, local; with `Z` or `+05:30`, that zone) or `after` (`90s`, `45m`, `2h`, `1h30m`, `3d`; a bare number is refused, so no unit is guessed), `note` | A timer, a minute to 7 days ahead. |
| `watch_files` | `paths` (files, folders or patterns with `*`, `?` and `**`, relative to the thread's folder; an absolute path inside it is made relative; anything outside is refused), `until?`, `note` | Wakes the thread when one of them changes. A folder's name watches what is in it. |
| `watch_command` | `command`, `every?` (1 minute to 1 day; default 5 minutes), `until?` (default 1 day), `note` | Runs the check in the thread's folder and wakes the thread the first time it exits 0. |
| `stop_watching` | `id` | Stops one of the thread's own timers or watches. |

**Limits:** a timer at most 7 days ahead and at least a minute; a watch lasts at most 7 days (it then ends with a wake that says so); 4
timers and 4 watches per thread, 32 watches in all. Each refusal says why in words the agent reads. Why 7 days for a watch and not a
month: a local Mac sleeps and shuts down, and a watch nobody remembers should end by itself.

**What the agent is told:** one paragraph after the subagents paragraph (`waits::PARAGRAPH`: do not wait inside a turn, set a timer or a
watch, end the turn, tell the person in a line what you wait for), the tools' descriptions, and each answer ("Timer wait_… is set: Neko
wakes you at 18:00 … end your turn now").

**Asking first:** `watch_command` runs a command, unattended, again and again. Under "Ask before risky steps" Neko's policy reads it as a
command (`Action::Watch`): the person sees "Run a check again and again" with the exact command in Needs you, and "Always" is scoped to
the program (`watch:gh`), apart from Bash's. Under "Follow Claude Code's own rules" Neko's own tools are allowed up front
(`--allowedTools mcp__neko__*`), so a real run sets it without asking, as it would run the same command with Bash under the person's
rules. The fake CLI asks for every MCP tool, so in tests Follow asks too. Full access goes ahead.

## Resting and waking

- The thread's run ends; a thread with a timer or watch still waiting becomes **Waiting** (`subagents::ended`, through the same check as
  kittens and crew). Its row reads "Waiting until 18:00", "Waiting on a file change", "Waiting on a check", or "Waiting until 18:00 or a
  file change"; the run's line and the conversation's last line say the same.
- **On the scheduler's thread** (`neko-rounds`, `rounds.md`): each pass looks at what is due on the scheduler's clock (the debug clock
  moves it in tests): a timer whose time came, a watch whose `until` passed, a command watch whose next check is due. It sleeps until the
  soonest, never more than 5 minutes at a stretch, as for rounds; with no timer or watch it asks for no timer.
- **A wake is a row in `wakes`** (`kind` `timer` or `watch`), written in the same transaction that settles a timer. The subagent hub
  delivers it as it delivers a kitten's report: at rest, a resumed run after the 2-second batch, with one `<neko-wake kind="timer"
  id="wait_…" at="18:00">` (its `<note>`); while the thread works, steered in. One wake run at a time per thread; at most 8 runs across
  Neko; held while the account's limit is reached or the Mac is too warm.
- **A watch's wake:** `<neko-wake kind="watch" status="changed" paths="…">` with `<changed>` (the files), and the line that the watch goes
  on; `status="succeeded" command="…"` with the check's last `<output>` (the watch is over); `status="ended"` when its time ran out.
- **The note:** a centred line before the run that read it, "Woken by its timer · 18:00", "Woken by a change to build.log", "Woken: its
  check passed", "The watch on src/** ended after its time", with the agent's own note (or the check's words) folded under it
  (`PartBody::EventWake`).

### Files

FSEvents, one stream per files watch on its folder (`fsevents.rs`), delivered on one serial queue. The callback only notes which paths
matched and unparks the scheduler; the scheduler decides. Changes heard while the thread's own run goes, or within 3 seconds after it
ended (FSEvents tells them about a second late), are the agent's own and wake nothing. A files watch wakes its thread at most once a
minute; changes in between go in the next wake. Streams come back at a restart. The thread's folder (a worktree or scratch folder) is
kept while a timer or watch waits on it: the tidy rule skips it.

Why our own binding: the usual crate for this (`notify`) is CC0-1.0, and Neko takes MIT, Apache-2.0 or OFL only. FSEvents needs a few
CoreServices calls; there is no new dependency.

### Checks

A command watch's check runs as `/bin/sh -c` in the thread's folder, in a process group of its own, stdin closed, one at a time per watch.
It is given a minute; past that its whole group is ended and it counts as "not yet". Its exit code and time are kept (`checked_ms`,
`last_exit`: the line says "not yet"). Exit 0 fires the watch with the end of what it printed. At the daemon's stop every running check's
group is ended (`run-guard.md`).

## Stopping, pausing, archiving

- **Stop watching** (the × on each line, `stop_waiting {task, wait}`) ends one. A thread that waited on nothing else is at rest (Done).
- **Stop** on the thread ends every timer and watch with its run and kittens.
- **Archive** a thread that rests only on its timers and watches: they stop and it is archived. (One still waiting on kittens or crew is
  refused, as before.) Removing a thread removes them.
- **Pause automatic work** (Rounds, and the same switch in Settings ▸ Rounds & wakes, which also says the limits above; the setting that was "Pause all rounds", `settings.rounds_paused`) holds rounds, timers and
  watches: a timer that comes due is **skipped** (`state = skipped`, `end_reason`, the event `wait_skipped`) and the thread says so, in
  a quiet notice: "Not woken at 18:00: automatic work was paused." with what the timer was for; a watch that ends meanwhile ends without
  a wake; files changes wake nothing and checks do not run. Nothing is caught up after a resume.

## In the app

- The sidebar row and the conversation's last line: "Waiting until 18:00" (the still ring).
- **The activity bar:** a thread that rests on its timers and watches alone shows them as quiet lines above the composer, one each: a clock,
  an eye or a terminal mark, "Until 18:00" / "Watching build/deploy.log" / "Checking curl -fsI …", the note or how often, and its ×
  ("Stop the timer", "Stop watching"). Beside kittens or a long command they join the bar's summary ("1 kitten working · until 18:00")
  and its open list.
- **Overview ▸ Waiting on:** the same lines, each with its ×.
- The composer says "The agent is waiting until 18:00. Say something to wake it now".
- A standing round's message: see `rounds.md`.

## Tests

- `neko-protocol/src/waits.rs` (patterns), `neko-core/src/waits.rs` (lengths of time, times, limits, paths, what is due, words),
  `neko-daemon/src/fsevents.rs` (a change heard, a stopped stream hears nothing).
- `crates/neko-daemon/tests/waits.rs`, the real daemon and the fake CLI through Neko's real MCP hop: a timer rests the thread and wakes it
  once with `--resume` when the test clock moves (and not an hour early); a watched file wakes it and Stop watching ends the watch; a
  command watch wakes it when its check exits 0; it asks first under Ask; a timer due while paused starts nothing, is stored skipped and
  the thread says so; a standing round posts into its Cattain's own thread and a round of nobody's starts a thread with no cat; the
  limits, and Stop ending them.
- The app: `apps/neko/tests/waits.rs` and the scenes `waits-timer`, `waits-timer-overview`, `waits-watch`, `waits-woken`, `waits-paused`,
  `round-standing`, `round-new-standing`.
- The rig: `timer-wake`, `watch-file`, `pause-automatic`, `round-standing` (`qa.md`).

## Not here yet

- GitHub pull request and CI watches through `gh`.
- Timers and watches for kittens.
- Waking the Mac for a timer, or starting Neko at login: a timer that came due while the Mac slept fires once when Neko next looks
  (within 5 minutes of a wake).

## Still not sure: check with the real CLI

1. That a real agent calls `mcp__neko__wake_me` when told to come back later, ends its turn, and reads the `<neko-wake kind="timer">`
   as its own timer when resumed.
2. That the real CLI lists the four tools (`init`) and allows `watch_command` without asking under the person's default mode, while
   Neko's hook asks for it under "Ask before risky steps".

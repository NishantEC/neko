# Rounds (2026-11-27)

A round is a cat's standing work that starts by itself: "every weekday at 09:00, mochi checks the docs for broken links in
acme-api". The daemon keeps the rounds and runs them; the app lists and changes them. Protocol 9, store 7.

Code: `neko-protocol/src/schedule.rs` (the arithmetic, shared with the app so the panel can say when the first run will be),
`neko-protocol/src/rounds.rs` (the shapes), `neko-core/src/rounds.rs` (rows and logic, no clock), `neko-daemon/src/rounds.rs`
(the thread and the requests), the app's `rounds.rs` (words and form) and `rounds_view.rs` (the page and the panel).

## The model

A round has an id (`round_…`), a name (one line, 1 to 60 characters; the title of every task it makes), the words (what to do),
a cat, a workspace (General when none), a schedule, an on/off switch, and created and changed times. The daemon sends each round
with its **status**, its **next due time**, a **note** (a line about the round itself) and its **last** history line. At most 64
rounds.

Status says whether it will run by itself, and if not why: `scheduled`, `manual`, `off`, `braked` (turned itself off), `paused`
(every round is), `cat_retired`, `folder_missing`, `workspace_removed`. The first that applies wins, in that order from `off`.

## Schedules

| Kind | Meaning |
| --- | --- |
| `daily {at}` | Every day at HH:MM (24-hour). |
| `weekdays {days, at}` | On chosen days (mon … sun) at HH:MM. |
| `every {hours}` | Every N hours, 1 to 24, of real time, counted from an **anchor**: when the round was made, turned on, or its schedule changed. The first run is N hours after the anchor. |
| `manual` | Never by itself: only Run now. |

Why these and not cron: a person picks from four plain choices; no expression to get wrong. Why "every N hours" counts from the
anchor (as Calendar's and Reminders' repeats count from their start): "every 6 hours" has no natural wall-clock grid, and an
anchor makes the first run predictable ("in 6 hours").

### Time zones and daylight saving

Times are the Mac's local time (`TZ`, else the system zone, read through `localtime_r` for each instant). The zone is handed in
(`schedule::Zone`), so tests give a fixed offset, `Rules::us` or `Rules::eu`, or the system zone; `tests/system_zone.rs` checks
the system zone against real tzdata for New York and Kolkata.

- A wall-clock time that **does not exist** that day (02:30 when the clocks jump from 02:00 to 03:00) runs **once, shifted forward
  by the length of the gap** (03:30 by the new clock), as Calendar does. It is not skipped and not run twice.
- A wall-clock time that **happens twice** (01:30 when the clocks go back from 02:00 to 01:00) runs **once, at its first
  occurrence**.
- So a daily round runs exactly once on every date it is due. "Every N hours" is elapsed time and is not moved by a change.
- Month and year ends, leap days and weekdays are plain civil-date arithmetic (tested across 2026-11-30, 2027-01-01, 2028-02-29).

## What a run is

Each due time makes an **ordinary task**: the round's words, titled with the round's name, for the round's cat in the round's
workspace, marked as the round's (`task.round {id, name, due_ms}`; `due_ms` is None for Run now). Everything a task has applies
unchanged: the cat's standing rules, the permission mode in Settings, the run guard's limits, checkpoints and Undo, Needs you and
its notification. The task starts through the same path as a task the person makes (`runs::start`).

### What an unattended run may and may not do

**It never gets more permission than an attended one.** Nothing in the round sets, changes or reads a permission mode, a hook or a
flag. In "Ask before risky steps" a step that would ask waits in Needs you (with its notification) until the person answers; in
"Follow Claude Code's rules" the person's own rules decide, as in the terminal. A test (`daemon/tests/rounds.rs`) makes a round
due in ask mode and compares its request with the same step in a task made by hand: same tool, title and kind of request, still
pending a moment later. A round cannot change settings, make or retire cats, or add workspaces: it only starts tasks. The words of
a round are the person's words, written by them.

## The scheduler

One thread, `neko-rounds`. It deals with every due time up to now, then sleeps.

- **No timer when no round is on.** With no round that is on and automatic (a paused round still counts: its time passes as a
  skipped line) the thread parks with no timeout. Measured as `tests/measure.rs` measures the folder watcher
  (`idle_with_rounds_on_and_off`): 2 ms of CPU in 30 seconds with no round, and the same with one round on (due in a day) and one off.
- **Otherwise it sleeps until the next due time**, but never more than 5 minutes at a stretch. The Mac's monotonic clock stands
  still while it sleeps, so a timer set for the due time would fire late by the length of the sleep; the cap bounds how long after
  a wake a missed time is noticed. Each wake reads the wall clock.
- **Noticing a wake or a clock change:** the wall clock and the monotonic clock are read together each time the thread goes to
  sleep and wakes; a wall clock that moved on more than 30 seconds beyond the monotonic one is a jump (a sleep, or a clock set
  forward). It is logged and the rounds are looked at at once. (Method: a jump in wall-clock time, `rounds::jumped`.)
- A change to a round, a pause, a round's task ending and a stop all unpark it at once.
- **The clock is handed in.** The core takes `Env {now, zone}` on every call; the daemon's `Hub::now()` supplies it.

### Missed times

Per round a **cursor** holds the last due time dealt with. Every due time after it up to now is dealt with once. When several are
due at once (the daemon was off, the Mac slept) the round runs **once, for the latest**, if that is at most 24 hours old
(`CATCH_UP_MS`; exactly 24 hours runs, a millisecond more does not); the earlier times are one history line, "Missed 3 earlier
times while Neko was not running or this Mac was asleep. It ran once, for the latest." A latest time older than 24 hours is
recorded as missed (one line) and nothing runs. Turning a round on, changing its schedule and pausing never cause a catch-up: they
move the cursor to now (pausing: each time that passes is a skipped line).

### A restart in the middle

The task, the history line and the cursor are written in **one transaction**, so a due time is neither run twice nor lost across a
crash. A task made but whose agent never started (the daemon went down between the commit and the start) is found at the next
start and started.

### No overlap

If the round's previous task is still **working or waiting for the person** (`new`, `working`, `needs_you`, `asked`) the new time
is skipped and recorded: "The last run is still working." or "The last run is waiting for you."

## Brakes

- **Three failed runs in a row** (task status `failed`; a success or a question resets the count; a stopped or interrupted run
  neither adds nor resets) turn the round off. A notice in its last task ("This round turned itself off after 3 failed runs in a
  row.") and the round's note say so. Turning it on clears the brake and counts from then.
- **A missing folder, a removed workspace, a retired cat:** the time is skipped with the reason ("The folder of acme-api was not
  found.", "mochi is retired.", "acme-api was removed from Neko."). The round says so (`status`, `note`).
- **Pause all** (`set_rounds_paused`): one switch in the store (`settings.rounds_paused`); it survives a restart. While paused each
  time that passes is a skipped line ("Every round was paused."); nothing is caught up after.
- **Run now** works whatever the switch, the pause or the schedule say, and is refused (`busy`) while the last task is unfinished
  and (`refused`) for a retired cat or a missing folder.
- **Removing a round** removes its history and never its tasks; they keep their `round` mark and name.

## History

Per round the last 50 lines (`round_runs`), newest first: due time (None for Run now), when the daemon dealt with it, `started` /
`skipped` / `missed`, the task, the reason, and how the task ended (`done`, `asked`, `failed`, `stopped`, `interrupted`,
`removed`; None while it works). The end is written when the task's status settles (`settle_rounds`).

## The app

The Rounds page (⌘3, View ▸ Rounds, the sidebar row under Needs you): a card per round led by its cat's portrait: name, "mochi ·
Weekdays at 09:00", "Next: tomorrow 09:00" or why not ("Paused", "Off", "mochi is retired", "Folder missing"), "Last: Done · today
at 09:00" with Open task, a switch, Run now and a menu (Edit…, Remove…), and a History fold. Pause all rounds / Resume rounds, and a
paused band. The empty state explains rounds and offers New round…. New round… and Edit are one panel: name, what to do, cat and
workspace chips (as the composer's), the schedule kind, the days and time or the hours, and one line saying when the first run will
be before saving. A cat's page lists its rounds with "New round for <cat>…". A task a round made says so in a strip above its
conversation ("From the round Docs check, due 09:00", with Open round); its sidebar row has a small round mark. Home has one quiet
line ("Next round: miso, Release notes, today 18:35") and nothing when there is none.

## The test clock

`DebugClock {offset_ms}` moves **only the scheduler's clock** forward. It exists for the QA rig (`rounds-due`) and for the
daemon's tests. It works only in a **debug build** (`#[cfg(debug_assertions)]`) started with `NEKO_DEBUG_CLOCK=1`. In a release
build the offset field is not compiled in and the request is refused (`refused`); a debug daemon without the variable refuses it
too (tested). It cannot change the event log's times, task times or ids.

## Not here yet

- **The Briefing:** a morning summary of what the rounds did overnight. Rounds leave tasks, history and Needs you items; nothing
  gathers them yet.
- **Running while the Mac sleeps:** rounds run while Neko (its daemon) is running and the Mac is awake. A time that passes
  meanwhile runs once on wake if it is at most a day old (see above). Waking the Mac for a round (launchd, `pmset`) is not done.
- **Starting at login:** the daemon starts with the app. Nothing starts it at login, so a round does not run if Neko was never
  opened that day.
- A schedule of "first Monday of the month", of seconds, or in another time zone than the Mac's; per-round permission modes (a
  round has the same as any task, on purpose).

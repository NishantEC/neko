## Schedules: the agents that start themselves

L4 of `docs/plan-agent-control-plane.md`. Paseo can start an agent on a cron
cadence, and a schedule is exactly the thing you set up once and then cannot
remember the state of. The `Schedules` command opens a mode listing them —
the third command/mode pair, costing what `modes.rs` has always said one
costs: a `CommandSpec`, a `ModeChrome`, and a `Provider` in
`AppState::mode_providers`.

**Enter pauses; it does not run.** The tempting primary action is "Run now"
and it is the wrong one — Enter is what a finger presses on the way past a
list, and running a schedule starts a real agent doing real work in a real
repository. Pausing is reversible, it is why you came, and pressing Enter
again undoes it. `Run now` stays in `⌘K` behind a deliberate second
keystroke, beside a `destructive` Delete.

Two details that are the difference between a useful row and a lying one:

- **The verb is per-row.** `"Pause ↵"` on an active schedule, `"Resume ↵"` on
  a paused one. A label reading "Pause" on something already paused is the
  one thing a person could not recover from misreading.
- **A paused schedule never shows a next-run time.** It keeps the `nextRunAt`
  it had when it was paused, and printing it would name a moment at which
  nothing will happen. "Paused" takes that half of the line; the cron
  expression keeps the other half, because it is still worth seeing.

`activate` **re-reads the live state instead of trusting the row** — the list
in front of you can be seconds old, and toggling from live state is always
what was meant, where resuming something already running is a confusing
no-op. `paused` is believed from `status` *or* `pausedAt`: the first is the
field that means it, the second is the one that proves it, and a daemon that
grows a status neko does not recognise still reports the timestamp.

**The whole shape came from a live probe, not from Paseo's source**: a real
schedule was created, listed, paused, resumed, logged, and deleted, and the
JSON it produced is what `parse_schedules` is written against and what the
fixture in its tests is copied from. One gotcha worth keeping — `create_schedule`
rejects a cron whose next occurrence it cannot compute (`0 4 29 2 *` fails on
a non-leap year), so a "far-future, harmless" probe cadence has to be a date
that really exists.

Verified end to end over the real wire: `"schedules"` in the root list
reaches the command with `enters_mode=schedule`; the mode row read
`in 129d · 0 4 1 1 *` with an `ACTIVE` badge; Enter flipped it to `PAUSED`
with `Resume ↵` and `Paused · 0 4 1 1 *`; Enter again restored it; Delete
removed it, leaving nothing behind.

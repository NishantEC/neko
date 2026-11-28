# The composer while the agent works (2026-11-27)

Words typed while a run goes can reach the agent three ways, and what has not yet gone waits above the composer. Protocol 16, store 12.
The app's side is `apps/neko/src/composer.rs`; the daemon's is `crates/neko-daemon/src/queue.rs` and `crates/neko-core/src/queue.rs`.
The scenarios are `steer`, `queue` and `interrupt` (`qa.md`).

## The three modes

| Key (while a run goes) | Mode | What the daemon does | What reaches the CLI |
| --- | --- | --- | --- |
| Return | **Steer** | Writes the words into the running agent's open input and records them in the transcript at once. A row waits above the composer until a later model call reads them. | One more `user` line on the same process's stdin. No run starts. |
| ⌥Return | **Queue** | Keeps the words in the thread's list. When the run ends well they start the next run, one message per run, in order. | Nothing now; later a new run (`--resume`) whose first `user` line is the words. |
| ⌘Return | **Interrupt** | Marks the run as interrupted, sends the CLI's interrupt, and when the turn is over starts a new run with the words at once. | A `control_request` `{"subtype":"interrupt"}` on stdin; when the turn has ended, a new process (`--resume`) with the words. |

At rest (no run going, or a thread that only waits on its kittens) every mode just sends: a new run resumes the conversation with the
words. ⌘Return also sends at rest; ⌥Return at rest stays a new line, as it is in every text field, so it never sends by surprise.

Settings ▸ Agents ▸ "While the agent works" swaps the first two (Return queues, ⌥Return steers). Interrupt is always ⌘Return. The
setting is `Settings.queue_first`; only the screen reads it (it chooses the mode it sends), the daemon never does. With no mode named,
`follow_up` steers, as it always did.

The Send button is Stop while the box is empty and Send once there are words. A chip beside it (`send-mode`) names what Return does
and opens a menu with the three ways and their keys; choosing one sends in that mode. Under the box a quiet line says what each key
does ("Return steers · ⌥Return queues · ⌘Return interrupts") while words are typed. The placeholder says "Steer the agent (⌥↩ to queue)".
Escape stops the agent whatever is typed, unless a request waits (then it denies, as before).

## What waits above the composer

One block with a hairline between rows, each one line, in the order the messages were sent:

- **Steered** ("Steered · waiting to be read"): in the agent already. No actions: it cannot be taken back.
- **Queued**: Send now (steers it into the running agent, or starts a run when none goes), Edit (the message leaves the list and its
  words come back into the composer, in front of what is there) and Discard (×).
- **Interrupting**: the turn is being stopped; no actions. A second ⌘Return while one is going waits as a Queued message.
- More than two queued fold behind one line, "3 queued", which opens on demand. (The spec says "two or more"; two rows are small
  enough to read, so the fold starts at three.)

The list is the daemon's (`queued` table, `Transcript.queued`, `queue_changed {task, queued}` with the whole list). It is stored, so
what was queued survives a restart. After a restart nothing is sent by itself: an interrupted run starts nothing, and an interrupt that
never went through becomes a queued message; steered messages were written (the transcript has them) and are dropped from the list.

↑ in an empty composer brings back the newest queued message to edit, else the last message sent; ↓ gives the (empty) draft back while
the words are unchanged.

## When a queued message is sent

When a run ends (`queue::after_run`, before anything else is decided about the thread):

- It ended **done**, or with a question, or **interrupted** by the person: the interrupting message first, else the oldest queued one
  starts a run (`runs::start`). The rest stay for the next end.
- It ended **stopped** or **failed**: nothing is sent. The list is held for the person (Send now, Edit, Discard still work). An
  interrupt that was cut short by Stop becomes a queued message. Nothing the person did not ask for runs after a Stop.
- Steered rows are removed: the run is over, so whatever it read it read.

A run that starts from the list skips `subagents::ended` for the run that just ended: the thread is not resting, and a kitten's
report is made when its last run ends.

## Words sent while a run is ending

A run's last `result` closes the CLI's input, and the run stays active until the process has exited (a second or so on the real CLI;
`runtimes.md`, "When a run ends"). The screen still shows the run as working then, so Return steers. **Words sent in that moment are
never refused:** whatever the mode (Steer, Queue or Interrupt), the daemon holds them as a queued message (`queue::for_next_run`), the
row shows above the composer, and the run that is ending starts the next one with them as soon as it is released (`after_run`, above).
A steer whose write fails because the input closed meanwhile is held the same way. If the run was released before it looked at the
list, `for_next_run` starts the run itself; the same goes for ⌥Return sent as a run is released. A CLI that does not exit is ended
through the run guard ten seconds after its result (the run is done), so the words never wait longer than that. If the run that
was ending turns out to have failed, the message is held for the person like any queued message (Send now starts it).

## Where steered words stand in the conversation

The words are stored when they are sent, but the agent reads them at its next step, so whatever it said in the rest of the step it was
taking stood before them and its answer to them seemed to come first. Now, when the next model call begins (`step_began`, the same moment
the "Steered" row goes), the daemon moves the person's part to the end of the transcript (`Store::move_part_to_end`: a new `seq` and
time, the event `part_moved`) and every screen hears `part_changed`; the app's book re-sorts a part whose `seq` changed.

The app draws them inside their run, at that point: the run's turn is cut there into pieces, the person's well between them. While it
works, the live steps and the live line follow the words. Once it ends, the run keeps its **one** "Worked for" fold, in the first piece;
words read during the folded work stand after the fold, before the answer they led to; words read inside the answer cut the answer there.
Steered words that came before the run said anything stand before its turn. The scenes `steered-done` and `steered-working`,
`apps/neko/tests/waits.rs` and `crates/neko-daemon/tests/composer.rs` check it.

## Interrupt, in detail

`queue::interrupt`: the first interrupt of a run sets `Active.interrupting`, adds an Interrupting row and calls `Control::interrupt`.
The Claude Code runtime writes the interrupt control request (`claude/mod.rs`) and treats the turn's `result` as the end whatever
message is still unconsumed (it closes stdin at once, so the process ends; the result's error text is not a failure). A CLI that does not end the turn within five seconds
(`NEKO_INTERRUPT_GRACE_MS` shortens it) is stopped through the run guard by a small watcher thread; the conversation is resumed all
the same, because `--resume` needs only the session id. The run ends **Interrupted**: the fold reads "Interrupted after 41s", the run's
reason says "You interrupted it with a new message", and the new run starts with the words. A Stop pressed meanwhile wins: the run is
Stopped and the words wait as a queued message.

## How a steered message is known to be read

With `--replay-user-messages` the CLI does echo each message when it consumes it, but the row does not use that yet: the echo is
counted only to end the run (`runtimes.md`). For the row, Neko watches model calls: `StepBegan` (a top-level `message_start`) and `StepEnded`
(`message_stop`). A steer written while a call goes cannot be in it, so it waits for the next call to begin; one written between calls
is read by the next. That is a good guess, not a proof: the CLI's `message_start` arrives a moment after the call began, so a steer
written in that moment can be marked read one call early. The row only says "waiting to be read"; the words are in the transcript
either way, and the row goes when the run ends.

## What the activity bar is

One thin line (32 pt) above the composer, shown only while something besides the run itself goes: a kitten of this thread that works
or waits its turn, or a command that has run for more than five seconds (a Bash step of the running run). "2 kittens working · 1
command running" ("1 kitten working, 1 waiting its turn": a queued kitten is not called working), with a caret; it opens to one line each, with × on the agent (the same as Stop) and on each kitten (`stop_task` on
the kitten: it stops with its own kittens). A command has no × of its own: Neko cannot stop one step, only the run. Pressing a kitten
opens its tab in the right pane. The pane's Overview keeps the detail (titles, states, the tree); the bar is the quick way to see and
to stop, and is not shown for a run that only works, since the conversation already says so.

Not seen by Neko: a command the agent started in the background and left (`&`, a background shell). The CLI does not report it, and
the run guard ends it with the run.

**Timers and watches** (`waits.md`) list here too. A thread that rests on them alone shows them as quiet lines, one each, with nothing to
fold ("Until 18:00 · its note", "Watching build/deploy.log", "Checking gh pr checks 12 · every 5 minutes"), each with its × (Stop the
timer, Stop watching). Beside kittens or a long command they join the summary ("1 kitten working · until 18:00") and its open list.

## Writing to a kitten (`message_subagent`)

The agent's `message_subagent {id, text, mode?}` (`subagents.md`, "Questions go up") goes through the same `queue::send` with the
kitten's task: a kitten at rest (it asked, or it ended) starts again with the words (a resumed run, its conversation kept); one that
works has them steered in, queued for its next turn or sent as an interrupt, exactly as the person's words reach a thread. The
person still cannot write to a kitten (`follow_up` on one is refused): what they want changed goes to the thread.

## Not here yet

- Send now as an interrupt (it steers), and an `interrupt` of a kitten from the activity bar (× stops).

## Still not sure: check with the real CLI

1. That the `interrupt` control request ends the turn and the process stays resumable (the fake models it: an error result, the
   process then ends when stdin closes). If the real CLI answers differently the watcher stops it after five seconds and the words
   still run, so the worst case is a five second wait.
2. ~~Whether a steered `user` line gets a `result` of its own.~~ **Settled on Claude Code 2.1.331: often it does not.** The CLI folds
   a steer into the turn already running, reads it at its next step and ends with ONE `result` for both. Counting results (the
   first S5 build closed stdin after one `result` per message) left such a run at "Working" until the five-minute silence limit
   failed it, although the steer had been obeyed. Neko now passes `--replay-user-messages` and counts the messages the CLI
   *consumed*: at a `result` the run ends when every message sent has been consumed (or the turn was interrupted); a message still
   unconsumed gets a turn of its own, so stdin stays open for that turn's `result`. Details and the probe: `runtimes.md`, "When a run
   ends". The fake folds by default and has `work:SECS:separate` for a CLI that does not.
3. That a `message_start` follows a step the CLI took after reading a steered message (the "read" guess above).

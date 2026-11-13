## Neko reaches your phone (2026-11-13)

Goal: when something needs the owner and they are away from the Mac, their
phone says so. Works for anyone, with no account by default. Both channels
are off until the person turns them on (Settings → Phone).

**What is pushed.** Only needs-you items that are new (a seq above the
daemon's cursor), and the Claude-outage notice (it is a needs-you item). The
Inbox dedupe refreshes an open item and keeps its `seq`, so a refresh never
pushes again. Turning a channel on never pushes what was already open (the
cursor is set at first look, whether or not a channel is on).

**When** (`neko_core::phone::decide`, tested):
- Skipped when the keyboard or mouse was used in the last 3 minutes
  (`HIDIdleTime` from `ioreg -c IOHIDSystem`, read by the daemon only when
  something is waiting); the macOS notification already reached them. Unknown
  idle counts as away.
- Quiet hours 23:00-08:00 local by default (setting; equal hours = none).
  Held items are sent when quiet hours end if still open, dropped after 24 h.
  An item the routine marks `urgent` (optional reply field, serde default,
  one sentence in the routine prompt) ignores quiet hours, never idle.
- At most 6 pushes per rolling hour, then one summary push ("N more things
  need you") per hour; the rest stay in the Inbox. No retry loops.

**Channel A, Apple Reminders** (default). The Swift app owns EventKit
(`native/NekoKit/Sources/NekoNative/PhoneBridge.swift`, same shape as
`CalendarBridge`), the daemon decides (`crates/neko-daemon/src/phone.rs`).
The app sends `PhoneRemindersSync` every 20 s and 2 s after a change (access,
ids in the list, completed ids) and gets a `PhonePlan` (create / delete).
Access is requested only when the plan says the channel is on
(`requestFullAccessToReminders`). It makes a "Neko" list on iCloud when there
is one (else the default, else local), each reminder with the item's detail
and link in the notes, a due time and alarm 5 s ahead (so iCloud notifies the
phone), and a last notes line `neko:<seq>` that finds it again after a lost
report. It reports what it did with `PhoneRemindersDone`. Item-to-reminder
mapping lives in `phone_state_v1` (not the snapshot). What completing
the reminder does is under "Answering from the phone" below; closing the
item in Neko in any way, or turning the channel off, deletes the reminder;
deleting the reminder yourself leaves the item open. A reminder the app has not made within an hour is dropped.
Limit: the mapping uses `calendarItemIdentifier`; if EventKit ever changes
one after a sync, that item just stops closing from the phone.
Each queued reminder carries a unique `key` that the app echoes back (summaries have no marker); a failed save is reported with an empty id and dropped. If the app ever lists nothing while reminders exist, their mappings are dropped (not fixed).
`NSRemindersFullAccessUsageDescription` and the older key are in
`scripts/Neko-Info.plist`. Denied or restricted shows one line in Settings.

**Answering from the phone (2026-11-14).** Every item that asks something
carries a **default**, and the reminder title says what completing it does
("Extend by 10 turns? ✓ = extend NK-14 by 10 turns"). Both ways of answering
end in `inbox_reply::apply` (`crates/neko-daemon/src/inbox_reply.rs`), the same
daemon path the Inbox's "Use the default" button takes (`AnswerNeedsYou`):

| Item | Default (complete the reminder) | Words (a `>` line) |
| --- | --- | --- |
| Goal BLOCKED (`goal:<id>`, `blocked.inbox_seq`) | `GoalCommand::Answer` with the goal's default answer | the same command, the words as the answer |
| "Extend by N turns?" (`goal.extend_seq`) | `GoalCommand::Extend` by half the cap again | a number of turns, 1 to 100 (anything else is refused) |
| "Raise today's ceiling to $X?" (the spend notice's seq) | `SetDailyBudget` to $X (half again, whole dollars) | an amount like `$20` |
| Item with `task:<id>` as its link | the item's default, queued for that task | the words, queued for that task |
| Any other routine item | the item's `default`, kept as the routine's `owner_replies` | the words, kept the same way |

A task message goes through the durable command queue (`CrewCommand::Enqueue`,
`after_turn`, kind `answer`); a routine reply goes to a small row
(`neko_owner_replies_v1:<routine>`, 10 newest, 7 days) that the routine's next
prompt shows as `memory.owner_replies` (never saved back into the routine's
own memory) and the contract tells it to act on first. The item then closes as
"Answered on your phone" (`inbox::ANSWERED_ON_PHONE`; "Answered by you" from
the web). **An item with no default** shows "No default" in the Inbox and in its
reminder notes, and completing its reminder only marks it seen: nothing is
applied, the item stays open, the reminder goes.

**Typing an answer.** On the phone the person edits the reminder's notes and
adds a line that starts with `>` (several lines are joined). The app sends the
notes only of reminders that have such a line (`PhoneRemindersSync.notes`;
the daemon never sees other notes) every 20 s or after an EventKit change.
The daemon (`phone::jobs_for`) accepts a reply only when:

- the reminder is one Neko made (it is in `mapped`) and its item is still open;
- the `>` line is the person's: every `>` at the start of a line in text Neko
  writes into a reminder is turned into `›` (`inbox::without_reply_marks`),
  so an agent's own quote can never count, and the how-to line has none;
- it is an answer (`inbox::answer_text`): something to read in it (not only
  punctuation), not the item's own title or a line of its detail quoted back;
  control characters are dropped, 1000 characters at most;
- it is new: the last text looked at is kept as a hash on the mapping, so the
  same text is never judged twice, and a text that was refused is not
  replaced by the default when the reminder is completed.

Then the answer goes only where a typed one would go (table above). It is
never run or interpreted as a command. The daemon puts `Got it: <words>
(<what happened>)` under the person's notes (`ReminderPlan.finish`), the app
writes those notes and completes the reminder, and reports
`PhoneRemindersDone.finished`. An answered reminder is kept (completed) for
24 h so the echo can be read, then deleted. Only reminders Neko made and
only text added after it was created count. Words on the digest's reminder
are not an answer to anything; completing it marks the digest read.

**Escalation discipline.** Every escalation is one question with a default:
the routine reply contract (`needs_you`: `title` is one question,
`default` the answer Neko takes if only accepted; `parse_reply` keeps the
text up to the first question mark and an empty default is none), the goal
judge rules (BLOCKED needs exactly one question and a default) and
`goals::block` (second question dropped, a missing default filled with "Use
your best judgement and keep going") enforce it; prompt goldens pin the
wording. An item that still arrives without a default is shown, not dropped.

**Channel B, ntfy** (optional). Server (default `https://ntfy.sh`, or your
own), a random topic (`neko-` + 24 characters, made on first read, shown
only to the owner, "New topic" replaces it), an `ntfy://host/topic`
subscribe link with Copy, and "Send titles only" (on): the server then sees
the title and "Open Neko on your Mac.", never the detail or link. The only action button
is **Open in Neko**, and only when the person typed an address in Settings
(`open_url`, `http(s)://…`, opens `<address>/#/inbox`): Neko never makes the Mac
reachable, so there are no Accept or Decline buttons (they would need an
endpoint the phone can reach) and Settings says replies need Reminders. The daemon
posts JSON to the server root (so any text is safe in the title) with a 10 s
timeout, no click URL, priority 4 for urgent. A failure stops the batch and
is logged once per distinct message. No QR code: nothing in `web/` makes one,
and the link opens the ntfy app.

**The morning digest** is also pushed here once (one reminder whose notes hold
it, one ntfy message): see `morning-digest.md`.

**Rows.** `phone_settings_v1` (choices), `phone_state_v1` (cursor, held
items, reminders in flight, echoes to write, send log; quiet writes). Nothing timer-driven is
in the workbench snapshot. The thread ticks every 30 s and costs one small
row read when nothing is waiting.

**Test.** `neko-daemon --request '"PhoneTest"'` (also the Settings button)
sends on every channel that is on, ignoring quiet hours, idle and the cap.
Tests never reach a real server (a local listener plays ntfy); the web check
points the server at a closed local port. No test creates a reminder: access
cannot be granted in a test, so the Swift tests cover the pure parts
(`PhonePayloadTests`: access names, source choice, the marker line, request
and plan shapes, which reminders send their notes, what counts as a reply) and
Rust covers the decisions (`phone::tests`: the default for each kind, the `>`
routing, what is ignored, the echo).

Check: `NEKO_WEB_ONLY=phone node scripts/web-verify.mjs` (area
`scripts/verify/phone.mjs`).

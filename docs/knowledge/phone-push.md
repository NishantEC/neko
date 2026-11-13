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
mapping lives in `phone_state_v1` (not the snapshot). Completing the
reminder, on the phone or the Mac, closes the item as "Done on your phone"
(`inbox::ON_PHONE`); closing the item in Neko in any way, or turning the
channel off, deletes the reminder; deleting the reminder yourself leaves the
item open. A reminder the app has not made within an hour is dropped.
Limit: the mapping uses `calendarItemIdentifier`; if EventKit ever changes
one after a sync, that item just stops closing from the phone.
`NSRemindersFullAccessUsageDescription` and the older key are in
`scripts/Neko-Info.plist`. Denied or restricted shows one line in Settings.

**Channel B, ntfy** (optional). Server (default `https://ntfy.sh`, or your
own), a random topic (`neko-` + 24 characters, made on first read, shown
only to the owner, "New topic" replaces it), an `ntfy://host/topic`
subscribe link with Copy, and "Send titles only" (on): the server then sees
the title and "Open Neko on your Mac.", never the detail or link. The daemon
posts JSON to the server root (so any text is safe in the title) with a 10 s
timeout, no click URL, priority 4 for urgent. A failure stops the batch and
is logged once per distinct message. No QR code: nothing in `web/` makes one,
and the link opens the ntfy app.

**Rows.** `phone_settings_v1` (choices), `phone_state_v1` (cursor, held
items, reminders in flight, send log; quiet writes). Nothing timer-driven is
in the workbench snapshot. The thread ticks every 30 s and costs one small
row read when nothing is waiting.

**Test.** `neko-daemon --request '"PhoneTest"'` (also the Settings button)
sends on every channel that is on, ignoring quiet hours, idle and the cap.
Tests never reach a real server (a local listener plays ntfy); the web check
points the server at a closed local port. No test creates a reminder: access
cannot be granted in a test, so the Swift tests cover the pure parts
(`PhonePayloadTests`: access names, source choice, the marker line, request
and plan shapes) and Rust covers the decisions.

Check: `NEKO_WEB_ONLY=phone node scripts/web-verify.mjs` (area
`scripts/verify/phone.mjs`).

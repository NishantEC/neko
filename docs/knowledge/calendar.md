## The Calendar: macOS Calendar plus Neko's dated work (2026-11-06)

Goal: a Calendar that works on anyone's Mac with no setup beyond allowing
Calendar access. DESIGN.md is the authority for how it looks (meetings as
cards with a coloured left edge, Neko's work on the all-day row in Iris, the
Iris now line and today badge, Day/Week/Month, the week number).

**Where events come from.** EventKit, read by the installed app. EventKit
already holds every account added in System Settings → Internet Accounts
(Google, iCloud, Exchange), so there is no OAuth, no Google API and no
per-account setup in Neko. Not used: an MCP calendar server or the
claude.ai connectors (owner-specific, and not available to everyone).

- **Native** (`native/NekoKit/Sources/NekoNative/CalendarBridge.swift`),
  started from `NekoStartup` beside `RoutineNotifier` and guarded the same
  way (only a real `.app` bundle with an identifier runs it; `swift run`
  has no usage description). Asks once with `requestFullAccessToEvents`
  (EventKit itself shows the prompt only while access is undecided). Every
  5 minutes, and 2 s after the last `EKEventStoreChanged` of a burst, it
  sends `SetCalendarEvents` with the events from local midnight 7 days ago
  to local midnight 35 days ahead (at most 2500 read). A repeating event's
  occurrences share `eventIdentifier`, so the id is `<identifier>@<start
  seconds>`. The link is the event's own `http(s)` URL, else the first
  meeting link (Meet, Zoom, Teams, Webex, …) in its location or notes, else
  a location that is itself a URL. Write-only, denied and restricted access
  are all sent as `Denied`. Access granted later in System Settings makes
  the next sync `reset()` the store. The pure parts live in
  `CalendarPayload` and are tested (`CalendarPayloadTests`).
- **Info.plist**: `NSCalendarsFullAccessUsageDescription` in
  `scripts/Neko-Info.plist` (copied into the bundle by
  `scripts/build-native.sh`). The bundle is signed without the hardened
  runtime, so no calendars entitlement is needed; add
  `com.apple.security.personal-information.calendars` if that ever changes.

**Daemon** (`neko_core::calendar`, protocol in `neko_protocol::calendar`).

- `SetCalendarEvents { events, source, access }` (access defaults to
  `Granted`) replaces the stored copy: one quiet row `calendar_events_v1`
  (`Db::set_setting_quiet`: outside the workbench snapshot, wakes no held
  `WorkbenchIfChanged`). `clean` trims fields, keeps only `#RRGGBB`
  colours and `http(s)` links, drops repeats of an id, sorts by start and
  keeps at most 2000 events and 1 MB (`truncated` says so). An identical
  resend writes nothing. Without access the stored events are dropped.
- The sync time changes every five minutes, so it is **not** what makes the
  status row (`calendar_status_v1`) rewrite: that row changes only when
  access, source, count or `truncated` change, or when its sync time is an
  hour old; the newest sync time lives in memory (`LAST_SYNC`). Measured in
  the test `store_keeps_events_quietly_and_skips_identical_writes`
  (`Db::setting_writes` unchanged by a resend).
- `CalendarEvents { from_ms, to_ms }` (at most 100 days) answers stored
  events overlapping the range, Neko's dated work in it and the status.
  `CalendarStatus` answers the status alone.

**Which Neko work has a date** (`neko_items`; tasks have no due date field,
so this uses what exists):

- tasks on the day they **finished** (`updated_at_ms` of a task that is
  ReadyForReview, Completed or Failed); open and cancelled tasks carry no
  date; opens `tasks?task=<id>`;
- each enabled **scheduled plan**'s coming runs (`next_due_ms`, then
  `scheduled_plans::following_due`, up to 60 per plan); opens `schedules`;
- each enabled **daily routine**'s coming runs (from now, in the Mac's zone)
  and each **repeating routine**'s next run only (an every-20-minutes
  routine would fill the row); opens `inbox`.

Past runs of schedules and routines are not listed: what they produced is a
task or an Inbox item. Recurrences are expanded after the database lock is
released. The snapshot is read through `workbench::load`, which is cached by
content stamp, so a poll costs a copy, not a reparse.

**Web** (`web/src/pages/Calendar.tsx`, route `calendar`). Week (Mon–Sun) by
default, Day and Month; Today, previous/next, an ISO week number. The hour
grid shows 07:00–20:00 and scrolls to the whole day; it opens on 07:00, or
two hours before now when that is later. Overlapping meetings share the day
side by side (`layoutDay`: each cluster gets as many columns as it needs at
its busiest). Cards take the calendar's colour as the left edge and a 14%
tint (mint when the calendar has none); a click opens a popover with time,
place, calendar and a Join/Open link (`http(s)` only). Neko items are Iris
chips on the all-day row (task status glyph, or the time) and open their
route. The page asks for its range every 60 s, only while visible.

States, each saying what to do: never synced ("Open the Neko app and choose
Allow…"), denied ("Allow Calendar access in System Settings → Privacy &
Security → Calendars", with a button to that pane), a sync older than 20
minutes ("Neko may not be running"), more than 2000 events.

**Verified** with fixtures only: EventKit access cannot be granted in a
test. Rust tests cover cleaning, caps, quiet writes, ranges, denial and the
Neko items; the daemon test round-trips the JSON the app sends; Swift tests
cover the payload, links, colours and the window. The page was looked at in
the shared Chrome against `web_harness` with fixture events (week, day,
month, popover, chip navigation, not-asked and denied, dark and light).

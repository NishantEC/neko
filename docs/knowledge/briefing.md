# The Briefing (2026-11-27)

What happened while the person was away, in one calm page, and one quiet notification in the morning. Protocol 10, store 7 (no new
table: see "Where it is kept").

Code: `neko-protocol/src/briefing.rs` (the shapes, `first_sentence`, the headline), `neko-core/src/briefing.rs` (the window, the
sections, the morning's decision), `neko-daemon/src/briefing.rs` (the requests and the push), `neko-daemon/src/rounds.rs` (the
scheduler thread decides the morning), the app's `briefing.rs` (words, state) and `briefing_view.rs` (the page).

## No model, and why

The briefing is composed from records Neko already keeps: tasks, runs, parts, undo state, round history and the event log. Nothing
asks a model to write it, so it costs nothing, needs no key, sends nothing anywhere and is the same after a restart (tested: a
daemon whose `claude` cannot run still derives one). The sentences in it are the cats' own, cut to their first. A written summary
by a cat (a real model call, which must ask first) is not here; see "Not here yet".

## The window

A briefing covers a window: **from the person's last "Mark as read" (the first time, 24 hours before now) to now**. The start is
inclusive and the end exclusive, so a task that ended at exactly the moment the window starts is in it, one that ended at exactly
its end is not, and two windows that meet (mark read, then the next) share their edge: nothing is shown twice and nothing is lost
(tested). `get_briefing {from_ms?, to_ms?}` takes a window of its own for tests and tools. A read mark in the future (the clock was
set back) is taken as now.

## What is in it

1. **Needs you**, first: everything still waiting, whenever it began (`needs::derive`, the list Needs you shows, whole), each item
   led by its cat. The only thing the person must act on.
2. **Finished**: a task whose newest run that ended in the window is a done, failed, stopped or interrupted one, and which is not
   waiting for the person (those are in Needs you) and not working again (those are in Still working). Newest first, at most 50
   (the rest are counted, "and 12 more finished earlier"). Each: the cat, how it ended (`finished`, `could not finish`, `was
   stopped`, `was interrupted`), when, the title, **the first sentence of the cat's final answer**, the files its run changed with
   added and removed (the run's own changes when its checkpoints say, else its branch's), the branch, whether it was **undone
   since**, and what the run's card offers (`neko_protocol::undo::offers` over all the task's runs: Review, Undo or Redo). A run
   that did not finish and said nothing gives its reason instead of a sentence.
3. **Rounds**: every line the scheduler wrote in the window for a due time (ran with its task and where it stands, skipped, missed,
   each with its reason) and every brake (a round turning itself off after three failures). A Run now is the person's own doing and
   has no line. Newest first, at most 40, the rest counted.
4. **Still working**: tasks working now.
5. **Totals**: tasks finished; files changed and lines added and removed by those runs (a run that was undone since is not
   counted: its files are not in the folder); and **the cost the runs that ended in the window reported**, as Claude Code reported
   it, only when some run reported one, with "API-equivalent, not charged on a subscription." when it is not money charged.

### The first sentence

`first_sentence` reads Markdown as the words it carries: emphasis, code spans, links and images keep their words; a heading is passed
over unless it is all there is; a list item or a quote loses its marker and a list item is a unit of its own; a code block is
skipped (prose before it counts; a block alone says "Code"); the sentence ends at its first full stop, question or exclamation mark
followed by a space or the end, but not at "v1.2", "e.g." or "etc."; one past 160 characters is cut at a word with an ellipsis.
An empty answer has none. Tested for each of these.

### News

`BriefingCounts.news` says there is something new to say: a task finished, a round was due, or something began to wait inside the
window. A request that has been waiting since before the window is **not news** (it is in Needs you and its count is on the
sidebar): so after "Mark as read" Home's line and the morning notification are quiet, while the request stays in sight on the page,
under "You're caught up". The headline is "3 finished, 1 needs you"; when neither is there, what the rounds did ("2 rounds ran",
"1 round did not run"), then "1 still working".

### Where it is kept

`settings` rows: `briefing_read_ms`, `briefing_morning_on`, `briefing_morning_at`, `briefing_morning_cursor_ms`. Each change appends
an event (`briefing_read`, `briefing_settings`, `briefing_morning`), the first look at the morning (which only starts the
cursor) none, so a fresh store stays at revision 0. No table was needed, so the store stays at version 7 and there is no migration;
a test opens a store made before the Briefing and checks its defaults (no read mark: the 24 hours; morning on at 08:00).

## Staying current

`briefing_changed {briefing}` is pushed whole when its content differs from what screens were last told, apart from the moment it
was made. The same thread that keeps Needs you pushed looks at it after the same changes and after a run changing, a round
changing or removed, a task made, and the settings; it is not pushed for a streaming word. `mark_briefing_read` answers with the
new briefing and pushes it to every screen.

## The morning notification

- **Settings:** `briefing_settings {morning_on, morning_at}`, local time of the Mac, default on at 08:00 (Settings ▸ Briefing). A
  time that is not a time of day is refused (`bad_request`) and changes nothing.
- **The scheduler is Rounds' scheduler.** The morning is a `Daily { at }` schedule on the same arithmetic (`schedule::Schedule`), on
  the same thread (`neko-rounds`) with the same clock handed in (`Hub::now`, and its debug offset), and its timer is the earlier of
  the next round's and the next morning's. With the morning off and no round on, the thread parks with no timer.
- **Once a day.** A cursor holds the last morning time dealt with; each due time is dealt with once. Changing the settings restarts
  the count from now: a time already past today is not caught up.
- **Only with news.** If the window has none, nothing is posted and the time is passed.
- **A Mac that slept.** On waking, the latest morning time since the cursor is looked at: if it is **today's**, it is posted once;
  if it was an earlier day's it is passed over, so a wake never posts a stack (tested for a sleep through one day and through three).
- **Daylight saving.** A time that does not exist that day (02:30 when the clocks jump) comes once, shifted forward by the gap; one
  that happens twice (01:30 when they go back) comes once, at its first occurrence (tested on New York's real rules).
- **The daemon only says when.** It pushes `briefing_due {counts}`, not stored, to the screens. The app decides to post (it holds
  the notification model): title "Your briefing: 3 finished, 1 needs you", body one line of what is first (what needs the person,
  else the newest finished task), silent, no actions. Same rules as Needs you: the system's permission and Focus apply, and nothing
  is posted when Neko is in front on the Briefing. A click opens `neko://briefing` (`deeplink::is_briefing`, read strictly: that
  exact link, a trailing slash allowed); opening the page, or Mark as read, takes the notification back.
- **No screen.** With no screen connected when the time comes, the notification waits (no timer) and is posted when one connects,
  if it is still that day.

Limits: nothing is posted while Neko is not running or the Mac is asleep (the daemon starts with the app); a notification waits for
a screen only as long as the daemon runs; Neko cannot wake the Mac. These are Rounds' limits too (`rounds.md`).

## The app

- **The page** (⌘4, View ▸ Briefing, the sidebar row between Home and Needs you, `neko://briefing`). Between Home and Needs you
  because the day reads in that order (where am I, what happened, what must I do), and ⌘1 to ⌘3 keep their keys.
  - The heading is the window ("Since yesterday 18:00"; "Since 08:30" for today) over the date, and under it the headline.
  - **Needs you** reuses the Needs you cards (`needs_view::item_row`): the same requests, answers, reply boxes and keys, answered
    where they are and gone at once; Escape goes back.
  - **Finished** is one calm well of rows, each led by its cat's portrait: who and how it ended and when, **Open task** at the right
    of that line, the title, the first sentence, and, only when there is something to say, the files with their figures, the
    branch, "Undone", and Review, Undo or Redo (the daemon's offer: the same rules as the run's card; Undo that finds files changed
    since asks first, in the same panel). A failure says "could not finish" in red; nothing else carries colour.
  - **Rounds** are one line each ("Docs check ran · done", "Dependency audit was skipped · the last run is still working", "Backup
    test turned itself off · after 3 failed runs in a row"), newest first by the time shown; a line with a task opens it.
  - **Still working**, the **totals** line (tasks, files with +/−, the cost as Claude Code reported it, and the note) and **Mark as
    read** with its key (Cmd Return in the caps; ⌘↩).
  - With **more than one workspace** having anything, each section is grouped under the workspace's name; otherwise one list.
  - **Caught up**: read, and nothing new: "You're caught up", "Nothing new since 08:41. Your next briefing is tomorrow at 08:00." (or
    that mornings are off, with where to turn them on). A request that still waits stays under it ("Still waiting for you").
  - **Empty**, never read: "Nothing happened since yesterday 08:05." and the next briefing; no Mark as read (nothing to read).
- **Home** has one quiet card above the house while there is news ("While you were away: 3 finished, 1 needs you", Open briefing)
  and nothing when empty or read.
- **Settings ▸ Briefing**: what it shows, the morning switch and the time (Save, or Return), with the plain note on when it posts.
- **Keyboard.** Tab goes through the page in reading order: the Needs you cards and their answers, each finished row and its
  buttons, the rounds that open a task, the working rows, Mark as read, which is the last stop. Up and Down move between finished
  rows, Return opens a task, Escape goes back, ⌘↩ marks read. Every row has a name and a description (who, how it ended, when);
  sections are named lists; the page has one level-1 heading.

## Tests

- `neko-protocol/tests/briefing.rs` pins the JSON; `src/briefing.rs` tests `first_sentence` and the headline.
- `neko-core/tests/briefing.rs`: window edges, the first window and mark-read, each section from seeded records, each way a task
  ends, Needs you not finished, news, undone since, totals with and without cost, every round outcome, the caps, an empty window,
  the older store, and the morning (once a day, sleep and wake, no screen, off, an empty window, a changed time, daylight saving).
- `neko-daemon/tests/briefing.rs`: the real daemon with the fake CLI and the debug clock: finished with its sentence, mark read,
  pushes, Needs you items, settings kept and refused, the morning's one event with its counts and no second, a screen coming late,
  an empty window, no model.
- `apps/neko/tests/briefing.rs`: the page's structure and order, grouping, Review/Undo/Redo, Tab order, arrows, every state, Home's
  line, the link, settings, 760-point clipping and names, contrast; and the whole flow against a real daemon, ending in the
  morning's notification. Every scene is also in the structure, accessibility and contrast tests.
- QA: `briefing` and `briefing-morning` (`qa.md`).

## Not here yet

- **A written summary by a cat.** A real model call would say it better, and it would have to ask before it spends anything.
- **The phone**, or any push beyond this Mac.
- Quiet hours, a different time per day, a per-workspace briefing, or a weekly one.
- Posting while Neko is not running.

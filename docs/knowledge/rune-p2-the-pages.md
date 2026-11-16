## Rune P2: the pages, and the fixes P1 needed (2026-11-16)

Code: `apps/neko-rune/`. Decision and phases: `docs/decisions/0002-rune-for-everything.md`. Layout and calm
numbers below were decided by the owner on 2026-11-16 and are tested; the web UI is untouched and stays the
default.

### The decided numbers

| Region | Default | Min | Max |
| --- | --- | --- | --- |
| Sessions column | 300 | 240 | 420 |
| Context panel | 420 | 320 | 640 |
| Neko column | 340 | 280 | 480 (a 44 point icon strip when folded) |

Below a 1100 point window Neko starts as its icon until the person opens it (`Layout::neko_auto`). Widths and what
shows are kept between runs in Rune's `prefs` store (`App::tick_layout`: written once the layout has stopped moving
for half a second, on its own thread, never during input or frames; headless runs and tests never write it).
A session row is one line: glyph, title (13.5), the unseen dot, the age (12.5 muted), and for amber rows the word
"Needs you" or "Building" instead of the age. No id (it is in the session's top line).

### What P1's review found, and how it is fixed

- **Neko column** (`view/neko.rs`): header, crew line, Inbox (collapsible, 3 rows then "and N more"), Digest
  (collapsible, closed, one summary line), then the chat fills the rest and sticks to its end, then the ask box and
  the status line. The top block scrolls inside at most 46% of the window height, so the chat keeps room at any
  height (`tests/part_a.rs`: no overlap at 700, 840 and 1200).
- **Calm rules**: an Inbox item is a row (title, one muted detail line), the default is a 24 point check mark that
  shows on hover or focus plus the right-click menu entry "Use the default: <text>"; an item a goal raised shows the
  goal's own actions in the menu (answer, resume, extend, stop), never the routines' default; one primary button
  per region (`accent_button` is the affirmative that is not the primary: Approve on a tool call, the lead action
  while a confirmation is open).
- **Refusals** for Inbox and goal calls show beside the item (`NekoCol::notes`, keyed `inbox:<seq>` or `goal:<id>`)
  and beside the goal's line, not above the composer.
- **Sticky goal line**: it floats over the top of the conversation; the scroll area's top inset is its height plus
  16 (`work::GOAL_LINE_H`); what a goal adds (its question, evidence) hangs under its line and scrolls on its own.
- **Sessions list** scrolls inside its region and fades at the edge; the foot (a hairline, "+ New", Calendar,
  Routines, Settings, More) is pinned.
- **Request bubble**: a task a chat opened shows only `Task::request`, never the goal line.
- **Seen**: a finished session is marked seen only while the window has the keyboard and the session is on screen
  (`App::session_in_view`, `set_focused`).

### The pages (full views replace the session region)

Each reads and writes only what the matching web page does; every request is in `tests/allow_list.rs`.

| Page | Web source | Requests added |
| --- | --- | --- |
| Routines | `pages/Inbox.tsx` | `Routines` (whole), `SaveRoutine`, `RunRoutineNow`, `SetRoutineEnabled`, `SetRoutinesOn`, `SetStandingOrders` |
| Calendar | `pages/Calendar.tsx` | `CalendarEvents` (read only; "Open in Calendar" is a `calshow:` link) |
| Settings | `pages/setup/Settings.tsx` and sections | `PhoneSettings`, `SetPhoneSettings`, `PhoneTest`, `SetInboxAgeOutDays`, `SetCalendarEnabled`, `SetAgentRuntime`, `SetTaskBudget`, `SetDailyBudget`, `SetMaxRunningTickets`, `SetMaxWritablePerRepository`, `SetPlanFirst`, `SetStartWithoutApproval`, `SetSelfUpdate`, `UpdateNekoNow`, `ApplyPendingUpdate`, `SetDevCheckout` |
| Usage | `pages/setup/Usage.tsx` | `Search` (`usage` provider, the quota meters) |
| Workspaces | `pages/setup/Workspaces.tsx` | none (the scopes in `Routines`) |
| Plugins | the plugin status the web reports | `PluginStatus` |

Behaviour worth knowing:

- **Routines**: a line is glyph (on, running, held, failed, paused), name, scope, cadence, next due or "Held until
  14:00", switch. A press opens one routine at a time: the editor (instruction, cadence select, model select,
  Chrome, Mac apps, catch up), "Run now", the recent runs. Save is the page's one primary button and shows only
  when something changed. Standing orders, one box per workspace with routines, under the list. Goal pseudo-routines
  (`goals:`) are left out, as on the web.
- **Calendar**: week by default, Day/Week, 07:00 to 22:00 at 48 points an hour, an all-day row with Neko's dated
  tasks (status glyph; pressing one opens the session) and routine runs, mint blocks, an Iris now line, events that
  overlap share a band (`clusters`, `lanes`). Local time through `clock.rs` (`localtime_r`, `mktime`).
- **Settings**: sections at the left (200), rows at the right (640): label left, control right, help in a tooltip.
  Search (⌘F focuses it) filters rows of every section. "Updates come from" is read-only (the app sets it).
  Not here: Appearance, Rulebook, Quick panel folders, Permissions, Diagnostics, Start over, Roll back (web only or
  destructive). The runtime is picked from the catalog and saved with "Use this runtime" (it does not send the
  web's one-message check).
- **Usage**: a "Today" line, a seven-day line drawn with `rune::canvas` shapes (24 hour buckets, no time zone),
  quota rows with a 4 point bar (Iris, amber past 80%, coral past 95%).
- **Fresh data**: the open page asks again every 30 seconds (`PAGE_POLL_MS`) and after a write the daemon answered
  without sending the page back. A failed read keeps the old data and says nothing; a refused write says why next
  to its control (`Pages::notes`).

### The reviewer's round (2026-11-16)

- **Fields show what is saved.** The budgets, the development checkout and the ntfy server are filled from the
  daemon (`App::seed_settings`, `Pages::seed`) and follow it until the person types; Save and Return send nothing
  for a field that was never filled or still shows what is saved (an empty, never-seeded box once read as "no
  limit"). `tests/pages.rs` covers it.
- **Short and narrow**: under 600 points tall the Inbox shows two rows, the ask box stops at 80 and the chat keeps
  160; an open goal is capped at 35% of the window and scrolls; under 760 points of page width Settings' rail is 150
  and its sides 16, rows wrap their control under the label and inputs shrink (max 320).
- **Runs** are rows: the undo shows on hover or focus, and a right-click menu has Undo this run, Undo back to here,
  View diff. Routine, workspace and plugin rows have right-click menus too (plugins: Check again and Open folder; a
  plugin cannot be reloaded from here, the web page does that).
- **Errors stay where they belong**: a failed read keeps the old page and says nothing; with no data the page says why in
  place of "Looking for…"; the Inbox says "Couldn't read the Inbox" with Retry; a phone refusal shows under its row;
  a calendar answer for days no longer shown is dropped; layout saves wait for half a second after the last change.

### Testing

`tests/pages.rs` (made-up data through the headless `Runtime`: each page's lines, controls and requests, one
primary button, nothing outside its region at 960 and 1360 and 1600 points wide), `tests/part_a.rs` (the P1 fixes)
and three tests in `tests/harness.rs` against the isolated daemon (routines: switch, Run now, Save, standing orders;
settings: budgets, plan switch, Calendar on and off, Reminders, search; calendar, usage, workspaces, plugins).
Snapshots: `apps/neko-rune/scripts/snap.sh` renders every scene (the page scenes are in `src/scenes_pages.rs`);
the committed light and dark pictures are in `docs/design/rune/p2-*.png`, the review fixes' before and after in
`a-before-*` and `a-after-*`.

### Not done in P2

Plugin pages (P3), the context panel (P3), the web's Rulebook, Appearance, Quick panel folders and Diagnostics,
Memory, Profiles, Watching, Pull requests and Tools pages (not in the owner's "More" list; the web UI has them),
the routines' "Closed recently" and "Mark done" (the Inbox page), a calendar week view that scrolls to the working
hours, remembered Workspaces open state across runs (it is remembered while the app runs).

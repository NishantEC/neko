# Threads (the sidebar, 2026-11-27)

A **thread** is one conversation with one agent. It was called a task; the stored record, the protocol and the code still say
`task`, and the older pages in this folder do too. Only what the person reads changed.

## The words

| Before | Now | Where it shows |
| --- | --- | --- |
| task | **thread** | every view, menu, dialog, notification, Settings page, accessibility label and the daemon's messages |
| workspace | **project** | the same places; a project is a folder the person added |
| General | **No project** | a thread with no folder works in a scratch folder of its own |
| Remove task… | **Remove thread…** | confirmation asks first, as before |

What did not change, on purpose: the protocol's field and message names (`task`, `workspace`, `create_task`, …), the store's table
names, the `neko://task/<id>` link (it is a contract with notifications already posted), Rune keys such as `task-more`, and the
daemon's own name for the built-in project (`General`, id `default`); the app shows it as No project
(`App::upsert_workspace`). New thread (below) replaced Home, so those words are gone.

## What the daemon works out (protocol 12, store 9; pin, settle and archive come below)

All of it is derived in `crates/neko-core/src/thread.rs`, from what the store holds, so every screen sees the same after a restart.
The Needs you list uses the same predicate (`needs::failure_of`), so a thread's flag and the list agree.

- `last_read_ms`: when the person last opened the thread. A thread the person makes is read at birth; a thread a round makes is
  unread until opened. Opening a thread sends `mark_read`, and so does anything that arrives in the thread in view while Neko is in front; Neko behind another app leaves it unread. Mark as unread (a row's menu, not offered for the thread in view) sends `mark_unread`.
- `activity_ms`: when something happened that the person did not do themselves: the agent's words, a run's end, a request, an
  interruption. A run starting and a stop are the person's own, so neither counts. **Unread** is `activity_ms > last_read_ms`
  (`Task::unread`); the app shows it as a semibold title.
- `latest_line`: the last line with words in it of the newest agent words (Markdown's marks dropped), cut to 120 characters. It
  is written with the part, in the same transaction, and the task is pushed (`task_updated`) when it changes, which is at most as
  often as the words are written to disk (every 3 s).
- `failure_line`: why the last run failed, while the thread is `Failed`.
- `thread_status`: **Working** (a run is going, also while it waits on a request), **Waiting** (no run, and the thread waits on its
  subagents: `subagents.md`), **Failed** (the last run failed and the person has not opened the thread since) or **Idle**. It is
  `thread::thread_status`, and the task's own `status` is `waiting` then.
- `needs_you`: a flag beside the status, true for a request, a question, and a failure the person can act on, and (version 13) for a
  request one of the thread's subagents has waiting: the thread's run has ended, so its own status cannot say it.

Migration 9 reads every existing task as read as of its last activity and fills `latest_line` from its newest agent words.

## What a row shows

- **The status mark** (`status_mark.rs`), 12 points in a 16 point slot, in place of the cat's portrait: an amber dot for Needs you,
  a red dot for Failed, a small turning ring in the identity teal for Working (still under Reduce Motion), the same ring held still in the second ink for
  Waiting (the thread is not busy), nothing for Idle. A face
  now means a Cattain. Colours are taken toward their strong ink until they reach 3:1 (dots) and 4.5:1 (ring) against the panel.
- **Line 1:** the title (semibold while unread), and at the right the age (`now`, `2m`, `3h`, `Mon`, `Oct 3`) or **Working** in
  teal, only while the mark is the working ring: a thread that needs the person shows its age beside the amber dot, never "Working". Under the pointer the age gives way to a menu: Mark as unread (or read), Remove thread….
- **Line 2**, the first that applies: `Needs you: <what>` in amber (a subagent's request reads `Needs you: Run a command · Kitten 1`);
  `Failed: <why>` in red; what it waits on ("Waiting on Kitten 1", "2 kittens working"; `App::thread_line`); the agent's latest line. A thread that
  has said nothing yet says what it is ("Not started", "Working…").
- **Order inside a group:** threads that need the person first, then by last activity, newest first. A thread that works keeps the
  place it had when it began (`App::sort_hold`), so a row does not move under the pointer as its words stream; it moves when the work
  ends. Opening a thread does not reorder anything.
- **Groups:** one per project in the person's order (⌥⌘↑/↓ as before), removed ones that still have threads, No project last. View
  (the three lines at the Threads header's right) groups by Project (default) or None (one list; each row names its project at the
  right of line 2). The choice is kept in the prefs (`thread-grouping`). Show more appears after 8 rows in a group.
- **With no thread at all** the section says "Threads you start appear here" and No project is not listed.

Subagents are not threads: no row, no group. Not here yet (later slices): drag a row onto a Cattain. `u` for Mark as unread (and `a` for Archive) was left out: in a list that jumps to a typed name it would swallow the letter.

## New thread (protocol 14, store 11)

The app opens on **New thread** (⌘N and ⌘1; the View menu's "New Thread"), not on a list. A thread has no cat: the daemon assigns
none (`cats.md`). The screen is `new_thread_view.rs`, its state `new_thread.rs`.

- **The screen:** the headline "What should we work on?" ("... in <project>?" once a project is chosen), the composer ("Describe
  the work"), then one row of chips. Under it, when they apply: the notice line, the daemon-slow line, the Briefing's line.
- **Chips:** Project (No project, the projects, Add folder…). Only for a project that is a git repository with a commit: **New
  worktree** or **In the folder**, and, for a worktree, the branch it starts from. New worktree is the default every time, and it
  is reset to it when the project changes; the choice is for the session only.
- **Return** starts the thread and opens it. **⌘Return** starts it without opening: the screen stays, the box is empty with the
  keyboard in it, and one quiet line says "Started <title>". A background start that is refused puts the words back.
- **The folder race.** Choosing Add folder… and sending at once used to start the thread in No project, because the project became
  the composer's choice only when the daemon answered. Now the folder is recorded at once (`App::begin_adding`) and the chip names it
  ("Project: <name>, adding"). A send meanwhile is held (`new_thread.held`: the composer is disabled and the words stay in the box);
  when the daemon answers `folder_added` chooses the project and sends. A refused folder or a lost link releases the hold
  (`folder_failed`) and keeps the words.
- **What the daemon takes:** `create_task {words, workspace?, in_folder?, from_branch?}`. `in_folder` works in the project's own
  folder, even a repository, with no worktree: the place is "plain", the first edit asks, and Undo uses the shadow repository as for
  any plain folder (`undo.md`). `from_branch` starts the worktree at that local branch's tip, not at the commit the checkout is on;
  it is checked by `workplace::branch_name_ok` (a name that is not plausible is `refused`) and a branch that does not exist fails
  the run's folder step with words. Both are kept on the task. `list_branches {workspace}` answers `branches` (the checkout's
  branch first; `git` is false for a folder that is not a repository with a commit, and for No project) and is answered on its own
  thread so the chip never waits. Code: `workplace::{Placement, Branches, branches, branch_name_ok}`, `Core::create_task_placed`.
- **Turns have no head.** The portrait, name and "done" row are gone: the words are the agent's. A parent that rests while its
  kittens work no longer reads "done"; the newest run's fold line carries "· Waiting on Kitten 1 and Kitten 2" (or "Reading the
  reports"), `subagents_view::waiting_words`.
- **Tests:** `apps/neko/tests/new_thread.rs` (end to end against a real daemon, the folder race included), core
  `a_thread_can_start_from_a_chosen_branch_or_work_in_the_folder_itself`, the daemon's `a_new_thread_says_where_it_can_start_from_…`.
  QA scenarios are in `qa.md`.

## Pin, settle, archive (protocol 17, store 13; S8, 2026-11-27)

Three ways to put a thread where the person wants it, and one way Neko puts one away. The rules are in one place,
`crates/neko-core/src/shelf.rs`; the stored times are `tasks.pinned_ms`, `settled_ms` and `archived_ms`, and the task carries them as the
booleans `pinned`, `settled`, `archived` (none is written until it is true).

- **Pinned** threads sit at the top of Threads in a group of their own, from any project, and are never settled by themselves.
- **Settled** threads wait in a folded **Settled (n)** line at the end of their group. New activity brings one back: the agent's words,
  the end of a run, a request, a run starting (the person's message, a wake, a round). A stop is the person's own doing and does not.
  Pinning a settled thread brings it back; settling a pinned thread unpins it (Undo puts the pin back too).
- **Archived** threads sleep: out of every list, out of Needs you (its flag and its request items go while it sleeps and come back when
  it is restored), kept as they were. The person's next message in one starts a run and wakes it; Restore (the row's menu) brings it back
  without a run. Archiving keeps a thread's pin, so Restore puts it back exactly. Timers and watches do not exist yet; when they do
  (S13) archiving ends them.
- **Refusals,** in words, never silent: a thread that works cannot be settled or archived ("It is working. Settle it when it stops."),
  and one that waits on its kittens cannot be archived. A kitten is not a thread: it cannot be any of the three. Taking one off, and
  asking for what already is, are always fine.
- **Auto-settle** is the daemon's housekeeping, in the rounds scheduler's loop (`rounds.rs`): a thread nothing has happened in for
  **7 days** (the newest of its creation, its activity and its runs) is settled when it is idle. It skips pinned, archived and settled
  threads, threads that work or wait on their kittens, threads that need the person, and threads with a report waiting to wake them. It
  looks when a thread's week is up and at least every 6 hours while any thread could still be settled; with none it asks for no timer.
  Its clock is the scheduler's, so the QA rig's debug clock moves it (`qa.md`, `auto-settle`). The setting "Auto-settle after" is S16.
- **Requests:** `pin_task {task, pinned}`, `settle_task {task, settled}`, `archive_task {task, archived}`; each answers `task` and every
  screen hears `task_updated`.

What the person sees (`sidebar.rs`, `shelf.rs`):

- **Pinned** is the first group under the Threads header (a pin glyph and the word, no fold: it is gone when nothing is pinned). Its rows
  are the ordinary rows and each names its project at the right of line 2, as in the ungrouped list.
- **Settled (n)** ends a group: a chevron, the word and the count, a control high, a row gap under the last row. Pressing it opens the
  settled threads in place as ordinary rows; View ▸ Show settled keeps every fold open. A group with only settled threads shows just
  its fold.
- **Archived threads** are in no list. Until Search (S9) finds them, View ▸ Show archived adds an **Archived** fold at the end of Threads,
  with the threads as rows naming their projects. This is the one thing the spec did not ask for.
- **The row's menu** (the three dots under the pointer; it shows no key hints, see below): Pin to top or Unpin, Settle (off while the thread works) or Bring back, Mark as
  unread or read, Archive or Restore, Remove thread…. The band's menu has Pin, Settle, Archive, then All changes and Remove thread….
- **Keys:** ⌘⇧P and ⌘⇧S pin and settle **the thread in view** (whatever has focus: when a row leaves the list the focus moves, and the
  key must not act on a row the person did not choose); ⌥1 to ⌥9 open the nth thread of the sidebar in the order it is drawn (Pinned,
  then each open group, its rows and then its settled ones when that fold is open); ⌘⌥↑ and ⌘⌥↓ step to the thread before and after,
  wrapping. The menus do not print the keys beside Pin and Settle: rune-ui's menu row sets its shortcut text in Inter, which the font rule (`tests/structure.rs`) refuses, so this is an upstream ask (`rune-ui-in-neko.md`); the keys are in this page and in app-shell.md. A bare `a` for Archive was left out for the reason `u` was: the list jumps to a typed name.
- **Toasts** (rune-ui's Toast, bottom centre, polite): Settled, Archived and Unpinned each say what happened, name the thread and offer
  Undo for 5 seconds; the timer holds while the pointer is over the toast. ⌘Z takes back the newest toast that has an Undo (the toast goes
  with it). Pinning shows no toast (the row jumps to Pinned: that is the news). A refusal shows a warning toast with the daemon's words
  and no Undo. A text field keeps its own ⌘Z. `App::undo_toast`, `App::toast_action`.
- Opening a settled or archived thread works as for any thread; it stays open when it is settled or archived under the person's eyes.

A row's right side is 38 points wide (it was 30): a date such as "Nov 17", which an archived or old thread shows, overflowed the old slot and touched
the title.

Tests: `crates/neko-core/tests/shelf.rs` (flags, refusals, what brings a thread back, the housekeeping), `crates/neko-daemon/tests/shelf.rs`
(requests, pushes, a message waking an archived thread, Needs you, the week with the debug clock), `apps/neko/tests/shelf.rs` (groups,
folds, menus, keys, toasts, order and rhythm). QA scenarios `settle-undo`, `auto-settle`, `archive-wake` (`qa.md`).

## Sidebar width

Fixed at 256 points. A resizable split (drag the edge between 208 and 360) is an upstream ask to rune-ui, listed in
`rune-ui-in-neko.md`.

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
(`App::upsert_workspace`). The words "napping", "the house" and "Any cat" are Home's and stay until Home is replaced.

## What the daemon works out (protocol 12, store 9)

All of it is derived in `crates/neko-core/src/thread.rs`, from what the store holds, so every screen sees the same after a restart.
The Needs you list uses the same predicate (`needs::failure_of`), so a thread's flag and the list agree.

- `last_read_ms`: when the person last opened the thread. A thread the person makes is read at birth; a thread a round makes is
  unread until opened. Opening a thread sends `mark_read`; Mark as unread sends `mark_unread`.
- `activity_ms`: when something happened that the person did not do themselves: the agent's words, a run's end, a request, an
  interruption. A run starting and a stop are the person's own, so neither counts. **Unread** is `activity_ms > last_read_ms`
  (`Task::unread`); the app shows it as a semibold title.
- `latest_line`: the last line with words in it of the newest agent words (Markdown's marks dropped), cut to 120 characters. It
  is written with the part, in the same transaction, and the task is pushed (`task_updated`) when it changes, which is at most as
  often as the words are written to disk (every 3 s).
- `failure_line`: why the last run failed, while the thread is `Failed`.
- `thread_status`: **Working** (a run is going, also while it waits on a request), **Failed** (the last run failed and the person
  has not opened the thread since) or **Idle**. The Waiting status of subagents is the one place to add: `thread::thread_status`.
- `needs_you`: a flag beside the status, true for a request, a question, and a failure the person can act on.

Migration 9 reads every existing task as read as of its last activity and fills `latest_line` from its newest agent words.

## What a row shows

- **The status mark** (`status_mark.rs`), 12 points in a 16 point slot, in place of the cat's portrait: an amber dot for Needs you,
  a red dot for Failed, a small turning ring in the identity teal for Working (still under Reduce Motion), nothing for Idle. A face
  now means a Cattain. Colours are taken toward their strong ink until they reach 3:1 (dots) and 4.5:1 (ring) against the panel.
- **Line 1:** the title (semibold while unread), and at the right the age (`now`, `2m`, `3h`, `Mon`, `Oct 3`) or **Working** in
  teal. Under the pointer the age gives way to a menu: Mark as unread (or read), Remove thread….
- **Line 2**, the first that applies: `Needs you: <what>` in amber; `Failed: <why>` in red; the agent's latest line. A thread that
  has said nothing yet says what it is ("Not started", "Working…").
- **Order inside a group:** threads that need the person first, then by last activity, newest first. A thread that works keeps the
  place it had when it began (`App::sort_hold`), so a row does not move under the pointer as its words stream; it moves when the work
  ends. Opening a thread does not reorder anything.
- **Groups:** one per project in the person's order (⌥⌘↑/↓ as before), removed ones that still have threads, No project last. View
  (the three lines at the Threads header's right) groups by Project (default) or None (one list; each row names its project at the
  right of line 2). The choice is kept in the prefs (`thread-grouping`). Show more appears after 8 rows in a group.
- **With no thread at all** the section says "Threads you start appear here" and No project is not listed.

Not here yet (later slices): Pinned and Settled groups, the Waiting status and subagent lines, the row's keys (⌘⇧P, ⌘⇧S, `a`),
drag a row onto a Cattain. `u` for Mark as unread was left out: in a list that jumps to a typed name it would swallow the letter.

## Sidebar width

Fixed at 256 points. A resizable split (drag the edge between 208 and 360) is an upstream ask to rune-ui, listed in
`rune-ui-in-neko.md`.

# Search, ⌘K (S9, 2026-11-28)

A calm way to get anywhere: ⌘K opens a palette that finds threads (by title and by anything said in them, including the thread's
first words), kittens, Cattains, projects and actions. Protocol 18, store 14. The daemon answers for threads and kittens (they are the
only things with words in the store); the app's own lists answer for the rest.

## What the person sees

- **Open:** ⌘K, File ▸ Search…, or the magnifier beside Hide sidebar in the sidebar's band (its tooltip says ⌘K). ⌘K again or Escape
  closes it. The keyboard is in its field.
- **A centred sheet,** 600 points wide, rune-ui's Command in the floating material over a soft scrim; its top edge is fixed for a
  window size (`search_view::top`), so the field never moves while rows come and go.
- **Nothing typed:** a **Recent** section (the six threads with the newest activity, put-away ones left out) and the **Actions**.
- **Typed:** sections in this order, empty ones left out: **Threads, Kittens, Cattains, Projects, Actions**. At most 12 rows.
  - A thread's row: its status mark (`status_mark::bare_mark`: amber dot, red dot, working ring, still ring), its title, and at the
    right its project (cut to 16 characters; none for No project) and its age. A thread that is put away says `settled · ` or
    `archived · ` first. A thread found by something said in it shows **the words around the match** on a second line.
  - A kitten's row is the same with its thread's name under the title ("Kitten 1 in Fix login").
  - A Cattain: its face and name. A project: a folder and its name, "New thread" at the right. An action: its icon, its name and its
    key. The letters that matched are semibold (Command's own fuzzy matching, in titles and in snippets).
- **Keys:** ↑ ↓ move, Return opens, ⌘Return opens beside, Tab turns the text into a new thread, Escape closes; the footer says so.
  - Return on a thread opens it; on a message hit the conversation scrolls to that message once the transcript is there (a finished
    run's folded work is opened first when the words are inside it). A thread's first words (not a part of its transcript) scroll to
    the top.
  - ⌘Return on a thread opens it with the right pane's Overview; on a kitten it opens it in the main view, where plain Return opens
    it as a tab of the pane beside its thread.
  - Return on a Cattain opens its own thread (`cats.md`); on a project starts a New thread in it; on an action does it.
  - **Tab:** closes the palette, goes to New thread with what was typed as its words, **without** the filters, and chooses the
    project a `project:` filter names. The keyboard is in the composer.
- **Loading:** the field shows a spinner while the daemon has not answered the text now in it, and the rows already there stay put
  until it does. With no rows it says "Searching…". Nothing waits: the request is sent from `poll`, the answer arrives on a later frame.
- **No rows** says "No results"; a filter that cannot be read says why ("“sleepy” is not a status. Try working, waiting, …").

## Filters and words

Typed into the same field: `project:acme` (part of a project's name; `none` or `general` is No project), `status:working|needs`
(`working`, `waiting`, `failed`, `needs`, `idle`, `settled`, `archived`, `pinned`; `|` or `,` means any of them), `cat:miso` (part of a
Cattain's name: reaches threads made before cats went away and by rounds) and `since:2d` (activity within `30m`, `36h`, `2d`, `1w`).
Quotes make a phrase (`"redirect alone"`, in order), `project:"my app"` takes a name with a space. A filter with no value yet is
dropped, never searched for as a word. Each bare word is a **prefix** ("log" finds "login") and **every word must be in the same
title or the same message**. Case and accents do not matter. A filter alone lists the threads it keeps, newest activity first.
Settled and archived threads are found (and say so), and a kitten is found only by words, never by a filter alone.
`working` includes a thread that waits on a request (its row shows the working ring); `needs` is the flag beside it.

## How the daemon finds (`crates/neko-core/src/search.rs`, `crates/neko-daemon/src/search.rs`)

- **Two FTS5 tables** (the bundled SQLite has FTS5 and JSON built in; checked by the tests), made by migration 14 from what exists and
  kept by triggers in the same transaction as every change: `find_titles(title, words)` over a task's title and its first words
  (`tasks.words`, which are not a part), and `find_words(text)` over the text of every part of the person or the agent (not tools,
  thinking, notices). Each row has the rowid of the row it indexes (`tasks.rowid`, `parts.rowid`; nothing vacuums the file). A part that
  streams is rewritten by `update_part`; the trigger replaces its row. Deleting a thread deletes its rows. Tokenizer `unicode61` with
  diacritics removed, prefix indexes for two and three letters. `tasks_by_activity` serves the recent list and the filters with no words.
- **Query:** `title : (…)` first (ranked by FTS5's bm25, then activity), then `words : (…)` (the first message), then the parts, **newest
  first**: SQLite walks the index backwards and stops after 300 candidates, which is what keeps a common word ("re") at about 6 ms on
  ten thousand threads; ranking every match took 57 ms. Recency is the right order for finding a thread you were in. The candidates
  are then loaded as tasks (derived state included) and kept or dropped by the status filter; at most four kittens; one row per task,
  its best reason first (title, then first words, then the newest part).
- **A read on its own connection** (`Store::open_reader`: `query_only`, no migration) on a thread of its own (`neko-search`, like
  `list_branches`): a search never waits for a run that is writing and a run never waits for it. Nothing is written by a search.
- **`search {query, limit?}`** answers `found {query, hits, problem?, took_ms}`; a hit is `{task, kitten?, by (title | message |
  recent), snippet?, part?, seq?}`. The screen already has each task (title, project, status, age); the hit says why it is listed. The
  snippet is one line of plain words (Markdown's `*` and backticks dropped) around the match, cut with "…".
- **Limits, said plainly:** at most 8 words count; 300 candidates per search, so a status filter that keeps few of a very common
  word's matches may list fewer than exist; the filter-only scan reads the 1000 most recently active threads; words that are in
  different messages of one thread do not match together.

## Speed (measured 2026-11-28, a development build, 10 000 threads and 40 000 messages)

`crates/neko-daemon/tests/search.rs` seeds the store as store 13 left it (raw SQL in one transaction), starts a real daemon (which
migrates it: **about 2.5 s** to build both indexes, once), and asks over the socket: a rare word 1 ms, a common word or prefix 6 ms,
`status:needs` 23 ms (it loads up to 1000 tasks and derives each), nothing typed 2 ms, a filtered word 6 ms. The test holds every
query to the spec's 50 ms, best of three (a development build's SQLite is built unoptimised, so a release daemon is faster still).

## In the app (`search.rs`, `search_view.rs`)

- `App::palette` (`Palette`): the field, the highlighted row, the last answer and the text it answers, the request in flight, a
  pending reveal. `palette_poll` (each frame) sends `search` when the text differs from the last one sent (limit 6 for an empty text,
  else 12) and walks a reveal along; `take_found` keeps an answer only if it is for the request in flight, so a slow old answer never
  replaces a newer one. Opening resets everything, so a new palette never shows the last one's rows.
- **Items are built in the view** (`CommandItem` is not `Send`). The daemon's rows carry the typed text as hidden keywords only when
  Command could not match them itself (an accent, a filter); otherwise Command marks the letters in the title or the snippet. Cattains,
  projects and actions are filtered by Command as the text is typed. Row keys are `thread:<id>`, `kitten:<id>`, `cattain:<id>`,
  `project:<id>`, `action:<name>` (`search::Row`).
- **Actions:** New thread, Needs you, Briefing, Rounds, Toggle right pane (when a thread is open), Toggle sidebar, Pause or Resume all
  rounds, Show or Hide archived threads, Show or Fold settled threads, Settings and each of its pages. Pause and Resume, Show and Hide are
  offered as the one that would change something (`App::action_offered`). View ▸ Show archived stays (S8's stopgap): Search finds
  archived threads too, and nothing was removed.
- Tab is taken in the palette's own `on_key_preview`, before the field would pass it on as a move of focus. ⌘K is taken in `on_key`.
- The reveal opens a finished run's fold when the found words are in it (`task_view::in_fold` is the same rule the conversation uses),
  then scrolls to the part's element (`part-<seq>-answer`, `part-<seq>-words`, `task-words`) with Rune's `scroll_into_view`.

## Not here yet

- Searching a thread's tool output, files, branch or PR; settings pages' options (the pages themselves are actions); `>` to force actions.
- The row's project and age are not read by VoiceOver (Command hides its hint from assistive technology); the title and the words
  around a match are. An upstream ask: speak the hint (`rune-ui-in-neko.md`).
- With the sidebar folded the band has no Search button: ⌘K and the menu item are the way.

## Tests and scenes

`crates/neko-core/tests/search.rs` (words, prefixes, quotes, accents, first words, what is and is not indexed, streaming rewrites,
deletion, the migration from store 13, filters, recents, kittens), `crates/neko-daemon/tests/search.rs` (the request over the wire and
the speed), `apps/neko/tests/search.rs` (the keys, the sections in order, a message hit's words, Return, ⌘Return, Tab, stale answers,
the menu item), QA `search-words`, `search-filters`, `search-new-thread` (`qa.md`). Scenes: `search-empty`, `-typing`, `-message`,
`-actions`, `-filter`, `-problem`, `-none`, `-put-away`, `-loading`, `-first-moment`.

Since protocol 21 a Cattain's own thread is never in the threads the daemon finds (`t.cattain = 0` in every query): it is found as
its Cattain, which opens it.

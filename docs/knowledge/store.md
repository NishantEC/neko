# The store (S1, 2026-11-26)

`crates/neko-core`. (Version 19 is a Cattain's crew; version 18 is a Cattain made by asking; version 17 is Cattains; version 16 is questions going up; version 15 is kittens' place, order and drafts; version 14 is search; version 13 is pin, settle and archive.) (The right pane, protocol 15, changed nothing here: the pane's open state is the app's prefs, its overview is derived from checkpoints.) SQLite through `rusqlite` with the `bundled` feature, at `<data>/neko.sqlite3`.

## Records

- `workspaces (id, name, created_ms, folder, removed_ms, position)`. General is id `default` with no folder, made at first open.
  Version 4 (workspaces) adds the folder (unique, links resolved), when it was removed and its place in the sidebar. Its
  migration renames the default row General and makes one workspace per folder tasks were made in (its id from its oldest
  task's, so no clock or randomness is needed), moving those tasks into it; tasks with no folder stay in General.
  See `workspaces.md`.
- `tasks (id, workspace_id, words, title, status, created_ms)`. `words` are the person's words exactly as sent (whitespace and
  all). `title` is the first line with words, spaces collapsed, cut at a word before 80 characters. `status` is the newest run's: `new`, `working`, `needs_you`, `done`,
  `failed`, `stopped`, `interrupted`.
- Version 2 (S1) adds where a task works, `runs`, `usage`, `parts`, `permissions` and `settings`: see `transcript.md`.
- Version 5 (undo) adds `checkpoints (id, task_id, run_id, kind, place, work, shadow, ref, commit_id, tree, head, files, bytes,
  took_ms, made_ms, state)`: one row per checkpoint (`start`, `end`, `before_undo`, `before_redo`; kept in the `repo` or a
  `shadow` repository; `kept` or `removed`), and on `runs` where its undo stands: `undo_state`, `undo_why`, `undo_files`,
  `undo_added`, `undo_removed`, `undo_committed`. Removing a task removes its rows. See `undo.md`.
- Version 6 (cats) adds `cats (id, name, name_key, face, purpose, rules, previous_rules, rules_saved_ms, workspace_id, created_ms,
  retired_ms)` (`name_key` is the name without case, unique), `cat_id` on `tasks`, and `cat_id` and `rules_hash` on `runs`. Its
  migration makes the starter cats when there are tasks and gives each task one, oldest first, by the least-busy rule. A store
  with no cats gets the starters at open, with no event. See `cats.md`.
- Version 7 (rounds) adds `rounds (id, name, words, cat_id, workspace_id, schedule, enabled, anchor_ms, cursor_ms, fail_streak,
  brake_note, created_ms, changed_ms)` (`schedule` is the protocol's JSON; `cursor_ms` is the last due time dealt with),
  `round_runs (id, round_id, due_ms, at_ms, kind, task_id, reason, ended)` (the last 50 per round; removed with the round), and on
  `tasks` `round_id`, `round_due_ms`, `round_name`. The pause is `settings.rounds_paused`. A due time's task, history line and
  cursor are one transaction. See `rounds.md`.
- Version 8 (plugins) adds `plugins (id, name, name_key, kind, folder, manifest, digest, approved_digest, approved_manifest, approved_ms,
  declined_digest, enabled, access_mode, access_cats, last_check, problem, created_ms, changed_ms, manifest_ms)` (`manifest` is the
  manifest's JSON; `digest` its SHA-256; the approved columns are what the person approved, kept when the manifest changes so the
  difference can be shown; `access_cats` is a JSON list of cat ids; `manifest_ms` is when the manifest became what it is, which is how
  long a plugin has waited) and `plugin_values (plugin_id, key, value, secret)` (a secret's row holds no value: it says the secret is set,
  and the secret is in the Keychain; removed with its plugin). Approval is worked out from the digests, never stored. Its events
  (`plugin_added`, `plugin_approved`, …, `plugin_setting`) carry names, digests and keys, never a value. See `plugins.md`.
- Version 9 (threads) adds `last_read_ms`, `activity_ms` and `latest_line` on `tasks`. Its migration reads every task as read as of its last run (or its creation) and fills `latest_line` from the newest agent words. `thread_status`, `needs_you` and `failure_line` are never stored: `Store::derived` works them out each time a task is read (`threads.md`). Events `task_read` and `task_unread` record a change of `last_read_ms`; a part of the agent's words updates `latest_line` and `activity_ms` in its own transaction.
- Version 10 (subagents) adds to `tasks` the columns `parent_id`, `sub_number` and `root_id` (all null for a thread; indexed where set) and
  `wakes (id, task_id, subagent_id, status, summary, files, branch, state, created_ms, delivered_ms)`: one row per report of a subagent
  that ended, `pending` until it was put in front of the thread's agent, then `delivered`, or `discarded` when the thread was stopped.
  A report is stored once (a subagent that ended with none gets one at the next start). `own_mcp` is a `settings` row. Events:
  `task_created` carries the subagent's number; `wake_created`, `wake_delivered`, `wake_discarded`. See `subagents.md`.
- Version 12 (steer, queue, interrupt; protocol 16) adds `queued (id, task_id, words, kind, at_ms)`: the messages waiting above a thread's composer, `kind` `queued`, `steered` or `interrupting` (`composer.md`), in the order of the table's rowid. A thread's rows go with it (`remove_task`); a stop settles them (steered rows are dropped, an interrupt that never went through becomes `queued`). Events: `queued`, `queued_changed`, `queued_left` (with the reason: read, sent, sent now, discarded, run ended, daemon stopped). `Settings.queue_first` is a `settings` row (`queue_first`, `yes`/`no`).
- Version 11 (new thread) adds `tasks.in_folder` and `tasks.from_branch` (work in the project's own folder with no worktree; the local branch a worktree starts from). `cat_id` was already nullable: no data changes and old tasks keep their cat, but nothing assigns one to a new task any more (`cats.md`, `threads.md`).
- Version 19 (a Cattain runs its crew; protocol 23) adds `tasks.started_by` (the Cattain whose agent started the thread with `start_crew_thread`), and to `wakes` `kind` (`subagent`, the default, or `crew`: a crew thread's turn end or request, waking its Cattain; `subagent_id` is then the crew thread) and `about` (the permission a `needs_you` crew wake is about, so each request wakes its Cattain once). Every thread that exists was started by the person or a round, and every wake is a kitten's. Events: `crew_wake_created` beside `wake_created`. See `cats.md`, "The crew".
- Version 18 (a Cattain made by asking; protocol 22) adds `cats.made_in`: the thread whose agent made the cat with Neko's `make_cattain`. None of the cats that exist has one. Events: `cat_added` carries `made_in` for such a cat; `cattain_file_seeded` is its first RULES.md (no previous version). "Agents may make Cattains" is a `settings` row, `cattains_by_hand` (`yes`/`no`; absent reads as on). See `cats.md`, "Made by asking".
- Version 17 (Cattains; protocol 21) adds `cats.thread_id`, `cats.model`, `cats.access` and `tasks.cattain`, and `cattain_files (cat_id, name, content, previous, changed_ms)`: what Neko last saw of each of a Cattain's four files and the version before it (the files themselves are on disk in `<data>/cattains/<id>/`). The migration writes each cat's rules to its RULES.md (with their previous version) and its CREW.md, and empty NOTES.md and DIGEST.md; it makes no thread (`cats.md`). Events: `cattain_file_saved`, `cattain_file_restored`, `cattain_file_changed` (the Cattain wrote it), `cat_model`, `cat_access`. The folders and files are made when the store opens (`Core::ensure_cattains`), so `Core` knows its data folder (the store's own).
- Version 16 (questions go up; protocol 20) adds `runs.ask` (JSON `{to, text, chain, unanswered}`: the question a run put with Neko's tools) and, on `wakes`, `run_id` (the kitten's run a report is about: a kitten resumed with an answer reports again, once per run), `question` and `question_chain`. The migration gives every report already made the id of its kitten's newest run, so none is made twice. Event: `run_asked`. A kitten's question (`SubagentOf.question`) is derived on read, not stored.
- Version 15 (kittens' place, order and drafts; protocol 19) adds `tasks.sub_placement` (`own_copy`, else null), `sub_after` (the sibling's number), `sub_draft` (0 no, 1 a draft, 2 a discarded one) and `sub_branch` (the branch the copy will have). Every subagent that exists works in its parent's folder as before. Events: `draft_started`, `draft_discarded`, `draft_edited`. A discarded draft is stopped and leaves no report (`ended_without_wake` skips it).
- Version 13 (pin, settle, archive; protocol 17) adds `tasks.pinned_ms`, `settled_ms` and `archived_ms` (null: not; else when). Nothing that exists is any of them. Events: `task_pinned`, `task_unpinned`, `task_settled`, `task_unsettled`, `task_archived`, `task_unarchived`. A run starting clears `settled_ms` and `archived_ms` and a run's end or the agent's words clear `settled_ms`, in the same transaction (`records.rs`); `Store::auto_settle` is the week's housekeeping (`shelf.rs`). An archived task is left out of `needs::derive` and its `needs_you` flag is false.
- Version 14 (search; protocol 18) adds two FTS5 tables, `find_titles(title, words)` (a task's title and its first words) and `find_words(text)` (the text of every `person` and `agent` part), each row numbered with the rowid of the row it indexes, filled by the migration from what exists and kept by triggers on `tasks` and `parts` in the same transaction as the change (insert, update, delete; `json_extract` reads the part's `text`). And the index `tasks_by_activity`. Tool output, thinking and notices are not indexed. Both SQLite's FTS5 and JSON are in the bundled build. A reader that only reads opens the file with `Store::open_reader`. See `search.md`.
- The Briefing (protocol 10) adds **no table and no migration**: where the person last read it, the morning switch and time and the morning's cursor are `settings` rows (`briefing_read_ms`, `briefing_morning_on`, `briefing_morning_at`, `briefing_morning_cursor_ms`). The store stayed at version 7 for it (plugins made the eighth migration). See `briefing.md`.
- `events (seq, at_ms, workspace_id, subject, kind, data)`. One row per change, in the same transaction as the change.
  `seq` is the revision the daemon pushes. Triggers refuse `UPDATE` and `DELETE` on it.

Ids are `task_` and 26 base-32 characters: 48 bits of time, 80 random bits (the ULID layout). They sort by time.

## Rules

- One row per entity, never one big JSON row. The earlier project rewrote megabytes on every tick that way.
- A write touches only the rows it changes.
- No clock and no randomness inside logic. `Core` is handed a `Clock` and an `Ids`; tests use `FixedClock` and `SequentialIds`.
- WAL, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout` 5 s, `journal_size_limit` 8 MB. The daemon checkpoints
  (`TRUNCATE`) at a clean shutdown.
- Migrations: `user_version` counts the migrations run; each runs in its own transaction. A migration is SQL, or SQL then a
  step in code (version 4 reads folders on disk). `Store::open_at` stops at an older version, so a test can build a store as an
  older Neko left it. A store from a newer Neko (a higher
  version) is refused, not touched.
- Two connections on one file see each other's writes (tested).

## Tests

`crates/neko-core/tests/workspaces.rs`: the migration from version 3, adding, renaming, removing, a missing folder, locating,
reordering, the tidy rule on real repositories. `crates/neko-core/tests/store.rs`: create and list, refusals, reopen, migration and the newer-version refusal, the migration to
version 5 with a run from version 4, the append-only log, two connections. `crates/neko-core/tests/cats.rs`: the migration to
version 6 from a version-5 store made in the test. `crates/neko-core/tests/rounds.rs`: the scheduler's logic with time handed in, and the migration to version 7 from a version-6 store made in the test. `crates/neko-core/tests/plugins.rs`: the migration to version 8 from a version-7 store made in the test, and the rest of plugins' logic (`plugins.md`). `crates/neko-core/tests/threads.rs`: unread, the latest line, the status, the flag and the migration to version 9 from a version-8 store made in the test. `crates/neko-core/tests/subagents.rs`: subagents' numbering, reports and the migration to version 10 from a version-9 store made in the test (`subagents.md`). `crates/neko-core/tests/threads.rs` also holds the migration to version 11 from a version-10 store made in the test. `crates/neko-core/tests/briefing.rs`: the Briefing's logic, and a store made before it (no briefing rows) opening with its defaults.

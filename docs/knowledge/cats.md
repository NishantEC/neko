# Cats (2026-11-20)

Phase 1 of the Capy-parity plan (the code first called a cat a "captain"; see "Renamed from captains" at the end). Spec: `docs/design/phase-1-cats-2026-11.md` (the rulings block there overrides its body).
This file grows one section per batch; B1 is the records, the files and the requests, B2 the sessions' `cat_id`, the cat's own thread and turns and the wake queue. The first Rune screens are the last section ("Rune: slice 1").

## What a cat is

A named agent that looks after one workspace's crew of sessions. `neko_protocol::cats::Cat`: `id`, `name`, `face`,
`workspace_id`, `area`, `model_policy`, `daily_limit_micro_usd`, `purpose`, `pinned`, `order`, `created_at_ms`, `archived`.
A cat is tied to its workspace for life (`workspace_id` is not in `Update`). Archiving hides it and unpins it; nothing is
deleted (its row and files stay; there is no restore in Phase 1).

## B1 storage

- Table `cats (id TEXT PRIMARY KEY, body TEXT NOT NULL)`, one JSON row per cat, created by `Db::migrate` with
  `CREATE TABLE IF NOT EXISTS`, so a fresh install and an upgrade take the same path (both tested: the upgrade test drops the
  table from a real database file and reopens it). Written only through `Db::put_cat` / `Db::cat_rows`; multi-row
  changes (`create`, `update`, `reorder`) run inside `Db::atomic`, so a failed file write rolls the row back.
- This differs from the spec body (settings rows plus a `cats_rev` counter, G6): the batch brief asked for a table of its own.
  Like `usage_events`, nothing `workbench::load` reads lives there, so a cat write does not bump `data_revision` and wakes
  no reader, and `Snapshot` has no `cats` field yet. A client reads cats with `Cats(List)`. When B2 needs clients
  to hear about changes, bump a revision there (do not put the records in the snapshot blob).
- `neko_core::cats` holds the rules and never reads the clock, makes ids or finds the data folder (callers pass them in).
  `neko-daemon/src/cats.rs::handle_in(state, root, command)` is the testable form; `handle` passes `support_dir()`.
  The daemon's `bin/*_harness.rs` include the module by path like the other handlers.

## Limits (brief, not the spec body's smaller ones)

Name 1 to 40 characters, trimmed, no control characters, unique among non-archived cats ignoring case (an archived name is
free again). Area at most 40. Purpose at most 2,000 characters, no NUL. `daily_limit_micro_usd` is `None` or 1 to
1,000,000,000,000. At most 16 active cats. `workspace_id` must exist. `model_policy` goes through
`runtime_selection::validate_preferences`. Face must be one of `neko_protocol::cats::FACE_PRESETS` (the one list:
`black-glasses`, `persian-glasses`, `tabby-headphones`, `grey-scarf`, `cream-bowtie`, `calico-welding`; the clients draw them
as vectors in code, the Figma PNGs wait for the licence decision, R5). An empty face on `Create` takes
`cats::default_face(name)` (FNV hash of the lowercase name modulo six; a test pins one value so faces do not shuffle).
This departs from G3 (free string): an unknown id is refused.

## Requests

`Request::Cats(CatCommand)` answers `Response::Cats(Box<CatsReply>)` (`Listing`, `Suggestion`, `Files`) or `Error`.
`List { include_archived }` (active by `order`, archived last), `Create`, `Update` (only given fields; `daily_limit_micro_usd:
null` removes the limit, a missing field leaves it), `Archive` (clients confirm), `Pin`, `Reorder` (ids must be exactly the
active set, else "The list changed; reload and try again"), `Suggest` (busiest workspace of the last 30 days, first free cat
name, a read that creates nothing), `Files { id }`, `WriteFile { id, kind, text }`. Create, Update, Archive, Pin and Reorder
answer with the active list. Update and Pin of an archived cat are refused. Not in B1: `Overview`, `Summaries`, `Assign`,
`Conversation`, `Send`, `Needs`, `NeedLater`, `NeedDismiss` (B2, B3).

## Files

`<data>/cats/<id>/{RULES.md, TASK_RULES.md, PLAN.md, BRIEFING.md}` (`neko_core::cat_files`). `Create` makes the folder (0700)
and seeds `RULES.md` with `# Rules` plus the purpose, and the other three with one heading (`# Task rules`, `# Plan`, `# Briefing`).
A file that is only its heading is empty for prompts: use `cat_files::body(text)` (`task_rules_section` and `prompt_body` do). Writes: at most
16 KB ("Keep it under 16 KB"), no NUL, the replaced text kept as `<name>.prev` on every write (temp file plus rename), empty or
whitespace text removes the file after the copy. One process-wide mutex serialises writes. A hand-edited file over 16 KB is read
cut at 16 KB on a character boundary with `truncated: true`. Confinement: the folder comes from a validated id
(`artifacts::valid_id`: letters, digits, `-`, `_`), file names from `CatFileKind`; a symlink as the cat's folder or as a
file is refused, and the folder is checked to canonicalise inside `cats/`. `Files` and `WriteFile` need the cat's row to
exist (archived is fine). B2 injects them (below).

Tests take the data folder as an argument and use temp folders, so none touches `NEKO_DATA_DIR` or the live data.

# B2: sessions, the thread, turns and wakes

Rulings R1 to R3 of the spec decide the shape: the cat thread is **not** the Home chat engine (nothing here reads or writes
`neko_chat_v2` or `Snapshot::conversation`; a test pins both unchanged after a turn), the wake queue is a small table of its own,
and a cat's cost needs no ledger migration.

## Sessions carry the cat

`Task::cat_id: Option<String>` (`neko-protocol`, `serde(default, skip_serializing_if = "Option::is_none")`): stored in the task row
JSON like every other field, so rows from before it load with `None` and a catless task writes the same bytes as before (tested).
`CrewRole::Cat { cat_id, person_turn }` is the role of a cat's lease. Its tools (`neko_protocol::crew`): `list_tasks`,
`task_status`, `create_task`, `steer_task`, `checkpoint_diff`, `cat_write` always; `wait_for` and `cancel_task` only in a **person
turn** (a wake must end quickly and cannot cancel on what a session said). No `schedule`, artifact, undo or merge tool exists for a
cat. `cat_write` (new, 12th crew tool, schema in `mcp_host/bridge.rs`) is the cat's own notes: `plan` and `briefing` in any
turn, `rules` and `task_rules` only in a person turn (text a wake read from a session is untrusted and must never rewrite what the person
trusts the cat to follow); the full chat, the read-only chat and ticket runs do not have it.

A cat sees only its own crew (`crew.rs::visible`: `task.cat_id == cat`): another cat's session and a catless one
answer "No task ... that this run can see" to `task_status`, `steer_task`, `cancel_task`, `checkpoint_diff`, and `list_tasks` omits them.
`create_task` from a cat: the workspace is forced to the cat's (a different `workspace_id` is refused, "A cat only looks
after its own workspace"), an archived cat is refused, the task gets `cat_id`, its delegation origin is `cat:<id>` (so the cap of 8
open sessions counts per cat), and it builds without waiting for plan approval only in a person turn; a wake turn's session waits for
the person to approve its plan. The session's request is not stored as `Task::request` (an agent's words are never presented as the
person's). Its first change counts as news: `cat_thread::note_new_session` records its fingerprint at creation.
Not in this batch: `Assign` (Move to / Remove from crew), `RoutinePolicy::cat_id`, a per-session `model` argument, goals attached by
a cat, `ask_person` (B3), the cat's own daily limit (B3).

## TASK_RULES.md reaches its sessions

`Controller::rules_for(snapshot, task)` = the rulebook plus `cat_files::task_rules_section(root, task.cat_id)`: exactly
`"\n\nRules from <cat name>\n" + body.trim_end() + "\n"`, and nothing for a catless session or an empty `TASK_RULES.md` (a heading-only file is
empty), so every prompt and golden is byte-identical without a cat. It replaces `rulebook::text` at the one place the ticket worker
builds its `rules` (scout, supervisor, builder, reviewer, repair and goal turns all use that variable). Routine runs do not get it yet.

## The thread

Table `cat_messages (cat_id, seq, role person|cat|event, text, at_ms)`, primary key `(cat_id, seq)`, append-only: `Db::append_cat_message`
numbers from 1 per cat in one statement; nothing edits or deletes a row. A stored reply is cut at 32 KB; the person's message is at most 8,000
characters (`MAX_SAY_CHARS`). `neko_core::cat_thread`: `say`, `append`, `view`, `recent`.

Requests (inside `Request::Cats`, so they share `Response::Cats` and the B1 handler): `CatThread { cat, since_seq }` answers
`Thread(CatThreadView { cat_id, messages, last_seq, running })` (`since_seq: 0` gives the newest 200, otherwise what is newer, oldest first);
`CatSay { cat, text }` stores the person's message and queues its wake in one transaction, answers `Thread` holding that message, and refuses an
archived cat, empty text and text over the limit. An archived cat's thread stays readable.

## The change signal

`Snapshot::cats_rev: u64` (skipped when 0) is read for readers by `workbench::load` from the settings row `neko_cats_rev_v1` and is never written into
the snapshot row (`encode` zeroes it). `cat_thread::bump_rev` writes that row with `set_setting`, which bumps the daemon's `data_revision` and the load
cache's stamp, so a held `WorkbenchIfChanged` wakes, sees a different digest and answers. Bumped on every thread write, when a turn starts and ends (so
`running` is fresh), and on a cat file write (`WriteFile`, `cat_write`). It is a row, not a process-wide counter, so two databases never share it and a
test comparing full snapshots is not raced; it is never bumped by a timer. The reply to `CatThread` is the data; the counter is only "something changed, ask
again". Note the wake queue and the seen-fingerprint rows do not bump it (nothing a client shows).

## Wakes

Table `cat_wakes (id autoincrement, cat_id, reason turn_finished|failed|needs_you|person_message, task_id, at_ms)`. Written when the person posts
(`say`) and when a session of the cat finishes a turn, fails, or needs the person. **How the session events are found (G11, accepted by R6):** by diffing,
not by hooks in the many status writers (`task.status =` is set in about forty places, including recovery, goals, pull requests and publish, and a hook in each
would miss one). Every cat pass (3 s, `neko-cats` thread) calls `cat_thread::notice_sessions`: it fingerprints each of the cat's sessions
(`status`, goal turns, goal blocked; never tool events or clocks, so a chatty run wakes nobody), compares with the row `neko_cat_seen_v1:<id>`
(`set_setting_quiet`, rewritten only when something changed) and queues a wake per change in one transaction. A session seen for the first time wakes nobody (an
upgrade never storms); the cat's own `create_task` records its fingerprint first. `diff`: rest to `Failed` is `failed`; to `AwaitingApproval` or a goal that
blocks on a question is `needs_you`; a working status (Queued, Planning, Building, Reviewing) to `ReadyForReview` or `Completed`, or a goal's turn count rising, is
`turn_finished`. At most 200 wakes wait per cat (the oldest go first).

## Turns

`workbench/cats.rs`. `cat_plan(now_ms, health)` (a pure-ish pass over the database and the controller's slots) returns the turns to start and takes each
cat's slot in `Controller::cat_active`; `cat_turn(launch, runner)` runs one. The real pass spawns each turn on its own thread with `run_agent`; tests call
the same two functions with a fake runner. Launch rules, all of which must hold per cat: it is not archived (else its wakes are dropped); wakes are pending; **no turn of
this cat is running** (so two never overlap, and a message that arrives meanwhile waits, taken by the next turn); the machine steward is not `holding` and, under a `cap`,
fewer agents run than the cap (ticket workers, the Home chat and cat turns all count; a cat turn takes no admission slot itself, G27); the global daily spend ceiling is
not reached (`spend::today`, which now includes cat turns in its automation line); and for an unattended batch the queue rules `launch_gate`: the oldest wake at least 20 s old
(a burst becomes one turn), at least 60 s since the last turn, fewer than 40 turns today. The last two read the usage ledger's `cat:<id>` lines, so there is no counter to drift.
A batch with the person's message in it is never debounced or capped (it still waits for the steward and the ceiling). Several pending wakes coalesce (`coalesce`) into ONE
turn: every pending wake is taken when the turn starts, repeats of the same (reason, session) fold, and wakes written while it runs wait.

The turn: build the prompt (`cat_prompt::prompt`, pure, golden `tests/prompts/cat-chat.md` for a person turn and `cat-wake.md` for a wake: RULES.md, PLAN.md and TASK_RULES.md bodies,
BRIEFING.md, the last 30 messages, up to 20 of its sessions with their states, the crew tools paragraph, and for a wake the events inside an "untrusted data" sentence), run the agent
(read-only, in the cat's workspace repository, `native_runner::run_with_bridge_streaming`, a lease `cat:<id>:<turn>` with `CrewRole::Cat` and no connection tools; the
runtime is the cat's `model_policy.preferences` previewed against the stored default), append the reply as a `cat` message, record usage. A reply of exactly `QUIET` (or
empty) from an unattended turn is not stored (the thread is append-only, so the exchange is simply never written); the person's turn always gets a reply. A failed turn appends one
`event` line ("I could not take this turn: ...") so nobody waits on silence. Not done: the Claude to Codex fallback that routines have when the login goes bad.

Usage (R3): `usage_ledger::record_cat_turn` writes one line per turn that ran an agent (`run_key` `cat:<id>:<started_ms>`, `task_id` `cat:<id>`, `kind` `cat`,
`reported` only when the agent reported a cost), failed or not. No migration. `spend::routines_usd_since` adds `kind = cat` lines, so cat turns count against the daily ceiling.
`Controller::busy()` counts a running cat turn like a chat reply.

## Tests

`neko-core`: `cat_thread` (thread, wakes, gate, diff, notice, change counter, migration, ledger line), `cat_files` (`task_rules_section`, `prompt_body`), `cat_prompt`, prompt goldens.
`neko-protocol`: the role table. `neko-daemon`: `workbench/cats_tests.rs` (every item of the batch brief) and the handler tests in `cats.rs`.

# Renamed from captains (2026-11-21)

Neko's own words (`docs/design/terminology.md`) say cat, not captain. One commit renamed the B1 and B2 code with no change in behaviour: the types
(`Cat`, `CatCommand`, `CatThread`, `CatSay`, `CrewRole::Cat`, `Request::Cats`, `Snapshot::cats_rev`, `Task::cat_id`), the modules (`cats`, `cat_files`, `cat_thread`,
`cat_prompt`, `workbench/cats.rs`), the crew tool (`captain_write` is `cat_write`; its `file` is `plan`, `briefing`, `rules` or `task_rules`) and the golden
prompts (`cat-chat.md`, `cat-wake.md`; the only diff is the wording: cat, "task rules", "briefing"). The file kinds are `CatFileKind::{Rules, TaskRules, Plan, Briefing}`.
The heading every task a cat starts is given is now `Rules from <cat name>` (it was "Your captain's rules"; `task_rules_section(root, cat_id, cat_name)`).

Everything already stored is moved, and every move is safe to repeat:

- **Tables** (`Db::migrate`, `migrate_captains_to_cats`, one transaction). `captains` to `cats`, `captain_messages` to `cat_messages` (role `captain` becomes `cat`),
  `captain_wakes` to `cat_wakes` (ids kept, so the sequence goes on), then the old tables are dropped. Each table is checked on its own, so a database from before B1 (none),
  with only B1 (`captains`) or with B1 and B2 all end the same; a fresh one makes only the new tables. Tests in `cats.rs` cover all four.
- **Settings rows.** `neko_captains_rev_v1` to `neko_cats_rev_v1`, `neko_captain_seen_v1:<id>` to `neko_cat_seen_v1:<id>`, and the delegation origins in `neko_crew_origin_v1`
  (`captain:<id>` to `cat:<id>`, so the cap of open sessions still counts a cat's earlier sessions).
- **Usage ledger.** `captain:<id>` in `run_key` and `task_id`, and `kind = captain`, become `cat:<id>` and `cat`, so the history and the daily ceiling keep counting a cat's turns.
- **Task rows.** `Task::cat_id` reads the old `captain_id` field (`serde(alias)`) and writes `cat_id`.
- **Files** (`cat_files::migrate_legacy(<data>)`, called once at startup in `neko-daemon/src/main.rs`, before anything reads a cat's files). `<data>/captains/<id>/` becomes
  `<data>/cats/<id>/`; `CREW.md` becomes `TASK_RULES.md` and `DIGEST.md` becomes `BRIEFING.md`, their `.prev` copies with them. It never overwrites and never deletes: a file whose new name is
  taken, a link and a clashing file stay where they are (one log line each, `cats: ...`), and the old folder is removed only if the move emptied it. A file that is exactly its old seed
  (`# Crew rules`, `# Digest`) takes the new heading; anything the person or the cat wrote is moved as it was.

**Admission (2026-11-21).** The cat tick no longer checks the steward and the ceiling itself: it asks `admission::may_start(CatTurn, ..)` and a held turn leaves one `Held:` Event line in the thread. See `docs/knowledge/admission-gate.md`.

# Rune: slice 1, your cats appear (2026-11-21)

The first screens for cats, in the Rune app (`apps/neko-rune`), after the approved prototype (`~/Documents/neko-design/prototype-2026-11-21`, `?p=cat&c=miso`). The daemon side is unchanged. Words are Neko's own: cats, tasks, rounds, Rules, Task rules, Plan, Briefing. The cat's conversation, Needs you items, rounds and the Briefing are later slices; this one is the cats themselves.

- **Faces** (`src/cat_face.rs`). The six presets of `FACE_PRESETS` (the ids are tested equal to the daemon's list) are flat vector cats: the approved mockups' cat (ears, head, muzzle, eyes, nose, whiskers, one 2.5 unit ink line) with a coat, eyes and one prop each (glasses, headphones, scarf, bow tie, welding helmet), on a rounded-square tile (radius 26% of the side, so `rx 16.64` on the 64 unit grid). Each face is an `rune::Icon` whose body names its own colours, so Rune rasterises it in colour with resvg and draws it as an image at 16, 20, 24, 36 or 44 points (`rune/src/text/color_icon.rs`, which needs the `svg` feature, on by default); the same face is the same in light and dark. Flat fills only (no gradients, glows or emoji: tests). The Figma PNGs still wait for the licence decision (R5). Two marks: an amber dot at the top right (from 20 points; a cat has a task waiting for approval) and a block of dots at the bottom right (from 26 points; a cat has tasks running), both ringed in the colour the face sits on. They are read from `Summary::meta[task].cat_id` (`TaskMeta::cat_id`, new), so they appear as soon as a cat's task exists.
- **State** (`src/cats.rs`). `App::cats` holds the active list (read with `Cats(List)`, never including archived), the open cat, the panel's tab, the sheet, the four files and the editor. The list is asked for with the first snapshot and again whenever `Summary::cats_rev` (new, from `Snapshot::cats_rev`) differs from the one it was read at; an open Files tab is read again then too. Every write answers with the active list and the screens use that answer. Pure rules are tested in the module: `display` (up to three pinned cats as faces in order, a fourth pinned one is a row), `reordered` (a drag moves a cat within its own group only), `micro_from_dollars` / `dollars_text` (the daily limit is typed in dollars, 0 refused because it would stop the cat), `field_of` (which field a daemon refusal belongs to, from its words), the 16 KB check.
- **Sidebar** (`src/view/cats.rs::section`). A "Cats" header with a plus at the top of the scrolling list (above Needs you), then up to three pinned cats as 44 point faces with names in a row of three equal cells, then every other cat as a 36 point row (20 point face, name, area at the right). The open cat is on the selected fill. No cats: "No cats yet" and a "New cat" button. Drag a face along the row or a row up and down to reorder (`App::cat_drag`, whole places, within its group, sends `Reorder` with every active id in the order drawn; a press that ends a drag is not a click); the right-click menu has Open, Edit, Pin or Unpin, Move up, Move down and Archive, so nothing is drag-only.
- **New cat sheet** (`mount_sheet`). A live preview row, the six faces (the next face nobody wears is chosen first), name, Looks after (workspace select, fixed once the cat exists, and area), model (Neko's default or any runtime and model `Models` lists), daily limit (optional, dollars) and purpose. With no cats it first asks `Suggest` and fills an untouched sheet with the busiest workspace's proposal. Create sends `Cats(Create)`; the answer closes the sheet and opens the new cat's page. What this client can refuse it refuses under the field without asking (no name, a limit that is not a dollar amount); the daemon's refusals land under the field their words name (`field_of`), or on a line of their own. Edit is the same sheet filled in; `Update` carries only what changed and an emptied limit is `Some(None)`.
- **Cat page** (`page`, `panel`). The middle has the header (face 22, name, "area · workspace", "$0.00 today", a menu with Edit, Pin or Unpin, Archive, and the panel toggle) and one quiet line where the conversation will be ("Talk to miso here. This comes in the next build."). The cost is the one number the ledger can give: it does not break a day down by cat yet. The panel sits in the context panel's slot (opened with the page, closed when the person leaves it; the Neko column folds by the usual layout pressure) with the tabs Overview and Files. Overview has Needs you, Working, Ready to merge and Rounds, each with its own honest empty line; tasks of the cat (by `cat_id`) are listed where they are, rounds are not tied to a cat yet. Files lists Rules, Task rules, Plan and Briefing and the purpose; a file opens in a mono editor with Save (⌘S), Revert, "Unsaved changes", the 16 KB message (it is not sent over the limit) and the daemon's own error. Leaving an unsaved file (another file, the tab, another cat or page) asks "Discard your changes?" through the rulebook's guard (`extra.rs::guard`, `Going::Cat*`). Saving empty text clears the file and asks first.
- **Requests and the allow-list.** `Call::Cats(CatCall)` (`link/cats.rs`) is one `CatCommand` each of List, Create, Update, Archive, Pin, Reorder, Suggest, Files, WriteFile (not the thread). The web does not send them, so `tests/allow_list.rs` names them (`CAT_REQUESTS`, and `Cats` in `RUNE_EXTRAS`) and checks they are exactly the protocol's but the thread. `CatCall::Archive` and an emptying `WriteFile` are built only inside `confirm(Ask {` in `src/cats.rs` (a scan). Two icons were added to the set: pin and archive.
- **Tests and pictures.** `tests/cats.rs` (25: the sidebar from a fake List, marks, the empty state, the first-cat suggestion, `cats_rev`, create and its fields, the model, inline refusals, client-side refusals, edit sending only what changed, the page and panel, the overview's empty lines, leaving the page, the files tab, saving by button and ⌘S, revert, the unsaved guard, 16 KB, clearing, archive's confirm, pin, drag of a row and of a face, the menu's moves, screen reader labels); the scenes `cats-0`, `cats-1`, `cats-3`, `cats-5`, `cat-sheet`, `cat-sheet-errors`, `cat-page`, `cat-files`, `cat-editor`, `cat-editor-dirty`, `cat-archive` (`src/scenes_cats.rs`) are crawled by `tests/structure.rs` at three window sizes. Snapshots: `apps/neko-rune/scripts/snap.sh /tmp/neko-slice1 cats-3 cat-sheet cat-page cat-editor`.
- **Differs from the prototype.** The prototype's faces are illustrated PNGs and its sidebar nests a cat's tasks under the open cat and has a Rounds tab; here the faces are vectors (licence), tasks stay in the sessions list below, and the panel has Overview and Files only. The cat page has no composer yet.

# Captains (2026-11-20)

Phase 1 of the Capy-parity plan. Spec: `docs/design/phase-1-captains-2026-11.md` (the rulings block there overrides its body).
This file grows one section per batch; B1 is the records, the files and the requests, B2 the sessions' `captain_id`, the captain's own thread and turns and the wake queue. No UI yet.

## What a captain is

A named agent that looks after one workspace's crew of sessions. `neko_protocol::captains::Captain`: `id`, `name`, `face`,
`workspace_id`, `area`, `model_policy`, `daily_limit_micro_usd`, `purpose`, `pinned`, `order`, `created_at_ms`, `archived`.
A captain is tied to its workspace for life (`workspace_id` is not in `Update`). Archiving hides it and unpins it; nothing is
deleted (its row and files stay; there is no restore in Phase 1).

## B1 storage

- Table `captains (id TEXT PRIMARY KEY, body TEXT NOT NULL)`, one JSON row per captain, created by `Db::migrate` with
  `CREATE TABLE IF NOT EXISTS`, so a fresh install and an upgrade take the same path (both tested: the upgrade test drops the
  table from a real database file and reopens it). Written only through `Db::put_captain` / `Db::captain_rows`; multi-row
  changes (`create`, `update`, `reorder`) run inside `Db::atomic`, so a failed file write rolls the row back.
- This differs from the spec body (settings rows plus a `captains_rev` counter, G6): the batch brief asked for a table of its own.
  Like `usage_events`, nothing `workbench::load` reads lives there, so a captain write does not bump `data_revision` and wakes
  no reader, and `Snapshot` has no `captains` field yet. A client reads captains with `Captains(List)`. When B2 needs clients
  to hear about changes, bump a revision there (do not put the records in the snapshot blob).
- `neko_core::captains` holds the rules and never reads the clock, makes ids or finds the data folder (callers pass them in).
  `neko-daemon/src/captains.rs::handle_in(state, root, command)` is the testable form; `handle` passes `support_dir()`.
  The daemon's `bin/*_harness.rs` include the module by path like the other handlers.

## Limits (brief, not the spec body's smaller ones)

Name 1 to 40 characters, trimmed, no control characters, unique among non-archived captains ignoring case (an archived name is
free again). Area at most 40. Purpose at most 2,000 characters, no NUL. `daily_limit_micro_usd` is `None` or 1 to
1,000,000,000,000. At most 16 active captains. `workspace_id` must exist. `model_policy` goes through
`runtime_selection::validate_preferences`. Face must be one of `neko_protocol::captains::FACE_PRESETS` (the one list:
`black-glasses`, `persian-glasses`, `tabby-headphones`, `grey-scarf`, `cream-bowtie`, `calico-welding`; the clients draw them
as vectors in code, the Figma PNGs wait for the licence decision, R5). An empty face on `Create` takes
`captains::default_face(name)` (FNV hash of the lowercase name modulo six; a test pins one value so faces do not shuffle).
This departs from G3 (free string): an unknown id is refused.

## Requests

`Request::Captains(CaptainCommand)` answers `Response::Captains(Box<CaptainsReply>)` (`Listing`, `Suggestion`, `Files`) or `Error`.
`List { include_archived }` (active by `order`, archived last), `Create`, `Update` (only given fields; `daily_limit_micro_usd:
null` removes the limit, a missing field leaves it), `Archive` (clients confirm), `Pin`, `Reorder` (ids must be exactly the
active set, else "The list changed; reload and try again"), `Suggest` (busiest workspace of the last 30 days, first free cat
name, a read that creates nothing), `Files { id }`, `WriteFile { id, kind, text }`. Create, Update, Archive, Pin and Reorder
answer with the active list. Update and Pin of an archived captain are refused. Not in B1: `Overview`, `Summaries`, `Assign`,
`Conversation`, `Send`, `Needs`, `NeedLater`, `NeedDismiss` (B2, B3).

## Files

`<data>/captains/<id>/{RULES.md, CREW.md, PLAN.md, DIGEST.md}` (`neko_core::captain_files`). `Create` makes the folder (0700)
and seeds `RULES.md` with `# Rules` plus the purpose, and the other three with one heading (`# Crew rules`, `# Plan`, `# Digest`).
A file that is only its heading is empty for prompts: use `captain_files::body(text)` (`crew_section` and `prompt_body` do). Writes: at most
16 KB ("Keep it under 16 KB"), no NUL, the replaced text kept as `<name>.prev` on every write (temp file plus rename), empty or
whitespace text removes the file after the copy. One process-wide mutex serialises writes. A hand-edited file over 16 KB is read
cut at 16 KB on a character boundary with `truncated: true`. Confinement: the folder comes from a validated id
(`artifacts::valid_id`: letters, digits, `-`, `_`), file names from `CaptainFileKind`; a symlink as the captain's folder or as a
file is refused, and the folder is checked to canonicalise inside `captains/`. `Files` and `WriteFile` need the captain's row to
exist (archived is fine). B2 injects them (below).

Tests take the data folder as an argument and use temp folders, so none touches `NEKO_DATA_DIR` or the live data.

# B2: sessions, the thread, turns and wakes

Rulings R1 to R3 of the spec decide the shape: the captain thread is **not** the Home chat engine (nothing here reads or writes
`neko_chat_v2` or `Snapshot::conversation`; a test pins both unchanged after a turn), the wake queue is a small table of its own,
and a captain's cost needs no ledger migration.

## Sessions carry the captain

`Task::captain_id: Option<String>` (`neko-protocol`, `serde(default, skip_serializing_if = "Option::is_none")`): stored in the task row
JSON like every other field, so rows from before it load with `None` and a captainless task writes the same bytes as before (tested).
`CrewRole::Captain { captain_id, person_turn }` is the role of a captain's lease. Its tools (`neko_protocol::crew`): `list_tasks`,
`task_status`, `create_task`, `steer_task`, `checkpoint_diff`, `captain_write` always; `wait_for` and `cancel_task` only in a **person
turn** (a wake must end quickly and cannot cancel on what a session said). No `schedule`, artifact, undo or merge tool exists for a
captain. `captain_write` (new, 12th crew tool, schema in `mcp_host/bridge.rs`) is the captain's own notes: `plan` and `digest` in any
turn, `rules` and `crew` only in a person turn (text a wake read from a session is untrusted and must never rewrite what the person
trusts the captain to follow); the full chat, the read-only chat and ticket runs do not have it.

A captain sees only its own crew (`crew.rs::visible`: `task.captain_id == captain`): another captain's session and a captainless one
answer "No task ... that this run can see" to `task_status`, `steer_task`, `cancel_task`, `checkpoint_diff`, and `list_tasks` omits them.
`create_task` from a captain: the workspace is forced to the captain's (a different `workspace_id` is refused, "A captain only looks
after its own workspace"), an archived captain is refused, the task gets `captain_id`, its delegation origin is `captain:<id>` (so the cap of 8
open sessions counts per captain), and it builds without waiting for plan approval only in a person turn; a wake turn's session waits for
the person to approve its plan. The session's request is not stored as `Task::request` (an agent's words are never presented as the
person's). Its first change counts as news: `captain_thread::note_new_session` records its fingerprint at creation.
Not in this batch: `Assign` (Move to / Remove from crew), `RoutinePolicy::captain_id`, a per-session `model` argument, goals attached by
a captain, `ask_person` (B3), the captain's own daily limit (B3).

## CREW.md reaches its sessions

`Controller::rules_for(snapshot, task)` = the rulebook plus `captain_files::crew_section(root, task.captain_id)`: exactly
`"\n\nYour captain's rules\n" + body.trim_end() + "\n"`, and nothing for a captainless session or an empty `CREW.md` (a heading-only file is
empty), so every prompt and golden is byte-identical without a captain. It replaces `rulebook::text` at the one place the ticket worker
builds its `rules` (scout, supervisor, builder, reviewer, repair and goal turns all use that variable). Routine runs do not get it yet.

## The thread

Table `captain_messages (captain_id, seq, role person|captain|event, text, at_ms)`, primary key `(captain_id, seq)`, append-only: `Db::append_captain_message`
numbers from 1 per captain in one statement; nothing edits or deletes a row. A stored reply is cut at 32 KB; the person's message is at most 8,000
characters (`MAX_SAY_CHARS`). `neko_core::captain_thread`: `say`, `append`, `view`, `recent`.

Requests (inside `Request::Captains`, so they share `Response::Captains` and the B1 handler): `CaptainThread { captain, since_seq }` answers
`Thread(CaptainThreadView { captain_id, messages, last_seq, running })` (`since_seq: 0` gives the newest 200, otherwise what is newer, oldest first);
`CaptainSay { captain, text }` stores the person's message and queues its wake in one transaction, answers `Thread` holding that message, and refuses an
archived captain, empty text and text over the limit. An archived captain's thread stays readable.

## The change signal

`Snapshot::captains_rev: u64` (skipped when 0) is read for readers by `workbench::load` from the settings row `neko_captains_rev_v1` and is never written into
the snapshot row (`encode` zeroes it). `captain_thread::bump_rev` writes that row with `set_setting`, which bumps the daemon's `data_revision` and the load
cache's stamp, so a held `WorkbenchIfChanged` wakes, sees a different digest and answers. Bumped on every thread write, when a turn starts and ends (so
`running` is fresh), and on a captain file write (`WriteFile`, `captain_write`). It is a row, not a process-wide counter, so two databases never share it and a
test comparing full snapshots is not raced; it is never bumped by a timer. The reply to `CaptainThread` is the data; the counter is only "something changed, ask
again". Note the wake queue and the seen-fingerprint rows do not bump it (nothing a client shows).

## Wakes

Table `captain_wakes (id autoincrement, captain_id, reason turn_finished|failed|needs_you|person_message, task_id, at_ms)`. Written when the person posts
(`say`) and when a session of the captain finishes a turn, fails, or needs the person. **How the session events are found (G11, accepted by R6):** by diffing,
not by hooks in the many status writers (`task.status =` is set in about forty places, including recovery, goals, pull requests and publish, and a hook in each
would miss one). Every captain pass (3 s, `neko-captains` thread) calls `captain_thread::notice_sessions`: it fingerprints each of the captain's sessions
(`status`, goal turns, goal blocked; never tool events or clocks, so a chatty run wakes nobody), compares with the row `neko_captain_seen_v1:<id>`
(`set_setting_quiet`, rewritten only when something changed) and queues a wake per change in one transaction. A session seen for the first time wakes nobody (an
upgrade never storms); the captain's own `create_task` records its fingerprint first. `diff`: rest to `Failed` is `failed`; to `AwaitingApproval` or a goal that
blocks on a question is `needs_you`; a working status (Queued, Planning, Building, Reviewing) to `ReadyForReview` or `Completed`, or a goal's turn count rising, is
`turn_finished`. At most 200 wakes wait per captain (the oldest go first).

## Turns

`workbench/captains.rs`. `captain_plan(now_ms, health)` (a pure-ish pass over the database and the controller's slots) returns the turns to start and takes each
captain's slot in `Controller::captain_active`; `captain_turn(launch, runner)` runs one. The real pass spawns each turn on its own thread with `run_agent`; tests call
the same two functions with a fake runner. Launch rules, all of which must hold per captain: it is not archived (else its wakes are dropped); wakes are pending; **no turn of
this captain is running** (so two never overlap, and a message that arrives meanwhile waits, taken by the next turn); the machine steward is not `holding` and, under a `cap`,
fewer agents run than the cap (ticket workers, the Home chat and captain turns all count; a captain turn takes no admission slot itself, G27); the global daily spend ceiling is
not reached (`spend::today`, which now includes captain turns in its automation line); and for an unattended batch the queue rules `launch_gate`: the oldest wake at least 20 s old
(a burst becomes one turn), at least 60 s since the last turn, fewer than 40 turns today. The last two read the usage ledger's `captain:<id>` lines, so there is no counter to drift.
A batch with the person's message in it is never debounced or capped (it still waits for the steward and the ceiling). Several pending wakes coalesce (`coalesce`) into ONE
turn: every pending wake is taken when the turn starts, repeats of the same (reason, session) fold, and wakes written while it runs wait.

The turn: build the prompt (`captain_prompt::prompt`, pure, golden `tests/prompts/captain-chat.md` for a person turn and `captain-wake.md` for a wake: RULES.md, PLAN.md and CREW.md bodies,
DIGEST.md, the last 30 messages, up to 20 of its sessions with their states, the crew tools paragraph, and for a wake the events inside an "untrusted data" sentence), run the agent
(read-only, in the captain's workspace repository, `native_runner::run_with_bridge_streaming`, a lease `captain:<id>:<turn>` with `CrewRole::Captain` and no connection tools; the
runtime is the captain's `model_policy.preferences` previewed against the stored default), append the reply as a `captain` message, record usage. A reply of exactly `QUIET` (or
empty) from an unattended turn is not stored (the thread is append-only, so the exchange is simply never written); the person's turn always gets a reply. A failed turn appends one
`event` line ("I could not take this turn: ...") so nobody waits on silence. Not done: the Claude to Codex fallback that routines have when the login goes bad.

Usage (R3): `usage_ledger::record_captain_turn` writes one line per turn that ran an agent (`run_key` `captain:<id>:<started_ms>`, `task_id` `captain:<id>`, `kind` `captain`,
`reported` only when the agent reported a cost), failed or not. No migration. `spend::routines_usd_since` adds `kind = captain` lines, so captain turns count against the daily ceiling.
`Controller::busy()` counts a running captain turn like a chat reply.

## Tests

`neko-core`: `captain_thread` (thread, wakes, gate, diff, notice, change counter, migration, ledger line), `captain_files` (`crew_section`, `prompt_body`), `captain_prompt`, prompt goldens.
`neko-protocol`: the role table. `neko-daemon`: `workbench/captains_tests.rs` (every item of the batch brief) and the handler tests in `captains.rs`.

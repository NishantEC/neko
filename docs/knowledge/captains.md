# Captains (2026-11-20)

Phase 1 of the Capy-parity plan. Spec: `docs/design/phase-1-captains-2026-11.md` (the rulings block there overrides its body).
This file grows one section per batch; B1 is the records, the files and the requests. No UI yet.

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
A file that is only its heading is empty for prompts: use `captain_files::body(text)` (B2's `crew_section` must). Writes: at most
16 KB ("Keep it under 16 KB"), no NUL, the replaced text kept as `<name>.prev` on every write (temp file plus rename), empty or
whitespace text removes the file after the copy. One process-wide mutex serialises writes. A hand-edited file over 16 KB is read
cut at 16 KB on a character boundary with `truncated: true`. Confinement: the folder comes from a validated id
(`artifacts::valid_id`: letters, digits, `-`, `_`), file names from `CaptainFileKind`; a symlink as the captain's folder or as a
file is refused, and the folder is checked to canonicalise inside `captains/`. `Files` and `WriteFile` need the captain's row to
exist (archived is fine). Nothing injects the files into prompts yet (B2).

Tests take the data folder as an argument and use temp folders, so none touches `NEKO_DATA_DIR` or the live data.

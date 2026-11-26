# The store (S0, 2026-11-26)

`crates/neko-core`. SQLite through `rusqlite` with the `bundled` feature, at `<data>/neko.sqlite3`.

## Records

- `workspaces (id, name, created_ms)`. One default workspace, id `default`, named Home, made at first open.
- `tasks (id, workspace_id, words, title, status, created_ms)`. `words` are the person's words exactly as sent (whitespace and
  all). `title` is the first line with words, spaces collapsed, cut at a word before 80 characters. `status` is `new`: no agent
  exists yet.
- `events (seq, at_ms, workspace_id, subject, kind, data)`. One row per change, in the same transaction as the change.
  `seq` is the revision the daemon pushes. Triggers refuse `UPDATE` and `DELETE` on it.

Ids are `task_` and 26 base-32 characters: 48 bits of time, 80 random bits (the ULID layout). They sort by time.

## Rules

- One row per entity, never one big JSON row. The earlier project rewrote megabytes on every tick that way.
- A write touches only the rows it changes.
- No clock and no randomness inside logic. `Core` is handed a `Clock` and an `Ids`; tests use `FixedClock` and `SequentialIds`.
- WAL, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout` 5 s, `journal_size_limit` 8 MB. The daemon checkpoints
  (`TRUNCATE`) at a clean shutdown.
- Migrations: `user_version` counts the migrations run; each runs in its own transaction. A store from a newer Neko (a higher
  version) is refused, not touched.
- Two connections on one file see each other's writes (tested).

## Tests

`crates/neko-core/tests/store.rs`: create and list, refusals, reopen, migration and the newer-version refusal, the append-only
log, two connections.

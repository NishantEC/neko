# Architecture

Neko is two programs on one Mac: a daemon that owns the data, and an app that draws it. This page describes what is built
now (slice S1). It changes as each slice ships.

## The crates

| Crate | Workspace | What it is |
| --- | --- | --- |
| `crates/neko-protocol` | root | The requests, replies and events between a screen and the daemon, as JSON. With the `ws` feature, the WebSocket link both ends use. |
| `crates/neko-core` | root | The store and its records (tasks, runs, parts, permissions, usage, the event log), the agent contract and the Claude Code runtime, the run guard, the permission policy, worktrees. Also `neko-fake-claude`, the fake CLI tests use (never shipped). |
| `crates/neko-daemon` | root | The `neko-daemon` binary. It owns one data folder, serves the protocol and runs tasks. As `--neko-run-guard` it is a run's guardian. |
| `apps/neko` | its own | The desktop app, drawn with Rune and rune-ui. |

The app has its own Cargo workspace. Rune needs a `[patch.crates-io]` table, and that table must not reach the daemon's
workspace. The app depends on `neko-protocol` by path and on nothing else of the daemon.

## The daemon and the screens

The daemon holds the truth. A screen is a thin client: it draws what the daemon says and sends what the person asks. It keeps
no records of its own, only where its window was.

The daemon runs with or without a screen: `neko-daemon --data-dir DIR`. The app starts it beside itself when none is running
(the binary sits next to the app inside `Neko.app`), and reconnects when it restarts. Quitting the app leaves the daemon
running.

One daemon serves one data folder. It holds an exclusive lock on `daemon.lock`; a second daemon for the same folder exits with
code 3. SIGTERM, SIGINT or SIGHUP stop it cleanly: connections close, the store is checkpointed, `daemon.json` is removed.

## Transport and token

- The daemon listens on `127.0.0.1` only, on a free port. It writes the port to `daemon.json` in the data folder.
- Every request carries the per-install token: `Authorization: Bearer <token>`. The token is in the folder's `token` file,
  mode 0600, made at first start. The folder itself is 0700.
- A request must name the daemon as its host (`127.0.0.1:<port>` or `localhost:<port>`) and carry no `Origin` header. This
  keeps web pages and DNS rebinding out.
- `GET /v1/hello` answers plain HTTP with the version. `GET /v1/ws` upgrades to a WebSocket that carries the protocol.
- Each connection has one thread that sleeps in `poll(2)` until the socket or its wake pipe has something. A quiet connection
  costs no CPU.

## The protocol

One JSON object per WebSocket text frame. A screen sends `{"id": n, "type": ...}`; the daemon answers with a reply carrying the
same id. The first request is `hello`, which names the protocol version; another version is refused. After `subscribe`, the
daemon pushes an event for every change. See `docs/knowledge/protocol.md`.

## The store

SQLite, bundled, in WAL mode, at `neko.sqlite3` in the data folder. One row per entity: `workspaces` (General and the folders the
person added), `tasks`, `runs`, `parts`,
`permissions`, `usage`, `settings`, `cats`, `rounds` and its `round_runs`, `plugins` and `plugin_values`. Every change
also appends one row to `events`, in the same transaction. Triggers refuse any update or delete of an event. The schema version
is SQLite's `user_version`; migrations run in order from version 1. Logic never reads the clock or makes an id itself: both are
handed in. See `docs/knowledge/store.md`.

## Running a task

The daemon runs the agent CLIs installed and signed in on the Mac (Claude Code today); it asks for no API key. A task is made in a workspace (`docs/knowledge/workspaces.md`). It starts
its agent at once: in a scratch folder (General), in its own git worktree on `neko/<short id>` when the chosen folder is a repository, or
directly in a plain folder. Each run has its own thread and runs under the run guard. Its events become stored parts and pushed
changes; words stream to screens at once and reach the disk every few seconds. Risky steps wait for the person in the task. One
run at a time per task; tasks run side by side. See `docs/knowledge/runtimes.md`, `permissions.md`, `run-guard.md`,
`transcript.md`. After each run an empty worktree or scratch folder is tidied away; one with work in it is kept. A checkpoint at each run's start and
end lets the person review, undo and redo it (`docs/knowledge/undo.md`).

## Rounds

A round is a cat's standing work that starts by itself. One daemon thread (`neko-rounds`) sleeps until the next due time (no timer
when no round is on), makes each due time an ordinary task and starts it through the same path as a task the person makes, so the
permission mode, the cat's rules, the run guard and Undo apply unchanged. See `docs/knowledge/rounds.md`.

## The Briefing

What happened while the person was away is derived by the daemon from the records above, never written by a model and never stored apart from where the
person last read it. It is pushed like Needs you, and the rounds scheduler's thread also decides the morning notification, which
the app posts. See `docs/knowledge/briefing.md`.

## Plugins

Neko is meant to be built from plugins; the host and the first kind, MCP servers, are here and none ships. A plugin is a manifest, the
digest of it and the digest the person approved. It runs only when those are the same: for a run, the cat's approved, enabled plugins go to
Claude Code in a config file made for that run alone (`--mcp-config`, mode 0600, removed with the run); a check starts a server under the run
guard on a thread of its own with a deadline. Secret settings are kept in the macOS Keychain and nowhere else. Anything that can wait on a
server, a folder or the Keychain runs off the connection's thread, so a stop never waits on a plugin. See `docs/knowledge/plugins.md`.

## The app

A sidebar of workspace sections and the content beside it, under one 52 point header band on the traffic lights' line. Home has a greeting and a
composer. Return sends the words to the daemon; the new task opens in the task view. Components come from rune-ui, copied in
by its CLI. The task view shows the conversation with the agent, permission requests, a run card, and a composer that sends
follow-ups and stops the run. Settings (⌘,) lists the agent CLIs. Needs you (⌘2) lists what waits for the person, derived by the daemon, with Mac
notifications and the Dock badge; `neko://task/<id>` opens a task. The Briefing (⌘4) says what happened while the person was away; `neko://briefing` opens it. See `docs/knowledge/needs-you.md`. See `docs/knowledge/app-shell.md` and
`docs/knowledge/rune-ui-in-neko.md`.

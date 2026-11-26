# The protocol (S0, 2026-11-26)

`crates/neko-protocol` is the contract between a screen and the daemon. Version 1.

## Shape

- One JSON object per WebSocket text frame, on `GET /v1/ws` with the token (see `docs/architecture.md`).
- A screen sends a `ClientMessage`: `{"id": 3, "type": "get_task", "task": "task_…"}`. The id is the screen's own; the reply
  carries it back.
- The daemon sends a `ServerMessage`, tagged by `kind`: `{"kind": "reply", "id": 3, "reply": {...}}` or
  `{"kind": "event", "revision": 8, "change": {...}}`.
- `GetTask` names its task `task`, not `id`: the message's own `id` is the request's.

## Requests and replies

| Request | Reply |
| --- | --- |
| `hello {client, protocol}` | `hello {daemon, protocol, workspace}`, or `error protocol_mismatch` and the connection closes |
| `list_tasks` | `tasks {tasks}`, newest first |
| `create_task {words}` | `task {task}`, or `error invalid_words` (empty, or over 100 KB) |
| `get_task {task}` | `task {task}`, or `error not_found` |
| `subscribe` | `subscribed {revision}`; then an event for every change |

Before `hello`, any request gets `error hello_first`. Something unparseable gets `error bad_request`; an unknown type is refused,
never guessed.

## Events

`task_created {task}`, with `revision`: the store's event sequence number. A screen that made the task may see the event before
its own reply; it keeps tasks by id, so nothing doubles.

## Rules

- The JSON shapes are pinned by `tests/round_trip.rs`. A change to a shape is a new version.
- `DaemonInfo` (`daemon.json`) is part of the contract: `{pid, port, protocol, version}`.
- `client::find(dir)` reads `daemon.json` and `token`. `client::Client` is blocking (tests, tools). `client::Link` runs on its own
  thread and hands replies and events to a callback (the app).
- `pump::run` drives one connection from one thread, blocking in `poll(2)` on the socket and a wake pipe. Both ends use it.

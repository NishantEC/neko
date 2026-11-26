# The protocol (S1, 2026-11-26)

`crates/neko-protocol` is the contract between a screen and the daemon. Version 2 (S1: tasks run).

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
| `create_task {words, folder?}` | `task {task}`, or `error invalid_words` (empty, or over 100 KB). Its agent starts by itself. |
| `get_task {task}` | `task {task}`, or `error not_found` |
| `subscribe` | `subscribed {revision}`; then an event for every change |
| `get_transcript {task}` | `transcript {transcript}`: the task, its runs, parts and permission requests |
| `follow_up {task, words}` | `ok`; a running agent gets the words, else a new run resumes the conversation. `busy` while it starts |
| `stop_task {task}` | `ok`, also when nothing runs |
| `answer_permission {permission, decision}` | `ok`, or `already_answered` (answered or expired) |
| `watch_task {task?}` | `ok`; this connection now gets that task's streaming words |
| `list_runtimes` | `runtimes {runtimes, settings}` |
| `set_settings {settings}` | `settings {settings}`, or `bad_request` for a runtime Neko cannot run |
| `day_usage {from_ms}` | `day_usage {usage}`: what runs started since then reported |

Before `hello`, any request gets `error hello_first`. Something unparseable gets `error bad_request`; an unknown type is refused,
never guessed.

## Events

`task_created`, `task_updated`, `run_changed`, `part_changed`, `permission_changed`, `settings_changed`, each with `revision`: the
store's event sequence number. `part_delta {task, part, text}` appends streamed words; it goes only to connections watching the
task, is not stored as an event, and carries revision 0. A screen that made the task may see the event before its own reply; it
keeps everything by id, so nothing doubles.

## Rules

- The JSON shapes are pinned by `tests/round_trip.rs`. A change to a shape is a new version. Version 4 added the task status
  `asked`: the run finished and its last words end in a question mark, so it is the person's turn (not working; a follow-up
  answers it).
- `DaemonInfo` (`daemon.json`) is part of the contract: `{pid, port, protocol, version}`.
- `client::find(dir)` reads `daemon.json` and `token`. `client::Client` is blocking (tests, tools). `client::Link` runs on its own
  thread and hands replies and events to a callback (the app).
- `pump::run` drives one connection from one thread, blocking in `poll(2)` on the socket and a wake pipe. Both ends use it.

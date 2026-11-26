# The protocol (S1, 2026-11-26)

`crates/neko-protocol` is the contract between a screen and the daemon. Version 8 (cats).

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
| `list_tasks` | `tasks {tasks}`, every workspace's, newest first |
| `create_task {words, workspace?, cat?}` | `task {task}` (no workspace: General; no cat: the workspace's default, else the least busy), or `error invalid_words` (empty, or over 100 KB), `refused` (a removed workspace, or its folder not found), `not_found`. Its agent starts by itself. |
| `remove_task {task}` | `ok`, `busy` while it runs, or `unsaved_work` (uncommitted changes or scratch files: nothing deleted) |
| `list_workspaces` | `workspaces {workspaces}`: General, the person's order, removed ones last; `missing` when the folder is gone |
| `add_workspace {folder}` | `workspace {workspace, existed?}`, or `invalid_folder` |
| `rename_workspace {workspace, name}` | `workspace {workspace}`, or `refused` (General, an empty name) |
| `remove_workspace {workspace}` | `workspace {workspace}` with `removed_ms`, or `refused` (General) |
| `locate_workspace {workspace, folder}` | `workspace {workspace}`, or `invalid_folder`, `refused` (another workspace's folder) |
| `move_workspace {workspace, to}` | `ok`; `to` counts the person's workspaces on the list from 0 |
| `get_task {task}` | `task {task}`, or `error not_found` |
| `subscribe` | `subscribed {revision}`; then an event for every change |
| `get_transcript {task}` | `transcript {transcript}`: the task, its runs, parts and permission requests |
| `follow_up {task, words, answering?}` | `ok`; a running agent gets the words, else a new run resumes the conversation. `busy` while it starts. With `answering` (a run): only while the task is still `asked` on that run, else `already_answered` |
| `list_needs` | `needs {items}`: what waits for the person, oldest first, grouped by workspace (`needs-you.md`) |
| `stop_task {task}` | `ok`, also when nothing runs |
| `answer_permission {permission, decision}` | `ok`, or `already_answered` (answered or expired) |
| `watch_task {task?}` | `ok`; this connection now gets that task's streaming words |
| `list_runtimes` | `runtimes {runtimes, settings}` |
| `set_settings {settings}` | `settings {settings}`, or `bad_request` for a runtime Neko cannot run |
| `get_diff {task, run?}` | `diff {diff}`: the run's files between its checkpoints (no run: the whole task), with counts, no lines; `refused` with no checkpoints |
| `get_diff_file {task, run?, path, full?}` | `diff_file {file}`: one file's lines, cut at 1,000 (`full`: 50,000) |
| `undo_run {task, run, anyway?}`, `redo_run {…}` | `ok`; `changed_since {files}` (nothing written; `anyway` goes ahead after keeping the folder as a checkpoint); `refused` with the line why (not the run offered, committed, mid-merge, a locked index); `busy` while it runs |
| `undo_storage` | `undo_storage {storage}`: bytes in Neko's undo folder, checkpoints, tasks |
| `clear_undo` | `ok`, or `busy` while any task works |
| `list_cats` | `cats {cats}`: active first, oldest first, then retired |
| `create_cat {name, face?, purpose?, workspace?}` | `cat {cat}`, or `invalid_name` (empty, over 40, taken), `refused` (16 active, an unknown face) |
| `rename_cat`, `set_cat_face`, `set_cat_purpose`, `set_cat_workspace`, `save_cat_rules`, `restore_cat_rules`, `retire_cat`, `bring_back_cat` | `cat {cat}`, or `invalid_name`, `refused` (the last active cat, rules over 16 KB, nothing to restore, 16 active), `not_found` |
| `set_task_cat {task, cat}` | `task {task}`, or `refused` once a run started |
| `day_usage {from_ms}` | `day_usage {usage}`: what runs started since then reported |

Before `hello`, any request gets `error hello_first`. Something unparseable gets `error bad_request`; an unknown type is refused,
never guessed.

## Events

`task_created`, `task_updated`, `task_removed`, `workspace_added`, `workspace_updated` (renamed, removed, restored, located, moved:
the whole row), `run_changed`, `part_changed`, `permission_changed`, `settings_changed`, `needs_changed` (the whole Needs you list, derived, sent when it differs), `cat_added`, `cat_updated` (the whole cat), each with `revision`: the
store's event sequence number. `part_delta {task, part, text}` appends streamed words; it goes only to connections watching the
task, is not stored as an event, and carries revision 0. A screen that made the task may see the event before its own reply; it
keeps everything by id, so nothing doubles.

## Rules

- The JSON shapes are pinned by `tests/round_trip.rs`. A change to a shape is a new version. Version 4 added the task status
  `asked`: the run finished and its last words end in a question mark, so it is the person's turn (not working; a follow-up
  answers it). Version 5 added workspaces: a task's `workspace` names one, `create_task` takes a workspace instead of a folder. Version 6 added Needs you: `list_needs`, `needs_changed`, a notice's `failure`
  kind, and `answering` on a follow-up. Version 7 added undo: a run's `undo` (state, why not, its own changes, committed), the
  diff requests, undo, redo, the storage figure and clearing it. Each change to a run's undo is a `run_changed` with the run whole.
  `neko_protocol::undo::offers` says which run may be undone or redone, for both ends. Version 8 added cats: a task's `cat`, a run's
  `cat` and `rules_hash`, the `rules_changed` part, a Needs-you item's `cat` and `cat_name`, the cat requests and events, and
  `invalid_name` (`cats.md`).
- `DaemonInfo` (`daemon.json`) is part of the contract: `{pid, port, protocol, version}`.
- `client::find(dir)` reads `daemon.json` and `token`. `client::Client` is blocking (tests, tools). `client::Link` runs on its own
  thread and hands replies and events to a callback (the app).
- `pump::run` drives one connection from one thread, blocking in `poll(2)` on the socket and a wake pipe. Both ends use it.

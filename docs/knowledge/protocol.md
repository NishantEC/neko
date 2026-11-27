# The protocol (S1, 2026-11-26)

`crates/neko-protocol` is the contract between a screen and the daemon. Version 12 (threads). The words the app shows say thread and project; the field and message names below keep `task` and `workspace` (`threads.md`).

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
| `mark_read {task}`, `mark_unread {task}` | `task {task}` (whole, with its new `last_read_ms`), or `not_found`. Nothing changes, and nothing is pushed, when the thread is already so |
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
| `list_rounds` | `rounds {rounds, paused}`: every round with its status, next due time, note and last history line |
| `create_round {name, words, cat, workspace?, schedule, on?}` | `round {round}`, or `invalid_name`, `invalid_words`, `refused` (an unknown or retired cat, a removed workspace, a schedule that makes no sense, 64 rounds) |
| `update_round {round, name, words, cat, workspace?, schedule}` | `round {round}`; a new schedule counts from now |
| `set_round_on {round, on}`, `remove_round {round}` | `round {round}` / `ok`, or `not_found`; turning on clears the brake; removing keeps the tasks |
| `run_round_now {round}` | `task {task}` (an ordinary task, marked with the round), or `busy` while its last task is unfinished, `refused` (retired cat, missing folder) |
| `set_rounds_paused {paused}` | `ok`; every round then follows as `round_updated` |
| `round_history {round}` | `round_runs {runs}`: the last 50 lines, newest first |
| `debug_clock {offset_ms?}` | `ok` in a debug daemon started with `NEKO_DEBUG_CLOCK=1`, else `refused` (`rounds.md`) |
| `get_briefing {from_ms?, to_ms?}` | `briefing {briefing}`: what happened in the window (the last mark-read, else 24 hours, up to now; start inclusive, end exclusive), derived by the daemon (`briefing.md`) |
| `mark_briefing_read` | `briefing {briefing}` for the new window, also pushed as `briefing_changed` |
| `get_briefing_settings`, `set_briefing_settings {settings}` | `briefing_settings {settings}` (`morning_on`, `morning_at` as HH:MM), or `bad_request` for a time that is not one |
| `list_plugins` | `plugins {plugins}`: every plugin, oldest first, with its approval, settings (a secret only as `set`), last check and what changed (`plugins.md`) |
| `add_mcp_json {json, name?}` | `plugins {plugins}` (the ones added; each waits for approval, nothing runs), or `invalid_plugin` with the words (bad JSON, no server found, a bad manifest), `refused` (a name taken, 64 plugins). A paste's secret values go to the Keychain, never the store. Answered on its own thread |
| `add_plugin_folder {folder}` | `plugin {plugin}` from the folder's `neko-plugin.json`, or `invalid_plugin`, `refused`. Answered on its own thread |
| `update_plugin_manifest {plugin, manifest}` | `plugin {plugin}`, now `changed` until approved again; `refused` for a plugin from a folder |
| `approve_plugin {plugin, digest}` | `plugin {plugin}`; `refused` when the manifest is not the one with that digest any more |
| `decline_plugin {plugin}` | `plugin {plugin}` (declined: no more asking until the manifest changes); `refused` for an approved one |
| `set_plugin_enabled {plugin, on}`, `set_plugin_access {plugin, access}` | `plugin {plugin}`, or `refused` (a cat that is not there; "only" with no cats) |
| `set_plugin_setting {plugin, key, value?}` | `plugin {plugin}`; no value clears it; a secret goes to the Keychain and comes back only as `set`; `invalid_plugin` for a value that does not fit the field. Answered on its own thread (the Keychain) |
| `test_plugin {plugin}` | `plugin {plugin}` with its new `last_check`, sent when the check ends (at most 10 s; the connection is never held), or `refused` (not approved, a web server, a problem), `busy` |
| `remove_plugin {plugin}` | `ok`; its Keychain items go first, and a Keychain that will not keeps the plugin with the words |
| `day_usage {from_ms}` | `day_usage {usage}`: what runs started since then reported |

Before `hello`, any request gets `error hello_first`. Something unparseable gets `error bad_request`; an unknown type is refused,
never guessed.

## Events

`task_created`, `task_updated`, `task_removed`, `workspace_added`, `workspace_updated` (renamed, removed, restored, located, moved:
the whole row), `run_changed`, `part_changed`, `permission_changed`, `settings_changed`, `needs_changed` (the whole Needs you list, derived, sent when it differs), `cat_added`, `cat_updated` (the whole cat), `round_added`, `round_updated` (the whole round: edited, switched, run, skipped, braked, a new next time), `round_removed`, `rounds_paused`, `briefing_changed` (the whole briefing, derived, sent when its content differs), `briefing_settings_changed`, `briefing_due {counts}` (the morning notification is due: not stored, revision is the last stored change), `plugin_added`, `plugin_updated` (the whole plugin: approved, declined, changed, switched, set up, checked, left to other cats), `plugin_removed`, each with `revision`: the
store's event sequence number. `part_delta {task, part, text}` appends streamed words; it goes only to connections watching the
task, is not stored as an event, and carries revision 0. A screen that made the task may see the event before its own reply; it
keeps everything by id, so nothing doubles.

## Rules

- The JSON shapes are pinned by `tests/round_trip.rs`. A change to a shape is a new version. Version 4 added the task status
  `asked`: the run finished and its last words end in a question mark, so it is the person's turn (not working; a follow-up
  answers it). Version 5 added workspaces: a task's `workspace` names one, `create_task` takes a workspace instead of a folder. Version 6 added Needs you: `list_needs`, `needs_changed`, a notice's `failure`
  kind, and `answering` on a follow-up. Version 7 added undo: a run's `undo` (state, why not, its own changes, committed), the
  diff requests, undo, redo, the storage figure and clearing it. Each change to a run's undo is a `run_changed` with the run whole.
  `neko_protocol::undo::offers` says which run may be undone or redone, for both ends. Version 9 added rounds: a task's `round` (`{id, name, due_ms}`), the round requests, replies and events, and `RoundStatus`/`RunKind`/`RunEnd` (`rounds.md`; the arithmetic is `neko_protocol::schedule`, shared with the app). Version 11 added plugins: the plugin requests, replies and events above, `Plugin` and what is in it (`protocol/src/plugins.rs`: the manifest, `Approval`, `Access`, `FieldValue`, `Check`, `PluginAsk`), `Need::Plugin` (a "Needs you" item that belongs to no task: its `task`, `workspace`, `cat` and `cat_name` are empty), `ErrorCode::invalid_plugin`, and `neko_protocol::plugins::parse_mcp_json`, which a screen runs itself to show what a paste will add. Version 12 added threads: a task's `last_read_ms`, `activity_ms`, `latest_line` (the last line of the agent's newest words, at most 120 characters), `failure_line`, the derived `thread_status` (`working`, `failed`, `idle`) and the `needs_you` flag, and `mark_read` / `mark_unread`; a `task_updated` is pushed when the latest line changes. All are optional on the wire (`serde(default)`), so a screen that does not know them still reads a task (`threads.md`). Version 10 added the Briefing: `get_briefing`, `mark_briefing_read`, the settings requests, the `Briefing` shapes (`protocol/src/briefing.rs`) and the three events; `undo::Offer` and `Action` are now serialisable, since a finished task carries what its run's card offers. Version 8 added cats: a task's `cat`, a run's
  `cat` and `rules_hash`, the `rules_changed` part, a Needs-you item's `cat` and `cat_name`, the cat requests and events, and
  `invalid_name` (`cats.md`).
- `DaemonInfo` (`daemon.json`) is part of the contract: `{pid, port, protocol, version}`.
- `client::find(dir)` reads `daemon.json` and `token`. `client::Client` is blocking (tests, tools). `client::Link` runs on its own
  thread and hands replies and events to a callback (the app).
- `pump::run` drives one connection from one thread, blocking in `poll(2)` on the socket and a wake pipe. Both ends use it.

## Rune P1: the session view, the Neko column, the write requests (2026-11-16)

Code: `apps/neko-rune/`. Decision and phases: `docs/decisions/0002-rune-for-everything.md`. The web UI is
untouched and stays the default; `apps/**` never triggers the installed app's reinstall.

### Where the data comes from

- **One held snapshot connection** (`link/mod.rs`, unchanged from P0) reduces each workbench to a
  `Summary` (`link/model.rs`): a row per task, workspaces, goals, queued messages, the last 24 turns of
  Neko's chat, and **the open task in full** (`Detail`, newest 600 events, 12 KB a message). The link
  thread learns which task is open from a shared `Wanted`; a task chosen in the window is also asked for
  once (`Call::Refresh`). The multi-megabyte snapshot is dropped after summarising.
- **Streaming is the snapshot.** The daemon writes a reply's parts into the workbench as they arrive and
  the held request returns on every change (the web's HTTP chat stream is the same data). The client reads
  `ChatMessage::parts` and draws them with `Markdown::streaming(true)` (the kit parses the open tail block).
- **A task's turns** are derived from its events in `turns.rs`, a port of the daemon's
  `task_chat::task_messages` (same test cases), because the client has only the snapshot.
- **Requests beyond the snapshot** are `Call`s (`link/write.rs`), each on a short connection
  (`link/reply.rs`). `App::call` sends it: on a thread in the window (`Dispatch::Async`), on the calling
  thread in headless runs against a daemon (`Dispatch::Blocking`, `NEKO_RUNE_SOCKET`), or nowhere (tests
  read `App::outbox`).

### The allow-list (`tests/allow_list.rs`)

The client may send exactly the requests listed there and no others; adding one means editing the list in
the same commit. Reads: snapshot, held snapshot, checkpoints, one run's diff, unseen marks, the Inbox
(`Routines`), the digest, the model list. Writes, each one a web button with the web's fields: queue a
message (`Crew::Enqueue`, kind by mode as `work/queue.tsx`) and cancel it, reply to a task, approve,
complete, stop, start again, a goal's pause/resume/extend/mark done/stop/answer/create, undo (with its
three options) and redo, answer an Inbox item (no words = its default), mark the digest read, mark a task
seen or unread, create a task, pin a runtime on `task:<id>`, and Neko's chat (send, stop, approve tool or
action). Nothing destructive (no delete, merge, publish, settings, MCP, skills). The only file the client
writes is a pasted image, in its own cache (`~/Library/Caches/neko/composer-attachments`), referenced as
`![Image](file://…)` like the web's composer.

### Behaviour worth knowing

- **Composer**: Enter sends, Shift+Enter is a new line, ⌘↩ steers now, an input method's Enter never
  sends. While a task is in a turn a message goes to the durable queue with the menu's mode (default after
  this turn); one command id is kept per (task, mode, text) so a retry is one message. Between turns it is
  a reply. The text leaves the box only when the daemon takes it. **Interrupt and restart is not queued**:
  the daemon hands the message over at once, so it never shows as a queued line.
- **Same glyph, different words.** `AwaitingApproval` and the running states are both the amber half
  circle in the web's mapping (`web/src/ui/status-glyph.tsx`); the Rune client matches it exactly and a row
  says "Needs you" or "Building" in words where the other rows say their age.
- **"+ New"** has no web form to copy (the web starts work from Neko's chat). It sends `CreateTask` (what
  the chat's `create_task` tool makes), then `Goals(Create)` on that ticket and `SetConversationRuntime`
  on `task:<id>` when asked. The ticket waits for approval like any other.
- **Undo** asks first, shows the daemon's refusal (`checkpoint:<code>: …`) and offers what the code allows.
- The Neko column no longer repeats the sessions' Needs-you tasks: it shows the Inbox (routine and goal
  items with their default and "Use the default"), the digest ("Nothing needs you" when empty), Neko's
  conversation and the ask box. Handoff lines (`Neko → Builder NEK-…`) open the session.

### Testing

`tests/shell.rs` and `tests/session.rs` drive the app through Rune's headless `Runtime` (real layout and
hit-testing). `tests/harness.rs` does the same against the daemon's `web_harness` (isolated data folder
under `/tmp`): select, streamed turns, tool card, run diff, undo with confirm and refusal, a message in
each steer mode, create a session with a goal, answer an Inbox default, ask Neko and watch the reply grow,
mark unread. It needs `cargo build -p neko-daemon --bin web_harness` and `node`; without them it says so
and passes (`NEKO_REQUIRE_HARNESS=1` makes that a failure). Snapshots: `apps/neko-rune/scripts/snap.sh`
(demo scenes) and `snap-harness.sh` (the daemon's own rows), committed under `docs/design/rune/`.

### Not done in P1

DESIGN.md is not yet rewritten for Rune; the context panel is still empty (P3); Calendar, Routines, Settings
and More are the P0 placeholders (P2); reasoning parts are not shown; the independent review is raw text;
approval of a Neko chat action works for tool calls and actions but the multi-choice and form blocks of a
reply are shown as text.

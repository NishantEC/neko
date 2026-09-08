# Codex control plane — design

**Status:** approved 2026-09-08

## Decision

neko becomes a first-class, local client for **every Codex task on this Mac**.
It does not read Codex's session files or automate its desktop UI. Instead,
`neko-daemon` owns one child `codex app-server` process over stdio and speaks
the documented JSON-RPC protocol. This is the supported route for thread
history, live turns, approvals, and task control.

The current Paseo integration remains available as a separate optional
backend. Codex is the default agent source whenever its executable and local
authentication are available; a missing Paseo daemon must no longer make the
Agents surface empty.

## Product boundary

The feature is a full *task client*, not a clone of every Codex Desktop page.

| neko owns | Codex continues to own |
| --- | --- |
| all local task discovery; status; project and text filters; pinned and archived tasks; one selected task's transcript; turn streaming; follow-ups; starting, interrupting, archiving and forking tasks; and approval decisions | settings; account sign-in; marketplace and plugin management; browser tabs; full diff/review tooling; and workspace administration that is not attached to a task |

`Open in Codex` is always present on a task. It is an escape hatch for rich
inspection, not the only way to do ordinary work.

## UX

1. The root panel's existing **Agents** tiles show the four most important
   Codex tasks: approval needed first, then working, then most recent. A
   `Codex` badge distinguishes them from legacy Paseo rows.
2. **Needs you** becomes backend-neutral. A Codex server request appears with
   its original reason and target. Enter opens a confirmation card; approve
   and decline are explicit actions. neko never approves a command or file
   edit merely because it was listed or selected.
3. **Agents** mode lists every non-archived local task, filterable by task
   title, workspace, state, model, and provider. It supports current, pinned,
   archived and text-filtered views without parsing `~/.codex` itself.
4. Selecting a task enters the existing conversation mode, upgraded into a
   focused Codex task view: recent turns, tool calls, streamed deltas,
   expandable file changes, a status rail, and a composer. The history is
   bounded and paged; neko is a panel, never an unbounded terminal transcript.
5. **New Codex task** takes a prompt and an explicit location. It defaults to
   the current project folder, visibly displays the path, and offers *New Git
   worktree* only after the user selects it and confirms its target branch.
   A task is never silently rooted at `$HOME` or at neko-daemon's inherited
   working directory.

## Architecture

### One supervised local transport

`neko-daemon` starts `codex app-server --listen stdio://` on demand. It owns
the child process, its stdin writer, and one reader thread. The reader parses
newline-delimited JSON-RPC replies and notifications. A request-id map wakes
the caller waiting for a reply; notifications update a shared immutable
`CodexSnapshot` and are broadcast to clients through the existing daemon event
channel.

The daemon starts with `initialize`, sends `initialized`, then performs a
paginated `thread/list` for all local task sources. It retains the app-server
child while neko-daemon lives, rather than spawning a process for every search.
If the child exits, discovery reports *Codex unavailable*, the UI preserves
the last known rows as stale rather than inventing an empty result, and the
supervisor retries with bounded exponential backoff. No listener is opened on
the network: stdio is local to the daemon child.

The implementation probes protocol capabilities at startup rather than
hard-coding a Codex CLI version. The app-server's stable thread and turn APIs
gate basic discovery/control. Historical turn/item pagination is requested
only when the server accepts `experimentalApi`; otherwise the task is still
controllable and `Open in Codex` handles older history. There is no fallback
to private state files.

### Snapshot-first providers

Add `neko_core::codex` with three independently testable pieces:

- **Wire types and reducer**: deserialize thread records, turn items and
  server requests into provider-neutral `Task`, `Turn`, `PendingApproval` and
  `TaskStatus` values. Notification ordering and duplicate events are handled
  here.
- **Snapshot store**: one lock-protected current view of those values, with
  generation and freshness timestamps. Reads never block on the app-server.
- **Action encoder**: converts an intentional neko action into the exact
  JSON-RPC request and checks its current task/turn/request identity before it
  is sent.

`neko-daemon` provides the transport and updates the snapshot. Search
providers only read snapshots, preserving the project's rule that a keystroke
cannot wait on a network or subprocess round-trip. The existing `Provider`
seam continues to own search ranking and result rendering.

### Protocol and modes

`neko-protocol` gains additive request/event types for task listing, opening a
task view, task pagination, starting a turn, interrupting a turn, archive and
fork, and resolving a pending Codex server request. The client never receives
the Codex credential cache, app-server token, or raw process handles.

The current agent and conversation modes become backend-aware through a
`TaskRef { backend, thread_id }`, replacing Paseo-only agent ids where needed.
Paseo's current `conversation.rs` remains an adapter until the Codex view is
feature-complete; it does not get rewritten as a generic transcript parser.

### Actions and safety

| User action | Codex operation | protection |
| --- | --- | --- |
| Send follow-up | resume thread, start turn | Enter sends text the user typed; task must be idle or explicitly interruptible |
| Start task | start thread, start first turn | visible folder and model; worktree creation is a separate confirmation |
| Interrupt | interrupt active turn | confirmation menu |
| Archive / fork | thread archive / fork | archive confirms; fork displays origin and target |
| Approve / decline | resolve server request | show Codex's reason, command/file context and exact decision first |

The approval reducer drops a card as soon as Codex resolves its request. A
late response returns an honest “already resolved” status; it never retries
against a newer request. Credential login/logout is outside neko. If Codex is
not signed in, neko tells the person to sign in through Codex and offers
`Open in Codex`.

## Compatibility and migration

The feature begins with a live compatibility probe against the installed
`codex app-server`, currently `codex-cli 0.146.1` on the development machine.
The probe validates initialization, local `thread/list`, a known inactive
thread's metadata, and notification handling before Codex rows are advertised
as live. It is a capability check, not a promise that a version number alone
means compatible.

**Validation on 2026-09-08:** a stdio probe initialized successfully and
`thread/list` returned the current Codex task alongside tasks rooted in other
local workspaces. This proves the needed cross-workspace discovery path is the
app-server's stored thread index, not private `~/.codex/sessions` parsing.

Paseo rows retain their source badge and existing actions. Shared UI language
uses **task**, not agent, because Codex's stored unit is a thread/task while
Paseo's is an agent. Preferences gain independent toggles for Codex and Paseo
sources; Codex defaults on, Paseo retains its current setting. This avoids a
destructive migration and lets a user who runs both see both.

## Tests and evidence

1. Reducer unit tests cover every task status, notification order, pagination,
   stale snapshots and already-resolved approvals.
2. A fake stdio app-server provides deterministic JSONL replies and
   notifications. It proves initialization, list pagination, follow-up,
   interruption, archive/fork, backoff after exit and no credential leakage.
3. Protocol tests pin all additive wire messages and prove old Paseo clients
   remain readable.
4. GPUI tests cover keyboard selection, composer send, confirmation cards,
   loading/stale/unavailable states and task-detail pagination without opening
   a real window.
5. One manual local-Codex evidence pass proves an existing Desktop/CLI task is
   discovered, a task streams, a real approval is displayed without automatic
   action, and opening a task hands off to Codex.

Update `README.md`, `docs/architecture.md`, `docs/adding-a-provider.md`, and
`AGENTS.md` when this lands. The existing Paseo-only agent-control plan stays
as historical evidence and receives a forward link to this design.

## Delivery order

1. App-server transport, capability probe and snapshot/reducer tests.
2. Codex task source in root/Agents modes plus backend-neutral inbox.
3. Focused task view, streaming and follow-up composer.
4. Start, interrupt, archive, fork and safe approval actions.
5. Worktree option, desktop handoff, live verification and documentation.

Each step leaves the Paseo path working. No step reads private Codex storage,
opens a non-loopback port, or handles credentials directly.

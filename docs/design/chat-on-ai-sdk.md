# Neko's chat on the Vercel AI SDK

Decided 2026-10-22 (owner): Neko's chat protocol is the AI SDK's. Not a shim: the
message model, the wire format and the web client are the SDK's, and the formats they
replace are deleted. Read against `ai@7.0.x` / `@ai-sdk/react@4.0.x` as installed in
`web/node_modules` (types in `ai/dist/index.d.ts`, docs in `ai/docs/04-ai-sdk-ui`).

## What stays as it is

The daemon is the source of truth. A conversation lives in the daemon (multi-window,
queue, interrupt-and-send, Plan/Ask, drafts, approvals, task conversations). The web
client holds no authority and no history of its own: `useChat` is the *view and stream
state machine* over a log the daemon owns. No new daemon authority: same token,
Host/Origin gates, body limits as every other `/api/` route.

## Message model

A conversation is a list of SDK `UIMessage`s: `{ id, role, parts, metadata }`.

| Part | Used for |
| --- | --- |
| `text` | prose (`state: "streaming" \| "done"`) |
| `reasoning` | the model's reasoning, when the agent reports it (Claude `thinking`, Codex `reasoning`) |
| `dynamic-tool` | every tool an agent used: `toolName`, `toolCallId`, `title`, `state`, `input`, `output`, `errorText`, `approval` |
| `data-neko-choices` `-form` `-plan` `-chart` `-files` `-sources` | the structured views a reply can carry (was ```` ```neko-* ```` fences in the text) |
| `data-neko-ticket` `-memory` `-watch` | what a reply proposed or saved, settled by the daemon (ticket opened, memory kept, watch saved paused) |
| `file` | a user's attachment (`url` is the daemon's `/api/attachments/…` path) |
| `step-start` | each assistant message of an agent run (Claude `message_start`, Codex message item) |

`dynamic-tool` rather than `tool-<name>`: the agents' tool names are open-ended (MCP
tools, `Bash`, `Edit`, …) and the SDK defines `dynamic-tool` exactly for that. States are
the SDK's: `input-streaming`, `input-available`, `approval-requested`,
`approval-responded`, `output-available`, `output-error`, `output-denied`.

`metadata` (typed on the web as `NekoMeta`): `{ atMs, startedMs?, state: "queued" |
"pending" | "done" | "failed", workspaceId?, tickets?: string[] }`. `startedMs`/`atMs` give
"Working for 12s" and "Worked for 1m 12s". `queued`/`pending`/`failed` are the daemon's
lifecycle flags; the web reads them from `metadata`, never from a second source.

### Approvals are tool approvals

Two things wait for a click, and both are SDK tool approvals (`state:
"approval-requested"`, `approval: { id }`), answered with `addToolApprovalResponse`:

1. an MCP tool call that is not yet granted (`ChatToolCall`, `DecideChatTool`);
2. an action Neko proposed on a task (`ChatAction`, `DecideChatAction`), a `dynamic-tool`
   named `neko_task_action` with `input: { taskId, kind, text }`.

The click goes to the daemon (the two existing commands run the existing checks); the
daemon answers the SDK way by streaming `tool-approval-response` and then
`tool-output-available` / `tool-output-error` / `tool-output-denied`, so every window
converges. The web calls `addToolApprovalResponse` locally for an instant UI and
`sendAutomaticallyWhen` stays off: the SDK never re-posts the conversation on approval,
because the daemon already holds it.

## Storage

`neko_chat_v2` replaces `neko_chat_v1`. A one-time versioned reset: v1 is dropped (the
SDK shape cannot be rebuilt from `text` + `activity` strings without inventing tool
inputs; the owner allowed it). A version row (`neko_chat_schema`) says which shape is on
disk; a daemon that finds anything else starts an empty conversation and removes the old
row.

The stored `ChatMessage` (neko-protocol) keeps the daemon's lifecycle and business state and
replaces the presentation fields:

```
id, at_ms, started_ms, role, parts: Vec<serde_json::Value>   // SDK parts, as above
pending, queued, failed, agent_profile_id/revision, workspace_id
ticket_ids, responsibility_ids                                // links the daemon follows
tool_calls: Vec<ChatToolCall>, actions: Vec<ChatAction>       // approval ledgers (below)
```

Deleted: `text`, `activity`, `remembered` (a `data-neko-memory` part now), `ChatMessage::
text` field (an accessor joins the text parts for prompts, search and the Swift app).

`tool_calls` and `actions` stay as **ledgers**: the MCP bridge's approval state machine
(`call_status`, `decide_call`) and the confirm path read them. The transcript is `parts`:
recording a call or changing its status updates the ledger *and* the part, in one function.

### Streaming deltas never rewrite the row

The in-flight reply lives in memory (`ChatStreams`, neko-core `ui_message`): its parts and
its subscribers. Deltas are applied to those parts (the same reducer the SDK runs) and
fanned out; the SQLite row is written deferred (`set_setting_deferred`, at most every 2 s,
folded into the next immediate write) and immediately on every state change that matters
(finish, approval, queue). A crash loses ≤ 2 s of a reply, never a decision; the existing
`recover_interrupted` marks the turn failed.

## Wire: the UI message stream

SSE exactly as the SDK documents: `data: <json chunk>\n\n`, header
`x-vercel-ai-ui-message-stream: v1`, terminated by `data: [DONE]`. Chunks used: `start`,
`start-step`/`finish-step`, `text-start|delta|end`, `reasoning-start|delta|end`,
`tool-input-start|delta|available`, `tool-approval-request`, `tool-approval-response`,
`tool-output-available|error|denied`, `data-neko-*`, `file`, `error`, `abort`, `finish`.

### Endpoints (all through `authorize`; Host and Origin checked; bodies ≤ 256 KiB)

| | |
| --- | --- |
| `GET /api/chat/<conversation>` | `{ revision, active: <messageId>\|null, messages: UIMessage[] }` |
| `POST /api/chat/<conversation>/stream` | send a user message (`{ id, message, workspaceId? }`); answers with the UI message stream of the reply it started. A message that had to queue, or a task reply, answers an empty stream (`[DONE]` only): the log changes instead |
| `GET /api/chat/<conversation>/stream` | the in-flight reply: its parts so far, replayed as chunks (`start` … ) and then live; `204` when nothing is running. This is the SDK's default reconnect URL |

`<conversation>` is `neko` (Home) or `task:<task id>`; anything else is `404`. A malformed
JSON body is `400`, an empty or oversized message `400`/`413`, a wrong method `405`.

Queue, interrupt-and-send, stop, discard, reorder and decisions are the existing
`Workbench` commands through `POST /api/request`; they are not part of the SDK protocol
and do not pretend to be.

### Resume and multi-window

*Replay is synthesised from the live message, not read from a log.* Attaching to a
running reply emits `start`, the parts built so far as chunks (a text part as
`text-start` + one `text-delta` of everything so far, a tool as `tool-input-available` and
its state), then every later chunk. A reload, a second window and the SDK's own
`resume: true` all take that path; the transport sets `resumeStreamIsReplay` so the client
resets the message at the replay's `start`. No chunk log grows with the reply.

Other windows learn that something changed from `Event::ChatChanged { conversation,
revision }` on `/api/events` (broadcast when a message is added, queued, settled or
finished; **not** per delta, deltas travel on the stream). On it an idle window refetches
`GET /api/chat/<conversation>`; a window that is itself streaming ignores it and refetches
when its stream ends.

### The web client

`web/src/chat/sdk/`: `NekoChatTransport implements ChatTransport<NekoUIMessage>` (
`sendMessages` → POST stream, `reconnectToStream` → GET stream, both `fetch` with the page's
cookie), `useNekoChat(conversation)` (one `Chat` per conversation, hydrated from the GET,
`resumeStream()` when `active`, refetch on `ChatChanged`), and the typed `NekoUIMessage`.
Idle send is `chat.sendMessage` (the SDK's own path; the user message id the client makes
is the id the daemon stores). While a reply runs the composer's send is Neko's queue
(`SendMessage`), and ⌘↵ is `InterruptAndSendMessage`: these are daemon features, kept.

Rendering: the T3-derived rows stay and read parts. Parts up to and including the last
tool/reasoning part are the work log (live line while streaming, one "Worked for …" fold
after); the text parts after them are the reply. `dynamic-tool` parts become the existing
ToolLine/ToolFold look (Bash → command with output, edits → "Edited path", MCP → its name
and arguments on demand); `approval-requested` parts feed the existing decision card
above the composer. `data-neko-*` parts are drawn by `reply-views.tsx`; nothing on the web
parses a JSON fence.

## From agent events to chunks

`native_runner` gains one emitter, `ChunkEmitter`, turning each CLI's event line into UI
message chunks (and nothing else): Claude Code `stream_event`s (`--include-partial-messages`:
text, thinking, tool input deltas) and the `assistant`/`user` lines (tool_use, tool_result);
Codex `item.started|completed` (`agent_message`, `reasoning`, `command_execution`,
`file_change`, `mcp_tool_call`, `web_search`); OpenCode `tool_use`/`text`. They travel
through the existing `on_event(String)` channel under one control prefix
(`STREAM_EVENT`), replacing `TEXT_RESET_EVENT`/`TEXT_DELTA_EVENT`. The bridge's own MCP
tools (`mcp__neko__*`, Codex server `neko`) are not emitted: the bridge records them in the
ledger (with approval), and that is their part.

The daemon applies each chunk to the live message through `ReplyStream`, the one place a
fence is read: model prose may still contain ```` ```neko-choices {json} ```` (the
only portable way for a CLI agent to say a structured thing), and `ReplyStream` turns a
*complete, valid* block into a `data-neko-choices` part while the text part closes and
reopens around it; a half-written fence is held back and never shown; an invalid one
stays text. Proposals follow the same rule, one block each, in order, anywhere in the
reply: ```` ```neko-ticket ````, ```` ```neko-remember ````, ```` ```neko-watch ````,
```` ```neko-action ````, ```` ```neko-used-memory ````. This replaces the single trailing
```` ```neko-reply ```` JSON block ("nothing may follow it") and the legacy one-object
form. At completion the daemon turns the collected proposals into the same `Reply` the
existing `complete_chat_reply` validates and applies (tickets, memories, watches,
actions), then records the outcome as `data-neko-*` parts and `ticket_ids`.

## Task conversations

`task:<id>` is the daemon's projection of the task's brief, your notes, agent messages,
command receipts and status events into UI messages (`task_messages`, the Rust port of
`threadRows`): your brief and notes are `user` messages, each agent's stretch is an
`assistant` message of `text` and `dynamic-tool` (Bash with its receipt) parts, status
lines are `data-neko-status`. Nothing new is stored: the events already are the log.
`ReplyToTask` is the send path. Token streaming for task runs is not part of this change
(task agents report whole messages today); the stream endpoints answer `204`/empty for
them and `ChatChanged` carries a task's changes, so the transport is the same one.

## Deleted

* `ChatMessage.text`, `.activity`, `.remembered`; `neko_chat::{activity_line, record_activity,
  record_partial, stream_visible, parse_reply, REPLY_BLOCK, MAX_ACTIVITY*}`; the
  ```` ```neko-reply ```` instruction and the legacy single-JSON form.
* `native_runner::{TEXT_RESET_EVENT, TEXT_DELTA_EVENT}`.
* Web: `NekoMessage`/`ToolCall` types, `chat/work.ts`'s activity-string parsing (the
  step kinds now come from tool names), `finishedBlocks` stays (it is rendering), the
  fence handling in `ChatMarkdownRenderer` for `neko-*`, `threadRows`' chat use.
* The native app's reads of `text`, `activity` and `tool_calls` move to parts.

## Not done / limits (stated up front)

* Task runs do not token-stream; task conversations update per message.
* `neko-*` fences remain what the *model* writes. The SDK's own answer, tool calls, needs a
  tool channel; the agent CLIs only have one (MCP bridge) when a workspace is scoped, and
  proposals must work unscoped. So the contract is typed blocks parsed once, in the daemon.
* Approvals and the bridge are untouched in authority; the change is on the self-update
  SENSITIVE list and needs the owner's click to go live.

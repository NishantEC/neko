## Chat is the AI SDK's (2026-10-22)

Owner decision: Neko's chat protocol is the Vercel AI SDK's (`ai`, `@ai-sdk/react`).
Design and what was deleted: `docs/design/chat-on-ai-sdk.md`. In short:

- **A message is a list of `UIMessage` parts** (`ChatMessage.parts`: text, reasoning,
  `dynamic-tool`, `data-neko-*`), stored in `neko_chat_v2`. **One-time reset**: the old
  `neko_chat_v1` row (`text` + `activity` strings) is dropped unread on first load.
  `ChatMessage::text()` is the plain-text view (prompts, search, Swift). `tool_calls` and
  `actions` stay as the approval ledgers; the transcript shows them as tool approval parts.
- **Agent events become chunks in one place**: `native_runner/chunks.rs` (Claude tokens,
  thinking, tools; Codex items; OpenCode parts) under the `chunks::STREAM_EVENT` prefix.
  `neko_core::ui_message` folds chunks into parts, holds replies in flight (`begin`/`emit`/
  `attach`/`finish`) and is the **only reader of `neko-*` blocks** a model writes: complete
  valid display blocks become `data-neko-*` parts, `neko-ticket/-remember/-watch/-action/
  -used-memory` become proposals. There is no trailing `neko-reply` block any more.
- **Streaming never rewrites the chat row**: deltas touch memory and followers; the parts
  are written deferred every 2s (`persist_parts`) and on every decision.
- **Endpoints** (same token/Host/Origin gates, 256 KiB bodies; tests per rejection in
  `web_tests.rs`): `GET /api/chat/<c>` (log), `GET /api/chat/<c>/stream` (resume: a full
  replay, `204` if idle), `POST /api/chat/<c>/stream` (send). `<c>` is `neko` or `task:<id>`
  (`task_chat::task_messages` projects a task's events). Queue, interrupt-and-send, stop,
  discard and decisions stay `Workbench` commands.
- **Web**: `web/src/chat/sdk/` (`NekoChatTransport`, `useNekoChat`, part helpers). The web's
  snapshot has no message parts and its digest ignores them, so a token never wakes the held
  request; a change to the chat's non-token state (`chatSignature`) refills the log.
  The shell's approval count still reads the ledger (`pendingDecisions`); Home's dock reads
  tool approval parts (`decisionsFromParts`).
- `web_harness`: `NEKO_WEB_SCRIPTED=1` answers messages with a scripted reply through the
  real chunk/reader/completion path (no model).

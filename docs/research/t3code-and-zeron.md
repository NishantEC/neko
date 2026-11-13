# T3 Code and Zeron: what Neko can learn (2026-11-13)

Read from the repositories' own docs and source layout (not marketing). Both are MIT, so code can be copied with the notice kept (Neko already keeps `THIRD_PARTY_NOTICES`-style credit for T3 snippets).

## Which "Zeron"?

The voice name most likely means **Zeron**, [zeronsh/zeron](https://github.com/zeronsh/zeron) (MIT, ~3.6k stars): "A native control plane for Claude Code, Codex, Cursor, Devin and other coding agents", a Rust + gpui rewrite with a headless engine. Other candidates:

- [Zeroshot](https://github.com/the-open-engine/zeroshot): MIT CLI that runs planner / implementer / reviewer / validator agents, SQLite state, resumable.
- [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw): Rust agent runtime (a "claw" assistant, not a coding harness), with a trace-fixture eval harness.
- Zed's agent panel: editor, not a harness.
- Zerion: crypto wallet, not relevant.
- Zerops: hosting platform, not relevant.

Best guess: zeronsh/zeron. Same shape as Neko (daemon that keeps running plus a UI that is only a viewport) and it targets "agents keep working after I close the laptop".

## T3 Code ([pingdotgg/t3code](https://github.com/pingdotgg/t3code), MIT)

### Architecture
- One Node WebSocket server wraps provider CLIs; web, Electron desktop and React Native mobile are clients over an authenticated RPC contract ([`packages/contracts/src/rpc.ts`](https://github.com/pingdotgg/t3code/blob/main/packages/contracts/src/rpc.ts)). Provider processes, terminals, git and files belong to the server; a client never substitutes its own ([overview](https://github.com/pingdotgg/t3code/blob/main/docs/internals/overview.md)).
- Subscriptions send only what a client views, so one open thread does not pay for all history.
- Provider adapters in `packages/provider-*` (Codex app-server, Claude, Cursor, OpenCode, Pi, Kiro, Grok, any ACP agent). Orchestration sees normalized commands and events. "Driver" is the kind, "instance" is one account/config; routing is by instance ([providers.md](https://github.com/pingdotgg/t3code/blob/main/docs/internals/providers.md)).

### Orchestration v2 (the part worth studying)
- Orchestrator serializes **commands**, decides **events** with no I/O. `EventSink` commits events, projections, the command receipt and **outbox effects** in one DB transaction. An `EffectWorker` runs side effects after commit and feeds results back as commands. A command ack means intent committed, not finished ([`orchestration-v2/`](https://github.com/pingdotgg/t3code/tree/main/apps/server/src/orchestration-v2)).
- Retries are idempotent through command receipts. Event log kept ~1 day for reconnect replay; clients with an old cursor resume from a snapshot.
- Run vs ProviderTurn are separate: one app-level run can have several provider attempts (`initial`, `steering_restart`, `retry`, `provider_recovery`). Steering modes: `steer_active`, `queue_after_active`, or interrupt and restart if the provider cannot steer ([feature-lifecycles](https://github.com/pingdotgg/t3code/blob/main/docs/orchestration-v2/feature-lifecycles.md)).
- Turn end and follow-up work (checkpoint, diff, PR discovery) are separate milestones, so a slow diff never makes the UI show the agent as still working.

### Checkpoints and undo
- `CheckpointStore` writes workspace state to **hidden git refs**, no commits on the user's branch. Gives per-turn diffs and restore. A revert must coordinate the file system with the provider conversation, and a provider that cannot roll back its conversation must refuse before touching files.

### Work that continues without the person
- **Thread settlement is server-owned**: `ThreadSettlementService` evaluates PR and inactivity settings with no client connected; a guarded `thread.auto-settle` command rejects newer activity, overrides and live work, and detaches idle sessions. `PullRequestSyncReactor` re-reads PR links when a run that ran `gh pr merge` ends, because no webhook arrives.
- Orchestrator **MCP server** (`http://127.0.0.1:<port>/mcp`): an agent can spawn sub-agents on any provider, wait for or poll results, cancel, create threads, send or steer messages, interrupt, and **schedule recurring work that runs without an active turn**. Child gets only its task prompt, not parent history ([doc](https://github.com/pingdotgg/t3code/blob/main/docs/orchestration-v2/orchestrator-mcp-server.md)).
- Background service: `t3 service install|status|restart`, `t3 update` that asks before restart interrupts running turns.
- Mobile: push when an agent finishes, fails, needs approval or asks input; iOS Live Activity and Android live updates ([notifications](https://github.com/pingdotgg/t3code/blob/main/docs/user/mobile-notifications.md)).
- Remote: same server over LAN, Tailscale or their tunnel; guest links.

### Provider switching and context
- Switching provider mid-thread uses a **budgeted handoff**, not an agent summary: recent turns, original request, command outcomes, references to omitted history that the agent can read back through a thread-reading tool; token cap default 16k. If the target has no room it starts a fresh conversation automatically ([portable-handoffs](https://github.com/pingdotgg/t3code/blob/main/docs/user/portable-handoffs.md)).
- Thread forks, lineage and context transfer docs in `docs/orchestration-v2/`.

### UI patterns
- Performance is a stated product value; dropped frames are bugs; regular audits for oversized websocket payloads, gpu-heavy CSS animation, hard-to-render lists ([AGENTS.md](https://github.com/pingdotgg/t3code/blob/main/AGENTS.md), `performance-regressions.md`).
- Tool calls collapse into one summary line ("Ran 2 commands and sent messages to 3 threads"), with live-updating output and "Open diff"; setting to hide successful calls.
- Composer banners as slot components with a priority stack; permission mode as a per-thread picker (Supervised / Auto-accept edits / Auto / Full access); command palette; subagent approvals surface in the parent thread.
- Safety rule worth stealing: setup never happens as a health-check side effect (probes must not start MCP servers, hooks or login browsers).
- Prompts: the repo has no public hidden system prompt; agent instructions are the AGENTS.md above and a "triage playbook" linked into a thread.

## Zeron ([zeronsh/zeron](https://github.com/zeronsh/zeron), MIT)

Sources: [ARCHITECTURE.md](https://github.com/zeronsh/zeron/blob/main/ARCHITECTURE.md), [README](https://github.com/zeronsh/zeron#readme).

- **Engine / viewport split.** `zeron-engine` is a Rust daemon that owns runs, terminals, worktrees and docs; the UI only renders. The app spawns it detached (not via a service manager, so macOS privacy permissions stay attributed to the app), so quitting the app leaves every session running. If the engine dies unasked: restart after 1, 2, 5, 15 s; a fifth loss in five minutes stops and shows Retry. It exits itself if the app is trashed. Logs rotate at 64 MB.
- **Run journal and crash auto-resume.** On-disk journal with resumable `seq` replay; recovery stamps interrupted runs `aborted` and auto-resumes; 10-minute **stall watchdog**; idle reaper for persistent sessions.
- **Durable command queue.** Send, steer, interrupt and answer-input are queue entries in the session doc; steer with no live run becomes the next turn; offline sends queue and execute when the device returns. Steering mailbox is read at step/turn boundaries.
- **Update as a transaction.** Install changes version in one atomic step only after the new version proves ready; the old version copies SQLite stores, leaves a watchdog, hands over; the new one holds back irreversible work (resuming chats, queued messages) until committed; a failed trial restores the stores. After an update the engine restarts only once no run or terminal is live.
- **Desktop notifications from the engine**, not the app: finished runs, questions, failures keep arriving with the app closed; "only in the background" uses focus the app reports.
- **Attention-sorted sessions list** in the sidebar; tabs are a device-local view on it; closing a tab is not archiving.
- **Multi-device and cloud**: Loro CRDT docs synced through Cloudflare Durable Objects; a session can run on a per-session hosted sandbox that sleeps when idle, with a vault Worker serializing credential refresh so parallel sessions never race a refresh token.
- Also: headless VPS mode (`zeron headless`, Linux installer sets a boot service), local-first profile that never uploads existing sessions on login, `CONTEXT.md`/`ARCHITECTURE.md` written for agents.

## Ranking for "super agent that keeps working"

1. **Stall watchdog + crash auto-resume with a run journal** (Zeron). Directly fixes "it just keeps working".
2. **Server-owned settlement and PR sync with no client** (T3). Merge, close and park decisions never wait for a UI.
3. **Orchestrator MCP tools for the chat agent** (T3): spawn tickets, steer, wait, schedule. Makes the chat a real chief of staff.
4. **Durable command queue with steer / queue / interrupt-restart** (both).
5. **Update as a trial with rollback and hold-back of irreversible work** (Zeron).
6. **Hidden-ref checkpoints per run** (T3): undo and per-run diffs without polluting branches.
7. Command receipts and outbox effects (T3); budgeted provider handoff (T3); collapsed activity summaries (T3); engine-owned notifications (Zeron).

## Neko today vs these (verify before acting)

Neko has tickets in worktrees, parking, responsibility wakes, phone push and self-update. It lacks (from this research, not an audit): a per-run journal with seq replay, an explicit stall watchdog for Claude/OpenCode (the CLAUDE.md notes they emit no command-start events), and transactional update rollback.

## Copy / adapt / avoid

- **Copy (idea):** watchdog; hidden-ref checkpoints (`refs/neko/checkpoints/<run>`); restart backoff ladder 1/2/5/15 s with a 5-in-5-min stop; handoff budget design.
- **Adapt:** orchestrator MCP as a Neko-owned endpoint with per-run scoped lease (Neko already scopes MCP); update trial using the existing `<data>/source` clone plus SQLite copy; settlement as a tick rule using `PRAGMA`-cheap reads.
- **Avoid:** CRDT sync and cloud sandboxes (large, and Neko is local-first); a 1-day event log that cannot rebuild projections; per-thread MCP entries for OpenCode (directory-scoped collision, see providers.md); "Full access" as default permission.

## The chat directs the crew (2026-11-14)

Research ideas 3 and 4 in `docs/research/t3code-and-zeron.md` (orchestrator
tools for the chat agent, a durable command queue). Before this the full-power
chat acted on Neko only by shelling out to `neko-daemon --request` and by
fenced blocks (`neko-ticket`, `neko-action`); a message to a running task went
through `ReplyToTask`, which stopped the turn and replanned (an interrupt-restart
whether the person meant it or not), and `AddTicketNote` was only read at the
next phase.

### The tools (MCP, over the existing `--mcp-bridge`)

No new server: the stdio bridge every run with a lease already gets
(`neko_core::mcp_host::bridge`) lists nine more tools and sends them to the
daemon as `BridgeAction::Crew{tool, arguments_json}`. The daemon
(`workbench/crew.rs`, `Controller::crew_call`, dispatched from `server.rs`)
looks up the lease's **crew role** (`neko_protocol::crew::CrewRole`, a field of
the capability `Scope`); the bridge asks the daemon which tools that role may
use and shows only those. Claude Code gets it through `--mcp-config`, Codex
through `-c mcp_servers.neko` (`enabled_tools` and approvals list the crew names;
the daemon, not that list, is the authority), OpenCode through its config.

| Tool | Full chat | Read-only chat | A ticket's own run |
| --- | --- | --- | --- |
| `list_tasks`, `task_status` | yes | yes (its workspace when scoped) | yes (its ticket's workspace) |
| `wait_for` (task or goal, until a state or a timeout, max 30 min) | yes | no | yes |
| `create_task` (title, request in the person's words, workspace, folder, optional goal) | yes | no | sub-tasks of its own ticket only, same workspace, never from a read-only ticket or a watch's ticket |
| `steer_task`, `cancel_task`, `schedule` | yes | no | no |
| `save_artifact`, `show_artifact` | yes | no | no |

Leases: the chat gets its role in `run_with` (`Full` when full power is on, else
`ReadOnly`), a chat with several workspaces and none chosen gets a crew-only lease
(`Host::crew_lease`, no connection tools); a ticket's builder or planner session
gets `Task{ticket_id, may_create}` in `responsibilities::with_lease`. Reviews and
watches get none. Dropping the lease (run end, Stop) ends the capability and any
`wait_for` in flight (`run_ended`).

Behaviour worth knowing:

- **Idempotent.** Mutating calls (`create_task`, `steer_task`, `cancel_task`,
  `schedule`) cache their result under `run id + tool + arguments` (or the caller's
  `idempotency_key`), bounded to 200 rows in `neko_crew_idem_v1`; a repeat returns
  the first result with `duplicate: true` and does nothing.
- **Audited.** Each mutating call leaves a `crew` event on the task
  ("chat: queued a message (steer now): …", "Opened by chat: …", "cancelled the
  task").
- **create_task** makes an ordinary ticket (`CreateTask[InFolder]`, `request` kept
  verbatim). From the full chat it builds without a plan approval
  (`start_when_planned`, as for a goal: the person asked their own chat); a
  sub-task still waits for approval. A `goal` object attaches a goal, which waits
  **paused** until the person reads its check, same as a `neko-goal` block. The
  task is linked to the reply in flight (`neko_chat::link_ticket`), so it shows as
  the existing crew line (`Neko → Builder  NEK-1BB3: …`) with its live status.
- **Cap.** One chat may have 8 open delegated tasks, a ticket 8 sub-tasks
  (`neko_core::crew`, origin rows in `neko_crew_origin_v1`); `Crew::SetDelegationCap`
  (1 to 64) raises it. Creating is cheap and queues; admission, the writable-per-repository
  cap and `machine-ok` decide when each starts, so a burst never starts at once.
- **wait_for** polls the snapshot every second; `until` is `settled` (default for
  tasks: awaiting approval, ready for review, completed, failed, cancelled),
  `finished`, or a status name; for a goal `finished` (default), `settled`
  (adds blocked and paused) or a status. A timeout returns `timed_out: true`, not an error.
  The bridge's read timeout and Codex's `tool_timeout_sec` are 1900 s for it.
- **schedule** saves and enables a read-only plan through the schedules Neko has
  (`ScheduleCommand::Save` then `SetEnabled`): `at_unix_ms` or `in_minutes` makes a
  one-off (`FREQ=DAILY;COUNT=1`), `recurrence` an RRULE with `timezone`. Each run
  opens a task for approval; nothing changes by itself.

The full-power instruction (`neko_chat::super_instruction`) has a paragraph: delegate
substantial work, open independent pieces separately, `wait_for` or check
`task_status`, report, and never call a task done before its status says so;
small things the chat just does. Read-only chat is told it can look
(`INSTRUCTION`). Goldens updated.

### The durable command queue

Messages to a running agent are entries in `neko_core::command_queue`: a row per
task (`neko_cmdq_v1:<task_id>`) and an index, **never the snapshot** (readers get
`Snapshot::queued_messages`, attached like goals and cleared in `encode`). An entry
has an id, a kind (send, steer, interrupt, answer), a mode, text, a state (queued,
delivered, cancelled, dropped), its source and how it was delivered. The same
`(task, id)` never queues twice, even after delivery, so a web retry is one
message. 32 queued per task; 12 settled kept.

Delivery (`Controller::deliver_queued`, every scheduler tick and right after a
message is queued; `plan()` is the whole rule):

| Mode | Task is in a turn | Task is between turns |
| --- | --- | --- |
| `after_turn` (default) | waits | delivered as a reply (a task that has not started takes a note) |
| `steer_now` | into the live session if the runtime takes input mid-turn, **else waits for the turn boundary** (the entry says so) | same as above |
| `interrupt_restart` | stops the worker and restarts the turn with the message (`ReplyToTask`) | same as a reply |

No runtime Neko drives takes input mid-turn today (`claude -p` and `codex exec`
read their prompt once), so the real injector declines and `steer_now` arrives at
the next boundary, with a visible reason. The seam is `deliver_queued_with(live)`;
a runtime that can will plug in there. Further messages join a reply that already
waits, in order. A cancelled task's entries are dropped with a reason; a taken-back
one is never delivered. Delivery is at-least-once: the entry is settled right after
the command applies, so a crash in between could deliver it again.

Web (`web/src/work/queue.tsx`, `TaskComposer`): while a task is planning, building
or reviewing, the composer shows the three modes (default after this turn; ⌘↩ sends
steer now) and one line per waiting message with Cancel. The composer keeps one
command id per text and mode until it succeeds. Between turns the composer is
unchanged. `scripts/verify/crew.mjs` drives it against a seeded building task.

### Not done

- No live injection for any runtime (see above); `answer` is in the kind list but
  the "answer the agent's question" path still uses the reply composer.
- The cap counts by origin, not by conversation id: there is one Home chat.
- `schedule` makes read-only plans only; a timed message to an existing task is not a tool.
- Native (Swift) app ignores the queue field and offers no modes.
- Settings has no control for the delegation cap (`Crew::SetDelegationCap` via `--request`).

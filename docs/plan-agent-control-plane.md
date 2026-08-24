# neko as an agent control plane — the build plan

neko today is a launcher that can *see* agents and start one. Paseo is a
control plane you have to switch to. This plan closes that: neko becomes the
keyboard-speed surface for everything you currently open Paseo to do, and
then becomes an agent itself.

Each layer is usable on its own and is the foundation of the next. Nothing
below L0 is possible; nothing at L6 makes sense without L0–L5 as its action
space.

## What was surveyed

Paseo's daemon (`refs/paseo`, AGPL-3.0 — **read for shape, every line here
written fresh**, the same rule `refs/README.md` sets for every clone) exposes
its whole agent surface at `POST /mcp/agents` as Model Context Protocol over
HTTP. Confirmed live against the running daemon on 2026-08-25: **61 tools**,
and `isAgentMcpRequestAuthorized` returns `true` outright when no daemon
password is configured, which is this machine's state.

That is the whole reason this plan is cheap. One client, 61 capabilities. The
WebSocket route this project bounced off during `/usage` — undocumented
`protocolVersion`, session routing nobody writes down — is not needed at all.

| group | tools | what it buys neko |
| --- | --- | --- |
| agents | `list_agents`, `get_agent_status`, `send_agent_prompt`, `cancel_agent`, `archive_agent`, `kill_agent`, `update_agent`, `set_agent_mode`, `get_agent_activity`, `create_agent` | act on an agent without leaving the panel |
| permissions | `list_pending_permissions`, `respond_to_permission` | the inbox — see L2, the centre of this plan |
| schedules | `create_schedule`, `list_schedules`, `inspect_schedule`, `pause`/`resume`/`delete`/`update`, `run_schedule_once`, `schedule_logs` | cron for agents |
| heartbeats | `create_heartbeat`, `delete_heartbeat` | recurring prompts to yourself |
| terminals | `list_terminals`, `create_terminal`, `kill_terminal`, `capture_terminal`, `send_terminal_keys` | peek at and drive a session |
| workspaces | `list_workspaces`, `create_workspace`, `archive_workspace`, `rename_workspace`, `list_workspace_scripts`, `start`/`stop_workspace_script` | run the dev server without a terminal |
| providers | `list_providers`, `list_models`, `list_profiles`, `inspect_provider` | choose model/mode when spawning |
| browser | 22 `browser_*` tools | **out of scope** — a launcher is not a browser driver |

## Layers

### L0 — the control channel

`neko_core::mcp`: a minimal MCP-over-HTTP client. `initialize`, `tools/list`,
`tools/call`; SSE `data:` framing; endpoint discovered from
`~/.paseo/paseo.pid`'s own `listen` field, never hard-coded. `curl` on stdin
for the transport, the same trade `usage.rs` made and for the same reason —
no async runtime, no TLS stack, and it is loopback.

Everything above depends on this and nothing else. A daemon that is not
running is a first-class answer ("Paseo isn't running"), never an error
dialog.

### L1 — act on an agent

Agent rows gain real actions in the `⌘K` menu: Send prompt, Cancel, Set mode,
Archive, Kill. Costs no protocol work — `SearchItem::actions` and
`Provider::perform_action` already exist and already carry the
destructive-confirm rule.

### L2 — the permission inbox

The one that changes what neko is. Agents block waiting for approval, and
today the only way to notice is to switch to Paseo. `list_pending_permissions`
returns every blocked agent across every workspace; `respond_to_permission`
answers one. In neko: its own provider, Enter approves, `⌘K` denies, and the
panel says how many are waiting the moment it opens.

A launcher's whole job is the shortest path from "something needs me" to
"handled". This is that path.

### L3 — ambient awareness

neko is already resident. A bounded poller keeps the inbox count and agent
liveness fresh without a summon, so the answer is on screen the instant the
panel opens rather than a round-trip later. `create_heartbeat` exposes
Paseo's own recurring-prompt mechanism through the same surface.

### L4 — schedules

A `Schedules` mode: what is scheduled, when it next runs, its recent runs,
pause/resume/run-now. Reuses the mode machinery and the meter/detail
vocabulary already built.

### L5 — terminals and workspace scripts

`list_terminals` + `capture_terminal` gives a read-only peek in a detail pane;
`start`/`stop_workspace_script` makes "restart the dev server" a palette row
rather than a terminal tab.

### L6 — neko as an agent

The palette takes a sentence and acts. The action space is L0–L5's tools; the
planner is one model call through the credential path `neko_core::usage`
already reads from the Keychain. "restart triage-fe's dev server" resolves to
`start_workspace_script`, shows what it will run, and needs one Enter.

Two rules it ships with: it proposes, you confirm — never a silent tool call;
and it can only reach tools neko already exposes as rows, so nothing becomes
possible through the agent that was not already possible by hand.

## Order and why

L0 is the only hard dependency. L2 is the feature that justifies the plan and
is deliberately second, not last. L1 comes before it only because it proves
the channel on a smaller surface. L4/L5 are breadth. L6 is the payoff and is
last because its value is exactly the size of the action space beneath it.

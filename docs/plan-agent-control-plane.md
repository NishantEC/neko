# neko as an agent control plane — the build plan

**Built, 2026-08-25.** Every layer below shipped; the two that turned out
differently say so in place, and `AGENTS.md` carries the decisions. Kept as
written — with corrections marked — because what a plan got wrong is worth
more later than a plan tidied to match the result.

When this was written, neko was a launcher that could *see* agents and start
one, and Paseo was a control plane you had to switch to. Each layer is usable
on its own and is the foundation of the next: nothing below L0 is possible,
and nothing at L6 makes sense without L0–L5 as its action space.

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

### L0 — the control channel ✅

`neko_core::mcp`: a minimal MCP-over-HTTP client. `initialize`, `tools/list`,
`tools/call`; SSE `data:` framing; endpoint discovered from
`~/.paseo/paseo.pid`'s own `listen` field, never hard-coded. `curl` on stdin
for the transport, the same trade `usage.rs` made and for the same reason —
no async runtime, no TLS stack, and it is loopback.

Everything above depends on this and nothing else. A daemon that is not
running is a first-class answer ("Paseo isn't running"), never an error
dialog.

### L1 — act on an agent ✅

Agent rows gain real actions in the `⌘K` menu: Send prompt, Cancel, Set mode,
Archive, Kill. Costs no protocol work — `SearchItem::actions` and
`Provider::perform_action` already exist and already carry the
destructive-confirm rule.

### L2 — the permission inbox ✅

The one that changes what neko is. Agents block waiting for approval, and
today the only way to notice is to switch to Paseo. `list_pending_permissions`
returns every blocked agent across every workspace; `respond_to_permission`
answers one. In neko: its own provider, Enter approves, `⌘K` denies, and the
panel says how many are waiting the moment it opens.

A launcher's whole job is the shortest path from "something needs me" to
"handled". This is that path.

### L3 — ambient awareness ✅ *(heartbeats: not possible)*

A daemon poller keeps the inbox warm, so blocked agents land in the panel's
*first* frame, and broadcasts the count to a Dock-tile badge — the one
ambient surface neko has, since notifications need a bundle identifier and
gpui's menu-bar API is dead code.

**`create_heartbeat` turned out to be out of reach, and this is the correction
rather than a deferral.** Paseo answers a top-level caller with
`create_heartbeat requires an agent-scoped session`: the tool sends a prompt
to *the calling agent*, and neko is not one. Tested against the live daemon
rather than assumed. It becomes possible only if neko ever has an agent
identity of its own.

### L4 — schedules ✅

A `Schedules` mode: what is scheduled, when it next runs, its recent runs,
pause/resume/run-now. Reuses the mode machinery and the meter/detail
vocabulary already built.

### L5 — terminals ✅ *(workspace scripts: nothing to build against)*

A `Terminals` mode: what is open, each one's last screen in the detail pane,
kill in `⌘K`.

**Two corrections to what this section originally said.** It assumed
`list_terminals` needed a fan-out over every workspace — it takes `all: true`,
and the fan-out would have been forty-four subprocesses per keystroke. And
**workspace scripts were not built**, because there is nothing on this machine
to build against: `list_workspace_scripts` requires a `workspaceId` and all 22
real workspaces return `{"scripts": []}`, with no repo carrying a Paseo config.
Writing it would have been unverifiable code — the same bar that kept Cursor,
Kimi, MiniMax and Z.AI out of `usage`.

### L6 — neko as an agent ✅

The palette takes a sentence and acts. The action space is L0–L5's tools; the
planner is one model call through the credential path `neko_core::usage`
already reads from the Keychain — confirmed live that the Keychain OAuth token
drives `api.anthropic.com/v1/messages` with tools, which was the make-or-break
unknown for this layer and was tested before a line was written.

**The example this section originally used no longer applies**: it said
"restart the dev server" resolving to `start_workspace_script`, and
workspace scripts were not built (see L5). What it really does, verified:
*"stop whatever the neko agent is doing"* → `cancel_agent(agentId: 05475348-…)`,
rendered as a row, run only on a second Enter.

Two rules it ships with, both enforced rather than intended: it proposes, you
confirm — planning and running are separate keystrokes with the exact call
between them — and it can only reach the eight verbs in `ask::CATALOG`, so
nothing becomes possible through the planner that was not already possible by
hand. `tool_choice` is `auto` rather than `any`, so a request no tool can serve
comes back as a sentence declining instead of a confidently wrong call.

## What was built that the plan did not name

Three things the layers implied but nobody wrote down:

- **An `Agents` mode.** L1 gave an agent row actions; the mode gives it a
  keyboard pointed at the thing you actually came to do — say something else
  to it. Enter sends a follow-up prompt, `⌘K` sets the session mode.
- **`SearchItem::keeps_open` and `SearchItem::preview`.** L6 needed a row that
  performs a *step* rather than a finish, and L5 needed many lines of text a
  row cannot hold. Both are additive wire vocabulary, neither is a
  provider-identity switch.
- **`action_label` rendered at all.** Every provider had always set it and the
  footer that displayed it had been removed, so it was live data going
  nowhere — which made a schedule's Enter (pause *or* resume) and an `ask`
  row's Enter (run a proposed call) both unlabelled. Found by photographing
  the surfaces, not by reading them.

## Order and why

L0 is the only hard dependency. L2 is the feature that justifies the plan and
is deliberately second, not last. L1 comes before it only because it proves
the channel on a smaller surface. L4/L5 are breadth. L6 is the payoff and is
last because its value is exactly the size of the action space beneath it.

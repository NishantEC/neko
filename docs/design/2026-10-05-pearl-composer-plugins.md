# Pearl composer and plugin direction

Status: proposed, not implemented. Paper mockups reviewed on 2026-10-05.

**2026-10-06 update:** the native Design lab implements the visual study. The user
selected Inline; production adoption and the plugin host remain proposed. The
following revision supersedes the earlier static bead and “3 agents” examples.

## Selected Inline revision: task ownership and pearl material

The primary chat is Neko. A task has one saved planning/building session; a fresh
reviewer checks the result. A reviewer, scout or recovery supervisor is a phase
role, not a new child task to add to the displayed team count. There is no global
three-agent cap. The existing split proposal is deliberately bounded to two or
three subtasks and approval creates the corresponding child tickets.

Show **Neko** when the current ticket has no created children. Once a split is
approved, show **N subtasks**, calculated from distinct `splits[].subtasks[].task_id`
values belonging to this `parent_id` that resolve in `snapshot.tasks`. Exclude
unapproved proposals, missing IDs, duplicates and the parent. Show the owner
separately in the popover. Never derive the number from plugin count, model count,
saved sessions or the total number of workspace tickets.

The count means membership, not simultaneous execution. Planning/Building/Reviewing
mean working; Queued means waiting; AwaitingApproval/ReadyForReview need the user;
Completed is done; Failed/Cancelled are stopped. Dependency-blocked children are
still subtasks, but are not labelled working. These are durable task statuses,
not measured live process counts. `TaskTeamSummary` implements this calculation
against the protocol snapshot shape and is currently consumed by sample data
only. No production chat integration is claimed.

The native preview offers One task, Split task (one working, one dependency waiting)
and Review examples. Inline retains its compact integrated control. The earlier
Context comparison remains available. Decorative orbs now use a Metal nacre
shader with broad softbox highlights and a softly coloured body instead of a
near-black metallic rim. Whole-button pointer tracking moves the light; hover
adds restrained scale/contrast, and press gives immediate feedback. No idle
TimelineView or timer runs for these beads. Reduce Motion keeps position/scale
still; Reduce Transparency or Increased Contrast selects an opaque fallback.
The actual composer continues to use system Liquid Glass.

Latest feedback: keep plugins and the agent controls; reject the detached dock. The integrated 23A/23B studies below supersede the original composer recommendation. The plugin library and setup direction in 22C/22D remain unchanged.

## Review the designs

[Paper: Neko — Redesign](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-1-0), page **Approved · Native macOS**. Existing 20A and 21A/B remain intact; these new boards are explicitly proposals despite the page name.

| Board | Purpose |
| --- | --- |
| 22.0 · Plugins — product flow & build boundary | Outcome through discovery, inspection, installation, connection and use; current versus proposed extension boundaries. |
| 22A · Pearl — tactile composer | Earlier reference. Attachment chip, shaded beads and spherical send control, within the existing native sidebar. |
| 22B · Dock — conversation with agent shelf | Rejected direction: detached agent shelf. Retained for comparison. |
| 22C · Plugins — discover and connect | Unified discovery and installed states with a selected plugin's capabilities and setup. |
| 22D · Add a plugin — inspect, connect, ready | Native sheet states, return to the original task, and actionable failure states. |
| 23A · Inline — agents and plugins in one composer | Recommended revision. Labeled Agents and Plugins controls share Pearl's integrated bottom row. |
| 23B · Expanded — integrated agent context | Alternative. Agent names/status and Plugins live in a top row inside the same composer surface. |
| 23C · Controls — agent and plugin menus | Separate open-menu studies showing child chat navigation and chat-level plugin selection. |

Catalog names, versions, counts and task activity are illustrative. No real catalog publisher, plugin installation, account connection or successful agent run is implied.

## Buildable visual contract

- Keep the system source-list sidebar, full-page conversations and existing draft ownership. Use the shared composer for Home and agent chats.
- Preserve the existing native text editor, input handling, IME, selection, keyboard shortcuts, attachment handling and draft persistence. Apply the visual treatment around it; do not replace typing with a web editor.
- Keep conversation content opaque and neutral. Apply native Liquid Glass to controls and navigation chrome using the existing availability-gated helpers in `DesignSystem.swift`; retain a readable material/solid fallback on older systems and with Reduce Transparency.
- Render the decorative beads with static SwiftUI radial gradients and a restrained shadow. Their associated labels are the controls; color alone never conveys identity or status. No physics engine, custom GPU renderer or continuous animation is required.
- Preserve the expanded “Shift-Return for a new line” hint inside the controls row. No redundant Enter-to-send label. Send uses a 44-point hit target; attachment/menu targets are at least 32 points.

22A's “Propose 3 agents” maps to the existing bounded split proposal, with approval before child creation. It does not claim that arbitrary workers are already running. 22B's shelf depicts an approved three-child split and opens existing child-agent conversations. Do not fabricate context percentages when provider telemetry is absent.

For narrower windows, clamp the text column, wrap attachments, and collapse expanded agent context into a labeled menu. Keep the native sidebar collapsible. Verify keyboard focus, VoiceOver labels, dark appearance, Reduce Motion and Reduce Transparency during implementation.

### Integrated composer revision

23A keeps the draft area quiet and puts attachment, Agents and Plugins controls on one bottom row. Agent beads remain decorative companions to the “3 agents” label. 23B instead embeds agent names and current status into the composer's top context row, with Plugins at the trailing edge. Neither has a detached dock. Both preserve the spherical send control, expanded new-line shortcut hint and native sidebar.

The Agents popover lists only the current task's existing child agents. A row opens that child's full chat; “View all agents” opens the existing agents page. When no split exists, use the existing split-proposal flow rather than displaying fictional workers. The Plugins popover lists installed, ready capabilities available to this workspace, with a route to the library.

The illustrated plugin checkboxes propose a new persisted chat-level selection, not an existing feature. Implementation must pass that selection into the daemon's next run as a subset of available workspace capabilities; the UI must not claim deselection if the runtime ignores it. Selection changes apply to the next message/phase and do not silently restart an active worker. Plugin selection never expands existing connection or publication authority. Show a setup action for capabilities that need a connection.

Use one native popover at a time, anchored to its button. Escape or an outside click dismisses it and restores focus. In 23B, agent labels open child chats directly. At narrow widths, collapse the expanded row to the two labeled controls, allowing the shortcut hint to wrap inside the composer. Avoid simultaneous agent/plugin menus and decorative permanent panels.

## What DeepSeek Harness actually provides

The user confirmed DeepSeek Harness as the reference. Its official product describes a plugin-based harness. The architecture uses Cordis services, typed events and lifecycle effects; models, tools and the agent loop can be plugins. Bundles declare `dsh.bundle` in package metadata, and profiles compose bundles through layered configuration. This is broader than an MCP server definition.

Its plugin manager separates inspection, installation, activation and readiness. Dependency/version checks, profile effects, rollback and restart requirements are useful product patterns for Neko. Executable host bundles and web UI integrations depend on Harness APIs; installation is not automatic native compatibility.

Primary sources inspected:

- [Official DeepSeek Harness](https://www.deepseek.com/en/harness/)
- [Architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md)
- [Plugin manager](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/boot/plugin-manager/README.md)
- [Extension cookbook](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/cookbook/extension-cookbook.md)

## Neko compatibility and implementation direction

| Capability | Current Neko foundation | Proposed addition |
| --- | --- | --- |
| MCP tools | Daemon-owned `mcp_host`, connections, credentials and workspace scope; `setup_import.rs` for existing definitions | Bundle descriptor plus shared discovery, installation and readiness UI. Reuse current connection and access semantics. |
| Skills | `skills.rs` discovers/imports supported `SKILL.md` content | Present supported skills alongside tools. Preserve installer validation and review requirements; do not imply arbitrary bundled scripts are already supported. |
| DeepSeek host bundle | No Harness plugin host or compatibility adapter | Inspect metadata first. A separately managed Harness runtime and bridge is a future option for compatible services. Validate lifecycle, dependencies, versions and exposed capabilities with a real bundle. |
| DeepSeek web UI or agent-loop replacement | No drop-in native equivalent | Explicit native adaptation or a separately chosen external runtime. Neko retains its own task lifecycle and native interface by default. |

Neko continues to own tasks, workspaces, memory, agent state and publication authority. Connections remain user-configured; a generic catalog does not make a service a built-in dependency. Do not copy credentials into bundle metadata or change global Codex configuration. A separate process would provide lifecycle separation, not by itself a security boundary.

## Lifecycle behavior to implement

1. Inspect the package source, resolved version, supported format, capabilities and required setup before adding. Existing local executable trust and skill-review requirements remain applicable; keep the prompt relevant to the package type.
2. Install with durable progress and a retry for the failed step. Retain the previous working version on failed updates. Install, enable, connect and ready are distinct states.
3. Connect only when needed, in the selected workspace, then verify discovery/availability before displaying Ready. A skipped connection leaves a clear Connect action.
4. Return to the originating task with its draft and intent intact. Trying an example opens a draft; it does not silently submit or publish work. Installing a plugin does not automatically grant unattended execution.
5. Installed settings offer enable/disable, reconnect, update and remove. Show active use before removal or restart, retain task history, and avoid interrupting running work without an explicit action.

## Verification boundary

Paper screenshots were inspected for spacing, hierarchy, contrast, row alignment and clipping. The cloned sidebar footer was corrected to fit the window; the later menu study also aligned its footer actions. The eight boards and four exported composer PNGs are design deliverables. Existing source seams were checked for feasibility; no application code or plugin host was changed, no packages/accounts were installed or connected, and no daemon restart was performed. These mockups do not verify native glass rendering, typing performance or actual plugin interoperability. No application tests were rerun for these documentation/design-only changes.

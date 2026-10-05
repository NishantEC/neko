# Pearl composer and plugin direction

Status: proposed, not implemented. Paper mockups reviewed on 2026-10-05.

## Review the designs

[Paper: Neko — Redesign](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-1-0), page **Approved · Native macOS**. Existing 20A and 21A/B remain intact; these new boards are explicitly proposals despite the page name.

| Board | Purpose |
| --- | --- |
| 22.0 · Plugins — product flow & build boundary | Outcome through discovery, inspection, installation, connection and use; current versus proposed extension boundaries. |
| 22A · Pearl — tactile composer | Recommended. Reference-inspired attachment chip, shaded beads and spherical send control, within the existing native sidebar. |
| 22B · Dock — conversation with agent shelf | Alternate. Active child agents have labeled controls above a compact composer. |
| 22C · Plugins — discover and connect | Unified discovery and installed states with a selected plugin's capabilities and setup. |
| 22D · Add a plugin — inspect, connect, ready | Native sheet states, return to the original task, and actionable failure states. |

Catalog names, versions, counts and task activity are illustrative. No real catalog publisher, plugin installation, account connection or successful agent run is implied.

## Buildable visual contract

- Keep the system source-list sidebar, full-page conversations and existing draft ownership. Use the shared composer for Home and agent chats.
- Preserve the existing native text editor, input handling, IME, selection, keyboard shortcuts, attachment handling and draft persistence. Apply the visual treatment around it; do not replace typing with a web editor.
- Keep conversation content opaque and neutral. Apply native Liquid Glass to controls and navigation chrome using the existing availability-gated helpers in `DesignSystem.swift`; retain a readable material/solid fallback on older systems and with Reduce Transparency.
- Render the decorative beads with static SwiftUI radial gradients and a restrained shadow. Their associated labels are the controls; color alone never conveys identity or status. No physics engine, custom GPU renderer or continuous animation is required.
- Preserve the expanded “Shift-Return for a new line” hint inside the controls row. No redundant Enter-to-send label. Send uses a 44-point hit target; attachment/menu targets are at least 32 points.

22A's “Propose 3 agents” maps to the existing bounded split proposal, with approval before child creation. It does not claim that arbitrary workers are already running. 22B's shelf depicts an approved three-child split and opens existing child-agent conversations. Do not fabricate context percentages when provider telemetry is absent.

For narrower windows, clamp the text column, wrap attachments, and collapse the shelf into a labeled agent menu. Keep the native sidebar collapsible. Verify keyboard focus, VoiceOver labels, dark appearance, Reduce Motion and Reduce Transparency during implementation.

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

Paper screenshots were inspected for spacing, hierarchy, contrast, row alignment and clipping. The cloned sidebar footer was corrected to fit the window. The five boards and two exported composer PNGs are design deliverables. Existing source seams were checked for feasibility; no application code or plugin host was changed, no packages/accounts were installed or connected, and no daemon restart was performed. These mockups do not verify native glass rendering, typing performance or actual plugin interoperability. No application tests were rerun for this documentation/design-only change.

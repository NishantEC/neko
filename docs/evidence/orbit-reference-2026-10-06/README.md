# Orbit reference audit — 6 October 2026

Open the [Paper reference atlas](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-4-0). Compare it with [Neko · Orbit redesign](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-3-0).

This folder contains 161 unchanged JPEG browser captures, a DOM snapshot for each, an action/result manifest, the public-source inventory and the native capability comparison. The visible reference product calls itself Aurora; the public page title is Orbit Agent Workspace.

## Start here

| Artifact | Purpose |
| --- | --- |
| [Capture index](capture-index.md) | Every numbered screenshot, action and observed outcome. |
| [Caption QA](capture-qa.md) | Independent review of the original 158 captures, with final correction and follow-up evidence notes. |
| [Manifest](manifest.json) | Capture timestamp, source URL and local path. Some early filenames describe the intended state; the final caption records what was actually observed. |
| [Source behavior map](source-behavior-map.md) | Complete public-source inventory: pages, actions, menus, scenes, role definitions and implementation gaps. |
| [Native capability map](native-capability-map.md) | Neko source/adapters available for implementation; this is source verification, not a running-app test. |
| [Native direction](../../design/orbit-native-direction.md) | Design recommendation, navigation/task flows, parity contract and implementation sequence. |

## Browser coverage

| Flow | Captures | What was exercised |
| --- | --- | --- |
| Home and basic overlays | 01–11, 75, 87–91, 121, 147–150, 158 | Seeded/idle Home, model/account menus, palette filtering, suggestions, multiline input and all five keyword-routing families. |
| Task creation and navigation | 10–14, 26–29, 92–93, 104–105, 131–134, 151–154 | Edited/empty task, assignment, busy queue, completed task, task actions, all four blank historical detail destinations and tab close. |
| Chat and composers | 15–21, 59–61, 83, 94–95, 136–143 | All seven recipients, thought/tool disclosures, canned code, offline acknowledgement, context menu, @ affordance, model selection and draft close/reopen. |
| Main pages | 22–40 | Projects, Agents, Tasks, Workflows, Files, Conversations, Runs, Activity and Settings, including representative no-op/preview destinations. |
| Agent cards and status variants | 41–58 | All seven role cards, nine status previews, right-click actions, paused state and disabled offline actions. |
| Full execution pipeline | 62–86, 159–161 | Plan, research, analysis inputs, handoffs, design variants, copy options, code diff, tests, deploy failure, defer, retry, remaining approval and final completion. |
| Scene shortcuts | 02, 62–84, 87, 92–114, 146, 157 | All 12 Scenes menu actions, including several agents working, agent inspector and reduced motion. |
| Command palette | 07–09, 106–111, 118 | Root, four nested categories, filter/no results, Backspace, Enter and Escape; root search does not index all entities. |
| Shell and responsive layout | 65–66, 80–82, 103, 115–130 | Sidebar/inspector/terminal, reduced motion, Command-B/K/J/I, desktop 1440×900, compact rail 768×1024 and mobile drawers 390×844. Other captures used the app's current viewport. |
| Persistence and boundaries | 134–146, 151–158 | Empty link/context handlers, preview sign-out, autonomy display state, missing @/N behavior, close/reopen continuity, no-open-tabs and reload reset. |

This is broad coverage of all discovered surfaces and unique interaction families, not proof of every timing combination, pointer path, keyboard sequence or breakpoint pixel. The source inventory also records aliases/placeholder rows whose equivalent destination is already captured; not every repeated inert row was separately clicked.

## Observed gaps in Orbit

- Four historical task rows open named but blank detail tabs. Most listed resources and historical conversations/runs have no destination.
- Create agent, Pause, Cancel, Sign out, Output and Problems show prototype notices. Context choices, Copy task link and Secrets Manage do not implement their advertised operation.
- Every chat recipient shares one transcript. Offline Ward replies with a canned acknowledgement; there is no delivery queue in the source. Home routes by keywords.
- All command output, generated artifacts, permission labels, approvals, model choices, durations and token counters are simulated. They cannot be used as evidence of a runnable harness.
- State is held in the page. Closing/reopening a tab retains content within that document; reload reconstructs the seeded state. Scene reset is partial and keeps some activity/chat/history.

## Evidence boundaries

Screenshots are original browser bytes. Paper previews preserve their aspect ratio; narrow captures are taller rather than stretched/cropped to 16:9. DOM snapshots can include hidden offscreen or collapsed content, so visible-state judgments use the screenshot when the two differ.

The page's public source was retrieved separately for a read-only behavior inventory. [Provenance](source/source-provenance.json) records the 198,126-byte snapshot and SHA-256. [Raw source](source/public-demo.html.txt) is stored as text and was not executed locally. No private accounts, credentials or Neko data were sent to the demo. Mock approval/token actions changed only the demo's in-memory state.

Pointer-hover motion was source-inspected but not manually verified. Static captures do not verify animation feel or timing. Full screen-reader traversal, focus trapping/restoration and every breakpoint boundary remain unverified. There was no real backend/provider/tool run to test.

The native proposal is static Paper work. Existing Neko routes and dispatch seams were inspected at `aef9df8` on `main`. No application source, daemon state, permissions or installed binary changed. Swift/Rust tests were not rerun for this documentation/design change; validate native interaction and materials during implementation.

# B · Execution workspace

Open [page 3 in Paper](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-3-0) to compare the four B artboards. All are **1440×900**, editable, screenshot-reviewed, exported and finished.

| Screen | Export | Direction |
|---|---|---|
| Home | [PNG](b-home.png) | Compact pearl, centered outcome composer, work overview and activity below. |
| Task execution | [PNG](b-task.png) | Approved plan/evidence-first workspace, compact conversation, Context/Team inspector and saved command receipts. |
| All agents — List and Board | [PNG](b-agents.png) | Same six illustrative tickets in both modes, with native table lanes and four work-state columns. |
| Tools & skills | [PNG](b-tools.png) | Connection list, skill preview and contextual connection/tool inspector. |

[Artboard and component IDs](direction-b.json) · [Required native map](../native-capability-map.md)

## Latest refinement

1. Home now uses a 64px static pearl cue derived from the existing `DesignLabOrb` palette, a 760px centered composer, and the preserved work table. Removed the oversized Continue card, duplicate connected label and Return-to-send footer.
2. Task execution is unchanged in this refinement. The freshly exported PNG is **byte-identical** to the previously approved export. It already had no send hint.
3. All agents adds List/Board comparison using cloned B chrome. **The two modes are stacked for review only**; production retains the existing mutually exclusive List/Board picker.
4. Tools & skills adds connected, sign-in-required and paused connection states; enabled/changed skill previews; selected connection details; tool search; and a schema disclosure.
5. Exported four PNGs, verified their 1440×900 dimensions, reviewed final images and finished **1216 B-owned nodes**. Only B artboards and descendants were changed.

## Shared app direction

The shell keeps a 232px source-list sidebar, 56px native titlebar safe area, 40px workspace tabs and 24px status strip. Dark content/evidence surfaces stay opaque. Existing CSS tokens, System Sans-Serif/Menlo, 28/20/15/13/12 type and 8/16/24 spacing remain; no global token changes or Dock.

B retains Orbit's persistent navigation, work visibility, resource context and bottom evidence. Task execution allocates 532px to plan/execution, 360px to compact conversation and 316px to Context/Team; receipts occupy 208px below. Tools uses an 860px main pane and 348px optional inspector. Both inspectors collapse before the primary work surface at narrower widths; drafts and selected resources must survive resizing.

Home supplies a clear starting point. All agents supplies the queue and lifecycle overview. Task execution supplies the work itself. Tools & skills supplies workspace access and instruction management. Workspaces, Watching, Schedules, Memory, Profiles, Settings and Design lab keep their existing routes in this same shell; their detailed states are not newly drawn in these four boards.

The pearl is a static **design cue**, not a Metal shader render. The buildable source uses pointer/press uniforms with no idle timer, Reduce Motion behavior and a solid fallback for reduced transparency/increased contrast. Reuse that view when implementing. Paper pixels do not verify native Liquid Glass, pointer response, accessibility or runtime behavior.

## Source-observed adapters and new composition — outside the UI

“Existing” below means inspected native code and the required map, not a live interaction or daemon test.

| Surface/action | Classification | Binding and boundary |
|---|---|---|
| Sidebar/workspace selection | Existing adapter | `NativeSidebar`, `NavigationSplitView`, AppModel selection. Preserve `stableSplitPane`, top safe area and return filters. |
| Workspace tabs and optional panes | New composition/navigation state | Tab selection, resource identity and pane visibility need state above the individual views. Preserve navigation-owned drafts. |
| Home pearl/composer | Existing views, new arrangement | `DesignLabOrb` plus `SharedComposer`/AppKit editor, runtime picker and Home `SendMessage`. Keep Home send/queue/stop semantics distinct from ticket replies. |
| Home work/activity overview | New composition | Current task status and retained activity; refresh time is not measured progress. New task focuses the Home composer. |
| Task plan/execution and Context/Team | New composition | Approved plans, child links, saved evidence, `TaskChanges` and `TaskTeamSummary`; the latter needs a production snapshot adapter beyond its Design lab caller. |
| Task reply, Stop, changes, folder | Existing adapters in new arrangement | `ReplyToTask`, `CancelTask`, `TaskChanges`, Finder reveal. Reply cancels/replans the prior worker under existing authority rules. |
| Bottom command evidence | New composition over existing receipts | Saved command, owner, duration, exit and retained output. Copy/close only; no shell prompt, input, cursor or execution control. Preserve shortening/damage/missing flags. |
| All agents List/Board/search | Existing adapters, new layout | `TicketsView` and AppModel search/filter. Review comparison shows both modes; runtime displays the selected mode. “All states” composes the existing filter state. |
| All agents state moves | Existing guarded actions | AwaitingApproval/Failed/Cancelled → Working uses `StartTask`; ReadyForReview → Done uses `CompleteTask` (local acceptance); active → Done confirms `CancelTask`. Review remains daemon-owned. |
| Work options / ticket selection | Existing adapters | Autostart preference, stopped visibility and clear-finished menu; row/card opens the full ticket and preserves return filters. |
| Tools Connections/Skills/Add | Existing adapters | Existing tabs; Browse MCP servers, Import from this Mac, Add manually menu. Connections remain user-owned and workspace-scoped. |
| Contextual connection inspector | New composition | Repositions existing connection detail sheet data/actions into a collapsible pane. Selection follows the active connection. |
| Refresh / Pause / Sign in | Existing adapters | `Mcp::Discover`, `Mcp::SetEnabled`, `Mcp::Authenticate`. Local trust remains a separate real grant. The pictured trust/connection values are illustrative. |
| Tool search/disclosures/schema | Existing data, new arrangement | Discovered tool names, descriptions, schema and server read-only annotations. No per-tool permission toggles; declarations are not a security boundary. |
| Workspace skills preview / Manage | New composition over existing Skills tab | Available/Enabled/Changed state and current workspace. Manage selects Skills. Review opens instructions; enabling an update uses the reviewed content hash. No new instruction-diff service is implied. |
| Universal editor, marketplace, cloud collaboration, publish/merge, interactive terminal | New capabilities, excluded | No such capabilities are drawn or implied. Local acceptance does not publish work. |

Sources: [native shell](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/NekoApp.swift:48), [pearl](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/DesignLabOrb.swift:3), [All agents](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TicketsView.swift:124), [guarded transitions](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TicketsView.swift:329), [Tools tabs and actions](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/ToolsView.swift:80), [connection detail](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/ToolsView.swift:395), [tool disclosures](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/ToolsView.swift:444), [skill state](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/ToolsView.swift:594).

## Sample entities and proof boundary

| Sample | Provenance and meaning |
|---|---|
| Ship dark mode / `preview-main` | Existing `DesignLabModel.sampleSnapshot` parent; Queued, displayed as Waiting for subtasks. |
| Shared colour tokens / `tokens` | Existing approved child; Building. |
| Appearance control / `settings` | Existing approved child; Queued, depends on tokens. |
| Team summary | **2 subtasks · 1 working · 1 waiting**, derived from distinct existing child IDs in the approved split. Combine/review is a phase, not a teammate. |
| Three additional All agents tickets | Illustrative standalone records for AwaitingApproval, ReadyForReview and Completed: Choose snapshot folder, Improve receipt wrapping, Keep chat drafts. Not members of the two-child sample. Board totals 1/3/1/1 count tickets. |
| Tools and skills | Project files, Documentation, Review service, the four discovered tool rows and two skills are illustrative fixtures. No connection, account or skill store was queried. |
| Task receipts / diff / conversation | Illustrative retained-output fixture, unchanged from the approved task design. The displayed `swift build` was not run by this design task. |

Membership sources: [reducer](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TaskTeamSummary.swift:7), [sample snapshot](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/DesignLabModel.swift:48). No fixed satellite role cast or phase-derived member count.

## Visual review

| Checkpoint | Verdict |
|---|---|
| Home | Pearl/composer centered, clear hierarchy, readable contrast, table/activity lanes aligned; all content fits. |
| Task | Approved composition preserved, saved output readable, no send hint or shell input. Export byte equality confirmed. |
| All agents List | Fixed icon/title/state/context/update/action slots align across six rows. |
| All agents Board | Corrected 72px cards to 80px and tightened inner gaps; metadata stays inside cards. Four columns fit without clipping. |
| Tools & skills | Connections, skill rows and inspector align; expanded schema stays readable; Add connection has a menu indicator; no overflow. |

Meaningful sections were reviewed during incremental writes. Final four-board screenshots and Home/Tools exported PNGs were inspected. Existing system font availability and tokens were checked in the design session. Export hashes/dimensions are saved in [export checks](b-export-checks.json).

## Native implementation acceptance — still unverified

1. Bind the new arrangements to existing AppModel/command adapters; exercise draft, filter, tab and selected-resource continuity through navigation and pane collapse.
2. Verify guarded ticket transitions, failure/cancel states and empty/search results. Never allow arbitrary moves into review or treat acceptance as merge.
3. Exercise connection loading/auth/discovery errors, removal while selected, local trust, tool schema states and skill hash changes; preserve workspace scope and existing authority.
4. Test actual receipt truncation/incomplete flags, missing/duplicate child IDs and retained prior attempts. Activity is not a full execution journal.
5. Check system appearance, Larger Text, VoiceOver, keyboard focus, Reduce Motion/Transparency and Increased Contrast on the native app.

No production files, audit manifests, A artboards or file-wide tokens were edited. No browser control, native runtime interaction, build or tests were performed. The parent owns exhaustive Orbit capture; these boards are a reviewed design direction rather than exhaustive implemented UI coverage.

Next review action (~2 minutes): open Tools & skills on page 3 and compare the contextual inspector with Task execution.

# Orbit demo: public-source behavior map

Open **Scenes → Replay the full run**, then **Build landing page** to capture the main execution. Keep the headline approval pending until after deploy recovery to expose the separate **Needs input** state.

**Public-source inventory.** This report records source findings; browser proof is linked separately in the capture index. Its original capture checklist is a research plan, not the final completion record. This investigation used only GETs of [the public root](https://orbit-agent-workspace.vercel.app/) and its declared Google Fonts stylesheet; no browser/Paper control, Neko changes, endpoint discovery, accounts or private data.

## Evidence and boundaries

| Item | Source-observed result |
|---|---|
| Snapshot | 2026-10-06 IST; 198,126 bytes, 2,567 lines; SHA-256 `bc8b0eea9ec65b9abf16a64e47bda4f8d54a9143d26599ee55aff3bf22d1fce0` |
| Files | [Original HTML](source/public-demo.html.txt), [manifest](source/source-provenance.json), [button-template index](source/button-source-inventory.json). The original combined source is retained; extracted working copies and HTTP headers were temporary research artifacts |
| Delivery | One inline application script, inline application CSS/SVG, external Geist/Geist Mono font CSS. No external application chunks referenced. Source line numbers below refer to `source/public-demo.html.txt`. |
| Routes | Only `/` was requested. All workspace navigation is in-memory tab keys; no URL/history router found. `/signup` and `/demo` occur inside the simulated code editor's string content, not workspace navigation. No other URLs were probed. |
| Execution | No application `fetch`, XHR, WebSocket, EventSource, beacon, service worker, cookies, localStorage, sessionStorage or IndexedDB usage found. No backend/model/shell/deploy execution exists in the inspected script. Clipboard copying is the one explicit external application effect. |

## Screens and seeded entities

**Entry state:** Home selected; five tabs pre-opened: Home, Build landing page, Chat, page.tsx, research.md. Six agents start idle, Ward offline. Terminal starts closed. `play('run')` is scheduled 700 ms after initialization and progresses in the background while Home stays selected. Default chat recipient Sage, model Opus 5.5, autonomy Ask first. [L941–942, L2547–2562]

| Surface / internal key | Contents and navigation | Source |
|---|---|---|
| Home / `home` | Atlas hero, five orbiting agent buttons, growing composer, four suggestion chips, conditional Needs you rows, first four tasks, last five activity items; View all → Tasks/Activity | L1764–1778, L1818–1889 |
| Projects / `projects` | Three display-only rows: `aurora-web` / `feat/landing`, `aurora-docs` / `main`, `aurora-brand` / `main`; agent avatars and updated labels | L1779–1782 |
| Agents / `agents` | Seven agent rows → agent card; Create agent → prototype notice | L1783 |
| Tasks / `tasks` | Five seed rows plus created tasks, owner/status/steps; New task dialog. All rows have tab handlers, including four seeds lacking detail renderers | L1368–1375, L1784 |
| Workflows / `workflows` | Landing page pipeline (6 steps), Weekly research digest (3), Release notes (4), Security review (5). Landing Run replays TASK-142; other Run buttons open generic New task | L1785–1789 |
| Files / `files` | Six rows: `app/(marketing)/page.tsx`, `docs/research.md`, `docs/recommendations.md`, `content/landing.md`, `design/hero/variant-b.png`, `components/marketing/index.ts`. Only first two open | L1790–1796 |
| Conversations / `conversations` | Landing page → shared Chat. Q3 signup numbers and Brand refresh feedback are display-only | L1797–1800 |
| Runs / `runs` | #218 landing page → TASK-142; #217 competitor digest, #216 OAuth, #215 illustrations are display-only. Historical duration/token metrics are literals | L1801–1805 |
| Activity / `activity` | Reverse chronological generated event list; empty state before any events | L1444–1489, L1806 |
| Settings / `settings` | Reduce motion switch, Default model menu, Ask first / Safe actions / Full, Secrets Manage button | L1807–1811 |
| Task / `task:142` | Status, owner/branch/elapsed, progress, six-step plan, execution log; Pause, More, Expand all, Collapse all; inline evidence, approval, failure/recovery | L1377–1442, L2153–2270 |
| Created task / `task:143` onward | New independent tab, selected agent, three scripted steps, queue if agent active or awaiting input; branch `fix/<number>` for coding, otherwise `main` | L2303–2341 |
| Chat / `chat` | One shared Landing page transcript; initial user request and Atlas response with thought disclosure/task/file references. Recipient/model/context controls; canned agent replies, expandable tool output, Forge code-copy button | L1630–1746 |
| Code / `file:page.tsx` | Read-only highlighted TSX. During main run: Forge editing chip, inserted/deleted lines, counts and dirty tab marker. No edit/save/run controls | L1535–1628, L1758 |
| Document / `file:research.md` | Static research article and example-domain citations; neither editable nor outbound links | L1759–1763 |
| No open tabs / `__empty` | Atlas avatar and command-palette hint after closing every tab | L1333–1344, L1812 |

Seed tasks: TASK-142 **Build the Aurora landing page** (Atlas, queued, 6 steps); TASK-141 **Weekly competitor digest** (Scout, completed, 3); TASK-140 **Migrate auth to OAuth** (Forge, completed, 7); TASK-139 **Onboarding illustrations** (Pixel, completed, 4); TASK-138 **Security review · release 2.3** (Ward, completed, 5). Source detail construction covers only 142 and newly assigned tasks. [L1368–1373, L1300–1312, L2550]

| Agent | Role / fixed model | Tools / displayed authority |
|---|---|---|
| Atlas | Orchestrator / Opus 5.5 | Planner, Files, Messages; assign work to any agent |
| Scout | Research / Sonnet 5.5 | Web, Files; read-only web/files |
| Sage | Analysis / Opus 5.5 | Files, Python, Docs; read files, sandboxed Python |
| Pixel | Design / Sonnet 5.5 | Canvas, Images, Files; write `/design` |
| Quill | Writing / Haiku 4.5 | Docs, Files; write `/content`, approval to publish |
| Forge | Engineering / Opus 5.5 | GitHub, Terminal, Files; write repository, commands, preview deploys |
| Ward | QA and security / Sonnet 5.5 | Terminal, Scanner; read-only repository/scanners; offline, “Back at 18:00” |

All names, permissions, weekly task counts and `SD` account initials above are **public seed literals**, not inspected user/account data. Agent definitions: L851–859.

Inspector Context stays tied to TASK-142/aurora-web/feat/landing. Four attached labels: research.md, customer_feedback.pdf, brand-guidelines.pdf, page.tsx; only the two file buttons open. Secret labels: DEPLOY_TOKEN initially not granted, ANALYTICS_KEY granted. Memory labels: “Tone: confident, plain, no hype” and “Pricing is per seat, never per agent.” These are display data, not real credentials or stored preferences. [L1224–1239]

## Action and overlay inventory

| Entry / overlay | Source-observed action and capture variants | Source |
|---|---|---|
| Shell | Sidebar toggle; inspector toggle; inspector Agents/Context tabs; crew status opens inspector (desktop selects Agents); terminal toggle/close; sidebar navigation; Account in sidebar aliases top-right account button | L2447–2498 |
| Tab strip | Open/select, close button, middle-click close, Enter/Space activation; last close shows empty view. Closing removes tab entry but retains DOM/content; reopening resumes it. Dirty/editing markers change during scenes | L1284–1355, L2485–2486 |
| Home hero | Atlas hover, pointer tracking, click/poke cycles four faces; satellites open agent cards; orbit slows on stage hover. Typing triggers a curious face. Empty submit focuses input; suggestion chips only fill it | L1818–1889, L2469 |
| Home send | Enter sends, Shift-Enter newline. Keyword routing priority: research/competitor/find/search/sources/compare → Scout; analysis/summary/feedback/data/insight/metric → Sage; design/visual/illustration/layout/mock/empty state/icon/hero → Pixel; writing/copy/headline/blog/release notes/announce → Quill; default Forge. A toast precedes assignment by 900 ms | L1861–1879 |
| Agent card | Sidebar, inspector, Agents table, satellites or palette open same floating dialog. Name/state/Now/model/tools/permissions/week count/recent four events; Message, Assign task, close. Ward Assign disabled by ID. Nine preview chips: Idle, Thinking, Working, Using tool, Waiting, Needs you, Done, Blocked, Offline | L1892–1926 |
| Agent right-click menu | Open inspector → card; Message → shared Chat recipient; Assign task → dialog (Ward disabled); Pause → sets global agent state to Waiting / Paused by you (offline disabled); View runs → general Runs | L1950–1959, L2488 |
| Model menus | Header, Settings and Chat offer Opus 5.5 / Sonnet 5.5 / Haiku 4.5, current checkmark, update labels and toast. Agent models stay fixed | L1960–1968 |
| Account menu | Signed in to Aurora heading; Settings → Settings; Keyboard shortcuts → command palette; Sign out → disabled-in-preview toast | L2498 |
| Task More menu | Copy task link → empty handler; Open in chat → shared Chat; View run #218 → Runs; Replay run → main replay; Cancel task → disabled-in-preview toast. Same menu even on newly created tasks | L2482 |
| New task dialog | Default text “Audit the pricing page for accessibility issues”; editable title, all seven agent radio buttons, offline disabled, Busy labels, Cancel and Assign to <agent>. Initial assignee Forge; last choice retained in memory. Empty title becomes Untitled task. Busy assignment queues | L2068–2080, L2303–2341, L2502 |
| Resolve dialog | Only when `S.blocked` exists: “Forge needs a deploy token”; seeded workspace-secret description; Not now or Grant access and retry. Grant changes local context label, adds event and resumes scripted main deploy | L2081–2086, L2280–2290 |
| Approval | Inline Approve and Discuss (shared Chat), sticky toast Approve, Home Needs you Approve. No approval modal. Clears pending headline approval, marks copy step complete | L2200–2207, L2291–2300 |
| Execution details | Row click or Enter/Space toggles evidence; Expand all/Collapse all. Failed deploy expands automatically. Detail types: plan, source list, research write, analysis inputs, three headline options, three CSS design variants, install/test command output, diff/file reference, deploy error/Resolve | L1445–1509, L2477–2480 |
| Chat composer | Agent menu includes all seven, including Ward; all choices share transcript. Model menu as above; Context menu has research.md, page.tsx, TASK-142 with empty handlers. Send streams fixed response per selected agent regardless of text. Ward produces offline acknowledgement; no queued delivery mechanism | L1630–1746, L2500–2511 |
| Chat disclosures / refs | Atlas thought and tool rows expand/collapse; TASK-142 and docs reference buttons open tabs. Forge canned reply includes actual Clipboard API code-copy request and success toast; failure is silent | L1660–1695, L1732, L2471, L2478 |
| Terminal | Output appears by scripted typed characters/lines; command prompt is presentation, not input. Hidden terminal gets output/error badge. Output and Problems buttons show notices and keep Terminal selected | L1512–1532, L2483 |
| Settings | Motion changes animation mode; model changes global display selection; autonomy only writes `S.autonomy` and pressed styles; Secrets Manage has no handler | L1807–1811, L2433–2439, L2470 |
| Toasts | Information, handoff, approval, blocked, completion; explicit dismiss; optional Approve/Resolve action. Normal default 4.2 seconds, critical notices sticky, maximum three visible; oldest is dismissed when a fourth appears, including sticky ones | L2089–2104 |

**Command palette:** top search / Cmd-or-Ctrl K / account Keyboard shortcuts. Root contains New Task, Open Agent, Run Workflow, Search Project (just Files), Search Files, Create Agent (notice), Switch Model, Open Settings, Go to Task, Go to Chat, shell toggles, motion and all 12 scenes. Four subpages: Agents (7), Models (3), Files (5), Workflows (3). Search filters only the current page by word matches; root does not index all agent/file contents. Files subpage has two openable files and three preview notices; the design PNG is absent. Workflows subpage omits Security review, and explicitly preselects Scout/Quill for digest/release notes, unlike the generic page Run buttons. Capture populated/filtered/no-results states, selection, breadcrumb, nested back and close. [L1974–2056]

**Keyboard/dismissal:** Cmd-or-Ctrl K palette, J terminal, B sidebar, I inspector, comma Settings. Palette Up/Down wrap, Enter executes, Backspace on empty subpage returns root. Menus Up/Down focus enabled entries. Escape precedence: palette → menu → dialog → agent card → drawers. Overlay scrim dismisses dialog/palette; mobile scrim closes drawers; outside click closes card/menu. Palette restores prior focus; dialogs/cards set initial focus, but no Tab focus trap or generic focus restoration found. Advertised `N` shortcut and typed `@` context affordance have no implementation found. [L1930–1948, L1998–2017, L2447–2543]

## Scene triggers: use to capture transient states

All 12 are available from status-bar **Scenes** and palette root. These are source traces, not proof that each scene completes in a browser. [L2344–2417]

| Scene label / key | Expected scripted surface/state |
|---|---|
| Replay the full run / `run` | Resets main world, starts six-step pipeline on current tab; open TASK-142 manually for execution capture |
| Idle and blinking / `idle` | Resets agents/main task, opens TASK-142, staggered blinks; visit Home afterward for idle hero |
| Assign a task / `assign` | Opens Pixel task dialog, fills “Design three hero variants for the pricing page”, automatically submits ~1.75 s later; manual New task is better for stable dialog capture |
| Thinking → streaming reply / `stream` | Opens Chat with Sage, appends research question and canned tool-backed answer |
| Tool execution / `tool` | TASK-142, Forge installing dependencies, terminal output and expandable result |
| Agent handoff / `handoff` | TASK-142, Scout → Sage findings.md transfer, handoff toast, Sage analysis |
| Blocked state / `blocked` | TASK-142, failed deploy, expanded error, sticky Resolve toast; manual grant required |
| Task completion / `complete` | TASK-142, Pixel generates three variants and completion toast; this is agent completion, not full task completion |
| Several agents working / `multi` | TASK-142, five distinct tool activities plus Atlas coordinating; no cleanup after its six-second wait |
| Command palette / `palette` | Opens root, types “agent”, enters first match (Agents), highlights third entry |
| Agent inspector / `inspector` | Scout card for 2.6 s, closes, then Forge card after 0.5 s |
| Toggle reduced motion / `reduced` | Switches reduced/full motion and shows toast |

**Main sequence:** Atlas plan → Scout search/read/write → Sage analysis and parallel Chat response → Pixel design and Quill copy in parallel → Quill requests headline approval → Forge install/edit/test → deploy fails → user grants mock token → retry succeeds → if headline still pending, TASK-142 enters Needs input → approval permits final Completed state. Capture plan/evidence both folded and expanded, code before/during/after edit, context before/after grant, and simultaneous approval+blocked Home/inspector states. Source allows mock deploy **before** headline approval; that approval gates final task completion, not deployment. [L2153–2270]

## Persistence, simulation and likely dead ends

| Finding | Source evidence / browser check still needed |
|---|---|
| Session memory only | `S`, `TASKS`, `ACTIVITY`, `TABS`, editor lines and DOM carry state. No storage/network persistence found. A reload is expected to recreate seeds and auto-run. Closed/reopened tabs retain content within the document. |
| Replay is partial reset | `resetWorld()` clears main log/plan, agent states, terminal, editor, pending approval/block, secret label and token counters. It does **not** clear created tasks, task numbering, accumulated Activity, shared Chat, model/autonomy, tabs or Home draft. Close-tab is not reset. L2141–2152. |
| Simulated work | Shell text, citations, generated variants, diffs, tests (42 passed), tools, deploy URL and agent permissions are literals/choreography. Tokens are random increments every 500 ms; timings and counts are not execution evidence. `.example` sources/deploy are plain text, not live links. L1275–1281, L1493–1509, L1630–1639, L2153–2341. |
| Definite empty handlers | Copy task link; all three Add context menu choices. Manage secrets lacks a handler. Terminal tab is already selected and has no switching handler. L1811, L2482, L2511. |
| Explicit prototype notices | Create agent, task Pause, Cancel task, Sign out, Output, Problems, and three non-openable palette files. Do not inventory these as implemented destination screens. |
| Display-only rows | All Projects; older two Conversations; older three Runs; four of six Files; PDF attachments and secret/memory rows; design variants and source citations. They lack navigation handlers, rather than being separate hidden screens. |
| Historical task detail gap | TASK-141/140/139/138 have clickable rows but no `VIEWS` renderer or `TaskView` instance. `ensureView` creates an empty section: likely blank task tabs, runtime-unverified. L1300–1312, L1784, L2550. |
| State preview affects global state | Card preview chips call `setAgent`, changing badges/inspector/queue eligibility. They do not create `S.blocked`/`S.approval` records: previewed Needs you/Blocked may show inert Home Approve/Resolve when revisiting Home. Scenes may overwrite previews. L1920–1926, L1884, L2082, L2292. |
| Pause is cosmetic | Task Pause only toasts. Agent context Pause only sets Waiting; no run cancellation or async coordination. Scripted work may continue/overwrite it. L1956, L2481. |
| Secondary scene inconsistency | Standalone Blocked scene calls `block(MAIN,5)` without a plan, then after grant celebrates Forge but never sets main task back to Running/Completed. Capture remaining task status separately. L2373–2387. |
| New-task/replay coupling | `TASKS.unshift` changes index 0, but Runs #218 reads `TASKS[0].status` and reset clears `TASKS[0].done`; could show the newest task's status/count instead of TASK-142. Mini-task waits omit the main `RUN` cancellation token, so replay need not stop them. L1802, L2151, L2313–2340. |
| Snapshot/stale presentation | Runs refreshes on activation, not each elapsed/token/status update. Activity stores the original event text and does not replace it when execution rows finish. Card preview does not itself re-render Home's Needs you section. Revisit views to compare. L1465–1486, L1748–1749, L1802, L1155–1187. |
| Decorative preferences | Autonomy has no decision-policy reader; model choice has no inference/provider call. “Offline message will be waiting” has no delivery queue. “Reusable pipeline creates a task” does not describe main replay or generic workflow dialogs accurately. |

## Layout and motion capture conditions

Source CSS is dark-only, near-black opaque shell/content, colored agent avatars and indicators, glass overlays. Base shell: 40 px header, 224 px sidebar, 316 px inspector, 24 px status bar; terminal height `min(208px,30vh)`. No split-pane drag/resize handlers found. All avatars are generated SVG/CSS with a shared animation loop, pointer/hover reactions and offscreen visibility gating. [L18–47, L77–82, L953–1124]

| Width / condition | Additional source-defined capture |
|---|---|
| Above 1180 px | Full inspector/sidebar; hide either/both; terminal open and closed |
| At/below 1180 px | Inspector becomes right overlay drawer with close button and scrim |
| At/below 1000 px | Part of breadcrumb hidden |
| At/below 860 px | Sidebar becomes 52 px icon rail; search label and execution timestamps/durations hidden |
| At/below 620 px | Sidebar becomes 232 px left drawer; inspector remains drawer; compact top/status bars, 180 px terminal, two-column assignment picker/variant grid; several table/details columns hidden |
| Reduced motion | OS preference on init/change or Settings/status/scene/palette toggle; stops avatar float/orbit and many indicators while retaining states and some timed sequencing. Capture comparison, not assume zero animation |

Breakpoints/reduction: L634–700, L2419–2439, L2541–2543. Overflow, readability, keyboard focus, drawer stacking and screen-reader behavior need browser verification.

## Original research checklist (superseded by final capture index)

### Pass 1 — screens and overlays

1. [ ] Capture all ten sidebar destinations, Chat, both file views, TASK-142, one created task and No open tabs; test each historical task tab for blank content.
2. [ ] Capture seven agent cards, all nine preview states, each tool-specific indicator, Ward's disabled assignment/offline reply and right-click menu; capture hero hover/poke/suggestions.
3. [ ] Capture palette root, four subpages, filtered/no-results states and every root command; test Backspace, arrows, Enter, Escape, focus return and advertised-but-unwired N/@.
4. [ ] Capture every menu family: model in three locations, account from both entries, task More, agent context, chat recipient/context and Scenes; record placeholder/no-op outcomes.
5. [ ] Capture New task (default, edited, empty title, busy/offline selection, cancel/submit), Resolve (Not now/grant), toast actions/dismissal/stack and Settings' three autonomy states.

### Pass 2 — work and continuity

1. [ ] Replay main run; capture planning, handoffs, parallel design/copy, pending approval, editor stream, tests, failed deploy, grant/retry, Needs input and final Completed; expand every evidence type.
2. [ ] Capture all 12 scene outcomes from the table; open terminal explicitly for tool/error scenes; verify secondary Blocked and Several agents working end states.
3. [ ] Submit one task for each routing family; assign another to a busy agent for queued → running → completed; compare task More/Run links and #218 status after creation.
4. [ ] Send to all seven Chat recipients; capture thinking/streaming/tool disclosure, Forge code-copy and offline response; verify context selections do nothing and messages share one transcript.
5. [ ] Compare close/reopen, scene replay and reload: draft, conversation, new tasks, Activity, model/autonomy and agent state; check content hidden by scrolling, dirty tabs and unread terminal badges.

### Pass 3 — shell and visual states

1. [ ] Capture desktop sidebar/inspector/terminal combinations; inspector Agents groups and Context before/after grant; all shell keyboard shortcuts.
2. [ ] Capture boundaries around 1180/1000/860/620 px, both drawers and scrim dismissal; check tables, menus and dialogs for clipping.
3. [ ] Compare full/reduced motion, pointer reactions, completion/blocked/approval expressions and keyboard-only overlay dismissal/focus.

For the Neko redesign, treat these as **reference screens and behavior candidates**. Mock permissions, runtime controls, tools, evidence, approvals and persistence require comparison with Neko's real contracts; this inventory establishes no Neko implementation coverage.

Final browser coverage and outstanding interaction limits are recorded in [the audit scope](README.md) and [caption QA](capture-qa.md); use [the capture index](capture-index.md) for the completed evidence.

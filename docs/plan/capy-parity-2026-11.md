# Neko's backlog for feature parity with Capy (2026-11-20)

The owner: "we want to make this as close as possible to Capy... list all the features they have and we would want to build them." This is that list. It was written from the Capy study (`/tmp/neko-design-refs/capy/capy-inventory.md`, outside the repository) and from what Neko has today. No code was changed.

How to read it:

- Facts about Capy come only from the study and its screenshots; a guess is marked "guess". Capy features are described in our own words. What Neko builds uses Neko's own names and art: cats, not capybaras.
- `screens: 13, 141` means files `/tmp/neko-design-refs/capy/13-*.png` and `141-*.png` (kept outside the repository). `v5` means the owner-approved mockup (`/tmp/neko-mockups/index.html`).
- `Neko today` doc names are files in `docs/knowledge/` unless a path is given. `SA-F1` to `SA-F8` are the eight planned features of `docs/design/super-agent-features-2026-11.md` (SA-F4 guard rails, SA-F5 usage ledger, SA-F6 goal receipts, SA-F7 deadline default are built; SA-F1 record-first, SA-F2 crew rules and plan files, SA-F3 outside-agent endpoint, SA-F8 goal packs are planned).
- Size: S = 0.5, M = 1.5, L = 3, XL = 6 agent-days (one agent working a focused day, tests and docs included). Priority: P0 the product does not make sense without it, P1 core parity, P2 worth building, P3 polish or edge. Items Neko already has are `[x]` with the doc that proves it.
- Rules that apply to every item: new storage is small rows, never the workbench snapshot (`CLAUDE.md`, resource limits); new requests are named in `web/src/` so the Rune allow-list sees them; anything destructive is confirmed; UI work follows the design system, with the approved mockup before any build.

## 1. Summary

Capy features counted: **235** (218 in the epics, 17 not planned).

| Neko status | Count |
| --- | --- |
| has | 42 |
| partial | 91 |
| missing | 85 |
| not planned | 17 |
| **total** | **235** |

By epic:

| Epic | has | partial | missing | total |
| --- | --- | --- | --- | --- |
| A Captains and crews | 1 | 9 | 18 | 28 |
| B Sessions (threads), chat and composer | 11 | 20 | 10 | 41 |
| C Needs-you, digest and Inbox | 5 | 7 | 3 | 15 |
| D Issues | 0 | 1 | 10 | 11 |
| E Pull requests and review | 2 | 8 | 9 | 19 |
| F Automations | 5 | 9 | 6 | 20 |
| G Models, cost and usage | 4 | 8 | 4 | 16 |
| H Machines, devices and handoff | 1 | 5 | 7 | 13 |
| I Knowledge: files, skills, memory, plugins, integrations | 6 | 7 | 6 | 19 |
| J Personalisation, themes, onboarding, settings | 2 | 6 | 4 | 12 |
| K Design system and UI craft | 3 | 8 | 6 | 17 |
| L Platform: API, CLI, mobile/phone reach | 2 | 3 | 2 | 7 |
| not planned | – | – | – | 17 |

By phase (what is left to build; `has` items are excluded):

| Phase | Items | S | M | L | XL | Agent-days | Calendar with 4 agents in parallel (guess) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 The v5 core | 42 | 12 | 24 | 6 | 0 | 60 | about 17 working days |
| 2 Automations and PR depth | 39 | 20 | 13 | 6 | 0 | 47.5 | about 14 working days |
| 3 Issues, models and usage, knowledge | 38 | 12 | 22 | 4 | 0 | 51 | about 15 working days |
| 4 The rest | 57 | 32 | 16 | 6 | 3 | 76 | about 22 working days |
| **All** | 176 | | | | | **234.5** | about 67 working days |

Already built (`[x]`): 42 items. Estimates are rough: agent-days are the sum of the size scale above; the calendar column divides by 3.5 for the parallelism the project already uses and ignores review and owner decisions.

## 2. Epics

### A. Captains and crews

- [ ] **A1 Captain record** — Capy: A long-lived agent with a name, a face, a project, a machine, a model and an access setting. It starts and reads a crew of threads, keeps its own files, wakes on events and schedules, and only works inside its project. (screens: 141, 81, 138) · Neko today: partial (goals-that-keep-working.md, the-chat-directs-the-crew.md) · Build: A `Captain` row: name, face, workspace and repository it looks after, model policy, daily spend limit, purpose sentence. It owns sessions (the crew), goals and automations. Goals already keep working with evidence; the captain is the standing owner above them. · Size L · Priority P0 · Depends on: none · Phase 1
- [ ] **A2 New captain dialog** — Capy: Dialog to create a Captain: name, face, project, machine, model, access. (The dialog itself is not in any first-party screenshot; guess from docs.) (screens: 138, 141) · Neko today: missing · Build: The v5 dialog: Face, Name, Looks after (workspace · repository), Model ("picks per session"), Daily spend limit, What is it for? Cancel / Create captain. The purpose sentence becomes the first line of its RULES file. · Size M · Priority P0 · Depends on: A1, A3, A8 · Phase 1
- [ ] **A3 Captain faces** — Capy: An avatar per Captain: illustrated animal portrait on a flat colour tile in a rounded square, a different prop per face (goggles, hard hat, detective cap, captain cap), about seven presets, a seeded default, uploaded photo or organisation library; name beneath in lowercase; changeable later with the alias. (screens: 141, 138, 13) · Neko today: missing · Build: A fixed set of cat faces (our own drawings, own props: scarf, headphones, bowtie, welding mask) on flat tiles, tile colour chosen from a small palette, default picked from a hash of the name, optional photo upload kept on this Mac. Drawn as SVG at 16, 24, 44 and 80 px. · Size L · Priority P0 · Depends on: none · Phase 1
- [ ] **A4 Face state glyph** — Capy: A 3x3 dot-grid glyph at the face's corner: grey when idle, animated while working; a blue dot at the top-right means something waits on you. (screens: 141, 138, 61) · Neko today: partial (the-shared-pulse-clock-and-the-repeating-animation-rule.md) · Build: Corner glyph from the shared pulse clock (working = stepped dots, waiting = slower, failed = coral) and an accent dot for needs-you, on the face in the sidebar, overview and tabs. Stops under Reduce Motion. · Size S · Priority P0 · Depends on: A3, K6 · Phase 1
- [ ] **A5 Promote a session to a captain and demote back** — Capy: Turn an existing thread into a Captain (keeps history, PRs, machine) and back; promoting a folder adds its threads to the crew. (screens: none) · Neko today: missing · Build: Session menu: Make a captain (keeps transcript as its first notes) and Back to a session. Same record, flag flips. · Size M · Priority P2 · Depends on: A1, A10 · Phase 4
- [ ] **A6 Ask-to-create a captain** — Capy: Tell any thread "Make a Captain called Mobile for the iOS and Android work" and it creates one. (screens: none) · Neko today: missing (the-chat-directs-the-crew.md) · Build: A `create_captain` tool on the chat bridge (one click to confirm, like other chat actions); the delegation cap stays. · Size S · Priority P2 · Depends on: A1 · Phase 2
- [ ] **A7 Captain overview** — Capy: Right pane: Needs you (first five), Threads (the crew with model, code, age, PR count), and a files strip; opens on the Overview tab. (screens: 13, 81) · Neko today: missing · Build: The v5 overview: Needs you, Working, Ready to merge, Automations, Files, Settled, in that order, each a calm grouped list with counts and a one-line row; empty groups take no space. · Size L · Priority P0 · Depends on: A1, C1, C2, C8, F17 · Phase 1
- [ ] **A8 Captain files (SA-F2)** — Capy: The Captain keeps plain files in its own Drive: AGENTS.md (its own rules), CREW.md (rules every crew thread reads each turn), NOTES.md (plan), DIGEST.md, skills/, reports/. Edited by telling the Captain; it rereads them on every wake. (screens: 81) · Neko today: partial (docs/design/super-agent-features-2026-11.md (SA-F2 planned)) · Build: Per captain: RULES.md (the captain's own), CREW.md (every session reads each turn, injected under "Your crew's rules"), PLAN.md, DIGEST.md, 16 KB each, `.prev` copy kept on every write. This is SA-F2, widened from per workspace to per captain. · Size L · Priority P0 · Depends on: A1 · Phase 1
- [ ] **A9 Rules as sentences** — Capy: Say "Ask before starting more than two threads at once" or "Nothing merges without my word" and the Captain saves it into its files. (screens: none) · Neko today: missing (docs/design/super-agent-features-2026-11.md (SA-F2: no tool writes the plan)) · Build: The captain proposes a rule edit as a small diff (as skill patches do); applied on one click, never silently. · Size M · Priority P1 · Depends on: A8 · Phase 2
- [ ] **A10 Crew membership** — Capy: A thread belongs to at most one crew in the Captain's project; add by drag, Move to, Remove from crew, or by asking; crew threads open beside the Captain's chat. (screens: 81) · Neko today: missing · Build: `captain_id` on a session; set at creation by the captain, or with Move to in the session menu and the palette; sessions with no captain stay in Sessions. · Size M · Priority P0 · Depends on: A1 · Phase 1
- [ ] **A11 Drag a session onto a captain** — Capy: Dragging a thread row onto a Captain row adds it to the crew. (screens: 81) · Neko today: missing (rune-parity-b.md (board drag exists)) · Build: Row drag onto a face in the sidebar, same refusal rules as the board's drag. · Size S · Priority P3 · Depends on: A10 · Phase 4
- [ ] **A12 Open-questions menu and count** — Capy: The Captain's chat gathers crew questions with Dismiss all; a blue number on the sidebar row counts open questions across the crew. (screens: 138, 141) · Neko today: missing · Build: Accent count beside the captain's name; the chat's question menu lists them with Use the default / Later / Dismiss, and Dismiss all. · Size S · Priority P0 · Depends on: C1, C5 · Phase 1
- [ ] **A13 Captains-first sidebar** — Capy: Opens on Captains: pinned Captains as a row of faces with names; list rows show a small face, name, age and, in blue, the question waiting on you or the newest message; Threads below (hybrid layout) or one at a time (split). (screens: 138, 141, 81) · Neko today: partial (rune-polish.md (sessions column exists)) · Build: Sidebar: Home, Search, Calendar, Captains (+), Sessions, Automations; each captain row = face, name, age, the question in accent colour. Pinned row of large faces is a toggle. · Size M · Priority P0 · Depends on: A3, A4, A12 · Phase 1
- [ ] **A14 Hybrid vs split sidebar layouts** — Capy: Hybrid (default) shows Captains then Threads in one list; Split shows one at a time; sidebar options menu changes it. (screens: 63, 141) · Neko today: missing · Build: Setting: Both lists (default) or one at a time. · Size S · Priority P3 · Depends on: A13 · Phase 4
- [ ] **A15 Captains message each other** — Capy: A Captain can message any Captain it can see to hand over work or ask; the reply returns to its thread; a rule can stop it. (screens: 70) · Neko today: missing (the-chat-directs-the-crew.md) · Build: `message_captain` tool with the delegation cap and a rule ("never message other captains") honoured; shows as a line in both chats. · Size M · Priority P3 · Depends on: A1, A8 · Phase 4
- [ ] **A16 Captain archive, delete, demote** — Capy: Archive puts the Captain and crew to sleep; Delete returns crew threads to the list; Demote returns it to a thread. (screens: none) · Neko today: missing · Build: Archive (sleeps, nothing wakes), Delete (sessions return to Sessions, files kept 7 days with Undo). · Size S · Priority P2 · Depends on: A1 · Phase 3
- [ ] **A17 Captain wakes on crew events** — Capy: The Captain hears from each crew thread when it finishes a turn or needs something, updates its plan quietly, rewrites the digest, and tags you only when a decision or blocker is yours; wakes on timers (7 days) and watches (30 days). (screens: 81) · Neko today: partial (goals-that-keep-working.md (goal tick), the-chat-directs-the-crew.md (wait_for, schedule)) · Build: Crew-turn-end and needs-you events enqueue one captain turn (debounced, one at a time, cost-capped); the turn reads RULES/CREW/PLAN, updates PLAN and DIGEST, and speaks in chat only if the person must act. · Size L · Priority P0 · Depends on: A1, A8, C2 · Phase 1
- [ ] **A18 Captain splits a request into sessions** — Capy: It does not do the work itself in the triage pattern: it starts one crew thread per piece, each with its own branch and PR, stacks PRs touching the same files, and may run a thread on a named device. (screens: 81) · Neko today: partial (the-chat-directs-the-crew.md (create_task, delegation cap), pull-requests-after-they-open.md) · Build: The captain turn uses `create_task` per piece, picks a model per piece (strong for a refactor, small for checks, v5 "3 sessions working · Claude for the rework, Haiku for the checks"), honours the writable-per-repository cap. · Size M · Priority P0 · Depends on: A1, A17, A19 · Phase 1
- [ ] **A19 Per-captain model** — Capy: Chosen at creation; crew threads use it unless you or the Captain pick another. (screens: none) · Neko today: partial (rune-parity-a.md (model chip per session)) · Build: Captain default model policy ("picks per session" or a fixed one); the session chip shows the pick and who made it. · Size M · Priority P0 · Depends on: A1 · Phase 1
- [ ] **A20 Captain spend limit** — Capy: (Neko addition from v5; Capy shows cost, not a per-Captain limit.) (screens: 81) · Neko today: partial (neko-maintains-itself.md (daily ceiling), goals-that-keep-working.md (goal budget)) · Build: Per-captain daily limit; at the limit the captain pauses its crew and asks (Raise by $2 / Hold until tomorrow). Rolls into the global daily limit. · Size M · Priority P0 · Depends on: A1, G1 · Phase 1
- [x] **A21 Hard rules prose cannot override** — Capy: Nothing merges unless you ask for that merge in the thread; an instruction cannot change it. (screens: none) · Neko today: has (pull-requests-after-they-open.md (merge is by click), docs/design/super-agent-features-2026-11.md (R1)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **A22 Starter captain templates** — Capy: Documented setups: Feature Captain, Triage Captain, Janitor Captain, plus non-code uses (research, daily scorecard, changelog posting, support queue). (screens: none) · Neko today: missing (packs.md) · Build: Three templates in the New captain dialog and as packs (purpose sentence, rules, one automation each). · Size S · Priority P2 · Depends on: A2, F16 · Phase 2
- [ ] **A23 Finished-sessions strip in the captain chat** — Capy: A collapsed row of small faces "3 threads finished" inside the Captain's chat that expands to the list. (screens: 81) · Neko today: missing · Build: Collapsed line "3 sessions finished" with small chips; expands. · Size S · Priority P2 · Depends on: A17 · Phase 2
- [ ] **A24 Crew chip above the composer** — Capy: A small count chip ("7") above the Captain's composer lists the running crew. (screens: 81) · Neko today: missing · Build: Chip with count of working sessions; click opens the list; Stop one from there. · Size S · Priority P2 · Depends on: A10 · Phase 2
- [ ] **A25 Alias for chat and Slack mentions** — Capy: A short name set in the Captain's Profile, usable as `captain=<alias>` in Slack. (screens: none) · Neko today: missing · Build: Alias field; `@miso` in any chat or front door routes to it. · Size S · Priority P3 · Depends on: A1 · Phase 4
- [ ] **A26 Captain machines (crew runs on)** — Capy: Crew runs on cloud or one paired device; name a machine for one thread; the Captain itself can run on a device. (screens: none) · Neko today: missing · Build: Deferred with devices: see H2 and H5. · Size S · Priority P3 · Depends on: H2 · Phase 4
- [ ] **A27 Captain chat** — Capy: The centre pane for a Captain: tab with face, name and running cost, a narrated conversation ("Ten threads are on it..."), a collapsed finished strip, the composer with a model chip. (screens: 81) · Neko today: missing · Build: The v5 captain screen: header (face, name, role, workspace and repository, cost today), narrated replies, an inline Needs-you card with an Answer box, composer "Answer miso in one line...". · Size M · Priority P0 · Depends on: A1, C4 · Phase 1
- [ ] **A28 Home composer routes work to a captain** — Capy: The new-thread composer has project, machine and repo pickers, model chip, attach, mic, send. (screens: 53) · Neko today: partial (rune-parity-a.md (Home composer, workspace switcher)) · Build: Home composer "What should your captains work on?" with a captain picker and Let Neko choose; sends to the chosen captain's chat. · Size M · Priority P0 · Depends on: A1, A18 · Phase 1

### B. Sessions (threads), chat and composer

- [ ] **B1 Session codes** — Capy: Each thread has a code like HOME-14 (project code + number), typed into the command menu to open it. (screens: none) · Neko today: partial · Build: Short code per workspace (WEB-14), shown beside the title and findable in the palette. · Size S · Priority P2 · Depends on: none · Phase 2
- [ ] **B2 Session states: Working, Waiting, Failed, Idle** — Capy: Status is set by the agent, not you: Working, Waiting (names the event it waits on), Failed, Idle (time since activity); "Needs you" is a separate flag. (screens: 61, 62) · Neko today: partial (rune-parity-b.md (board states), docs/research/design-references-2026-11.md) · Build: Four visible states plus the needs-you overlay on every session row; Waiting says what it waits on (checks, a timer, a goal). · Size M · Priority P0 · Depends on: K6 · Phase 1
- [x] **B3 Composer: model chip, attach, mic, send** — Capy: One box with model + reasoning chip, `+` attach, tune icon, mic, send with a dropdown. (screens: 53) · Neko today: has (rune-parity-a.md (composers, model chip, chips upload)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **B4 Steer, Interrupt, Queue** — Capy: Enter steers at the next step, Cmd+Enter interrupts, Option+Enter queues; keys are remappable. (screens: 59, 60) · Neko today: has (the-chat-directs-the-crew.md, rune-parity-a.md (queue tray, interrupt)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **B5 Queued message tray** — Capy: A queued message waits above the composer with send now, discard, edit. (screens: 60) · Neko today: has (rune-parity-a.md (queue tray: Edit, Send now, Cancel)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **B6 Edit a sent message and keep versions** — Capy: Editing continues from the changed message; a version switcher keeps the original. (screens: 60, 104) · Neko today: missing · Build: Edit on your own message forks the transcript from there; arrows switch versions; the original stays. · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **B7 Voice dictation** — Capy: Mic dictates at the cursor in every input and dialog; hold Alt+M to talk; models can read voice messages (transcribed). (screens: none) · Neko today: missing · Build: Hold a key to dictate with macOS on-device speech into any Neko text field; nothing leaves the Mac. · Size M · Priority P2 · Depends on: none · Phase 4
- [ ] **B8 Reply to a message** — Capy: Reply to any message, yours or a teammate's. (screens: none) · Neko today: missing · Build: Quote a message into the composer; the agent sees the quote. · Size S · Priority P3 · Depends on: none · Phase 4
- [ ] **B9 Slash commands** — Capy: `/plan`, `/side`, `/btw`, `/fork`, `/fork-summary`, `/private`, `/shared`, `/subscribe`, `/wait`, `/continue`. (screens: none) · Neko today: partial (rune-parity-a.md, rune-parity-b.md (`/` list), goals-that-keep-working.md (`/goal`)) · Build: Add `/plan`, `/side`, `/fork`, `/wait`, `/subscribe`, `/captain` to the existing list. · Size S · Priority P2 · Depends on: B10, B13 · Phase 3
- [ ] **B10 /plan** — Capy: Capy explores and asks first and changes nothing until you approve its plan; works from Slack, Linear and the API. (screens: none) · Neko today: partial (neko-is-the-chief-of-staff.md (plan stage and approval per ticket)) · Build: `/plan` on a running session or a new one: read-only run that ends in a plan card with Approve; in v5 terms a captain can require this by rule. · Size S · Priority P2 · Depends on: none · Phase 2
- [ ] **B11 Skill picker** — Capy: Typing `$`, `/` or `@` lists skills, files and context. (screens: 56) · Neko today: partial (rune-parity-a.md (`/` and `@` popover)) · Build: Add skills to the popover with a source badge (built-in, workspace, pack). · Size S · Priority P2 · Depends on: I5 · Phase 3
- [ ] **B12 Inline visuals in chat** — Capy: Charts, diagrams, interactive models with tabs and sliders, Mermaid; follow dark mode and Reduce Motion. (screens: 139) · Neko today: partial (everything-renders.md) · Build: Interactive cards (tabs, sliders) in replies through a sandboxed view; charts follow theme and Reduce Motion. · Size M · Priority P3 · Depends on: none · Phase 4
- [ ] **B13 Side chat** — Capy: `/side`: a full copy of the conversation on the same machine that does not report back. (screens: none) · Neko today: missing · Build: A throwaway branch of the conversation that cannot change files or message the crew. · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **B14 Fork and fork-summary** — Capy: `/fork`: a new thread on a fresh machine from a full copy or a summary. (screens: none) · Neko today: missing (checkpoints-and-undo.md) · Build: New session from the full transcript or a summary, in its own worktree at the checkpoint. · Size M · Priority P3 · Depends on: none · Phase 4
- [x] **B15 Session tabs in the right pane** — Capy: Overview, Diffs, Files, Setup, Browser, shell, Desktop, pull requests, subagents and side chats as tabs; split panes by dragging a tab to an edge. (screens: 65, 135) · Neko today: has (rune-p3a-the-context-panel.md, rune-live-browser.md (tabs, diff, files, terminal, browser)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **B16 Split panes** — Capy: Drag a tab to an edge to split the chat, files, issues and PRs into a pane grid. (screens: 135) · Neko today: missing · Build: Two-pane split first (chat plus one panel), saved layout. · Size L · Priority P3 · Depends on: none · Phase 4
- [x] **B17 File viewer for many types** — Capy: Markdown, Mermaid, CSV/XLSX, HTML sandbox, images with pan/zoom, PDF, video/audio, 3D/CAD, Word, source; compare versions. (screens: 124, 129) · Neko today: has (everything-renders.md, rune-p3a-the-context-panel.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **B18 Annotate images, video and 3D** — Capy: Pin notes to an image, a video frame or a 3D model and send them all as one message. (screens: 134) · Neko today: partial (the-canvas-beside-the-chat.md (comment mode on artifacts)) · Build: Pins on images and video frames first; Send N notes as one message. · Size M · Priority P3 · Depends on: none · Phase 4
- [ ] **B19 In-app browser with element picker** — Capy: A browser tab; in the desktop app a Design mode picks an element on the page. (screens: none) · Neko today: partial (rune-live-browser.md (watch-only)) · Build: Let the person drive the tab and pick an element to quote into the composer. · Size L · Priority P3 · Depends on: none · Phase 4
- [x] **B20 Terminal tab** — Capy: A shell tab on the thread's machine with a find bar and an exit-status line. (screens: none) · Neko today: has (rune-p3a-the-context-panel.md (terminal tab)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **B21 Message ticks** — Capy: Ticks at the chat's left edge mark each human message; hovering previews it. (screens: none) · Neko today: missing · Build: Ticks with hover preview; J/K jumps between them. · Size S · Priority P3 · Depends on: K4 · Phase 4
- [ ] **B22 Auto titles and title rules** — Capy: Auto-title up to 50 characters; org and personal title instructions; regenerate rules. (screens: none) · Neko today: partial (CLAUDE.md (naming sweep), rune-parity-a.md (rename)) · Build: A title-style sentence in Settings and a Regenerate title action. · Size S · Priority P3 · Depends on: none · Phase 4
- [ ] **B23 Context meter and compaction** — Capy: Per-thread compaction threshold (default 400k tokens or 80% of a small model's window); a breakdown of what loads before the first message. (screens: 120) · Neko today: partial (tickets-learning-and-checkpoints.md (chat checkpoint)) · Build: Context meter in the session header, Compact now, and a threshold; the breakdown links to I4. · Size M · Priority P2 · Depends on: I4 · Phase 3
- [ ] **B24 Archive with undo and bulk select** — Capy: Archive means done and wakes nothing; an Undo toast; select many rows for bulk actions. (screens: 64) · Neko today: partial (rune-parity-a.md (toasts, Closed recently)) · Build: Archive/Unarchive with the toast; multi-select with a bar of actions. · Size S · Priority P2 · Depends on: C8 · Phase 3
- [ ] **B25 Command menu search depth** — Capy: Cmd+K finds threads, codes, subagents, message text, PRs, automations and settings, with category pills and filters (`author:`, `repo:`); Tab turns the text into a new thread. (screens: 103) · Neko today: partial (rune-parity-a.md (palette: Sessions, Go to, Actions, Appearance, Plugins)) · Build: Add message search, PRs, automations, captains; filters; Tab = New session with the typed text. · Size M · Priority P1 · Depends on: B1 · Phase 2
- [ ] **B26 Sidebar options** — Capy: Sort, Group by (None, Projects, Folders, Author), Detailed rows, Everyone, Automation; the pages row is draggable; Option+1..9 jumps to a thread. (screens: 63, 140, 142) · Neko today: partial (rune-polish.md) · Build: Sort and Group by (Captain, Workspace, None), Detailed rows, ⌥1..9 for pinned sessions. · Size M · Priority P2 · Depends on: K9 · Phase 3
- [ ] **B27 Saved views** — Capy: Named filter views on the Threads page. (screens: none) · Neko today: missing (rune-parity-b.md (All sessions)) · Build: Save the current All-sessions filter by name. · Size S · Priority P3 · Depends on: none · Phase 4
- [ ] **B28 Pinned sessions, folders, tags** — Capy: Pinned is a private folder shown first; folders do not nest and a thread can be in several; org-wide coloured tags, up to 20 per thread. (screens: 61, 140) · Neko today: missing · Build: Pinned group and personal folders now; tags later. · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **B29 Unread and mention badges** — Capy: Bold title for unread, a number badge for mentions, a Dock badge, Mark all as read. (screens: none) · Neko today: partial (rune-parity-a.md (Dock badge, unseen dot)) · Build: Bold-unread on rows and a Mark all as read action. · Size S · Priority P2 · Depends on: none · Phase 2
- [ ] **B30 Waiting-on line and running watches** — Capy: A waiting thread's last line names the event it waits on as a link; running watches sit above the composer, each with Stop watching. (screens: none) · Neko today: partial (the-chat-directs-the-crew.md (wait_for)) · Build: Line above the composer: "Waiting for checks on #214" with Stop waiting; same for timers. · Size M · Priority P2 · Depends on: B2 · Phase 2
- [x] **B31 Sessions keep working with the window closed** — Capy: Threads keep working in the cloud after you close the tab. (screens: none) · Neko today: has (the-daemon-client-split-and-why-the-hotkey-is-registered-whe.md (the daemon is resident)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **B32 Wake events** — Capy: CI result, human feedback, conflict (within 3 days), subagent finished or blocked, timers up to 7 days, watches up to 30 days. (screens: none) · Neko today: has (pull-requests-after-they-open.md, the-chat-directs-the-crew.md (wait_for, schedule)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **B33 Subagents** — Capy: A thread's agent spawns full child agents with their own conversation, up to three levels, addressed by position (Subagent 1.3); formerly Tasks. (screens: 58) · Neko today: partial (neko-is-the-chief-of-staff.md (splits, tree), CLAUDE.md (delegation cap)) · Build: Child sessions shown under their parent with position numbers; depth cap 3. · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **B34 Editable subagent draft** — Capy: A subagent starts as an editable draft before it runs. (screens: 58) · Neko today: missing · Build: Child session proposal card: edit the brief, Start or Drop. · Size S · Priority P3 · Depends on: B33 · Phase 4
- [ ] **B35 Turn summaries** — Capy: "Worked for 6m 24s" with what was edited and run, collapsed. (screens: 25, 61) · Neko today: partial (docs/research/design-references-2026-11.md (T3 vendored chat)) · Build: One folded line per turn in the session view with files and test result. · Size S · Priority P0 · Depends on: none · Phase 1
- [ ] **B36 Step list in a session** — Capy: The agent's plan as ticked steps (step 4 of 7). (screens: none) · Neko today: partial (goals-that-keep-working.md (plan lines)) · Build: Step list at the top of the session with the current step; v5 shows "step 4 of 7". · Size M · Priority P0 · Depends on: none · Phase 1
- [ ] **B37 Session header** — Capy: Title, status, cost, overflow, Stop; the captain it came from. (screens: 81, 25) · Neko today: partial (rune-parity-a.md (header and badges)) · Build: v5 header: crumb (captain / title), step, running cost, Stop, ... menu, "From miso" brief line. · Size M · Priority P0 · Depends on: G1, A10 · Phase 1
- [ ] **B38 Replay stream** — Capy: A button in the thread header that replays the run (seen only in a screenshot). (screens: 81) · Neko today: partial (goals-that-keep-working.md (run journal)) · Build: Replay the last turn from the journal as a timeline. · Size M · Priority P3 · Depends on: none · Phase 4
- [x] **B39 Attachments and file drops** — Capy: Drop files and images into the composer; chips upload. (screens: 53) · Neko today: has (rune-parity-a.md (drop overlay, chips upload)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **B40 Slash and @ popover** — Capy: `/` and `@` open commands and files. (screens: 56) · Neko today: has (rune-parity-a.md (popover, /api/files)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **B41 Workspace and repository pickers in the composer** — Capy: Project picker and repo chip ("2 repos") above the composer. (screens: 53) · Neko today: has (rune-parity-a.md (workspace switcher, scope)) · Build: none · Size – · Priority – · Depends on: – · Phase done

### C. Needs-you, digest and Inbox

- [ ] **C1 Needs-you as a flag** — Capy: Separate from status: someone waits on you for an answer, decision, env vars, MCP approval, desktop hand or credits. A label on the row and a line under the chat ("Waiting on you to ..."). (screens: 62, 13) · Neko today: partial (rune-parity-a.md (Inbox), neko-runs-the-day.md) · Build: Overlay on every session and captain row from the Inbox items; the line under the chat; one source of truth. · Size M · Priority P0 · Depends on: none · Phase 1
- [ ] **C2 One-line digest** — Capy: The Captain rewrites DIGEST when crew state changes: Needs you, what's new, in progress. Read in a tab; the top five show on the overview. (screens: 81, 13) · Neko today: partial (morning-digest.md (daily, from Neko's data), goals-that-keep-working.md (journal)) · Build: Per captain, rewritten on each captain turn: one line per item, three parts. Shows on Home and the overview; also a file (DIGEST.md). · Size L · Priority P0 · Depends on: A1, A8 · Phase 1
- [ ] **C3 Digest row anatomy** — Capy: One sentence with inline PR or issue chips in the accent colour, a right-aligned age (4 h, idle 27 h), grey group bars with counts, "1 more". (screens: 13) · Neko today: partial (rune-parity-a.md (Inbox rows)) · Build: Row = sentence + chips + age; groups collapse; "N more". · Size M · Priority P0 · Depends on: C1, K10 · Phase 1
- [ ] **C4 Answer in one line** — Capy: Reply in the Captain's chat ("merge #3780", "hold the migration until Monday"); it acts and drops the item. (screens: 81) · Neko today: partial (phone-push.md (a `>` line answers), goals-that-keep-working.md) · Build: Answer box under each question; the captain parses the line, acts, closes the item. · Size M · Priority P0 · Depends on: C1, A17 · Phase 1
- [ ] **C5 Use the default, Answer, Later, Dismiss** — Capy: Each request offers Dismiss and Later; requests stay open until answered, with a line that reopens them. (screens: 62) · Neko today: partial (rune-parity-a.md (Use the default, Mark done, Reopen)) · Build: Four actions on every item: Use the default, Answer..., Later (comes back in 4 h), Dismiss (with Undo). · Size S · Priority P0 · Depends on: C1 · Phase 1
- [ ] **C6 Typed request cards** — Capy: Requests come as cards for: question, decision, environment variables, MCP approval, desktop hand, credits. (screens: 62) · Neko today: partial (the-agent-control-plane-mcp-and-the-permission-inbox.md) · Build: Card kinds with the right control each (secret field for env vars, Approve for tools, Open for a hand-off). · Size M · Priority P2 · Depends on: C1, H8 · Phase 3
- [x] **C7 Closed recently and Reopen** — Capy: Dismissed requests stay reachable for a while. (screens: none) · Neko today: has (rune-parity-a.md (Closed recently, Reopen)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **C8 Settled** — Capy: A read, idle thread leaves the active list until new activity ("Settled today · 9"). (screens: 64, 81) · Neko today: missing (docs/research/design-references-2026-11.md (T8)) · Build: Rows that are read, idle and need nothing move to Settled automatically; any new activity brings them back. · Size M · Priority P0 · Depends on: B2 · Phase 1
- [ ] **C9 Digest posted in chat, tagging only when it is yours** — Capy: When a report changes the digest it pastes Needs you into chat and tags you only if the decision or blocker is yours. (screens: 81) · Neko today: missing · Build: Captain posts the Needs-you part; notification only when an item is newly yours. · Size S · Priority P0 · Depends on: C2 · Phase 1
- [ ] **C10 Shape the digest by telling the captain** — Capy: "Group the digest by project" saves a rule that changes its layout. (screens: none) · Neko today: missing · Build: A digest rule line in RULES.md the captain edits (A9). · Size S · Priority P2 · Depends on: C2, A9 · Phase 2
- [x] **C11 Needs-you push to the phone with reply** — Capy: Needs-you items pushed away from the Mac. (screens: none) · Neko today: has (phone-push.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **C12 Morning digest** — Capy: One daily summary. (screens: none) · Neko today: has (morning-digest.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **C13 Deadline default** — Capy: (v5 shows: "If you don't answer by 9am I'll use the jobs queue".) (screens: none) · Neko today: has (deadline-default.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **C14 Inbox for routines** — Capy: Routine output and questions collect in one place. (screens: none) · Neko today: has (neko-runs-the-day.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **C15 Home** — Capy: (Neko screen.) Capy opens on the Captains sidebar plus Needs you. (screens: 141, 13) · Neko today: partial (rune-polish.md (Home and its art)) · Build: v5 Home: greeting with counts (3 working, 2 need you, $6.20 today), captain tiles with their last line, Needs you, Settled today. · Size M · Priority P0 · Depends on: C3, A13 · Phase 1

### D. Issues

- [ ] **D1 Issues page** — Capy: Tabs For you / Assigned to me / Mentioned / All, filter chips (assignee, creator, status, labels, due, relations), grouped by status with code, title, blocked-by chip, sub-issue progress, PR chips, labels, due date, priority bars, assignee faces, age. (screens: 132) · Neko today: missing (rune-parity-b.md (All sessions is a session view, not a tracker)) · Build: A local Issues page in the same row anatomy; assignees are you or a captain's face. · Size L · Priority P1 · Depends on: D2, A3 · Phase 3
- [ ] **D2 Issue record** — Capy: State, priority (urgent, high, normal, low), assignees (primary first), due date, labels, parent, relations, files, comments, timeline, linked PRs and threads. (screens: 132) · Neko today: missing · Build: SQLite rows in their own table, never the snapshot; every change in the timeline. · Size L · Priority P1 · Depends on: none · Phase 3
- [ ] **D3 Custom states** — Capy: Orderable states with a default: Backlog, Todo, In progress, In review, Done, Deployed, Canceled, Won't fix. (screens: 132) · Neko today: missing · Build: Editable state list with five defaults. · Size S · Priority P2 · Depends on: D2 · Phase 3
- [ ] **D4 Issue code prefix** — Capy: A per-organisation prefix (CAP-12 style), settable. (screens: 132) · Neko today: missing · Build: Prefix per workspace. · Size S · Priority P3 · Depends on: D2 · Phase 3
- [ ] **D5 Sub-issues and relations** — Capy: Parent and sub-issues with progress (7/15), relations blocks, blocked by, related, with a blocked-by chip on the row. (screens: 132) · Neko today: missing · Build: Sub-issue progress chip and blocks/blocked-by. · Size M · Priority P1 · Depends on: D2 · Phase 3
- [ ] **D6 Agents file, update and close issues** — Capy: Threads file, update and close issues as they work; the owning thread wakes on a comment; Captains track work as issues. (screens: none) · Neko today: missing (neko-is-the-chief-of-staff.md (FOLLOW_UP proposals)) · Build: Chat tools `file_issue`, `update_issue`, `comment_issue`; a comment wakes the owning session. · Size M · Priority P1 · Depends on: D2, A17 · Phase 3
- [ ] **D7 Linked PRs and sessions on an issue** — Capy: PR chips and the linked thread appear on the issue row and page. (screens: 132) · Neko today: missing · Build: Chips on rows; the issue page lists sessions and PRs. · Size M · Priority P1 · Depends on: D2, E16 · Phase 3
- [ ] **D8 Display menu and quick status change** — Capy: Group, order and hide row properties; click the status icon to change. (screens: none) · Neko today: missing · Build: Display menu; status glyph is a button. · Size S · Priority P3 · Depends on: D1 · Phase 4
- [ ] **D9 Mentions in comments** — Capy: Tag skills, PRs and files in comments; annotate images and video; issue links unfurl in Slack. (screens: none) · Neko today: missing · Build: `@` in a comment picks session, PR, file. · Size S · Priority P3 · Depends on: D2 · Phase 4
- [ ] **D10 Linear two-way sync** — Capy: Delegating a Linear issue starts a thread; status is written back; team-to-project routing fails closed. (screens: 39, 67) · Neko today: partial (neko-runs-the-day.md (Linear watches file tickets)) · Build: Import issues from Linear, write status back; see I16 for delegation. · Size L · Priority P2 · Depends on: D2 · Phase 4
- [ ] **D11 Saved filters** — Capy: "Match any filter" chips and Clear. (screens: 132) · Neko today: missing · Build: Filter chips on Issues, reusing B27 saved views. · Size S · Priority P3 · Depends on: D1, B27 · Phase 4

### E. Pull requests and review

- [ ] **E1 Open a PR from a session** — Capy: Agent commits, pushes and opens a PR to the base branch; never pushes to main or release; configurable branch prefix; Ready or Draft default; a link back from the description. (screens: 52) · Neko today: partial (pull-requests-after-they-open.md, CLAUDE.md (branch `codex/neko-<folder>`)) · Build: Settings: branch prefix, Draft or Ready default, "Open in Neko" link in the body. · Size S · Priority P1 · Depends on: none · Phase 2
- [ ] **E2 Commit identity** — Capy: You and the agent as co-author (default), You, agent plus you, agent alone; signed commits with a generated key. (screens: 10) · Neko today: missing · Build: Setting for author and co-author trailer. · Size S · Priority P3 · Depends on: none · Phase 4
- [ ] **E3 Merge menu** — Capy: Squash, merge commit or rebase, optional comment, update branch, enable auto-merge, force merge; shows why it is blocked. (screens: 52, 110) · Neko today: partial (pull-requests-after-they-open.md (squash only, update-branch, waiting reason)) · Build: A merge menu with method and comment; blocked reason always shown. · Size M · Priority P1 · Depends on: E16 · Phase 2
- [ ] **E4 Wake settings per event** — Capy: CI results, human feedback, conflict response each React / Keep as context / Ignore; bot list; conflict max age 3 days. (screens: 10) · Neko today: partial (pull-requests-after-they-open.md (MAX_FOLLOW_UPS, bot rule)) · Build: Three-way setting per event per workspace. · Size S · Priority P2 · Depends on: none · Phase 2
- [ ] **E5 Stacked PRs** — Capy: Native GitHub stacks or base-branch stacks; chip "2/3"; detects other tools' stacks; merge from the bottom up. (screens: 110) · Neko today: missing · Build: Stack chip, stack-aware merge order, a captain rule to stack PRs touching the same files. · Size L · Priority P1 · Depends on: E16 · Phase 2
- [ ] **E6 Fork PRs** — Capy: Check out and review fork PRs; push to a fork if edits are allowed. (screens: none) · Neko today: missing · Build: Fork checkout through `gh pr checkout`. · Size M · Priority P3 · Depends on: E16 · Phase 4
- [ ] **E7 Review agent with findings** — Capy: A dedicated agent reads the diff in a real checkout at the head; findings carry category (Bug, Risk, Maintainability, Refactor), severity, confidence (Confirmed, Investigate), file and line. (screens: 54, 107) · Neko today: partial (pull-requests-after-they-open.md (review stage, Re-check all)) · Build: A read-only review run producing structured findings; stored with the PR. · Size L · Priority P1 · Depends on: E16 · Phase 2
- [ ] **E8 Triage of findings** — Capy: Open, Resolved, Irrelevant; the owning thread fixes high findings before reporting ready; re-reviews are incremental; irrelevant ones never return. (screens: none) · Neko today: missing · Build: Status per finding, auto-fix for High, incremental re-review. · Size M · Priority P1 · Depends on: E7 · Phase 2
- [ ] **E9 When a review starts** — Capy: Per repository Off, Once, or Every push; a manual button; `@capy review <focus>` comment; effort Medium first, Low for follow-ups. (screens: 54) · Neko today: partial (pull-requests-after-they-open.md (ReviewPullRequests)) · Build: Per-workspace mode, a Review button, a PR comment `@neko review <focus>` read by polling. · Size M · Priority P1 · Depends on: E7 · Phase 2
- [ ] **E10 Review rules and filters** — Capy: Rules scoped by repo and path; filters skip PRs by label, author, title, base branch; ignored paths; AGENTS.md rules become findings. (screens: 97, 109) · Neko today: partial (neko-is-the-chief-of-staff.md (Rulebook)) · Build: Rule list with scope, skip filters, ignored paths. · Size M · Priority P2 · Depends on: E7 · Phase 2
- [ ] **E11 Review in the app** — Capy: A Review button with a summary and Comment, Request changes or Approve posted to GitHub as you; per-file Reviewed checkboxes (3/7); reviewers list; compare range. (screens: 107, 108) · Neko today: missing (rune-parity-b.md (PR list only)) · Build: PR page review panel with the same three actions and per-file checkboxes. · Size L · Priority P1 · Depends on: E16 · Phase 2
- [ ] **E12 Guided PR walkthrough** — Capy: A guide per PR in chapters: overview prose, a before/after card, a diff per chapter; generated before you look. (screens: 115, 83) · Neko today: missing · Build: A guide generated on PR open, chapter per change, one-line gloss on changed lines. · Size L · Priority P1 · Depends on: E16 · Phase 2
- [ ] **E13 Copy finding as a prompt** — Capy: Copies a finding as a prompt for another coding agent. (screens: none) · Neko today: missing · Build: "Send to a session" and "Copy as prompt". · Size S · Priority P3 · Depends on: E7 · Phase 4
- [ ] **E14 Deploy preview chips** — Capy: Preview deployment links and status appear on the PR page. (screens: 44) · Neko today: missing · Build: Read checks and comments for preview URLs; chip. · Size S · Priority P2 · Depends on: E16 · Phase 2
- [x] **E15 Autofix of CI, review comments, conflicts; merge all** — Capy: The owning thread fixes failing CI, review comments and merge conflicts; "Merge all" in a captain demo. (screens: none) · Neko today: has (pull-requests-after-they-open.md (hand-back), rune-parity-b.md (Merge all ready)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **E16 PR page** — Capy: One page per PR: diff, guide, findings, merge menu, stack chip, checks, preview. (screens: 52, 107) · Neko today: partial (rune-parity-b.md (PR list, Close with reason, Merge when ready)) · Build: A PR page in the right pane; the list stays. · Size M · Priority P1 · Depends on: none · Phase 2
- [ ] **E17 PRs filtered by captain** — Capy: A filter lists every PR a captain's crew opened. (screens: none) · Neko today: missing · Build: Captain filter on the PR list and the Ready to merge group in the overview. · Size S · Priority P2 · Depends on: A10, E16 · Phase 2
- [x] **E18 Close with reason and PR sync** — Capy: Close with a reason; merged or closed on GitHub shows as such. (screens: none) · Neko today: has (rune-parity-b.md, pull-requests-after-they-open.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **E19 Ready to merge group** — Capy: PRs that wait on you, grouped (v5 "Ready to merge · 1"). (screens: 81) · Neko today: partial (rune-parity-b.md (VERDICTS groups)) · Build: Group inside the overview. · Size S · Priority P0 · Depends on: A7 · Phase 1

### F. Automations

- [x] **F1 Automations as stored prompts** — Capy: A stored prompt in one project that runs on triggers; each run is a normal thread. (screens: 12, 125) · Neko today: has (schedules-the-agents-that-start-themselves.md, neko-runs-the-day.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **F2 Schedule trigger** — Capy: Cron with an IANA timezone, minimum five minutes; missed runs coalesce. (screens: 12) · Neko today: partial (rune-parity-a.md (Every/Daily editor), neko-maintains-itself.md (catch-up)) · Build: Add weekdays, timezone and a cron field to the cadence editor. · Size M · Priority P1 · Depends on: none · Phase 2
- [ ] **F3 GitHub triggers** — Capy: PR opened, pushed, merged; comments, reviews, labels, checks, workflow runs, branch pushes. (screens: 12) · Neko today: partial (pull-requests-after-they-open.md (polls PRs)) · Build: Polling triggers through `gh` first; webhooks arrive with F5. · Size L · Priority P1 · Depends on: F5 · Phase 2
- [ ] **F4 Slack trigger** — Capy: Channel messages (bursts grouped 1-300 s) and a Slack reaction start a run. (screens: 42) · Neko today: missing (neko-runs-the-day.md (reads chat servers)) · Build: Behind the Slack front door. · Size M · Priority P3 · Depends on: I14 · Phase 4
- [ ] **F5 Incoming webhook trigger** — Capy: A secret URL with an `Idempotency-Key` header and a 256 KB body limit starts a run; rotate the secret. (screens: 12) · Neko today: missing · Build: Loopback endpoint with a per-automation secret on the SA-F3 endpoint server; a tunnel is the owner's choice. · Size M · Priority P1 · Depends on: L1 · Phase 2
- [x] **F6 On demand** — Capy: Button, API or agent starts a run. (screens: none) · Neko today: has (routine-guard-rails.md (Run now)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **F7 Linear trigger** — Capy: A Linear event starts a run. (screens: 39) · Neko today: partial (neko-runs-the-day.md (watches)) · Build: Reuse the watch as a trigger. · Size S · Priority P2 · Depends on: none · Phase 2
- [ ] **F8 Conditions and run_when** — Capy: Filters on repos, branches, authors, labels with regex; a free fast-model check of a plain-English sentence before the run. (screens: 12) · Neko today: partial (routine-guard-rails.md (`run_only_when`, a sandboxed command)) · Build: Add the plain-English check (small model) and the filters; keep the command form. · Size S · Priority P1 · Depends on: F2 · Phase 2
- [ ] **F9 Model pin** — Capy: Pinned (fails visibly if rights are lost) or unpinned (follows defaults). (screens: 12) · Neko today: partial (rune-parity-a.md (pins per axis)) · Build: Pinned or follows the default, per automation. · Size S · Priority P2 · Depends on: none · Phase 2
- [x] **F10 Standing or fresh thread** — Capy: `new` makes a thread per run, `single` keeps one standing thread. (screens: none) · Neko today: has (routine-guard-rails.md (`thread: standing`)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **F11 Run limits** — Capy: Opt-in max runs per day; over-cap runs recorded as skipped. (screens: none) · Neko today: has (routine-guard-rails.md (`daily_cap`, coalesced skip rows)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **F12 Missed runs coalesce** — Capy: After downtime one catch-up run, not many. (screens: none) · Neko today: has (neko-maintains-itself.md (one catch-up per routine)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **F13 Automation management** — Capy: Create in app, by asking, or API; enable/disable, delete/restore, run, cancel a run, list runs, filter by project/state/date. (screens: 125, 91) · Neko today: partial (rune-parity-c.md (Schedules), routine-guard-rails.md) · Build: Run list with skip reasons, Cancel run, restore a deleted one, filters. · Size M · Priority P2 · Depends on: none · Phase 2
- [ ] **F14 Automation notes carry state** — Capy: A Drive scope for an automation carries state between runs. (screens: none) · Neko today: partial (routine-guard-rails.md (standing thread)) · Build: A NOTES.md per automation, read at each run. · Size S · Priority P2 · Depends on: I1 · Phase 3
- [ ] **F15 Several triggers per automation** — Capy: One to twenty triggers, OR'd. (screens: 12) · Neko today: missing · Build: List of triggers with Add trigger. · Size S · Priority P2 · Depends on: F2 · Phase 2
- [ ] **F16 Automation recipes** — Capy: Examples: nightly dependency check, doc fix on merge, CI-failure fixer, Slack support triage, deploy-failure investigation. (screens: none) · Neko today: missing (packs.md) · Build: Five recipes as a pack, each starting in record-first mode. · Size S · Priority P2 · Depends on: F8, F20 · Phase 2
- [ ] **F17 Automations belong to a captain** — Capy: Captains schedule themselves and receive tickets from automations. (screens: 70) · Neko today: missing · Build: `captain_id` on an automation; the overview's Automations group shows each (v5: "Weekdays at 9:00 · review open pull requests · runs only when there are open PRs · 3 of 10 runs today · fresh session each time"). · Size M · Priority P0 · Depends on: A1 · Phase 1
- [ ] **F18 Captain schedules itself** — Capy: Ask a Captain to schedule itself ("Every weekday at 9am ..."). (screens: 70) · Neko today: partial (the-chat-directs-the-crew.md (`schedule` tool)) · Build: `schedule` creates an automation owned by the captain after one click. · Size S · Priority P1 · Depends on: F17 · Phase 2
- [ ] **F19 Automation editor** — Capy: Name, description, Active toggle, Triggers card (weekday chips, time, timezone), Instructions with model and reasoning chips. (screens: 12, 125) · Neko today: partial (rune-parity-a.md, rune-p2-the-pages.md (Routines page)) · Build: The editor in the v5 look. · Size M · Priority P2 · Depends on: F2 · Phase 2
- [ ] **F20 Record first (Neko's own)** — Capy: (Not in Capy. In the v5 settings: new automations only write down what they would have done until you let them act.) (screens: none) · Neko today: missing (docs/design/super-agent-features-2026-11.md (SA-F1 planned)) · Build: Build SA-F1 as specified: `mode: act | record`, "Would have" journal line. · Size M · Priority P1 · Depends on: none · Phase 2

### G. Models, cost and usage

- [ ] **G1 Cost in the header** — Capy: A dollar total at the top of a Captain's or thread's chat ("$8.57"); cost of a Captain is model usage plus machine time. (screens: 81, 25) · Neko today: partial (usage-ledger.md (per-run rows, History)) · Build: Running total in every captain and session header and on the account footer (v5: "$6.20 today"); setting Show cost. · Size M · Priority P0 · Depends on: none · Phase 1
- [ ] **G2 Model picker** — Capy: Search, Favourites with Option+1..9, Featured, provider groups, a badge for the connection used. (screens: 49, 137) · Neko today: partial (rune-parity-a.md (model chip), docs/design/2026-10-06-model-effort-speed.md) · Build: Favourites with ⌥1..9, grouped providers, search. · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **G3 Model side panel** — Capy: Context, reasoning slider, Fast toggle, Runs on, Favorite. (screens: 137, 127) · Neko today: partial (docs/design/2026-10-06-model-effort-speed.md) · Build: Side panel with Favorite and Runs on added. · Size M · Priority P2 · Depends on: G2, G6 · Phase 3
- [x] **G4 Reasoning effort and Fast mode** — Capy: Effort none to max; Fast mode; models for subagents. (screens: none) · Neko today: has (docs/design/2026-10-06-model-effort-speed.md, rune-parity-a.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **G5 Subagent models** — Capy: Choose a different model for subagents. (screens: none) · Neko today: missing · Build: Setting: sessions' children use a cheaper model by default. · Size S · Priority P3 · Depends on: B33 · Phase 4
- [ ] **G6 Ordered connections** — Capy: Subscriptions, API keys, coding plans, gateways and custom endpoints in priority order; a thread runs on the first that serves its model. (screens: 46, 47, 48) · Neko today: partial (usage-quota-read-from-each-provider-s-own-api-and-the-meter-.md, switching-provider-when-a-quota-runs-out.md) · Build: Settings → AI: ordered list of the agent CLIs and keys Neko may use, plus a local model endpoint. · Size L · Priority P1 · Depends on: none · Phase 3
- [ ] **G7 Usage caps per connection** — Capy: Several subscriptions share load with usage caps. (screens: 46) · Neko today: missing · Build: Cap per connection per window. · Size M · Priority P2 · Depends on: G6 · Phase 3
- [ ] **G8 Run-out behaviour** — Capy: Wait for the reset or use the next connection or credits; a default with an override. (screens: 50) · Neko today: partial (switching-provider-when-a-quota-runs-out.md, goals-that-keep-working.md (quota hold)) · Build: A setting: Wait for reset (default) or Use next connection; per captain override. · Size S · Priority P1 · Depends on: G6 · Phase 3
- [ ] **G9 Runs-on badge** — Capy: The model chip shows which connection serves it. (screens: 137) · Neko today: missing · Build: Small badge on chip and session header. · Size S · Priority P3 · Depends on: G6 · Phase 3
- [ ] **G10 Usage page** — Capy: Spend over any window split billed vs on your own keys, by member, model, thread; Activity hours. (screens: 02, 101) · Neko today: partial (usage-ledger.md (History, CSV), rune-p2-the-pages.md (Usage)) · Build: Split by captain, model and session; reported vs estimated stays. · Size M · Priority P2 · Depends on: none · Phase 3
- [x] **G11 Usage export** — Capy: Export by hour or day. (screens: 02) · Neko today: has (usage-ledger.md (UsageCsv)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **G12 Activity page** — Capy: Hours agents worked. (screens: 101) · Neko today: missing · Build: Hours per captain per day beside Usage. · Size S · Priority P3 · Depends on: G10 · Phase 4
- [ ] **G13 Cost transparency on request** — Capy: Ask what a thread cost: tokens by kind, turns, dollars charged. (screens: none) · Neko today: partial (goal-receipts.md (cost in the receipt), usage-ledger.md) · Build: A `cost_of` chat tool. · Size S · Priority P2 · Depends on: none · Phase 3
- [x] **G14 Pause instead of fail on money** — Capy: Out of credits pauses a thread, says whether model or machine needs funding, offers a free model. (screens: none) · Neko today: has (goals-that-keep-working.md (quota hold, budget pause), switching-provider-when-a-quota-runs-out.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **G15 Daily spend ceiling** — Capy: A cap that stops work for the day. (screens: none) · Neko today: has (neko-maintains-itself.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **G16 Model catalogue detail** — Capy: About 60 models with context, price per token, BYOK flag and retention status. (screens: 49) · Neko today: partial (rune-parity-a.md (AgentModels catalogue)) · Build: Show context and price where Neko knows them. · Size S · Priority P3 · Depends on: G2 · Phase 4

### H. Machines, devices and handoff

- [ ] **H1 Where it runs, on every row** — Capy: Cloud or device shown on every thread row; the composer's machine menu can move it. (screens: 31, 33) · Neko today: missing · Build: "This Mac" or a device name on rows, hidden while only one Mac exists. · Size S · Priority P3 · Depends on: H2 · Phase 4
- [ ] **H2 Pair another computer as a device** — Capy: A computer paired by the desktop app or `capy dock` (QR or code); a headless server; personal or shared. (screens: 06, 31) · Neko today: missing (docs/design/super-agent-features-2026-11.md (SA-F3 pairing is for outside agents)) · Build: A second Mac pairs with a code and runs sessions for this one over a private link. · Size XL · Priority P3 · Depends on: L1 · Phase 4
- [ ] **H3 Handoff between devices** — Capy: Continue on cloud or device carries branch, commits, staged, unstaged and untracked changes; automatic offline handoff. (screens: 33, 34, 84) · Neko today: partial (checkpoints-and-undo.md, CLAUDE.md (parking saves work to the branch)) · Build: Move a session by checkpoint to a paired device. · Size L · Priority P3 · Depends on: H2 · Phase 4
- [ ] **H4 Keep awake while agents run** — Capy: Mac stays awake on power while agents run, off 10 minutes after activity; Keep awake for 4 h. (screens: 93) · Neko today: missing (CLAUDE.md (machine steward)) · Build: Optional, off by default, never while hot. · Size S · Priority P2 · Depends on: none · Phase 3
- [ ] **H5 Run one session on a named device** — Capy: "Run this one on my Mac mini." (screens: none) · Neko today: missing · Build: Machine choice in the composer once devices exist. · Size S · Priority P3 · Depends on: H2 · Phase 4
- [ ] **H6 Environment setup scripts** — Capy: `initialize` and `refresh` scripts, snapshots, startup entries as named terminals with readiness ports and previews. (screens: 36, 98) · Neko today: partial (CLAUDE.md (reseed node_modules in a worktree)) · Build: Per-workspace setup script run in each new worktree; a Setup tab with its output. · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **H7 Startup entries with previews** — Capy: Named terminals with readiness ports and a preview link. (screens: 37, 45) · Neko today: missing · Build: Dev-server entries with a preview chip. · Size M · Priority P3 · Depends on: H6 · Phase 4
- [ ] **H8 Secrets** — Capy: Project environment variables encrypted at rest, injected at boot; the agent sees names only; paste a `.env`. (screens: 55) · Neko today: missing · Build: Stored in the Keychain, injected into run env, names only in prompts; an env-var needs-you card (C6). · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **H9 Desktop app features** — Capy: Tabs and multiple windows; worktree default, existing worktree or current checkout; import Claude Code and Codex history; notifications. (screens: 82, 90) · Neko today: partial (rune-as-the-app.md, CLAUDE.md (worktrees)) · Build: Import Claude Code and Codex history as sessions; checkout choice per workspace. · Size M · Priority P3 · Depends on: none · Phase 4
- [ ] **H10 Projects with icon, code and repoless** — Capy: Repository catalogue with base branch each, icon or emoji, three-letter code, private, repoless projects. (screens: 51) · Neko today: partial (rune-parity-c.md (Workspaces editor)) · Build: Icon and code on a workspace; workspaces with no repository. · Size S · Priority P3 · Depends on: none · Phase 4
- [ ] **H11 Lazy repositories** — Capy: Primary repositories cloned everywhere; lazy ones only when needed, with a generated one-line description. (screens: 51) · Neko today: missing · Build: Per-workspace primary and on-demand repositories. · Size M · Priority P3 · Depends on: H10 · Phase 4
- [ ] **H12 Long-running commands** — Capy: A background command with a timeout up to 6 h. (screens: 93) · Neko today: partial (CLAUDE.md (30 min wall clock)) · Build: Explicit long timeout with a visible wait. · Size S · Priority P3 · Depends on: none · Phase 4
- [x] **H13 Worktree per session** — Capy: Each thread gets its own isolated checkout. (screens: none) · Neko today: has (CLAUDE.md (Worktree lifecycle, writable per repository)) · Build: none · Size – · Priority – · Depends on: – · Phase done

### I. Knowledge: files, skills, memory, plugins, integrations

- [ ] **I1 Files by scope** — Capy: Files, instructions and skills outside git, shared across threads by scope (Organization, Personal, Project, Project + personal, Automation; each Captain has its own); browse, upload. (screens: 35, 57, 129) · Neko today: partial (the-canvas-beside-the-chat.md, docs/design/super-agent-features-2026-11.md (SA-F2 files)) · Build: A Files page: Everywhere, Workspace, Captain, Automation; plain files in the data folder. · Size L · Priority P2 · Depends on: A8 · Phase 3
- [ ] **I2 File history and restore** — Capy: 24-hour history with Restore and Compare. (screens: 35) · Neko today: partial (docs/design/super-agent-features-2026-11.md (`.prev` copy), checkpoints-and-undo.md) · Build: 7-day history per file with Compare and Restore. · Size M · Priority P2 · Depends on: I1 · Phase 3
- [x] **I3 Global instructions** — Capy: Drive-level global instructions and `AGENTS.md` (root, nested, glob rules, Cursor rules read natively). (screens: none) · Neko today: has (neko-is-the-chief-of-staff.md (Rulebook), rune-parity-d.md (rulebook editor)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **I4 Instructions and Skills pages** — Capy: Show what loads and each item's token share; skills flagged Not loaded or Replaced by. (screens: 133) · Neko today: missing (docs/research/super-agent-references-2026-11.md) · Build: A page "What a new session loads" with token share. · Size M · Priority P2 · Depends on: none · Phase 3
- [x] **I5 Skills** — Capy: Reusable `SKILL.md` loaded on demand; sources built-in, repository, files, plugin. (screens: 56, 133) · Neko today: has (tickets-learning-and-checkpoints.md, neko-maintains-itself.md, rune-parity-c.md (Skills)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **I6 Built-in skills** — Capy: `artifact`, setup, find-mcp-server, stacks, deck, page, prototype, paper, spreadsheets. (screens: 56) · Neko today: partial (packs.md) · Build: A starter pack of skills (deck, page, spreadsheet, setup). · Size M · Priority P3 · Depends on: none · Phase 4
- [ ] **I7 New skill form** — Capy: Create a skill from the Files page. (screens: 57) · Neko today: missing · Build: Form in Files; also by asking. · Size S · Priority P3 · Depends on: I1 · Phase 3
- [x] **I8 Memory** — Capy: Memory keeps what you teach across threads (an experiment in Capy). (screens: 92) · Neko today: has (rune-parity-c.md (Memory)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **I9 MCP servers** — Capy: URL or STDIO, OAuth, key or none, per-tool toggles, statuses, import from Claude Code and Codex, set up by the agent. (screens: 40, 94, 95) · Neko today: has (rune-parity-c.md (Tools)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **I10 Plugins and marketplace** — Capy: Verified marketplace, GitHub plugins with Sync, Roll back, Update needs you; the agent proposes a plugin when a service is missing. (screens: 41, 117, 128) · Neko today: has (packs.md, rune-parity-d.md (roll back), self-improvement.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **I11 Explore page** — Capy: Plugins, skills and themes with Popular and Recommended; Submit to Explore. (screens: 128, 41) · Neko today: missing (packs.md) · Build: Browse the bundled catalogue of packs, skills and themes; no cloud store. · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **I12 Design file integration** — Capy: Open, read and edit Paper design files in a thread with an embedded canvas and comments. (screens: 118) · Neko today: partial (one-registry-plugins-are-not-a-separate-world.md) · Build: Through an MCP tool plus the Canvas; no special build. · Size S · Priority P3 · Depends on: none · Phase 4
- [ ] **I13 GitHub integration** — Capy: App install, many accounts, dependency reader, `@capy` and `@capy review` in PR comments, events wake threads. (screens: 38) · Neko today: partial (pull-requests-after-they-open.md (uses `gh`)) · Build: Poll PR comments for `@neko` mentions and start a session. · Size M · Priority P2 · Depends on: E9 · Phase 4
- [ ] **I14 Slack front door** — Capy: `@Capy` starts a thread; synced thread; `aside`, `mute`, `archive`, `stop`; directives `project= folder= model= machine=`; `!fast !queue !steer`; channel-to-project mapping; DM when a PR merges. (screens: 42, 66, 76) · Neko today: missing (neko-runs-the-day.md (reads Slack servers)) · Build: A Slack app in Socket Mode on the daemon (no inbound port): mention starts a session, replies sync, DM on merge. · Size XL · Priority P2 · Depends on: L1, A25 · Phase 4
- [ ] **I15 Slack captain routing** — Capy: `captain=<name>` routes a message to a Captain; `!queue` and `!steer`. (screens: 42) · Neko today: missing · Build: Route by alias with the same delivery modes. · Size S · Priority P3 · Depends on: I14, A25 · Phase 4
- [ ] **I16 Linear front door** — Capy: Assign an issue to Capy: a native agent session maps to one thread; stop signal interrupts; people link accounts once. (screens: 39, 67, 77) · Neko today: partial (neko-runs-the-day.md (Linear watches)) · Build: Delegate flows into a captain's crew; the stop signal interrupts. · Size L · Priority P2 · Depends on: D10, A1 · Phase 4
- [ ] **I17 Web scraping tool** — Capy: A built-in web extraction integration. (screens: 112) · Neko today: partial (packs.md) · Build: A pack, no core work. · Size S · Priority P3 · Depends on: none · Phase 4
- [ ] **I18 Goal packs (Neko's own, SA-F8)** — Capy: (Not in Capy.) A pack may carry goals, installed paused. (screens: none) · Neko today: missing (docs/design/super-agent-features-2026-11.md (SA-SA-F8 planned)) · Build: Build SA-F8 as specified. · Size M · Priority P2 · Depends on: none · Phase 3
- [x] **I19 Chat server sources** — Capy: Slack, Teams, Discord checked on a timer. (screens: none) · Neko today: has (neko-runs-the-day.md) · Build: none · Size – · Priority – · Depends on: – · Phase done

### J. Personalisation, themes, onboarding, settings

- [ ] **J1 Themes** — Capy: 14 built-ins, your own, a dashboard background image or GIF with brightness, blur, opacity, fade, a dither effect, code font size, synced across devices. (screens: 130) · Neko today: partial (themes.md, rune-parity-d.md (make theme, derive.js), rune-polish.md (dithered art)) · Build: Add the background image and dither effect to the theme editor. · Size M · Priority P3 · Depends on: none · Phase 4
- [ ] **J2 Theme sharing** — Capy: Submit a theme to Explore; sync across devices. (screens: 130) · Neko today: missing · Build: Export and import a theme file; sync with H2. · Size S · Priority P3 · Depends on: J1 · Phase 4
- [ ] **J3 Ask Neko to change the look** — Capy: Ask in chat to change appearance, with Undo. (screens: 130) · Neko today: partial (rune-parity-d.md (Change Neko)) · Build: An `appearance` chat tool with Undo. · Size S · Priority P3 · Depends on: none · Phase 4
- [ ] **J4 Shortcuts page** — Capy: All shortcuts remappable in Settings → Shortcuts. (screens: 59) · Neko today: partial (rune-parity-a.md (`?` overlay)) · Build: Rebindable keys in Settings. · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **J5 Notifications** — Capy: Sounds with a custom upload, system alerts, push, a page to turn off emails; Slack DM when a PR merges. (screens: 32) · Neko today: partial (markdown-in-the-detail-pane-the-menu-bar-menu-the-chime-and-.md, rune-as-the-app.md (routine notifications)) · Build: Sound choice, per-kind switches (needs you, finished, failed). · Size S · Priority P3 · Depends on: none · Phase 4
- [x] **J6 First run** — Capy: Sign in, connect GitHub, pick repositories, first thread; short steps that survive a reload. (screens: 53) · Neko today: has (first-run.md, rune-parity-d.md (six-step first run)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **J7 First captain in first run** — Capy: (Neko addition.) (screens: none) · Neko today: missing (first-run.md) · Build: Last step offers "Let Neko choose a captain for your busiest repository" or skip. · Size S · Priority P0 · Depends on: A2 · Phase 1
- [ ] **J8 Settings for captains** — Capy: General, Captains, AI and agents, Phone, Rulebook, Appearance, This Mac, Quick panel, Permissions, Updates, About (v5); Capy has many more pages. (screens: 10, 46) · Neko today: partial (rune-parity-d.md (Settings searchable)) · Build: Add Captains page, On its own (Automations, Record first, Take the default), Spending (Daily limit, Show cost). · Size M · Priority P0 · Depends on: A1, G1 · Phase 1
- [ ] **J9 Experiments page** — Capy: Opt-in features, e.g. Memory. (screens: 92) · Neko today: missing · Build: A short opt-in list. · Size S · Priority P3 · Depends on: none · Phase 4
- [x] **J10 Appearance follows the system** — Capy: Light, dark, system. (screens: none) · Neko today: has (current-standalone-workspace.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **J11 Account footer** — Capy: Avatar, name, plan, org, a status dot. (screens: 141, 81) · Neko today: partial (rune-polish.md) · Build: Avatar, name and today's cost (v5 "$6.20 today"). · Size S · Priority P0 · Depends on: G1 · Phase 1
- [ ] **J12 Code font size** — Capy: Setting. (screens: 130) · Neko today: missing · Build: Setting beside theme. · Size S · Priority P3 · Depends on: none · Phase 4

### K. Design system and UI craft

- [ ] **K1 One 16 px icon set** — Capy: (Neko craft item 1.) (screens: none) · Neko today: partial (docs/design/principles.md, native-polish.md) · Build: One set drawn on a 16 px grid, one stroke weight, filled variant for active, replacing mixed glyphs. · Size M · Priority P0 · Depends on: none · Phase 1
- [ ] **K2 Strict 4/8 grid and alignment** — Capy: (Neko craft item 2.) Capy rows and chrome sit on a regular rhythm. (screens: none) · Neko today: partial (docs/design/principles.md (DesignSystemGuardTests caps raw padding)) · Build: Audit every screen to the 4/8 rhythm; guard test lowers its ceiling; alignment checked as one region. · Size M · Priority P0 · Depends on: none · Phase 1
- [ ] **K3 Faces tuned per size** — Capy: (Neko craft item 3.) Capy faces appear at about 80, 44 and 14-16 px. (screens: 141, 132, 25) · Neko today: missing · Build: Hand-tuned variants at 16, 24, 44, 80 px; state glyph hidden under 20 px. · Size M · Priority P0 · Depends on: A3 · Phase 1
- [ ] **K4 Hover previews and right-click menus** — Capy: (Neko craft item 4.) Capy reveals actions on hover. (screens: 61) · Neko today: partial (rune-parity-a.md (right-click menus, hover actions)) · Build: Hover preview card for sessions and captains (latest line, cost, state); right-click menu on every row. · Size M · Priority P0 · Depends on: none · Phase 1
- [ ] **K5 Keyboard-first** — Capy: (Neko craft item 5.) Capy: Cmd+K, Option+1..9, many shortcuts. (screens: 103, 59) · Neko today: partial (rune-parity-a.md (palette, J/K, `?`)) · Build: ⌘K everywhere, J/K in every list, 1 to 3 for the overview tabs and the answer options. · Size M · Priority P0 · Depends on: none · Phase 1
- [ ] **K6 Dot-grid status glyph** — Capy: Animated dot grid for Working, slower for Waiting, red for Failed. (screens: 61) · Neko today: partial (the-shared-pulse-clock-and-the-repeating-animation-rule.md) · Build: Stepped 3x3 grid on the shared pulse clock; static under Reduce Motion. · Size S · Priority P0 · Depends on: none · Phase 1
- [ ] **K7 Accent shows only where you are needed** — Capy: Blue is the colour of needs-you text, count badges and a face's dot. (screens: 141) · Neko today: missing · Build: Reserve the accent for needs-you text, counts and the face dot. · Size S · Priority P0 · Depends on: none · Phase 1
- [ ] **K8 Grouped lists with bars and counts** — Capy: Grey group bars with counts and a collapse chevron; a "1 more" expander. (screens: 13, 132) · Neko today: partial (rune-polish.md) · Build: One group-bar component for every list. · Size S · Priority P0 · Depends on: none · Phase 1
- [ ] **K9 Dense rows with chips** — Capy: Single-line rows with model, repo, labels, PR count, age; Detailed rows toggle. (screens: 61, 132) · Neko today: partial (rune-polish.md) · Build: Row with up to four chips and age; Detailed shows the last message. · Size M · Priority P2 · Depends on: none · Phase 3
- [ ] **K10 State-coloured PR and issue chips** — Capy: PR chips yellow when open, purple merged, green ready; inline in sentences. (screens: 13, 132) · Neko today: missing · Build: Chip component in Neko's palette. · Size S · Priority P0 · Depends on: none · Phase 1
- [ ] **K11 Cats in empty and error states** — Capy: Illustrated mascots in roles on empty and error states; a sleeping mascot on the 404 page. (screens: none) · Neko today: missing (rune-polish.md (dithered art)) · Build: Sleeping cat for empty Inbox and 404; the same faces' style. · Size S · Priority P3 · Depends on: A3 · Phase 4
- [ ] **K12 Rows hold their place** — Capy: Finished threads open at rest; rows being read stay put when a long thread re-lays out; steps no longer blink. (screens: none) · Neko today: partial (the-root-list-scrolled-all-along-the-keyboard-just-never-mov.md) · Build: No reorder under the pointer; no relayout of the row being read. · Size S · Priority P2 · Depends on: none · Phase 2
- [ ] **K13 Receding rows** — Capy: Rows that need nothing fade to about 75% and return on hover. (screens: none) · Neko today: missing (docs/research/design-references-2026-11.md) · Build: Quiet rows recede. · Size S · Priority P2 · Depends on: B2 · Phase 2
- [ ] **K14 Bottom page bar** — Capy: An icon bar of pages (Threads, Issues, PRs, Automations, Explore, Drive, More), draggable. (screens: 141) · Neko today: missing · Build: Not needed in v5's sidebar; a page strip only if the sidebar grows. · Size S · Priority P3 · Depends on: none · Phase 4
- [x] **K15 Dark mode as first-class** — Capy: Dark mode exists everywhere. (screens: 81) · Neko today: has (current-standalone-workspace.md, themes.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **K16 Toasts with Undo** — Capy: Toast after archive, dismiss and similar, with Undo. (screens: 64) · Neko today: has (rune-parity-a.md (toasts)) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **K17 Calm empty states** — Capy: Plain empty states. (screens: none) · Neko today: has (docs/design/principles.md ("Nothing needs you right now")) · Build: none · Size – · Priority – · Depends on: – · Phase done

### L. Platform: API, CLI, mobile/phone reach

- [ ] **L1 Outside-agent endpoint (SA-F3)** — Capy: (Not Capy's shape.) The base for the Capy API: let other agents and scripts read and steer Neko. (screens: 66) · Neko today: missing (docs/design/super-agent-features-2026-11.md (SA-F3 planned; R1-R4 rulings)) · Build: Build SA-F3 as ruled: loopback MCP endpoint, off by default, pairing on the native socket, levels read and steer. · Size L · Priority P1 · Depends on: none · Phase 2
- [ ] **L2 Public API for the whole product** — Capy: Keys per organisation acting as a user or service user; resources Projects, Threads, Messages, Files, Attachments, Streams (SSE), Subagents, Folders, Captains, Issues, Automations, Reviews, Review rules, Usage, Users, Models; idempotent `requestId`. (screens: 66) · Neko today: partial (chat-tool-authority.md, docs/design/super-agent-features-2026-11.md (SA-F3)) · Build: A documented local HTTP and SSE API over the same requests the UI sends, tokens from the SA-F3 pairing, `requestId`. · Size XL · Priority P2 · Depends on: L1 · Phase 4
- [ ] **L3 CLI** — Capy: `capy dock` and friends: status, doctor, dirs, verify, version; start and follow threads. (screens: none) · Neko today: partial (CLAUDE.md (`neko-daemon --machine-check`, `--request`)) · Build: `neko` with new, follow, answer, status, doctor. · Size M · Priority P2 · Depends on: L1 · Phase 4
- [ ] **L4 Follow from any browser, incl. phone** — Capy: Start, follow and reply from any browser; iPhone push when added to the Home Screen. (screens: 32) · Neko today: partial (phone-push.md (Reminders, ntfy, reply line)) · Build: A phone page served by the daemon over a private link (Tailscale or a paired tunnel), same levels as SA-F3; read, answer, steer. · Size L · Priority P2 · Depends on: L1 · Phase 4
- [x] **L5 Phone reply by one line** — Capy: Answer from the phone. (screens: none) · Neko today: has (phone-push.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [x] **L6 Dock and menu bar badge** — Capy: A badge of what needs you. (screens: none) · Neko today: has (rune-as-the-app.md, out-of-the-dock-the-activation-policy-and-the-menu-bar-item.md) · Build: none · Size – · Priority – · Depends on: – · Phase done
- [ ] **L7 Outgoing event hooks** — Capy: (Guess: Capy API streams.) A URL called on session events. (screens: 66) · Neko today: missing · Build: Optional URL per captain for needs-you and finished. · Size S · Priority P3 · Depends on: L1 · Phase 4

## 3. Not planned

Recommended not to build. The owner may override any of them; each keeps its reason so a reversal is a decision, not a rediscovery.

- [ ] **X1 Members, roles, invites, domain join** — not planned: Neko is one person's agent; a roles system is cost with no user.
- [ ] **X2 SSO, SAML, MFA, passkey enforcement** — not planned: Needs an identity server and a team; there is none.
- [ ] **X3 Service users and run-as principals** — not planned: One owner; every run is the owner's.
- [ ] **X4 Private/shared visibility, share dialogs, ownership transfer, ask-for-access** — not planned: Everything is already private to this Mac.
- [ ] **X5 Cloud VMs, machine sizes, machine minutes, sleep rules** — not planned: Cloud-only; Neko runs on the owner's Macs.
- [ ] **X6 Cloud phones (iOS and Android devices as machines)** — not planned: Cloud-only and expensive; test on the owner's own devices.
- [ ] **X7 Tailscale cloud nodes and OIDC cloud access (AWS, Google Cloud, Azure, Vault)** — not planned: Only meaningful for cloud machines holding cloud credentials.
- [ ] **X8 Plans, credits, top-ups, auto-reload, invoices, sponsored allowance, programmes** — not planned: There is nothing to bill.
- [ ] **X9 Free-with-ads models** — not planned: No ad network; the owner's own subscriptions pay.
- [ ] **X10 Review credits (1, 5, 10 per review)** — not planned: Reviews cost the owner's own tokens; show cost instead (G1).
- [ ] **X11 Organisation policy controls ("Only this key", "Never use credits") and audit log for admins** — not planned: Single owner; the usage ledger and run journal already record what happened.
- [ ] **X12 Windows and Linux builds** — not planned: macOS-only product.
- [ ] **X13 A tab strip of open chats** — not planned: The owner's v5 layout has no tabs (ADR 0002).
- [ ] **X14 A native mobile app** — not planned: Phone reach is Reminders, ntfy and a phone page (L4).
- [ ] **X15 In-app support chat and SLA, Slack Connect** — not planned: No company behind Neko to staff it.
- [ ] **X16 Streamed cloud desktop (XFCE) in a pane** — not planned: Cloud-only; the Mac's own screen is already there.
- [ ] **X17 SOC 2 programme and zero-retention agreements** — not planned: A vendor certification; Neko keeps data local.

## 4. Phases

### Phase 1: The v5 core

42 items, about 60 agent-days (about 17 working days with parallel agents).

**At the end the owner can:** Create a captain with a cat face, give it a goal in one sentence, watch it split the work into sessions, and see "3 working, 2 need you, $6.20 today" on Home. Answer a question in one line, take the default, send it Later or Dismiss it. Read the captain's one-line digest, edit its RULES, CREW and PLAN files, see cost beside every captain and session, and let finished work settle out of sight. The five craft items are in: one icon set, a 4/8 grid, faces tuned per size, hover previews with right-click menus, and keyboard-first lists.

Pre-condition: the owner answers open question 1 (face art) before A3 starts; mockup approval for any screen not in v5.

**Build order** (dependencies first, then priority):

1. A1 Captain record (L)
2. A3 Captain faces (L)
3. A8 Captain files (SA-F2) (L)
4. A2 New captain dialog (M)
5. A10 Crew membership (M)
6. A19 Per-captain model (M)
7. B35 Turn summaries (S)
8. B36 Step list in a session (M)
9. C1 Needs-you as a flag (M)
10. C2 One-line digest (L)
11. A17 Captain wakes on crew events (L)
12. A18 Captain splits a request into sessions (M)
13. A28 Home composer routes work to a captain (M)
14. C4 Answer in one line (M)
15. A27 Captain chat (M)
16. C5 Use the default, Answer, Later, Dismiss (S)
17. A12 Open-questions menu and count (S)
18. C9 Digest posted in chat, tagging only when it is yours (S)
19. F17 Automations belong to a captain (M)
20. G1 Cost in the header (M)
21. A20 Captain spend limit (M)
22. B37 Session header (M)
23. J7 First captain in first run (S)
24. J8 Settings for captains (M)
25. J11 Account footer (S)
26. K1 One 16 px icon set (M)
27. K2 Strict 4/8 grid and alignment (M)
28. K3 Faces tuned per size (M)
29. K4 Hover previews and right-click menus (M)
30. K5 Keyboard-first (M)
31. K6 Dot-grid status glyph (S)
32. A4 Face state glyph (S)
33. A13 Captains-first sidebar (M)
34. B2 Session states: Working, Waiting, Failed, Idle (M)
35. C8 Settled (M)
36. A7 Captain overview (L)
37. E19 Ready to merge group (S)
38. K7 Accent shows only where you are needed (S)
39. K8 Grouped lists with bars and counts (S)
40. K10 State-coloured PR and issue chips (S)
41. C3 Digest row anatomy (M)
42. C15 Home (M)

### Phase 2: Automations and PR depth

39 items, about 47.5 agent-days (about 14 working days with parallel agents).

**At the end the owner can:** Give a captain automations that run on a schedule, GitHub or a webhook, with a plain-English `run_only_when`, a daily cap and record-first mode. Review a PR in Neko: findings with severity and confidence, a guided walkthrough, per-file Reviewed checkboxes, Approve or Request changes posted to GitHub, a merge menu with the blocking reason, stacked PRs, and a review agent that runs once or on every push.

**Build order** (dependencies first, then priority):

1. A9 Rules as sentences (M)
2. E1 Open a PR from a session (S)
3. E16 PR page (M)
4. E3 Merge menu (M)
5. E5 Stacked PRs (L)
6. E7 Review agent with findings (L)
7. E8 Triage of findings (M)
8. E9 When a review starts (M)
9. E11 Review in the app (L)
10. E12 Guided PR walkthrough (L)
11. F2 Schedule trigger (M)
12. F8 Conditions and run_when (S)
13. F18 Captain schedules itself (S)
14. F20 Record first (Neko's own) (M)
15. L1 Outside-agent endpoint (SA-F3) (L)
16. F5 Incoming webhook trigger (M)
17. F3 GitHub triggers (L)
18. A6 Ask-to-create a captain (S)
19. A23 Finished-sessions strip in the captain chat (S)
20. A24 Crew chip above the composer (S)
21. B1 Session codes (S)
22. B25 Command menu search depth (M)
23. B10 /plan (S)
24. B29 Unread and mention badges (S)
25. B30 Waiting-on line and running watches (M)
26. C10 Shape the digest by telling the captain (S)
27. E4 Wake settings per event (S)
28. E10 Review rules and filters (M)
29. E14 Deploy preview chips (S)
30. E17 PRs filtered by captain (S)
31. F7 Linear trigger (S)
32. F9 Model pin (S)
33. F13 Automation management (M)
34. F15 Several triggers per automation (S)
35. F16 Automation recipes (S)
36. A22 Starter captain templates (S)
37. F19 Automation editor (M)
38. K12 Rows hold their place (S)
39. K13 Receding rows (S)

### Phase 3: Issues, models and usage, knowledge

38 items, about 51 agent-days (about 15 working days with parallel agents).

**At the end the owner can:** Keep issues locally (states, priority, relations, sub-issues, linked PRs and sessions) and let captains file and close them. Order several model connections with caps and choose Wait or Use next when one runs out; favourites in the model picker; usage by captain, model and session. Browse Files by scope with history, see what a new session loads, use Explore for packs, skills and themes, and run the outside-agent endpoint.

**Build order** (dependencies first, then priority):

1. D2 Issue record (L)
2. D1 Issues page (L)
3. D5 Sub-issues and relations (M)
4. D6 Agents file, update and close issues (M)
5. D7 Linked PRs and sessions on an issue (M)
6. G6 Ordered connections (L)
7. G8 Run-out behaviour (S)
8. A16 Captain archive, delete, demote (S)
9. B6 Edit a sent message and keep versions (M)
10. B11 Skill picker (S)
11. B13 Side chat (M)
12. B9 Slash commands (S)
13. B24 Archive with undo and bulk select (S)
14. B28 Pinned sessions, folders, tags (M)
15. B33 Subagents (M)
16. D3 Custom states (S)
17. G2 Model picker (M)
18. G3 Model side panel (M)
19. G7 Usage caps per connection (M)
20. G10 Usage page (M)
21. G13 Cost transparency on request (S)
22. H4 Keep awake while agents run (S)
23. H6 Environment setup scripts (M)
24. H8 Secrets (M)
25. C6 Typed request cards (M)
26. I1 Files by scope (L)
27. F14 Automation notes carry state (S)
28. I2 File history and restore (M)
29. I4 Instructions and Skills pages (M)
30. B23 Context meter and compaction (M)
31. I11 Explore page (M)
32. I18 Goal packs (Neko's own, SA-F8) (M)
33. J4 Shortcuts page (M)
34. K9 Dense rows with chips (M)
35. B26 Sidebar options (M)
36. D4 Issue code prefix (S)
37. G9 Runs-on badge (S)
38. I7 New skill form (S)

### Phase 4: The rest

57 items, about 76 agent-days (about 22 working days with parallel agents).

**At the end the owner can:** The rest: Slack and Linear front doors, the public API and CLI, a phone page, a second Mac as a device with handoff, split panes, annotations, forks and fork summaries, voice dictation, import of Claude Code and Codex history, and finishing touches on themes and cats.

**Build order** (dependencies first, then priority):

1. A5 Promote a session to a captain and demote back (M)
2. B7 Voice dictation (M)
3. D10 Linear two-way sync (L)
4. I13 GitHub integration (M)
5. I16 Linear front door (L)
6. L2 Public API for the whole product (XL)
7. L3 CLI (M)
8. L4 Follow from any browser, incl. phone (L)
9. A11 Drag a session onto a captain (S)
10. A14 Hybrid vs split sidebar layouts (S)
11. A15 Captains message each other (M)
12. A25 Alias for chat and Slack mentions (S)
13. I14 Slack front door (XL)
14. B8 Reply to a message (S)
15. B12 Inline visuals in chat (M)
16. B14 Fork and fork-summary (M)
17. B16 Split panes (L)
18. B18 Annotate images, video and 3D (M)
19. B19 In-app browser with element picker (L)
20. B21 Message ticks (S)
21. B22 Auto titles and title rules (S)
22. B27 Saved views (S)
23. B34 Editable subagent draft (S)
24. B38 Replay stream (M)
25. D8 Display menu and quick status change (S)
26. D9 Mentions in comments (S)
27. D11 Saved filters (S)
28. E2 Commit identity (S)
29. E6 Fork PRs (M)
30. E13 Copy finding as a prompt (S)
31. F4 Slack trigger (M)
32. G5 Subagent models (S)
33. G12 Activity page (S)
34. G16 Model catalogue detail (S)
35. H2 Pair another computer as a device (XL)
36. A26 Captain machines (crew runs on) (S)
37. H1 Where it runs, on every row (S)
38. H3 Handoff between devices (L)
39. H5 Run one session on a named device (S)
40. H7 Startup entries with previews (M)
41. H9 Desktop app features (M)
42. H10 Projects with icon, code and repoless (S)
43. H11 Lazy repositories (M)
44. H12 Long-running commands (S)
45. I6 Built-in skills (M)
46. I12 Design file integration (S)
47. I15 Slack captain routing (S)
48. I17 Web scraping tool (S)
49. J1 Themes (M)
50. J2 Theme sharing (S)
51. J3 Ask Neko to change the look (S)
52. J5 Notifications (S)
53. J9 Experiments page (S)
54. J12 Code font size (S)
55. K11 Cats in empty and error states (S)
56. K14 Bottom page bar (S)
57. L7 Outgoing event hooks (S)

## 5. Open questions for the owner

1. **Face art.** Who draws the cats for A3? (a) a fixed set of 8 to 12 hand-made SVG cats drawn by an agent from the v5 mockup and tuned per size, (b) a commissioned illustrator, (c) generated images. Recommendation: (a), photo upload as the escape hatch.
2. **Issues.** Build the local tracker of epic D (about 16 agent-days), or keep Neko's sessions as the only work list and sync with Linear and GitHub issues instead? Recommendation: build the thin local version (D1, D2, D5, D6) and skip the rest of D until it is used.
3. **Reach beyond this Mac.** Build a second-Mac device with handoff (H2, H3, about 9 agent-days) and a phone page over a private link (L4), or stay single-Mac with Reminders and ntfy? Recommendation: L4 first, devices later.
4. **Front doors.** Which first: Slack (I14, a Socket Mode app, XL) or Linear delegation (I16, L)? Recommendation: Linear first; Neko already reads Linear.
5. **How far does the review product go?** Findings, triage, guide and in-app review (E7 to E12, about 14 agent-days) or only the guide and findings (E7, E12)? Recommendation: findings and guide first, in-app posting after.

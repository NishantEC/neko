# Rune against the web: identity report (2026-11-18)

Percentages only; the pictures are of the owner's own data and stay in `/tmp/neko-rune-identity` (never committed).
Method and caveats: `docs/knowledge/rune-identity.md`. `apps/neko-rune/scripts/identity.sh` regenerates this table.

Dark, Rune's content region against the web page in embed mode, **one fixed 1440x900 window at 2x** (Setup content is 634 wide,
Settings and the other pages 834 wide, which puts the web under its 640 and 900 point breakpoints). Pass threshold 2.5% differing
pixels (pixelmatch, threshold 0.1, anti-aliasing ignored). "First run" is the original code at 1440; "Baseline at 1440" is the state
after Gate 1 (structure fixes); "After" is the last run of this pass. The settings-budget and settings-workspaces pairs are dropped:
the web has no such sections (Budget is part of "AI & agents", Workspaces is a Setup page).

**27 of 27 pairs pass the 2.5% pixel measure (the last full run, 2026-11-18, after Pull requests, Schedules, Packs, Tools and the new Improvements page were ported). The words-placed measure is at least 85% for every pair but one: all-sessions-board (76.9%, accepted: an invisible twin title that lifted it was removed on purpose, and nothing like it is to be added). Pull requests, Schedules, Packs, Tools and Improvements are 100%, 100%, 100%, 98% and 100%.**

| Pair | First run | Baseline at 1440 | After | Words placed | Pass |
| --- | ---: | ---: | ---: | ---: | :---: |
| all-sessions-tree | 7.36 | 7.36 | 1.01 | 99% | yes |
| all-sessions-list | 5.77 | 5.77 | 1.33 | 98.9% | yes |
| setup-memory | 6.08 | 5.07 | 0.45 | 100% | yes |
| setup-watching | 4.61 | 4.65 | 0.5 | 100% | yes |
| routines | 4.75 | 4.63 | 0.82 | 97.7% | yes |
| calendar-month | 4.22 | 4.12 | 1.64 | 100% | yes |
| all-sessions-board | 3.95 | 3.95 | 0.62 | 76.9% | yes |
| usage | 4.0 | 3.84 | 0.4 | 94.4% | yes |
| plugins | 3.23 | 3.26 | 0.94 | 94.4% | yes |
| pull-requests | 2.61 | 2.41 | 0.47 | 100% | yes |
| setup-workspaces | 3.4 | 2.41 | 1.02 | 100% | yes |
| setup-schedules | 1.63 | 1.83 | 1.09 | 100% | yes |
| settings-appearance | 12.76 | 12.76 | 1.53 | 91.9% | yes |
| settings-rulebook | 6.51 | 6.51 | 1.13 | 85.7% | yes |
| setup-skills | 8.81 | 7.28 | 1.09 | 95.8% | yes |
| setup-tools | 6.14 | 6.58 | 0.81 | 98% | yes |
| settings-about | 1.39 | 1.39 | 0.91 | 86.2% | yes |
| settings-ai | 3.45 | 3.45 | 0.76 | 88.4% | yes |
| settings-permissions | 2.23 | 2.23 | 0.75 | 90.6% | yes |
| settings-diagnostics | 1.92 | 1.92 | 0.67 | 86.4% | yes |
| settings-updates | 3.3 | 3.29 | 0.67 | 92.9% | yes |
| settings-phone | 1.88 | 1.88 | 0.38 | 90.9% | yes |
| settings-general | 1.77 | 1.77 | 0.38 | 90.6% | yes |
| settings-quick-panel | 1.44 | 1.44 | 0.24 | 86.4% | yes |
| setup-profiles | 1.13 | 0.29 | 0.18 | 100% | yes |
| setup-packs | 4.42 | 3.16 | 0.06 | 100% | yes |
| setup-improvements | n/a (new page) | n/a | 0.18 | 100% | yes |

## Done in this pass

Pull requests, Schedules, Packs, Tools, Improvements (2026-11-18): `PullRequests.tsx` (the work header with "Merge N ready" and its menu, groups and rows with the note line, the 72 point age column, hover actions over the row's end), `Schedules.tsx` (picker, a section per workspace with its "New", rows with two-node recurrence text and the three-dot menu), `Packs.tsx` (Add pack with its plus, Installed, 48 point rows, outline Review/Update buttons), `Tools.tsx` (the picker pill, Add connection button, the banner in two text nodes, trailing words 8 apart) and the new Improvements page (`Improvements.tsx`, `ImproveDetail.tsx`; list, off state, empty state, detail dialog, the `Improve` requests). Not matched: Tools' attention sentence (one JSX text node over two lines, compared as one box; 98%); Improvements' "Open evidence" does not scroll the detail to its heading. Rows of Schedules, Packs and Improvements were compared with placeholder data made in the isolated harness copy (the copied data has no schedule and no pack), never in the live data. Rune-only controls: Schedules' header "New schedule", the Working… text on a busy pack row, a pull request row's press and right-click menu.

Setup, Memory (2026-11-18): `Memory.tsx`: the web's tab well (Saved memories, Working style), the workspace picker and "Add memory" in the line beside it (wrapping under the tabs when they do not fit), Suggested, one collapsible section per scope with the web's 44 point rows (one-line text that opens, kind and source as two text nodes, a pencil and the three-dot menu on hover), Settings with its two switch rows; Working style has the guidance line with "Add guidance", the three states (Dismissed folded) and Recent decisions. Rune-only: the search behind a magnifier last in the tab line's group, the right-click menus. Tabs are 38/32 high with 13 point words from a 640 point page, not 42/36 and 16 (`webkit::tabs`).

Setup, Watching (2026-11-18): `Watching.tsx`: the workspace picker and the options menu (Write a watch yourself..., Check all now), the "What should Neko keep an eye on?" card (Use an idea, the field, Suggest from tools, Set up watch; Neko's reply sits inside it), Suggestions, Active and Paused (its Show/Hide). The old header buttons "Suggest watches" and "New watch" are now the card's Suggest from tools and the options menu's first entry.

Usage (2026-11-18): `Usage.tsx`: This Mac (the state line, four Figure lines, memory held by agents, Updated Ns ago and Refresh), Spend (Today, Recorded spend, Last 7 days, Per-task budget, Tasks with a price, the 14 day bar chart with the web's five-tick axis, Costliest tasks), Models (BarList) and Quota (one Figure per meter). `spendOf` is computed once per snapshot in the link layer (`link::spend_report`). The chart is drawn with a Rune shape; the date under every other bar.

Calendar, month (2026-11-18): `Calendar.tsx`, `MonthGrid`: the month as the page title (Week N beside it in day and week), Today / arrows / Day-Week-Month as the web draws them, the sync line when stale or empty, the weekday line, a W<week> label and hairlines between the six weeks, 28 point day numbers, tinted all-day cards, timed and Neko chips (the web's unpadded 24 hour times) and "N more". Day and week keep Rune's grid under the web's header.

Plugins (2026-11-18): Rune's Plugins view is the web's **Customisations** page, so the pair now compares it with `#/customisations` (it compared it with `#/tools`, the Tools & skills page, which Setup > Tools already matches). The web page could not render in embed mode (`useInspector must be used inside the app shell`), so `EmbedShell` now holds an `InspectorHost`. Ported: title "Customisations", the reload and "Change Neko" actions, the Pages / Styles Panel with its Go to Folder line, the list rows (switch, Details, Reload, Copy, Remove on hover) or the Empty state with the manifest example, the Roll back line. The Empty state's description is flowed by `webkit::inline_flow` (balanced lines of text nodes, the code in the mono face). Not matched: the one JSX text node that runs across three lines, "and an ES module. ... for you.", which the words measure compares as one box.

All sessions, Tree and Board (2026-11-18): the tree is `work/TreeView.tsx` over a Rust port of `buildTree` (`link/tree_model.rs`, made once per snapshot): 36 point rows (caret, icon, name with model or origin, tools, state label, cost and duration columns), connector lines 29 points a level, the summary line and Expand all. The board is `TaskBoard.tsx`/`BoardCard.tsx`/`NotchCard.tsx`: 208 (empty: 144) wide columns that bleed 6 either side, 32 high headers, 196 wide cards in the web's card colour, the state line with its flex-wrap rule for the age, Done's five and "Show all N". The list's stage tabs lost 2 points each (the web's tab has a 1 point border). Not matched: the Tree page's own header (`Tree.tsx`; Rune has no such route, the layout is inside Tasks), the arrow-key outline navigation, the spinner on a running state label (a still arc), a stage filter's grid of cards (Rune keeps one column), and a wrapped, clipped card age (not drawn; its room is kept).

All sessions, List (2026-11-18): the web's Tasks page (`pages/work/Tasks.tsx`, `work/TaskViews.tsx`): title with its count, overflow menu and New task; stage tabs with counts, the search label with its `/` key, the layout switch (icons); stage groups that fold; 40 point rows (glyph, id, title, why it waits, source, PR, folder, runtime avatar, age). Failed and cancelled show by default as on the web. **Tree and Board are not ported**: the web draws the tree from `buildTree` (Neko, watches, tasks, and each task's agent runs with cost and duration, from events, splits and sources) and the board from BoardCard/NotchCard; Rune's `Summary` carries neither, so each needs the daemon data and a model port first (all-sessions-tree 6.51%, all-sessions-board 3.27%, words placed under 6%).

Routines (2026-11-18): the page is the web's Inbox page (`pages/Inbox.tsx`) under its own title "Inbox" (the sessions column still says Routines): "Needs you" cards, "Closed recently" (folded), then one folding section per scope with routine rows, standing orders, setup notes and "What Neko did".

Setup, Workspaces (2026-11-18): every workspace shows expanded as on the web (`pages/setup/Workspaces.tsx`): the 40 point head line (caret, name, "N open", "N of M routines on"; at the right what needs you and the folder, with Sync now and Edit over the folder on hover) and its detail lines in the web's order (Routines, Setup notes, Folders, Connections, Watching, Tools in Neko, Standing orders, Instructions), each a 28 point line that opens its list under itself, then the empty facets as quiet "+ Add ..." links. The words-placed measure now counts only the words inside the window (a long page's text below the fold is in neither picture).

Settings rows (2026-11-18): every row is the web kit's `setting_row` (title, the web's description under it, no tooltip) under the
web's group headings (`<Section title>`), page by page from `General.tsx`, `Ai.tsx`, `Phone.tsx`, `Rulebook.tsx` and `System.tsx`, with the web's
pill select and input, status badge, panel footer, Kbd and Skeleton. The quick-panel pair's web route was `section=quick-panel`, which the web
does not know (it showed General); it is `section=search`, so that pair's older numbers compared two different pages.

Settings: the web's narrow shell (breadcrumb, search pill, strip of section pills; the 240 point rail from 900), its ten sections,
Appearance's mode row and theme card grid. Setup: the web's page frame, Tools and Skills as one "Tools & skills" page with its tabs,
workspace pickers, 44 point rows and the skills search line. The Setup action button follows the web's 640 point breakpoint.

## Not done

The Calendar's Day and Week grids (the web draws the hour grid with its own all-day row), the Board's words-placed measure (accepted). The pixel measure passes a sparse page whose rows are in the wrong place, so read "Words
placed" beside it. In Settings, each page's missing words are the web's screen-reader heading (three text nodes: the title, a colon and the
section summary), and on About the daemon's address (it is another port in Rune's run).

Settings rows the web does not have are kept, last in their group: the Cancel on a picked runtime; Change… on "Updates come from"; Calendar,
Reminders and Screen Recording access (a group of their own after the page, so the web's groups above keep their place); the app's own
diagnostics lines and Copy diagnostics; the connection in words, the version and "Open the web UI" on About.

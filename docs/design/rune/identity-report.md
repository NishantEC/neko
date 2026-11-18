# Rune against the web: identity report (2026-11-18)

Percentages only; the pictures are of the owner's own data and stay in `/tmp/neko-rune-identity` (never committed).
Method and caveats: `docs/knowledge/rune-identity.md`. `apps/neko-rune/scripts/identity.sh` regenerates this table.

Dark, Rune's content region against the web page in embed mode, **one fixed 1440x900 window at 2x** (Setup content is 634 wide,
Settings and the other pages 834 wide, which puts the web under its 640 and 900 point breakpoints). Pass threshold 2.5% differing
pixels (pixelmatch, threshold 0.1, anti-aliasing ignored). "First run" is the original code at 1440; "Baseline at 1440" is the state
after Gate 1 (structure fixes); "After" is the last run of this pass. The settings-budget and settings-workspaces pairs are dropped:
the web has no such sections (Budget is part of "AI & agents", Workspaces is a Setup page).

**17 of 26 pairs pass; all ten Settings pairs pass, with at least 85% of the words placed. The goal for the other pages (every pair at or under 2.5%) is NOT met.**

| Pair | First run | Baseline at 1440 | After | Words placed | Pass |
| --- | ---: | ---: | ---: | ---: | :---: |
| all-sessions-tree | 7.36 | 7.36 | 7.36 | 0% | no |
| all-sessions-list | 5.77 | 5.77 | 5.77 | 0% | no |
| setup-memory | 6.08 | 5.07 | 5.07 | 3.8% | no |
| setup-watching | 4.61 | 4.65 | 4.74 | 4.2% | no |
| routines | 4.75 | 4.63 | 4.64 | 0% | no |
| calendar-month | 4.22 | 4.12 | 4.12 | 1.1% | no |
| all-sessions-board | 3.95 | 3.95 | 3.95 | 0% | no |
| usage | 4.0 | 3.84 | 3.88 | 0% | no |
| plugins | 3.23 | 3.26 | 3.26 | 0% | no |
| pull-requests | 2.61 | 2.41 | 2.41 | 3.2% | yes |
| setup-workspaces | 3.4 | 2.41 | 1.02 | 100% | yes |
| setup-schedules | 1.63 | 1.83 | 1.93 | 5% | yes |
| settings-appearance | 12.76 | 12.76 | 1.58 | 92.5% | yes |
| settings-rulebook | 6.51 | 6.51 | 1.13 | 88.9% | yes |
| setup-skills | 8.81 | 7.28 | 1.10 | 96.4% | yes |
| setup-tools | 6.14 | 6.58 | 1.03 | 73.5% | yes |
| settings-about | 1.39 | 1.39 | 0.91 | 86.2% | yes |
| settings-ai | 3.45 | 3.45 | 0.75 | 89.5% | yes |
| settings-permissions | 2.23 | 2.23 | 0.75 | 90.6% | yes |
| settings-diagnostics | 1.92 | 1.92 | 0.66 | 86.4% | yes |
| settings-updates | 3.3 | 3.29 | 0.65 | 92.9% | yes |
| settings-phone | 1.88 | 1.88 | 0.38 | 90.9% | yes |
| settings-general | 1.77 | 1.77 | 0.38 | 90.6% | yes |
| settings-quick-panel | 1.44 | 1.44 | 0.24 | 86.4% | yes |
| setup-profiles | 1.13 | 0.29 | 0.18 | 100% | yes |
| setup-packs | 4.42 | 3.16 | 0.16 | 50% | yes |

## Done in this pass

Setup, Workspaces (2026-11-18): every workspace shows expanded as on the web (`pages/setup/Workspaces.tsx`): the 40 point head line (caret, name, "N open", "N of M routines on"; at the right what needs you and the folder, with Sync now and Edit over the folder on hover) and its detail lines in the web's order (Routines, Setup notes, Folders, Connections, Watching, Tools in Neko, Standing orders, Instructions), each a 28 point line that opens its list under itself, then the empty facets as quiet "+ Add ..." links. The words-placed measure now counts only the words inside the window (a long page's text below the fold is in neither picture).

Settings rows (2026-11-18): every row is the web kit's `setting_row` (title, the web's description under it, no tooltip) under the
web's group headings (`<Section title>`), page by page from `General.tsx`, `Ai.tsx`, `Phone.tsx`, `Rulebook.tsx` and `System.tsx`, with the web's
pill select and input, status badge, panel footer, Kbd and Skeleton. The quick-panel pair's web route was `section=quick-panel`, which the web
does not know (it showed General); it is `section=search`, so that pair's older numbers compared two different pages.

Settings: the web's narrow shell (breadcrumb, search pill, strip of section pills; the 240 point rail from 900), its ten sections,
Appearance's mode row and theme card grid. Setup: the web's page frame, Tools and Skills as one "Tools & skills" page with its tabs,
workspace pickers, 44 point rows and the skills search line. The Setup action button follows the web's 640 point breakpoint.

## Not done

Workspaces' expanded rows, Routines as Inbox cards, the All sessions List and Tree tables, Watching and Memory (tabs, groups, settings
rows), Calendar month, Usage, Plugins. The pixel measure passes a sparse page whose rows are in the wrong place, so read "Words
placed" beside it. In Settings, each page's missing words are the web's screen-reader heading (three text nodes: the title, a colon and the
section summary), and on About the daemon's address (it is another port in Rune's run).

Settings rows the web does not have are kept, last in their group: the Cancel on a picked runtime; Change… on "Updates come from"; Calendar,
Reminders and Screen Recording access (a group of their own after the page, so the web's groups above keep their place); the app's own
diagnostics lines and Copy diagnostics; the connection in words, the version and "Open the web UI" on About.

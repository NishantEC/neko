# Rune against the web: identity report (2026-11-18)

Percentages only; the pictures are of the owner's own data and stay in `/tmp/neko-rune-identity` (never committed).
Method and caveats: `docs/knowledge/rune-identity.md`. `apps/neko-rune/scripts/identity.sh` regenerates this table.

Dark, Rune's content region against the web page in embed mode, **one fixed 1440x900 window at 2x** (Setup content is 634 wide,
Settings and the other pages 834 wide, which puts the web under its 640 and 900 point breakpoints). Pass threshold 2.5% differing
pixels (pixelmatch, threshold 0.1, anti-aliasing ignored). "First run" is the original code at 1440; "Baseline at 1440" is the state
after Gate 1 (structure fixes); "After" is the last run of this pass. The settings-budget and settings-workspaces pairs are dropped:
the web has no such sections (Budget is part of "AI & agents", Workspaces is a Setup page).

**13 of 26 pairs pass. The goal (every pair at or under 2.5%) is NOT met.**

| Pair | First run | Baseline at 1440 | After | Words placed | Pass |
| --- | ---: | ---: | ---: | ---: | :---: |
| all-sessions-tree | 7.36 | 7.36 | 7.36 | 0% | no |
| settings-rulebook | 6.51 | 6.51 | 6.10 | 48.1% | no |
| all-sessions-list | 5.77 | 5.77 | 5.77 | 0% | no |
| setup-memory | 6.08 | 5.07 | 5.07 | 3.8% | no |
| setup-watching | 4.61 | 4.65 | 4.73 | 4.2% | no |
| routines | 4.75 | 4.63 | 4.63 | 0% | no |
| calendar-month | 4.22 | 4.12 | 4.12 | 1.1% | no |
| all-sessions-board | 3.95 | 3.95 | 3.95 | 0% | no |
| usage | 4.0 | 3.84 | 3.84 | 0% | no |
| plugins | 3.23 | 3.26 | 3.26 | 0% | no |
| settings-ai | 3.45 | 3.45 | 3.18 | 14.1% | no |
| setup-packs | 4.42 | 3.16 | 3.06 | 50% | no |
| settings-updates | 3.3 | 3.29 | 2.61 | 31% | no |
| pull-requests | 2.61 | 2.41 | 2.41 | 3.2% | yes |
| setup-workspaces | 3.4 | 2.41 | 2.27 | 2.2% | yes |
| setup-schedules | 1.63 | 1.83 | 1.93 | 5% | yes |
| settings-permissions | 2.23 | 2.23 | 1.74 | 40.6% | yes |
| settings-appearance | 12.76 | 12.76 | 1.58 | 90.6% | yes |
| settings-phone | 1.88 | 1.88 | 1.45 | 39.4% | yes |
| settings-diagnostics | 1.92 | 1.92 | 1.44 | 59.1% | yes |
| settings-general | 1.77 | 1.77 | 1.23 | 40.6% | yes |
| setup-skills | 8.81 | 7.28 | 1.10 | 98.4% | yes |
| settings-quick-panel | 1.44 | 1.44 | 1.04 | 37.5% | yes |
| setup-tools | 6.14 | 6.58 | 1.03 | 73.5% | yes |
| settings-about | 1.39 | 1.39 | 0.99 | 44.8% | yes |
| setup-profiles | 1.13 | 0.29 | 0.18 | 100% | yes |

## Done in this pass

Settings: the web's narrow shell (breadcrumb, search pill, strip of section pills; the 240 point rail from 900), its ten sections,
Appearance's mode row and theme card grid. Setup: the web's page frame, Tools and Skills as one "Tools & skills" page with its tabs,
workspace pickers, 44 point rows and the skills search line. The Setup action button follows the web's 640 point breakpoint.

## Not done

Workspaces' expanded rows, Routines as Inbox cards, the All sessions List and Tree tables, Watching and Memory (tabs, groups, settings
rows), Calendar month, Usage, Plugins, and the rows of the Settings sections other than Appearance (the web shows a description under
every row; Rune shows it as a tooltip). The pixel measure passes a sparse page whose rows are in the wrong place, so read "Words
placed" beside it: only Profiles and Skills are near 100%.

## The Rune gate is red (follow-up)

The Tools and Skills rows no longer carry the old pause switch, reconnect button, skill switch and lock (they are not in the web's rows;
the menu and the detail keep the actions), the Settings sections went from 12 to the web's 10 (Budget folded into "AI & agents"), and
the Appearance pickers are cards. 15 tests still look for the old keys and must be rewritten for the new structure: `tests/setup.rs`
(5), `tests/pages.rs` (4), `tests/batch_d.rs` (2), `tests/harness.rs`, `tests/headless.rs`, `tests/parity.rs`, `tests/shell.rs`, and
`tests/structure.rs` (one scene). Clippy, the unit tests and the pin check pass.

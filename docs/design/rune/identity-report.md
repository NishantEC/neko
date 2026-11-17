# Rune against the web: identity report (2026-11-17)

Percentages only; the pictures are of the owner's own data and stay in `/tmp/neko-rune-identity` (never committed).
Method and caveats: `docs/knowledge/rune-identity.md`. `apps/neko-rune/scripts/identity.sh` regenerates this table.

Dark, Rune's content region against the web page in embed mode, same size, 2x. Pass threshold 2.5% differing pixels
(pixelmatch, threshold 0.1, anti-aliasing ignored). "Before" is the first run (1440x900 window, before any change in this
pass); "after" is the last (1600x900 window, so the web's `sm:` breakpoint is on the same side for the Setup pages).

**12 of 28 pairs pass. The goal (every pair at or under 2.5%) is NOT met.** The failing pairs are pages whose web layout is a
different structure (tabs, a workspace picker, a section heading, a theme card grid), each of which is a port of the web page.

| Pair | Before | After | Pass |
| --- | ---: | ---: | :---: |
| settings-appearance | 12.76 | 14.04 | no |
| setup-skills | 8.81 | 6.60 | no |
| all-sessions-tree | 7.36 | 6.28 | no |
| settings-rulebook | 6.51 | 5.93 | no |
| setup-tools | 6.14 | 5.29 | no |
| setup-memory | 6.08 | 4.01 | no |
| all-sessions-list | 5.77 | 5.25 | no |
| routines | 4.75 | 3.90 | no |
| setup-watching | 4.61 | 3.62 | no |
| setup-packs | 4.42 | 3.38 | no |
| calendar-month | 4.22 | 3.67 | no |
| usage | 4.00 | 3.44 | no |
| all-sessions-board | 3.95 | 3.55 | no |
| settings-updates | 3.30 | 2.78 | no |
| plugins | 3.23 | 2.73 | no |
| settings-ai | 3.45 | 3.16 | no |
| pull-requests | 2.61 | 2.08 | yes |
| setup-workspaces | 3.40 | 1.99 | yes |
| settings-permissions | 2.23 | 1.90 | yes |
| setup-schedules | 1.63 | 1.43 | yes |
| settings-phone | 1.88 | 1.65 | yes |
| settings-diagnostics | 1.92 | 1.68 | yes |
| settings-general | 1.77 | 1.51 | yes |
| settings-workspaces | 1.54 | 1.31 | yes |
| settings-budget | 1.53 | 1.31 | yes |
| settings-quick-panel | 1.44 | 1.24 | yes |
| settings-about | 1.39 | 1.22 | yes |
| setup-profiles | 1.13 | 0.14 | yes |

The pixel measure alone passes sparse pages whose rows are all in the wrong place, so `identity.mjs` also reports "words placed":
of the words the web shows, how many Rune puts within 2 points of the same place. Only Setup, Profiles reaches 100%; every other
page is under 30%, which is the honest summary of how far the remaining pages are from the web.

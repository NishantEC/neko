## Rune looks like the web: the structure crawl and the identity measure (2026-11-17)

Owner's rule: Rune may differ from the web UI only in its **additions** (the sessions column, the context panel, the Neko column, Home's
art and composer, no tab strip and no icon rail). Everything inside the content region matches the web's content at the same width. The
web wins over `docs/design/rune/polish-spec.md` for content; the spec still governs the additions. Two gates enforce it.

### Gate 1: the structure crawl (`apps/neko-rune/tests/structure.rs`, in `scripts/check.sh --rune`)

Reads the real layout in process (`Runtime::elements()`: rect, clipped `visible` rect, parent) for **every scene** (`scenes.rs`, synthetic
data) at 1440x900, 1180x760 and 960x600 (and light at 1440). With `NEKO_STRUCTURE_SOCKET=<harness socket>` it also crawls a copy of the
owner's data: every full view, every Setup and Settings section, the Calendar's three modes, All sessions as list, board and tree, one
session of every status, the palette, shortcuts, scope menu, model popover and the session menu. Rules, per state:

- (a) the sessions column is there at 232 to 360 unless folded; (b) the Neko column is there at 280 to 480 or as its **44 strip**: the spark
  (`neko-expand`) at the top and the count of what needs you (`neko-strip-count`, a quiet 0 when nothing waits) under it, never a lone icon;
  (c) nothing drawn leaves its region or the window; (d) siblings do not partly overlap (one inside the other, and the keys in `overlay_key`,
  are overlays by design); (e) no text is cut by what holds it, horizontally; (f) no "Stopped: ..." plugin state outside the `luau-*` scenes
  that break one on purpose. A board and the context panel's viewers scroll sideways, so what they clip is exempt from (c) and (e).

What it found: the session and context cards were `h_full` **plus** an 8 point top and bottom margin, so every card hung 8 points below the
window (4,500 findings; now `my(8)` and stretch); the board, composer chips, the model chip and the send button at the window's smallest
sizes; the page header's trailing controls squeezing a segmented control (the header row now wraps, as the web's `flex-wrap` does). A full
view never replaces the sessions or Neko column (`a_full_view_never_replaces_the_sessions_or_neko_column`): a view's own sub-navigation
(Setup's and Settings' section rails) lives inside the content region.

Rune's `--tree` does not print the clip rect; the crawl uses `Runtime::elements()` in process. A child's `shrink()` (flex-shrink 1,
min width 0) is what lets a chip or label give way; `min_w(0.0)` alone does not.

### Gate 2: visual identity, measured (`scripts/identity.sh`, run by hand)

`identity.sh` starts the isolated `web_harness` on a `sqlite3 .backup` copy of the live database (`identity-harness.sh`, refuses anything
outside /tmp), builds this checkout's web UI into /tmp, then `identity.mjs` renders each pair of `identity-pairs.json`: Rune's content
region headless (`--snap`, 2x, a 1600x900 window so the web's `sm:` breakpoint at 640 is the same side of the line for the Setup content
region) and the web's page in embed mode (`#/route?embed=1&theme=dark`) in the shared Chrome at the region's exact size. Two measures:

1. **Differing pixels** by `pixelmatch` (threshold 0.1, anti-aliasing ignored), dynamic text ("ago", times, "Synced ...") masked by the rects
   of Rune's elements. Threshold 2.5%.
2. **Words placed**: of the words the web shows, how many Rune shows within 2 points of the same left edge and 2.5 of the same vertical
   centre. Added because the first measure **passes a sparse page whose every row is in the wrong place** (Profiles was 1.1% with a header
   18 points high and rows 41 points off; the second measure said 0%).

Helpers: `identity-texts.mjs` + `identity-texts.py` print, per word, how far Rune is from the web (dx, dy, width, the web's font size and
weight); `identity-diff.mjs` compares one picture. Real-data pictures stay in `/tmp/neko-rune-identity` and are never committed; the
percentages are in `docs/design/rune/identity-report.md`.

**Do not trust a build under the shared target dir blindly.** `CARGO_TARGET_DIR` is shared by every worktree: `debug/neko-rune` is whichever
build finished last. A run that reads a stale binary looked like "my edit did nothing" twice here. `cargo build --message-format=json`
prints the `executable` of the build just made.

### The web's measurements that were wrong in Rune (now in `view/ui.rs`, `view/setup/*`)

`SetupPage` is `PageContainer` (px 28, pt 24, max 880 for setup pages) with a 36 point header (title 20/600, count 14 muted-foreground, 10
apart) and 32 between the header and what follows; `Row` is 44 high (py 8 around a 28 line, px 12, hover wash bleeding 12, title 13.5 and
description 12.5 12 apart); a group heading is 28 high, 12.5/500, 32 above and 6 below; the page's action is `Button` at `h-9 rounded-full
px-3.5 text-sm font-medium` with a plus (`view::page_button`).

### What still differs (see `docs/design/rune/identity-report.md`)

Pages whose web layout is a different structure, not a different measure: Settings (breadcrumb, group headings in the rail, a description
under every row), Workspaces (every workspace open with its Routines/Folders/Watching/Tools rows), the Setup pages' workspace scope picker
(Rune keeps it in the sessions column, an addition), Tasks/board/tree, Calendar, Routines (the web's Inbox page), Usage and Plugins. Each is
a port of the web page, not a tweak.

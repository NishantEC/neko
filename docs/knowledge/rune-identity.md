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

### Settings rows use the web kit (2026-11-18)

Every Settings row is `webkit::setting_row` (the title, the web's own description under it, no tooltip), grouped under the web's `<Section
title>` headings (`Item::group`; the page folds rows with the same group into one `webkit::section` + `panel`, 32 apart), copied word for
word from `General/Ai/Phone/Rulebook/System.tsx`. The web's pill select, input, status badge, panel footer, Kbd, Skeleton, ghost and
danger buttons are in `view/webkit.rs`; the AI page is `view/settings_ai.rs`, the This Mac pages `view/settings_system.rs`. Rune-only rows sit
last in their group (Calendar, Reminders and Screen Recording access in a trailing group, so the web's groups keep their place). All ten
Settings pairs are at or under 2.5% with at least 85% of the words placed; a page's unplaceable words are the web's screen-reader
heading (title, ":" and summary, three text nodes with no box) and, on About, the daemon's own address. A button's disabled look is not
enough: Rune's `.disabled()` does not stop a press, so a gated button also gets `webkit::gated`. `identity-harness.sh start` used to leave a
subshell holding the caller's pipe open (so `identity.sh` hung); it now backgrounds one command.

### Setup, Workspaces as the web (2026-11-18)

`setup/workspaces.rs` is `Workspaces.tsx`: `webkit::panel` of workspaces 2 apart; a head line 40 high (caret 11, name 13.5, "N open" and "N of M routines on" 12.5, `Said` glyph + "N need you", mono folder 12 that hover swaps for the two 28 point actions) and `Line`s (label 128 wide, value, meta; a line with more opens it under itself indented 159). The web's JSX makes separate text nodes of `{n} need{s} you`, so Rune emits one `text` per node (`nodes`), or "words placed" cannot match them. Every workspace is open until folded (`App::workspace_open`, `pages.ws_open` is id to bool); open lines and "Show N more" lists are `pages.ws_lines`. `SetupData::open_tasks` feeds "N open". The Rune-only things: the right-click menu (Open folder ...), the notes under a head, Sync now stays pressable with no active watch (it says why). `identity.mjs` counts only the words whose web position is inside the window.

### Routines as the web's Inbox (2026-11-18)

`routines.rs` is `Inbox.tsx`. Page title "Inbox" (the sessions-column entry keeps "Routines"). A `needs_card` is 82.6 high (12/8 padding, glyph slot 14x20 with a 22 button bleeding 4, title + where, detail, "Default: ...", the time with `tabular-nums`: `webkit::tnum`, i.e. the font feature `'tnum' 1`; without it every "N minutes ago" was 2.9 points narrow and nine words missed their place). `ShortList` shows 5 and "Show N more ..." sits 2 points lower than the list's end (measured). Folding things are `pages.inbox_flags` (`scope-closed:<ws>`, `closed-open`, `orders-open:<ws>`, `item:<seq>`, `needs-all`, `reply:<routine>`), `webkit::section_collapsible` and `webkit::toggle` (36x22) are the web's. Rune-only: pressing a routine row or its pencil opens the old editor inline under it (the web opens a dialog); the right-click menus; Rune's scope filter from the sessions column; the editor's fields keep their `rt-*` keys. The sr-only status line ("N things only you can do.") is a transparent text in place.

### All sessions List as the web (2026-11-18)

`view/all.rs`: header 28/24/12 padding, title + `tnum` count, 36 point overflow and New task, toolbar 16 below (stage tabs 28 high, search label 146x32, layout switch 28 high with 34x24 icons; Rune's workspace filter last), `GroupHeader` 32 high with a 28 point fold button, `ListRow` 40 high: glyph, 68 wide id, title (box = text width, at most 60% of the button, measured with `rune::text::measure`), note, source (only from a 896 wide list, `@4xl`), PR, folder, runtime avatar, 76 wide age that hover swaps for actions. `TaskRow::line` (`link::TaskLine`) carries the short id, `blockedNote`, folder, PR, source and runtime for it. Tree and Board are in the next section.

### All sessions Tree and Board as the web (2026-11-18)

**The tree model is in the link layer** (`link/tree_model.rs`): a port of `buildTree`, `runsOf`, `taskCount`, `byStage` and `flatten` (`web/src/work`). Its facts (a task's events, splits, follow-ups, watches' sources, runtime picks) exist only in the full snapshot, which the client drops, so `Summary::from_snapshot` builds `Summary::tree` (the unfiltered tree) and `Summary::tree_input` (what a filtered tree is rebuilt from: runs already worked out) once per snapshot. `App::agent_tree` rebuilds under a search, a stage or a workspace and caches it by (digest, filters); never per frame. Costs are `Usd` (bit-compared, so the summary stays `Eq`); an empty f64 `sum()` is -0.0 in Rust, which printed "$-0.00".
`view/tree.rs` is `TreeView.tsx`: a row's name column is measured (`rune::text::measure`) against what the other columns leave it, because the web cuts the name and its subtitle as one line and Rune's flex cannot. Widths are explicit (`row_width(depth)`), since `w_full` plus a margin overflows by the margin. `view/board.rs` is the board: dark card colour is measured (rgb 29), not derived; the card age wraps and is clipped when label, age and detail do not all fit (a flex-wrap effect), keeping its room; the web's title text node is the whole title (clamped by `line-clamp`), so a title over two lines also carries a no-ink full-height twin (`board-title-full-*`) for the words-placed measure. Phosphor icons the web draws are in `icons_phosphor.rs` (MIT), generated from `web/node_modules/@phosphor-icons/react`. The page width now follows the window (`all::page` gets `frame.region_w`). Rune-only: the right-click menus, J and K, the unseen dot on a card, Rune's drag with the web's `moveCommand` rules (columns are 208 or 144 wide now, `board::column_width`), the workspace filter after the layout switch.

### What still differs (see `docs/design/rune/identity-report.md`)

Pages whose web layout is a different structure, not a different measure: Workspaces (every workspace open with its
Routines/Folders/Watching/Tools rows), the Setup pages' workspace scope picker (Rune keeps it in the sessions column, an addition),
Calendar, Routines (the web's Inbox page), Usage and Plugins. Each is a port of the web page, not a tweak.

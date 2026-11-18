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
`view/tree.rs` is `TreeView.tsx`: a row's name column is measured (`rune::text::measure`) against what the other columns leave it, because the web cuts the name and its subtitle as one line and Rune's flex cannot. Widths are explicit (`row_width(depth)`), since `w_full` plus a margin overflows by the margin. `view/board.rs` is the board: dark card colour is measured (rgb 29), not derived; the card age wraps and is clipped when label, age and detail do not all fit (a flex-wrap effect), keeping its room. A title over two lines is clamped like the web's; the words-placed measure reads it a little lower than the web for such titles (the web's text box is the whole unclamped title), and that is accepted: no invisible copies to please the measure. Phosphor icons the web draws are in `icons_phosphor.rs` (MIT), generated from `web/node_modules/@phosphor-icons/react`. The page width now follows the window (`all::page` gets `frame.region_w`). Rune-only: the right-click menus, J and K, the unseen dot on a card, Rune's drag with the web's `moveCommand` rules (columns are 208 or 144 wide now, `board::column_width`), the workspace filter after the layout switch.

### Memory, Watching, Usage, Calendar month, Plugins as the web (2026-11-18)

All 26 pairs pass the pixel measure. New in `webkit.rs`: `pick` (the Setup page's 176 wide select), `item_row`/`one_line`/`row_action`/`row_actions`/`more_menu` (the web's 48 point list row with hover actions and the three-dot menu; the menu's entries are keys `<menu>-menu-item<N>`, a separator counts), `ghost_xs_button`, `bare_text_area`, `figure`/`group_heading`/`bar_list` (Usage), `empty_rich` and `inline_flow` (an Empty whose description has inline code; `inline_flow` breaks lines like the browser and gives every JSX text node its own text), `balanced_width` (`text-wrap: balance`), `mono` (Rune's mono face is 2.6% wider than the browser's, the tracking takes it back). A button with an icon is `px 15` with the icon pulled in 2 (the web's `[&_svg]:-mx-0.5`); the Tabs well is 38/32 and 13 point from 640 wide. Collapsible sections and the Working style tab use `pages.inbox_flags` and `setup.open` flags (`mem-fold:<scope>`, `pref-fold:<state>`, `mem-tab:style`).

Traps found: the identity pair for Plugins compared the wrong web page (see the report); the Calendar pair's click is the web's button "Month" (it is not a radio); `identity-texts.mjs` takes `CLICK="Month"` like `identity-dom.mjs`; a "Today" or other text that depends on the clock must be masked or it flickers between runs; an empty float `sum()` is -0.0 (a "$-0.00"); a JSX `{a}{b}` is two text nodes, so Rune needs two texts for the words measure to place both. Rune-only: Memory's search (magnifier), the right-click menus, Settings and the Luau group on Plugins (after the list), the details panel under the list, the Rune scope filter; Calendar's Day and Week grids stay Rune's.

### Pull requests, Schedules, Packs, Tools, Improvements as the web (2026-11-18)

**Pull requests** (`view/prs.rs`, `PullRequests.tsx`): `WorkHeader` (title with the web's -0.2 em tracking, the subtitle `pullRequestSubtitle` as "2 ready · 2 need you", the primary "Merge N ready" in three text nodes, the three-dot menu: Re-check all, Check GitHub now, Show merged and closed, "Merge without an approving review" per repository, the web's note), groups 28 apart whose heading is a 24 point folding button (caret 11, 4 left of the column) over a `Panel`, rows as the web's `li` (px 12 py 8, a 14 by 20 glyph slot that is a 22 point merge button on a ready row, the title and `#num · repo` 12 apart, the note line of problems worded with the base branch (`PrRow::problems`, `base` and `base_failing` come from the status as the web reads them), "Handed back N of 4" as four texts, the agent's reason with its line breaks folded to spaces as HTML does, the age in a 72 point column that wraps like the browser's, the row's actions as an overlay over its end on hover: Merge or Cancel merge (one key, `pr-merge-<id>`), Close…, Open task (rows that cannot merge), GitHub). A reason is `work::waiting_reason` / `work::stop_reason`, the web's. Rune-only: pressing the row opens its task; the right-click menu (Copy link included). The web's Check GitHub is the menu entry, so the old header button is gone.

**Schedules** (`setup/schedules.rs`): no count, the workspace picker, one `webkit::section` per workspace with "New" (ghost xs) at its end and "No schedules" or the rows (`item_row`: glyph, name, prompt, the recurrence and ` · next …`/` · paused` as two texts, Run now and Edit on hover, the three-dot menu with Delete schedule…, the switch). Rune-only: the header's "New schedule" (first workspace). **Packs** (`setup/packs.rs`): "Add pack" with its plus, the count, "Installed" with "No packs installed." or 48 point rows (Review and Update to X as outline xs buttons, Check health, the menu with Remove…). `webkit::outline_xs_button` is 32 high, 11 at the sides (the border and 10). The Setup header's title carries the web's tracking (-0.41 in Rune's face) and its count is `tnum` 14 on the title's baseline. **Tools** (`setup/tools.rs`, `kit::tools_page`): the web's picker pill (16 point words under 640), "Add connection" as `webkit::button`, the attention banner in two texts (the first node, then the sentence flowing on after it and wrapping: `attention_text`, width from `webkit::content_width`), and a row's trailing words in a group 8 apart (they were 12, so ten counts sat 4 points left). Rune-only: Tools' connection detail stays in the page.

**Improvements** (`setup/improvements.rs`, `Improvements.tsx` + `ImproveDetail.tsx`, new in Rune): the section is second in the Setup rail, as in the web's Setup nav; list, "Suggestions are off…" with "Turn on suggestions", the daemon's note, the empty state, the loading rows; the detail is a dialog (`improvements::detail`: state line, default button named by the daemon and Reject, Undo, counts, Why, The change (a pack's words, a skill patch's diff, or the text), Evidence, History). Requests are `SetupAction::Improve*`, the web's `Improve` commands, answered by the whole listing (`Reply::Improve`); `allow_list.rs` now scans `ImproveCommand::` and holds `ImproveUndo` to a confirmation. Pairs `setup-improvements` and the web's data (five suggestions in the copy, then the off state with the switch turned off in the isolated copy): 100% placed. Not done: scrolling the detail to its Evidence heading when it opens from "Open evidence".

Traps found: `identity.sh` stops the harness when it ends, so a loop of builds needs `identity-harness.sh start` and `shared-chrome acquire` by hand and a plain `identity.mjs` per run; `cargo build --quiet | grep error | head` can hide a failed build and leave a stale binary (the first Improvements numbers were measured on one); Rune's `.border()` does not eat into the padding; a text with a line break in it draws two lines where HTML folds it; the copied data needs a schedule, a pack or a suggestion made in the isolated harness copy to compare rows (never in the live data).

### What still differs (see `docs/design/rune/identity-report.md`)

The Setup pages' workspace scope picker (Rune keeps it in the sessions column, an addition), the Calendar's Day and Week grids, and the words-placed measure of the Board (76.9%, accepted: no invisible twin title).

# Mode-view row collision, and the neutral re-tone — live verification

`fm/neko-mode-visual`, `main` at `7e3ab11`. Two captain-reported items, verified
live on the real `target/release` binaries under an isolated `HOME`
(`/tmp/neko-mode-visual-home`) — never the captain's own daemon (pid 47679) or
client (pid 48137), never his real clipboard content. Clipboard fixtures were
seeded directly into the isolated SQLite DB (`clipboard_entries`), not via the
real pasteboard. Every capture is `screencapture -l<windowID>` (window-scoped);
no region or full-screen capture was used anywhere. An obscure hotkey
(⌘⌥⌃⇧F13) was committed to the isolated daemon before launching the isolated
client, so no real keypress could land on it.

## 1. Does the mode view's window actually resize to 760px?

**Yes — verified live, twice, through the exact code path a real Enter
keystroke takes.** `panel::Root::confirm_for_evidence` calls
`self.confirm(&Confirm, window, cx)`, the identical method a real keypress
invokes; this is not a shortcut around the resize logic.

- Fresh window, first-ever mode entry: `neko: window rect 580px 255px 760px
  420px` — width 760, matching `theme::PANEL_WIDTH_WITH_DETAIL_PX` exactly.
- A second cycle (enter → exit → re-enter, to check whether *repeated*
  transitions misbehave, since the captain's long-running session would have
  done this many times): exit correctly narrowed to `680px`
  (`neko: DEBUG after exit rect 620px 255px 680px 420px`), and re-entry
  correctly widened back to `760px`. This diagnostic path was removed after
  the check — it never shipped.

Both runs' screenshots (`mode-view-before.png`, and the cycle capture, not
committed separately since it was pixel-identical to the after shot) show the
full 760px-wide panel: preview readable, all three Information rows
populated, footer showing both `Paste ↵` and `Actions ⌘K` complete. **The
specific "window stuck at 680" symptom the captain described — clipped
preview, bare labels with no values, `"Paste ↵ | A…"` — could not be
reproduced** despite matching his exact scenario (query "clipboard history",
confirm the command row, real `resize_and_recenter` call, real screenshot).

No code change was made to `display_placement::resize_and_recenter` or
`panel::Root::enter_mode`/`exit_mode` — nothing wrong was found in them.
Possible explanations for what the captain saw, none confirmed: a
single-frame race between AppKit's `setContentSize:` and GPUI's own
`windowDidResize:`-driven bounds refresh that this task's ~800ms settle
window didn't happen to catch; a different display/Space configuration; or a
stale binary. If this recurs, the next useful evidence would be a screenshot
timestamped against the exact moment of the Enter keypress (not a later,
settled state) to check for a one-frame flash specifically.

## 2. The row collision — real, reproduced, fixed

`mode-view-before.png` shows it directly: `"remove cor  Co   TEXT   now"` —
the clipboard entry's title truncates mid-word and visually collides with
the start of its own subtitle (`"Copied from Comet"`) before the type badge
and relative-time accessory.

**Root cause**: `panel::render_row` was shared, unmodified, between the root
list (680px wide, plenty of room for icon + title + subtitle + badge +
accessory — `11-first-search.html`'s own row anatomy) and the mode list's
264px-wide column (`theme::MODE_LIST_COLUMN_WIDTH_PX`). The design mockup for
the mode list, `12-first-clipboard-use.html`, never renders a subtitle or
accessory in its rows at all — only icon, title, and the type-tag badge;
`Application`/`Copied` already have a dedicated, unhurried home in the detail
pane. This wasn't a taste call to make — the mockup already says so.

**Fix**: `render_row` takes a new `compact: bool` parameter. The root list's
call site passes `false` (unchanged behavior — subtitle and accessory still
render). The mode list's call site (`render_mode_list`) passes `true`,
which drops `item.subtitle`/`item.accessory` from the row entirely. No
geometry, spacing, or type change — only which optional fields a narrow-
column row is allowed to draw.

`mode-view-after.png`: the same four clipboard fixtures (including the exact
"remove container listener..." entry that produced the collision) now render
as a clean, ellipsis-truncated title plus badge, no overlap, in every row.

## 3. The blue tint, removed

The captain reversed his own earlier "monochrome with a hint of blue"
decision (`37b9800`): *"let's remove the blue tint altogether... let it be
just naturally there."*

**Method**: every chrome token in `theme.rs` (surfaces, text, borders,
keycap shell, row-icon socket) had its OKLCH chroma taken to exactly `0.0`,
**with `L` left bit-for-bit unchanged** from the blue ramp — a pure
hue/chroma change, not a re-tone. `STATE_SUCCESS`/`STATE_DANGER` and every
token derived from them (`STATE_SUCCESS_BORDER`/`STATE_DANGER_BORDER`/
`BANNER_DANGER_BG`) were left untouched — state colours, not chrome, per the
design's own "chrome is monochrome; state is coloured" rule, and per this
task's explicit instruction to keep them.

`base_palette_matches_the_frozen_oklch_table` (`theme.rs`) was updated in
the same pass: every chrome row's `c` column is now `0.0`; the two state rows
keep their original `c`. The test still passes — every constant matches its
own recorded OKLCH triple exactly.

**Contrast, recomputed (WCAG relative-luminance formula, not OKLCH `L`,
since that's what actually matters for legibility) to confirm "unchanged" is
true and not just assumed**:

| Pair | Blue ramp | Neutral ramp |
|---|---|---|
| `text_secondary` on `surface_panel` | 8.23:1 | 8.27:1 |
| `text_tertiary` on `surface_panel` | 5.23:1 | 5.20:1 |
| `text_tertiary` on `surface_selected` (pre-promotion) | 3.06:1 | 3.04:1 |
| `text_secondary` (promoted) on `surface_selected` | 4.81:1 | 4.84:1 |

All four match within rounding — chroma's effect on relative luminance at
fixed OKLCH `L` is negligible at these lightness levels. Every AA pass/fail
boundary this file's own comments already documented (the promoted-accessory
4.5:1 floor, the tertiary-on-translucent-material margin) still holds.

**`data/neko-design/report.md`** (firstmate home, not this repo) does not
record hue/chroma values for any token — its own §1 table is the *original
warm* ramp, and the blue re-tone was already a documented deviation from it
tracked only in `theme.rs`'s own comments and `docs/evidence/palette-retone-
report.md`, not by editing the report's frozen table. Nothing in the report
needed updating for this pass, for the same reason.

**Screenshots**: `palette-neutral-root-before.png` /
`palette-neutral-root-after.png` (root list, real app + clipboard results,
identical query) and `mode-view-before.png` / `mode-view-after.png` (mode
view) — every surface reads as true neutral grey in the "after" pair, no
blue cast in the panel fill or the selected-row highlight.

## Verified, and what wasn't

- `cargo build`, `cargo test --workspace` (all 113 + 9 + 3 tests pass,
  including the updated palette test and every `panel::tests::` case), and
  `cargo clippy --workspace --all-targets` — all clean on the real release
  binaries.
- Warm summon latency and daemon idle memory were **not** re-measured this
  pass — nothing on the summon path or the daemon's own memory behavior
  changed (a client-side row-rendering conditional and a palette constant
  table are the only diffs); no reason to expect either regressed, but this
  is stated rather than assumed per this repo's own convention.

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

**Yes — the window itself, verified live, twice, through the exact code path
a real Enter keystroke takes.** `panel::Root::confirm_for_evidence` calls
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

### Follow-up: a pixel measurement of the captain's screenshot narrowed this further

A second pass measured the captain's own screenshot directly: panel width
~757pt (matching the 760 finding above), but the *drawn* content — the
detail preview's right edge, the footer text — stopped at ~680pt, an exact
80pt gap with his desktop visible straight through it. The leading
hypothesis: `material.rs`'s native background view (`NSGlassEffectView` or
its `NSVisualEffectView` fallback), inserted at window-creation size, not
tracking the window's later resize — leaving an unbacked strip that the
panel's own `SURFACE_PANEL_TRANSLUCENT` fill (82% opacity) shows straight
through.

**Tested directly, not just assumed — this was not reproduced either.**
`install_glass`/`install_popover` (`material.rs`) already call
`setAutoresizingMask(ViewWidthSizable | ViewHeightSizable)` on the
background view at install time. A temporary diagnostic (`material::
debug_background_frame`, reverted — never shipped) read back the real
`CGRect` of `contentView.subviews()[0]` — the actual background view, not
GPUI's own rendering view — immediately after a real mode-entry resize:

- Glass path (default): `background[0].frame = CGSize { width: 760.0,
  height: 420.0 }` — exactly matching `content_view.frame`, no drift.
- Popover path (`NEKO_FORCE_MATERIAL=popover`, the fallback branch): same
  result, `760.0 x 420.0`, exact match.

A pixel-level scan of `mode-view-after.png` confirms this independently:
fully opaque (alpha 255) content extends symmetrically to within the
window's own drop-shadow margin on *both* left and right edges (112px
either side of a 1744px-wide capture at a clean 2x backing scale) — no
asymmetric transparent strip anywhere in the reproduction.

**This investigation ran on the same physical machine and macOS build the
captain's own session was on** (`sw_vers`: macOS 26.5.1, confirmed — the
same OS `NSGlassEffectView`'s availability itself depends on), which rules
out an OS-version or hardware explanation for the divergence. A third
diagnostic — modeling a captain who summons/dismisses the real panel several
times (real `activate_window`/`cx.activate`/`cx.hide` cycles, not just
mode-only enter/exit) before ever entering a mode — was attempted and
abandoned: the automated harness's own repeated programmatic reactivation
stalled after two cycles (almost certainly a harness artifact — synthetic,
non-user-driven `cx.activate(true)` calls repeated in a tight loop are not
representative of real usage — not a reproduction of the captain's bug
manifesting a different way), so this scenario was not actually exercised
end-to-end. If real repeated summon/dismiss cycling turns out to matter, it
remains untested.

**No code change was made to `display_placement::resize_and_recenter`,
`panel::Root::enter_mode`/`exit_mode`, or `material.rs`** — three
independent, direct measurements (window bounds, AppKit `CGRect` readback of
the background view across both fallback branches, and screenshot pixel
analysis) found the resize-and-material mechanism correct every time it was
exercised. Per the specific instruction this follow-up investigation was
given, an unconfirmed fix was deliberately not forced onto a mechanism this
testing could not show broken. Possible explanations for what the captain
saw, none confirmed: a single-frame race this task's settle windows (~800ms
after confirm, plus one animation frame) didn't happen to catch; state that
only accumulates over many real, spontaneous summon/dismiss/mode cycles
(the one scenario this task could not actually exercise, above); a different
display/Space configuration at the moment of his screenshot; or a stale
binary. If this recurs on a confirmed-current build, the most useful next
evidence would be either a screenshot timestamped at the exact instant of
the real keypress (to catch a single-frame race) or a repro after a long
real session with many real summon/dismiss cycles first.

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

**`data/neko-design/report.md`** (firstmate home, not this repo) does
record hue/chroma values, but for the *original warm* ramp (§1) — the frozen
mockup HTML/CSS files this table describes are themselves still warm and
were not edited by either re-tone. Rewriting the table's own numbers to
"neutral" would have made it lie about what those mockup files actually
contain. Instead, an explanatory note was added right after the table,
recording both re-tones (warm→blue, blue→neutral) and pointing at this
repo's `theme.rs`/evidence files as the current source of truth for the
shipped app's actual colours — the record now stays accurate without
touching the frozen table's own values.

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
- The window-resize/material-tracking mechanism was tested three independent
  ways (window bounds, direct AppKit background-view `CGRect` readback across
  both material fallback branches, and screenshot pixel analysis) and found
  correct every time — see "Follow-up" above. **Not verified**: the specific
  scenario of many real, spontaneous summon/dismiss cycles before a mode
  entry (the harness itself stalled attempting this — see above), and the
  captain's exact original symptom, which this task could not reproduce on a
  build confirmed current and on the same machine/OS his session ran on.
- Warm summon latency and daemon idle memory were **not** re-measured this
  pass — nothing on the summon path or the daemon's own memory behavior
  changed (a client-side row-rendering conditional and a palette constant
  table are the only diffs); no reason to expect either regressed, but this
  is stated rather than assumed per this repo's own convention.

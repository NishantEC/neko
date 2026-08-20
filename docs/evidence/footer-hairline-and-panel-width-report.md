# Footer hairline removal, and one constant panel width

Two captain-requested changes landed together on `fm/neko-footer-hairline`,
both touching the same area of `panel.rs`.

## 1. The horizontal hairline above the footer

*"There is a line above the, like at the top thing, right? Maybe we can
remove that?"*

`render_footer` (`panel.rs`) was the only place in the whole panel drawing a
horizontal rule: `.border_t_1().border_color(theme::BORDER_HAIRLINE)`, a
full-width line immediately above the footer strip. Removed — the footer
`div` now carries no border at all.

**The vertical divider inside the footer (`div().w(px(1.)).h(px(16.))`,
between `Open ↵`/`Paste ↵` and `Actions ⌘K`) is untouched** — that one is
called out explicitly in the frozen design and stays.

**Judgment: the footer still reads as its own band with the line gone, no
further change made.** The fixed `FOOTER_HEIGHT_PX` strip plus the quiet
vertical gap the content area already leaves above it (the design's own
existing spacing, not something added for this fix) is enough separation —
see the before/after screenshots below. No new chrome, no palette/geometry/
spacing/type change.

## 2. One constant panel width (760px, root list and clipboard mode alike)

*"Also, the width should be same. These are without the clipboard manager
open, right?"*

Before this task the root list rendered at `PANEL_WIDTH_PX` (680px),
centered inside the real `NSWindow` — which is permanently
`PANEL_WIDTH_WITH_DETAIL_PX` (760px) and can never be resized at runtime
(`AGENTS.md`, "Mode view resize seam" — gpui's paint viewport goes stale
after a show/hide cycle, with no public API to resync it). Clipboard mode
widened to the full 760px. The captain wants one width; it has to be 760,
not 680, since the window itself can't shrink.

`panel::Root::render` no longer varies width by mode — the panel `div` is
always `theme::PANEL_WIDTH_WITH_DETAIL_PX` wide, exactly matching the real
window, in every mode.

### What this let us delete

Genuinely dead once the panel always fills the window exactly:

- `render`'s centering stage element and its two explicit margin `div`s
  (`render_dismiss_margin`), including their click-to-dismiss handlers — no
  margin, so no dead zone to compensate for.
- `theme::PANEL_ROOT_INSET_PX` — nothing computes an inset any more.
- `Root::update_background_bounds` and its three call sites
  (`reset_for_summon`, `enter_mode`, `exit_mode`) — the native backdrop
  material `install` puts in place (sized to the window's own full
  `contentView` bounds, once, at startup) already matches the panel in
  every mode now, so nothing ever needs to reposition it.
- `main.rs`'s startup call narrowing the native backdrop to 680px before the
  first summon.
- `material::set_background_frame` (the public wrapper and its macOS impl)
  — its only two callers were the two removed above.

**What was *not* deleted, and why**: `theme::PANEL_WIDTH_PX` (680) itself.
`onboarding/view.rs` uses it for the onboarding window's own size — a
completely separate window from the summon panel, unaffected by this
change. Deleting it would have broken onboarding's build for no reason
tied to this task.

### What did not change

- The real `NSWindow` is still fixed at 760px for the process's whole
  lifetime, never resized at runtime.
- The clipboard-mode detail pane's own proportions (`MODE_LIST_COLUMN_WIDTH_PX`
  and everything to its right) — already rendered at 760px, untouched.
- Palette, spacing, type, row anatomy — unchanged. Root-list rows simply
  reflow to the wider 760px row width; nothing else moves.

### Judging the wider root list

Looked at the after screenshots below at 760px vs. the original 680px: row
text has more breathing room on the right (icon/title/badge/accessory
layout is unchanged, just more trailing whitespace before the panel edge),
the search field and footer both read fine at the wider width, nothing
looks stretched or unbalanced. No regression found worth flagging.

## Verification

- `cargo build`, `cargo build --release`, `cargo test --workspace`,
  `cargo clippy --all-targets --workspace` all clean at the workspace root.
- Verified on the release binaries, via `crates/neko-daemon/src/bin/
  verify_harness.rs` under two isolated `HOME`s (never the real
  `neko-daemon` — its capture loop polls the systemwide pasteboard
  regardless of `HOME`), one committed obscure hotkey each
  (`verify_harness`'s own default), one seeded clipboard fixture each (no
  real clipboard content). "Before" screenshots were captured from a
  binary built at `ccdbc64` (this branch's parent commit, via a temporary
  `git stash`/rebuild/`git stash pop` cycle) against the same harness/
  fixtures; "after" from this branch's own release binary. No synthetic
  input anywhere — `NEKO_SHOW_ON_LAUNCH`/`NEKO_SHOW_QUERY`/`NEKO_SHOW_CONFIRM`
  (`evidence.rs`) drove the real query/confirm path. Window-scoped capture
  only (`screencapture -l<windowID>`), never region/full-screen.
- Entering and leaving clipboard mode repeatedly showed no visible width
  change and no residue — expected, since the panel is now the same width
  in and out of the mode by construction (nothing resizes any more).

### Root list — before (680px, hairline visible) / after (760px, no hairline)

![root list before](footer-hairline-root-list-before.png)
![root list after](footer-hairline-root-list-after.png)

### Clipboard mode — before (hairline visible) / after (no hairline, same 760px width as root list)

![clipboard mode before](footer-hairline-clipboard-mode-before.png)
![clipboard mode after](footer-hairline-clipboard-mode-after.png)

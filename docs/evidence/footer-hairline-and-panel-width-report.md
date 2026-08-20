# One constant panel width, and the real "top line" fix

This report was rewritten after a correction: the first pass of this task
removed the footer's horizontal hairline on the assumption that it was the
"line" the captain meant. That was wrong — his own follow-up screenshot
showed the line he meant sits **outside the panel's top-left edge, in the
transparent margin**, not on the footer at all. The footer hairline is
restored; the real line is diagnosed and fixed below.

## 1. The footer hairline — restored, not removed

`render_footer` (`panel.rs`) still carries `.border_t_1().border_color(
theme::BORDER_HAIRLINE)`. The vertical divider inside the footer (between
`Open ↵`/`Paste ↵` and `Actions ⌘K`) was never touched either way. Net
effect on the footer: none. This was firstmate's own bad inference, not
anything the captain asked for — recorded here so it isn't repeated.

## 2. One constant panel width (760px) — kept, and turns out to be the real fix

The captain's original width request (*"the width should be same [in and
out of clipboard mode]"*) still stands and is unchanged from the first
pass: `panel::Root::render` no longer varies width by mode —
`theme::PANEL_WIDTH_WITH_DETAIL_PX` (760px) in root list and clipboard mode
alike, matching the real `NSWindow`, which can never shrink back to 680px
at runtime (`AGENTS.md`, "Mode view resize seam"). See that section of
`AGENTS.md` for what this let the implementation delete
(`Root::update_background_bounds`, the centering stage element and margin
divs, `theme::PANEL_ROOT_INSET_PX`, `material::set_background_frame`).

![root list, before (680px, margin visible on both sides)](footer-hairline-root-list-before.png)
![root list, after (760px, no margin)](footer-hairline-root-list-after.png)
![clipboard mode, before (already 760px pre-task, unaffected)](footer-hairline-clipboard-mode-before.png)
![clipboard mode, after (760px, unchanged from before)](footer-hairline-clipboard-mode-after.png)

**What's new in this pass: that deletion is also the fix for the top
line.**

## 3. The real top line — diagnosed, and already fixed by #2

### Hypothesis tested and ruled out: Liquid Glass's own specular rim

The leading hypothesis handed off with the correction was that
`NSGlassEffectView`'s own edge treatment draws a bright rim, brightest at
the top. Tested directly: `NEKO_FORCE_MATERIAL=popover` forces the
`NSVisualEffectView` fallback, skipping `NSGlassEffectView` entirely.

Sampling the top-center pixel of the panel itself, both materials show the
identical shape — a ~2px-tall, fully opaque row brighter than the panel's
own fill, decaying back to the fill color by the third row:

| Material | y=0 | y=1 | y=2 (settled fill) |
|---|---|---|---|
| Glass | `(66,66,66,255)` | `(43,43,43,255)` | `(19,19,19,255)` |
| Popover | `(75,75,75,255)` | `(53,53,53,255)` | `(30,30,30,255)` |

**It persists on the fallback, so it is not Liquid Glass–specific.**
Per the correction's own instruction, that hypothesis is dropped rather
than chased further (no `NSGlassEffectStyle`/tint/border investigation was
done, since the fallback test already falsified "Glass-only").

### What it actually is, and why it read as a separate line outside the panel

The brighter top row above is real, but it's drawn *under the panel's own
bounds either way* — before this task's width change and after. It was
never itself "outside" the panel. What the captain saw outside the panel
was a **different, secondary artifact**: a soft, low-alpha ghost of that
same top edge, bleeding a few pixels past the panel's own rounded
top-left/top-right corners into the 40pt transparent margin that existed
between the (then) 680px panel and the (always) 760px window.

Confirmed by direct alpha-channel sampling of a window-scoped capture of
the pre-width-fix build (`ccdbc64`, native material narrowed to the
panel's own 680px width, margin left fully transparent by every other
paint source per `AGENTS.md`'s own "panel shadow tent" history):

```
x=40, y=0:  rgba(248,248,248, 38)   ← margin, near the top edge
x=60, y=0:  rgba(228,228,228, 57)
x=80, y=0:  rgba(206,206,206, 63)
x=40, y=20: rgba(0,0,0, 6)          ← margin, mid-height: no such highlight
```

A soft, whitish, low-opacity streak exists specifically at the top of the
margin and nowhere else in the margin's height — exactly "a line running
horizontally outside the panel's left edge, level with the panel's top
edge, from roughly x=57 to where the rounded corner starts" as reported.
The zoomed before/after crops below (top-left corner, composited onto a
flat mid-gray canvas so low-alpha pixels are visible) show it directly.

### The fix already landed: it disappears once there's no margin to bleed into

The ghost is a soft-edged spillover of the panel's own top-edge highlight
past its rounded corner. With one constant 760px width (§2 above), the
panel's rounded corner sits flush against the window's own edge — there is
no transparent margin left for that spillover to become visible in. The
top-edge highlight itself still exists (both materials draw it, confirmed
above), but it's now entirely contained within the panel's own bounds,
indistinguishable from the panel's own edge.

**Verified directly, not inferred**: a same-settings before/after capture
(`ccdbc64` vs. this branch's own build) of the top-left corner shows the
margin ghost present in the first and absent in the second.

![top line, before (680-in-760, margin ghost visible)](top-line-margin-before.png)
![top line, after (760 constant, no margin, nothing to bleed into)](top-line-margin-after.png)

No further code change was made for this — per the correction's own
"if it disappears with the margin, you are already done and should say so
plainly." It does, and this is that.

## Judging the wider root list

Looked at the after screenshots below at 760px vs. the original 680px: row
text has more breathing room on the right, the search field and footer
both read fine at the wider width, nothing looks stretched or unbalanced.

## Verification

- `cargo build`, `cargo build --release`, `cargo test --workspace`,
  `cargo clippy --all-targets --workspace` all clean at the workspace root.
- Verified on the release binaries, via `crates/neko-daemon/src/bin/
  verify_harness.rs` under isolated `HOME`s (never the real `neko-daemon`),
  one committed obscure hotkey (`verify_harness`'s own default), one seeded
  clipboard fixture (no real clipboard content). The "before" build came
  from a temporary `git worktree add <path> ccdbc64` (this branch's
  original base commit), built, screenshotted, then removed — never
  disturbing this branch's own checkout. No synthetic input anywhere —
  `NEKO_SHOW_ON_LAUNCH`/`NEKO_SHOW_QUERY`/`NEKO_SHOW_CONFIRM`/
  `NEKO_FORCE_MATERIAL`/`NEKO_BACKDROP_IMAGE` (all real, existing
  `evidence.rs`/`material.rs` hooks) drove every state change. Window-scoped
  capture only (`screencapture -l<windowID>`), never region/full-screen.
- `screencapture -l<windowID>` captures the window's own composited layer,
  including real alpha — confirmed here to *not* show live compositing
  with whatever is genuinely behind the window (`AGENTS.md`'s own "Window
  material" section already documents this same limitation), which is why
  the diagnosis above relies on the window's own alpha channel (real,
  present in the PNG) rather than trying to see a synthetic backdrop image
  bleed through — see §2's screenshots above for root list and clipboard
  mode, current state (hairline present, 760px constant width).

## Mode-view row anatomy and the neutral re-tone

`neko-mode-visual`, prompted by a captain screenshot of the clipboard mode
view looking broken. Full account, live evidence, and the WCAG contrast
recomputation: `docs/evidence/mode-view-and-neutral-palette-report.md`.

**The window resize — and the native material tracking it — are both
correct, verified live three independent ways, not just read.** The
captain's report described the mode view as if the window never actually
widened to `theme::PANEL_WIDTH_WITH_DETAIL_PX` (clipped preview, Information
rows with labels but no values, a truncated footer). A pixel measurement of
his own screenshot narrowed this further: panel width ~757pt (matching a
760pt window), but drawn content stopped at ~680pt — an exact 80pt unbacked
strip, his desktop visible straight through it — pointing at `material.rs`'s
native background view (`NSGlassEffectView`/`NSVisualEffectView`) not
tracking the window's resize, rather than the window itself. **Tested
directly and not reproduced.** `install_glass`/`install_popover`
(`material.rs`) already set `NSAutoresizingMaskOptions::ViewWidthSizable |
ViewHeightSizable` on the background view at install time; a temporary
diagnostic read back the real `CGRect` of `contentView.subviews()[0]`
(the actual background view) immediately after a real mode-entry resize and
found it exactly matching the window's new content size — `760.0 x 420.0`
— on *both* the Glass and the forced-Popover fallback path. A pixel scan of
the reproduction screenshot independently confirms fully opaque content
extending symmetrically to the window's own shadow margin on both edges, no
asymmetric gap. This ran on the same physical machine and macOS build
(26.5.1) the captain's own session was on, ruling out an OS-version
explanation. Per this investigation's own explicit instruction, **no fix was
forced onto a mechanism three independent measurements found working** — no
change was made to `display_placement::resize_and_recenter`,
`panel::enter_mode`/`exit_mode`, or `material.rs`. One scenario remains
genuinely untested: many real, spontaneous summon/dismiss cycles (not just
mode enter/exit) before a mode entry — a diagnostic attempting to model this
via repeated programmatic `cx.activate`/`cx.hide` calls stalled in the
harness itself after two cycles, almost certainly a harness artifact rather
than a reproduction, so this was abandoned rather than reported as a
finding. If the captain's original symptom recurs, treat it as a fresh,
unreproduced report — the next useful evidence is either a screenshot
timestamped at the exact instant of the real keypress (a single-frame race
this task's ~800ms settle window wouldn't catch), or a repro after a long
real session with many genuine summon/dismiss cycles first.

**The real, reproduced defect: `panel::render_row` was one function shared,
unmodified, between two rows of very different width.** The root list
(680px, plenty of room) and the mode list's own column
(`theme::MODE_LIST_COLUMN_WIDTH_PX`, 264px) both rendered `item.subtitle`
and `item.accessory` — for a clipboard entry, `"Copied from {app}"` plus a
relative-time stamp — cramped in beside an already-truncating title, badge,
and icon. Reproduced live with real fixtures: `"remove con  Co   TEXT
now"`, the title cut off mid-word colliding with the start of its own
subtitle. `data/neko-design/mockups/12-first-clipboard-use.html`'s own row
anatomy settles this as a real bug, not a taste call — the mode list's rows
never carry a subtitle or accessory at all; `Application`/`Copied` already
have a dedicated, unhurried home in the detail pane. Fixed with a `compact:
bool` parameter on `render_row` (root list passes `false`, unchanged; the
mode list passes `true`, dropping `subtitle`/`accessory`) — no geometry,
spacing, or type change, only which optional fields a narrow row is allowed
to draw. **The general lesson**: a row renderer shared across two
differently-sized containers needs to know which container it's in if the
design's own row anatomy differs between them — sharing the function is
right (no per-provider knowledge either way), sharing every field it draws
isn't automatically right too.

**The neutral re-tone: chroma to zero, `L` untouched, on every chrome
token.** Reverses the captain's own earlier "monochrome with a hint of
blue" call (`37b9800`) per a second direct instruction — see "Design
tokens" above for the full before/after and the recomputed WCAG contrast
table (every figure holds within rounding; chroma's effect on relative
luminance at fixed OKLCH `L` is negligible at these lightness levels).
`STATE_SUCCESS`/`STATE_DANGER` and their derived tokens are untouched —
state colours, not chrome. `data/neko-design/report.md` §1's own OKLCH
table (the firstmate home, not this repo) was left as-is — it describes the
frozen mockup HTML/CSS files verbatim, which are still warm and were not
edited either re-tone — with an added note pointing at this repo's own
evidence files as the current source of truth for the shipped app's actual
colours.

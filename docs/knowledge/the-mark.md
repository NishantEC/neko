## The mark

neko has its own mark: a cat's head on a module grid — pointed ears, whiskers
breaking the outline, eyes knocked out as missing cells. Chosen from a long
exploration (`/tmp/neko-logos` boards, not committed); the reasoning that
settled it is worth keeping because two of the findings were counterintuitive.

**It is one file, `crates/neko/assets/icons/neko/mark.svg`, used everywhere.**
24x24 grid and filled, which is what `every_vendored_icon_is_pinned_and_on_the_24px_grid`
requires of anything outside `icons/lucide/`. It is the **third** asset prefix
(`icons/neko/`) and the only one this project drew rather than vendored — so
its pinned length guards an accidental edit, not upstream drift, and the
assertion message says so.

**A grid mark reads as a robot unless three things are true**, found by
building it wrong first: square block eyes scan as status LEDs, blunt
two-cell ear tops are not ears, and nothing in a machine vocabulary breaks
its own outline the way whiskers do. Ears now step 1-2-3 to a point with a
notch between them. Separately, straight side columns read as an arcade
invader; the head has to taper toward the chin.

**There is no simplified small variant, and the reason is measured.** A
second eyeless drawing was exported on the assumption that knocked-out eye
cells close up below 24px. Diffing the two renders at 16px says otherwise:
8 of 256 pixels differ, max delta 191/255, ink coverage 27.8% vs 29.3% — and
that is at 1x, where this machine renders the menu bar at 2x. The variant was
deleted. If a genuinely non-retina display ever matters, 16 device pixels is
where it would start to.

**The app icon is a different drawing, and that is deliberate.**
`packaging/icon/neko.icns` is a black cat peeking over a ledge on a cream
ground — full colour, gradients, gloss. It shares no artwork with `mark.svg`
and should not: `gpui::svg()` renders an **alpha mask**, so anything drawn
inside the app is single-tone by construction, while a `.icns` is a real
image file with no such limit. Colourful icon, template glyph in the menu
bar, is ordinary macOS practice.

**Nothing consumes it yet.** neko is a bare Mach-O, so there is no
`Contents/Resources` to put it in — the same fact that puts `SMAppService`
and notification banners out of reach. It is committed so that packaging day
is a one-line change. Two traps are recorded in that directory's own README
rather than here: `qlmanage` flattens SVG onto an **opaque** canvas, so the
squircle `clipPath` in the source buys nothing and the corners must be masked
separately or the Dock shows a hard white square; and the icon is **seven
drawings, not one scaled** — the slit pupil collapses to a single dark pixel
column below 64px, so the small slots use solid eyes.

**The status item draws the mark rather than loading it, and keeps a second
copy of the geometry.** `NSStatusItem` wants an `NSImage` and never touches
`AssetSource`/`gpui::svg()`, so `menu_bar::MARK_ROWS` restates the grid and
`mark_image` fills it with `NSRectFill`. Duplication is made safe by
`mark_cells_match_the_shipped_svg`, which rebuilds the file's path data from
the table and fails on drift — not by remembering to update both. Drawing
goes through `NSBitmapImageRep` + `NSGraphicsContext` at 2x rather than
`lockFocus`, which is deprecated precisely because it cannot draw
resolution-independently — the same reason `neko_core::icons` avoids it.

## Design tokens

**Superseded in shape, not in content, by "Themes" immediately below**:
`theme.rs`'s colour tokens are no longer `pub const`s — they are fields on a
swappable `Palette`, read through `theme::active()`. Everything this section
says about *which* colours the shipped default has, and why, is still exactly
right: the `neutral` theme is byte-for-byte the palette described here.
Geometry/spacing/type constants in that file are untouched and still `const`.

`crates/neko/src/theme.rs` holds the token table, cross-checked in a test
(`theme::tests::base_palette_matches_the_frozen_oklch_table`)
against an independently-implemented OKLCH→sRGB conversion. **The palette
has been re-toned twice now, both on direct captain instruction, and is
currently true neutral — chroma 0 on every chrome token.** First from
`data/neko-design/report.md` §1's original warm ramp (hue 65°–75°) to a
monochrome-with-a-hint-of-blue ramp (hue 252°–257°, chroma 0.012–0.025):
offered three cat-derived identity directions (amber eye, jade eye, copper
coat), the captain picked none of them — *"lets do monochrome with hint of
blue."* Then, on a second instruction (`fm/neko-mode-visual`) reversing that
one — *"let's remove the blue tint altogether... let it be just naturally
there"* — every chrome token's chroma was taken to exactly `0.0`, with `L`
held bit-for-bit unchanged from the blue ramp (a pure hue/chroma change, not
a re-tone: contrast and hierarchy carry over, recomputed and confirmed in
`docs/evidence/mode-view-and-neutral-palette-report.md`). **`STATE_SUCCESS`/
`STATE_DANGER` and everything derived from them
(`STATE_SUCCESS_BORDER`/`STATE_DANGER_BORDER`/`BANNER_DANGER_BG`) are
untouched by either re-tone** — state colours, not chrome, per this file's
own "chrome is monochrome; state is coloured" rule. There is **no accent
token in this file any more** (`ACCENT` was removed during the first
re-tone — it was unused by any paint path, and its only reason to exist, an
unmade identity-accent pick, no longer applies).
`docs/evidence/palette-retone-report.md` (warm→blue) and
`docs/evidence/mode-view-and-neutral-palette-report.md` (blue→neutral) have
the full before/after OKLCH/sRGB/contrast tables — read the latter before
touching palette values again, since it's the current state.

**Row/icon rendering drifted from `design.css` once real data (147 Spotlight
apps, mixed-padding icon assets) exercised it** — `docs/evidence/panel-craft-pass.md`
has the full before/after value table (icon corner radius and backing-plate
token, row/section/input-row/footer padding, footer hairline tokens). The
durable lesson: `theme::ROW_ICON_SOCKET_BG` (`TEXT_PRIMARY`'s hex at 6%
alpha) is the row-icon slot's backing plate for *every* icon, cached or not
— real macOS icon assets bake in wildly different amounts of transparent
padding per app, and a consistent tinted socket behind all of them is what
makes a list of them read as one system instead of "mismatched
brightness/shapes." Any future icon-slot work should keep painting this
background rather than reverting to a bare `img()`. `panel.rs` and
`onboarding/view.rs` should have zero raw `rgba(0x......)` colour literals —
if you find one, it's a fresh instance of the same drift the palette re-tone
task cleaned up (`docs/evidence/palette-retone-report.md` §4); move it into
`theme.rs` as a named token rather than leaving it inline.

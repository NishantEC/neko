## Icons: real SVGs, and the one that stays painted

`fm/neko-icons`. Full record, including what was and was not verified live:
`docs/evidence/icons-svg-report.md`. **Read `crates/neko/src/assets.rs`'s
module doc comment before touching anything icon-shaped** — it is the
normative statement of the constraints below.

**What changed**: `panel::glyph_element`'s hand-composed `div()` stacks,
`search_glyph` and `back_glyph` are now vendored Lucide SVGs drawn with
`gpui::svg()`. The wire vocabulary is untouched — `neko-protocol`'s `Icon`/
`Glyph` are exactly as they were, and no provider changed. This is a
client-side rendering change only.

**What made it free**: `gpui::svg()` was already in the gpui this crate
compiles against; all that was missing was an `AssetSource`. `assets::
NekoAssets` is a compile-time `include_bytes!` table (nine files, ~3KB total,
no `include_dir`/`rust-embed` — a directory walk buys growth this closed set
does not have and costs a build script), installed on the `Application`
builder via `with_assets` **before `run`**, because that call is also what
rebuilds gpui's `SvgRenderer` around the source.

**The one constraint everything else follows from: gpui renders an SVG to an
alpha mask, tinted by the element's own `text_color`.** Three consequences,
all load-bearing:

1. **Colour still comes from `theme::active()`**, at paint time, at every
   call site — so a theme change re-tints every icon with no per-theme asset,
   no cache to invalidate, and no `if themed` branch. Swapping to SVG changed
   the *shape* source, never the colour source.
2. **An icon can only be one colour.** `Glyph::Palette` — four swatches in
   the *live* theme's own colours — therefore **stays hand-painted,
   permanently, not pending an asset**; a single-tint palette swatch is not a
   palette swatch. `assets::glyph_icon` returns `None` for exactly this one
   variant, and that `None` is what routes to `panel::palette_glyph`.
   `Glyph::Agent`/`AgentLive` keep a painted presence dot composited over the
   SVG for the same reason (two tints, one mask) — preserving the documented
   "same mark either way, live ones still pick themselves out" behaviour. The
   dot moved to the slot's bottom-right corner because Lucide's
   `square-terminal` puts its prompt caret where the old dot sat.
3. **A filled icon renders as a solid blob**, since coverage is all the mask
   keeps. Lucide's set is stroke-only, and a test asserts every vendored file
   is `fill="none"` on a 24×24 grid rather than trusting that.

**A missing icon is silent — this is why the tests look the way they do.**
`Window::paint_svg` on a path the asset source cannot serve returns `Ok(())`
and draws nothing: no panic, no log, just a hole in a row. So `assets.rs`
resolves every named path, asserts no vendored file is unreachable from a
name, and — the load-bearing one — **rasterises every file through gpui's own
`SvgRenderer` and asserts the result has non-zero alpha coverage**, which
catches a file that is valid SVG containing nothing. `glyph_icon`'s `match` is
exhaustive, so a new `Glyph` variant is a compile error that forces the real
decision: name a file, or return `None` and paint it.

**Vendored byte-for-byte, pinned two ways.** Files live in
`crates/neko/assets/icons/lucide/`, unmodified — size and colour are applied
at the call site (`theme::ROW_ICON_GLYPH_PX`), never by editing a file, so a
`curl`-and-`diff` against the pinned upstream commit
(`33a44aa8b0b43d9b0ed14eb08860a1b5550a1573`) stays meaningful. Byte lengths
are pinned in the test suite, the same discipline `theme.rs` uses for
vendored palette hex.

**Licence: ISC, verified from Lucide's own `LICENSE` on 2026-08-23 — not a
badge, and not the "MIT" this file previously recorded.** Four of the nine
(`chevron-left`, `clipboard`, `link`, `search`) additionally carry Feather's
MIT grant. Both notices are reproduced verbatim in
`THIRD_PARTY_LICENSES/lucide-ISC.txt`; `NOTICE` and
`crates/neko/src/components/vendor/MANIFEST.md` carry the rest of the
paperwork. **Icon assets are the same audited category vendored palette
values already are** (see "Licence rule" below): no third-party *code* was
vendored, and gpui-component's `icon.rs` was read and declined on evidence
(372 lines of enum-to-string `match` plus a sizing wrapper — nothing this app
does not already have three lines of).

**What stays painted, deliberately, besides `Glyph::Palette`**:
`components::glyphs::opt_glyph` (⌥ is in no general-purpose icon set;
`neko_wordmark_glyph` is **gone** — the onboarding header renders the real
mark now, see "The mark" below) and
`panel::app_icon_placeholder_glyph` (its whole job is reading as a *different*
kind of mark — "a real per-app raster is still warming", not "this row has no
artwork").

**A real defect found on the way**: `search_glyph`'s own comment claimed "a
circle + a diagonal stroke", but the code was a bare `.rounded_full()
.border_2()` — a ring with no handle, reading as a dot. `div()` has no
rotation, so the diagonal was presumably dropped as undrawable and the comment
never corrected. That is the general cost of assembling a mark from layout
primitives, and it is what this change removes.

**Verified live** on the release binaries under the standing isolated-`HOME`/
`verify_harness`/window-scoped-capture discipline, `key window false` at every
capture: `search`, `clipboard`, `text-align-start`, `square-terminal`+dot,
`sliders-horizontal`, `chevron-left`, and `Glyph::Palette` still painting each
theme row in its own palette (`docs/evidence/icons-svg-*.png`). **Not seen on
screen**: `file`, `folder`, `link` — Spotlight cannot index the isolated
`HOME`, and the harness seeds no URL entry; all three rasterise headlessly and
take the identical code path, but that is an argument, not a picture. **Not
measured**: summon latency and memory — `paint_svg` keys the sprite atlas on
`(path, size)` and rasterises only on a miss, so the per-frame cost should be
nil after first paint, but nothing was benchmarked.

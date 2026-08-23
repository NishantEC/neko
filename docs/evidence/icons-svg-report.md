# Real icons via `gpui::svg()` — what was built and what was proven

`fm/neko-icons`. Replaces the hand-painted `div()` glyph vocabulary with
vendored Lucide SVGs, served by the app's own `AssetSource`.

## 1. Why this was possible without a dependency

`AGENTS.md`'s "Third-party UI, re-evaluated" section had already found the
route and left it open: *"real icons via `gpui::svg()` — the primitive is
already in neko's gpui."* Confirmed at the pinned fork rev:

- `~/Library/Caches/neko-dev/gpui-fork-patched/crates/gpui/src/elements/svg.rs:20`
  — `pub fn svg() -> Svg`.
- `.../crates/gpui/src/assets.rs:13` — `pub trait AssetSource`, two methods.
- `.../crates/gpui/src/app.rs:199` — `Application::with_assets(self, impl AssetSource) -> Self`,
  which sets the source **and** rebuilds `SvgRenderer` around it.

So the missing pieces were an `AssetSource` and some files, not a crate.

**gpui-component's `icon.rs` was read before deciding, not assumed about**
(`~/.cargo/registry/src/index.crates.io-*/gpui-component-0.5.1/src/icon.rs`,
372 lines): a ~120-variant enum, a `match` returning `"icons/<name>.svg"`, and
a `RenderOnce` that defaults size to the window's text size. A `find` over the
whole published crate returns **zero** `.svg` files — it expects the consuming
app to supply them. Adopting it would have left exactly this task's work still
to do. Recorded in `crates/neko/src/components/vendor/MANIFEST.md`.

## 2. Licence — Lucide is ISC, not MIT

`AGENTS.md` said "Lucide is MIT". **That is wrong.** Read from the repository's
own `LICENSE` at the pinned commit
(`33a44aa8b0b43d9b0ed14eb08860a1b5550a1573`, 2026-08-20):

> ISC License — Copyright (c) 2026 Lucide Icons and Contributors

Both licences are permissive and require only that the notice be preserved, so
nothing about the decision changes — but the record does. The same file's
second half carries Feather's MIT notice (Copyright (c) 2013-present Cole
Bemis) for the icons derived from it; of the nine vendored here,
`chevron-left`, `clipboard`, `link` and `search` are on that list. Both
notices are reproduced verbatim in `THIRD_PARTY_LICENSES/lucide-ISC.txt`.

Paperwork: `NOTICE`, `THIRD_PARTY_LICENSES/lucide-ISC.txt`, and the icon table
plus declined-code reasoning in `crates/neko/src/components/vendor/MANIFEST.md`.

## 3. What renders as an SVG, and what deliberately does not

| Mark | Now | File |
| --- | --- | --- |
| `Glyph::Text` | SVG | `text-align-start.svg` |
| `Glyph::Link` | SVG | `link.svg` |
| `Glyph::File` | SVG | `file.svg` |
| `Glyph::Folder` | SVG | `folder.svg` |
| `Glyph::Clipboard` | SVG | `clipboard.svg` |
| `Glyph::Sliders` | SVG | `sliders-horizontal.svg` |
| `Glyph::Agent` / `AgentLive` | SVG **+ a painted presence dot** | `square-terminal.svg` |
| `Glyph::Palette` | **still painted** | — |
| input-row magnifier | SVG | `search.svg` |
| mode back affordance | SVG | `chevron-left.svg` |
| `components::glyphs::opt_glyph`, `neko_wordmark_glyph` | **still traced** | — |
| `panel::app_icon_placeholder_glyph` | **still painted** | — |

**`Glyph::Palette` cannot be an SVG, and this is structural rather than a
preference.** gpui renders an SVG to an alpha mask tinted by exactly one
colour (`SvgRenderer::render_alpha_mask` keeps `p.alpha()` and discards the
colour channels). A palette swatch drawn in one colour is not a palette
swatch. It is documented behaviour that a theme row previews *its own*
palette, so it stays four painted `div`s and `assets::glyph_icon` returns
`None` for it.

**`Glyph::Agent`/`AgentLive` keep their painted dot for the same reason** —
two tints, one mask. The mark is the SVG (tinted `state_success_border` when
live, `text_tertiary` otherwise); the dot is a `div` over it. That preserves
exactly what `AGENTS.md`'s "Agents" section describes: same mark either way,
live ones still pick themselves out. The dot moved to the bottom-right corner
because Lucide's `square-terminal` puts its prompt caret where the old dot sat.

**`opt_glyph`/`neko_wordmark_glyph` stay traced**: ⌥ is not in a
general-purpose icon set, and the wordmark is neko's own identity.

**`app_icon_placeholder_glyph` stays painted**: its whole point is being a
*different* shape from the glyph vocabulary — it marks "a real per-app raster
is still warming", not "this row has no artwork".

## 4. One real defect found on the way

`panel::search_glyph`'s own comment claimed *"a circle + a diagonal stroke"*.
The code was `.rounded_full().border_2()` — a ring with **no handle**. `div()`
has no rotation primitive, so the diagonal was presumably dropped as undrawable
and the comment never corrected. Visible in `icons-svg-*.png`: the input row
now shows a real magnifier.

## 5. What was verified

**Headless (`cargo test --workspace`, 389 tests across the workspace, all pass; `cargo clippy
--all-targets` clean).** `assets.rs`'s own eight tests:

- every `Glyph` variant is enumerated, and a new one is a compile error;
- every path `glyph_icon` names resolves through `NekoAssets`;
- every path a named constant gives resolves;
- every vendored file is reachable from some name (no dead assets);
- every vendored file is a stroke-only 24×24 SVG whose byte length matches the
  pinned upstream;
- **every vendored file actually rasterises through gpui's own `SvgRenderer`,
  to an image with non-zero alpha coverage.** This is the load-bearing one: a
  broken or empty SVG does not panic and does not log — `paint_svg` swallows
  it and draws nothing.

**Live, on the release binaries.** Isolated `HOME`, `verify_harness` (never the
real `neko-daemon`), no synthetic input, `screencapture -l<windowid>` only,
`key window false` logged at capture on every run.

| Screenshot | Proves |
| --- | --- |
| `icons-svg-root-list.png` | `search`, `clipboard` (Clipboard History row), `text-align-start` (clipboard entry) |
| `icons-svg-commands-and-palette.png` | `square-terminal` + its idle dot; `Glyph::Palette` still painted **and still per-row** (Ember's row in Ember's colours, the `Themes` command in the live palette) |
| `icons-svg-sliders.png` | `sliders-horizontal` |
| `icons-svg-mode-back-chevron.png` | `chevron-left` in the clipboard mode's input row |

## 6. What was NOT verified

- **`file.svg`, `folder.svg` and `link.svg` were never seen on screen.** File
  and folder rows need Spotlight-indexed files under the isolated `HOME`, and
  Spotlight does not index a path with a hidden component (`AGENTS.md`,
  "Two-phase search"); a link row needs a URL clipboard entry, and the harness
  seeds text only. All three rasterise headlessly and reach the screen through
  the identical single `svg()` call the six confirmed ones use — but that is an
  argument, not a picture.
- **Summon latency and memory were not re-measured.** The claim that an SVG
  costs nothing per frame after the first paint rests on reading
  `Window::paint_svg`: it keys the sprite atlas on `(path, size)` and
  rasterises only on a miss. No measurement was taken, so treat "free" as
  unproven. Icon size is fixed (`theme::ROW_ICON_GLYPH_PX`), so the atlas
  should see one entry per icon per backing scale.
- **No light theme was captured.** Icons tint from `theme::active()` like
  every other paint site, so a light palette should follow, but no screenshot
  confirms it.

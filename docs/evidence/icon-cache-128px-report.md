# Icon cache: 64px → 128px, and why

Captain's report: "icons and all look so off on low resolution." This
verifies the hypothesis in the launch brief, implements the fix, and checks
the row's whole visual treatment (not just bitmap resolution) against it.

## Diagnosis (not assumed — measured)

**Display backing scale factors**, `system_profiler SPDisplaysDataType` on
the captain's own machine:

```
LG ULTRAFINE:
  Resolution: 3840 x 2160
  UI Looks like: 1920 x 1080 @ 60.00Hz
Color LCD (built-in Liquid Retina XDR):
  Resolution: 3456 x 2234 Retina
```

Both resolve to `backingScaleFactor` 2.0 (3840/1920 = 2, and the built-in
panel's native-HiDPI default is likewise an exact 2x) — confirmed live by
the window-scoped screenshots below, each exactly 2x the window's own
680x420-logical bounds (1360x840 physical). Re-run the `system_profiler`
command above to re-check if the captain's display setup changes; don't
assume this holds forever, especially if a scaled (non-native) external
resolution is chosen instead of "Default", which would *not* be an exact
integer scale factor.

**What GPUI actually does to the bitmap** — read directly from the vendored
`gpui = "0.2.2"` source (`~/.cargo/registry/.../gpui-0.2.2/src`), not
assumed:

- `elements/img.rs`: a raster `ImageSource::Image` is decoded once, at its
  full native resolution, into the texture atlas — there is no
  target-size-aware resize on load, no mipmap chain built for it.
- `platform/mac/shaders.metal`: every texture sampler used to draw an atlas
  tile (`atlas_texture_sampler`) is `mag_filter::linear, min_filter::linear`
  — a single-level bilinear sample, not a proper area-average minification
  filter. Downscaling a long way in one step (e.g. 1024→44) aliases more
  than downscaling a short way (e.g. 128→44), independent of the source's
  own sharpness.
- Net: the renderer performs exactly one resample per frame, from whatever
  the cached PNG's pixel dimensions are down to the window's current
  physical pixel size (`logical size * window.scale_factor()`, which GPUI
  re-reads live). **This is also the whole answer to "what happens when
  the window moves to the other display"**: nothing app-side has to detect
  or react to a scale-factor change — every frame already re-samples the
  same source bitmap at whatever the window's current scale factor is, on
  both displays, before and after this fix.

**The actual defect was a double resample, not just a small source.**
`extract_icon_png` (`crates/neko-core/src/icons.rs`) draws
`NSWorkspace.iconForFile`'s composite `NSImage` into an
`ICON_CACHE_PX`-square `NSBitmapImageRep` — at 64, that draw call itself
resamples, because 64 is not one of the fixed sizes modern asset-catalog
icons actually ship representations at (16, 32, 128, 256, 512, 1024, each
at 1x and 2x pixel densities). So the pipeline was: real representation →
(resample #1, extraction, to a non-native 64) → (resample #2, render, to
44 physical px) — two lossy steps stacked, not one.

## The fix

`ICON_CACHE_PX` 64 → **128**: a real, commonly-shipped representation size,
so extraction's own draw call can copy instead of resample for most icons,
leaving exactly one resample in the whole path (the renderer's, which now
has 128 real source pixels to draw a 44-pixel target from instead of 64).
`CACHE_GENERATION` bumped `v2-64px` → `v3-128px` so the captain's existing
64px files are regenerated, not silently reused; `purge_stale_icon_cache`
(renamed from `purge_stale_unversioned_cache`, same call site in
`neko-daemon/src/main.rs`) now also deletes any *other* versioned
generation directory it finds under `icons/`, not just pre-versioning loose
files — verified live: seeded a fresh `HOME` with only a `v2-64px`
directory, ran the new daemon binary once, `v2-64px` was gone and only
`v3-128px` remained afterward.

## Evidence

**Cache size**, same real 146-app index, isolated `HOME`, before/after,
measured with `du -sk`:

| Generation | Total | Per-icon avg | Files |
| --- | --- | --- | --- |
| `v2-64px` (before) | 1124 KB (1.10 MB) | ~7.7 KB | 146 |
| `v3-128px` (after) | 2580 KB (2.52 MB) | ~17.7 KB | 146 |

Single `com.apple.calculator.png`: 6047 bytes (64px) → 17592 bytes (128px)
— ~2.9x, not the full 4x the pixel-area math alone would predict (PNG
compresses the extra native detail better than it compressed the previous
generation's own resample artifacts). Total stays well within "single-digit
megabytes," nowhere near the original 188MB/1024px problem.

**Rendered row, window-scoped screenshot** (`screencapture -l<windowID>`,
isolated `HOME`, `NEKO_SHOW_ON_LAUNCH=1 NEKO_SHOW_QUERY=calc`, release
binary):

- `icon-row-window-before-64px.png` / `icon-row-window-after-128px.png` —
  full summon panel, both generations, same query, same window bounds.
- `icon-row-rendered-blowup-before-64px.png` /
  `-after-128px.png` — the Calculator row's icon cropped from the screenshot
  above and blown up 8x nearest-neighbor (so no *further* resample is
  introduced by the blow-up itself), showing the actual rendered pixels.
  The 128px generation's edges (calculator screen bezel, keypad button
  boundaries, outer squircle edge) are visibly cleaner.
- `icon-cached-source-blowup-before-64px.png` /
  `-after-128px.png` — the *cached PNG itself* (not the rendered row),
  nearest-neighbor blown up to the same final size for direct comparison.
  This isolates extraction quality from the renderer's own downscale: the
  64px source's calculator keys are visibly diamond-shaped/blocky
  (aliased at extraction time already), the 128px source's are round.
  This is the single clearest piece of evidence that the fix addresses a
  real extraction-time defect, not just "a bigger number."

## Row/socket treatment (the brief's other ask)

Checked `panel.rs`'s `render_row` (`Icon::Image` branch) and
`theme::ROW_ICON_PX`/`ROW_ICON_RADIUS_PX`/`ROW_ICON_SOCKET_BG` against the
frozen mockup (`data/neko-design/mockups/design.css`, `.row-icon`/
`.row-icon img`) and the `better-ui` skill's concentric-radius and
image-outline guidance. Findings:

- **Geometry matches the frozen mockup exactly**: 22x22 slot, 6px radius on
  both the socket and the `img` itself, socket background at low-alpha
  `TEXT_PRIMARY`. Nothing here is a drift from spec — it's what the design
  literally specifies, and per the brief's own constraints, panel geometry
  is settled and out of scope for this task regardless.
- **The socket isn't fighting the icon's shape.** Inspected the rendered
  blow-ups above at 8x: the tinted plate reads as a soft halo behind each
  icon's own (mostly-squircle) transparent padding, not a competing or
  visibly clipping second edge — consistent with the rationale already on
  record in `AGENTS.md`'s "Design tokens" section (`ROW_ICON_SOCKET_BG`
  exists specifically to unify wildly different per-app padding/brightness
  into one consistent system, from the `panel-craft-pass` task). No visible
  double-ring or hard clip line on Calculator or ChatGPT Classic (a white
  squircle app, the highest-contrast case against the dark socket) in
  either screenshot.
- **Conclusion: no row-treatment defect found beyond bitmap sharpness.**
  The captain's "icons and all look so off" reads, on this evidence, as
  the bitmap softness this task fixes — the socket/radius treatment itself
  was already correct and deliberate. Not changed, per this task's own
  constraint against touching panel geometry/spacing.

## Regression checks

Isolated `HOME`, release binaries, same machine:

- **Daemon idle memory**: `ps -o rss= -p <daemon-pid>` after startup +
  icon extraction settled → **13456 KB (≈13.1 MB) RSS**, matching the
  documented ~13MB baseline — no regression from caching 4x the pixel area
  per icon (PNG files on disk, not resident memory; nothing new is held in
  the daemon's own memory per icon beyond the same one-PNG-at-a-time
  extraction loop as before).
- **Warm summon latency**: `NEKO_BENCH=8`, same methodology as
  `docs/evidence/summon-latency.md`. First sample is the documented cold
  path (37.8ms, in line with the existing ~65-160ms-range cold budget for
  a *process* cold start — this bench's "cold" is warmer than a true fresh
  launch since the window/renderer are already live); the next 7 warm
  samples: 4.14, 6.19, 6.86, 2.66, 1.49, 2.21, 2.70ms — squarely inside the
  documented 2.9-6.4ms warm range, no regression. Icon cache size is
  entirely off the summon path (summon is hotkey-press → first frame,
  independent of any daemon round-trip — see `AGENTS.md`'s "Summon
  latency").

## What wasn't changed, and why

- **Panel geometry, spacing, palette, `material.rs`**: explicitly out of
  scope per this task's constraints; also, per the row/socket investigation
  above, not actually broken.
- **Real per-file icons for the file-search provider**: still the painted
  `Glyph::File`/`Glyph::Folder` placeholders from the provider-abstraction
  task — unrelated to app-icon sharpness, not touched.
- **A generation past 128px** (e.g. 256px, the next native size up) was
  considered and rejected: 128px already gives the renderer more real
  source detail than the display's own physical pixel budget needs at
  either of the captain's confirmed 2x displays (128 vs. a 44px target),
  with real headroom for a hypothetical 3x display (66px) this repo has no
  way to test against; doubling again to 256 would roughly double storage
  again for no measurable sharpness gain at any scale factor this task can
  verify.

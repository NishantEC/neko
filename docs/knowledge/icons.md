## Icons

`neko_core::icons` extracts each app's real icon via `NSWorkspace::iconForFile`
(not a loose `.icns`-file parse, which silently fails on modern
asset-catalog-only apps) and caches it as a PNG under
`~/Library/Caches/neko/icons/v3-128px/`. Runs once per app on daemon startup, in a
background thread *after* the app index is already searchable, so it never
delays the first search.

**Cached at `ICON_CACHE_PX` (128px) — a real macOS icon-representation
size, not a value picked to be "big enough."** The 1024px→64px pass (below)
fixed a genuine disk-space problem but picked its replacement size as
"roughly 3x the 22pt render slot," without checking what macOS actually
ships representations at (16, 32, 128, 256, 512, 1024, each at 1x/2x pixel
density). That made 64 a *non-native* target: `extract_icon_png`'s own
`drawInRect` call resampled once to reach a non-native 64x64, and GPUI's
renderer (a plain bilinear `min_filter: linear` sampler, no mipmap chain —
`gpui-0.2.2/src/platform/mac/shaders.metal`) resampled a second time down
to the display's physical pixels (44px at the captain's confirmed 2x
backing scale on both his displays) — two compounding lossy steps for
artwork whose whole problem was already "looks soft." Caching at 128
instead — a real representation size — lets extraction's draw call copy
instead of resample for most icons, leaving exactly one resample in the
whole pipeline. Full diagnosis, before/after screenshots (including a raw
cached-PNG nearest-neighbor blow-up that isolates extraction quality from
rendering), and the display-move reasoning (nothing app-side has to
special-case it — GPUI re-samples the same cached bitmap at the window's
live `scale_factor()` every frame already):
`docs/evidence/icon-cache-128px-report.md`. Storage, same real 146-app
index: 1.10MB at 64px → 2.52MB at 128px — both trivially far from the
original 188MB/1024px problem below.

**Cached at `ICON_CACHE_PX`, not whatever size the source representation
happened to be — the original, 1024px→64px fix.** A first pass here
trusted `NSImage.setSize` + `TIFFRepresentation` to produce a small bitmap;
it doesn't — a modern asset-catalog icon's source representation can be as
large as 1024x1024, `setSize` only changes the image's reported *drawing*
size, and `TIFFRepresentation` serializes the untouched source
representation. Measured on a real 147-app index: 188MB across 142 files
(`com.apple.calculator.png` alone 1.4MB) for icons displayed at 22px.
`extract_icon_png` now draws into an explicitly `ICON_CACHE_PX`-sized
`NSBitmapImageRep` via `NSGraphicsContext::graphicsContextWithBitmapImageRep`
(the non-deprecated replacement for `NSImage.lockFocus`/`unlockFocus` —
works from a background thread with no window or run loop, same as every
other AppKit call site in this daemon) instead. `CACHE_GENERATION`
(`"v3-128px"`) is a versioned subdirectory, not a flat rename:
`ensure_cached_icon` only checks whether *a* file exists at its cache path,
not what size it is, so bumping this constant is what makes a future
pixel-size change regenerate rather than keep serving a stale file forever.
`purge_stale_icon_cache` (called once from `neko-daemon`'s startup
icon-extraction thread) is the cleanup for caches written under any
previous generation — deletes loose `*.png` files sitting directly in
`icons/` (the pre-versioning layout) *and* any versioned subdirectory other
than the current `CACHE_GENERATION` (e.g. `v2-64px` after the 128px
upgrade), so it's a no-op once it's caught up. No code reads old-generation
files as a fallback; a captain who never runs the daemon again after
upgrading keeps the old bytes on disk until this cleanup pass runs once,
which happens automatically on the next `neko-daemon` start.

**A client already showing search results does not, on its own, ever find
out an icon it was missing has since been extracted — a real defect, not
hypothetical.** `AppsProvider::search` (`apps.rs`) sets `SearchItem::icon`
to `Icon::Placeholder` until it finds the file on disk
(`icons::cached_icon_path(...).filter(|p| p.exists())`, re-checked fresh on
every `Request::Search`, then `Icon::Image(path)`), but a client's results
are a one-time snapshot: nothing re-runs `run_search` on its own after that
snapshot lands, and background extraction (real AppKit work, run once after
the daemon's own startup — see above) can easily still be in flight when a
captain's very first summon after a fresh install or a daemon restart
renders. Confirmed live: summon immediately after clearing the cache and
restarting the daemon left every icon socket blank even once extraction had
finished in the background, until the client process itself was relaunched.
Fixed with a push, not a poll: `Event::IconsUpdated` (`neko_protocol`)
broadcasts once after each background extraction pass finishes (the startup
pass and each `watch_applications`-triggered incremental pass —
`server::notify_icons_updated`, `neko-daemon/src/main.rs`); `neko`'s
daemon-event loop (`main.rs`) routes it to `panel::Root::refresh_icons`,
which re-runs the *current* query (not `reset_for_summon` — this can fire
mid-search, and clearing what the captain already typed would be wrong) so
already-visible rows pick up icons that just finished, live, in the same
window, no relaunch and no per-frame filesystem check anywhere on the client
side (the daemon is the only place that stats an icon file, once per search
it already has to run). `panel::app_icon_placeholder_glyph` fills a
not-yet-resolved app row's `Icon::Placeholder` slot with a small
hand-painted neutral mark instead of an
empty hole, self-healing to the real icon in place once `refresh_icons`
re-renders it — see "Provider abstraction" below for `Icon`/`Glyph`, the
wire-level rendering vocabulary this and every other provider's row now
draws from instead of a client-side `match` on result type. **It stays a
painted `div` mark even now that there is an icon-asset pipeline**
(`fm/neko-icons`, "Icons: real SVGs, and the one that stays painted"
below) — reading as a *different* kind of mark from the real glyph
vocabulary is its whole job.
`NEKO_ICON_EXTRACT_DELAY_MS` (`neko-daemon/src/main.rs`) is a
verification-only, unset-by-default hook (same pattern as `evidence.rs`'s
`NEKO_BENCH`/`NEKO_FORCE_MATERIAL`) that stretches out the startup
extraction pass — real hardware finishes 146 icons in well under a second
even at 128px, too fast to reliably land a screenshot mid-pass without it.
Before/after window-scoped screenshots of the same summon (no relaunch, cold
→ resolved): `docs/evidence/icon-cache-cold-before.png` /
`icon-cache-warm-after.png`.

**The above is the daemon-side, on-disk PNG cache. Client-side, GPUI's own
image cache is separately bounded — `crates/neko/src/row_icon_cache.rs`.**
A static, source-only investigation
(`data/neko-leak-audit/report.md` in the firstmate home, §2 alternative #2 /
§4 item 3) found that `gpui`'s sprite atlas (`MetalAtlas::remove`,
`gpui-0.2.2/src/platform/mac/metal_atlas.rs`) only returns a whole texture
to the free list once every tile inside it has been individually removed,
and that this crate never called the one caller-facing entry point that
removes a tile at all (`Window::drop_image`/`image_cache(...)`) — every row
icon (`panel::render_row`'s `Icon::Image` case) went through GPUI's default,
never-evicted per-`App` asset cache instead, an unbounded hole for as long
as the process ran. **The report explicitly does not believe this was the
primary driver of a captain-reported 20 GB resident-memory growth** — it
ranks a GPUI window-activation code path (`windowDidBecomeKey:`'s forced
synchronous redraw, gated on key-window transitions) above it, and that
cause remains unconfirmed; this fix closes a real, documented gap this
crate owns outright, not the 20 GB investigation itself.

`row_icon_cache::RowIconCache` is a bounded-LRU `ImageCache`
(`ROW_ICON_CACHE_CAPACITY`, 256 — comfortably above the real, closed
row-icon identity space of ~150 apps plus one shared System Settings icon;
file-search and clipboard rows use painted `Glyph`s, never `img()`, so they
never touch this cache at all), installed once on the results-list
container (`panel::Root::render_content_area`, `.image_cache(...)`) rather
than per row. Adapted from `gpui`'s own shipped
`examples/image_gallery.rs::SimpleLruCache` (Apache-2.0, the same license
`gpui` is already vendored under — not one of this project's GPL-licensed
reference apps), with the recency/eviction bookkeeping split into its own
`Window`-free `RecencyOrder` type specifically so the bound could be unit
tested without a live GPUI window.

**Deliberately not cleared at `reset_for_summon`/`refresh_icons`, even
though those are the two natural "the result set changed" boundaries.** A
full clear on every fresh summon would drop icons about to be shown again
immediately, forcing a redundant disk reload and a visible blank-then-appear
flash on almost every summon — the same "evicted too eagerly" regression
class this section's cold-cache and placeholder fixes above already closed
once each. The bounded LRU evicts continuously instead, only when a
genuinely new icon identity is requested while already at capacity, which a
fresh summon's query (or a `Placeholder`→`Image` promotion) naturally
triggers on its own — see `row_icon_cache.rs`'s and `panel.rs`'s own doc
comments on `reset_for_summon`/`refresh_icons` for the full reasoning.

**No new filesystem work on the summon path.** The actual `fs::read` of a
cached icon PNG already happened asynchronously, off the GPUI-frame path,
before this task — through GPUI's own `ImageAssetLoader` via the default
per-`App` asset cache. This task changes *which* cache holds the decoded
result (and adds real eviction), not when or how the file is read; summon
latency (`AGENTS.md`'s own "Summon latency" section) was not remeasured as
part of this task since nothing on that path changed.

**What this did not verify, and why.** The task brief that produced this
section explicitly ruled out launching the real client, opening a window, or
capturing a screenshot (the captain was using this machine at the time), so
this was verified with `cargo test`'s headless `#[gpui::test]` machinery
only (`TestAppContext`/`TestWindow` — the same mechanism already used by
`text_field.rs`'s pre-existing tests; no OS window, no screen pixels) plus
plain `#[test]`s for the eviction-order bookkeeping itself. **Not verified**:
that this bound has any measurable effect on the captain's real, reported
20 GB growth (the report's own position, restated above); that a real
summon's visible icons actually come from a warm cache hit in practice
rather than incidentally re-loading (both are correct either way, but only
one is "free"); and end-to-end confirmation via `vmmap`/real memory sampling
that `drop_image` calls here actually shrink resident atlas memory — the
report's own §6 names the exact `vmmap` methodology that would settle that,
not attempted here per the brief's "no rendering" constraint.

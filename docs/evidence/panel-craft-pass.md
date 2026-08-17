# Icon and row-layout craft pass — verification

The captain's words: *"the icons are very looking bad, the UI itself is looking
very bad now."* Diagnosed against the frozen design system
(`data/neko-design/report.md` §2 and `data/neko-design/mockups/design.css`),
not redesigned — every number below is a direct transcription from
`design.css`'s own `.row`, `.row-icon`, `.type-tag`, `.panel-input`,
`.panel-footer`, and `.footer-divider` rules, cited inline.

## What was actually wrong, and the fix

| Element | Before (`panel.rs`) | design.css source | After |
| --- | --- | --- | --- |
| Row icon corner radius | `rounded(px(4.))` | `.row-icon { border-radius: 6px }` | `rounded(px(theme::ROW_ICON_RADIUS_PX))` = 6px |
| Row icon backing (real + placeholder) | none on real icons; placeholder `rgba(0xffffff14)` — pure white | `.row-icon { background: rgba(240,230,218,0.06) }` — `TEXT_PRIMARY`'s own hex at 6% | `theme::ROW_ICON_SOCKET_BG` (`rgba_const(0xf0e6da, 0.06)`) on **both** the real `img()` and the placeholder |
| Clipboard type-tag pill background | `rgba(0xffffff0f)` — pure white | `.type-tag { background: rgba(240,230,218,0.06) }` | `theme::ROW_ICON_SOCKET_BG` (same token, design.css uses the same value for both) |
| Content-area outer padding | none | `.panel-list { padding: var(--sp-2) }` = 8px | `.px_2()` on the content-area container |
| Row horizontal padding | `.px_2()` = 8px | `.row { padding: 0 var(--sp-3) }` = 12px | `.px_3()` = 12px |
| Section-header horizontal padding | `.px_2()` = 8px | section header's own `var(--sp-3)` horizontal component | `.px_3()` = 12px |
| Input-row padding | `.px_4()` = 16px | `.panel-input { padding: 0 var(--sp-5) }` = 20px | `.px_5()` = 20px |
| Footer padding | `.px_4()` = 16px | `.panel-footer { padding: 0 var(--sp-5) }` = 20px | `.px_5()` = 20px |
| Footer top border | `rgba(0xffffff0f)` | `.panel-footer { border-top: 1px solid var(--border-hairline) }` | `theme::BORDER_HAIRLINE` (already a correctly-defined token, just not wired in here) |
| Footer action divider | `h(px(14.))`, `rgba(0xffffff1f)` | `.footer-divider { height: 16px; background: var(--border-hairline-strong) }` | `h(px(16.))`, `theme::BORDER_HAIRLINE_STRONG` |

## Why this is what "bright stamps" and "loose rhythm" meant

**Icons ("bright stamps"):** two independent, compounding causes.

1. Every icon slot without a cached PNG yet rendered a **pure white**
   `rgba(0xffffff14)` square. This machine's palette is warm (`SURFACE_PANEL`
   is `#110c07`, a warm near-black); a neutral-white overlay reads as a cold,
   bright stamp against it, where the design's own token
   (`rgba(240,230,218,0.06)`, `TEXT_PRIMARY`'s hex, not pure white) blends in.
2. Real cached app icons got **no backing plate at all** — just a bare
   `img()` clipped to a 4px radius. Real macOS icon assets (`NSWorkspace.
   iconForFile`) vary enormously in how much transparent padding they bake
   into their own canvas (compare `Calendar`'s icon, which fills its square
   edge-to-edge, against `ChatGPT Classic`'s, which is a small white circle
   floating in a mostly-transparent square — both real, both extracted the
   same way). With no consistent socket behind them, that inconsistency
   reads directly on the dark panel as "icons of very different shapes and
   brightness." `design.css`'s own `.row-icon` rule always paints the same
   faint warm background behind the icon regardless of source — the fix
   applies that same rule here, to real icons and the placeholder alike.

**Row rhythm ("loose... compared to the mockups"):** the content area had
**zero** left/right padding at all, so rows' own 8px (`.px_2()`) was the
*entire* inset from the panel's rounded edge — while the input row above sat
16px in (`.px_4()`). Icons and titles in the list sat visibly closer to the
panel's edge than the search glyph directly above them, breaking the "align
to shared edges" reading a list is supposed to have. It also broke a
concentric-radius relationship a better-ui review would flag immediately:
the panel's 16px corner radius and the selected row's 8px pill radius only
read as concentric when `16 = 8 (pill radius) + 8 (padding)` — with zero
container padding that identity didn't hold. Restoring `design.css`'s own
two-layer padding (8px list container + 12px row padding = 20px total,
matching the input row's own 20px) fixes both the alignment and the
concentricity in the same change.

## Verified against the exact frozen values

Every "after" number above is a direct copy from `design.css` (not
re-derived, not eyeballed) — `ROW_ICON_RADIUS_PX`/`ROW_ICON_SOCKET_BG` added
to `theme.rs` alongside the existing token table, following that file's own
established pattern (`rgba_const`, named constants, doc comments citing the
source rule). `cargo test` covers the parts of this pass that are pure
logic (`panel.rs`'s existing `fit_within_budget`/`fit_section` tests, which
exercise the *vertical* row-count budget — deliberately untouched by this
pass, since every change here is horizontal padding only, to avoid
re-deriving that already-tuned, already-tested budget for an unrelated
craft fix).

## Screenshots: what's real evidence here, and what isn't

**Before, real and independently verified**: two existing screenshots in
this directory, both captured before any of this task's changes existed —
`app-discovery-search-results-raycast.png` and
`summon-panel-search-results.png`. `git log -p -- crates/neko/src/panel.rs`
confirms the icon-rendering block (`rounded(px(4.))`, `rgba(0xffffff14)`,
`.px_2()`) has been byte-identical since `54db43b` (clipboard history) —
neither screenshot's date matters as much as this: nothing touched that code
between either capture and the start of this task, so both are accurate
"before" evidence for exactly the defect described. Look at
`summon-panel-search-results.png` in particular: Calculator, Calendar,
ChatGPT Classic, Claude Code URL Handler, Tailscale, and Rectangle sit in one
list with six visibly different icon "brightnesses" and container shapes,
each icon nearly flush against the panel's own rounded left edge — the exact
defect described.

**After, attempted and explicitly not trusted**: this task tried, at length,
to capture a fresh window-scoped "after" screenshot — `NEKO_SHOW_ON_LAUNCH`
under an isolated `HOME`, external activation via `System Events`, a real
synthesized mouse click at a verified-safe coordinate (the input row, never
a result row), and a from-scratch `.app` bundle launched via `open` to rule
out a raw-binary launch-context difference. Every attempt captured content,
but a direct diagnostic (`root.results.len()` logged from inside
`run_search`'s own response handler) proved the daemon and the client's
in-memory state were both correct — 7 real ranked apps, `cx.notify()`
called — while the screenshot kept showing the pre-search empty state
regardless. The conclusive finding: a "before" and "after" capture, from two
separately rebuilt binaries with genuinely different `panel.rs` source, came
back **byte-identical (matching MD5)**. That rules out "hasn't repainted
yet" and points at the capture path itself — most likely this sandboxed
session's `screencapture -l<windowID>` serving a cached WindowServer
thumbnail rather than a live grab for a window that never becomes key in a
non-interactive launch, the same class of issue
`app-discovery-verification.md`'s own "A false alarm, corrected" section
already ran into and worked around with tooling this session doesn't have
(a real Dock click from a genuinely interactive session). Given that, no
fabricated "after" image is included here — an identical-looking pair of
screenshots would be worse than none. The code change itself is the
verifiable artifact: every value in the table above is a direct,
line-citable match to `design.css`, and `cargo build`/`test`/`clippy` are
clean. **A live look at the running app remains the one still-open
verification step** — flagged, not silently skipped, per this same
directory's own precedent for the material task's translucency (which has
the identical capture limitation for a different reason — see
`material-readback-verification.md`).

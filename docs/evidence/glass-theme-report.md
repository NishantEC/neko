# Glass theme (2026-10-22)

A glassmorphism built-in, id `glass`, label "Glass", added in all three places
themes live. It is neko's own palette (nothing vendored, nothing to license).

| Surface | Where | What Glass is there |
| --- | --- | --- |
| GPUI quick panel | `crates/neko/src/theme.rs` `THEMES`, `neko_protocol::BUILTIN_THEMES` | A `Spec` like every other: navy panel `#0f1a2b` at `panel_alpha` 0.64 (the lowest dark fill beside `neutral`, so the native material's blur carries the surface), bright hairlines at 0.14. |
| Native quick panel | `native/NekoKit/.../PaletteThemes.swift` | The same five values under the same id. |
| Web UI | `web/src/theme.ts`, `web/src/styles/globals.css`, the three pickers | A fourth Appearance choice. It sets `html.dark.glass`, so every dark rule still applies and `html.glass` overrides only tokens: translucent card/popover/sidebar/background, `backdrop-filter: blur(22px) saturate(160%)`, white 14% hairlines, an inset highlight plus a soft drop shadow, and a navy/violet/teal gradient wash on `body`. Solid fills replace the translucent ones where `backdrop-filter` is unsupported. The wash is not drawn inside the native window (`data-shell="native"`), which shows its own material. |

Every other theme is untouched: no existing `Spec`, Swift entry or token value
changed, and all web rules are scoped to `html.glass`.

## Contrast

GPUI/native: `glass` passes every gate in `theme.rs` (`themes_all_pass_wcag_aa`'s
eleven pairs, the worst-wallpaper composite, the selection step, the ramp
tests). Worst pairs: `text_secondary` on `surface_selected` 5.98:1;
`text_primary` over the panel composited on white at 0.64 alpha about 4.9:1
(it fails AA below about 0.62, which is why 0.64 is the floor).

Web: measured from the rendered page (screenshot blurred 8–12px so text does
not count as background, brightest pixel per region):

| Region | Brightest background | `--foreground` `#e8eef8` | `--muted-foreground` `#a9b8cd` |
| --- | --- | --- | --- |
| Main, left gutter (violet wash peak) | `#161c42` | 14.1:1 | 8.2:1 |
| Main, right gutter (teal wash) | `#052239` | 13.9:1 | 8.1:1 |
| Chat column (text included, so conservative) | `#2d394f` | 10.0:1 | 5.8:1 |
| Composer | `#232f45` | 11.5:1 | 6.7:1 |
| Sidebar (text included) | `#313c52` | 9.5:1 | 5.5:1 |

`--sidebar-foreground` `#c3cfdf` on the sidebar sample: 7.0:1. All clear AA
4.5:1.

Graphical objects (WCAG 1.4.11, 3:1). `StatusGlyph` (`src/work/tone.tsx`)
draws the Completed check and the Failed "!" in `var(--background)` on a
filled `currentColor` circle. Glass's `--background` is 42% alpha, so the
mark blended into the circle; `html.glass svg[data-status]` now gives the
glyph the solid fallback `#0d1626`:

| Mark | Circle | Translucent mark (before) | Solid `#0d1626` (after) |
| --- | --- | --- | --- |
| Completed check | `--success` `#4cc38a` | 2.31:1 | 8.17:1 |
| Failed "!" | `--destructive` `#e5484d` | 2.04:1 | 4.63:1 |

`glass-theme-status-glyphs.png` (shared Chrome, the production build's CSS
under `html.dark.glass`, StatusGlyph's exact SVG markup on a `bg-card`
fixture, not the live task list; the computed strokes read
`rgba(13, 22, 38, 0.42)` before and `rgb(13, 22, 38)` after).

Other `var(--background)` uses as a foreground, stroke or ring, audited:

- `ui/avatar.tsx` `ring-background`: Avatar is imported only by
  `pages/Gallery.tsx`, not by any app page; the ring is a separator between
  overlapping avatars, not information. Left as is.
- `ui/button.tsx` `ring-offset-background`: the offset is the gap between a
  button and its focus ring; the ring itself is the opaque `--ring`. A
  translucent gap shows the same frosted surface the button sits on. Left as is.
- `ui/kbd.tsx` `[[data-slot=tooltip-content]_&]:text-background`: no `Kbd`
  renders inside a tooltip anywhere in `src/` (grep). Unreachable.
- `styles/sky-materials.css` `--sky-ink-fg`: no element carries `sky-ink`
  (grep), so it is unused. `--sky-pebble` mixes `--background` into a fill,
  not a foreground.

## Verified

- Picker (shared Chrome, test harness, synthetic seed): Glass appears in the
  status-bar theme menu and Settings → Appearance; selecting it sets
  `html.dark.glass`, persists, survives reload; the body gets the wash and the
  sidebar computes `blur(22px) saturate(1.6)`; choosing Dark removes `glass`
  and the wash. No page errors.
- Dark and Light, HEAD build against this build: differences are only the
  streaming reply and running timers, not styling.
- GPUI Themes mode: the existing preview/commit/revert tests in `panel.rs`
  pass with Glass in the registry (they do not single it out).

Not verified:

- The GPUI and SwiftUI quick panels on screen in Glass (no window-scoped
  capture was taken).
- The web UI inside the native WKWebView shell, which is how
  `/Applications/Neko.app` shows it (`data-shell="native"`, injected by
  `WebHome.swift:67`). There the glass wash is not painted (the
  `:not([data-shell="native"])` rule), and the translucent surfaces sit over
  the native window material, which follows the system appearance. Text
  contrast in that host was not measured; a light system appearance under
  Glass's light text is the case most likely to fall short.

Screenshots (1440×900, shared Chrome): `glass-theme-before-dark-{main,sidebar}.png`
(the Dark theme, unchanged) and `glass-theme-after-glass-{main,sidebar}.png`,
in `docs/evidence/` (PNGs there are gitignored, so they are not committed).

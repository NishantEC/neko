# Themes — the swappable token table, seventeen built-ins, and what was verified

`fm/neko-themes`. The captain: *"can we suport like themes section where people
can select? by default we can have catpuccin and gruvbox, solaroid and
similar"*.

Read `crates/neko/src/theme.rs`'s own module doc comment first — it is the
normative statement of the token-table contract. This file is the evidence:
the licence audit, the per-theme contrast numbers, the twenty AA corrections,
the screenshots, and the measurements.

---

## 1. What was built

* **`theme.rs` is a table, not a wall of constants.** Twenty colour tokens
  moved from `pub const` into one `Palette` struct; every paint site reads
  `theme::active()`. Geometry, spacing and type stayed `const` — a theme is
  colour and surface only, and cannot move a row or change a radius.
* **`Spec` → `build()` is the only constructor.** A theme supplies its
  independent colours; the seven dependent tokens
  (`surface_panel_translucent`, `menu_glass_tint`, `text_tertiary_on_selected`,
  the two hairline weights, the two state borders, the danger banner, the icon
  plate) are derived in one place. A theme cannot redefine what a token
  *means*, only what it is.
* **A `Themes` command entering a `theme` mode**, exactly like
  `Clipboard History`: `neko_core::commands` gained one `CommandSpec`,
  `crate::modes` one `ModeChrome`, `neko_core::themes` one `Provider`. Nothing
  in `enter_mode`/`exit_mode`/`run_search` or the wire protocol changed to
  accommodate it — which is the accounting `modes.rs`'s own doc comment
  promised a second command would pay.
* **Live preview.** Arrowing (or typing to filter) applies the palette to the
  whole panel as the selection lands. Escape restores what was in use before
  the mode opened; Enter clears that restore point and persists.
* **Persistence** through the existing `settings` KV table
  (`neko_core::themes::{get_theme,set_theme}`), read at client startup via
  `Request::GetTheme`, fanned out to other clients via `Event::ThemeChanged`.

### What "no per-frame cost" actually means here

`theme::active()` is one `Ordering::Relaxed` `AtomicUsize` load and a slice
index into `&'static [Theme]` — no allocation, no lock, no `Arc` clone. The
data it indexes is immutable rodata, so a reader cannot observe a half-published
theme however the load and store are reordered; there is nothing for a stronger
ordering to buy. The alternatives considered and rejected: a `Mutex`/`RwLock`
(a lock acquisition on the render path), and an `Arc<Palette>` swap (an atomic
refcount increment on the render path).

The one genuinely new per-frame operation is `Root::sync_window_appearance`:
a single enum compare against a cached value. The AppKit call behind it happens
once per *change*, and
`panel::tests::a_light_theme_puts_the_window_into_the_light_appearance_and_a_dark_one_takes_it_back`
pins that ("an unchanged appearance must not make an AppKit call every frame").

---

## 2. Licences — verified from source, not assumed

Every vendored palette is **MIT**. Nothing GPL entered the tree; no palette
ships as code, only as colour values re-expressed in `theme.rs`'s own `Spec`
form. Verified 2026-08-21 by fetching each project's own licence file (or, for
Gruvbox, its two in-repo declarations), not by trusting a badge or memory.

| Palette | Upstream | Licence | How it was verified |
|---|---|---|---|
| Catppuccin (Latte, Frappé, Macchiato, Mocha) | [catppuccin/catppuccin](https://github.com/catppuccin/catppuccin) · values from [catppuccin/palette](https://github.com/catppuccin/palette) `palette.json` | MIT | [`LICENSE`](https://github.com/catppuccin/catppuccin/blob/main/LICENSE) fetched; opens "MIT License / Copyright (c) 2021 Catppuccin" |
| Gruvbox (dark, light) | [morhetz/gruvbox](https://github.com/morhetz/gruvbox) · values from `colors/gruvbox.vim` | MIT | **No `LICENSE` file exists in the repo** — stated instead in [`README.md`](https://github.com/morhetz/gruvbox/blob/master/README.md) ("License — MIT/X11") and in `package.json` (`"license": "MIT"`). Recorded exactly that way rather than implying a licence file that isn't there. |
| Solarized (dark, light) | [altercation/solarized](https://github.com/altercation/solarized) · values from `vim-colors-solarized/colors/solarized.vim` | MIT | [`LICENSE`](https://github.com/altercation/solarized/blob/master/LICENSE) fetched; MIT text, "Copyright (c) 2011 Ethan Schoonover" |
| Nord | [nordtheme/nord](https://github.com/nordtheme/nord) · values from `src/nord.css` | MIT | GitHub licence API → SPDX `MIT`, [`license`](https://github.com/nordtheme/nord/blob/develop/license) |
| Tokyo Night (Storm) | [tokyo-night/tokyo-night-vscode-theme](https://github.com/tokyo-night/tokyo-night-vscode-theme) · values from `themes/tokyo-night-storm-color-theme.json` | MIT | GitHub licence API → SPDX `MIT`, [`LICENSE.txt`](https://github.com/tokyo-night/tokyo-night-vscode-theme/blob/master/LICENSE.txt). **Deliberately the original VS Code theme, not `folke/tokyonight.nvim`** — that popular port is Apache-2.0, which is fine but would have been the only non-MIT entry in this table for no gain. |
| Rosé Pine (main, dawn) | [rose-pine/rose-pine-theme](https://github.com/rose-pine/rose-pine-theme) · values from [rose-pine/palette](https://github.com/rose-pine/palette) `source/index.ts` | MIT | Both repos' `LICENSE` fetched / licence API → SPDX `MIT` |
| Dracula | [dracula/dracula-theme](https://github.com/dracula/dracula-theme) · values from `README.md`'s palette table | MIT | [`LICENSE`](https://github.com/dracula/dracula-theme/blob/master/LICENSE) fetched; "The MIT License (MIT) / Copyright (c) 2023 Dracula Theme" |
| Everforest (dark) | [sainnhe/everforest](https://github.com/sainnhe/everforest) · values from `palette.md` | MIT | [`LICENSE`](https://github.com/sainnhe/everforest/blob/master/LICENSE) fetched; "MIT License / Copyright (c) 2019 sainnhe" |

`neutral`, `ember` and `catnap` are this project's own work — `neutral` is
byte-identical to what `theme.rs` shipped before this task, and the other two
come from `data/neko-cozy-theme/mockups/palette-{ember,catnap}.css` in the
firstmate home.

### Which neighbours were chosen, and why

Beyond the three families the captain named: **Nord, Tokyo Night, Rosé Pine
(+ Dawn), Dracula, Everforest**. The selection rule was *sourceable cleanly and
distinguishable on screen*: each has a canonical machine-readable palette in
its own repo under a verifiable MIT licence, and each occupies a hue region no
other built-in does (Nord's desaturated blue-grey, Tokyo Night's indigo, Rosé
Pine's mauve, Dracula's high-chroma purple/pink, Everforest's green-grey).
Rosé Pine Dawn was added because four light themes across three families is a
more useful spread than three.

**Not shipped, and why:** Everforest Light (four light themes already, and it
sits close to Gruvbox Light's cream); Rosé Pine Moon (visually between `main`
and Catppuccin Macchiato — a row that would not tell you anything new);
Nightlight, Hearth and Sherbet from `data/neko-cozy-theme` (Hearth and Sherbet
both need geometry changes or `material.rs` work beyond an appearance switch;
Nightlight needs a `linear_gradient` on the selected row and five per-section
hue constants — real features, not palettes, and this task's brief scopes it
to colour and surface only).

---

## 3. Contrast — every theme, every pair, WCAG AA

Enforced by a test, not by this table:
`theme::tests::themes_all_pass_wcag_aa` fails the build if any pair drops below
**4.5:1**. `1°`/`2°`/`3°` are `text_primary`/`text_secondary`/`text_tertiary`;
`sel step` is `surface_selected` against `surface_panel`, pinned separately at
≥1.30:1 by `every_theme_has_a_visible_selection_step_away_from_its_panel` so
contrast tuning can never "fix" a text pair by walking the selection back into
the panel.

**There are no AA failures to name.** All seventeen pass all eleven pairs.

| Theme | 1° on panel | 2° on panel | 3° on panel | 1° on raised | 2° on raised | 1° on input | 1° on sel | 2° on sel | success | danger | on_light/1° | sel step |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Neko Neutral** | 15.86 | 8.27 | 5.20 | 14.77 | 7.70 | 16.44 | 9.28 | 4.84 | 8.31 | 6.30 | 15.64 | 1.71 |
| **Ember** | 15.26 | 9.49 | 6.01 | 12.94 | 8.04 | 16.75 | 7.55 | 4.69 | 8.61 | 6.09 | 15.92 | 2.02 |
| **Catnap** | 12.66 | 8.98 | 6.24 | 10.09 | 7.15 | 15.51 | 6.79 | 4.81 | 7.85 | 5.57 | 16.03 | 1.87 |
| **Catppuccin Mocha** | 11.34 | 9.26 | 7.37 | 8.69 | 7.10 | 12.14 | 6.31 | 5.15 | 11.03 | 7.08 | 12.97 | 1.80 |
| **Catppuccin Macchiato** | 9.92 | 8.17 | 6.62 | 7.55 | 6.22 | 10.85 | 5.59 | 4.61 | 9.17 | 5.96 | 11.73 | 1.77 |
| **Catppuccin Frappé** | 8.06 | 7.93 | 5.55 | 6.19 | 6.09 | 9.04 | 4.68 | 4.61 | 7.10 | 4.65 | 9.83 | 1.72 |
| **Catppuccin Latte** | 7.06 | 6.38 | 4.71 | 6.57 | 5.93 | 6.57 | 5.17 | 4.67 | 4.60 | 4.80 | 6.04 | 1.37 |
| **Gruvbox Dark** | 12.99 | 10.75 | 6.77 | 10.22 | 8.45 | 14.45 | 7.78 | 6.43 | 7.14 | 4.57 | 12.99 | 1.67 |
| **Gruvbox Light** | 12.99 | 10.22 | 5.74 | 13.39 | 10.53 | 11.73 | 8.59 | 6.76 | 4.55 | 7.60 | 12.99 | 1.51 |
| **Solarized Dark** | 13.92 | 6.50 | 4.75 | 12.05 | 5.63 | 15.87 | 9.77 | 4.56 | 4.69 | 4.58 | 13.92 | 1.42 |
| **Solarized Light** | 13.92 | 12.05 | 4.99 | 14.52 | 12.57 | 12.25 | 9.92 | 8.59 | 4.74 | 4.63 | 13.92 | 1.40 |
| **Nord** | 10.84 | 10.26 | 9.25 | 8.73 | 8.26 | 12.15 | 7.49 | 7.09 | 6.13 | 4.72 | 10.84 | 1.45 |
| **Tokyo Night** | 9.02 | 6.90 | 6.00 | 9.63 | 7.37 | 10.23 | 6.08 | 4.65 | 8.75 | 5.51 | 10.23 | 1.48 |
| **Rosé Pine** | 13.39 | 7.73 | 4.60 | 12.50 | 7.22 | 12.15 | 7.93 | 4.58 | 10.37 | 6.07 | 13.39 | 1.69 |
| **Rosé Pine Dawn** | 6.88 | 6.86 | 4.67 | 7.23 | 7.22 | 6.48 | 4.63 | 4.62 | 5.59 | 4.63 | 6.88 | 1.48 |
| **Dracula** | 13.36 | 7.77 | 4.63 | 11.06 | 6.44 | 14.81 | 8.59 | 5.00 | 10.38 | 4.53 | 13.36 | 1.56 |
| **Everforest Dark** | 7.38 | 6.06 | 4.61 | 6.40 | 5.26 | 8.62 | 5.57 | 4.57 | 6.23 | 4.55 | 7.38 | 1.33 |

Two pairs are deliberately absent from the gate, both accounted for:

* **`text_tertiary` on `surface_selected` fails in every palette checked,
  including the neutral one neko already shipped** (3.04:1 there). It is solved
  by the pre-existing promotion rule — a selected row renders that text as
  `text_tertiary_on_selected`, which *is* `text_secondary` — and that promoted
  pair is in the table above.
* **Nothing is checked against the translucent panel fill composited over a
  live desktop.** That depends on the wallpaper. This app's own worst case was
  measured separately in `data/neko-native-material/report.md` §6 (5.15:1 for
  the material alone, with no panel tint on top at all); at 0.82–0.90 panel
  alpha the backdrop's contribution to the final pixel is a small fraction of
  that already-comfortable case.

### The twenty corrections to upstream values

A terminal colour scheme is designed against a terminal's pairs, not this
app's. Twenty of 187 vendored token values needed a **lightness** correction to
clear AA here; hue and chroma were not touched, and `build()`'s derivations were
not bent to accommodate any of them.
`theme::tests::vendored_themes_match_their_pinned_upstream_and_shipped_hex`
pins *both* numbers for every token and asserts the count is exactly 20, so a
future edit that quietly walks another value away from upstream fails a test.

| Theme | Token | Upstream | Shipped | Was | Now | Pair that forced it |
|---|---|---|---|---|---|---|
| Catppuccin Frappé | `text_secondary` | `#b5bfe2` | `#c7cfe9` | 3.92:1 | 4.61:1 | secondary on selected |
| Catppuccin Latte | `text_secondary` | `#5c5f77` | `#53566b` | 4.05:1 | 4.67:1 | secondary on selected |
| Catppuccin Latte | `text_tertiary` | `#6c6f85` | `#676a7f` | 4.37:1 | 4.71:1 | tertiary on panel |
| Catppuccin Latte | `state_success` | `#40a02b` | `#327c21` | 2.96:1 | 4.60:1 | success on panel |
| Gruvbox Dark | `state_danger` | `#fb4934` | `#fb5643` | 4.29:1 | 4.57:1 | danger on panel |
| Gruvbox Light | `state_success` | `#79740e` | `#74700d` | 4.29:1 | 4.55:1 | success on panel |
| Solarized Dark | `text_secondary` | `#93a1a1` | `#a1adad` | 3.94:1 | 4.56:1 | secondary on selected |
| Solarized Dark | `state_danger` | `#dc322f` | `#e56663` | 3.25:1 | 4.58:1 | danger on panel |
| Solarized Light | `state_success` | `#859900` | `#667500` | 2.97:1 | 4.74:1 | success on panel |
| Solarized Light | `state_danger` | `#dc322f` | `#d72724` | 4.29:1 | 4.63:1 | danger on panel |
| Nord | `state_danger` | `#bf616a` | `#d18d93` | 3.05:1 | 4.72:1 | danger on panel |
| Rosé Pine | `text_secondary` | `#908caa` | `#aca9c0` | 3.25:1 | 4.58:1 | secondary on selected |
| Rosé Pine | `text_tertiary` | `#6e6a86` | `#837f9a` | 3.42:1 | 4.60:1 | tertiary on panel |
| Rosé Pine Dawn | `text_primary` | `#575279` | `#555076` | 4.49:1 | 4.63:1 | primary on selected |
| Rosé Pine Dawn | `text_secondary` | `#797593` | `#55526a` | 2.71:1 | 4.62:1 | secondary on selected |
| Rosé Pine Dawn | `text_tertiary` | `#9893a5` | `#716b80` | 2.73:1 | 4.67:1 | tertiary on panel |
| Rosé Pine Dawn | `state_danger` | `#b4637a` | `#ab526c` | 3.84:1 | 4.63:1 | danger on panel |
| Dracula | `text_tertiary` | `#6272a4` | `#8692b9` | 3.03:1 | 4.63:1 | tertiary on panel |
| Everforest Dark | `text_secondary` | `#9da9a0` | `#aeb7b0` | 3.86:1 | 4.57:1 | secondary on selected |
| Everforest Dark | `text_tertiary` | `#859289` | `#95a099` | 3.84:1 | 4.61:1 | tertiary on panel |

The policy that produced these, in order: **move the text, not the surface**
(neko's own neutral palette solves its one failing pair that way already), and
only move a surface when the text has run out of headroom towards white or
black *and* the surface can still keep a visible step from the panel. An
earlier pass that let surfaces move first produced `rose-pine`'s
`surface_selected` at `#272532` against a `#191724` panel — AA-compliant text
on an invisible selection row, which is a worse defect than the one it fixed.

Three light themes also needed a **darker upstream selection step** than the
obvious one, chosen from their own palettes rather than computed: Gruvbox Light
`light2` `#d5c4a1` (not `light1`), Rosé Pine Dawn `highlightHigh` `#cecacd`
(not `highlightMed`), and Solarized Light a darkened `base2`, which is the one
invented surface in the set — Solarized simply has no fourth light tone.

---

## 4. The two things that bite on a light theme

### 4.1 The icon plate inverts

`row_icon_socket_bg` was `text_primary` at 6% — a pale film, invisible on a
pale surface, which `data/neko-cozy-theme/report.md` called out precisely. The
*rule* is unchanged ("a low-alpha plate of the opposite polarity to the panel");
a light theme's `Spec` supplies a dark `icon_socket` hue instead of a light one.
Enforced structurally by
`theme::tests::every_theme_has_an_icon_plate_that_reads_against_its_own_panel`,
which compares relative luminance rather than pinning a hex, so a future theme
can pick any hue as long as the plate still reads.

Every screenshot in §5 shows real macOS app icons (`NSWorkspace.iconForFile`,
extracted into the isolated cache by the harness) on that plate.

### 4.2 `NSAppearance`, which a token swap cannot reach

The panel is translucent over a real `NSGlassEffectView`, and that view renders
in the **window's** appearance. A cream Latte panel over a `darkAqua` blur reads
as a cream card with a dark halo leaking through wherever the fill is thin —
the cozy report's own reason a light direction "needs real native work".

`material::set_window_appearance` sets `NSAppearance(named: .aqua/.darkAqua)` on
the real `NSWindow` (found through the same `raw-window-handle` walk
`material.rs` and `spaces.rs` already use), and
`material::window_appearance_name` reads it back — the same "verified, not
trusted" pattern as `verify_installed`/`verify_shadow_disabled`/`spaces::verify`,
logged at every launch.

It is set on the *window*, not on the material view, so appearance inherits down
to whichever material actually installed **and** to the `⌘K` menu overlay,
without either having to be found again.

**Every built-in keeps its translucency** (panel alpha 0.82–0.90); none of the
seventeen goes opaque and discards the material.
`theme::tests::every_theme_keeps_the_native_material_visible` asserts it. The
seam for an opaque theme exists (`Theme::keeps_translucency`) but nothing uses
it, because an opaque theme would be throwing away the most expensive thing
this app renders.

---

## 5. The gallery

All seventeen, same query (`con`), same isolated corpus — a mixed list with
four sections (**Applications** with real icons, **System Settings** with the
real Settings icon, **Commands**, **Clipboard**).

| Theme | id | | |
|---|---|---|---|
| **Neko Neutral** | `neutral` | Dark | ![neutral](themes/neutral.png) |
| **Ember** | `ember` | Dark | ![ember](themes/ember.png) |
| **Catnap** | `catnap` | Dark | ![catnap](themes/catnap.png) |
| **Catppuccin Mocha** | `catppuccin-mocha` | Dark | ![catppuccin-mocha](themes/catppuccin-mocha.png) |
| **Catppuccin Macchiato** | `catppuccin-macchiato` | Dark | ![catppuccin-macchiato](themes/catppuccin-macchiato.png) |
| **Catppuccin Frappé** | `catppuccin-frappe` | Dark | ![catppuccin-frappe](themes/catppuccin-frappe.png) |
| **Catppuccin Latte** | `catppuccin-latte` | Light | ![catppuccin-latte](themes/catppuccin-latte.png) |
| **Gruvbox Dark** | `gruvbox-dark` | Dark | ![gruvbox-dark](themes/gruvbox-dark.png) |
| **Gruvbox Light** | `gruvbox-light` | Light | ![gruvbox-light](themes/gruvbox-light.png) |
| **Solarized Dark** | `solarized-dark` | Dark | ![solarized-dark](themes/solarized-dark.png) |
| **Solarized Light** | `solarized-light` | Light | ![solarized-light](themes/solarized-light.png) |
| **Nord** | `nord` | Dark | ![nord](themes/nord.png) |
| **Tokyo Night** | `tokyo-night` | Dark | ![tokyo-night](themes/tokyo-night.png) |
| **Rosé Pine** | `rose-pine` | Dark | ![rose-pine](themes/rose-pine.png) |
| **Rosé Pine Dawn** | `rose-pine-dawn` | Light | ![rose-pine-dawn](themes/rose-pine-dawn.png) |
| **Dracula** | `dracula` | Dark | ![dracula](themes/dracula.png) |
| **Everforest Dark** | `everforest-dark` | Dark | ![everforest-dark](themes/everforest-dark.png) |

### The mode itself

| | |
|---|---|
| The `Themes` mode, listing every built-in. Each row previews **its own** palette in the swatch; `CURRENT` marks the persisted choice; the footer verb is `Use Theme ↵`. | ![themes list](themes/mode-themes-list.png) |
| **Live preview.** The same list with the selection on *Catppuccin Latte*: the entire panel — chrome, rows, footer, input row — has become Latte, while `CURRENT` is still on Neko Neutral because nothing has been confirmed. Escape from here restores Neutral. | ![preview latte](themes/mode-preview-latte.png) |
| The same, on Gruvbox Dark. | ![preview gruvbox](themes/mode-preview-gruvbox-dark.png) |
| **Persistence.** Tokyo Night was committed over the real socket, the daemon was stopped and restarted, and a *fresh client process with no theme hook set* renders it. | ![persistence](themes/persistence-after-restart.png) |

**A limitation of the capture, stated rather than glossed:**
`screencapture -l<windowid>` renders the window's own layer tree in isolation
with no live compositing of what is behind it (`AGENTS.md`, "Window material" —
this is a documented limitation of the only capture form this machine permits,
not a missing flag). So these show each palette's fills over the material's own
tint, not over a real desktop. That also means the sampled panel pixel sits a
few units off the nominal hex; see §6 for how each capture was nonetheless
proven to be its own theme.

---

## 6. How the screenshots were verified — and a real defect that caught

Every capture: window-scoped only, **non-activating**
(`material::order_front_regardless`), `neko: key window false` logged at the
moment of capture in all twenty runs, no synthetic input of any kind, isolated
`HOME` at `/tmp/neko-themes-ev`, served by `crates/neko-daemon/src/bin/verify_harness.rs`.
**The real `neko-daemon` was never launched**, so its clipboard capture loop
never touched the systemwide pasteboard; clipboard rows are seeded fixtures.

`evidence.rs` gained one hook, `NEKO_SHOW_THEME=<id>`, which calls the same
`theme::set_active` the real live preview calls — the real paint path, not a
capture-only shortcut. Driving the actual `Themes` mode for each screenshot
would have meant synthesising Down-arrow keystrokes, which this repo's standing
rule forbids.

**The defect this found**: the first seventeen captures included two rendered in
the *default* palette. `main.rs`'s startup `Request::GetTheme` reply was landing
hundreds of milliseconds after the hook had applied its theme, and winning.
Fixed by having that fetch yield when `evidence::show_theme()` is set. It is
evidence-path-only — nothing else writes the theme at startup — but it is
exactly the class of thing that would have shipped as "two of the screenshots
look wrong and nobody knows why".

**How each capture was then proven to be its own theme**, rather than eyeballed:
the selected row is painted at full alpha, so it is the one large area whose
pixels are the token value. Sampling it and classifying against all seventeen
`surface_selected` values put every capture on its own palette, with one
genuine near-collision — Dracula `#44475a` and Catppuccin Mocha `#45475a`
differ by one unit in one channel — which the `surface_panel` sample (`#282a36`
vs `#1e1e2e`) separates unambiguously.

---

## 7. Measurements

Both taken on the release binaries under the isolated `HOME` above.

### Warm summon latency — A/B against the pre-change binary, not regressed

`NEKO_BENCH=15` on both release binaries, same isolated `HOME`, back to back.
The baseline is `96fc807` (this branch's own merge base) built from a `git
worktree` into a separate target dir, so it is the *same machine, same
session, same load* rather than a figure quoted from an earlier report:

| Binary | Cold (summon 0) | Warm (1–14): min / median / max / mean |
|---|---|---|
| `96fc807` (before) | 38.19 ms | 0.84 / 4.63 / 8.07 / **4.05** ms |
| this branch | 25.92 ms | 0.82 / 4.85 / 8.16 / **3.52** ms |

Indistinguishable — the 0.5 ms difference is well inside this window's own
~8 ms `CVDisplayLink` tick quantisation (`AGENTS.md`, "Summon latency"), and the
cold difference is one sample each. Nothing this task added is on the summon
path: the palette is a static table and the theme is applied once at startup,
not per summon.

### Idle CPU — A/B, and the honest caveat

Panel visible, nothing happening, `ps -p <pid> -o %cpu=` sampled once a second
for 12 s. Runs rotated `base → neutral → latte` so a drifting machine load
cannot land on one arm:

| Binary / theme | samples |
|---|---|
| `96fc807` (before) | 1.07%, 0.36%, 0.39%, 0.83% |
| this branch, `neutral` (dark) | 0.36%, 0.67%, 1.58% |
| this branch, `catppuccin-latte` (light) | 0.67%, 1.64%, 0.94% |

**The three distributions interleave completely** — there is no signal
separating them, which is the result that matters twice over: the theme system
costs nothing at rest, and a *light* theme (the one that exercises the
`NSAppearance` path) costs nothing extra either.

The absolute numbers are higher than `AGENTS.md`'s standing ~0.16% figure, and
this report does **not** claim otherwise: this machine was running several other
agents' release builds throughout, and that figure was taken on a quiet one.
Which is exactly why the comparison above is against a binary measured *here*,
in the same rotation, rather than against a number from a different day.

---

## 8. What was not done

* **No geometry, spacing or type changed.** Every `*_PX` constant in `theme.rs`
  is untouched, and no theme can reach them.
* **No user-supplied palettes.** `BUILTIN_THEMES` is compiled in on both sides.
  A file-loaded theme would need a real format, validation, and an answer for
  "what happens when a loaded theme fails contrast" — none of which this task
  was scoped for.
* **No per-theme geometry**, which is what Sherbet from the cozy study would
  have needed (radii 16/8/6 → 22/12/8) and what a gradient direction like Ember
  would want to use properly. Ember and Catnap ship as flat-fill palettes here;
  their `--grad-from`/`--grad-to` tokens are not used, since a `linear_gradient`
  on the panel is a rendering change, not a colour one.
* **Onboarding was not re-screenshotted.** It reads the same tokens through the
  same `theme::active()` and compiles against the new table, and its window
  installs no material — but no window-scoped capture of it in a non-default
  theme was taken this pass.

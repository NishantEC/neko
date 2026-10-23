## Themes

`fm/neko-themes`, from the captain's own *"can we suport like themes section
where people can select?"*. **Read `crates/neko/src/theme.rs`'s module doc
comment before touching anything colour-related** — it is the normative
statement of the contract below. `docs/evidence/themes-report.md` has the
licence audit, the full per-theme contrast table, the twenty AA corrections
with before/after numbers, all seventeen window-scoped screenshots, and the
A/B measurements.

### The token-table contract

Two kinds of token, behaving differently on purpose:

- **Geometry, spacing and type stay `pub const`.** A theme is colour and
  surface only — it cannot move a row, change a radius, or resize the panel,
  and nothing reads those through a theme.
- **Colour lives on `theme::Palette`**, one struct of `PALETTE_TOKEN_COUNT`
  (22) `Rgba` fields. Every paint site reads the active one:
  `theme::TEXT_PRIMARY` became `theme::active().text_primary`, unchanged
  otherwise. **There is no `if themed` branch anywhere**, and adding one would
  be the wrong fix for anything.

**A theme supplies values; it never redefines what a token means.** Enforced
structurally, not by convention: a built-in is authored as a private `Spec` of
*independent* colours, and `theme::build()` — one `const fn`, one place —
derives every dependent token (`surface_panel_translucent` from
`surface_panel`; `menu_glass_tint` from `surface_raised`;
`text_tertiary_on_selected` *is* `text_secondary`, the promotion rule;
`border_hairline_strong` is the hairline hue at exactly double alpha;
`banner_danger_bg`/`state_*_border` from the state colours;
`row_icon_socket_bg` from one socket hue). A theme that wanted the strong
hairline *weaker* than the plain one cannot express it. `theme::tests::
every_theme_derives_its_relationship_tokens_from_its_base_colours` is what
makes an edit to `build()` itself fail loudly.

**Reading the active theme costs nothing per frame**: `theme::active()` is one
`Ordering::Relaxed` `AtomicUsize` load plus a slice index into
`&'static [Theme]` — no allocation, no lock, no `Arc` clone. Relaxed is
sufficient because the data it indexes is immutable rodata, so no reader can
observe a half-published theme however the load and store are reordered; a
`Mutex` would put a lock on the render path and an `Arc<Palette>` swap would
put an atomic refcount increment there, both for no correctness gain. **Do not
"harden" this to `SeqCst` or a lock** without a concrete reason the doc comment
does not already answer.

### The seventeen built-ins, and the licence rule they live under

`neutral` (the default — byte-identical to the pre-`fm/neko-themes` palette, so
nobody's app changes appearance on upgrade), `ember` and `catnap` (this
project's own, from `data/neko-cozy-theme/mockups/`), plus Catppuccin ×4,
Gruvbox ×2, Solarized ×2, Nord, Tokyo Night (Storm), Rosé Pine ×2, Dracula,
Everforest Dark.

**Every vendored palette is MIT, verified from its own source on 2026-08-21 —
not assumed, and not taken from a badge.** The tree stays GPL-free. Full table
with links and *how each was checked* is in `docs/evidence/themes-report.md` §2;
the two entries worth knowing without opening it:

- **Gruvbox has no `LICENSE` file at all.** It declares MIT in its `README.md`
  ("License — MIT/X11") and in `package.json` (`"license": "MIT"`). Recorded
  that way rather than implying a licence file that does not exist.
- **Tokyo Night is vendored from the original
  `tokyo-night/tokyo-night-vscode-theme` (MIT), deliberately not from
  `folke/tokyonight.nvim`** — that popular port is Apache-2.0, fine but it
  would have been the only non-MIT entry for no gain.

Adding a theme means: one `Spec` in `theme.rs`'s `THEMES`, one `BuiltinTheme`
in `neko_protocol::BUILTIN_THEMES`, its licence row in the report, and its hex
pinned in `vendored_themes_match_their_pinned_upstream_and_shipped_hex`.
`theme::tests::themes_match_the_protocol_registry` fails if you do one side and
not the other.

### Contrast is a gate, not a report

`theme::tests::themes_all_pass_wcag_aa` fails the build if any of eleven
text-on-surface pairs in any built-in drops below **4.5:1**. All seventeen
pass all eleven today. Two pairs are deliberately outside the gate and both are
accounted for in the report: `text_tertiary` on `surface_selected` (fails in
*every* palette checked including the neutral one, 3.04:1 — solved by the
promotion rule, whose promoted pair *is* gated), and anything composited over a
live desktop through the translucent fill (wallpaper-dependent; measured
separately in `data/neko-native-material/report.md` §6).

`every_theme_has_a_visible_selection_step_away_from_its_panel` (≥1.30:1) exists
because contrast tuning otherwise "fixes" a failing text pair by walking
`surface_selected` back into `surface_panel` — an earlier pass of this task
produced exactly that (`rose-pine` at `#272532` on a `#191724` panel: AA text on
an invisible selection row). **The tuning policy, if you ever re-tune: move the
text, not the surface** — neko's own neutral palette already solves its one
failing pair that way — and only move a surface when the text has run out of
headroom toward white/black.

**20 of 187 vendored token values are lightness-corrected** (hue and chroma
untouched) because a terminal scheme is designed against a terminal's pairs, not
this app's. The pin test asserts *both* the upstream and the shipped hex and
that the count is exactly 20, so a future edit that quietly walks another value
away from upstream fails.

### Light themes: two things a token swap cannot do

- **The icon plate inverts.** `row_icon_socket_bg` was `text_primary` at 6% — a
  pale film, invisible on a pale surface (`data/neko-cozy-theme/report.md`
  called this out precisely). The *rule* is unchanged ("a low-alpha plate of the
  opposite polarity to the panel"); a light `Spec` supplies a dark socket hue.
  `every_theme_has_an_icon_plate_that_reads_against_its_own_panel` checks
  relative luminance rather than pinning a hex.
- **`NSAppearance` has to follow the theme.** The panel is translucent over a
  real `NSGlassEffectView`, and that view renders in the *window's* appearance —
  a cream panel over a `darkAqua` blur reads as a dark halo leaking through
  wherever the fill is thin. `material::set_window_appearance` sets it on the
  real `NSWindow` (so it inherits to whichever material installed *and* to the
  `⌘K` menu overlay); `material::window_appearance_name` reads it back, logged
  at every launch beside the existing material/shadow/style-mask readbacks.

`panel::Root::sync_window_appearance` reconciles it **once per frame in
`render`, one enum compare**, with the AppKit call only on a real change —
deliberately not at each of the four theme-change call sites (preview, commit,
`ThemeChanged` broadcast, startup read). The setter is *injected*
(`panel::AppearanceSetter`, the same shape `accessibility` already uses) for two
reasons, the second not optional: it makes the behaviour headlessly testable,
and **GPUI's test-platform window `unimplemented!()`s (panics, rather than
returning `Err`) on `window_handle()`**, so any unconditional native call from
`render` takes every panel test down with it. Every other native call in this
crate lives in `main.rs`, which tests never run; this is the first one on the
render path.

**Every built-in keeps translucency** (panel alpha 0.82–0.90) —
`every_theme_keeps_the_native_material_visible` asserts it. `Theme::
keeps_translucency` is the seam for an opaque theme; nothing uses it.

### The `Themes` command and mode

The second command/mode pair, and the proof `modes.rs`'s own "what a second
command has to implement" accounting was honest: it cost one `CommandSpec`
(`neko_core::commands`), one `ModeChrome` (`crate::modes`), and one `Provider`
(`neko_core::themes::ThemesProvider`, id `"theme"`). **Nothing in
`enter_mode`/`exit_mode`/`run_search`'s scoping branch or the wire protocol
changed to accommodate it.**

- **Live preview is the feature.** Arrowing, or typing to filter, applies the
  palette to the whole panel as the selection lands (`Root::
  preview_selected_theme`). `ActiveMode::restore_theme` holds what to put back;
  Escape restores it, Enter clears it and persists. `restore_theme` is `None`
  for a mode that does not preview themes — deliberately not "every mode
  records and restores it, which is a harmless no-op", because that turns
  leaving *any* mode into a global palette write.
- **Entering lands on the theme already in use**, not on whatever sorts first,
  so opening the list never repaints the app on its own. Gated on "the
  previously-highlighted row was not itself a theme", which is exactly the
  entering case (`enter_mode` does not clear `results`).
- **`has_detail: false`**, a real choice: the preview *is* the panel, so a
  second column would take 496px away from the thing being previewed. A mode
  with no detail pane now renders its list full-width with full rows
  (`render_mode_list(has_detail)`); the clipboard mode's 264px column is
  unchanged.
- **Each row's swatch previews its own palette** — `glyph_element` takes the
  row's `SearchItem::id` and resolves `Glyph::Palette` through
  `theme::theme_by_id`, falling back to the live palette for a row whose id
  names no theme (the `Themes` command itself). A lookup keyed on data already
  on the row, not a `match` on which provider produced it.
- **Persistence** rides the existing `settings` KV table
  (`neko_core::themes::{get_theme,set_theme}`), read at client startup via
  `Request::GetTheme` and fanned out by `Event::ThemeChanged`. **There is no
  `SetTheme` request** — `Request::Activate { kind: "theme", id }` already means
  "do this row's thing", which for a theme row is "remember it".
  A persisted id naming no built-in reads as the default rather than erroring;
  `theme::set_active` likewise returns `false` and leaves the live palette
  alone, so a corrupted cosmetic setting degrades to "keep what's on screen".

### Evidence hook, and the race it exposed

`NEKO_SHOW_THEME=<id>` (`evidence.rs`) renders a capture in a specific built-in
by calling the same `theme::set_active` live preview calls — the real paint
path, not a capture-only shortcut. Driving the real mode per screenshot would
need synthetic Down-arrows, which this repo forbids. Focus-neutral: it touches
no window state.

**`main.rs`'s startup `Request::GetTheme` reply yields when that hook is set.**
Not defensive coding — caught live: two of the first seventeen screenshots came
back in the default palette because the daemon's reply landed hundreds of
milliseconds after the hook had applied its theme, and won.

**Verifying a themed screenshot actually rendered its theme**: sample the
*selected row* (painted at full alpha, so its pixels are the token value) and
classify against every built-in's `surface_selected`. Do not sample the panel
background — the translucent fill composited over the material's own tint sits
4–16 units off the nominal hex, which is enough to misclassify. One genuine
near-collision exists (Dracula `#44475a` vs Catppuccin Mocha `#45475a`); the
`surface_panel` sample separates those two unambiguously.

### Anything that calls `theme::set_active` in a test must hold `theme::test_lock()`

`ACTIVE` is process-global. **One lock for the whole crate**, not one per
module: `cargo test` will happily run a `theme.rs` test and a `panel.rs`
theme-mode test at the same instant, and two separate mutexes do not stop them
stepping on each other's palette (confirmed the hard way — this task shipped two
mutexes first and chased the resulting flake). Same discipline
`text_field::tests::pasteboard_test_lock` established for the systemwide
`NSPasteboard`.

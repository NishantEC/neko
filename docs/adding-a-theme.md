# Adding a theme

Seventeen ship. Adding an eighteenth is four edits and one test run, and the
tests will tell you what is wrong.

Everything is in `crates/neko/src/theme.rs` and
`crates/neko-protocol/src/lib.rs`. Read `theme.rs`'s module doc comment — it is
the normative version of the contract summarised here.

## The contract

**A theme is colour and surface. Nothing else.** Geometry, spacing and type are
`pub const` and no theme can reach them. You cannot change a radius, move a
row, or resize the panel.

**A theme supplies values; it never redefines what a token means.** You write a
`Spec` of *independent* colours. One `const fn build()` derives every dependent
token from it:

| Derived token | Rule |
| --- | --- |
| `surface_panel_translucent` | `surface_panel` at `panel_alpha` |
| `menu_glass_tint` | `surface_raised` at `menu_tint_alpha` |
| `text_tertiary_on_selected` | *is* `text_secondary` — the promotion rule |
| `border_hairline` / `border_hairline_strong` | one hairline hue at `a` and `2a` |
| `banner_danger_bg` | `state_danger` at `BANNER_ALPHA` |
| `state_success_border` / `state_danger_border` | the state colour at `STATE_BORDER_ALPHA` |
| `row_icon_socket_bg` | one socket hue at `icon_socket_alpha` |
| `snap_guide` / `snap_guide_muted` | `text_primary` at `SNAP_GUIDE_ALPHA` and at half it |

A theme that wanted a *weaker* strong hairline cannot express it. Changing that
means editing `build()`, which is one place, reviewed once, rather than
eighteen palettes free to drift.

## The four edits

### 1. A `Spec` in `THEMES` (`crates/neko/src/theme.rs`)

Nineteen fields. Copy a nearby entry with the same appearance and replace the
values. Here is the shipped `catppuccin-latte`, a light one:

```rust
theme(Spec {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    appearance: Appearance::Light,
    surface_panel: 0xeff1f5,
    panel_alpha: 0.88,          // keep translucency; a test enforces it
    surface_raised: 0xe6e9ef,
    menu_tint_alpha: 0.68,
    surface_input: 0xe6e9ef,
    surface_selected: 0xccd0da,
    text_primary: 0x4c4f69,
    text_secondary: 0x53566b,   // corrected from upstream 0x5c5f77
    text_tertiary: 0x676a7f,    // corrected from upstream 0x6c6f85
    text_on_light: 0xdce0e8,
    keycap_shell_bg: 0xccd0da,
    state_success: 0x327c21,    // corrected from upstream 0x40a02b
    state_danger: 0xd20f39,
    hairline: 0x4c4f69,         // dark hue on a light theme
    hairline_alpha: 0.1,
    icon_socket: 0x4c4f69,      // dark hue on a light theme
    icon_socket_alpha: 0.07,
}),
```

Order in `THEMES` is the order the `Themes` mode lists them. `neutral` stays
first.

### 2. A `BuiltinTheme` in `BUILTIN_THEMES` (`crates/neko-protocol/src/lib.rs`)

```rust
BuiltinTheme {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    description: "Light · Catppuccin · translucent",
},
```

Same `id` and `name` as the `Spec`. `theme::tests::themes_match_the_protocol_registry`
fails if you do one side and not the other.

### 3. A row in the pin table

In `theme::tests::vendored_themes_match_their_pinned_upstream_and_shipped_hex`,
add `(theme_id, &[(token, upstream_hex, shipped_hex)])` covering all eleven
pinned tokens. For a palette of your own invention, upstream and shipped are
the same value.

**The test asserts the total number of corrections is exactly 20.** If your
theme needs one, bump that constant in the same commit and say why in the
comment above your `Spec`. That is the whole point of the assertion: a value
quietly walking away from upstream fails a test instead of shipping.

### 4. A licence row

If you vendored the palette from somewhere, add it to
`docs/evidence/themes-report.md` §2 with a link and how you verified the
licence. Every one of the eight upstream projects neko draws on is MIT, checked
from source rather than from a badge. Two entries there are worth reading
before you assume: Gruvbox has no `LICENSE` file at all and declares MIT in its
README and `package.json`; Tokyo Night is taken from the original VS Code theme
(MIT) rather than the popular Neovim port (Apache-2.0).

Keep the tree MIT. A GPL palette is not worth the licence conversation.

## The tests that will fail on you

Run `cargo test -p neko theme`. Four gates matter.

**`themes_all_pass_wcag_aa`** — eleven text-on-surface pairs in every theme
must clear **4.5:1**. Not a report, a build failure. The pairs:

```
text_primary   / text_secondary / text_tertiary   on surface_panel
text_primary   / text_secondary                   on surface_raised
text_primary                                      on surface_input
text_primary   / text_secondary                   on surface_selected
state_success  / state_danger                     on surface_panel
text_on_light                                     on text_primary
```

Two pairs are deliberately outside the gate, both accounted for in the
evidence report. `text_tertiary` on `surface_selected` fails in every palette
checked, including neko's own neutral one (3.04:1) — the promotion rule solves
it, and the promoted pair *is* gated. And nothing is checked against the
translucent fill composited over a real desktop, because that depends on the
wallpaper.

**`every_theme_has_a_visible_selection_step_away_from_its_panel`** —
`surface_selected` must be at least **1.30:1** away from `surface_panel`.

This test exists because of a real mistake. Contrast tuning will "fix" a
failing text pair by walking `surface_selected` back toward `surface_panel`,
and an earlier pass of this work shipped exactly that: `rose-pine` at `#272532`
on a `#191724` panel — AA-clean text on an invisible selection row. That trades
a readability defect for a worse usability one.

**The tuning policy: move the text, not the surface.** neko's own neutral
palette solves its one failing pair that way. Only move a surface when the text
has run out of headroom toward white or black.

**`every_theme_has_an_icon_plate_that_reads_against_its_own_panel`** — see
below.

**`every_theme_keeps_the_native_material_visible`** — `panel_alpha` must stay
translucent. Every built-in sits between 0.82 and 0.90. `Theme::keeps_translucency`
is the seam for an opaque theme; nothing uses it.

Any test that calls `theme::set_active` must hold `theme::test_lock()`.
`ACTIVE` is process-global, and there is **one** lock for the whole crate, not
one per module — `cargo test` will run a `theme.rs` test and a `panel.rs`
theme-mode test at the same instant. Two separate mutexes do not stop them
stepping on each other's palette; that was shipped once and produced a flake.

## Two things a light theme has to get right

Both are cases where swapping token values is not enough.

**The icon plate inverts.** `row_icon_socket_bg` is the tinted backing plate
behind every row icon — it exists because real macOS icon assets bake in wildly
different amounts of transparent padding, and a consistent socket is what makes
a list of them read as one system. On dark themes it is a pale film. On a pale
surface a pale film is invisible. The *rule* is unchanged — a low-alpha plate of
the opposite polarity to the panel — so a light `Spec` supplies a dark
`icon_socket` hue. The test checks relative luminance rather than pinning a hex.

**`NSAppearance` has to follow the theme.** The panel is translucent over a real
`NSGlassEffectView`, and that view renders in the *window's* appearance. A cream
panel over a `darkAqua` blur reads as a dark halo leaking through wherever the
fill is thin. Setting `appearance: Appearance::Light` is all you do;
`panel::Root::sync_window_appearance` reconciles it once per frame in `render`
with one enum compare, and `material::set_window_appearance` makes the AppKit
call only on a real change.

## Why upstream values needed correcting at all

Twenty of 187 vendored token values are lightness-corrected. Hue and chroma are
untouched, and `build()`'s derivations were not bent to accommodate any of them.

The reason is simple: a terminal colour scheme is designed against a terminal's
pairs, not this app's. Solarized Light's `state_success` (`#859900`) is fine as
a syntax colour on a terminal background and is 2.97:1 against neko's panel.
Rosé Pine Dawn's `text_secondary` (`#797593`) is 2.71:1 against its own selected
row. Both fail a hard gate, so both moved in lightness only, to 4.74:1 and
4.62:1. The full table, with the pair that forced each one, is in
`docs/evidence/themes-report.md`.

## Seeing it

`NEKO_SHOW_THEME=<id>` renders a capture in a specific built-in by calling the
same live-preview path the mode uses. It touches no window state and takes no
keyboard focus.

```sh
NEKO_SHOW_ON_LAUNCH=1 NEKO_SHOW_THEME=catppuccin-latte ./target/release/neko
```

The process prints the real `NSWindow` number to stderr; capture with
`screencapture -l<windowNumber>`. Window-scoped only — never region or
full-screen, on any machine that has someone else's work on it.

To verify a screenshot really rendered its theme, sample the **selected row**.
It is painted at full alpha, so its pixels are the token value. Do not sample
the panel background: the translucent fill over the material's own tint lands
4–16 units off the nominal hex, which is enough to misclassify. One genuine
near-collision exists — Dracula `#44475a` against Catppuccin Mocha `#45475a` —
and the `surface_panel` sample separates those two.

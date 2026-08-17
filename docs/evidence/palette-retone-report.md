# neko — palette re-tone: monochrome with a hint of blue

The captain was offered three cat-derived identity directions (amber eye, jade eye, copper coat,
`data/neko-design/report.md` §1 "Open: the colour-identity pick") and chose none of them: *"lets do
monochrome with hint of blue."* This closes that open question for good — geometry, layout, type,
spacing, the fourteen-screen onboarding sequence, and the row anatomy are all untouched; only colour
values changed, plus a handful of raw colour literals moved into the token module (§3 below).

Same method as the original report: every value is an OKLCH triple run through Björn Ottosson's
OKLab→linear-sRGB matrices (independently re-implemented and cross-checked in `theme.rs`'s own test,
not ported from any reference), then WCAG-contrast-checked against every pair it's actually used
against. Hue held to 252°–257° across the whole neutral ramp (tighter than the original's 65°–75°
band) — a true blue (pure sRGB blue is ≈264° in this space), at low enough chroma that it reads as
neutral-with-a-cool-cast rather than a blue-tinted UI.

## 1. Base palette — before / after

| Token | Old OKLCH (warm) | Old sRGB | New OKLCH (cool) | New sRGB | Role |
|---|---|---|---|---|---|
| `surface_panel` | `oklch(0.16 0.014 70)` | `#110c07` | `oklch(0.16 0.014 255)` | `#090e13` | The one surface — input, list, footer |
| `surface_raised` | `oklch(0.20 0.016 70)` | `#1b150e` | `oklch(0.20 0.016 255)` | `#11161d` | Mouse hover only |
| `surface_selected` | `oklch(0.37 0.022 70)` | `#473e33` | `oklch(0.35 0.022 255)` | `#333b46` | Keyboard selection — opaque, not translucent |
| `text_primary` | `oklch(0.93 0.020 75)` | `#f0e6da` | `oklch(0.93 0.020 257)` | `#e0e9f6` | Row titles, input text |
| `text_secondary` | `oklch(0.72 0.025 70)` | `#afa294` | `oklch(0.735 0.025 255)` | `#9faab9` | Subtitles, footer labels |
| `text_tertiary` | `oklch(0.58 0.022 65)` | `#84786d` | `oklch(0.615 0.024 252)` | `#7b8693` | Timestamps, placeholders |
| `state_success` | `oklch(0.72 0.150 145)` | `#61bd67` | unchanged | `#61bd67` | Permission granted, only |
| `state_danger` | `oklch(0.68 0.160 35)` | `#e96e50` | unchanged | `#e96e50` | Permission denied / hotkey inactive, only |
| `text_on_light` | (hand-picked) | `#14100a` | `oklch(0.17 0.014 255)` | `#0b1015` | Text on a `text_primary` fill (primary button label) |

`state_success`/`state_danger` are unchanged, per the brief's "re-tone them only if they now clash
with the cool base" — checked explicitly (§2) and they don't; a saturated green/red pair against a
blue-grey neutral base is a standard, legible pairing, not a clash.

`surface_selected`'s L moved from 0.37 to 0.35 — a deliberate, small adjustment, not a rounding
artifact. See §2's contrast-bug fix below for why.

**Removed**: `ACCENT` (`oklch` amber `#efa831`, "Direction A — Amber eye"). It was unused by any
paint path already; its entire reason to exist — a still-open identity-accent pick — is now closed
by the captain's own decision (no accent). Keeping a dead, now-incorrectly-documented warm constant
around was worse than deleting it; nothing referenced it (verified by grep before removal).

## 2. Contrast — measured, not assumed

Same WCAG 2 relative-luminance formula as the original report, computed against the *actual* sRGB
bytes each OKLCH triple resolves to (not inferred from L alone — chroma and hue do shift luminance
slightly even at matched L).

| Pair | Old ratio | New ratio | Floor | Verdict |
|---|---|---|---|---|
| `text_primary` / `surface_panel` | 15.78:1 | **15.83:1** | 7:1 (AAA) | beats old, AAA |
| `text_secondary` / `surface_panel` | 7.80:1 | **8.23:1** | 7:1 (AAA) | beats old, AAA |
| `text_tertiary` / `surface_panel` | 4.53:1 | **5.23:1** | 4.5:1 (AA) | beats old, real headroom (§4) |
| `text_secondary` / `surface_raised` | — | 7.72:1 | 4.5:1 (AA) | AAA |
| `text_tertiary` / `surface_raised` | — | 4.91:1 | 4.5:1 (AA) | AA, comfortable |
| `text_primary` / `surface_selected` | — | 9.25:1 | 4.5:1 (AA) | AAA |
| `text_primary` / `keycap_shell_bg` | 12.69:1 | 12.94:1 | 4.5:1 (AA) | beats old, AAA |
| `text_on_light` / `text_primary` fill | ~15.6:1 | 15.61:1 | 4.5:1 (AA) | AAA |
| `state_success` / `surface_panel` | — | 8.29:1 | 3:1 (UI) | fine, unchanged colour |
| `state_danger` / `surface_panel` | — | 6.28:1 | 3:1 (UI) | fine, unchanged colour |

**The known contrast bug — checked explicitly, and actually fixed this time.** The original report's
one documented defect: `text_tertiary` on `surface_selected` measures only 2.44:1 (failing AA),
worked around by promoting a selected row's accessory text to `text_secondary` instead
(`theme::TEXT_TERTIARY_ON_SELECTED`). Re-measuring that promoted pair in the *original* warm ramp
with this task's own independent OKLCH→sRGB implementation gives **4.1993:1** — the report called
this "passing," but it is in fact just under the 4.5:1 AA floor. In the new ramp the same promoted
pair (`text_secondary` on `surface_selected`) measures **4.81:1**, a genuine pass. That's what the
0.37→0.35 nudge to `surface_selected`'s L bought: not a fresh problem, a pre-existing near-miss
closed for real while everything else in this ramp was already being re-derived. The new ramp's raw
(unused) `text_tertiary`/`surface_selected` pair is 3.06:1 — still fails, exactly as the original
did, exactly why the promotion rule still exists and is still exercised.

## 3. Native material — why `text_tertiary` moved more than a hue swap

`data/neko-native-material/report.md` §6 measured this app's own worst-case material path (the
`NSVisualEffectView` Popover fallback `material.rs` uses when `NSGlassEffectView` isn't available)
reducing placeholder-text contrast to ~94% of the fully opaque case in that report's own sampling
(6.08:1 vs. 6.46:1 baseline). The original warm `text_tertiary` sat at exactly 4.53:1 against the
panel — already barely over the 4.5:1 AA floor before any translucency was applied, so that same ~6%
haircut would have put it under AA on the real fallback path (not the primary Glass path, which
measured closer to opaque at 6.31:1, but the fallback is real and reachable — see `material.rs`'s own
three-step chain). This ramp's `text_tertiary` now measures 5.23:1 opaque, which survives the same
worst-case reduction with margin (~4.92:1) rather than sitting exactly on the line. `text_secondary`
didn't need the same treatment — its opaque margin (8.23:1) already absorbs a 6% cut with room to
spare (~7.74:1) — so the fix is scoped to the tier that actually needed it, not a blanket lift of
both tiers. No screenshot exists proving *live* `BehindWindow` compositing on this machine (the same
standing capture-safety rule that blocked it for the original material task blocks it here too — see
`AGENTS.md`, "Window material"); this is the same reasoned-not-screenshot-verified posture that
section already documents for `SURFACE_PANEL_TRANSLUCENT`, applied to the one token whose margin
actually mattered.

## 4. Raw colour literals moved into the token module

Found by grepping every `.bg(`/`.text_color(`/`.border_color(` call in the `neko` crate for values
not already routed through `theme::`. Six call sites, all in `panel.rs` and
`onboarding/view.rs` — `panel.rs`'s and `onboarding/view.rs`'s only other colour-touching code already
went through named tokens.

| File : line (pre-change) | Literal | New token |
|---|---|---|
| `panel.rs:330` | `rgba(0xe96e5014)` | `theme::BANNER_DANGER_BG` |
| `onboarding/view.rs:872` | `rgba(0xf0e6da0f)` | `theme::ROW_ICON_SOCKET_BG` (already existed — same value, just duplicated inline instead of reused) |
| `onboarding/view.rs:891` | `rgba(0xf0e6da0f)` | `theme::ROW_ICON_SOCKET_BG` (same duplicate) |
| `onboarding/view.rs:903` | `rgba(0x61bd6759)` | `theme::STATE_SUCCESS_BORDER` |
| `onboarding/view.rs:904` | `rgba(0xe96e5059)` | `theme::STATE_DANGER_BORDER` |
| `onboarding/view.rs:930` | `rgba(0x2a221aff)` | `theme::KEYCAP_SHELL_BG` |

Two of the six (the `ROW_ICON_SOCKET_BG` duplicates) weren't even a new colour — they were
`TEXT_PRIMARY`'s hex reproduced inline at the same 6% alpha the existing token already uses, so this
also removes a real duplication risk (a future palette change to `ROW_ICON_SOCKET_BG` would have
silently missed those two spots). The other four needed genuinely new tokens
(`BANNER_DANGER_BG`, `STATE_SUCCESS_BORDER`, `STATE_DANGER_BORDER`, `KEYCAP_SHELL_BG`) since no
existing token carried the right alpha or, for the keycap shell, the right base colour at all —
each is derived by the same OKLCH method and documented in `theme.rs` at its definition.

## 5. Verification

- `cargo build --workspace`, `cargo test --workspace` (95 tests across all five crates, including
  `theme::tests::base_palette_matches_the_frozen_oklch_table`), and
  `cargo clippy --workspace --all-targets` are all clean.
- Verified on the release binaries (`cargo build --release --workspace`), from a fresh first-run
  state (`HOME` pointed at an isolated, disposable directory per run — nothing written to the
  captain's real clipboard history or settings), the path the captain actually runs.
- Window-scoped evidence (`screencapture -l<windowID>`, never region/full-screen, per the standing
  rule) for the summon panel and two onboarding screens, before and after:
  `palette-summon-panel-{before,after}.png`,
  `palette-onboarding-00-welcome-{before,after}.png`,
  `palette-onboarding-01-what-neko-needs-{before,after}.png`.
- Reaching the second onboarding screen for the "before"/"after" pair used no synthetic keystroke or
  screen-coordinate mouse automation (both forbidden by this task's standing rules) — a temporary,
  env-var-gated call to the same pure `Flow::advance_from_welcome()` the real "Continue" action
  invokes (added, used, then reverted before this commit; not part of the committed diff) drove the
  step transition directly, and a temporary `eprintln!` of the window number (via the existing,
  untouched `material::window_number`, same call `evidence.rs` already makes for the summon panel)
  supplied the ID `screencapture -l` needed. `material.rs` itself was not modified.
- No new dependency; no change to `crates/neko/src/material.rs`.

## 6. What didn't change

Geometry, spacing, type, copy, and the fourteen-screen sequence are untouched — every constant in
`theme.rs` below the colour tokens (radii, heights, widths, onboarding clearances) is byte-identical
to before this task. `state_success`/`state_danger` are unchanged values. The one rule carried from
Helm — chrome is monochrome, state is coloured — still holds: selection (`surface_selected`) never
borrows a semantic hue, and the only two genuinely coloured tokens in the file remain
permission-granted/denied.

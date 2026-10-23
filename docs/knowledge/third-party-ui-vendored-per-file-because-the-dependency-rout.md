## Third-party UI: vendored per file, because the dependency route is closed

**Re-measured 2026-08-25 and the gap has widened: 81 compile errors, up from 13.**
gpui-component targets the crates.io `gpui`; this repo targets the `wingleeio`
fork at a pinned rev. Added as-is: *"multiple different versions of crate
`gpui`"*. Forced onto one gpui with `[patch.crates-io]`: 44 × `E0061` (arity —
`focus(window)` is now `focus(window, cx)`), 13 × `E0432` (`gpui::Corner` and
`gpui::Timer` do not exist in the fork), 8 × `E0599`, 7 × `E0308`
(`ScrollHandle::max_offset` is `Point` here, `Size` there), 4 × `E0063`
(`BoxShadow` gained `inset`). **A plain `cargo check` passes in the
multiple-versions state and is a false positive** — it only fails once something
calls a component. Do not re-run this; vendor per file instead.

**`refs/comet` is the source that actually compiles.** It pins the *identical*
gpui rev (`e2ddcc6`, the same line in both `Cargo.toml`s) and is MIT, so its
components need no repair at all. `loungy` pins gpui git-main, `t3code` has no
`gpui` line, `waku`/`codux` are GPL — read-only regardless. **Prefer comet over
gpui-component for anything either could supply.**

**The scrollbar is vendored and live.** Neko's root list and mode list both
scrolled with no thumb (`grep -c scrollbar` returned 0). `components/vendor/
gpui_component/scrollbar.rs` is upstream's element with the drift repaired;
`components/scroll.rs` owns the mounting. **It must not be a child of the
scrolling container**, which is how gpui-component's own `ScrollableElement`
helper mounts it: gpui applies scroll offset by wrapping the whole child list in
`window.with_element_offset` (`gpui/src/elements/div.rs:1851`) with no exemption
for absolutely positioned children, so a bar mounted inside slides off the top
of the viewport exactly when the list is long enough to need one. `with_scrollbar`
wraps instead, making the bar a sibling in the viewport's own coordinate space.

Behaviour, verified on screen in two themes (`docs/evidence/scrollbar-*.png`):
absent at rest, appearing while the offset changes — including keyboard-driven
`scroll_to_item` — and fading after 3s. Measured in the thumb's own column: dark
theme `rgb(71,71,71)` on a `(20,20,20)` panel, light theme `(188,191,182)` on
`(244,239,223)`, both 6pt wide, neither overrunning the rounded corner. **The
fade and the bar answer different questions** and both are mounted: the fade says
"there is more", the bar says "how much, and where you are".

**`components/vendor/gpui_component/shim.rs` is the substrate, and it is
reimplemented rather than copied** — no gpui-component source is in it. Every
vendored component reaches for the same library-private traits (`ActiveTheme`,
`StyledExt`, `Sizable`, `AxisExt`), so they are supplied once against
`theme::active()`. That is what makes a vendored component theme-reactive for
free: it calls `cx.theme()` exactly as upstream and gets neko's live palette.
**Roles neko's palette lacks are derived, never added as tokens** — `warning` is
the midpoint of the success→danger OKLCH ramp; the thumb alphas come off
`text_primary`. `PALETTE_TOKEN_COUNT` is unchanged. Two mappings are refusals:
`shadow` is always transparent (neko draws no box shadows — "The panel shadow
tent"), and the scrollbar track is transparent (a filled track would be an opaque
stripe through a translucent panel).

**`skeleton.rs` and `spinner.rs` were declined even under an explicit mandate to
overwrite neko's own components.** Each is under 70 lines and each *is* an
`Animation::new(..).repeat()` — the thing comet's own measured incident forbids
(one repeating element pinned a window at 120Hz, 36% CPU). `motion::PulseClock`
is the sanctioned alternative and already exists, so vendoring these would import
the bug and then delete their only content.

### The original evaluation (superseded above)

The captain asked twice about component libraries. Recorded so nobody
re-runs it:

- **`rust-ui.com` (Leptos) and `dioxus.rust-ui.com`** are Tailwind/DOM
  component registries. neko has no DOM and no CSS engine — `div()` here is a
  layout struct compiled to Metal draw calls. Adopting either means putting a
  webview under neko. **Rule: a UI library is only a candidate for this repo
  if it targets GPUI.** That one line rules out nearly all of them.
- **`longbridge/gpui-component`** (Apache-2.0, 13.3k stars) does target GPUI,
  and **still cannot be used** — measured both ways, not argued:
  - added as-is → `error[E0308]`, with the compiler's own note *"there are
    multiple different versions of crate `gpui` in the dependency graph"*.
    It depends on **crates.io** `gpui`; neko runs the **wingleeio fork**.
    A plain `cargo check` **passes** in this state and is a false positive —
    it only fails once something actually calls a component.
  - forced onto one gpui via `[patch.crates-io]` → **13 × `error[E0432]`**,
    `no Corner in the root`, `no Timer in the root`. It is written against
    crates.io gpui 0.2.2 and the fork has drifted.

  The only remaining route is abandoning the fork, which costs
  `paint_backdrop_blur`, `EdgeFade`, native window drag, the
  `windowDidBecomeKey:` deadlock fix and the ~30ms summon patch. Not taken.

**`components/keycap.rs` is what came of it.** gpui-component's `kbd.rs` was
read and declined on evidence rather than on principle: ~250 of its 324 lines
are `format(Keystroke) -> String` and `binding_for_action` lookups, both of
which neko already has or does not need, and the valuable ~30 lines are a
styled div that must be rewritten against this app's tokens anyway.
Importing mostly-duplicate code to obtain a div is the worse outcome. neko's
version renders **one cap per key**, which is what Raycast does and what
makes `⌃⇧K` read as three keys rather than one string.

**Built — see "Icons: real SVGs, and the one that stays painted" below.** This
paragraph used to read "still open, and the better path than any library:
real icons via `gpui::svg()` … Lucide is MIT." The route was right and it was
taken. **Two corrections it forces on this section's own text**: Lucide is
**ISC**, not MIT (verified from its own `LICENSE`; permissive either way, but
the record was wrong), and gpui-component's published crate ships **zero**
`.svg` files — its `icon.rs` names paths a consuming app's `AssetSource` must
resolve, so adopting it would not have supplied icons even if the gpui-version
mismatch above had not already ruled it out.

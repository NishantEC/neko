# Vendored components

Two kinds of thing are listed here and they are not governed the same way.
**Code** is vendored only under the narrow exception `AGENTS.md`'s "Licence
rule" carves out — one file, adapted, attributed. **Assets** (icon geometry,
colour values) are a separate, audited category: nothing executes, nothing is
copied into a `.rs` file, and the upstream is pinned so a drift stays visible.
Palette values already live in that category (`theme.rs`, seventeen themes);
icons joined it here.

Mirrors the pattern used in the captain's hushbacks repo (`packages/ui`): external
component collections are vendored as adapted source under `src/components/vendor/<collection>/`,
listed here, rather than pulled in as a runtime dependency. Rationale for neko specifically: our
visual system (`data/neko-design/report.md`) is frozen and bespoke, so we want the underlying
mechanics without a component library's own theming/styling opinions riding along, and without
upstream churn on every `cargo update`.

Evaluated source: [`gpui-component`](https://crates.io/crates/gpui-component) 0.5.1, Apache-2.0
(`THIRD_PARTY_LICENSES/gpui-component-APACHE-2.0.txt`, verified from its own `LICENSE-APACHE` file,
not a badge). It is licence-clean — no `ztracing`/`zlog` anywhere in its 757-package resolved graph. It was
rejected as a Cargo dependency twice, for two different reasons, and the second one is now the
binding one:

* **When this repo still used the published `gpui`** the objection was cost: 757 resolved packages
  for three mechanics, a second Objective-C bridging stack (`cocoa`/`cocoa-foundation`) beside the
  `objc2` one `gpui` and `neko-core` already use, and 60+ components' worth of styling assumptions
  to fight against a frozen bespoke geometry.
* **Since the fork migration it simply does not compile.** gpui-component targets the crates.io
  `gpui`; this repo targets `wingleeio/zed` at a pinned rev. Added as-is you get
  *"there are multiple different versions of crate `gpui` in the dependency graph"*; forced onto one
  gpui with `[patch.crates-io]` you get **81 errors** of genuine API drift, re-measured 2026-08-25:
  44 × `E0061` (arity — `focus(window)` became `focus(window, cx)`), 13 × `E0432` (`gpui::Corner`
  and `gpui::Timer` do not exist in the fork), 8 × `E0599`, 7 × `E0308` (`ScrollHandle::max_offset`
  is `Point` here and `Size` there), 4 × `E0063` (`BoxShadow` gained `inset`), 1 × `E0433`.

  **A plain `cargo check` passes in the multiple-versions state and is a false positive** — it only
  fails once something actually calls a component. So the dependency route is closed until either
  gpui-component targets the fork or this repo leaves it, and leaving it would cost
  `paint_backdrop_blur`, `EdgeFade`, native window drag, the `windowDidBecomeKey:` deadlock fix and
  the inactive-window summon-latency patch.

Vendoring per file, repairing the drift once each, is therefore the only route — and it was already
the preferred one on cost grounds.

| Our name | Category | Upstream file (gpui-component 0.5.1) | Status |
|---|---|---|---|
| `blink_cursor::CursorBlink` | Text input mechanics | `src/input/blink_cursor.rs` | **Vendored**, adapted in `blink_cursor.rs` |
| `scrollbar::Scrollbar` | Scroll affordance | `src/scroll/scrollbar.rs` | **Vendored**, repaired in `scrollbar.rs` |
| `shim` (`ActiveTheme`, `StyledExt`, `Sizable`, `AxisExt`) | Substrate | `src/{styled,theme/*}.rs` | **Reimplemented**, not copied — see below |
| — (list virtualization) | Result list | `src/list/{list,cache,delegate,list_item}.rs` | **Declined** — see below |
| — (rich text input engine) | Text input | `src/input/{state,element,movement,rope_ext,text_wrapper}.rs` | **Declined** — see below |
| — (`icon.rs` / the `IconName` set) | Icons | `src/icon.rs`, `src/icons/*.svg` | **Declined as code, replaced by assets** — see "Icons" below |

## Icons — assets, not code

**Source: [Lucide](https://github.com/lucide-icons/lucide), pinned at commit
`33a44aa8b0b43d9b0ed14eb08860a1b5550a1573` (2026-08-20).**

**Licence: ISC — verified on 2026-08-23 by reading the repository's own
`LICENSE` file at that commit, not a badge and not a package manifest.**
`AGENTS.md`'s "Third-party UI, re-evaluated" section previously recorded
"Lucide is MIT"; that was wrong, and the correction is the reason this row
says how it was checked. ISC is a permissive, MIT-equivalent grant requiring
only that the copyright and permission notice be preserved, which
`THIRD_PARTY_LICENSES/lucide-ISC.txt` does verbatim. The same file's second
half carries Feather's MIT notice for the subset of icons derived from it —
of the nine vendored here, `chevron-left`, `clipboard`, `link` and `search`
are on that list.

| Vendored file | Used for |
|---|---|
| `chevron-left.svg` | the mode input row's back affordance (`panel::back_glyph`) |
| `clipboard.svg` | `Glyph::Clipboard` |
| `file.svg` | `Glyph::File` |
| `folder.svg` | `Glyph::Folder` |
| `link.svg` | `Glyph::Link` |
| `search.svg` | the input row's magnifier (`panel::search_glyph`) |
| `sliders-horizontal.svg` | `Glyph::Sliders` |
| `square-terminal.svg` | `Glyph::Agent` / `Glyph::AgentLive` |
| `text-align-start.svg` | `Glyph::Text` |

They live in `crates/neko/assets/icons/lucide/`, **byte-for-byte unmodified**,
compiled into the binary by `assets.rs`'s `include_bytes!` table and pinned by
length in that module's own tests. Size and colour are applied at the call
site, never by editing a file, so `curl`-and-`diff` against the pinned commit
stays a meaningful check.

**What was declined, and why it is not the same decision as the rows above.**
gpui-component's `icon.rs` is the natural thing to reach for — it is what
proved `gpui::svg()` viable in the first place. Read directly at
`~/.cargo/registry/src/index.crates.io-*/gpui-component-0.5.1/src/icon.rs`
(372 lines), it is: a ~120-variant `IconName` enum, a `path()` whose entire
body is a `match` mapping each variant to the literal string
`"icons/<kebab-name>.svg"`, and a `RenderOnce` wrapper that defaults an
un-sized icon to the window's own text size and otherwise forwards to
`gpui::svg()`. Nothing there is a mechanic worth importing: the mapping is
`assets::glyph_icon`, three lines shorter and keyed on the wire vocabulary
this app actually has, and the sizing wrapper would be rewritten against
neko's tokens anyway — the exact shape `keycap.rs` already declined `kbd.rs`
for.

**The published crate ships no icon files at all** — verified, not assumed:
`find` over the whole 0.5.1 registry checkout returns **zero** `.svg` files.
`IconName::path()` returns a path that a *consuming application's* own
`AssetSource` is expected to resolve. So even taking gpui-component as a
dependency would have left exactly the work this task did — find real SVGs,
write an `AssetSource` — still to do. Its icon names are Lucide's own
kebab-case set (`a-large-small`, `chevrons-up-down`, …), which is what
pointed at Lucide, but that is an inference from naming; the files were
fetched from Lucide directly, so provenance is first-hand either way.

## Vendored

**`blink_cursor.rs` → `CursorBlink`.** Self-contained (only depends on `gpui::{Context, Task,
Timer}`, no coupling to the rest of the crate). Implements the actual hard part of cursor-blink
correctness — an epoch counter that cancels a stale scheduled toggle when a new edit/move
supersedes it, plus a separate pause-then-resume delay — which is easy to get subtly wrong by hand
(stale timers double-toggling, or the cursor going invisible mid-keystroke). Wired into
`text_field.rs` as an observed child entity. Adaptation notes are in the file's own header comment
per Apache-2.0 §4(b).

## Declined, with reasons

**List virtualization (`src/list/`).** Our result list is server-ranked and capped
(`Request::Search { limit, .. }` in `neko-protocol`) — the client never renders more than the
footer's visible count (~8–10 rows), so there is no long-list scroll-performance problem to solve.
What's actually in `list.rs` is coupled to this crate's own `IndexPath`, `ListDelegate` trait,
`ActiveTheme`, and `h_flex`/`v_flex` layout macros; vendoring it "cleanly" would mean vendoring
that whole supporting layer too, which reintroduces the styling-fights-our-tokens risk the
dependency route was rejected for, just via copy-paste instead of a `Cargo.toml` line. Our own
`panel.rs` renders the (small, bounded) row list directly against our token module.

**The rich text input engine (`src/input/{state,element,movement,rope_ext,text_wrapper}.rs`).**
This is a full multi-line, IME-composing, LSP-integrated editor built on a rope data structure
(note the `src/input/lsp/` subdirectory) — order-of-magnitude more machinery than a single-line
search field needs, and tightly self-coupled (`movement.rs` alone reaches into `InputState`,
`RopeExt`, `text_wrapper`'s display-point math — not extractable in isolated form). `text_field.rs`
in this crate already solves the problem we actually have (single-line `EntityInputHandler` input)
and was proven end-to-end in slice 1 at ~11–14ms warm summon before this task started; replacing it
would be strictly more risk for no capability we need.

**Keyboard/focus handling for row selection.** Considered vendoring `list.rs`'s
`SelectDown`/`SelectUp`/`Cancel`/`Confirm` action handling, but it's ~15 lines of index
clamp/wrap logic once separated from `IndexPath`/`ListDelegate` — not worth a vendor entry.
Hand-rolled directly in `panel.rs` against our own `Vec<SearchItem>`.

## simple-icons — brand marks (CC0-1.0)

| file | source | licence |
| --- | --- | --- |
| `crates/neko/assets/icons/simple-icons/claude.svg` | simple-icons `icons/claude.svg` | CC0-1.0 |
| `crates/neko/assets/icons/simple-icons/googlegemini.svg` | simple-icons `icons/googlegemini.svg` | CC0-1.0 |

Pinned to upstream commit `c956d67dfa7c37ae65206fc0775b0c02d1e695c2`, vendored
byte-for-byte, verified 2026-08-24.

**CC0 waives copyright; it does not grant trademark rights** — and that is the
whole reason these sit apart from the Lucide set rather than beside it. The
Claude and Gemini marks remain trademarks of Anthropic PBC and Google LLC.
neko uses them *nominatively*: to say which tool is running a given agent, on
a row describing that agent. See `THIRD_PARTY_LICENSES/simple-icons-CC0-1.0.txt`
for the full note, and revisit it before neko is ever distributed.

Unlike Lucide's stroke-only icons these are **filled**, which is correct here:
gpui renders an SVG to an alpha mask, so a filled path becomes the silhouette
in one tint — which is what a brand mark is.


## The scrollbar, and the shim under it

**`scrollbar.rs` is the one piece of this library with mechanics worth importing.** Neko's root
list and mode list both genuinely scroll (`overflow_y_scroll` + `track_scroll`) and drew no thumb
at all — `grep -c scrollbar` over `crates/neko/src` returned **0** before this. What a rewrite
would have had to reproduce is not the drawing but the bookkeeping: thumb length and position from
viewport-over-content, drag tracking with a grab offset, the wheel/hover state machine, and the
idle fade-out.

Repairs from upstream, all forced by the fork and all listed in the file's own header: the four
`Bounds::from_corner_and_size(Corner::…)` calls became explicit origin arithmetic, the one
`Timer::after` became `cx.background_executor().timer(..)`, `content_size` composes `Point` and
`Size` by hand, and `ScrollbarShow` lost its `serde`/`schemars` derives because neko persists no
scrollbar setting. The mechanics are untouched.

**`shim.rs` is reimplemented rather than vendored**, and the distinction matters for the licence
position: no gpui-component source is copied into it. Every vendored component reaches for the same
few library-private traits — `ActiveTheme`, `StyledExt`, `Sizable`, `AxisExt` — so those are
supplied once, backed by `theme::active()`, instead of being patched out of each file. That is what
makes a vendored component theme-reactive for free: it asks `cx.theme()` for a colour exactly as it
did upstream and gets neko's live palette, across all seventeen themes, with no per-theme asset and
no `if themed` branch.

Roles gpui-component's theme has and neko's palette does not are **derived, never added as
tokens** — `warning` is the midpoint of the success→danger ramp neko already computes in OKLCH, the
two scrollbar thumb alphas come off `text_primary`. Adding palette tokens to match a vendored
library's vocabulary would let the library dictate what a neko theme means, which is the opposite
of the arrangement.

Two mappings are deliberate refusals rather than translations. `shadow` is always fully transparent:
neko draws no box shadows, because both of the ones it used to draw were measured spilling into the
panel's own margin and removed (`AGENTS.md`, "The panel shadow tent"). The scrollbar `track` is
likewise transparent — the panel is translucent over a live native material, and a filled track
would be an opaque stripe through it.

### Declined from this library even under a mandate to overwrite neko's own components

`skeleton.rs` and `spinner.rs` are each under 70 lines and both were rejected on the same measured
ground: their entire mechanic is `Animation::new(..).repeat()`, which this project forbids after
comet's own recorded incident (one repeating element pinned a window at 120Hz, measured 36% CPU).
Neko already has the sanctioned alternative — `motion::PulseClock`, a shared 12.5Hz clock that
stops when nothing is using it — so vendoring these would mean importing the bug and then removing
the only thing they contain. `tooltip.rs` and `kbd.rs` remain declined as duplicates of
`panel::TextTooltip` and `components/keycap.rs`.


## comet — adapted, not copied

**Source: [`zeronsh/comet`](https://github.com/zeronsh/comet), MIT**, vendored
read-only at `refs/comet` (gitignored; see `refs/README.md`). It pins the
*identical* `gpui` rev this workspace does, which is why it is the preferred
source for anything either library could supply: its code needs no API repair at
all, where every gpui-component file needs four kinds.

| Our name | Upstream | Status |
|---|---|---|
| `motion::HoverFades` + `mix` | `crates/ui/src/motion.rs` | **Adapted** — the store, its frame-counter staleness rule, and the premultiplied blend |
| `motion::CubicBezier`, `MotionSpec` | same | Independently written earlier; comet's curve values are the reference |
| `motion::PulseClock` | same | Independently written earlier from comet's recorded 36%-CPU finding |
| `loaders.rs` (the zeron mark, gradient spinner) | `crates/ui/src/loaders.rs` | **Declined** — the shapes are comet's own identity, not a general mechanic |
| `frost.rs`, `edge_fade.rs`, `popover.rs` | same | Already reimplemented natively in earlier work |
| `markdown/*`, `notify.rs`, `sound.rs` | same | **Not yet** — real gaps, sized in `AGENTS.md` |

**What was actually taken is the hover-fade mechanic**, and it is a mechanic
rather than a component: gpui's `.hover()` applies its style on the frame the
pointer enters and offers no hook to interpolate, so a wash that fades has to be
driven by hand from wall time. comet solved that once; reproducing it from
scratch would have meant rediscovering the same three non-obvious parts — the
re-anchor on direction reversal, the premultiplied blend, and the frame-counter
liveness stamp that prunes an element which unmounted while hovered and will
never receive its leave event.

Two things were changed rather than carried over. The store's frame counter is
**rate-limited** here (`MIN_TICK_INTERVAL`), because this app has two windows
that render independently and two advances inside one real frame would let each
prune the other's live entries — comet drives a single shell. And `set_target_at`
is new: comet only ever drives this from hover events, while the Preferences tab
selection is state re-derived on every render, which the event form would
re-anchor every frame.

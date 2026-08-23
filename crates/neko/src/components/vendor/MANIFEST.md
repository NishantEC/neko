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
not a badge). Its own `Cargo.lock` was checked directly for the GPUI GPL hazard described in
`AGENTS.md`: no `ztracing`/`zlog` anywhere in its 757-package resolved graph — it depends on the
same published, GPL-free `gpui ^0.2.2` this repo uses, not git-`main`. So it is licence-clean, but
adopting it as a Cargo dependency (the option evaluated first) was rejected: 757 resolved packages
for three mechanics, a second Objective-C bridging stack (`cocoa`/`cocoa-foundation`) running
alongside the `objc2` stack `gpui` and `neko-core` already use, and 60+ components' worth of
styling assumptions to fight against our exact frozen geometry. Vendoring only the pieces that are
genuinely self-contained avoids all three costs.

| Our name | Category | Upstream file (gpui-component 0.5.1) | Status |
|---|---|---|---|
| `blink_cursor::CursorBlink` | Text input mechanics | `src/input/blink_cursor.rs` | **Vendored**, adapted in `blink_cursor.rs` |
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

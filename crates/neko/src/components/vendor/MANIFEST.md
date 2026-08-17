# Vendored components

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

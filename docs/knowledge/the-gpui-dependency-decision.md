## The GPUI dependency decision

**Reversed 2026-08-20, by explicit captain decision — this section
previously chose the opposite route specifically to avoid the cost this
reversal accepts. Read this whole section, not just the summary below, if
you're touching this dependency again.**

**Current state**: `crates/neko` depends on the `wingleeio/zed` fork of
`gpui` (plus its own `gpui_platform`), pinned by git rev
`e2ddcc6805f8c5088e62a60dfe517abcccd61a9a` — the same rev
`refs/comet` pins, chosen specifically so the primitives this
migration was taken for are known to exist at that exact commit. Pinned by
rev, never by branch. Full migration record, build-cost numbers, and the
four-primitive availability confirmation:
`docs/evidence/gpui-fork-migration-report.md`.

**What was decided, in the captain's own words**: shown verified evidence
that this fork's `gpui` unconditionally links three GPL-3.0-or-later crates
(`gpui → sum_tree → ztracing`, plus `ztracing`'s own `zlog`/`ztracing_macro`
dependencies) with no Cargo feature that avoids it, and that this is not
fork-specific — vanilla Zed's own `gpui` carries the identical chain,
tracked upstream as `zed-industries/zed#55470`, still open, unfixed — the
captain's response was *"its okay lets do it, i dont care about licence."*
Full evidence and decision record (outside this repo, firstmate home):
`data/neko-gpui-fork-licence/report.md`,
`data/neko-gpui-fork-licence/decision-adopt-fork.md`.

**Why the original decision (below) chose the opposite route**: the
published `gpui = "0.2.2"` crate on crates.io is genuinely Apache-2.0 with
no GPL edge — `sum_tree`/`ztracing` aren't part of what crates.io publishes
under that version, only present via a git dependency on the fork/mainline.
Depending on the published crate was the whole point: get gpui without the
GPL chain. This reversal gives that up on purpose, for capabilities the
published crate doesn't have and won't get without waiting on an unknown
future release.

**What it buys, concretely** (see `docs/evidence/gpui-fork-migration-report.md`
§4 for citations): `window.paint_backdrop_blur`/`BackdropBlur` (a real
Metal backdrop-blur scene primitive — `fm/neko-frost`'s own follow-up to
build a native frosted backdrop with, not attempted by the migration task
itself); `gpui::EdgeFade`/`window.with_edge_fade` (a scoped edge-fade
primitive, likewise left to `fm/neko-frost`); `Window::start_window_move()`
gaining a real mac implementation (`NSWindow.performWindowDragWithEvent:`)
instead of gpui's cross-platform no-op default — closes the "Window chrome"
section's own documented header-drag gap, though nothing wires it up yet;
and, empirically confirmed by driving the real summon/dismiss path 3 times
without a hang (not just read in source), a fix for the
`windowDidBecomeKey:` self-deadlock documented below in "A known,
upstream-fixed-but-unreleased deadlock in the real summon path" —
`window_did_change_key_status` (`gpui_macos/src/window.rs`) now drops its
state lock before calling back into AppKit's `resignKeyWindow`.

**The licence consequence, stated plainly**: a **built, distributed**
`neko` binary is no longer shippable under a clean MIT grant — GPL-3.0
obligations (including source disclosure) attach to the combined work once
it's distributed. This repo's own source remains MIT (see "Licence rule"
below — nothing here was relicensed), and the workspace `Cargo.toml`'s
`license = "MIT"` field and `NOTICE` both now carry an explicit caveat
saying so, rather than silently describing something no longer true of a
release binary.

**The obligation is real but currently dormant — GPL-3.0 triggers on
distribution, not on use.** neko ships **local-only**: no remote, no
releases, built and run on the captain's own machine, for himself. While
that stays true there is nothing to disclose and no obligation to
discharge — today's practical exposure is essentially zero. It becomes live
the moment a `neko` binary is ever distributed to anyone else. **Before
that day, revisit this with counsel** — the open questions (whether linking
functionally-inert GPL code creates a derivative work at all; an
unreferenced Apache licence file sitting in `ztracing`'s own directory
next to its GPL manifest declaration; `-or-later` semantics) are legal
judgment calls, not settled facts, and weren't resolved by the decision
above — only accepted as a known, live risk. **`refs/comet`
shipping MIT-declared binaries is not precedent for this** — an MIT grant
on comet's own code doesn't discharge an obligation attaching to a
distributed *combined* binary, and comet's own maintainer also owns the
`wingleeio/zed` fork, so comet's licensing choice isn't an independent
validation of the arrangement being clean.

**Practical rule going forward**: fine to build, run, and develop against
locally. Do not distribute a `neko` release binary to anyone — including a
GitHub release, a download link, or handing a built binary to another
person — without first resolving the open legal questions above, with
counsel. This is a standing constraint on this repo now, not a one-time
warning.

### The original decision (2026, superseded above)

`gpui` is Apache-2.0 as published on crates.io, but git-`main` currently links
a GPL-3.0-or-later crate (`ztracing`) through an unfixed dependency edge — see
`data/dim-licence/report.md`. **Chosen route, unchanged from slice 1: depend on
the published crate, `gpui = "0.2.2"` from crates.io — not git, no `[patch]`
block.** Re-verified again after clipboard history added `objc2-core-foundation`
as a direct dependency (`Apache-2.0 OR MIT OR Zlib`, already transitively
present via `gpui`/`objc2-app-kit`) — `docs/evidence/cargo-tree.txt` and
`docs/evidence/cargo-license.txt` are current as of that change: still no
GPL/AGPL anywhere in the tree, still only `self_cell`'s dual
`Apache-2.0 OR GPL-2.0` line, used under the Apache-2.0 arm.

**This verification is stale as of the 2026-08-20 reversal above** — it
described the dependency tree before the fork switch and no longer
reflects what `cargo tree`/`cargo license` actually report. Kept here as
the historical record the original decision was based on, not as current
fact.

To re-verify (now expected to show the GPL-3.0 chain, not flag one):

```sh
cargo license 2>/dev/null | grep -iE '\bgpl\b|agpl'   # now expected to show ztracing/zlog/ztracing_macro (GPL-3.0-or-later)
cargo tree | grep -i 'ztracing\|zlog'                  # now expected to show the gpui -> sum_tree -> ztracing chain
```

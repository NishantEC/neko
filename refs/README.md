# Reference clones — read-only

`refs/` holds seven third-party repositories, cloned here for reading. They are
not part of neko's build, are not vendored into it, and are not committed —
`.gitignore` keeps everything in this directory out of the repository except
this file. Only this README is tracked, because what it records is a rule, not
a checkout.

## The standing rule

**Read these for inspiration. Write every line ourselves.**

Study the architecture and the ideas, then reimplement in neko's own terms.
That holds even for the permissively-licensed ones: an MIT licence would permit
copying with attribution, and the captain's rule still says don't. See the
"Licence rule" section of `AGENTS.md` for how that has been honoured — every
line of source in this repository is written fresh, with one documented,
attributed exception.

## What is here, and what each one allows

| repo | licence | copying |
| --- | --- | --- |
| `comet` (zeronsh/comet) | MIT | permitted with attribution — but the standing rule is inspiration only |
| `loungy` (MatthiasGrandl/loungy) | MIT (verified from its own `LICENSE.md`, 2026-08-20) | permitted with attribution — inspiration only |
| `t3code` | MIT | permitted with attribution — inspiration only |
| `waku` (egoist/waku) | **GPL-3.0** | **forbidden — never copy a line.** Copyleft would infect neko |
| `codux` (duxweb/codux) | **GPL-3.0** | **forbidden — never copy a line.** Copyleft would infect neko |
| `agent-sessions` (jazzyalex/agent-sessions) | MIT (verified from its own `LICENSE`, 2026-08-22) | permitted with attribution — inspiration only. Swift/AppKit, so nothing is copyable in practice anyway |
| `paseo` (getpaseo/paseo) | **AGPL-3.0** (verified from its own `LICENSE`, 2026-08-24) | **forbidden — never copy a line.** Stricter than the two GPL entries below: AGPL's copyleft reaches *network use*, not only distribution |

The two GPL-3.0 entries are the reason this file exists. neko's own source is
MIT end to end and is meant to stay that way; a copied line from `waku` or
`codux` would attach copyleft obligations to the whole work, which no amount of
later attribution undoes. Read them, learn from them, close the file.

The separate question of what a *built binary* links — neko depends on a fork of
gpui that pulls GPL-3.0-or-later code through its own dependency chain — is not
about these clones at all. That decision, and the distribution constraint it
carries, is recorded in `AGENTS.md` under "The GPUI dependency decision".

## Why these five

- `comet` — GPUI, MIT, the closest match for interaction craft: floating menu
  layers, motion discipline, backdrop material. neko's actions-menu positioning
  and its `motion.rs` catalog were both designed against patterns read here and
  reimplemented from scratch.
- `loungy` — a WIP launcher in the vein of Raycast and Alfred, Rust on GPUI.
  The closest peer neko has: same framework, same product shape, same problems
  (global hotkey summon, a fast results list, extensions). Note that it depends
  on `gpui` as an *unpinned* git dependency on Zed main — read it for product
  and interaction ideas, not for dependency guidance.
- `t3code`, `waku`, `codux` — further GPUI applications, read for architecture
  and API shape only.

GPUI's own bundled `examples/` (Apache-2.0) are read the same way and live in
the gpui checkout, not here.

## Refreshing them

These are shallow clones (`--depth 1`) placed by hand. Nothing in the build
looks for them, so a missing or stale `refs/` breaks nothing; re-clone whichever
one you need to read. If you add a sixth, add its licence row to the table
above before you read a line of it.

## A note on `paseo`, because interop is not derivation

neko talks to Paseo — it reads `~/.paseo/agents/*.json`, opens `paseo://` deep
links, and could call the daemon's local HTTP API. **None of that creates an
AGPL obligation.** Copyleft attaches to code derived from the licensed work,
not to a separate program that speaks to it over a documented interface, and
AGPL's network clause governs offering *Paseo itself* over a network, not
being a client of it.

What would create one is copying source, or transliterating a non-trivial
implementation out of this checkout into `neko-core`. So the standing rule
applies here with more force than anywhere else in this table: read it to
learn what the daemon exposes, then write neko's own client from scratch.

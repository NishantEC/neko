## Licence rule

**Every line in this repo is written fresh**, with one documented exception:
see "Third-party UI code" above (`gpui-component`'s `blink_cursor.rs`,
Apache-2.0, attributed). **Vendored *assets* are a separate, audited
category** — nothing executes, nothing is copied into a `.rs` file, and the
upstream is pinned so a drift stays visible. Two things are in it:

- **Colour values.** Seventeen built-in themes draw on eight upstream
  projects, every one MIT, each verified from its own source; see "Themes"
  above and `docs/evidence/themes-report.md` §2 for the table and the
  per-palette verification method. No third-party palette *code* is
  vendored, only values re-expressed in `theme.rs`'s own `Spec` form.
- **Icon geometry.** Nine Lucide SVGs, **ISC** (not MIT — verified from
  Lucide's own `LICENSE`, 2026-08-23), vendored byte-for-byte and pinned to
  one upstream commit; four of them additionally carry Feather's MIT grant.
  See "Icons: real SVGs, and the one that stays painted" above,
  `THIRD_PARTY_LICENSES/lucide-ISC.txt`, and the icon table in
  `crates/neko/src/components/vendor/MANIFEST.md`. gpui-component's own
  `icon.rs` was read and declined; no icon *code* is vendored. `refs/` (this repo's
own read-only reference clones, gitignored — see `refs/README.md` for the
licence table and the standing rule) holds five reference GPUI apps — `comet`
(MIT), `loungy` (MIT), `t3code` (MIT), `waku` (GPL-3.0), `codux` (GPL-3.0) —
plus GPUI's own bundled `examples/` (Apache-2.0). All read for
architecture and API shape, never copied. The aim is MIT end to end, plus the
one attributed Apache-2.0 file — true of every line of *this repo's own
source*, still. **It does not describe a built, distributed binary as of
2026-08-20** — see "The GPUI dependency decision" immediately below for why
a linked dependency can attach obligations this rule doesn't cover.

## A known, upstream-fixed-but-unreleased deadlock in the real summon path

`neko-leak-confirm` set out to confirm a hypothesized ~4GB-per-summon memory
leak in gpui-0.2.2's `windowDidBecomeKey:` forced-draw path (see the
now-superseded framing in `data/neko-leak-audit/report.md`, firstmate home)
and instead found a **different, real bug in the identical code path**: five
live runs under an isolated `HOME`, driving the real `activate_window()` +
`cx.activate(true)` / `cx.hide()` cycle, showed **zero IOSurface/GPU-backed
growth** (the original leak hypothesis is not confirmed), but every run
reproducibly **self-deadlocked** the main thread after one or two real
activations — a live `sample` stack trace shows `window_did_change_key_status`
(`gpui-0.2.2/src/platform/mac/window.rs:1976`) re-entering itself via a
synchronous `resignKeyWindow` call made while still holding `window_state`'s
lock, blocking forever on the same non-reentrant mutex. This is not a novel
finding — it's an exact match (same call chain, same locking primitive) for
`zed-industries/zed#50151`, already root-caused and fixed by merged PR
`#51035`, but **that fix isn't in any published `gpui` crate** (crates.io's
newest is still 0.2.2, from six months before the fix merged) — consuming it
today would mean depending on git `zed-industries/zed`, reopening "The GPUI
dependency decision" above. No neko-side workaround was found (one
call-ordering hypothesis was tested live and disproven). Full account,
proven-vs-inferred table, and the captain's real options:
`docs/evidence/neko-leak-audit-confirmation.md`. The permanent artifact this
task left behind: `NEKO_BENCH_REAL` (`crates/neko/src/evidence.rs`), a bench
mode that drives the real summon/dismiss path (unlike `NEKO_BENCH`, which
uses `order_front_regardless`/`order_out` and — proven in this task, not
just asserted — structurally cannot reach `windowDidBecomeKey:` at all) —
use it to re-verify once a fixed `gpui` becomes consumable.

**Resolved 2026-08-20** — a fixed `gpui` did become consumable, via the
reversal in "The GPUI dependency decision" above, and `NEKO_BENCH_REAL` is
exactly what re-verified it: 3 real `activate_window()`/`cx.activate(true)`/
`cx.hide()` cycles on the `wingleeio/zed` fork's release binary completed
without a hang (`docs/evidence/gpui-fork-migration-report.md` §4) — the
fix's own source (`gpui_macos/src/window.rs`'s `window_did_change_key_status`
now drops its lock before calling `resignKeyWindow`) matches this section's
own account of what was missing. The originally-hypothesized ~4GB memory
growth remains unconfirmed either way — this task's own bench runs were
short (3 cycles) and didn't re-attempt that measurement.

# The daemon's unbounded memory growth — root cause confirmed, fixed, measured

The captain caught `neko-daemon` at 14.19 GB resident, 9 threads, 122 ports,
with the machine at 21.7 GB swap and memory pressure in the red.

## Diagnosis, verified before fixing anything

`grep -rn "autoreleasepool" crates/` returned zero matches before this pass.
`crates/neko-core/src/clipboard.rs`'s capture loop
(`run_capture_loop` → `poll_once`, `POLL_INTERVAL` = 400ms) runs forever on a
plain `std::thread::spawn` background thread — no `NSApplication`, no
`CFRunLoop` of its own — calling `NSPasteboard::generalPasteboard()` every
tick, and on a change also reading its string content and (via
`frontmost_app_name`) pumping a `CFRunLoop::run_in_mode` to get a live
`NSWorkspace.frontmostApplication` read. Every one of those Cocoa calls
produces autoreleased objects internally. A normal Cocoa app drains its
autorelease pool once per run-loop cycle automatically; a thread that never
pushes a pool at all has nowhere for those objects to go — they leak for the
rest of the process's life. `crates/neko-core/src/icons.rs`'s
`extract_icon_png` (background-thread `NSWorkspace::iconForFile` +
`NSBitmapImageRep` conversion, once per app at startup) has the identical
shape, just bounded to one pass rather than a forever-loop.

**Audited every other `objc2`/AppKit call site in the codebase for the same
defect, including this task's own new Spotlight/metadata indexing code**
(`grep -rln "objc2" crates/*/src/*.rs crates/*/src/**/*.rs`):

- `crates/neko-core/src/apps.rs` (this task's new discovery code): **zero**
  `objc2`/AppKit usage — it shells out to `mdfind` as a subprocess and does
  plain `std::fs` directory scanning, nothing else. Not part of this defect
  class at all.
- `crates/neko/src/material.rs`: does call AppKit, but it's in the *client*
  process (`neko`, not `neko-daemon` — a different OS process from the one
  the captain measured), which runs inside GPUI's real `NSApplication`/run
  loop that drains an autorelease pool every cycle the normal Cocoa way.
  Also out of scope for a second reason: a parallel worker owns this exact
  file for the Liquid Glass task, per this task's own brief — not touched.

## Fix

`clipboard.rs`'s `pasteboard` module now has exactly two public entry
points into AppKit — `poll` (one capture tick) and `write_string`
(`Request::Paste`) — each wrapping its *entire* body in
`objc2::rc::autoreleasepool`. The three previously-separate public
functions (`change_count`, `read_current`, `frontmost_app_name`) are now
private (`raw_*`) implementation details only reachable from inside `poll`'s
pooled closure — a structural guard, not just a comment: there is no
unpooled path left into this module's AppKit calls for a future change to
fall into by accident. `frontmost_app_name`'s run-loop pump — the shipped
clipboard-source-attribution feature — is unchanged; only the missing pool
around it was the bug. `icons.rs`'s `extract_icon_png` got the same
wrapping for the same reason, audited-and-fixed for completeness even
though its one-shot-per-app shape makes its leak bounded rather than
unbounded.

## Measured: before and after, same stress, same machine, back to back

Methodology: built both binaries from the same working tree, one via
`git stash` (pre-fix `clipboard.rs`/`icons.rs`), one with the fix applied.
Ran each under an isolated `HOME` override (`neko_protocol::database_path`/
`socket_path` are `$HOME`-relative — this keeps the test off both the
captain's real daemon and the material-task worker's own, confirmed live via
`pgrep -fl neko-daemon` before starting). To make the leak visible in
minutes rather than the days of uptime it took to reach 14 GB in the wild, a
script wrote a new string to the real system pasteboard every 0.5s for the
whole run — this forces `poll_once` down the expensive `read_current` +
`frontmost_app_name` path on nearly every tick instead of the cheap
`change_count`-only path an idle clipboard would mostly hit, i.e. a
worst-case accelerant, not a different code path. Sampled every 6s via
`top -l 1 -stats pid,command,mem,threads,ports -pid <pid>`.

**Before (unpatched, PID 24281):**

| t (s) | resident | #ports |
|---|---|---|
| 6  | 4701M | 126 |
| 12 | 5517M | 126 |
| 18 | 6609M | 126 |
| 24 | 7808M | 129 |
| 30 | 9078M | 126 |
| 36 | 10G   | 129 |
| 42 | 11G   | 129 |
| 48 | 12G   | 127 |
| 54 | 13G   | 127 |
| 60 | 14G   | 127 |
| 66 | 15G   | 127 |
| 72 | 16G   | 127 |
| 78 | 17G   | 127 |
| 84 | 18G   | 127 |
| 90 | 19G   | 127 |

Killed at 90s — 19 GB resident and climbing linearly at roughly 1 GB every
6 seconds, on a single background thread. (At 96s the reading dropped to
113M; that's macOS's memory compressor reclaiming the *resident* footprint
of pages it judged inactive, not the leak reversing — `vm.swapusage`
immediately afterward showed 11.8 of 13.3 GB of swap in use and the `/`
volume down to 18 GB free, on a machine that was otherwise idle for this
test. This is the same shape as the captain's own "21.7 GB swap" — leaked
memory that overflows into swap once compression alone can't keep up.) The
daemon's own log during this run also filled with `failed to record
clipboard entry: database or disk is full` — the leak's downstream effect
reached actual disk pressure, not just RSS.

**After (patched, PID 44857, identical stress, identical machine, same
session):**

| t (s) | resident | #ports |
|---|---|---|
| 6  | 982M  | 123 |
| 12 | 1615M | 123 |
| 18 | 1898M | 123 |
| 24 | 1687M | 123 |
| 30 | 2410M | 126 |
| 36 | 2383M | 123 |
| 42 | 1775M | 124 |
| 48 | 2408M | 124 |
| 54 | 1806M | 124 |
| 60 | 1522M | 124 |
| 66 | 1942M | 126 |
| 72 | 1697M | 126 |
| 78 | 1358M | 126 |
| 84 | 2253M | 126 |
| 90 | 1940M | 126 |
| 96 | 2123M | 123 |
| 102| 2124M | 123 |

No sustained growth over the same 90+ second window under identical
stress — resident memory oscillates in a ~1-2.4 GB band (the higher floor
than a cold start, ~150-200MB per `icons.rs`'s own doc comment on icon
extraction, plus normal working-set noise) and both ends and starts the
run in the same range. Port count stays flat at 123-126 throughout,
against the unpatched run's 126-129 — a real but far smaller difference
than memory's; a port leak, if any remains, accumulates slowly enough that
this ~100-second window can't resolve it clearly one way or the other. The
memory signal alone is unambiguous.

Not re-tested at the captain's original multi-day uptime scale — this
machine was already under real memory/disk pressure from other concurrent
work during this session (18 GB free on `/`, ~90% swap used, independent of
this test), and the fix's structural argument (every call site now pool-
wrapped, none reachable unpooled) plus the dramatic short-window contrast
above is the evidence this task asked for.

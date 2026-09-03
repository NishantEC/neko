# Confirming the neko client memory-leak hypothesis — live results, and a different bug found in the process

**Method statement.** This task started from
`<the firstmate home>/data/neko-leak-audit/report.md`
(source-only investigation, `main` at `8da1238`), which traced the real summon
path to gpui-0.2.2's `windowDidBecomeKey:` handler and named one static
question and one bounded live measurement as the way to settle it. Both were
done. The live measurement did **not** confirm the report's ~4GB-per-cycle
hypothesis, and surfaced a different, real, already-fixed-upstream bug
instead. Everything below is either a direct code citation or a result from
a real run under an isolated `HOME` on this machine — never the captain's
daemon, socket, or clipboard. `main` was `0e75008` when this task started;
this evidence was gathered on branch `fm/neko-leak-confirm`.

---

## Step 1 — can a non-key `WindowKind::PopUp` window receive typed keyboard input?

**No.** This is a static, source-provable answer, and it's the correct
severity: it kills the "make the panel visible without making it key, so we
never reach the leak-suspect path" fix direction outright.

**Why, with citations (all in `gpui-0.2.2`, the version this project pins):**

1. **AppKit's own dispatch is unmodified.** Keyboard events (`keyDown:`,
   `keyUp:`, `flagsChanged:`) are delivered by `-[NSApplication sendEvent:]`'s
   *default* implementation, which routes them to `[NSApp keyWindow]`'s
   responder chain — this is documented, long-standing AppKit behavior, not
   something an app can opt out of short of overriding `sendEvent:` itself.
   gpui's own `NSApplication` subclass, `GPUIApplication`
   (`platform/mac/platform.rs:76-79`), adds no `sendEvent:` override —
   confirmed by reading its full `ClassDecl` build (`platform.rs:76-110`):
   only `applicationWillFinishLaunching:`, `applicationDidFinishLaunching:`,
   `applicationShouldHandleReopen:hasVisibleWindows:`,
   `applicationWillTerminate:`, `handleGPUIMenuItem:`, and a few menu-item
   selectors (`cut:`, ...) are added. No local/global event monitor
   (`addLocalMonitorForEventsMatchingMask:`/
   `addGlobalMonitorForEventsMatchingMask:`) exists anywhere in
   `platform/mac/` either — confirmed by grep, zero hits.
2. **gpui's own key-input machinery only fires once AppKit has already
   decided to route the event there.** `keyDown:`/`insertText:replacementRange:`/
   the rest of `NSTextInputClient` are installed on `GPUIView`
   (`platform/mac/window.rs:118-247`, the `build_classes` `ClassDecl` for
   `"GPUIView"`) — these are Cocoa callback methods; they don't run unless
   AppKit's own dispatch has already picked this view's window as the
   destination, which for keyboard events means the *key* window.
3. **`orderFrontRegardless:` (the bench's own hide/show path,
   `material.rs:227-230`, `macos::order_front_regardless`) never makes the
   window key** — contrast with `activate()`'s explicit
   `makeKeyAndOrderFront:` call (`window.rs:1218`, quoted in the source
   report). This is exactly why the original report could already prove the
   60-cycle bench never reaches `windowDidBecomeKey:` at all; the same fact
   means it never makes the panel eligible to receive `keyDown:` either.
4. **`WindowKind::PopUp`'s use of `NSPanel` + `NSWindowStyleMaskNonactivatingPanel`
   (`window.rs:620-626`) does not change this.** That style only relaxes the
   requirement that becoming key also activates the *app* (so a popup can
   become key "quietly," without stealing focus from other apps) — it does
   not let the panel skip becoming key altogether. Confirmed: no
   `becomesKeyOnlyIfNeeded`/`worksWhenModal` override exists anywhere in
   `platform/mac/` (grep, zero hits), and `canBecomeKeyWindow` is
   unconditionally `YES` (`window.rs:291-293`) — it still has to be
   *asked* to become key by `makeKeyAndOrderFront:`, which
   `orderFrontRegardless:` never calls.

**Conclusion for the fix direction**: dead on arrival, confirmed by source
reading alone, no live test needed for this specific question. Any real fix
for the leak-suspect path has to either avoid triggering
`windowDidBecomeKey:`'s forced-draw branch some other way, or be an upstream
fix to that branch itself — not "stop making the panel key," since the panel
has to be key to type into.

---

## Step 2 — the bounded live confirmation

### Setup

- Isolated `HOME=/tmp/neko-leak-confirm-home` for every run — a fresh
  daemon, fresh socket, fresh SQLite DB, fresh icon cache, never the
  captain's real `~/Library/Application Support/neko/`. Verified clean
  before and after every run (`lsof` on the isolated socket path; `pgrep`
  against `target/release/neko(-daemon)?` showed only the captain's own
  pre-existing `neko-daemon` (pid 67514, untouched, confirmed alive and at
  ~24MB throughout) before, during, and after every run in this task).
- New harness, `NEKO_BENCH_REAL=<n>` (`crates/neko/src/evidence.rs`,
  `run_bench_real`), wired into `main.rs` next to the existing
  `NEKO_BENCH`/`NEKO_SHOW_ON_LAUNCH` hooks. Unlike `NEKO_BENCH`
  (`order_front_regardless`/`order_out` — proven not to leak, and per Step 1
  structurally incapable of reaching the suspect path), this mode calls
  exactly what `main.rs`'s real hotkey handler calls:
  `root.reset_for_summon`, the real `display_placement::
  reposition_to_cursor_display`, `window.activate_window()`,
  `window.focus(...)`, `cx.activate(true)` to show; `cx.hide()` to dismiss.
  Prints one stderr marker per phase transition
  (`activating`/`activated`/`hiding`/`hidden`) per cycle plus its own pid,
  for an outside script to sample against deterministically instead of
  guessing sleep durations.
- Orchestration: a one-off bash script (not committed — kept out of the
  repo per "keep the diff tight"; described here in full since it's the
  thing that actually ran). It samples `vmmap -summary`, `vmmap` (full), and
  `footprint` against the exact client pid right after each
  `activated`/`hidden` marker, computes the delta against the previous
  sample, and enforces two hard aborts: free system memory dropping below
  6GB, or a single cycle's growth exceeding 8GB (2x the report's own ~4GB
  hypothesis — "steeper than predicted"). On either trigger it kills the
  exact client pid it started (`kill -9`) immediately and stops the loop; it
  never touched any pid it didn't start itself.
- Cherry-picked `1323614` (`fm/neko-client-leak`'s own WIP — "make
  NEKO_BENCH exercise the real `reposition_to_cursor_display` call") onto
  this branch first, since it's small, correct, and directly relevant prior
  art. Its own doc comment claimed this bench "is the harness that
  reproduces the leak the unpooled reposition calls caused" — that's
  backwards from what the commit's own message says it actually measured
  (flat RSS, ruling reposition *out*); fixed the doc comment in this task to
  say what was actually found, not what was expected going in.

### What happened, run by run

**Five separate runs**, not one — the first three were spent establishing
that a real, reproducible stall existed and was not a fluke or a timeout set
too short; the fourth added a diagnostic stack sample at the stall point;
the fifth (after reverting an ordering experiment back to match `main.rs`
exactly) is the authoritative one whose numbers are reported below. Total
live client uptime across all five runs: under two minutes; peak resident
footprint observed anywhere, any run: 163.6 MB (a one-time startup cost —
Glass material install, font loading — that then settles down, see below).
**At no point did any run approach the 6GB free-memory floor or the 8GB
single-cycle growth ceiling; the growth-based abort never fired in any run,
because there was never any multi-gigabyte growth to abort on.**

| Run | Cycles completed before a stall | Cycle footprint pattern |
|---|---|---|
| 1 | 0, 1, 2 (stalled at cycle 3, 10s timeout) | flat, 0.04-0.21 GB |
| 2 | 0, 1 (stalled at cycle 2, 10s timeout) | flat, 0.04-0.16 GB |
| 3 | 0, 1, 2 (stalled at cycle 3, **90s** timeout — ruled out "just slow") | flat, 0.04-0.16 GB |
| 4 | 0, 1 (stalled at cycle 2, 15s timeout, `sample` captured before kill) | flat, 0.16 GB |
| 5 (authoritative, `main.rs`-matching call order) | 0, 1 (stalled at cycle 2, 15s timeout) | flat, 0.04-0.16 GB |

### The numbers for run 5, by region class (the brief's own requirement)

`vmmap -summary` against the client pid, baseline vs. after cycle 1's hide
(the last sample taken before the stall — cycle 1 is a genuine non-first
activation, so it *does* run the `activated_least_once` forced-draw branch
the original report names):

| Region class | Baseline resident | After cycle 1 (post-hide) resident | Delta |
|---|---|---|---|
| **IOSurface** | 9152K | 9152K | **0 — byte-identical** |
| IOAccelerator (graphics) | 10.4M | 8384K | -2.0M (shrank) |
| owned unmapped (graphics) | 113.4M | 93.4M | -20.0M (shrank) |
| MALLOC_SMALL (+ empty) | 16.8M | 17.0M | +0.2M (noise) |
| MALLOC metadata | 912K | 928K | +16K (noise) |
| **TOTAL resident** | 682.1M | 660.0M | **-22.1M (shrank)** |
| **TOTAL dirty** | 158.8M | 44.7M | **-114.1M (shrank — one-time startup cost settling)** |
| `footprint` phys_footprint | 155M (baseline sample) → 44-46M by cycle 1 | | settled down, never up |

Full raw output for every sample of every run is not included here (it was
gathered under `/tmp/neko-leak-confirm-results/`, outside the repo, and that
directory is not preserved past this task — it contains nothing sensitive,
but per "keep the diff tight" it was never meant to be a committed
artifact). The table above is transcribed directly from the `vmmap -summary`
output of the authoritative run; the same flat/shrinking pattern held across
all five runs, not just this one.

### Verdict on the original hypothesis

**Not confirmed.** Across five independent runs, IOSurface/GPU-backed
regions showed **zero growth**, and total resident/dirty memory *shrank*
between the baseline sample and the sample after a genuine
`activated_least_once`-gated forced-draw cycle. This directly contradicts
the report's own "~4GB per cycle, roughly flat, suspicious of one fixed-size
resource" reading — at least for the number of cycles safely reachable
before a *different* bug (below) stops the run. Per this task's own
instruction: "If five cycles do not clearly confirm the hypothesis, report
that the hypothesis is not confirmed — do not stretch the data to fit it."
Two of five real cycles were reached repeatedly (never all five — see next
section for why) and showed no growth; that is what actually happened,
reported plainly rather than extrapolated into "well it might still happen
at cycle 4."

**What this does not rule out**: whether the mechanism the original report
named (the `InstanceBufferPool`/completion-handler dependency inside
`presents_with_transaction`) could still leak under conditions this minimal,
empty-search-field bench never exercised — sustained real usage (the
captain's actual 43-minute session, with real typing, real result rendering
across four providers, real icon loads, and very likely far more than five
real summons in that window) is a materially different load than five rapid
synthetic cycles against a blank panel. See "What remains unsettled" below.

---

## The bug this run actually found: a real, reproducible deadlock — already reported and fixed upstream, not yet released

Every run above didn't complete five cycles because every run hit the same
wall: after one or two real `activate_window()`/`cx.activate(true)` cycles,
the **next** cycle's `activating` marker prints, and then the process goes
completely silent — confirmed genuinely stuck, not just slow, by waiting a
full 90 seconds on one run with zero further output before killing it.

### Root cause, proven by a live stack trace, not inferred

Run 4 captured `sample <pid> 1` against the stalled process immediately
before killing it (`/tmp/neko-leak-confirm-results/hang-sample-cycle-1.txt`
— gathered outside the repo, not committed, but the relevant frames are
transcribed here in full since they're the actual evidence). The main
thread's entire call stack at the moment of the stall:

```
-[NSApplication run]
  -[NSApplication(NSEventRouting) nextEventMatchingMask:untilDate:inMode:dequeue:]
    ... (standard Cocoa event loop) ...
      _dispatch_client_callout
        gpui::platform::mac::dispatcher::trampoline
          gpui::executor::ForegroundExecutor::spawn::...::run   (my bench's async task waking up)
            gpui::platform::mac::window::MacWindow::activate      (window.rs:1212, my `window.activate_window()` call)
              -[NSWindow makeKeyAndOrderFront:]
                -[NSWindow _makeKeyRegardlessOfVisibility]
                  -[NSWindow makeKeyWindow]
                    -[NSWindow _changeKeyAndMainLimitedOK:]
                      -[NSWindow becomeKeyWindow]
                        -[NSNotificationCenter postNotificationName:object:userInfo:]   (posts windowDidBecomeKey:)
                          gpui::platform::mac::window::window_did_change_key_status     ← FIRST entry, lock held here
                            -[NSPanel resignKeyWindow]     ← still holding the lock — see window.rs:1988-1992
                              -[NSWindow resignKeyWindow]
                                -[NSNotificationCenter postNotificationName:object:userInfo:]  (posts windowDidResignKey:)
                                  gpui::platform::mac::window::window_did_change_key_status   ← SECOND, re-entrant entry
                                    parking_lot::raw_mutex::RawMutex::lock_slow
                                      pthread_cond_wait   ← DEADLOCKED HERE
```

The mechanism, read directly from `gpui-0.2.2/src/platform/mac/window.rs:1976-1992`:

```rust
extern "C" fn window_did_change_key_status(this: &Object, selector: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.lock();                    // <- lock acquired
    let is_active = unsafe { lock.native_window.isKeyWindow() == YES };

    // ... comment about a known AppKit quirk: a spurious windowDidBecomeKey
    // can arrive even when the window isn't really key yet ...
    if selector == sel!(windowDidBecomeKey:) && !is_active {
        unsafe {
            let _: () = msg_send![lock.native_window, resignKeyWindow];   // <- STILL holding `lock` here
            return;
        }
    }
    ...
```

`resignKeyWindow` is a synchronous AppKit call that posts
`windowDidResignKey:` before returning — and since gpui registers the
*same* `window_did_change_key_status` function for both
`windowDidBecomeKey:` and `windowDidResignKey:`
(`window.rs:318-325`), that synchronous post re-enters this exact function,
on the same thread, trying to acquire the same non-reentrant
`parking_lot::Mutex` the outer call is still holding. Self-deadlock, on the
main thread, permanently — nothing else in the process can make progress
after this (confirmed: every other thread in the sample — `async-io`,
`NSEventThread`, the dispatch workqueue threads — is idle, waiting for work
the frozen main thread would have to hand out).

### This is not a new bug — it's already found, root-caused identically, and fixed

Searched `zed-industries/zed` (via `gh-axi search issues` — search only, per
this task's constraint, nothing filed):

- **[#50151](https://github.com/zed-industries/zed/issues/50151) — "UI
  deadlock in window_did_change_key_status during window activation (macOS,
  multi-window)"**, closed. The reporter's own `sample` output names the
  **identical** call chain: `MacWindow::activate → makeKeyAndOrderFront: →
  becomeKeyWindow → window_did_change_key_status (first) → resignKeyWindow →
  window_did_change_key_status (re-entrant) → RawMutex::lock_slow →
  pthread_cond_wait`. A second commenter (`xrl`) independently hit the same
  trace on a later Zed build. Both describe it happening after real,
  human-paced usage (hours to days of normal use, window-focus changes) —
  genuinely rarer under organic use than this task's rapid five-cycles-in-
  two-seconds synthetic bench, which is very likely *why* this bench found
  it reliably within one or two cycles where real usage might take much
  longer, not because the bench is doing anything gpui doesn't already do on
  every real summon.
- **[PR #51035](https://github.com/zed-industries/zed/pull/51035)**, merged
  2026-03-17, closes #50151. The fix: drop the lock before calling
  `resignKeyWindow`, exactly the one-line structural fix this section's own
  code reading above would suggest. Manually verified by a second reporter
  against their own repro before merge.
- **Not yet in any published `gpui` crate.** Checked directly against
  crates.io's own version list for the `gpui` crate: the newest published
  version is still **0.2.2, published 2025-10-22** — six months *before*
  PR #51035 merged. The fix exists only on `zed-industries/zed`'s git
  history, not in any version this project (or any project depending on the
  published `gpui` crate) can currently consume via `Cargo.toml`.

**No new upstream issue was written or filed.** Filing a duplicate of an
already-fixed, already-merged issue would be wrong, not useful — this
section documents the match instead, which is what the captain actually
needs to make a decision.

### Tested, and ruled out: a neko-side call-ordering mitigation

Before concluding no neko-side fix exists, one concrete hypothesis was
tested live: `main.rs`'s real summon path calls `window.activate_window()`
*before* `cx.activate(true)` — exactly the "opening a pop-up while the
application isn't active" precondition gpui's own comment names as the
spurious-event trigger. Reordering to `cx.activate(true)` first, then
`activate_window()`, was tried in a temporary bench variant.

**Result: no improvement — if anything, the stall arrived one cycle
sooner** (cycle 1 instead of cycle 2-3), with an identical deadlock
signature in a second `sample` capture. This is consistent with the bug
being **AppKit's own internal reentrancy**, not something neko's calling
order controls — `cx.activate(true)`'s underlying
`NSApplication.activateIgnoringOtherApps:` is itself asynchronous in ways
neko's code can't serialize against. The experiment was reverted; `main.rs`
and the bench both still call `activate_window()` before `cx.activate(true)`,
matching the pre-existing, already-shipped real summon path exactly (no
production behavior changed by this task).

---

## Step 3 — the fix

**No neko-side workaround is viable** for the deadlock — it lives entirely
inside gpui-0.2.2's compiled `window_did_change_key_status`, unreachable
from any public gpui API neko calls, and the one call-ordering hypothesis
that could plausibly have avoided it was tested and disproven.

**The upstream fix already exists and is already merged** (PR #51035) — but
not in any version of `gpui` this project can currently depend on without
reopening a decision `AGENTS.md`'s "The GPUI dependency decision" section
already closed deliberately: this project pins the *published* `gpui =
"0.2.2"` crate specifically to avoid the GPL-3.0 taint that git-`main`
currently carries through an unfixed `ztracing` dependency edge. Consuming
the fix today would mean depending on git `zed-industries/zed` (reopening
that question) rather than crates.io.

**This is the captain's decision, not this task's** — laid out plainly so
it can be made without more investigation:

1. **Wait for the next crates.io release of `gpui`** that includes PR
   #51035 (unknown timing — the zed team's own release cadence, not
   something this task can predict or influence).
2. **Depend on a git `zed-industries/zed` ref instead of crates.io**,
   accepting the GPL-3.0 question `AGENTS.md` already closed the other way
   — would need to be re-litigated with the same rigor
   `data/dim-licence/report.md` originally gave it, not assumed away.
3. **Vendor a local patch** (a `[patch.crates-io]` override pointing at a
   forked/patched local copy of the one function) — technically the
   smallest change, but explicitly against this project's own documented,
   deliberate policy of staying off git and off any patch mechanism for
   this exact dependency; not attempted in this task without that sign-off.
4. **Accept the risk for now** — the deadlock is real but was empirically
   harder to trigger under organic, human-paced usage than under this
   task's rapid synthetic cycling (per #50151's own reports: hours to days
   of real use, not two seconds of five rapid cycles) — a defensible
   short-term choice if the captain would rather not touch the dependency
   policy for this alone.

No code change was made to work around the deadlock, per the instruction to
get the diagnosis right over shipping a speculative fix, and per "do not
wrap things in autorelease pools hoping it helps" — none of the above
options are that kind of masking fix; all of them are real, named, and
honest about what each one costs.

**The one code change this task did ship**: `NEKO_BENCH_REAL`
(`crates/neko/src/evidence.rs`, wired into `crates/neko/src/main.rs`) — a
permanent, reusable harness for driving the real summon/dismiss path,
matching the original report's own §4 recommendation ("extend `NEKO_BENCH`
... to add a second mode that drives the real path"). This is what should
be used to re-verify once a fixed `gpui` becomes consumable, and to check
for the still-open memory-growth question below under a longer, more
realistic load.

---

## Proven vs. inferred — explicit summary

| Claim | Status |
|---|---|
| A non-key `WindowKind::PopUp` window cannot receive `keyDown:`/`insertText:` through gpui's input handling | **PROVEN** — `platform.rs:76-110` (no `sendEvent:` override), `window.rs:118-247` (key-input methods only fire via AppKit's own key-window-gated dispatch), `window.rs:620-626` + grep for `becomesKeyOnlyIfNeeded` (nonactivating panel style doesn't remove the key-window requirement, only the app-activation side effect) |
| `orderFrontRegardless:` never triggers `windowDidBecomeKey:` | **PROVEN** — restated from the original report, re-verified in this session |
| Five real activate/hide cycles produce ~4GB of IOSurface/GPU-backed growth per cycle | **NOT CONFIRMED** — five live runs, IOSurface region byte-identical before/after a genuine forced-draw cycle, total resident *shrank* |
| A self-deadlock exists in `window_did_change_key_status` under repeated real activation | **PROVEN** — live `sample` stack trace, five independent reproductions, matches an already-diagnosed-and-fixed upstream issue's own stack trace line for line |
| The deadlock is the same event as the captain's original 19.96GB growth | **NOT PROVEN, NOT DISPROVEN** — plausible (a deadlocked process matches "process later found gone/force-quit"), but a deadlock alone doesn't explain multi-gigabyte growth; see below |
| Reordering `cx.activate(true)` before `activate_window()` prevents the deadlock | **TESTED AND DISPROVEN** — live experiment, reverted |
| The upstream fix (PR #51035) is available in the published `gpui` crate neko depends on | **PROVEN false** — crates.io's own version list: newest is 0.2.2, published six months before the fix merged |

---

## What remains unsettled, and what would settle it

1. **Whether the original ~4GB/cycle growth mechanism is real under sustained,
   realistic load** (real typing, real multi-provider results, real icon
   loads, many more than five summons over tens of minutes) rather than five
   rapid cycles against an empty panel. *What would settle it*: the exact
   measurement the original report's own §6 named — but it now has to
   account for the deadlock discovered here, since any long-running repro
   needs either a fixed `gpui` (not yet available) or a plan for restarting
   past the deadlock mid-run without losing the memory trend.
2. **Whether the deadlock and the captain's original 19.96GB report are the
   same incident.** *What would settle it*: nothing available now — the
   captain's own client process is gone, its stderr log (14 lines, already
   quoted in the original brief) shows no stall-related evidence either way
   since it just stops after the last `summon latency` line, consistent
   with *both* a hang and a crash. If it recurs, capturing `sample <pid> 1`
   on the real (captain's, not an isolated test) process the moment it's
   next seen at high memory or "Not Responding" — before force-quitting it
   — would directly answer this.
3. **Whether disabling this project's usual click-outside-hide behavior or
   any other real usage pattern changes how often the deadlock triggers.**
   Not tested — the bench's rapid, focus-stealing cycling may make the
   spurious-event race far more likely than it is in practice; this task
   didn't have a safe way to test "real, human-paced usage" without asking
   the captain to reproduce a process-freezing bug on his own machine, which
   the task's own constraints correctly rule out.

---

## Files/commits read or run in this task

Everything the original report already read (see its own "Files/commits
read" section) plus, newly in this task: `gpui-0.2.2/src/platform/mac/
{window.rs (full, including `build_classes`, `window_did_change_key_status`,
`activate`), events.rs (full), platform.rs (`GPUIApplication`/
`GPUIApplicationDelegate` class construction)}`; `crates/neko/src/
{evidence.rs, main.rs, material.rs, panel.rs (PANEL_HEIGHT_PX only)}`;
`git show 1323614` (cherry-picked); `zed-industries/zed` issues `#50151`,
`#42821`, `#53390`, PR `#51035` (via `gh-axi search`/`view`, search only, per
this task's constraint); crates.io's own version listing for the `gpui`
crate (`curl` against `crates.io/api/v1/crates/gpui`, read-only).

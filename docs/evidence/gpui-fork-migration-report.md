# Migrating `crates/neko`'s gpui dependency to the `wingleeio/zed` fork

Full record for `fm/neko-gpui-fork-migration`. Read alongside `AGENTS.md`'s
"The GPUI dependency decision" (rewritten by this task — the licence record,
what was decided and why) and the source decision documents:
`data/neko-gpui-fork-licence/decision-adopt-fork.md` and
`data/neko-gpui-fork-licence/report.md` (firstmate home, outside this repo).

## 1. What changed

`crates/neko/Cargo.toml`'s `gpui` dependency moved from the published
`gpui = "0.2.2"` crate to a git dependency on `https://github.com/wingleeio/zed`,
pinned by rev `e2ddcc6805f8c5088e62a60dfe517abcccd61a9a` — the same rev comet
pins (`refs/comet/Cargo.toml`), on the captain's own explicit
instruction ("exactly what comet uses, so the primitives are known to exist
at that revision"). A new direct dependency, `gpui_platform` (same rev, same
features comet takes: `wayland`, `x11`, `font-kit`, `runtime_shaders`), was
also added — this fork's `gpui::Application::new()` no longer exists; the
real bootstrap goes through `gpui_platform::application()`, which selects
the mac platform backend (`gpui_macos::MacPlatform`) internally. Both are
pinned by commit, never by branch.

## 2. Compile fixes forced by API drift

None of these touch behavior — every one is a mechanical adaptation to a
renamed/reshaped API at this rev, confirmed against the fork's own source
under `~/.cargo/git/checkouts/zed-*/`:

| Old (published 0.2.2) | New (this rev) | Where |
|---|---|---|
| `gpui::Application::new()` | `gpui_platform::application()` | `main.rs` |
| `gpui::Timer::after(d).await` | `cx.background_executor().timer(d).await` | `main.rs`, `evidence.rs`, `onboarding/view.rs`, vendored `blink_cursor.rs` |
| `window.focus(&handle)` | `window.focus(&handle, cx)` | every call site |
| `Entity::update(cx, f) -> R` unchanged; `AsyncApp`/`App`-level `cx.update(f)` used to return `Result<R>` | now returns `R` directly (`WindowHandle::update`/`WeakEntity::update` are unchanged — still `Result<R>`) | `main.rs`, `evidence.rs`, `panel.rs` — 18 `let _ = cx.update(...)` bindings became clippy `let_unit_value` warnings, fixed mechanically |
| `TextLayout::paint(origin, line_height, window, cx)` | gained explicit `TextAlign`/wrap-boundary params: `paint(origin, line_height, TextAlign::Left, None, window, cx)` | `text_field.rs`'s `TextFieldElement::paint` |
| `gpui::Corner::BottomRight` | `gpui::Anchor::BottomRight` (identical variant names, renamed type) | `panel.rs`'s `render_actions_menu` — this specific fix was needed only after rebasing onto `fm/neko-craft-pass` (`a19cffb`), which built the actions-menu floating layer against `Corner` before this branch's own dependency switch landed |

Full diff is in the branch's own commit history (`f0a8032`, `ecc947c`,
`551d277`). No behavior changed; `cargo build`/`cargo test`/`cargo clippy`
are all clean at the workspace root (`0` warnings beyond one pre-existing,
unrelated dependency notice — `block v0.1.6`/`proc-macro-error2 v2.0.1`
"will be rejected by a future version of Rust," present on `main` before
this task too, nothing to do with `gpui`).

## 3. Coordination: rebasing onto `fm/neko-craft-pass`

`fm/neko-craft-pass` (the actions-menu `deferred`/`anchored`/`occlude`
rewrite) landed on `main` as `a19cffb` mid-task. Per the launch brief's own
instruction, this branch rebased onto it rather than duplicating work:

- `evidence.rs`: craft-pass had independently built its own
  `NEKO_SHOW_ACTIONS_MENU` hook (`Root::open_actions_menu_for_evidence`,
  routed through the real `open_actions_menu_for_selected_row`) for exactly
  the same reason this task needed one (a window-scoped actions-menu
  screenshot with no synthetic input) — this branch had built an equivalent,
  differently-named hook (`NEKO_OPEN_ACTIONS_MENU`) before the rebase. The
  duplicate was dropped entirely in favor of craft-pass's version during
  conflict resolution; nothing of this branch's own actions-menu tooling
  survives the rebase.
- `panel.rs`: craft-pass's own new code used `gpui::Corner::BottomRight`,
  written against the still-published crate before this branch's dependency
  switch existed on `main`. Fixed post-rebase (see the table above) — a real,
  if small, compile break the rebase itself introduced, not present in
  either branch alone.
- A genuinely interesting cross-confirmation: craft-pass's own
  `evidence.rs` doc comment independently documents the exact "window loses
  activation mid-capture, another concurrently-running agent's own `neko`
  client contending for key-window status" symptom this task also hit live
  while capturing screenshots (§5) — the same root cause, found
  independently by two different tasks running in parallel on this shared
  machine.

## 4. The four availability checks (step 3 of the launch brief)

All four checked directly against the fork's own source at
`~/.cargo/git/checkouts/zed-d032abea1bc23d84/e2ddcc6/crates/`, not assumed
from a changelog.

**1. `window.paint_backdrop_blur` / `BackdropBlur` — exists.**
`crates/gpui/src/window.rs:3992` (`Window::paint_backdrop_blur`, doc:
"Paint a within-window backdrop blur: everything already painted beneath
`bounds` is snapshotted and painted back gaussian-blurred… macOS Metal
only"); the scene primitive itself at `crates/gpui/src/scene.rs:621`
(`pub struct BackdropBlur`). Not used by this task — `fm/neko-frost` owns
building the real native frosted backdrop on top of this primitive, per the
launch brief's own instruction not to duplicate that work.

**2. `gpui::EdgeFade` / `window.with_edge_fade` — exists.**
`crates/gpui/src/window.rs:682` (`pub struct EdgeFade`, doc: "A scoped
vertical edge fade… primitives painted inside the scope get their opacity
multiplied by a ramp… Built for scroll-edge fades over translucent/blurred
window backgrounds, where a backdrop-colored gradient overlay cannot
exist"); `Window::with_edge_fade` at `crates/gpui/src/window.rs:3436`. Also
not used here — `fm/neko-frost` is told to keep its own edge-fade
implementation, not migrate it to this primitive, per the launch brief.

**3. `Window::start_window_move()` — has a real mac implementation.**
`crates/gpui/src/window.rs:2425` (`Window::start_window_move`, delegates to
`self.platform_window.start_window_move()`); the mac backend's own
implementation, `crates/gpui_macos/src/window.rs:1841`
(`MacWindow::start_window_move`), is a real, non-trivial body:

```rust
fn start_window_move(&self) {
    let this = self.0.lock();
    let window = this.native_window;
    unsafe {
        let app = NSApplication::sharedApplication(nil);
        let event: id = msg_send![app, currentEvent];
        let _: () = msg_send![window, performWindowDragWithEvent: event];
    }
}
```

This is exactly the "small, targeted native call
(`NSWindow.performWindowDragWithEvent:` from the header's mouse-down
handler, calling it from inside GPUI's own native mouseDown dispatch so
`[NSApp currentEvent]` is still the right event)" `AGENTS.md`'s "Window
chrome" section names as one of the two real fixes for onboarding's
non-draggable header — deferred there specifically because it wasn't
available on the published crate. It's available now. Not wired up by this
task (out of scope — "keep your own diff to the dependency switch"); the
seam is open for a follow-up.

**4. The `windowDidBecomeKey:` self-deadlock — fixed, confirmed two ways.**

*Source*: `crates/gpui_macos/src/window.rs`, `window_did_change_key_status`
(lines 2512–2536). The spurious-event workaround this handler implements
(documented in its own comment: "Cocoa sends a spurious `windowDidBecomeKey`
message to the previous key window… the following code detects the
spurious event and invokes `resignKeyWindow`") now drops the state lock
*before* calling back into AppKit:

```rust
extern "C" fn window_did_change_key_status(this: &Object, selector: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    let lock = window_state.lock();
    let is_active = unsafe { lock.native_window.isKeyWindow() == YES };
    lock.cursor_visible.store(true, Ordering::Relaxed);
    if selector == sel!(windowDidBecomeKey:) && !is_active {
        let native_window = lock.native_window;
        drop(lock);                                        // <-- the fix
        unsafe {
            let _: () = msg_send![native_window, resignKeyWindow];
        }
        return;
    }
    ...
```

This is exactly the shape `AGENTS.md`'s own account of the deadlock
describes as missing on the published crate — `window_did_change_key_status`
(`gpui-0.2.2/src/platform/mac/window.rs:1976`) "re-entering itself via a
synchronous `resignKeyWindow` call made while still holding `window_state`'s
lock." At this rev, the lock is dropped first.

*Live confirmation, not just source-read*: `NEKO_BENCH_REAL=3` (the real
`activate_window()` + `cx.activate(true)` / `cx.hide()` path — the one that
reliably deadlocked in 1–2 cycles on the published crate,
`docs/evidence/neko-leak-audit-confirmation.md`) ran 3 full cycles to
completion against this rev's release binary under an isolated `HOME`,
verified by a real daemon (`verify_harness`, never the real `neko-daemon`):

```
neko: real-bench pid 24777
neko: real-bench cycle 0 activating
neko: real-bench cycle 0 frame latency 61.741042ms
neko: real-bench cycle 0 activated
neko: real-bench cycle 0 hiding
neko: real-bench cycle 0 hidden
neko: real-bench cycle 1 activating
neko: real-bench cycle 1 frame latency 32.033083ms
neko: real-bench cycle 1 activated
neko: real-bench cycle 1 hiding
neko: real-bench cycle 1 hidden
neko: real-bench cycle 2 activating
neko: real-bench cycle 2 frame latency 11.142375ms
neko: real-bench cycle 2 activated
neko: real-bench cycle 2 hiding
neko: real-bench cycle 2 hidden
neko: real-bench complete (3 cycles)
```

No hang, no timeout, process exited cleanly. This is the one item of the
four that the migration was explicitly taken partly *for* — confirmed real,
not just present in the diff.

## 5. Live verification (step 2 of the launch brief)

All verification: release binaries, isolated `HOME` (`/tmp/neko-verify-home`,
removed after this task), the real `verify_harness` binary (never the real
`neko-daemon` — its clipboard capture loop is never started), an obscure
committed hotkey, no synthetic input anywhere (every screenshot driven
through `evidence.rs`'s own hooks: `NEKO_SHOW_ON_LAUNCH`, `NEKO_SHOW_QUERY`,
`NEKO_SHOW_CONFIRM`, `NEKO_CYCLE_MODE_ONCE`, `NEKO_SHOW_ACTIONS_MENU`),
window-scoped capture only (`screencapture -l<windowID>`).

- **Panel summons, apps render with real icons** —
  `gpui-fork-migration-app-search.png` ("chrome" → Google Chrome, real
  extracted icon, footer verb `Open ↵`).
- **Search across providers**: Commands + Clipboard together
  (`gpui-fork-migration-commands-clipboard-search.png`, "clip" — the
  `Clipboard History` command with its `COMMAND` badge, and the seeded
  clipboard fixture with its `TEXT` badge and relative-time accessory);
  Applications + System Settings together
  (`gpui-fork-migration-settings-search.png`, "bluetooth" — `Bluetooth File
  Exchange` app icon and the `Bluetooth` pane's System Settings icon, both
  real extracted icons). File search itself
  (`gpui-fork-migration-files-empty.png`) correctly returns "No matching
  results" for a fixture file seeded under `/tmp` — `mdfind` doesn't index
  `/tmp` (a pre-existing, documented limitation, `AGENTS.md`'s "File
  search" section — not a regression; `files::tests`' full suite, unaffected
  by this migration, passes and is the real coverage for this provider's
  logic).
- **Clipboard mode enters, shows its detail pane, and exits** —
  `gpui-fork-migration-mode-detail-pane.png` (confirming the `Clipboard
  History` command row: window widens to the real two-column
  `PANEL_WIDTH_WITH_DETAIL_PX` layout, "Today" grouping, back-arrow, and the
  detail pane's Application/Content Type/Copied fields, all rendering
  correctly) and `gpui-fork-migration-mode-exit.png` (Escape-equivalent
  dismiss restores the root list and the saved query).
- **The actions menu opens** — `gpui-fork-migration-actions-menu.png`
  (craft-pass's own floating `deferred`/`anchored(Anchor::BottomRight)`
  layer, Paste/Copy/Delete, `Delete` in the danger color) — this is also
  the live confirmation that this task's own `Corner` → `Anchor` rebase fix
  (§3) is correct, not just compiling.
- **Click-outside dismissal** — confirmed via the exact code path
  (`main.rs`'s `cx.observe_window_activation` → `!window.is_window_active()`
  → `cx.hide()`), triggered organically during capture (this shared
  machine's own real window-activation contention from other concurrently
  running agents' `neko` clients — the same cause craft-pass's own
  `evidence.rs` comment independently documents, §3) rather than by a
  literal click, since synthetic input is disallowed. The handler fired and
  called `cx.hide()` correctly both times it was observed.
- **Liquid Glass material installs and verifies** — printed and confirmed
  on every single run: `neko: material verified: NSGlassEffectView at
  contentView.subviews()[0] (below GPUI's rendering view, 2 total
  subviews) — readback style=NSGlassEffectViewStyle(0) cornerRadius=16`.
- **Spaces reachability reads back correctly** — printed and confirmed on
  every run: `neko: Spaces/full-screen reachability verified: readback
  bits=0x101 (NSWindowCollectionBehavior(257)) — reachable from every Space
  and over full-screen apps` (unchanged from `AGENTS.md`'s own documented
  value).
- **Native window shadow stays disabled** (the double-panel fix,
  `ad3967a`) — also confirmed on every run: `neko: native window shadow
  disabled (verified)`.
- **Never resizes the real `NSWindow` at runtime** — unchanged by this
  task; the mode-view resize-seam architecture (`AGENTS.md`, "Mode view
  resize seam") wasn't touched, and nothing in this migration's own
  verification exercised a runtime resize to check whether the fork fixes
  the underlying `viewport_size` staleness (out of scope per the launch
  brief: "do not change the architecture in this task" unless the fork's
  fix was going to be *relied on*, which it isn't here).
- **Full test suite** — `cargo test --workspace`: 237 passed, 0 failed
  (`neko`: 94, `neko-client`: 3, `neko-core`: 119, `neko-daemon`: 9,
  `verify_harness`'s own copy: 9, `neko-protocol`: 3 — the jump from 82 to
  94 in `neko`'s own suite is `fm/neko-craft-pass`'s tests, picked up by
  the rebase, not this task's own addition).

## 6. Build cost

Methodology: `cargo clean` (target dir fully removed) then a timed
`cargo build --release --workspace`, cargo's global registry/git caches left
warm (the realistic "a developer already has this fork checked out once"
case — re-cloning the fork itself from GitHub is a separate, one-time cost
below). Baseline measured the same way, same machine, back to back, via a
disposable `git worktree add --detach <path> ad3967a` (removed
immediately after) at `crates/neko/Cargo.toml`'s pre-migration state,
`CARGO_TARGET_DIR` pointed at an isolated directory (also removed
immediately after) so it couldn't clobber this branch's own build.

```
                          before (ad3967a,   after (this branch,
                          published gpui)    wingleeio/zed fork)
clean release build time  61.14s             53.48s
release-only target size  1.6 GB             1.8 GB   (+~200 MB, ~12%)
```

The fork build is not slower — likely more available parallelism at the
moment each ran (this is a shared, multi-agent machine) rather than a real
per-crate speed difference; not worth over-reading. Target-directory growth
is modest: ~200 MB more for a release-only build, dwarfed by other already-
large dependencies both trees share (`resvg`, `taffy`, `blade`/`metal`,
`rustybuzz`).

**One-time cost not captured above, since caches were warm for both
measurements**: the fork itself has to be fetched once, via cargo's normal
git-dependency mechanism (`git+https://github.com/wingleeio/zed`) — a
shallow-ish clone of the relevant commit graph, landing in
`~/.cargo/git/{db,checkouts}/`. Measured on this machine, already warm from
another concurrent task also depending on this same rev:
`~/.cargo/git/checkouts/zed-d032abea1bc23d84` alone is 444 MB (`~/.cargo/git`
total 465 MB). This is a **global cache, not per-worktree or per-branch** —
every worktree/branch on this machine that depends on this exact rev shares
one copy; it's paid once per machine, not once per `cargo build`.

**Combined target directory as this branch actually leaves it** (dev build
+ release build + `cargo test` artifacts, i.e. what a working session
really accumulates, not just the isolated release-only number above):
**6.5 GB**. This is the number closest to "how much bigger does my
`target/` get from working on this branch," and it's the one worth watching
if disk gets tight again — `cargo clean` reclaims all of it identically to
before this migration (nothing here changes how disposable `target/` is).

## 7. What this task deliberately did not do

Per the launch brief's own "if you have to choose": a clean, fully-verified
migration beats a half-migration that already reaches for the new
primitives. Confirmed available and cited (§4); **not used**:
`paint_backdrop_blur`/`BackdropBlur` (left to `fm/neko-frost`'s own
follow-up), `EdgeFade`/`with_edge_fade` (`fm/neko-frost` keeps its own
implementation), `start_window_move`'s real mac body (the onboarding
header-drag gap it would close is untouched — a real, scoped follow-up, not
attempted here). The `Corner` → `Anchor` fix in §3 is the one code change
this task made to a rendering path beyond the dependency switch itself, and
it's a forced compile fix from the rebase, not new primitive usage.

## 8. Real cost found, not hypothesized: warm summon latency regressed

`AGENTS.md`'s "Summon latency" section documents ~2.9–6.4 ms warm (mean
~4.1 ms) on the published crate, inside slice 1's own ~11–14 ms budget.
Re-measured on this migration's release binary, `NEKO_BENCH=15`, same
isolated `HOME`/daemon, same machine: **consistently 25.9–40.5 ms across
all 15 samples, no warm-up trend** (unlike the published crate, which
settles from an ~20 ms first sample down to 1–9 ms and stays there). To
control for this being shared-machine noise rather than a real regression,
the published crate was rebuilt from a disposable worktree
(`git worktree add --detach … ad3967a`) and bench'd *interleaved*, on the
same machine, against the same isolated daemon, in the same few minutes:

```
fork (this branch), NEKO_BENCH=15:  25.9, 34.1, 37.9, 40.5, 25.9, 34.2,
                                     37.0, 38.4, 34.2, 29.7, 29.6, 37.9,
                                     30.1, 26.0, 34.2  (ms)

published (ad3967a), NEKO_BENCH=10: 22.4 (cold), 8.2, 1.4, 2.7, 3.5, 4.0,
                                     8.4, 8.6, 9.2, 1.1  (ms)
```

The published build's numbers land inside its own documented range; the
fork's don't warm down at all across 15 samples. This isn't explained by
general machine load — both were measured back-to-back under identical
contention, and only one side shows the elevated, flat plateau. The real
`activate_window`/`cx.activate` path (`NEKO_BENCH_REAL`, §4) shows the same
pattern in its own `frame latency` numbers (61.7 ms, 32.0 ms, 11.1 ms for
3 cycles) — consistent with the `NEKO_BENCH` finding, not contradicting it.

**This is a genuine, reproducible regression, not investigated further in
this task** — root-causing it (candidates: the `runtime_shaders` feature
compiling Metal shaders at runtime instead of build-time and never fully
amortizing; a heavier per-frame cost somewhere in this rev's renderer or
layout engine; something specific to `font-kit`) is real, scoped follow-up
work, not something to chase inside a dependency-switch task per the
brief's own "keep your own diff to the dependency switch." Flagged loudly
here and in `AGENTS.md`'s "Summon latency" section per the launch brief's
own instruction — 25–40 ms is still well under 100 ms and not
human-perceptible as sluggish, but it's a real, measured ~6–8x regression
against the documented budget and should not be silently absorbed.

## 9. Daemon idle memory — unaffected, as expected

`neko-daemon`/`neko-core` do not depend on `gpui` at all — this migration
touches only the `neko` (client) crate's dependency. Measured anyway, for
completeness: `verify_harness` (the real `server` module, no capture loop),
isolated `HOME`, `ps -o pid,rss,vsz`, 15s after startup: **12.9 MB**,
consistent with (in fact marginally under) the `settings-and-clipboard-
ranking.md`-documented baseline of ~13.6–13.8 MB. No regression, no reason
to expect one.

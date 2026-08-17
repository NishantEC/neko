# neko — project knowledge

neko is a hotkey-summoned, GPU-rendered launcher for macOS, written from scratch
in Rust on [GPUI](https://gpui.rs) (Zed's UI framework). See `README.md` for how
to run it and `data/dim/plan.md` (in the firstmate home, not this repo) for the
full product plan.

## Current scope: the spine — protocol, daemon, client, search, the frozen panel

Slice 1 was a single-crate proof that GPUI works at all. This task restructured
that proof into the plan's real five-crate workspace and built the first
end-to-end feature: **summon with a hotkey, type, see your real applications
ranked sensibly, press enter, the app launches** — on the frozen visual design,
over real system window vibrancy. Still not built: onboarding UI, clipboard
history, the WASM extension system, agent capability. See "Seams for follow-up
work" below for exactly where each plugs in.

## Crate layout

```
crates/
  neko-protocol/  pure wire types + length-prefixed JSON framing, no logic
  neko-core/      daemon-owned logic: SQLite, app index, search ranking,
                  launching, hotkey-setting persistence, the AgentProvider seam
  neko-daemon/    thin binary hosting neko-core behind a Unix socket
  neko-client/    SDK: persistent auto-reconnecting connection to the daemon
  neko/           the GPUI app — window, panel, live OS hotkey capture
```

Boundary rule the crate split exists to enforce: `neko-protocol` depends on
nothing but `serde`. `neko` (the app) depends on `neko-client` and
`neko-protocol` only — never `neko-core` — so it physically cannot reach past
the daemon boundary to touch SQLite or the app index directly.

## The daemon/client split, and why the hotkey is registered where it is

The daemon is resident, owns SQLite as the single writer (`neko_protocol::database_path()`,
`~/Library/Application Support/neko/neko.db`) and the application index, and
survives the client being killed — a fresh client process just opens a new Unix
socket connection (`neko_protocol::socket_path()`) to the same live daemon, no
handshake state required. `neko`'s `daemon_launcher` module spawns
`neko-daemon` unconditionally on client startup; the daemon's own
`bind_singleton` (in `neko-daemon/src/server.rs`) makes a redundant spawn a
no-op — it pings the existing socket and exits quietly if something already
answers, or takes over a stale socket file if nothing does.

**The live OS-level hotkey registration happens in the client, not the
daemon**, even though the daemon owns and persists the *setting*
(`neko_core::hotkey`, a `settings` KV table). This looked like it could go
either way; it can't, for two reasons: (1) `global-hotkey`'s registration
needs a live run loop, and the client is the process with GPUI's foreground
executor already polling `GlobalHotKeyEvent::receiver()` — routing the actual
keypress through the daemon over IPC would add a socket round-trip to the
exact path the ~11–14ms warm-summon budget is measured on; (2) only a live
registration attempt can prove a conflict (see below), and only the client can
make one. `neko::hotkey_client::HotkeyController` owns this; `neko_core::hotkey`
never touches AppKit/Carbon.

## The hotkey is a runtime-configurable setting, not a constant

Added mid-task, on top of the original single-crate proof's hard-coded
`⌥Space`. The daemon persists the current `HotkeyCombo` and serves it over
`neko-protocol::Request::{GetHotkey, CheckHotkeyConflict, CommitHotkey}`; on a
successful `CommitHotkey` it broadcasts `Event::HotkeyChanged` to every
connected client. `neko::hotkey_client::HotkeyController::rebind` is the real
capture mechanism: it attempts a live OS registration of the candidate first,
and only unregisters the old combo once the new one is proven live — a failed
rebind never loses the working hotkey. `neko_core::hotkey::check_known_conflict`
is a fast, side-effect-free pre-check against well-known reserved combos
(Spotlight, Mission Control, screenshot shortcuts, ...) — real but a heuristic,
not a guarantee; only a live registration attempt can catch a *third-party*
app's custom binding.

**Nothing in this codebase calls `rebind` yet** — there is no rebinding UI in
this slice (that's onboarding's step 09, "Learn the hotkey," explicitly out of
scope here). This is the seam: a future onboarding screen captures a candidate
combination, calls `HotkeyController::rebind`, and on success sends
`Request::CommitHotkey` — see `hotkey_client.rs`'s doc comments and its test
suite (`FakeRegistrar`) for the exact state machine, already covered without
needing real OS calls in the test run.

## Search and ranking

`neko_core::search::fuzzy_score` is a small, dependency-free subsequence
scorer (fzf-shaped: contiguous-run and word-boundary bonuses, a length
penalty) combined with a recency/frequency boost from the `launches` table
(`neko_core::search::rank_apps`). The daemon caps results server-side
(`Request::Search { limit, .. }`, the client asks for 8) — the client never
renders more than that, which is why the result list isn't virtualized (see
"gpui-component" below).

## Icons

`neko_core::icons` extracts each app's real icon via `NSWorkspace::iconForFile`
(not a loose `.icns`-file parse, which silently fails on modern
asset-catalog-only apps) and caches it as a PNG under
`~/Library/Caches/neko/icons/`. Runs once per app on daemon startup, in a
background thread *after* the app index is already searchable, so it never
delays the first search. `neko::panel` reads `SearchItem::icon_path` and
falls back to a plain placeholder square when a PNG isn't cached yet — the
list self-heals on the next search once the background pass catches up.

## The `AgentProvider` seam

`neko_core::agent::AgentProvider` is exactly what the plan asked for: an
object-safe trait with a `FakeProvider` proving it compiles, registered
nowhere in the daemon's request handling. Wiring a real provider's results
into `Request::Search`'s response as more `SearchItem`s (`ResultKind` would
need a new variant) is the seam — "just another result type in the same fast
list," per the plan.

## Third-party UI code: evaluated, then narrowly vendored

`longbridge/gpui-component` (Apache-2.0, crates.io) was evaluated as a
dependency for its virtualized list, text input, and keyboard/focus handling,
and rejected as a dependency (757 resolved packages for three mechanics, a
second `cocoa`/`cocoa-foundation` Obj-C bridging stack alongside the `objc2`
one this repo already uses, and 60+ components' worth of styling opinions to
fight against the frozen bespoke tokens). One genuinely self-contained,
non-trivial piece — cursor-blink timing — was vendored instead, adapted, and
attributed: `crates/neko/src/components/vendor/gpui_component/blink_cursor.rs`.
Everything else evaluated (the virtualized list, the rope-based rich-text
input engine) was declined with reasons recorded in
`crates/neko/src/components/vendor/MANIFEST.md` — read that file before
reaching for either of those again; the reasoning (list virtualization solves
a problem this app's server-capped result list doesn't have; the input engine
is a full multi-line/LSP-integrated editor, an order of magnitude more than a
single-line search field needs) is still valid unless those constraints
change. Apache-2.0 attribution: `NOTICE`, `THIRD_PARTY_LICENSES/gpui-component-APACHE-2.0.txt`.

## Design tokens

`crates/neko/src/theme.rs` is `data/neko-design/report.md` §1's token table as
literal Rust constants (the report's own `sRGB` hex column, not re-derived),
cross-checked in a test against an independently-implemented OKLCH→sRGB
conversion — all 8 base-palette tokens matched exactly; nothing in the table
looked wrong. The identity accent (`ACCENT`, `#efa831`, direction A "amber
eye") is the one named constant the captain asked for — swapping it for
direction B or C is a one-line change in that file. It's intentionally unused
by any paint path right now: the report's colour-identity pick is still open,
and v1 ships in the base neutral palette until the captain picks one.

## Window material

`crates/neko/src/material.rs` is the one-function seam the launch brief asked
for: today it returns GPUI's own `WindowBackgroundAppearance::Blurred` (real,
shipping window-level vibrancy — design report §5 "Direction 1"). The
`neko-native-material` spike (a separate, parallel task) is bridging real
`NSVisualEffectView`/`NSGlassEffectView` under a live GPUI window; when it
lands, its recommended code path replaces this function's body, nothing else.

## v1 simplification: fixed-size window, not dynamic per-keystroke resize

`gpui::Window::resize` exists, but this task had no safe way to verify its
resize-anchor behavior on a borderless always-on-top popup window before
committing to it on the hotkey-latency-critical path. `neko::panel::Root`
instead renders at one fixed size (680×448) with the footer pinned to the true
bottom via a flex-grow content area, so short result lists leave quiet space
above the footer rather than the window growing or shrinking. Documented in
`panel.rs`'s own module doc comment. Real dynamic resizing is a follow-up, not
attempted here specifically because it was untested territory on the one path
this task's acceptance criteria measures a hard number against.

## Summon latency

Re-measured after the restructure (release binary, `RUST_LOG` off — full
methodology and all samples in `docs/evidence/summon-latency.md`):

- **Warm** (window, renderer, and daemon connection already live): **~2.9–6.4ms**
  across 6 samples, mean ≈4.1ms — inside slice 1's own budget (~11–14ms) and
  faster than slice 1, despite the panel now rendering far more per frame.
  The daemon round-trip for `Search` is off the summon path entirely (summon
  is hotkey-press → first-frame-after-activation, purely client-side,
  unchanged from slice 1) — a slow or unreachable daemon cannot make summon
  itself slower.
- **Cold** (first summon after process launch): **~65–160ms** across 2
  samples — roughly double slice 1's ~89ms. This is the one place the extra
  panel content and `WindowBackgroundAppearance::Blurred` compositing
  genuinely cost something, paid once per process lifetime (the process stays
  resident — see "Current scope" above), not per summon. Reported plainly
  rather than absorbed, per the brief's own instruction.

Testing caveat carried over from slice 1: synthetic hotkey events via
`osascript … key code 49 using {option down}` are unreliable in rapid
succession (roughly half dropped between AppleScript and the OS's global-hotkey
delivery), even though every event that *did* arrive was handled correctly.
Real hardware keypresses weren't re-tested with the same instrumentation this
pass either — don't be surprised if automated re-tests need retries.

## Hotkey scoping (must never leak into other apps)

`global-hotkey`'s macOS backend registers through Carbon's
`RegisterEventHotKey`, which is an OS-level *consuming* registration — a
registered combo's keystroke is delivered to the registering process's
callback and does not also reach the focused application. This was already
true in slice 1; nothing in this task's daemon/client split or the
configurable-hotkey addition changes which API performs the registration.
Manually verified this pass: typed `BASELINE-` into a focused TextEdit
document, pressed neko's live hotkey (which correctly summoned the panel),
typed `-AFTER` back into TextEdit — result `BASELINE—AFTER` with nothing
(not even a space) inserted between the two typed strings. Screenshot:
`docs/evidence/hotkey-no-leak-textedit.png`.

## Licence rule

**Every line in this repo is written fresh**, with one documented exception:
see "Third-party UI code" above (`gpui-component`'s `blink_cursor.rs`,
Apache-2.0, attributed). `data/helm/refs/` (in the firstmate home) holds four
reference GPUI apps — `comet` (MIT), `waku` (GPL-3.0), `codux` (GPL-3.0),
`t3code` (MIT) — plus GPUI's own bundled `examples/` (Apache-2.0). All read for
architecture and API shape, never copied. The aim is MIT end to end, plus the
one attributed Apache-2.0 file.

## The GPUI dependency decision

`gpui` is Apache-2.0 as published on crates.io, but git-`main` currently links
a GPL-3.0-or-later crate (`ztracing`) through an unfixed dependency edge — see
`data/dim-licence/report.md`. **Chosen route, unchanged from slice 1: depend on
the published crate, `gpui = "0.2.2"` from crates.io — not git, no `[patch]`
block.** Re-verified for the full five-crate workspace this task built
(`docs/evidence/cargo-tree.txt`, `docs/evidence/cargo-license.txt`): still no
GPL/AGPL anywhere in the tree, still only `self_cell`'s dual
`Apache-2.0 OR GPL-2.0` line, used under the Apache-2.0 arm.

To re-verify after any dependency bump:

```sh
cargo license 2>/dev/null | grep -iE '\bgpl\b|agpl'   # expect only the self_cell dual-license line
cargo tree | grep -i 'ztracing\|zlog'                  # expect no output
```

## Seams for follow-up work

- **Onboarding UI** (14-screen sequence, design report §3): drives
  `HotkeyController::rebind` for step 09; nothing else in this codebase
  assumes it exists. The panel's own first-run/empty state
  (`panel::render_empty_state`) is deliberately generic, not onboarding.
- **Clipboard history**: `neko_protocol::ResultKind` has one variant (`App`);
  a `Clipboard` variant plus a daemon-side clipboard watcher is additive, not
  a restructure. `theme::PANEL_WIDTH_WITH_DETAIL_PX` (760px) is already
  reserved for the detail-pane width clipboard entries need.
- **Dynamic window resize**: see "v1 simplification" above.
- **`AgentProvider` wiring**: see above.
- **Native window material spike**: see "Window material" above.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.

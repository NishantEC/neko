# neko — project knowledge

neko is a hotkey-summoned, GPU-rendered launcher for macOS, written from scratch
in Rust on [GPUI](https://gpui.rs) (Zed's UI framework). See `README.md` for how
to run it and `data/dim/plan.md` (in the firstmate home, not this repo) for the
full product plan this is a slice of.

## Current scope: single-crate proof, not the target architecture

The full plan (`data/dim/plan.md`) calls for five crates — `neko-protocol`,
`neko-core`, `neko-daemon`, `neko-client`, `neko` — with a resident daemon and a
resident GPUI client talking over a protocol boundary. **This slice deliberately
does not build that.** The captain narrowed the brief mid-task to the smallest
possible proof: *does GPUI work at all* — one crate, one window, a global
hotkey, escape to dismiss, nothing else. No daemon, no client SDK, no
round-trip. The five-crate split and the daemon are follow-up work, not done
here — don't assume they exist.

What this slice does keep from the target architecture, because it doesn't cost
anything extra to keep: **the process stays resident.** `main.rs` opens the
window once at startup with `show: false`, and it is never destroyed — the
hotkey handler and the `Escape` action only call `window.activate_window()` /
`cx.hide()` (an app-level `NSApp` hide, not a window close). This is why warm
summons are ~10x faster than the first one (see "Summon latency" below): there
is no per-summon window creation or GPU pipeline warmup after the first.

## Licence rule

**Every line in this repo is written fresh.** `data/helm/refs/` (in the
firstmate home) holds four reference GPUI apps — `comet` (MIT), `waku`
(GPL-3.0), `codux` (GPL-3.0), `t3code` (MIT) — plus GPUI's own bundled
`examples/` (Apache-2.0, via the `gpui` crate on crates.io). All of these were
read for architecture and API shape (how does GPUI's `EntityInputHandler` work,
how do other apps structure a window). None of it was copied. `src/text_field.rs`
in particular follows the same overall shape as GPUI's own `examples/input.rs`
(there's really only one way to wire up `EntityInputHandler` + a custom
`Element` correctly) but every line was written from understanding, not copied,
and the selection/mouse/clipboard/IME-composition parts of that example were
cut rather than ported. The aim is MIT end to end.

## The GPUI dependency decision

`gpui` is Apache-2.0 as published on crates.io, but git-`main` (what the wider
GPUI ecosystem tracks) currently links a GPL-3.0-or-later crate (`ztracing`)
through an unfixed dependency edge — see `data/dim-licence/report.md` for the
full writeup and evidence trail.

**Chosen route: depend on the published crate, `gpui = "0.2.2"` from
crates.io — not git, no `[patch]` block.** Verified directly: `ztracing`
didn't exist yet when 0.2.2 was cut (Oct 2025), and `cargo tree` /
`cargo license` on this workspace (`docs/evidence/cargo-tree.txt`,
`docs/evidence/cargo-license.txt`) show no GPL/AGPL anywhere in the tree — the
only GPL mention at all is `self_cell`'s dual `Apache-2.0 OR GPL-2.0`, used
here under the Apache-2.0 arm. This is simpler than the licence report's
recommended patch-and-track-HEAD route and sufficient for this slice; if a
later feature needs something only on git-main, revisit then rather than
patching preemptively.

To re-verify after any dependency bump:

```sh
cargo license 2>/dev/null | grep -iE '\bgpl\b|agpl'   # expect only the self_cell dual-license line
cargo tree | grep -i 'ztracing\|zlog'                  # expect no output
```

## Crate/module layout (this slice)

One crate, `neko` (`src/main.rs`, `src/text_field.rs`). No workspace yet — see
"Current scope" above.

- `src/main.rs` — app entry point, window lifecycle (open once, hide/show
  forever), global hotkey registration and polling, key bindings.
- `src/text_field.rs` — a minimal single-line text field wired through GPUI's
  `EntityInputHandler` + a custom `Element`, which is the native-correct way
  to receive typed/IME input in GPUI. Deliberately does **not** implement
  mouse selection, clipboard, or IME composition (marked text) — those are
  real, separate pieces of work, not needed to prove the window works.

## Summon latency

Measured in-process (`src/main.rs`'s `on_next_frame` hook after
`activate_window()` — the closest proxy GPUI exposes for "visible and
accepting input"), hotkey-press to first-frame-after-activation, via a
release build:

- **Cold** (first `⌥Space` after process launch): **~89ms** — includes the
  first GPU frame / text-shaping warmup.
- **Warm** (subsequent summons; window and renderer already live): **~11–14ms**,
  averaging ~12ms across repeated trials.

The ~7x gap between cold and warm is exactly the case the architecture report
warned about: a launcher that cold-starts its UI per summon cannot hit this
latency budget. Keeping the window resident (see above) is what buys the
warm number. Reproduce with `RUST_LOG` off and `cargo run --release`, then
watch stderr while pressing `⌥Space` a few times.

Testing caveat: synthetic hotkey events sent via
`osascript … key code 49 using {option down}` were unreliable in rapid
succession (roughly half were dropped somewhere between AppleScript and the
OS's global-hotkey delivery) even though every event that *did* arrive was
handled correctly. This looks like a synthetic-input artifact, not an app bug
— real hardware keypresses were not tested with the same instrumentation.
Don't be surprised if automated re-tests need retries.

## GPUI, pleasant or painful (for whoever picks this up next)

- Window/app lifecycle (`open_window`, `WindowOptions`, `App::hide`/`activate`,
  `Window::activate_window`) is small, well-documented, and did exactly what
  the docs said on the first try.
- There is no cross-platform "hide this one window" — only `App::hide()`
  (hides the whole app, AppKit `NSApp` semantics). Fine for a single-window
  launcher; would need per-window handling if a second window ever exists.
- There is no built-in text input widget and no simple "give me keystrokes"
  API for arbitrary typed characters — the only correct path is
  `EntityInputHandler` + a hand-written `Element` that calls
  `Window::handle_input`, which is real IME-integration work, not a
  one-liner. `gpui`'s own bundled `examples/input.rs` is the reference for
  this; there's no shortcut for it.
- No first-party global-hotkey support (expected — that's an OS-level
  concern, not a rendering one). The independent `global-hotkey` crate
  (tauri-apps, dual MIT/Apache-2.0) filled this gap without friction, and its
  event channel polls cleanly from a `cx.spawn` loop on GPUI's foreground
  executor.
- The Metal shader toolchain (`xcodebuild -downloadComponent MetalToolchain`)
  was not installed on this machine and `gpui` failed to compile until it
  was — a one-time ~690MB download. Worth knowing before a fresh-clone build
  on a new machine.
- Net: for this slice's scope, GPUI was pleasant, not painful. The one real
  cost was the text-input path, and that cost is inherent to doing IME-correct
  text input on any native toolkit, not specific to GPUI.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.

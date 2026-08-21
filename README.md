# neko

A hotkey-summoned launcher for macOS, written from scratch in Rust on
[GPUI](https://gpui.rs). Press ⌥Space anywhere, type, press Enter.

![The neko panel, showing application, System Settings, command and clipboard
results for one query](docs/screenshot.png)

## What it does

One query searches five sources at once, each under its own section header:

- **Applications** — every app Spotlight knows about, plus the ones on the
  sealed system volume that Spotlight does not index. Enter launches.
- **Clipboard history** — the last 200 distinct text and link copies, with the
  app they came from. Enter puts one back on the pasteboard.
- **Files and folders** — name-prefix search across `~/Documents`, `~/Desktop`
  and `~/Downloads`. Enter opens.
- **System Settings panes** — "displays", "bluetooth", "sound". Enter opens
  that pane.
- **Commands** — rows that open a mode inside the panel instead of launching
  something. Two exist: Clipboard History and Themes.

⌘K opens the actions menu for the selected row. Escape leaves a mode, then
hides the panel. Seventeen themes ship built in, with live preview as you
arrow through them.

## What it is not

- **Not cross-platform.** It links AppKit directly for window material,
  pasteboard access, hotkeys and icon extraction.
- **Not a plugin host.** Result types are compiled in. The extension seam is
  the `Provider` trait, not WASM — see
  [docs/adding-a-provider.md](docs/adding-a-provider.md).
- **Not networked.** Nothing leaves the machine. Search runs against a local
  SQLite database and Spotlight's own index.
- **Not distributable.** See the licence section below.

## Build and run

Requires macOS and Rust 1.97.1 (`rustc --version`).

```sh
./scripts/setup-gpui-patch.sh   # once, before the first build
cargo build --release
./target/release/neko
```

The first step is not optional. neko depends on a pinned fork of gpui with one
local patch applied on top, and the workspace `[patch]` section points at a
checkout that this script populates under
`~/Library/Caches/neko-dev/gpui-fork-patched`. Without it `cargo build` fails
with a missing-path error. Run it again whenever the pinned rev changes. The
patch itself fixes a ~30ms warm-summon regression in the fork; see
[docs/architecture.md](docs/architecture.md#the-gpui-dependency).

Run it once more from a clean shell to confirm — it is idempotent and prints
`nothing to do` when the checkout is already current.

The first launch walks a 14-screen onboarding arc: what neko needs, the
Accessibility permission ask (a real macOS prompt), the clipboard-history ask,
and choosing and testing the summon hotkey. After that, launching goes straight
to summon-on-hotkey. To replay onboarding:

```sh
NEKO_RESET_ONBOARDING=1 ./target/release/neko
```

If you decline Accessibility, the hotkey cannot register. neko stays usable —
click its Dock icon to summon the panel instead.

`cargo run --release` works for development. The latency numbers in
`AGENTS.md` are measured against the release binary run directly.

## How it is put together

Five crates: a pure wire protocol, a resident daemon that owns SQLite and every
index, a thin client SDK, and the GPUI app. Read
[docs/architecture.md](docs/architecture.md) before the code.

| Document | What it covers |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | Crates, the daemon/client split, the wire protocol, providers, ranking, modes, the window |
| [docs/adding-a-provider.md](docs/adding-a-provider.md) | Adding a sixth result type |
| [docs/adding-a-theme.md](docs/adding-a-theme.md) | Adding an eighteenth theme |
| `AGENTS.md` | The full engineering record: every decision, what was measured, and what was ruled out |
| `docs/evidence/` | Reports and window-scoped screenshots behind each of those decisions |

`AGENTS.md` is long because it is the project's memory. When a document above
and `AGENTS.md` disagree, `AGENTS.md` is the record and the code is the truth.

## Development

```sh
cargo test --workspace
cargo clippy --all-targets
```

Tests are headless. The GPUI ones run against `TestAppContext`, so no window
opens. Two suites take a process-wide lock because they touch shared OS state:
theme tests (`theme::test_lock`, the active palette is a process global) and
pasteboard tests (`text_field::tests::pasteboard_test_lock`, there is one
systemwide `NSPasteboard`).

`crates/neko/src/evidence.rs` holds env-gated hooks for capturing screenshots
without synthetic input — `NEKO_SHOW_ON_LAUNCH=1`, `NEKO_SHOW_QUERY=<text>`,
`NEKO_SHOW_THEME=<id>`, `NEKO_BENCH=<n>` and others. Every one of them shows
its window **without taking keyboard focus**. That is a deliberate default: an
evidence window that took focus once captured real keystrokes meant for another
app. Activation is a separate opt-in, `NEKO_EVIDENCE_ACTIVATE=1`, and it warns
on stderr when set.

## Licence

Every line of source in this repository is MIT — see `LICENSE`. One vendored
file is Apache-2.0 and attributed in `NOTICE`. Seventeen theme palettes vendor
colour *values* from eight upstream projects, every one MIT and verified from
its own source; the table is in `docs/evidence/themes-report.md`.

**A built binary is a different matter.** `crates/neko` links the
`wingleeio/zed` fork of gpui, which unconditionally pulls in GPL-3.0-or-later
code through `gpui → sum_tree → ztracing`, with no feature that avoids it. That
obligation triggers on distribution, not on local use, so building and running
neko on your own machine carries nothing. Do not publish a release binary
without resolving it first. The full record, including why the fork was adopted
anyway, is in `AGENTS.md` under "The GPUI dependency decision".

# neko

A hotkey-summoned launcher and agent control plane for macOS, written from
scratch in Rust on [GPUI](https://gpui.rs). Press ⌥Space anywhere, type, press
Enter.

![The neko panel, showing application, System Settings, command and clipboard
results for one query](docs/screenshot.png)

## What it does

One query searches every source at once, each under its own section header:

- **Applications** — every app Spotlight knows about, plus the ones on the
  sealed system volume that Spotlight does not index. Enter launches.
- **Clipboard history** — the last 200 distinct text and link copies, with the
  app they came from. Enter puts one back on the pasteboard.
- **Files and folders** — name-prefix search across `~/Documents`, `~/Desktop`
  and `~/Downloads`. Enter opens.
- **System Settings panes** — "displays", "bluetooth", "sound". Enter opens
  that pane.
- **Agents** — the coding agents running on this machine right now, as tiles
  above the results.
- **Codex tasks** — recent local Codex tasks in that same tile strip. Their
  data is a warmed local snapshot, so a palette keystroke never starts Codex
  or reads a session file.
- **Needs you** — any agent stopped waiting for a permission. These lead the
  list: choose Approve from ⌘K, or explicitly confirm Decline there.
- **Commands** — rows that open a mode inside the panel instead of launching
  something: Clipboard History, Themes, Agents, Schedules, Terminals, Usage,
  New Agent, and Ask neko.

⌘K opens the actions menu for the selected row. Escape leaves a mode, then
hides the panel. Seventeen themes ship built in, with live preview as you
arrow through them.

## Agents

neko drives the agents [Paseo](https://paseo.sh) supervises, over its daemon's
own MCP endpoint, so the things you would switch apps for are a keypress away:

- **An agent blocked on a permission** leads the root list, and the Dock icon
  badges the count even while the panel is hidden — so you find out without
  looking.
- **Agents** lists every session; Enter sends a follow-up prompt, ⌘K changes
  its session mode or cancels the run.
- **Codex tasks** share the agent tile strip, but keep a separate local
  app-server control path. A task opens a compact, read-only activity view;
  a permission request presents its reason and explicit Approve/Decline
  actions. A decline uses the same confirmation guard as other destructive
  menu actions.
- **Schedules** shows what runs on a cron and when it next fires. Enter pauses
  or resumes; running one now is behind ⌘K, because Enter is what a finger
  presses on the way past a list.
- **Terminals** shows what is open and what each last printed.
- **Usage** reads your quota straight from each provider's own API — Claude
  Code, Codex and Grok.
- **Ask neko** takes a sentence. It proposes exactly one tool call, shows you
  the call, and runs nothing until you press Enter again.

All of it degrades to nothing if Paseo is not running; none of it is required
for the launcher half to work.

Codex follows the same rule. neko supervises one local `codex app-server
--stdio` child and renders only its compact in-memory projection. If Codex is
missing, signed out, or stops, retained task tiles say they are unavailable and
approvals are not actionable; applications, files, clipboard history and the
rest of the launcher continue to work. Opening a task can request up to 40
visible activity summaries only when Codex accepts its experimental history
capability. neko never reads Codex session/rollout files.

## What it is not

- **Not cross-platform.** It links AppKit directly for window material,
  pasteboard access, hotkeys and icon extraction.
- **Not a plugin host.** Result types are compiled in. The extension seam is
  the `Provider` trait, not WASM — see
  [docs/adding-a-provider.md](docs/adding-a-provider.md).
- **Not a cloud app, but no longer entirely offline.** Searching is local:
  SQLite and Spotlight's own index, with nothing leaving the machine. Three
  features do make outbound requests, all of them to somewhere you are already
  signed in, and none of them running unless you use it: **Usage** reads quota
  from Anthropic, OpenAI and xAI; **Ask neko** sends your sentence and the
  names of your agents to Anthropic's Messages API to plan a tool call; and
  everything under **Agents** talks to Paseo's daemon on `127.0.0.1`. The
  optional Codex projection talks only to one local child over stdio.
  Credentials are read from the Keychain and from the CLIs' own config files,
  are never persisted by neko, and are passed to `curl` on stdin so they
  cannot appear in `ps` output.
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
checkout that this script populates at `.gpui-fork-patched/` inside the repo
(gitignored — it is a whole third-party monorepo). Without it `cargo build` fails
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
| [docs/adding-a-provider.md](docs/adding-a-provider.md) | Adding a result type through the `Provider` seam |
| [docs/plan-agent-control-plane.md](docs/plan-agent-control-plane.md) | How the agent half was surveyed and built, including the two layers that turned out differently |
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

**A built binary is a different matter, and it is GPL, not MIT.**
`crates/neko` links the `wingleeio/zed` fork of gpui, which unconditionally
pulls in GPL-3.0-or-later code through `gpui → sum_tree → ztracing`, with no
feature that avoids it. This is not particular to that fork — vanilla Zed's
gpui carries the same chain (`zed-industries/zed#55470`, open).

So, taking the cautious reading:

- **Source**: redistribute freely. MIT is GPL-compatible; nothing conflicts.
- **Binaries**: you may ship them, under **GPL-3.0-or-later** terms — complete
  corresponding source to recipients, same freedoms, no added restrictions.
- **Using it yourself**: nothing attaches. GPL obligations arrive on
  conveyance, not on use.

`NOTICE` states this in full, including the two questions that are genuinely
unsettled rather than answered. None of it is legal advice; if you intend to
ship this commercially, ask a lawyer. The record of why the fork was adopted
with the exposure understood is in `AGENTS.md`, "The GPUI dependency decision".

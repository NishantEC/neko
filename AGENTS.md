# Neko: rules for working here

This repository is public. Read this file first. Detailed knowledge goes in `docs/knowledge/`, one topic per file, indexed at the
bottom; keep this file small.

## Nothing private enters the repository

- No real names, employers, clients, handles, ticket keys, machine names or home folders, in files or in commit messages.
- Write placeholders instead: Asha, Lee, Acme, ENG-123, example.com, `/Users/example`, cats named miso, mochi and tofu.
- `scripts/privacy-check.sh` runs before every commit (`git config core.hooksPath .githooks`). Never skip the hook. The owner's
  private pattern file lives outside the repo and must never be committed.
- Renders and screenshots use placeholder data only. Captures of a real screen stay out of the repo.
- See `docs/knowledge/keeping-neko-public.md`.

## Other people's work

Read others' code for ideas and write every line fresh. Dependencies are MIT, Apache-2.0 or OFL only; nothing GPL. Record what
ships in `NOTICE` and `THIRD_PARTY_LICENSES/`.

## How work is done

- **Thin slices.** One user-visible slice at a time. A slice is done when it was tried in the real, launched app, its tests pass,
  nothing private is in it, licences are recorded and its doc is written.
- **No surprises.** Default to what well-made tools and Mac apps already do. Differ only with a stated reason.
- **Design in the app.** Screens are designed in the real Rune app and shown as real renders, not as mockups.
- **rune-ui.** Components are copied in by the `rune-ui` CLI and recorded in `rune-ui.lock`. Never hand-edit a copy: fix it
  upstream and run `rune-ui update`.
- **Report what is true.** If a check failed or a step was skipped, say so. Never weaken a test to make it pass.
- **Conventional commits:** `type(scope): summary`.

## Rules learned the hard way

- **Verify in a launched app.** Headless renders and tests miss whole classes of problems. After a change to the shell or a
  framework bump, launch the real app and check that its window is on screen and that its Dock menu is normal.
- **Register with the Dock on the main thread before Rune starts** (see the "Known Rune issue on macOS" section of rune-ui's
  README). Otherwise the Dock may show the app as "Application Not Responding".
- **Shell scripts under `pipefail` never pipe into `grep -q`.** It stops reading early, the writer dies of SIGPIPE and the check
  fails although it matched. Read all the input (`grep -c … >/dev/null`).
- **Stop only processes you started,** by PID. Never kill by name or pattern, and never quit, kill or reinstall an installed app
  from a build or test.
- **No paid model calls** from tests, builders or scripts. Anything that spends money asks first.
- **Keep the machine cool.** Before a heavy build, run `~/.agents/bin/machine-ok` where it exists and build only on `OK`.
- **Clean up.** Remove worktrees, build caches and temp files when a slice merges.

## Knowledge index

- [Keeping Neko public](docs/knowledge/keeping-neko-public.md) — what may not enter the repo, the placeholders, the check.
- [0001: Neko starts again](docs/decisions/0001-neko-starts-again.md) — why the repository was emptied and what the new one is built on.
- [Architecture](docs/architecture.md) — the crates, the daemon and the screens, the transport and token, the store.
- [The store](docs/knowledge/store.md) — SQLite rows per entity, the append-only event log, migrations, the clock and ids handed in.
- [The protocol](docs/knowledge/protocol.md) — requests, replies and events as JSON, versioned at hello, the WebSocket pump.
- [The app shell](docs/knowledge/app-shell.md) — the band, folding, the window's frame, menus, keyboard, accessibility, the link.
- [rune-ui in Neko](docs/knowledge/rune-ui-in-neko.md) — installing with the CLI, never editing copies, the theme, licences, upstream asks.
- [Runtimes](docs/knowledge/runtimes.md) — the agent contract, how Claude Code is found and run, the fake CLI, what is not sure yet.
- [Permissions](docs/knowledge/permissions.md) — what goes ahead, what asks, request states, "always" scoped to the task.
- [The run guard](docs/knowledge/run-guard.md) — nothing a run starts outlives it; the limits; the known gap.
- [The transcript](docs/knowledge/transcript.md) — runs, parts, usage, write rate, interrupted runs, cost as reported.

## The gate

`scripts/check.sh` runs clippy's always-a-defect lints and every test in both workspaces (the app's end-to-end test starts a real
daemon), then the privacy check. Every test runs the fake CLI as `claude` (`NEKO_CLAUDE_BIN`); none runs the real one. `--quick` compiles only; the pre-commit hook runs it. `scripts/snap.sh` renders every scene to
`/tmp/neko-s0/` (or the folder given): look at each picture before calling a change done.

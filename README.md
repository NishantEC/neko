# Neko

An operating system for agents. Anything a person does on a computer, Neko and its cats can do, and keep doing while the person
is away: code, browsers, files, spreadsheets, documents and creative apps, alongside the person or on their own.

**Status: slice S0, a walking skeleton.** Neko is being rebuilt from an empty repository (begun 2026-11-26). What runs today: a
calm Mac app with a sidebar and Home. Type a task and press Return; the daemon stores it, it appears under Today and opens with
your words and a line saying no cat is working on it yet. There is no agent yet. Quit and relaunch: the window is where it was
and the task is still there. This page changes as each slice ships, and only describes what is built.

## What it will be

- **Cats.** A named agent per kind of work, as many as you like, each with its own rules, plan and budget.
- **Tasks and rounds.** Hand a task to a cat and it works to done. Rounds are the work that starts itself and keeps going overnight.
- **Needs you.** Neko asks only what matters, one question with a default. Sending, merging, deleting and paying wait for a yes.
- **Briefing.** One summary of what finished, what is stuck and what it cost.
- **Every dime.** Each model call has a price before it runs and a ledger line after. Budgets stop work; they do not warn.
- **Local first.** Your machine, your data. The same daemon can run on several machines, paired once.

## How it is built

- A Rust daemon holds the tasks, the store, permissions and the cost ledger, and runs Neko's own agent behind one contract, so the
  agent can be replaced without the rest changing.
- Screens are thin clients over the daemon's requests and events. On the desktop they are drawn with
  [Rune](https://github.com/anomalyco/rune), using components from [rune-ui](https://github.com/NishantEC/rune-ui). <!-- privacy-ok: the project's own public repository -->
- Standards where they exist: MCP for tools, `SKILL.md`, `AGENTS.md`, git worktrees.

## Build and run

You need macOS and Rust (stable, 1.89 or later).

```sh
scripts/run.sh          # build from source and open Neko against a throwaway data folder (removed afterwards)
scripts/check.sh        # the gate: lints and every test, in both workspaces
scripts/snap.sh         # headless pictures of every screen, into /tmp/neko-s0/
scripts/build-app.sh    # Neko.app (the daemon inside), signed ad hoc
scripts/install.sh      # the gate, then Neko.app into /Applications
```

The daemon also runs on its own: `neko-daemon --data-dir DIR`. Its data lives in `~/Library/Application Support/neko` unless
`NEKO_DATA_DIR` or `--data-dir` says otherwise. See `docs/architecture.md`.

## Working in this repository

`AGENTS.md` holds the rules for people and agents: nothing private enters the repo, read others' code for ideas and write fresh,
build in thin slices, and call a slice done only after it was tried in the real app.

## Licence

MIT. See `LICENSE`. What ships from others is listed in `NOTICE` and `THIRD_PARTY_LICENSES/`.

# 0001: Neko starts again from an empty repository

Date: 2026-11-26. Status: accepted.

## Decision

The earlier implementation of Neko is retired and this repository starts again from nothing. Neko is rebuilt in thin slices, each
one tried in the real app before the next begins. Nothing is carried over by default: a piece of the earlier code comes back only
when a slice needs it and it fits the design below.

## Why

The earlier app grew feature by feature and kept everything it ever had. The product was then planned again from first
principles, and building that plan on top of the old code kept the old shape, the old screens and the old data. A skeleton that
grows is easier to keep small, fast and understood than a large app that is trimmed.

## What the new Neko is built on

- **A Rust daemon** that owns tasks, the store, permissions, the run guard and the cost ledger. It can run with no screen.
- **One agent, Neko's own,** behind an agent contract (start, steer, stop, stream, tools, permissions, cost per call), so another
  agent can take its place without the rest changing.
- **Money first.** Every model call has a price before it runs and a ledger line after; budgets stop work.
- **Screens are thin clients** over the daemon's requests and events. The desktop app is drawn with Rune, with components from
  rune-ui, and behaves like a Mac app (menus, keyboard, accessibility, system settings).
- **Standards:** MCP for tools, `SKILL.md`, `AGENTS.md`, git worktrees.
- **Local first:** SQLite and plain files on the person's own machine.

## Consequences

- For a while Neko does less than the earlier app did.
- Old data is not imported.
- The repository's history begins again at this decision.

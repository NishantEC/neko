# Proactive Source Watching Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A connected, trusted MCP source begins a bounded read-only watch in its workspace without a second per-watch activation step.

**Architecture:** Reconcile source watches from existing scoped connections in the daemon scheduler. Preserve a paused watch as the user's stop control. Mark background leases distinctly and enforce a read-declared-tool filter at the MCP bridge; prompt text alone cannot constrain tools.

**Tech Stack:** Rust, serde-backed Neko snapshots, the existing MCP bridge, SwiftUI Watching screen.

---

### Task 1: Seed source watches without duplicates

**Files:**
- Modify: `crates/neko-core/src/mcp_host/store.rs`
- Modify: `crates/neko-daemon/src/workbench/responsibilities.rs`

- [x] **Step 1: Add failing core tests.** Test idempotent seeding, untried suggestion activation, durable pause and no-readable-tool behavior.
- [x] **Step 2: Verify the tests fail.** `cargo test -p neko-core seed_source_watches --lib` failed because the function was missing.
- [x] **Step 3: Implement the bounded reconciler.** `seed_source_watches` now considers each eligible source/workspace once, starts an untried chat suggestion instead of a duplicate, and otherwise creates one plan-only watch. A durable scope marker preserves later pauses.
- [x] **Step 4: Verify green.** All three focused core tests passed.
- [x] **Step 5: Wire it before the scheduler claims work.** `claim_due` reconciles and saves before selecting a due watch; the daemon test proves an automatically created watch is claimed.

### Task 2: Make unattended tool execution read-only by policy

**Files:**
- Modify: `crates/neko-daemon/src/workbench/responsibilities.rs`
- Modify: `crates/neko-daemon/src/mcp_host.rs`
- Modify: `native/NekoKit/Sources/NekoNative/Watching/ResponsibilitiesView.swift`

- [x] **Step 1: Add a failing bridge test.** A background run granted both a read-declared and a mutating tool must list only the former and reject a direct call to the latter before transport dispatch.
- [x] **Step 2: Verify red.** `cargo test -p neko-daemon background_run --bin neko-daemon` failed because the mutating tool was listed.
- [x] **Step 3: Enforce the tool filter.** Responsibility run IDs use `watch:` and the bridge filters and rejects non-read-declared tools.
- [x] **Step 4: Verify green and UI clarity.** Focused daemon tests passed; Watching explains automatic checking, queued first runs and pause controls.
- [x] **Step 5: Verify the release boundary.** Rust and Swift suites passed, `git diff --check` passed, the signed native bundle built and installed, and live Linear `watch:` receipts succeeded. The first Mobbin check showed its catalog cannot monitor changes, so the refined build paused that generated watch. Paper's current authentication error and the admin MCPs' absence of read-declared tools keep them out of unattended runs.

This first slice is deliberately narrower than full replacement-user autonomy. It does not authorize external writes, messages, merges, or deployments; those need separately delegated policy. It also does not use source annotations as proof of behavior from a dishonest MCP server.

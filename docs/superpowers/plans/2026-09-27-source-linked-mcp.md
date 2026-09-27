# Source-linked MCP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Neko use a Codex or Claude MCP definition already present in a selected folder, without copying its credentials or silently granting access.

**Architecture:** Reuse `setup_import`'s bounded, read-only adapters to surface source definitions. Persist only a source reference, reviewed configuration identity, and Neko's own tool grants; resolve source configuration and credentials afresh in the daemon before discovery and dispatch. Existing Neko-owned connections remain unchanged.

**Tech Stack:** Rust, GPUI, daemon IPC, SQLite, rmcp.

---

### Task 1: Source identity and resolution

**Files:** `crates/neko-protocol/src/mcp_host.rs`, `crates/neko-core/src/setup_import.rs`, `crates/neko-core/src/mcp_host/store.rs`

- [x] Add an optional `SourceLink { source_path, candidate_id, config_hash, executable_identity }` to `McpConnection` with serde defaults; leave existing rows compatible.
- [x] Add a bounded resolver accepting the selected workspace and exact source identity; reject vanished, disabled, changed, unsupported, or out-of-scope definitions. Derive `config_hash` only from the supported transport fields, never from credentials.
- [x] Write and run failing tests for workspace scope, changed config, removed source, and script identity; implement until they pass.

### Task 2: Link and dispatch through existing approval boundary

**Files:** `crates/neko-protocol/src/mcp_host.rs`, `crates/neko-daemon/src/mcp_host.rs`, `crates/neko-core/src/mcp_host/store.rs`

- [x] Add `McpCommand::LinkSource { workspace_id, candidate_id, trust_local_process }`. A link starts with no discovered tools or grants; local processes require explicit trust.
- [x] Resolve the source before MCP discovery, browser authentication, and every tool dispatch; use the resolved current credentials only in daemon memory. Reject a changed source before transport, and keep the per-run bridge scoped to Neko grants.
- [x] Write and run failing daemon tests proving no dispatch before grant, a changed/missing source blocks a granted call, and existing Neko-owned connections still work.

### Task 3: Workspace UI

**Files:** `crates/neko/src/workspace/tools.rs`, `crates/neko/src/workspace.rs`, `crates/neko-protocol/src/workbench.rs`, `crates/neko-daemon/src/workbench/import.rs`

- [x] Show read-only source discovery under the selected workspace's Connections tab, scoped to that workspace plus global definitions; show unavailable reasons without an import table.
- [x] Allow an individual source definition to be linked after local process trust where applicable. Retain the existing Discover tools and per-tool grant controls.
- [x] Add UI/state tests for one workspace seeing only its own and global source definitions, existing linked definitions not duplicating, and no implied grant.

### Task 4: Clean up and verify

**Files:** `crates/neko/src/onboarding/setup.rs`, `README.md`, `docs/architecture.md`, `docs/superpowers/specs/2026-09-27-linked-workspace-capabilities-design.md`

- [x] Remove unreachable onboarding import/skill/MCP screens while retaining daemon migration protocol; update docs to the actual behavior.
- [x] Run `cargo fmt --all`, `cargo test --workspace -- --test-threads=1`, `cargo clippy -p neko -p neko-core -p neko-daemon --all-targets -- -D clippy::correctness`, `cargo build -p neko -p neko-daemon`, and `git diff --check`.
- [x] Exercise an isolated daemon with a local stdio MCP fixture and capture the native onboarding UI. Commit on `main`; do not clear or overwrite installed app data without a separate explicit install request.

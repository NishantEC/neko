# User-owned MCP Implementation Plan

> **For agentic workers:** Use subagent-driven-development for independently
> bounded transport work and read-only review; the primary agent owns integration.

**Goal:** Replace service-specific setup and polling with user-added MCP tools
and scoped responsibilities, preserving existing tasks and approval boundaries.

**Architecture:** The resident daemon owns upstream sessions and Keychain
credentials. A per-run bridge exposes only currently granted tools to the local
agent runtime. Versioned storage retains old Linear evidence but disables old
polling and permissions on migration.

**Tech Stack:** Rust, serde wire contracts, SQLite, official rmcp SDK, Tokio,
existing GPUI/client/daemon boundary and local Codex CLI adapter.

## Delivery order and ownership

1. Primary: protocol, bounded persistence, migration and grants.
2. Bounded implementation worker: SDK transport and authentication primitives.
3. Primary: daemon lifecycle, per-run bridge and native connection UI.
4. Primary: generic responsibilities and source receipts.
5. Independent review, full tests, native evidence and documentation.

Existing uncommitted native-workbench changes must remain intact. Do not stage
unrelated changes or modify the user's global Codex configuration. Execute in
the existing `codex/personal-agent` worktree. No external account is activated
without user configuration.

## 1. Wire contracts, persistence and migration

Files: add `crates/neko-protocol/src/mcp_host.rs` and
`crates/neko-core/src/mcp_host/store.rs`; modify protocol exports and workbench
snapshot/command dispatch. Keep the protocol serde-only: JSON schemas/results
travel as bounded strings, parsed and validated in core.

- [x] Add failing migration tests: old snapshot retains tasks, plans, results,
  source records and worktrees, but all old connection/away permissions turn off.
- [x] Add failing grant tests: no tools initially granted; changed schema loses
  grant; connection/workspace mismatch and paused connections deny execution.
- [x] Define `ServerConfig` (stdio executable/args, HTTP URL), `McpConnection`,
  `McpTool`, `ToolGrant`, and `Responsibility`, with bounded core validation.
  Secret values remain write-only inputs and Keychain records, not snapshots.
- [x] Implement atomic migration and a version guard; reject unknown future
  versions rather than overwriting them. Preserve historical Linear data.
- [x] Run `cargo test -p neko-core mcp_host -- --test-threads=1` and protocol tests.

Core denial contract:
```rust
assert!(authorize(&snapshot, "workspace-b", "connection-a", "lookup").is_err());
assert!(authorize(&snapshot, "workspace-a", "paused", "lookup").is_err());
```

## 2. MCP transport and authentication

Files: `crates/neko-core/src/mcp_host/transport.rs`, authentication module,
core Cargo dependencies, deterministic local/HTTP fixtures under scripts.

- [x] Pin rmcp after inspecting actual SDK API and protocol revision support.
- [x] Test real stdio fixture discovery before implementation; require launch
  trust and absolute executable, no implicit shell or ambient secret environment.
- [x] Implement SDK sessions, paginated discovery, bounded call/discovery output,
  cancellation, timeouts, process cleanup and fixed concurrency.
- [x] Test remote HTTP fixture, reject remote cleartext/credential-bearing URLs,
  use redirect-denying clients and redact remote errors.
- [x] Implement per-connection Keychain secrets and HTTP authorization with
  validated discovery, PKCE/state/issuer binding, expiry/refresh, and explicit
  failure states. Authentication must not grant tool execution permission.
- [x] Test duplicate server instances, expired auth, redirect refusal, response
  limits, malformed discovery and disconnect. No live credentials in fixtures.

Transport target interface:
```rust
// Returns JSON schemas/results, never credentials.
discover(config, credentials, cancellation) -> Result<Vec<McpTool>, HostError>
call(config, credentials, tool, arguments_json, cancellation) -> Result<String, HostError>
```

## 3. Daemon bridge and native management

Files: daemon MCP controller/bridge, native runner, daemon main/server dispatch,
GPUI workspace and focused native MCP view module.

- [x] Verify the installed runtime can invoke one ephemeral stdio bridge while
  model shell network access stays disabled. A failed probe is a blocker, not
  permission to use `danger-full-access` or pass upstream secrets to workers.
- [x] Add run-bound capabilities with expiry and revocation; check current grants
  and schema on every list/call, record bounded metadata-only receipts.
- [x] Test revoked grants between discovery/call, expired/foreign run tokens,
  paused connection and cleanup on run cancellation/completion.
- [x] Native UI: local/remote server setup, explicit local-process trust,
  connection status, discovered tools, per-workspace grants and pause controls.
  Hide secret fields and clear after submission. Remove default Linear setup.
- [x] Keep legacy Paseo provider MCP implementation unchanged and opt-in.

## 4. Generic responsibilities

Files: daemon responsibility scheduler, core evidence/policy helpers, protocol
responsibility commands, native responsibility editor and status views.

- [x] Test ten-minute due scheduling, durable restart, single-flight and bounded
  exponential failure backoff; do not replay every missed wake after sleep.
- [x] Agent wakes with the responsibility text and approved tools, not named
  Linear operations. Store source provenance and successful call receipts.
- [x] Deduplicate observations/tasks by responsibility/source/revision. Failed
  retrieval does not revoke assignments or imply source deletion.
- [x] Keep local fix assessment and independent review. Recheck responsibility,
  connection grants and source freshness at claim; unverifiable state holds.
- [x] Replace fixed polling with generic scheduling; migration revokes old
  auto-build permissions without deleting old task evidence or credentials.

## 5. Acceptance and handoff

- [x] Run fixtures using two arbitrary MCP servers and separate workspaces;
  prove one low-risk local fix, one held sensitive change, no duplicate task on
  subsequent wake, and no unintended cross-workspace tool access.
- [x] Run `cargo test --workspace --quiet -- --test-threads=4`,
  `cargo build -p neko -p neko-daemon --quiet`, and the updated daemon smoke.
- [x] Inspect native loaded/error/permission states without synthetic OS input.
- [x] Run independent spec review then quality review, resolve important findings,
  rerun affected tests and `git diff --check`.
- [x] Update README, architecture, AGENTS current section, and evidence with exact
  supported scope and remaining live-provider gates. Do not claim activated
  monitoring from fixtures or silently omit unsupported auth/protocol behavior.

The design was approved by the user's “build it.” The execution choice is
same-session work with bounded skill-directed subagents; no further design gate
is required. Any runtime/authentication blocker must be reported explicitly.

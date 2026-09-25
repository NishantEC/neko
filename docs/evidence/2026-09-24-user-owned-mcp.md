# User-owned MCP: implementation and verification

Implemented in `codex/personal-agent`, in the existing personal-agent worktree.
The native workbench foundation and this migration remain local/uncommitted.
No global Codex configuration or external account was changed.

## Delivered

- Native **Tools / MCP** setup for explicitly trusted local processes and remote
  Streamable HTTP servers. Multiple connections have independent workspace
  scope and credentials. Discovery does not grant permission; grants bind to
  the complete discovered tool identity/schema hash. Pause revokes grants.
- Daemon-owned Keychain credentials and OAuth discovery/PKCE/state/issuer,
  bounded asynchronous loopback callback, token refresh and credential-generation
  guards. Credentials are not included in snapshots or worker configuration.
  Neko credential JSON is excluded from clipboard history.
- Expiring per-run capabilities and a real stdio bridge to the daemon. Current
  authorization is checked after credential acquisition and again after MCP
  rediscovery immediately before dispatch. Calls, frames, schemas, results and
  receipts are bounded. No automatic retry of uncertain tool calls.
- Generic editable responsibilities, ten-minute durable wake claims, failure
  backoff, scoped source receipts, revision deduplication, pause and check-now.
  Automatic local fixes require fresh evidence and explicit permission plus a
  bounded low-risk assessment. Claim authority survives checkout/launch and is
  revalidated; sensitive/uncertain/manual tasks remain approval-required.
- Versioned, transactional legacy migration preserves tasks, results, source
  records and worktrees while disabling old Linear polling/standing grants.
  Oversize and future-version snapshots are not overwritten. Live legacy
  Linear command handlers are retired, not silently adapted into MCP grants.

## Deterministic end-to-end proof

`node scripts/smoke-workbench.mjs` uses real daemon IPC, SQLite, Git worktrees,
MCP transport and the real stdio bridge, with a deterministic agent process.
It configures two arbitrary MCP connections in separate workspaces entirely
through IPC—no database seeding or service credentials.

Verified: scoped discovery and calls, successful call receipts, foreign
connection denial, one automatic low-risk isolated fix, sensitive and manual
approval holds, repeated-wake deduplication, grant revocation, cancellation,
independent review and restart durability. Nothing is published.

First passing scratch run:
`/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-workbench-smoke-gXGX98`.
Final integration rerun also passed at
`/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-workbench-smoke-1rl2TD`.

## Actual model / bridge proof

`NEKO_SMOKE_LIVE=1 node scripts/smoke-mcp-live.mjs` passed once using
`/opt/homebrew/bin/codex` (0.155.1), existing CLI authentication, and a local
credential-free fixture. The real model called the actual Neko bridge, made
one successful upstream call, and returned two validated observations with
`eligible=false`. Zero tasks, no repository changes. The test responsibility
was paused and its daemon terminated. Duration: 27.7 seconds.

Receipt: `b110ff56d9143c1b747d6f407e5180ea`.
Scratch: `/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-mcp-live-gbRQE0`.
The runner retained read-only shell sandboxing, disabled shell network, ignored
global user configuration/rules and preapproved only Neko's two bridge tools.
This is real-model/local-fixture proof, **not external-provider OAuth proof**.

## Native proof and review findings

`2026-09-24-user-owned-mcp-native.png` is a window-scoped capture of the actual
GPUI app on isolated fixture data. It shows Tools / MCP, a discovered schema,
paused/revoked connection, responsibility status and a missing-grant error.
No synthetic keyboard/mouse input was used; native readback reported the
evidence window was not key.
`2026-09-24-user-owned-mcp-tasks.png` captures the loaded task view after a
native relaunch: completed manual task, low-risk result ready for review,
sensitive/manual approval holds and retained cancellation. The replacement
daemon logged “another instance is already running, exiting”; only the
original daemon PID remained. Both QA processes were stopped afterward.

The first non-activating capture retained startup pixels despite loaded state.
Reordering the evidence window once after its first successful snapshot made
the current frame visible; no production activation or keyboard-focus change
was added. Native QA also reproduced two daemons after reopening: the singleton
probe mistook an initial AttentionChanged event for a failed Pong. It now skips
bounded events with a deadline and never unlinks a connected nonresponsive
peer's socket. Tests cover event-before-Pong and preservation of live listeners.

Independent spec and quality reviews found and drove regressions for OAuth
callback timing, stale credential writes, oversized migration writes, stale
permissions during preparation/discovery, raw bridge frame limits, outdated
wake results, and the claim-to-builder authorization gap. Each was corrected
and its affected tests rerun.

## Final verification

`cargo test --workspace --quiet -- --test-threads=4` passed: **957 passed,
5 ignored, zero failures** (GUI 340, client 13, core 443, daemon 77,
harness 78, protocol 6). `cargo build -p neko -p neko-daemon --quiet`,
the final deterministic smoke, JavaScript syntax checks and `git diff --check`
passed. Existing dead-code warnings remain.

Singleton recovery now also holds an owner-only, no-follow startup lock across
probe, stale-socket recovery and bind. Concurrent recovery passed 20 repeated
runs. Its test fixture initially closed accepted sockets too early, causing
macOS timeout configuration to return EINVAL; retaining those sockets like the
real handler fixed the fixture. This race did not reproduce before the lock;
the symlink-rejection regression did demonstrate red/green behavior.

Final process inspection found no remaining QA app or daemon process. These
changes are local and uncommitted; the installed signed app is unchanged.

## Remaining boundaries

No user-chosen external MCP account was authenticated or activated. Live
browser login, Keychain prompts and compatibility with a particular provider
remain acceptance gates. Remote auth requires discovered metadata and a usable
public-client registration. DNS rebinding is not pinned. macOS synchronous
Keychain operations cannot be preempted; dispatch checks the deadline afterward.

Local MCP executables are trusted host programs, not sandboxed plugins. Tool
grants may permit remote side effects; annotations and model instructions are
not an enforcement boundary. Revocation cannot undo a dispatched action.
Source interpretation and risk classification are model judgments. The Codex
sandbox constrains writes, not confidentiality of reads across workspaces.
No marketplace, package auto-installation, arbitrary swarm, cloud execution,
cross-workspace sharing or automatic publication is claimed. The installed
signed Neko.app has not been replaced by this local debug build.

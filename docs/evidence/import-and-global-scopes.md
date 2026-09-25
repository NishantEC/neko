# Unified local import and global MCP scope verification

Verified on 2026-09-25 in the personal-agent worktree; no installed app replacement.

## Implemented boundary

The first-run Import step uses one read-only scan for Codex, Claude, and
portable Paseo-compatible metadata, plus global/workspace `SKILL.md` files and
documented schedule files. The scan returns a stable, redacted candidate
ledger grouped by source and scope. It never starts a local executable, reads
Keychain credentials, imports transcripts, or treats provider/live state as
authority. Unsupported items become warnings or candidates with a problem.

- Codex and Claude configuration discovery produces a secret-free preview. Applying
  explicitly selected candidates can copy credentials through the existing Keychain
  boundary. Discovery never launches an MCP server or grants a tool.
- Empty connection workspace denotes a global definition, not global authority.
  Each workspace must grant each current tool schema independently. Revoking one
  workspace leaves another workspace's grant intact; pausing the server revokes all.
- Source-disabled servers remain disabled from their initial creation. Import
  retries preserve existing definitions and avoid duplicate connections.
- Cached credentials expire after ten minutes and are evicted by the daemon tick.
  Fresh installs receive the stored preview even before creating a workspace.
- Project stdio commands with arguments that need unsupported working-directory
  semantics are reported as unsupported rather than silently run from another folder.
- Selected workspaces are created idempotently; selected skills become review
  proposals with `enabled=false`; selected schedules are saved paused. MCP
  definitions remain disabled and have no grants or discovered tools until the
  user reviews and activates them separately.

## Automated evidence

Core import tests (8), daemon import tests (2), initial-disabled connection regression,
global MCP policy tests (7), and the real local bridge workspace-isolation regression
passed. Import and global-scope changes passed independent spec and quality review.

`node scripts/smoke-import.mjs` passed against a built daemon and synthetic home:
Codex/Claude/Paseo-compatible metadata, global/workspace source grouping,
redacted credentials and skill bodies, one unsupported item, selected workspace
and skill apply, disabled MCP/skills, a paused schedule, no implicit grants or
external tool launch, and idempotent retry. Paseo causality is exercised by a
first discovery with no repository arguments: the workspace exists only because
the Paseo project metadata is read, and the Paseo live-state warning is present.
The fixture's executable MCP server writes a sentinel if launched; the sentinel
is absent after discovery and apply. The smoke deliberately did not write
Keychain credentials or authenticate a provider.

The existing onboarding unit harness also verifies the automatic Import-step
state machine: `cargo test -p neko --bin neko import_discovery` covers one scan per entry,
busy duplicate suppression, explicit retry, and reset after failure. No native
capture is claimed here.

## Scheduling foundation

Four recurrence tests cover DST wall time, durable interval anchors, invalid or
exhausted schedules, and old hourly anchors. Evaluation enables the library's
per-step limits and examines at most 10,000 occurrences in total. An older schedule
exceeding that budget fails closed; it is not silently rebased. This helper alone is
not a finished schedule runtime or schedule importer.

## Not proven here

External OAuth/account authentication, actual Keychain import acceptance, and manual
native onboarding interactions require separate evidence. These checks do not claim
that provider accounts, the installed app, or a real native capture were exercised.

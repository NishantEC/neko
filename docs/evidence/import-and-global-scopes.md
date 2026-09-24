# Import and global MCP scope verification

Verified on 2026-09-25 in the personal-agent worktree; no installed app replacement.

## Implemented boundary

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

## Automated evidence

Core import tests (8), daemon import tests (2), initial-disabled connection regression,
global MCP policy tests (7), and the real local bridge workspace-isolation regression
passed. Import and global-scope changes passed independent spec and quality review.

`node scripts/smoke-import.mjs` passed against a built daemon and synthetic home:
three discovered candidates, redacted credentials, source-disabled state, correct
global/project scope, workspace creation, no implicit grants, and idempotent reapply.
The smoke deliberately did not write Keychain credentials or start external tools.

## Scheduling foundation

Four recurrence tests cover DST wall time, durable interval anchors, invalid or
exhausted schedules, and old hourly anchors. Evaluation enables the library's
per-step limits and examines at most 10,000 occurrences in total. An older schedule
exceeding that budget fails closed; it is not silently rebased. This helper alone is
not a finished schedule runtime or schedule importer.

## Not proven here

External OAuth/account authentication, actual Keychain import acceptance, and manual
native onboarding interactions require separate evidence. These checks do not claim
that provider accounts or the installed app were exercised.

# Authenticated Linear lookup through the native daemon

## Successful real-model follow-up

The follow-up used `scripts/poc-linked-mcp-session.mjs` against the retained isolated daemon. Neko's existing OAuth flow completed successfully: `oauth=true`, `has_credentials=true`, no connection error. No OAuth retry or token extraction was needed. `Discover` returned 68 actual Linear tools from `https://mcp.linear.app/mcp`.

Only `list_issues`, declared read-only by the server, was granted to the proof workspace. A real Neko `SendMessage` launched the actual Codex CLI in `read-only` mode, listed the scoped tools, and called `list_issues` once with limit 3 through Neko's daemon bridge. The completed reply was:

> Lookup succeeded. The sole granted tool was list_issues; it returned 3 issues.

- Turn: `f209290c93ad3b4904f048167ab022ba`, `failed=false`.
- Actual daemon tool receipt: `9b00a40201cb05e07087a6c3599eae40`, `success=true`.
- Schema identity: `c9a1a98f9badd3f76401fa1af2627a19b75996569e8ec74dbd645d4a316c9223`.
- Exactly one receipt; zero tasks created. No external mutating tool was granted.
- The temporary grant was revoked in the script's `finally` block.
- Issue titles and descriptions were not copied into this evidence.

Boundary: this proves existing-source discovery → Neko OAuth → actual tools/list → workspace-scoped grant → real model → Neko bridge → authenticated Linear read → successful receipt → grant revocation. This was IPC-driven; native UI interaction and screenshots are separate evidence.

The proof daemon remains running for the parent agent's native UI inspection. Its isolated SQLite state and Neko-owned Keychain credential are retained. No production Neko configuration was changed.

### Separate validation issue found

Changing the proof workspace from its existing folder to a new temporary folder with `SaveWorkspaceWithFolders` failed with `Invalid workspace folders`. The old folder map is validated while `SaveWorkspace` changes the repository, before the new map is installed. No task history existed. Consequently the successful chat used the existing workspace directory with the daemon's read-only runtime, not a newly attached fixture folder. This must not be represented as filesystem isolation beyond the current Codex sandbox.

## Initial unauthenticated baseline

Executed `node scripts/poc-linked-mcp-readonly.mjs` against the current debug daemon on 2026-09-29. The script creates its own `NEKO_DATA_DIR`, retains it for inspection, and stops only its owned daemon. No model, authenticated issue query, external write, or background responsibility is run.

## Verified

- Real IPC created a non-Git workspace and discovered existing Codex/Claude configuration.
- Available HTTP Linear definitions: global `linear-personal`; workspace `linear-fc`; workspace `linear-hme`; and a Codex workspace `linear` definition. The older Claude SSE `linear` definition was unavailable.
- No Slack definition was discovered.
- The global `linear-personal` candidate linked successfully through `Mcp::LinkSource` without copying credentials.
- `Mcp::Discover` reached the transport boundary but returned `MCP handshake failed`, zero tools, `oauth=false`, `has_credentials=false`.
- Snapshot contained zero tasks. No tool grants were created.

## Authentication boundary

The source definitions provide HTTP endpoints but no inline bearer credentials. Current source linking does not reuse Codex/Claude OAuth stores: `LinkSource` initializes `oauth=false`, and source parsing only supports explicitly configured bearer/header/environment credentials. A populated OAuth cache belonging to another client is not proof that Neko is authenticated.

The successful follow-up above performed Neko's existing `Mcp::Authenticate` flow, storing credentials under Neko's own connection identity, then discovered tools and exercised a scoped read-only issue lookup. The initial handshake failure here is retained as the unauthenticated baseline; it is not the final outcome.

Evidence directory from the run: `/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-linked-readonly-3qohLM`. This is sanitized source state, not an export of OAuth secrets.

# Linked workspace capabilities

Status: approved product direction, engineering design for implementation.
Paper source: `neko`, page `p-1-0`, the edited `03`, `03a`, `03b`, `03f Skills`, `03f MCP connections`, and `04` artboards.

## Product rule

A Neko workspace points at an existing folder. Neko discovers instructions, skills, and MCP definitions from that folder and the supported user-level roots. It does not present another app's setup as a batch of things to copy. The UI shows each item's source, effective scope, and current readiness. The source file remains authoritative; Neko stores only a workspace registration, grants, provenance, and bounded discovery metadata.

Skills found in a workspace are visible without import. Neko may read their instructions during an agent run only after the workspace is trusted by the user. Skill content is rechecked at use time and bounded by the existing size and prompt limits. A changed skill remains visible but is held for review; it never grants tool authority.

MCP definitions are visible without import, but unavailable to agents until the user approves the connection for that workspace and its individual tools. Approval is never global, even for a global definition. Local process trust, credentials/OAuth, tool grants, and unattended responsibility authority stay separate decisions. A changed executable, URL, arguments, working directory, or tool schema invalidates the affected approval before another call. Missing or unsupported source configuration is shown as unavailable, not silently promoted to global or copied.

Neko's daemon remains the MCP host and per-run bridge. Agent processes continue to ignore ambient Codex configuration and receive only scoped Neko bridge capabilities. A linked definition is resolved and fingerprinted by the daemon; raw upstream credentials never enter a worker prompt or CLI arguments. Existing Neko-owned connections remain supported. Existing imported copies remain paused/unchanged until the user explicitly chooses to replace one with a link; migration must not delete a connection, grant, credential, schedule, or task.

## Onboarding and workspace UI

Step 3 starts with folders, not Codex/Claude/Paseo cards. Choosing a folder registers a workspace and triggers read-only discovery. The review shows Skills, Connections, and Workspace guidance. A skill row opens its source instructions. A connection row shows source, scope, and `Approve` or `Approved`; an unrelated workspace's item is labeled `Other workspace` and cannot be approved from the current workspace. Setup can continue with zero approvals. Step 4 groups discovered capabilities by scope and lets users review permissions; it does not imply that a discovered connection is already connected.

After setup, Tools & skills refreshes the selected workspace on opening and on explicit refresh. File changes may be detected by a bounded watcher or by the next view/run; no background tool process is launched by discovery. A source disappearing changes its row to unavailable and blocks calls. Duplicate definitions show one effective item only when source content and scope match; provenance lists every matching source. Differing definitions stay separate.

## Verification

Use temporary global and workspace config/skill fixtures. Prove: discovery has no side effects; no duplicate install; global and workspace scope; same-name differing definitions; skill changes held; no MCP dispatch before workspace/tool approval; source config and schema changes revoke eligibility; revoked or missing source blocks a later call; existing Neko-owned connections and data survive. Exercise real daemon IPC and one local stdio bridge fixture. Capture the native setup and Tools & skills screens against the Paper artboards. No real third-party credentials are required for tests.

## Delivery boundary

The Paper design is updated first. The current installed app still uses the legacy import flow until the linked discovery and authorization implementation has passed the above checks and a new app is deliberately installed. This spec does not authorize skipping those checks or clearing the user's current state.

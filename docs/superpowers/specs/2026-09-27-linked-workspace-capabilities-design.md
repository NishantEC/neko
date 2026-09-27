# Linked workspace capabilities

Status: approved product direction; onboarding simplified after Paper cleanup.
Paper source: `neko`, page `p-1-0`, `03 · Choose a workspace`. Earlier skill and MCP review artboards are preserved on the Archive page, not part of first run.

## Product rule

A Neko workspace points at an existing folder. It does not present another app's setup as a batch of things to copy. The source files remain authoritative; first run stores only the workspace registration.

Global and workspace skills are advertised to the agent by path and description without import. The agent can read a relevant SKILL.md in place. The catalog is rebuilt on each run and bounded by discovery and prompt limits. A changed local file is treated as the folder's current content; separately installed Neko skill proposals retain their explicit activation and content-hash review. Neither kind grants tool authority.

Source-linked MCP definitions are visible without import, but unavailable to agents until the user approves the connection for that workspace and its individual tools. Approval is never global, even for a global definition. Local process trust, credentials/OAuth, tool grants, and unattended responsibility authority stay separate decisions. A changed executable, URL, arguments, working directory, or tool schema invalidates the affected approval before another call. Missing or unsupported source configuration is unavailable, not silently promoted to global or copied.

Neko's daemon remains the MCP host and per-run bridge. Agent processes continue to ignore ambient Codex configuration and receive only scoped Neko bridge capabilities. Linked definitions are resolved and fingerprinted by the daemon; raw upstream credentials never enter a worker prompt or CLI arguments. Existing Neko-owned connections remain supported. A matching saved copy changes to a link only when the user explicitly chooses that action; this clears its tool grants without deleting the connection, credential, schedule, or task.

## Onboarding and workspace UI

First run is Welcome → optional Mac basics → Choose workspace → Today. Step 3 starts with folders, not Codex/Claude/Paseo cards, and offers the native folder picker. Choosing a folder registers it without importing or copying any skill, MCP definition, schedule, or credential. Existing workspace cards can be reused. No skills, connections, or scope-review tables appear in onboarding; those older explorations are archived in Paper. Setup may continue without a folder.

Neko agents receive a bounded catalog of available global and selected-workspace skills by path; they read a relevant skill in place. Neko-installed skill proposals remain explicitly enabled and content-pinned. MCP connections still require per-workspace and per-tool approval, with local process trust and credentials handled separately. Discovery alone never executes an MCP server or grants it access.

Tools & skills refreshes the selected workspace on opening and on explicit refresh. The daemon revalidates linked definitions on the next view/run; no background tool process is launched by discovery. A source disappearing blocks calls and revokes grants. Duplicate definitions show one effective discovery item only when source content and scope match; provenance lists every matching source. Differing definitions stay separate.

## Verification

Use temporary global and workspace config/skill fixtures. Prove: discovery has no side effects; no duplicate install; global and workspace scope; same-name differing definitions; skill changes held; no MCP dispatch before workspace/tool approval; source config and schema changes revoke eligibility; revoked or missing source blocks a later call; existing Neko-owned connections and data survive. Exercise real daemon IPC and one local stdio bridge fixture. Capture the native setup and Tools & skills screens against the Paper artboards. No real third-party credentials are required for tests.

## Delivery boundary

The Paper design was updated first. Code and the installed app are separate states: a new build is not installed or given access merely because this spec changed. Linked definitions remain unusable until connection trust, tool discovery and workspace grants are explicitly completed. This spec does not authorize clearing the user's current state.

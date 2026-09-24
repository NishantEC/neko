# User-owned MCP connections

Status: Product direction approved in conversation; written migration spec
awaiting review. No MCP migration is implemented by this document.

## Product boundary

Neko is a personal agent with user-added tools, not a Linear client with an
agent bolted on. It ships with no required service connection. Users add MCP
servers, authorize their use within workspaces, and give Neko responsibilities
in plain language. Linear is an optional server, not a built-in domain model.

Neko continues to own tasks, memory, schedules, workers, isolated worktrees,
approvals, and results. Codex CLI remains an execution adapter. Adding a server
must not install it into this Codex desktop session or modify the user's global
Codex configuration.

## Architecture choice

1. **Daemon-owned MCP host and scoped agent bridge — selected.** One owner for
   connection lifecycle, credentials, tool discovery, authorization, and audit.
   More initial work, but revocation and workspace boundaries do not depend on
   which agent runtime is used.
2. **Pass server configurations directly to Codex — not selected.** Faster for
   a demonstration, but distributes secrets and duplicates policy across runs.
3. **Service-specific connectors behind an MCP-shaped interface — rejected.**
   Leaves Linear concepts embedded in the product and does not satisfy the
   approved direction.

The existing `neko-core::mcp` speaks specifically to Paseo. Leave that legacy
opt-in surface isolated; build the generic host as a separate module rather
than silently changing its existing consumers.

## Deliver in three dependent slices

### 1. Connections and permissions

Replace the default Linear setup UI with **Tools / MCP servers**. A connection
has a stable ID, user label, transport configuration, credential references,
status, and explicit workspace grants. Multiple instances of the same server
remain distinct, including separate credentials and consent. Nothing is
shared between workspaces by default.

Support local stdio processes and remote Streamable HTTP. Use the maintained
official Rust SDK where it covers the required protocol and authentication
flows. Pin and test the actual supported protocol revisions; do not claim
universal compatibility or hand-roll the protocol around Paseo assumptions.

Local setup takes an executable and argument array, not an implicit shell
command. Show the launch configuration and require explicit trust before
starting it: an MCP subprocess is executable code, not a sandboxed plugin.
Only explicitly configured environment values are provided, plus a minimal
launch environment. Package installation is a separate user-approved action.

Remote setup takes a URL. Support browser authorization where the server
provides it, plus explicit secret configuration for servers that require it.
MCP does not eliminate authentication. Store secrets in macOS Keychain; never
include secret values in snapshots, model prompts, logs, or process arguments.
Reject URL-embedded credentials. Restrict cleartext HTTP to explicit loopback
endpoints. Validate authentication discovery and redirect destinations and
never forward credentials to an unrelated origin. Bind authorization state,
PKCE, issuer, and account identity to the specific connection attempt.

Discover and display bounded tool schemas. Newly discovered tools and changed
schemas do not inherit old grants automatically. Server annotations are hints,
not proof that a tool is read-only. Users approve the tools allowed unattended;
everything else needs approval or stays unavailable. Connection, authentication,
and discovery failures have separate visible states and retry controls.

### 2. Agent tool access

Expose approved tools through a Neko-owned per-run bridge. The daemon resolves
the connection and checks the run, workspace, current grant, and tool schema on
every call. Workers receive scoped capabilities, not upstream credentials or
all configured servers. Capability tokens are short-lived and invalidated on
run completion, cancellation, or revocation. Discovery alone never authorizes
execution. Pause/revoke prevents subsequent calls immediately; in-flight calls
are cancelled where possible, without claiming to undo completed remote work.

The bridge must work without disabling the existing worker sandbox or enabling
unrestricted worker network access. Verify this against the installed runtime
before integration. If that cannot be proven, report the runtime constraint;
do not silently bypass the daemon or relax sandbox settings.

Keep bounded call receipts identifying run, workspace, connection, tool,
schema version, timing, and outcome. Treat tool descriptions and results as
untrusted content, not instructions or permission grants. Avoid retaining
secret-bearing arguments and responses. Bound discovery size, response size,
call time, concurrency, and retry attempts. Never automatically retry an
uncertain mutating call.

### 3. Responsibilities without service-specific polling

Replace the hardcoded Linear polling loop with durable responsibilities:
instruction, workspace, selected connection grants, enabled state, wake policy,
last attempt/result, and next due time. Default monitoring wakes every ten
minutes while the daemon is running, with one active run per responsibility,
bounded work, failure backoff, and no catch-up storm after sleep or restart.
This does not promise execution while the computer is asleep or offline.

The agent discovers and calls the selected approved tools to investigate the
responsibility. Do not assume every MCP server supplies issue events, a
subscription API, or Linear-shaped results. Missing capabilities become a
visible blocker, not an invented successful sync.

Store generic source evidence: connection ID, source identifier, content
fingerprint/revision, retrieval time, and supporting call receipts. Deduplicate
observations and task creation across wakes and restart. A failed/incomplete
retrieval must not be treated as deletion or revoked assignment.

Preserve the tested low-risk local-fix workflow, but replace its Linear-specific
eligibility inputs. At claim time recheck responsibility permission, connection
grants, and fresh source evidence. If assignment or revision cannot be verified
through available tools, ask for approval rather than granting automatic edits.
Existing risk assessment remains model judgment, not proof. No remote writes,
PR creation, push, or merge is implied by permission to prepare a local fix.

## Data preservation and UI migration

Version the persisted snapshot and migrate transactionally. Preserve tasks,
plans, decisions, results, and worktrees. Keep old Linear source records as
read-only historical evidence with their original provenance. Disable legacy
polling and standing authorization on migration; old API-key access must never
be silently converted into an MCP grant. Do not delete old credentials or
source records automatically. Explain that reconnecting through a user-added
server is required. Retire default Linear commands/forms only after migration
and compatibility tests exist.

Keep the launcher and full workspace distinct. The workspace manages MCP
connections, responsibilities, approvals, and detailed activity. The launcher
provides quick access and attention status, not a second setup system.

## Verification and acceptance

- Protocol fixtures cover local and remote discovery/calls, pagination,
  malformed responses, disconnect/reconnect, timeout, cancellation, and bounds.
- Authentication tests cover per-connection isolation, redirect/state/issuer
  validation, token expiry, failed refresh, and credential redaction.
- Authorization tests prove cross-workspace denial, default-denied tools,
  schema-change invalidation, and revocation between planning and execution.
- A deterministic end-to-end fixture uses two independently named MCP servers,
  no Linear-specific tool names, and two workspaces. A responsibility discovers
  a source item, prepares an isolated local fix, and produces a review result.
  Sensitive/uncertain work remains held; repeated wakes do not duplicate tasks.
- Migrate a populated old snapshot without losing tasks or evidence. Run the
  full Rust suite and inspect real native connection/approval/error screens.
  Separately verify one user-chosen real server before calling live monitoring
  active. Fixture success is not live authentication or provider proof.

## Non-goals

No marketplace, automatic installation of untrusted packages, universal support
for every MCP extension, arbitrary unattended external writes, cross-workspace
data sharing, or unrestricted agent swarm in this migration. These exclusions
do not prevent later expansion through explicit capabilities and permissions.

## References checked on 2026-09-24

- https://modelcontextprotocol.io/specification/2026-07-28/basic/transports
- https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization
- https://github.com/modelcontextprotocol/rust-sdk

The transport and authorization requirements inform the target contract;
the selected SDK's actual coverage must be verified during implementation.

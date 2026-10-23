## Current standalone workspace (2026-09-24)

The native workspace now follows system appearance, with Light / Dark overrides
in Settings and the sidebar. Shared typography uses 32pt rounded page introductions and
15pt body text, with 40pt page insets and bounded 880pt management / 760pt chat
content. Native sidebar and toolbar keep platform materials; the floating chat
composer uses Liquid Glass on macOS 26 and a material fallback below 26. Content
and evidence stay opaque. Preserve `stableSplitPane`, the native top safe area,
and navigation-owned drafts when changing these surfaces. See
`docs/design/native-polish.md` and `docs/evidence/native-liquid-glass.md`.

The selected Orbit visual study (2026-10-06, 02 · Native glass) refines existing
components only: neutral near-black dark surfaces, 24/8/16 conversation spacing,
shared attachment chips and a circular composer action. Home, ticket chats and
Design lab share the composer surface. Reduce Transparency or Increased Contrast
replaces its background with an opaque surface without replacing the editor.
Navigation, control behavior and runtime authority remain unchanged. See
`docs/design/orbit-visual-style.md`.

Design lab (2026-10-06) is an interactive native preview page. Inline is the
selected composer direction; Separate exposes individual model/effort/speed
controls. Both reuse production selector views against labelled sample metadata.
Its observation model has no AppModel, IPC or provider dependency; sends and
Apply sample stay local. Plugin switches do not install or grant anything.
Home/ticket composers use real per-conversation settings and capability discovery.
Task membership stays in the sample conversation, derived by `TaskTeamSummary`
from distinct child ticket IDs in approved splits, never phases or workspace totals.
Decorative beads use a pointer-driven Metal pearl shader with no idle timer.
Reduce Motion disables movement; Reduce Transparency/Increased Contrast use a
solid fallback. System Liquid Glass remains a separate native material.

As of 2026-10-03, the native client has Home, All agents (List/Board), full-page
agent chats and a system source-list sidebar with bounded live agent groups.
Ticket drafts and return filters survive navigation. Memory / Working style
manages confirmed contextual preferences. Decision episodes persist independently
of task status, with immutable observations and contextual corrections; preferences
never grant execution or publication authority. See `docs/architecture.md`.

As of 2026-10-05, Home and ticket chats share the AppKit growing composer,
attachment strip and revision-safe draft store. Drafts survive navigation, not
app restart. Home send/queue/stop and ticket `ReplyToTask` remain separate adapters;
Return never stops work. Settings → AI owns global runtime defaults. Composer
runtime pins are per Home profile/workspace or per ticket. `runtime_selection`
uses tool-free model judgment over advertised capabilities; independent pins
win, unsupported choices fail, and classifier failure uses a validated default.
Automatic paid speed needs its own saved preference. There is no quota-aware
failover. Captured choices/reasons persist in a bounded 200-dispatch history;
active workers retain them through recovery, and changes affect the next run.
Manual pin saves perform a bounded inference check before commit. Requested
service tier is not proof of the tier actually served.
Settled agent chats expose the result and review while folding older plans,
conversation and decisions. Rich output wraps table cells and makes available
terminal/diff preview limits explicit. This is presentation, not runtime compaction.

Home prompt history filters by exact workspace and profile before choosing up to
12 completed messages. A 12 KiB transcript budget preserves complete recent
messages in order, with explicit omission notices; only an individually oversized
newest message uses a UTF-8-safe tail. Task command receipts budget fields before
JSON serialization, retaining exit status and explicit shortening/incomplete flags.
Native evidence expands into readable commands and saved output; damaged legacy
records remain visibly incomplete. Retained activity can span earlier attempts and
is not a full execution journal. See `docs/evidence/chat-continuity-evidence.md`.

As of 2026-10-02, `/Applications/Neko.app` is built from `native/NekoKit`
(SwiftUI/AppKit) over the same daemon and workbench protocol. The GPUI
`workspace.rs` client described in the historical implementation record below
is not the native installer's source.

The current default supersedes the historical agent-browser and accessory-only
window decisions below. Neko opens a regular, persistent `workspace.rs` app
window; the hotkey remains a separate palette, dismissed with native
`order_out`, never app-wide `hide`. Dock reopen and the menu open the workspace.

Native work uses `neko-protocol::workbench`, daemon-owned bounded SQLite
snapshots in `neko-core::workbench`, the daemon's `workbench` supervisor, and
`native_runner` ephemeral Codex CLI processes. **By default (2026-10-11) a ticket
is a normal writing session**: it builds straight away in its isolated worktree
with no read-only plan or approval step (`Snapshot::plan_first` off). The agent
itself stops on critical work (`NEEDS_APPROVAL:` → AwaitingApproval; approving
builds it without asking again), closes tickets with nothing to do (`NO_CHANGE:`
→ Completed; a reply reopens), and routes `QUESTION:` through recovery first.
Read-only replies, pending split proposals and the "Plan before working" setting
keep the older flow: a scout plans read-only,
explicit approval or a direct human work request allows an isolated-worktree build, and a
independent reviewer returns a result for human review. Authorized builders and
reviewers have full local filesystem access for dependency setup, caches and tests. Workers never push;
once an independent review passes, the daemon commits the reviewed worktree, pushes its task branch and opens
(or reuses) a GitHub pull request via `gh` (`native_runner::publish`, `workbench/publish.rs`). Nothing merges
unless you click Merge (see "Pull requests after they open").
Before committing, a read-only agent reads the staged diff and ticket evidence and writes a Conventional Commits
subject (the commit and PR title; repo conventions for scope) plus the description (Issue, Impact, Root cause, Fix,
Options considered, Verification, Risks; signed `` `=^.^=` neko ``). Both are rewritten on every push to an open
PR, and landing on main uses the same subject. If the agent fails, the evidence is the body and the title is `fix: …`.
Repositories listed in `Snapshot::land_on_main` (per checkout, toggled in a task page’s ⋯ menu) instead
fast-forward local `main` with nothing pushed: a moved `main` is rebased onto, a conflict or an overwrite of
local edits is refused, and the ticket records `landed_commit`. The `neko` repo itself is set to land on main.
Every eligible ticket starts its own worker without a fixed global cap, but at
most 2 tickets per repository hold a writable run at once
(`Snapshot::max_writable_per_repository`, read-only phases uncounted; see
"Resource limits"), and new workers are paced (`workbench/admission.rs`): a burst of
six, then one every ten seconds, and none while the one-minute load average is
20 or more (this machine restarted at load 240 on 2026-10-11). Running work is
never touched. Active claims prevent duplicate
workers for one ticket; dependencies, approvals and scope checks still apply.
Git checkout creation serializes per shared repository with cancellable waiting;
model work stays concurrent. After that lock is released, a new task worktree is
seeded with APFS copy-on-write clones of the source checkout's Git-ignored
`node_modules` folders (`native_runner::seed_dependency_folders`, best effort):
the agent's own install then reconciles lockfile differences instead of a full
download, a seeded folder never appears in snapshots or review, and any failure
leaves the plain checkout (measured on hme/athena: 39s and 62 MB of real disk
for a 2.3 GB `node_modules`). Restarted split parents wait for verified children.
The selected runtime runs each phase; quota-aware provider switching is not
implemented. An awaiting
plan can request a read-only two/three-subtask proposal; explicit approval
creates isolated child tickets using those same admission checks. Verified child patches
integrate only into the parent task worktree, followed by independent review.
Structured reviewer verdicts require successful command receipts and complete
host-observed diff coverage; findings, missing evidence and conflicts fail
closed. A fresh recovery supervisor investigates worker errors, rejected reviews
and scout questions on directly authorized work. It directs the same saved
agent through up to three recovery attempts, or asks one necessary user-owned
question. The same failure (compared case-, spacing- and number-insensitively)
coming back after an attempt stops recovery early, and each reviewer sees the
earlier blockers. Codex/Claude/OpenCode runs stop when one command fails with
identical output four times; Codex runs also stop after ten silent minutes with
no command running (`native_runner::StallGuard`); quiet long commands are allowed.
Processes a run started in other process groups are killed when it ends. Unchanged patches may still represent dependency/test setup progress.
Every repaired result receives a fresh independent review. HEAD, index, Git
control metadata, tracked content (including preexisting dirty files) and
nonignored untracked files are captured before/after reviewers and recovery
supervisors; mutations or snapshot failures reject the run. Ignored caches and
dependencies may change. This is detection, not isolation or a preventive sandbox.
The original diff base, captured authority, budget and dependency scope survive
the loop; split parents do not gain a builder that could rewrite child work.
Read-only human requests, planning, chat, background source checks and tool-free
extraction remain restricted. Publication and scoped MCP authority do not expand.
Cancellation/restart preserve task work, including failed work. **A worktree
exists only while an agent works in it (2026-10-17)**: a sweep
(`workbench/parking.rs`, every 60s, tickets idle 2+ min, never with a live
worker, an in-flight publish or a split) commits unfinished work to the
ticket's own branch as one `neko: parked work in progress` commit, removes the
folder and records `Task::parked`; running or publishing it again calls
`ensure_restored`, which recreates the folder at the same path (so saved agent
sessions still resume), takes the parked commit back off and re-seeds
dependencies. Ignored output (deps, caches) is not carried. Changes for a
parked ticket are read from its branch. A recorded folder that vanished
without parking is recreated from the branch too (see "Resource limits"). Full-access
workers are trusted local processes; do not describe worktrees or scoped prompts
as filesystem security boundaries. See `docs/evidence/full-access-supervision.md`.

Tickets are agents (2026-10-03). Each ticket opens as a conversation
(`TicketThread.swift`) and `ReplyToTask` hands your reply to its agent:
replies re-plan in the preserved worktree, including review feedback. A new reply
invalidates the old claim and cancels any active worker before admitting another
phase. A tool-free intent call in an empty scratch directory receives only the
unread human messages, one at a time in order: a direct work request authorizes local continuation,
questions or no-change instructions persist a read-only restriction, and factual
context preserves the existing authority. Malformed/unavailable interpretation
fails read-only. Durable reply revisions prevent stale results, duplicate
interpretation, rapid clarifications swallowing unread restrictions, and old notes
overriding a newer Start/Approve action. Source
text never enters this interpreter. Planning and building continue
one saved Codex session per ticket (`task_sessions`, `exec resume`; authorized
execution uses `--dangerously-bypass-approvals-and-sandbox`); reviewers are always fresh and ephemeral; a session
that can't resume falls back to a fresh start from the ticket history.
Codex exec sessions did not appear in the Codex app's thread list when
checked. The agent decides the next move: a supervisor `ask_user`/`skip` or a
scout's closing `QUESTION:` line on plan-only work waits for you. Directly
authorized scout questions first reach the recovery supervisor. A fix builds only when you
started the ticket (`StartTask`, drag to Working), directly requested local work
in its chat, or allow starting. Directly requested work uses the scout instead of
unattended source triage; missing reproduction or tests become investigation
steps, not automatic questions. Unattended watch policy, scoped tool grants
and publication boundaries are unchanged. A supervisor question prevents
automatic readmission under an old prepare-fix assessment.
Daemon restarts resume interrupted work up to twice. Repository discovery
matches whole words and ignores generic names (`init`, `web`, ...).

User-owned MCP replaces the built-in Linear integration. The separate
`neko-core::mcp_host` module owns SDK transport, workspace-scoped tool access,
Keychain credentials/OAuth, run capabilities, source receipts and generic
responsibility policy. The legacy Paseo-specific `mcp` module is unchanged.
Versioned migration retains old tasks, sources, worktrees and credential IDs,
but disables legacy polling/away authority. No global Codex configuration is
modified. Never reintroduce a service-specific connector as the default.

Users add local executable/argument configurations or remote HTTP URLs in
Tools / MCP, then discover tools. Discovered tools become available in the
connection's workspace without a per-tool switch. Enabling a responsibility
with selected connections separately authorizes unattended checks. Local
executable trust is separate; server annotations are not a security boundary.
A per-run daemon bridge keeps upstream credentials out of worker configuration
and rechecks current connection scope and schema.
**Agents run with everything the user's own CLI has (2026-10-11, owner
decision: "it replaces a developer").** Codex agents load the user's config and
rules, skills, plugins, hooks, web search, sub-agents and browser/computer use
(only features that wait for a human answer stay off: `UNATTENDED_DISABLED_FEATURES`).
Claude Code agents load the user's settings, CLAUDE.md, hooks, plugins, skills
and MCP servers with every built-in tool and `bypassPermissions`. OpenCode
loads project config and may fetch. Neko's bridge is added alongside. Tool-free
classifiers (reply intent, runtime selection, memory extraction) stay fully
isolated. Two checks remain because they are about correctness, not permission:
read-only phases stay off the disk, and a reviewer cannot change what it reviews.
Agents may message people and update issues; committing, pushing and pull
requests stay with Neko so only reviewed work leaves the machine.
Remote tools may mutate systems when called: local-fix authority is not
publication authority and cancellation cannot undo completed remote effects.

Generic responsibilities run a full model wake at most every ten minutes, with
durable due claims and backoff capped at one hour. A successful wake records its
read-only tool calls as a probe (`McpState::probes`); every three minutes the
daemon replays them through the normal scoped bridge and compares normalized
result fingerprints (volatile fields such as counts, cursors and last-seen
times ignored). No change means no model run. Any change, probe error, edited
instruction/connections, a manual Check now, or three hours since the last full
wake runs a full wake. A failed wake keeps the old probe so a change is not lost. A wake
whose tool calls failed is marked `Degraded:` and records no probe, so a
blind watch keeps running full checks instead of a probe of its few working
reads saying "no change". A call refused because the vendor changed the tool
since discovery rediscovers that connection (at most every 10 minutes),
re-grants it and retries once (`Host::bridge`); a recovered call does not
degrade the wake. Oversized agent event lines (a huge tool result) are skipped
rather than failing the run; total output per run is capped at 64 MiB. Successful scoped receipts back source
observations; a fresh unchanged actionable source plus a bounded low-risk
model assessment can authorize a local fix when explicitly enabled. Manual
tasks, missing evidence and sensitive/uncertain changes still need approval.
Editing responsibility instructions/connections invalidates old eligibility.
Trusted, connected sources with a granted read-declared tool that lists changing work now get one
plan-only watch per available workspace without a second activation. An
untried chat suggestion for that source is activated instead of duplicated;
durable scope markers keep later manual pauses paused. Background watches may
call every granted tool, writes included, and act as the user would; only the
timed change-check replays are limited to tools declared read-only, so a write
is never repeated.
No automatic installation, marketplace, arbitrary swarm, cloud execution,
cross-workspace sharing, automatic merging or deployment is claimed. Legacy
Codex/Paseo providers remain opt-in via `NEKO_LEGACY_AGENTS=1`.

`NEKO_DATA_DIR` isolates socket/database/task worktrees for development without
changing HOME. `scripts/smoke-workbench.mjs` exercises real daemon IPC, storage,
approval, worktrees, result review and restart with a deterministic child
fixture, not a live model or an authenticated user MCP account. See README and architecture
for setup, permissions, and current limits.

neko is a hotkey-summoned, GPU-rendered launcher for macOS, written from scratch
in Rust on [GPUI](https://gpui.rs) (Zed's UI framework). See `README.md` for how
to run it and `data/dim/plan.md` (in the firstmate home, not this repo) for the
full product plan.

# Neko personal-agent platform — product design

## Decision

Neko becomes a standalone, local-first personal-agent platform for work. It
owns the task graph, personal context, workspaces, agent lifecycle, worktrees,
approvals, and user experience. Its agents are called **Neko agents** and are
identified by their role, not by a second product name.

Firstmate and Paseo are architectural references only. They are not Neko
dependencies, task sources, adapters, or visible product surfaces. Codex
Desktop is not a Neko integration either. The local Codex CLI may be the first
implementation of Neko's private agent-runtime contract, but it is invisible
to the person using Neko: Neko owns the task identity and UI from creation to
outcome.

The command center remains the fast, hotkey-summoned front door. A persistent
Neko workspace is the place for work that needs attention. They are two speeds
of the same product, not two competing apps.

## Product outcome

Neko turns connected work signals into safe, complete outcomes:

1. It reads the sources the person explicitly connects.
2. It correlates related source records into a proposed outcome.
3. It ranks the outcome using the person's priorities, project context,
   current work, and explicit autonomy policy.
4. It either presents a decision, queues the work, or creates a Neko task.
5. A Neko task can create a temporary team of role-based Neko agents.
6. The parent task gathers evidence and returns a PR, report, decision request,
   cancellation, or a completed outcome.

Neko should feel proactive but calm. It batches routine information, keeps
healthy work quiet, and interrupts only for a genuine decision, risk, blocker,
or completion.

## Neko agents

Neko agents are native workers created and supervised by Neko. A task has one
parent outcome and can create a small, bounded team:

| Role | Responsibility | Default authority |
| --- | --- | --- |
| Scout | Map context, reproduce a problem, identify constraints | Read-only |
| Builder | Implement the smallest correct solution | One isolated worktree |
| Reviewer | Challenge correctness, scope, and risk | Read-only against candidate output |
| Verifier | Run relevant checks and attach evidence | Test/tool scope granted by parent |

Every agent has a goal, a bounded tool scope, a budget, a finish condition, and
a parent task. Builders use isolated worktrees by default. Scouts may inspect
the main checkout only in read-only mode. An agent cannot merge its own result
into the task outcome; the parent records an explicit accept, reject, or
escalation event.

### Runtime boundary

Neko defines a private runtime contract with `spawn`, `send`, `read_bounded`,
`stop`, and `health` operations. The contract receives Neko's task and agent
ids and returns runtime state; it never makes the runtime's own sessions the
source of truth.

The first runtime may invoke the local Codex CLI to access capable local coding
agents. That is an implementation detail. Neko does not show Codex thread ids,
Codex tasks, provider terminology, or Codex Desktop UI.

## Workspaces and source connections

A Neko workspace is an independent work domain. It owns projects, repositories,
task history, connected sources, memory, and autonomy policy. A person can have
many workspaces.

A source connection is separate from a Neko workspace. It has a provider,
account/workspace identity, credential reference, granted scopes, sync policy,
and revocation state. A Neko workspace can use one or more source connections;
a source connection can only be shared with another workspace through an
explicit configuration.

The first source integration is Linear. Neko must support multiple independent
Linear workspaces from its first release. Each connection has its own selected
teams/projects, permissions, and sync state. External source records are
identified by provider plus connection plus the provider's stable identifier;
an issue key alone is never globally unique.

### Linear first slice

The first Linear slice supports:

- connecting more than one Linear workspace;
- selecting which teams and projects each connection may index;
- an origin-labelled unified priority inbox;
- source evidence on every proposed outcome;
- conversion of a selected issue into a Neko task;
- independent read/write authorization and revocation per connection.

External Linear writes—creating an issue, changing state, or commenting—remain
explicit and opt-in per workspace. They must never happen merely because an
issue was indexed or because another workspace permits similar writes.

Slack, Gmail, and GitHub follow the same source-connection model after Linear.
They are not required for the initial shippable slice.

## Cross-workspace federation

The top-level personal Neko can read fleet summaries across all connected Neko
workspaces: task title, state, priority, owner, timing, and whether action is
needed. It can form a cross-workspace plan from those summaries.

Neko agents do not receive that broad visibility. They receive their task's
workspace context by default. A parent Neko task may create an explicit,
read-only context capsule to bridge information from another workspace. The
capsule records its source workspace, source records, selected facts, recipient
task, creator, and expiry. It contains only task-relevant material, not raw
workspace history.

No external write permission crosses a workspace boundary automatically. A
cross-workspace plan that requires an external write must ask under the target
workspace's policy.

## Time, heartbeat, and attention

Neko tracks a durable, typed event log rather than depending on raw agent chat
or terminal text. Each task and agent reports one of: `queued`, `working`,
`waiting`, `blocked`, `verifying`, `completed`, `cancelled`, or `lost`.

Four mechanisms use that state:

| Mechanism | Trigger | Behavior |
| --- | --- | --- |
| Agent heartbeat | Runtime state or bounded liveness interval | Records health; stays silent while healthy |
| Event wake | Blocker, failure, approval, completion | Creates an actionable inbox event |
| Scheduled beat | Source refresh, deadline, paused-task revisit | Re-evaluates priority without unnecessary interruption |
| Escalation heartbeat | Missing expected progress | Investigates first, then asks the person with evidence |

The scheduler must be cheap, durable across restart, idempotent, and quiet when
the state has not materially changed. It is not a high-frequency polling loop
or an excuse to send repeated status notifications.

## User experience

### Command center

`⌥Space` remains fast and dismissible. It answers “what needs me now?”, finds
tasks and commands, starts a safe action, and opens a task in the workspace.
It does not render a fake transcript.

### Workspace

The persistent Neko workspace is task-first, not chat-first. Its default task
view contains the goal, source evidence, plan, role-based agents, event
timeline, worktree/files/tests, approvals, and outcome. Conversation is a
supporting detail when it improves understanding.

The global inbox contains actionable blockers, approvals, completed outcomes,
and Neko's proposed next outcomes. Every proposal states why it was ranked and
links to the source records that produced it.

### Memory scopes

Neko keeps personal, workspace, project, task, and agent memory separate.
Personal memory informs ranking across workspaces. Project/task memory is only
available within its workspace unless the parent task creates a context capsule.

## Safety and failure behavior

- Source reads require an explicit enabled connection and selected scope.
- Credentials are held by an OS-backed credential reference, never in task
  bodies, event logs, agent prompts, or diagnostic output.
- Irreversible, destructive, access-changing, financial, and external-send
  actions require the workspace's explicit approval policy.
- A stopped runtime makes its agent `lost`; it never silently implies success.
- A missing source connection leaves retained evidence readable but marks it
  stale and prevents new source actions.
- A failed context-capsule grant leaves the target task unchanged and exposes a
  clear reason to the parent task.
- A task cannot become complete until its declared finish condition and required
  verification evidence are recorded.

## Delivery sequence

### Phase 1 — native task spine

Build Neko's durable task graph, workspace/project model, agent lifecycle,
event log, isolated-worktree manager, runtime contract, one-agent execution,
health state, and task-first workspace UI. Prove one local coding outcome from
goal to review-ready result.

### Phase 2 — small, visible teams

Add parent/child Neko agents, Scout/Builder/Reviewer/Verifier roles, bounded
context handoff, result acceptance, verification gates, stop/retry, and
heartbeat-based attention.

### Phase 3 — Linear intelligence

Add multi-workspace Linear connections, selected team/project indexing,
origin-labelled priority inbox, issue-to-task conversion, and per-workspace
authorization. Add external writes only after the read and task path is
proven.

### Phase 4 — personal work intelligence

Add workspace federation, source correlation, priority explanation, context
capsules, scheduled beats, and a quiet briefing. Then add Slack, Gmail, and
GitHub through the same connection model.

## Verification

Each phase needs pure-state tests for task transitions, permission decisions,
context-capsule construction/expiry, heartbeat de-duplication, and source
identity. Runtime integration tests must prove no agent leaks a worktree, a
lost worker cannot be reported complete, and a stopped task cannot receive more
work. UI tests must prove the command center never blocks on a source refresh,
the inbox labels every source workspace, and task detail presents evidence
instead of invented activity.

The first manual evidence pass must create one task in an isolated worktree,
observe a healthy heartbeat, force a meaningful blocker, approve or decline it,
and inspect the completed outcome and its verification record.

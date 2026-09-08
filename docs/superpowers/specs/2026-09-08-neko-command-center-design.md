# Neko command center — product design

**Status:** approved direction, awaiting spec review

## Product decision

neko is a local-first Mac work surface with three deliberately different
depths. It replaces the daily *quick* use of Raycast without pretending a
command palette is a full development environment. It also replaces the
constant switching between coding-agent clients and work trackers when a task
needs sustained attention.

The three surfaces share one local work graph; they do not share one crowded
window.

| Surface | Job | It deliberately does not do |
| --- | --- | --- |
| **Palette** | Find and complete a tiny action in seconds | Render a task transcript, issue board or terminal |
| **Neko app** | Stay with one coding task until it is done | Search applications or act as a clipboard browser |
| **Work inbox** | Turn external signals into prepared, ranked decisions | Autonomously modify code or communicate externally |

## Palette: a Raycast replacement

The existing summon panel stays the primary interaction. `⌥Space` searches
applications, files, System Settings, clipboard history and commands exactly
as it does now. It adds only the agent/work information that changes what a
person should do immediately:

- four compact live task tiles, ordered **needs you → working → recent**;
- a backend-neutral **Needs you** section for a pending agent approval,
  explicit question, or an actionable work-inbox item;
- quick commands such as *Open neko*, *New Codex task*, *Review approval*,
  *Snooze issue*, and *Open clipboard history*.

The palette never shows a full conversation, a project sidebar or a feed of
every Linear and Slack message. Its exit condition is one action, then it
hides. A task detail row can open the Neko app or the original provider.

## Neko app: a coding workspace

The Dock app has a persistent shell: **Command Center**, Needs you, Tasks,
Changes, Terminals, Projects and History. Its central view is one selected
task: the live agent conversation, tool activity, approval cards, follow-up
composer, diff/file-change summary, workspace and model. This is the surface
where a person can tell a provider “fix it, keep it low priority, stop,” or
review what it did.

Codex is the first native adapter and uses the documented local app-server;
the approved [Codex control-plane design](2026-09-08-codex-control-plane-design.md)
defines its transport, task view and safety model. Paseo stays as a separate
adapter, so an existing session remains visible where the user still has one.
No adapter is allowed to claim a capability it cannot perform: task actions
and source identity remain visible.

The app does not duplicate account settings, plugin management, provider
marketplaces or every provider-specific full-screen view. `Open in Codex` and
`Open in Paseo` are first-class handoffs for those cases.

## Work inbox: proactive, controlled intake

Linear, Slack and GitHub are inputs to the work graph. An incoming assignment,
mention, review request or changed agent state creates a normalized item with:

- source and deep link;
- project/workspace association, when reliable;
- urgency and confidence;
- a short grounded summary and suggested next action;
- an optional linked coding task.

The item surfaces in the palette only when it needs a decision. Otherwise it
appears in the app inbox, where it can be reviewed, snoozed, assigned a lower
priority, linked to an existing task, or turned into a new task.

**Default safety policy:** Neko may read permitted data and prepare a plan
proactively. It can start code work, post a message, alter an issue, push, or
raise a PR only after an explicit user action. A later project-level policy may
authorize a narrow named workflow—for example, “triage new `neko` bug issues”
—but must name its project, trigger, allowed actions, and approval point. It
is off by default, visible in the app, and revocable.

## Shared work graph

`neko-core` gains provider-neutral domain objects instead of making UI code
understand Slack, Linear, Codex or Paseo independently:

```
Source event → WorkItem → Project link → Suggested action → Task run → Outcome
```

- **Source event:** an immutable received fact (for example, a Linear
  assignment or an app-server approval request).
- **Work item:** current local state: unread, needs decision, snoozed,
  planned, in progress, done or dismissed.
- **Project link:** a user-confirmed or high-confidence association to a local
  folder/repository. Low-confidence guesses are displayed as suggestions, not
  silently used to start an agent.
- **Task run:** an adapter-owned Codex or Paseo task, with a stable provider
  id and workspace reference.
- **Outcome:** a link to the pull request, response, commit or explicit
  dismissal that resolved the work item.

The graph and its user choices persist in neko's SQLite database. Tokens and
third-party credential files do not. Each integration provides a narrow
connector that fetches normalized source events and an explicit outbound
action interface; it cannot perform arbitrary remote writes on behalf of a
planner.

## Delivery projects

This is not one feature; it is three independently shippable projects. Each
project is useful on its own and must pass its own live verification.

1. **Quick attention loop — first build.** Implement the Codex app-server
   adapter, all-local task snapshot, palette tiles, backend-neutral urgent
   approvals and `Open neko`. This fulfills the daily launcher/clipboard plus
   “are any agents blocked?” promise. The existing Codex control-plane spec
   supplies its technical design.
2. **Coding workspace.** Add the persistent app shell and focused task view:
   transcript, streaming, follow-up, start, interrupt, archive, fork, diffs,
   terminals, projects and explicit worktree flow. The palette remains quick;
   task depth moves here.
3. **Connected work inbox.** Add Linear, Slack, then GitHub in that order as
   read/triage connectors. Each begins with an explicit connected-account
   setup, read-only sync, deep links and proposal cards. Outbound actions and
   project-level automation are added one named action at a time, with a live
   verification against the connected service.

## UX and reliability requirements

- A palette search never waits on an external integration or provider process;
  it reads warmed local snapshots.
- The app shows freshness and unavailable state rather than pretending a
  silent connector has no work.
- An item cannot initiate a code task from a guessed home directory. A task
  start always displays the selected local project or a user-confirmed
  worktree target.
- Every remote action is attributable to its source work item and task run.
- No notifications or polling flood: only needs-decision, failed, completed
  or policy-triggered state changes can interrupt the person.
- Existing launcher, clipboard and Paseo flows stay functional during the
  transition.

## Tests and evidence

The command-center product adds a reducer and connector test suite around the
work graph. Fixtures cover duplicate events, stale pages, unlinked projects,
snooze/priority state, task handoff and revoked authorization. Fake connectors
prove no outbound action occurs from intake alone. GPUI tests prove palette
ordering and no accidental expansion into app depth. Each real connector gets
a one-account manual evidence pass that checks current data, an explicit
outbound action, and a revoked/expired connection.

## Documents affected when implementation begins

`README.md`, `docs/architecture.md`, `docs/adding-a-provider.md` and
`AGENTS.md` need revisions as each project lands. The existing agent-control
plan remains historical evidence and receives forward links to this parent
design and the Codex control-plane design.

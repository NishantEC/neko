# Neko runs the day: scopes, routines, inbox, notifications

Date: 2026-10-29. Owner conversation, same day.

## Why

Neko today is a strong engineering-task runner. The owner's actual day runs
elsewhere: three Paseo schedules (`daily-work-digest` 10:30, `message-watch-adaptive`
every 20 min, `weekly-pr-sweep` Monday 11:00) gather messages, PRs and issues
and then only *report*. Neko has no messages inbox, no notifications, files
everything it finds as an engineering ticket, and on 2026-10-27 had 29 tickets
waiting on the owner. It adds to the owner's queue instead of emptying it.

Owner, verbatim intent: Neko should "take care of all of these"; it must keep
"different directories for different scope and different MCPs for different
scope directories"; on autonomy: "There is no restriction. It can do whatever
it wants", and do not ask the owner questions. Reach the owner by macOS
notifications for now.

## Goals

1. **Scopes.** Neko knows the owner's scopes. A scope is a named set of
   folders. Its connections are the MCP servers configured for those folders:
   the top folder's `.mcp.json`, the per-project entries in `~/.claude.json`,
   and the repos' own `.mcp.json`. Neko shows the inventory and flags setup
   mess: the same server copied into many sibling repos, or configured twice
   in one repo.
2. **Routines.** Recurring agent runs, one scope each, that *act* and then
   report. They replace the three Paseo jobs: message watch, daily digest and
   PR sweep.
3. **Inbox.** One web page showing, per scope, what Neko did and what only
   the owner can do, newest first. Each item can be cleared. Each routine can
   be run now, paused and edited.
4. **Notifications.** Every "needs you" item becomes a macOS notification.
   Clicking it opens the inbox.
5. **Standing orders.** Per-scope plain-language rules that go into every
   routine run in that scope. The default is the owner's: act without asking,
   report afterwards.
6. **Paseo cutover.** Once a Neko routine of a kind has run successfully, the
   matching Paseo schedule is paused.

## Non-goals (for this design)

- Learning rules from the owner's approvals (suggest "auto-send these from now
  on"). It is pointless while the default is "act".
- Phone or away-from-Mac delivery. The owner chose notifications.
- Rewriting the task/ticket pipeline, the watch (responsibility) system or the
  MCP host. Routines sit beside them.
- Moving the owner's MCP config files between folders. Neko reports what to
  move. Editing `~/.claude.json`, which Claude Code rewrites continuously, is a
  separate change once the reports are trusted.

## Anti-goals

- A routine that only lists things. The Paseo jobs already do that. A routine's
  instruction says to do the work.
- New noise: a quiet run produces no inbox item and no notification.
- A cost or heat spike: routines obey the machine steward, run one at a time,
  and back off when quiet.

## How it works (decided)

**Scopes are Neko workspaces.** `Workspace` plus `workspace_folders` already
model a named set of folders. Discovery (`neko_core::scopes`) reads the disk
and proposes:

| Scope | Folders |
| --- | --- |
| Acme | `~/Documents/work` |
| Client A | `~/Documents/clientco` |
| Client B | `~/Documents/web-v2` |
| Personal | the existing "Asha" workspace (`/Users/example`) |

The rule is a top folder under `~/Documents` that has its own `.mcp.json` or
more than one repo with MCP config. The rest stays in "Asha", which keeps
its tasks, grants and watches untouched. Creating a proposed scope is one call;
in this pass the daemon creates them on first discovery, because the owner
asked not to be asked.

Connections per scope are read from disk, not imported into Neko's MCP host. A
routine runs Claude Code with its working directory in the scope folder, so
Claude Code loads that folder's MCP servers with the owner's existing sign-ins
and the claude.ai connectors (Gmail, Calendar, Drive, Slack). Nothing has to be
signed in again. This is the same mechanism the Paseo jobs use and is proven
working.

**Routines** (`neko_core::routines`, new):
- `Routine { id, workspace_id, kind, name, instruction, cadence, enabled, model }`
- Kinds: `messages`, `digest`, `pr_sweep`, `custom`.
- Cadence: either `Every { minutes, from_hour, to_hour }` with adaptive
  back-off (interval × 2^quiet_streak, capped at 4×) or `Daily { hour, minute }`.
  Times are local.
- Per-routine state (`RoutineState { last_run_ms, next_due_ms, quiet_streak,
  memory, running }`). `memory` is an opaque JSON blob the agent returns and
  gets back next run, such as last-checked timestamps and who is expected to
  reply.
- Runs (`RoutineRun { id, routine_id, started_ms, finished_ms, outcome, did[],
  needs_you[], error }`) live in their own rows, not the workbench snapshot.
  Anything on a timer gets its own row (CLAUDE.md, resource limits). The last
  50 runs per routine are kept.
- The runner is one daemon thread that wakes every 30 s. It starts at most one
  run at a time, the most overdue. It skips while `machine::holding` or a
  `machine-ok` WAIT is in force, and leaves the routine due.
- A run is `native_runner::run` with `provider: "claude"` and a routine model
  (default `sonnet`), `writable: true` (Claude Code in `bypassPermissions` with
  the owner's full setup), directory = the scope's first folder, timeout 15
  min. The prompt is the routine instruction, then the scope's standing orders,
  then the previous `memory`, then the output contract.
- Output contract: the reply ends with a fenced `json` block
  `{"quiet": bool, "did": [str], "needs_you": [{"title","detail","link"?}], "memory": {...}}`.
  A missing or unparseable block is a failed run whose text is kept for
  debugging.

**Standing orders.** A per-workspace string (`routine_rules`), default:

> You act for Asha without asking. Reply, send, post, review, rebase,
> merge, close and archive when it is the right call, and say what you did.
> Put something in needs_you only when it truly requires Asha: a decision
> only he can make, a credential or sign-in, money, or something you tried
> and could not do.

**Inbox** (web page `inbox`, in the sidebar's Main group, first item): one
card per scope with its latest needs-you items (each with a Done button) and
a "what Neko did" list from recent runs. Below that sit the routines with
their status (next run, last outcome) and Run now / Pause / Edit
(instruction, cadence, model, standing orders).

**Notifications.** The daemon keeps a monotonic counter of needs-you items.
The native app polls `RoutineNotifications { after }` every 30 s while running
and posts one `UNUserNotificationCenter` notification per new item (title =
scope · item title, body = detail). Clicking opens the Inbox page. Permission
is requested once at first use.

**Seeded routines**, created with the scopes and mirroring the Paseo prompts
but told to act:

| Scope | messages (every 20 min, 08–23) | digest (daily 10:30) | pr_sweep (daily 11:00) |
| --- | --- | --- | --- |
| Client A | slack-client-a DMs and mentions, Jira ENG, GitHub client-a-ai review comments | yes | client-a-ai |
| Acme | slack-work DMs and mentions, Linear HEA, GitHub Acme review comments | yes | Acme |
| Client B | Linear (linear-b), GitHub | yes | its org |
| Asha (personal) | Gmail, Calendar, GitHub example-owner, linear-personal | yes, plus Sibling app and machine lines from the old digest | example-owner |

**Paseo cutover.** After the first successful run of a kind, the daemon runs
`paseo schedule pause <id>` for the matching schedule (`message-watch-adaptive`,
`daily-work-digest`, `weekly-pr-sweep`) and records it on the routine. The
Sibling app schedules are not part of this design and stay in Paseo.

## Left to the builder

- Exact wire/command names, following `server.rs`/`web.rs` conventions.
- Inbox visual details within the web design system (`web/src/ui`).
- Where the native poll lives (`AppModel` or its own small type).

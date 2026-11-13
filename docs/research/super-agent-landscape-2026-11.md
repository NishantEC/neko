# Super agents that keep working: landscape and gaps (2026-11)

Scope: what the strongest personal and long-running agents do, and what Neko still lacks. Sources are official docs where found; third-party pages and recollection are marked "(unverified)". Re-check before copying any detail.

## The ten worth studying

| # | Product | Why it matters |
|---|---|---|
| 1 | OpenAI Codex (CLI, app, cloud) | `/goal`: a persisted objective with a verifiable stop condition |
| 2 | Claude Code | Stop hooks, `/loop`, background agents, cloud routines |
| 3 | OpenClaw | The reference always-on personal agent: gateway, heartbeat, chat channels |
| 4 | Manus | Cloud sandbox per task, plan file, asynchronous "come back later" |
| 5 | OpenHands | Open-source runtime, event-sourced sessions, resumable |
| 6 | Devin | Plan-then-confirm, long sessions, playbooks and knowledge |
| 7 | Cursor background agents / Jules | Remote VM per task, PR as the handoff |
| 8 | Goose (Block) | Recipes, scheduled runs, extension (MCP) discovery |
| 9 | Amp and Factory droids | Thread handoff, sub-agents, org-wide guardrails |
| 10 | Vibe Kanban / Conductor / Sculptor / Terragon | Parallel worktree agents under one board |

"Ralph loop" is a pattern, not a product: run the agent in `while true`, state lives in files and git, each iteration starts with a fresh context. Several products above are this pattern with a nicer shell.

## How each one keeps working

### Codex goals
Objective plus completion condition plus checks plus constraints; it keeps working across turns, can run many hours, and can pause and resume under a token budget ([OpenAI cookbook](https://developers.openai.com/cookbook/examples/codex/using_goals_in_codex), [use-case page](https://developers.openai.com/codex/use-cases/follow-goals)). Third-party reports (unverified): a known bug where a goal lost its continuation prompt after context compaction ([write-up](https://codex.danielvaughan.com/2026/07/06/codex-cli-goal-mode-long-running-autonomous-agents-verification-trust-architecture/)), and phone monitoring through ChatGPT. Lesson: a goal is only as good as its check, so the goal text must carry a command that proves done.

### Claude Code
`/goal` is described as a session-scoped, prompt-based Stop hook: after every turn an evaluator decides whether the condition holds ([guide](https://mer.vin/2026/07/slash-loop-explained-loop-in-claude-code-scheduling-equivalents-and-loop-vs-goal/), third-party). `/loop` is session-scoped and expires; cloud routines run on Anthropic infrastructure on schedule, API call or GitHub event ([scheduled tasks docs](https://code.claude.com/docs/en/scheduled-tasks)). `claude --bg` and `/bg` detach a session (unverified). Hooks give deterministic gates. Lesson: Stop-hook verification is the loop engine; everything else is scheduling.

### OpenClaw
One long-lived gateway process fans in many chat channels (WhatsApp, Telegram, Slack, Signal, iMessage). A heartbeat wakes the agent on a timer to read `HEARTBEAT.md`, a checklist of things to look at. Memory is plain Markdown (`MEMORY.md`) plus a local hybrid vector and keyword index. Skills are Markdown folders, installable from a registry ([repo](https://github.com/openclaw/openclaw), [overview](https://agentic-ai.readthedocs.io/en/latest/AgentPlatforms/openclaw/), secondary sources). Lesson: the human-readable checklist and memory files are the product; the user edits them. Warning: its broad access is the reason security guides exist.

### Manus
A cloud computer per task, a visible todo/plan file the agent rewrites as it goes, and notification when finished, so the user leaves. Context engineering notes (file system as memory, keep failures in context) are in the [Manus blog](https://manus.im/blog) (unverified). Lesson: the plan file is both memory across compaction and the progress UI.

### OpenHands
Open-source; every action and observation is an event in an append-only log, so a session replays and resumes after a crash; condenser strategies compact old events; a headless mode runs in CI ([repo](https://github.com/OpenHands/OpenHands), unverified detail). Lesson: event-sourcing makes crash-resume nearly free.

### Devin
Proposes a plan the user can edit before it starts, runs in its own VM for long sessions, asks questions in Slack or the web app when blocked, and reuses saved "knowledge" and playbooks ([docs](https://docs.devin.ai), unverified detail). Lesson: escalate with a specific question plus a default, and keep going on other work meanwhile.

### Cursor background agents and Jules
Remote VM per task, branch and PR as the unit of handoff, status in web and mobile, follow-ups by comment ([Cursor](https://cursor.com/docs), [Jules](https://jules.google), unverified detail). Lesson: the PR is the checkpoint everyone already understands.

### Goose
Recipes (a prompt, parameters, extensions as a file) that can be scheduled; MCP extensions enabled per recipe ([repo](https://github.com/block/goose), unverified detail). Lesson: shareable, parameterised recipes beat free-form prompts for repeat work.

### Amp and Factory
Amp: threads that can be handed off into a fresh thread with a summary, plus sub-agents ([manual](https://ampcode.com/manual)). Factory: droids with org-level policy and many integrations ([docs](https://docs.factory.ai)). Both unverified in detail. Lesson: handoff is a first-class command, not a failure mode.

### Parallel orchestrators
Vibe Kanban, Conductor, Sculptor and Terragon each give one worktree or container per agent and a board or list for status. Terragon and Sculptor stress sandboxes; Vibe Kanban stresses review-and-merge from the board (all unverified detail). Lesson: the human's job becomes review, so the review surface decides throughput.

## Mechanisms, side by side

| Mechanism | Strongest example | Pattern |
|---|---|---|
| Goal/plan file | Codex goals, Manus | Objective, check command and constraints in one persisted record the agent re-reads each turn |
| Loop until done | Claude Stop hook, Ralph | An independent check (tests, evaluator) decides stop, not the agent's own claim |
| Checkpoint/resume | OpenHands, Codex pause/resume | Append-only events; state in files and git, not in the process |
| Handoff | Amp, Devin | Summary-plus-next-steps document that seeds a new session |
| Compaction | Codex, OpenHands condenser | Compact, then re-inject the goal and constraints, since compaction can drop them |
| Budget/time | Codex token budget | Hard ceiling per goal, paused (not killed) on reach |
| Escalation | Devin, Cursor, OpenClaw | A push to where the person already is; one specific question; keep working elsewhere |
| Parallel | Vibe Kanban, Conductor | Worktree per agent, cap per repository |
| Control without babysitting | Codex, Claude hooks | Pre-approved scopes, deterministic gates, pause/resume, a morning summary |

## Super-agent mechanisms

- **One entry point:** OpenClaw makes every chat app the entry. Neko's chat plus Cmd-K is the equivalent on the desk; phone is the weak side.
- **Tool and skill discovery:** OpenClaw registry, Goose extensions, Claude Code skills. The agent searches a catalogue when it lacks a tool rather than the user pre-wiring everything.
- **Memory of the person:** plain Markdown the user can read and edit; the agent proposes additions, the user prunes.
- **Proactive suggestions:** a heartbeat reading a checklist, then reporting only when something changed (silence is the default).
- **Connectors:** MCP everywhere, scoped per project (Goose recipes, Neko scopes).
- **Mobile:** chat channel as the remote (OpenClaw), native app with live goal view (Codex, Cursor, Jules).

## What Neko already has (do not rebuild)

Checked in `crates/` and `web/src/`, and in `docs/knowledge/neko-runs-the-day.md` and `neko-is-the-chief-of-staff.md`:

- Tickets in worktrees, per-repository writable cap, machine steward (admission, night cap, comfort wait), orphan-process cleanup, wall-clock run timeouts.
- Scoped routines with per-scope MCP config, the Inbox, needs-you items, failure escalation, Codex fallback when the Claude login fails, per-routine memory handed back each run.
- Chat that can propose actions on tasks, with a rulebook, a record of all tasks, a chat checkpoint (`crates/neko-daemon/src/workbench/checkpoints.rs`), and duplicate detection.
- Phone push (Reminders, ntfy) with idle and quiet-hours rules (`crates/neko-core/src/phone.rs`).
- Prior-work lookup before a first run, review repair, self-update, calendar, canvas and plugin viewers.

## Gaps, ranked

1. **Goal records with a machine-checkable done condition.** Grep finds `goal` only in prompt text, not as a stored object. A ticket or routine gets `{objective, check_command, constraints, budget}`; the runner re-runs the check after each agent turn and continues, pauses or escalates.
2. **Verify-and-loop engine.** Today a run ends when the agent says so. Add an iteration loop: run, check, feed failure output back, bounded by iteration count, wall time and cost. This is the Stop-hook and Ralph pattern.
3. **Crash and reboot resume for runs.** Per-run progress file in the worktree (plan, done, next) plus a daemon-start sweep that restarts interrupted runs from it. Today only the chat has a checkpoint.
4. **Handoff and compaction that re-injects the goal.** A run that nears its context limit writes a handoff note and starts a fresh session seeded with goal, plan and note. Test explicitly that constraints survive.
5. **Cost and time budgets as pause, not kill.** Per-goal ceilings and a daily ceiling; on reach the run parks with its state, an Inbox item asks to extend.
6. **Two-way phone and a morning digest.** Phone push exists; add reply actions (approve, extend budget, answer a question) and one scheduled summary: what finished, what is stuck, what needs a decision. Escalations must carry one question and a default.
7. **Heartbeat checklist and editable memory file.** A user-readable standing-orders file per scope that routines read, plus a visible memory page where the person prunes what Neko remembers. Routines already return `memory`; surface and edit it.
8. **Skill and tool discovery.** When a run lacks a tool, search a catalogue (installed MCP servers, skills, a curated registry) and propose installing it, rather than failing. Pairs with recipes: a saved, parameterised routine others can import.

## Risks to design against

- Self-verification: an agent that grades itself ends early. Keep the check independent.
- Runaway spend: every loop needs a hard stop first, before the loop is built.
- Silent failure: a long run that stalls should reach the person within one heartbeat, not at the deadline.
- Breadth of access: always-on agents with wide connectors are the main security story around OpenClaw; keep Neko's per-scope tool leases.

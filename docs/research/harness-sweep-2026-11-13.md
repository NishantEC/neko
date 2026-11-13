# Harness sweep, 2026-11-13: what changed and what Neko is missing

Sources read 2026-11-13, shallow clones of [Hermes Agent](https://github.com/NousResearch/hermes-agent) (`fbfeb47b`, MIT), [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) (`39163dd3`, MIT) and the release feeds of [herdr](https://github.com/herdrdev/herdr), [herdr-app](https://github.com/herdrdev/herdr-app) and [herdr-core](https://github.com/herdrdev/herdr-core) (Apache-2.0). Nothing was run. Earlier notes: [hermes-agent.md](hermes-agent.md), [deepseek-harness.md](deepseek-harness.md), [herdr.md](herdr.md). Quotes are short and credited to their repositories.

## 1. What changed since 2026-11-05

**Hermes** shipped [v0.23.0 "Hearth"](https://github.com/NousResearch/hermes-agent/releases/tag/v0.23.0) (11-05) and [v0.23.1](https://github.com/NousResearch/hermes-agent/releases/tag/v0.23.1) (11-10), plus daily canaries.
- One gateway owns every session; CLI, TUI, desktop, editors, bots and cron are all clients of it. This fixed messages written twice or lost between surfaces.
- Price tags and spend limits on paid MCP tools: the approval prompt shows each call's price and the chat's running total, and asks before a chat passes its limit, even in YOLO mode.
- Cron: stop a running job, and per job say what happens to runs missed while the machine was off (`catch_up`).
- Cost work for always-on use: every fire of one cron job shares a prompt-cache key, one-shot calls send 1,570 fewer prompt tokens, goal and heartbeat turns no longer rewrite the cached prefix, and opt-in `compression.prepare_ahead` writes the next summary in the background.
- Compaction summaries mark guesses on `UNVERIFIED:` lines so a guess does not come back as a fact.
- `hermes curator lock` stops the agent rewriting a skill you hand-curated, on every write path. Built-in skills went from 58 to 44; plugins grew to 1,050+ and updates that would break a setup are held back.
- Pluggable delegation (a named model or separate profile per subagent) and Group Chats with a lead or router bot.
- Browser tools collapsed to one `browser_exec`; computer use is a swappable provider plugin (`plugins/computer_use`).

**DeepSeek Harness** shipped [0.2.2 rc.1 to rc.3](https://github.com/deepseek-ai/deepseek-harness/releases) (11-04, 11-11, 11-13). Mostly UI, but two things matter to Neko: an experimental "change requests and worktrees" view (per-session git worktree, file-by-file diff, PR checks and review state, selected code and comments handed back to the agent), and named, multi-level subagents with a per-session AgentTeam and a shared task panel. Plugin updates now roll back if hot-reload fails. `dsh-superagent-prototype` has had no push since 11-03.

**herdr** itself has been quiet (last push 11-07, v0.9.5 on 10-14). The work moved to the app and core: cores that survive a crash and re-adopt terminals (`announce a status that crash or relink adoption changed`), a full disk no longer stops the core saving terminals, ssh auth backoff, and `hc wait` that reports exit when a program that said "working" dies. These are small, but each one is a recovery rule.

## 2. What the earlier notes missed

### Long-running and autonomous operation

**Goal loop with a judge (Hermes).** [`hermes_cli/goals.py`](https://github.com/NousResearch/hermes-agent/blob/main/hermes_cli/goals.py): a goal persists across turns; after each turn a cheap auxiliary model rules DONE, BLOCKED, CONTINUE or WAIT. It is fail-open, and a 20-turn budget is the backstop. The judge prompt: "DONE requires the deliverable to actually exist." With a draftable completion contract the rule becomes "not a claim like 'done' or 'all tests pass' without evidence". WAIT is the cost saver: if the agent is gated on a running process, a rate limit or running subagents, the loop parks for a pid, a session watch pattern or N seconds instead of burning a turn. Three consecutive unparseable verdicts auto-pause the loop.

**Goal in DeepSeek.** [`packages/goal`](https://github.com/deepseek-ai/deepseek-harness/tree/main/packages/goal): the goal records state only; continuation is a separate plugin. Each round injects `<goal_round>` with "Treat the current workspace, tool results, and durable session state as authoritative; inspect them instead of assuming earlier narration is still current... gather evidence that the whole objective is achieved, read the current goal, and mark it complete." Round counts are capped (`maxGoalRounds`) and a stale prompt is rejected if the goal revision changed (race fences).

**Heartbeat and loop.** [`hermes_cli/heartbeat.py`](https://github.com/NousResearch/hermes-agent/blob/main/hermes_cli/heartbeat.py): 60 s floor, fires only into an idle session with an empty queue ("a real user message always wins"), and the prompt ends "reply briefly that nothing has changed and stop — do not invent work". [`loops.py`](https://github.com/NousResearch/hermes-agent/blob/main/hermes_cli/loops.py) stops on a `LOOP_COMPLETE` line, a tick count, an `--until` judge or a max-tick backstop.

**Recovery and budget.**
- [`cron/missed_runs.py`](https://github.com/NousResearch/hermes-agent/blob/main/cron/missed_runs.py): one catch-up run, and every skipped slot booked as a single `missed xN` ledger row so history shows the gap.
- [`cron/quota_hold.py`](https://github.com/NousResearch/hermes-agent/blob/main/cron/quota_hold.py): when a provider says "retry after N seconds", the job's `next_run_at` is parked at the window boundary instead of probing and alerting every fire.
- [`agent/kanban_turn_recovery.py`](https://github.com/NousResearch/hermes-agent/blob/main/agent/kanban_turn_recovery.py): a worker whose provider call dies retries in place (same session, bounded budget, backoff) only if the failure is typed `retryable`, never after an interrupt, and only after proving it still owns a live claim lease. Quota walls go to a cooldown breaker.
- Kanban config reclaims running cards with no heartbeat for 4 h and requeues orphans with broken claim bookkeeping each tick.
- DeepSeek schedule: RFC 5545 recurrence; after a restart a recurring task sends only its latest missed occurrence. Guards: `repeat-tool-reminder` (same call twice, change approach) and a per-tool cooperative timeout.

### Memory and skills that improve

[`agent/background_review.py`](https://github.com/NousResearch/hermes-agent/blob/main/agent/background_review.py) forks the agent after a turn, on the same cached prefix, with a tool whitelist, and asks what to save. The skill prompt: "Be ACTIVE... A pass that does nothing is a missed learning opportunity." Signals: user corrections of tone or workflow, a non-trivial fix, a loaded skill found wrong. Preference order: patch the loaded skill, then an umbrella skill, then a `references/`, `templates/` or `scripts/` support file, and only then a new skill. [`agent/curator.py`](https://github.com/NousResearch/hermes-agent/blob/main/agent/curator.py) runs on inactivity (no cron), "never delete, only archive", skips pinned skills, and [`tools/skill_lock.py`](https://github.com/NousResearch/hermes-agent/blob/main/tools/skill_lock.py) lets the owner freeze one.

### Multi-agent orchestration

Hermes: kanban with dispatcher, `auto_decompose` of triage cards (3 per tick), a swarm module and a PR-acceptance store. DeepSeek: a [`workflow`](https://github.com/deepseek-ai/deepseek-harness/tree/main/packages/workflow) tool where the model writes a JavaScript script using `agent()`, `parallel()`, `pipeline()` and `phase()` to fan out subagents, plus a shared team task board with ownership and dependencies.

### Self-extension

DeepSeek's Creator preset adds `plugin_manager` and a plugin-writing skill folder, and updates roll back on failed hot-reload. Hermes has a plugin catalog with a Verified tier and held-back breaking updates. Plugins from outside the core are how both keep the core small.

### Computer and browser use

Hermes uses one `browser_exec` tool, a real-profile login vault (key in the OS keychain), URL policy, and a swappable `computer_use` provider (default `cua`). DeepSeek has `packages/browser-use` and `computer-use`. Neko only lists these as MCP scopes in `native_runner.rs`.

### Verifying their own work

[`agent/verification_stop.py`](https://github.com/NousResearch/hermes-agent/blob/main/agent/verification_stop.py) is a turn-end gate. If code was edited and the evidence ledger holds no passing run for those paths, the stop is replaced by a synthetic follow-up: "You edited code in this turn, but the workspace does not have fresh passing verification evidence yet... If verification is not possible, explain the concrete blocker instead of claiming the work is fully verified." It is capped at 2 attempts, ignores prose-only edits, and rejects a run whose exit status it cannot attribute to the check. `hermes verify` runs detect, build, test, boot and a readiness poll, and records evidence.

### Staying cheap

Shared cache keys across cron fires, no system-prompt mutation for goals and heartbeats, a cheap auxiliary model for judge, review and curator, WAIT instead of busy re-poking, DeepSeek's [`spill`](https://github.com/deepseek-ai/deepseek-harness/tree/main/packages/spill) (big tool output kept on disk with head, tail and a path), and per-call price tags.

## 3. Neko today (grep, not full audit)

Already present: chat checkpoints, ticket lessons and memory learning, independent review and repair, responsibilities with a due time committed before launch (restart does not replay), duplicate and prior-work checks, descendant cleanup, writable caps, the machine steward. Not found in `crates` or `web/src`: a user-level goal loop with a judge, a missed-run ledger, a provider quota hold, an evidence-gated stop, a skill curator or lock, spend limits on tool calls, prompt-cache keys, and a plugin that rolls back on failure.

## 4. Gaps ranked

1. **Goal loop with a cheap judge and WAIT.** `/goal` on a ticket or chat.
2. **Evidence-gated stop.** Do not let a ticket reach ReadyForReview without recorded passing checks.
3. **Quota hold and typed in-place retry.** Park work until the provider's reset instead of failing 12 of 18 tickets.
4. **Missed-run ledger** with one catch-up per responsibility.
5. **Curator and lock for skills and memory.** Archive, never delete.
6. **Spend limits per chat, ticket and paid tool,** shown at approval time.

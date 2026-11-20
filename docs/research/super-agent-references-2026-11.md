# Super-agent references: Capy, T3 Code, DeepSeek Harness (2026-11-20)

Question: what should Neko copy, avoid and do differently, given the goal "a super agent anyone can use that keeps on working".
All facts were read on 2026-11-20 from official docs and the GitHub API unless marked "(earlier note)" or "unverified". Descriptions are paraphrased. The Neko column comes from `docs/knowledge/` (goals-that-keep-working, neko-maintains-itself, self-improvement, phone-push, packs, luau-plugins, the-chat-directs-the-crew, checkpoints-and-undo, morning-digest). Earlier studies: `t3code-and-zeron.md` (2026-11-13), `deepseek-harness.md` (2026-11-08), `super-agent-landscape-2026-11.md`.

## Which projects were studied

| Name said | Studied | Why | Alternatives (one line) |
| --- | --- | --- | --- |
| "Cappy" | **Capy**, capy.ai, an agent platform for engineering teams, made by Scrapybara, Inc. (San Francisco, YC-backed per its About page) | It is the only "Capy" that is an AI coding agent with a desktop app; it matches "agents that keep working". Confidence: high on identity, medium on facts (closed source, docs only). | `capy-ui/capy` (a native-UI toolkit, MPL-2.0, ~2.7k stars) and Ruby's Capybara test tool: not agents. |
| "T3 code" | **T3 Code**, `pingdotgg/t3code`, MIT | Exact name match, repo and homepage t3.codes checked. Confidence: high. | None seen (one third-party guide names a different org; the `pingdotgg` repo is the one the README and releases live in). |
| "DeepSeek" | **DeepSeek Harness** (`dsh`, `deepseek-ai/deepseek-harness`, MIT) plus its **superagent prototype** (`deepseek-ai/dsh-superagent-prototype`, MIT). "DeepSeek" alone is a model family; this is DeepSeek's own agent side. Confidence: high that these are the official ones. | Both are published by the DeepSeek organisation on GitHub. | `Hmbown/DeepSeek-TUI` (community, MIT, ~41k stars on 2026-11-20, a terminal agent for the models; not an official DeepSeek product). |

## 1. What each one is

### Capy (closed source, cloud-first)
- **Purpose.** a platform for running many agents: threads start from the web or desktop app (macOS, Windows, Linux), Slack, Linear or an API; each thread runs on an isolated cloud VM (or on your own computer) and ends in a pull request, CI fixes and a review agent. Docs fetched 2026-11-20.
- **Licence and activity.** The docs and About page do not state a licence; the GitHub account `capy-ai` has 0 public repositories (2026-11-20), so treat it as proprietary. SOC 2 Type II per its FAQ. Plans from free to $200 a month plus credits.
- **Best at.** **Captains**: long-lived agents that run a crew of threads, keep their plan and rules in plain files, and tell you only what needs you. Also very good wake-up and steering rules (below).

### T3 Code (MIT, local-first control plane)
- **Purpose.** An open-source control plane for coding agents: one server on your machine drives Claude Code, Codex, Cursor, OpenCode and many others (plus any ACP agent), with desktop, web, iOS and Android clients. It does not resell tokens.
- **Licence and activity.** MIT. Created 2026-02-08; ~66.7k stars and ~11.6k forks on 2026-11-20; last stable release v0.1.10 on 2026-11-17 with nightly builds almost daily (v0.1.11-nightly on 2026-11-20).
- **Best at.** One fast interface over every agent, with the phone as the approval and notification surface (push, Live Activities) and a server that keeps working when no client is open (earlier note).

### DeepSeek Harness and the superagent prototype (MIT, developer preview)
- **Purpose.** `dsh` is an open-source agent harness where everything is a plugin (model adapter, tools, even the agent loop), run as a web UI (`npx @deepseek-ai/dsh web`) or Electron desktop app. The superagent prototype is a proactive coding agent for a single GitHub repository that listens to GitHub (and Feishu), works in its own sessions and answers in the original conversation.
- **Licence and activity.** Both MIT. Harness created 2026-08-13, ~284k stars, release candidates `dsh-v0.2.2-rc.5` on 2026-11-18; its README warns of breaking changes and SAFETY.md says it has had no security audit. Superagent prototype created 2026-10-31, ~24k stars, labelled by DeepSeek as a research prototype, not a product.
- **Best at.** The superagent's discipline about human attention: it acts only when asked or when a written rule allows it, starts proactive work in a mode that only records what it would have done, and speaks only when a person must act.

## 2. Feature matrix

| Capability | Neko (today) | Capy | T3 Code | DeepSeek Harness / superagent |
| --- | --- | --- | --- | --- |
| Persistent goals, long-running work | `/goal` with objective, shell done-check, budget; judge plus **evidence-gated DONE**; run journal; resume after crash; stall watchdog; quota hold | Captain (long-lived, plan in `NOTES.md`, resumes where it stopped); no done-check or evidence rule documented | No goal object found in the docs read; server-owned thread settlement and PR sync (earlier note) | `goal` package: one durable goal per session, optional round driver continues while idle, round allowance, exhaustion records a blocker; no evidence check documented |
| Scheduling and routines | Routines with cadence, watches, calendar, one catch-up run after downtime, morning digest | Automations: cron (min 5 min), GitHub, Slack, webhook, on-demand; agent can set timers up to 7 days and watches up to 30 days; missed runs coalesce | Orchestrator MCP `schedule_task` for recurring work with no active turn | `schedule` package: RFC 5545 recurrence delivered as a follow-up message to a session; no push |
| Multi-agent crews | Chat orchestration tools (`create_task`, `steer_task`, `wait_for`, delegation cap), tickets in worktrees, writable cap per repo | Captains plus crew threads, subagents, forks, side chats; "up to 25 parallel" is vendor marketing (unverified) | Orchestrator MCP: spawn sub-agents on any provider, wait, steer, cancel; subagent approvals show in parent thread | `subagent` (spawn/fork), `team` task board, `workflow` fan-out; superagent: router plus workers with mandates |
| Approvals and safety | Needs-you Inbox with defaults, plan approval, pack digest approval, machine steward, owner-chosen "acts without asking" | No plan mode; agent never merges unless asked in the current thread; three rules prose cannot override; per-device first-use prompt; secrets hidden from the model but readable by running code | Four modes (Supervised, Auto-accept edits, Auto, Full access), default **Full access**; admin policy can cap modes; approve from phone | Sandbox mode and approval policy as two knobs, answerers fail closed; optional LLM auto-review; SAFETY.md says do not rely on it as the only control |
| Memory and skills | Memory with provenance rule, skills from lessons, `PATCH` proposals, curator that archives, never deletes | Drive (instructions, `AGENTS.md`, skills) shared across threads; no memory feature documented; agents can write repo skills on request | No memory of its own seen; defers to each provider's skills and instructions | No built-in long-term store; `AGENTS.md`, skills, `session-query`; memory only through an optional MCP server |
| Plugins and extensions | Packs (tools, skills, UI, routines, one approved digest), web plugins, sandboxed Luau cards and views | Plugins bundle MCP servers and skills, marketplace or GitHub; MCP servers | Any ACP agent; MCP apps render in-thread; no plugin system seen (the README says to fork it) | Everything is a plugin (Cordis), plugin market; installing a plugin loads code with host permissions |
| UI model | Web, Rune desktop, Swift menu bar app, phone reminders | Web and desktop, Slack, Linear, API; no native mobile app (phone browser only) | Electron desktop, web, iOS and Android apps | Web UI and Electron desktop; superagent adds a web panel showing only what needs a person |
| Cost and usage | Quota in menu bar, spend ceiling, goal budgets, quota hold and provider switch | Per-thread cost split (own-key usage shown as not charged), usage API, credits and machine minutes | Usage page from each provider's local history, labelled as estimated API-equivalent cost, kept 92 days by default | unknown (not checked) |
| Phone or notification reach | Reminders and optional ntfy, quiet hours, 6 an hour, answer by completing the reminder | Web push only when the site is added to an iPhone home screen; Slack and Linear are the other reach | Native push, iOS Live Activity, Android live updates, approve from the phone (needs the T3 Connect account) | None documented; schedules deliver into sessions, not notifications |
| Self-improvement | Improvements Inbox (repeated trouble becomes one suggestion, applied on click, verified 7 days, Undo), skill curator | Not documented | Triage playbook for T3's own bugs only | Creator preset builds plugins; superagent prompt overrides keep a "default updated" diff |
| Open source and licence | Public, MIT | Closed (no licence stated) | MIT | MIT (both repos) |

## 3. Copy (max 5)

Licence rule used throughout: only MIT/Apache/BSD/Zlib code may be copied, MPL only unmodified, never GPL/LGPL/AGPL; ideas are free. Capy is closed source, so **ideas only**. T3 Code and DeepSeek are MIT, so code could be copied with the notice kept, but each item below is a design Neko should write itself in Rust.

### 3.1 Attention as the budget, and "record first" for anything proactive (DeepSeek superagent)
- **Idea.** Every message, question and setting costs someone's attention and must earn it. Work nobody asked for needs a written rule and starts in a mode that only records what the agent would have done; the operator widens it once the record looks right. Limits such as reply length are enforced in code, not in the prompt.
- **Evidence.** Superagent `docs/000-philosophy.md` ("take as little human attention as possible") and `docs/002-authorization.md` (read 2026-11-20); prompts-vs-code split in `deepseek-harness.md` section 3.
- **Neko would.** Add a per-routine and per-watch "Rehearse" state: it runs, writes what it would have filed or sent into a ledger row, and touches nothing; the morning digest shows "would have done N things, open to switch on". Count pushes and questions per day as an attention number in Settings, Phone.
- **Licence.** MIT, idea only.

### 3.2 Per-crew rules and plan files the person can edit (Capy Captains)
- **Idea.** A Captain keeps three plain files: its own rules, rules every crew thread reads on every turn, and a plan that lets it resume where it stopped; plus a digest of what needs you, what is new and what is in progress. Constraints such as "ask before starting more than two threads" are just sentences.
- **Evidence.** `docs.capy.ai/captains.md` and `/async-work.md`, 2026-11-20.
- **Neko would.** Let a goal or a workspace own a `CREW.md`-style rules text that is added to every ticket run it spawns (Neko already adds the journal and ledger to the builder prompt via `goals::prompt_context`), shown and editable in the goal's detail, with the same digest-and-diff treatment as the rulebook. Keep hard limits (delegation cap, writable cap) in code.
- **Licence.** Closed source, idea only.

### 3.3 Neko as an MCP server for outside agents, with scoped levels (T3 Code)
- **Idea.** Agents T3 did not start (a terminal Claude Code or Codex, a chat assistant) can read projects and threads, start and message threads, after a one-time sign-in where the person picks what the agent may do; a code works once, for 5 minutes, and approves only that level. An admin policy can cap the broadest mode.
- **Evidence.** `docs/user/outside-agents.md`, `docs/user/managed-policy.md`, `docs/orchestration-v2/orchestrator-mcp-server.md` in `pingdotgg/t3code`, 2026-11-20.
- **Neko would.** Expose today's crew tools (`list_tasks`, `create_task`, `task_status`, `wait_for`) on a loopback-only endpoint with levels read / file tickets / steer, a single-use pairing code and an Inbox entry for each paired agent. Neko's bridge is per-run lease today; I did not find an outside-agent endpoint in `docs/knowledge/` (unverified that none exists).
- **Licence.** MIT; design only.

### 3.4 Guard rails on automations: caps with visible skips, a cheap "run only when" check, one standing thread or a fresh one (Capy)
- **Idea.** `max_runs_per_day` records events over the cap as visible `skipped` runs; a `run_when` sentence is judged by a fast model before a run starts; missed scheduled occurrences coalesce into one run; each automation chooses a fresh thread per run or one standing thread that keeps its context; a run whose owner lost access fails visibly and disables the automation with a reason.
- **Evidence.** `docs.capy.ai/automations.md`, 2026-11-20.
- **Neko would.** Add the same three fields to routines and watches (cap, `run_when` on the goal judge's cheap model, standing vs fresh), and show skipped runs in run history. Neko already has the catch-up run and a daily spend ceiling, so this is small.
- **Licence.** Closed source, idea only.

### 3.5 Honest usage history that outlives the provider's own (T3 Code)
- **Idea.** A Usage page reads every provider's local history, labels dollar figures as estimated API-equivalent cost (not the subscription bill), separates what cannot be split, and keeps its own copy for 92 days by default because one provider deletes its transcripts after 30.
- **Evidence.** `docs/user/usage.md` in `pingdotgg/t3code`, 2026-11-20. Capy shows the same honesty in a thread's cost split (subscription usage listed as not charged), `docs.capy.ai/models-and-pricing.md`.
- **Neko would.** Store a small per-day usage row per provider (own row, not the snapshot) so Usage and the morning digest can show a week even after a provider trims history; label estimates in the UI and in the digest.
- **Licence.** MIT; design only.

## 4. Avoid (max 5)

1. **Defaulting to full permissions for other people (T3 Code).** Its docs say the initial permission default is Full access. That suits the owner, who chose "no restriction", but a public super agent that strangers install needs a narrow first-run default (read-only plus ask), with the owner's own profile set wider. Reason: the first bad surprise decides whether a new user stays.
2. **Cloud-first design and per-minute machines (Capy).** Thread messages and tool output live in Capy's cloud even when the thread runs on your own computer; cloud machines bill by the minute and sleeping machines are deleted after 14 days with unpushed work; secrets are hidden from the model but any command it runs can print them. Neko's local-first, own-data position is the opposite and is a selling point; do not add a hosted sandbox.
3. **Safety by prose alone (Capy Captains).** The docs describe no built-in approval gate for a Captain: limits come from rules the person writes. Capy itself hard-codes three rules (no merge unasked, no leaked secrets, no fabricated results) because prose is not enough. Keep Neko's limits in code and use prose only to explain them, as the DeepSeek superagent does.
4. **A plugin graph that loads code with host power, and breaking changes as a policy (DeepSeek Harness).** Making everything a plugin, with hot reload, gives a huge surface (its SAFETY.md says it is unaudited developer-preview software, and its README warns of compatibility-breaking changes). Neko's pack digest, health check and Luau sandbox are the safer shape; keep them.
5. **Chasing provider count and release churn (T3 Code).** It lists 11 or more providers and ships nightlies almost daily; each adapter is permanent maintenance and Neko's differentiator is not "one more agent in the list". Support the few the owner uses, route others through ACP if wanted, and stay with a calm release rhythm.

## 5. Extraordinary, not copied

None of the three does these; each rests on parts Neko already has.

### 5.1 A receipt for every finished goal
- **Idea.** When a goal reaches DONE, Neko assembles one shareable page: the objective, each check with its exit code and output tail, the run-start and run-end checkpoint diff, the journal, cost and wall time, and the PR if any, and sends it as the single "finished" push. "Keeps working" becomes "keeps working and shows its proof". Capy tells its agent not to fabricate results and DeepSeek's superagent prompts say an idle ticket is not a completed one, but neither stores a check-backed ledger the person can open.
- **Built on.** Evidence ledger and evidence-gated finish (goals), hidden-ref checkpoints and per-run diff, artifacts, phone reach.
- **Smallest first step (about half a day).** On DONE, write the ledger, journal and `checkpoint_diff` summary into one markdown artifact (`save_artifact`) and link it from the existing morning-digest "Finished" line. No new storage.

### 5.2 A deadline on every question: the default runs, and Undo is real
- **Idea.** Every blocked goal already has exactly one question with a non-empty default and can be answered from the phone. Add an optional per-goal "if I do not answer in N hours, take the default", allowed only while the work in between is checkpointed, and pair it with a single Undo that returns the folder to the pre-default checkpoint. The agent never waits on a sleeping person, and nothing is irreversible. The three studied projects all stop and wait (or, in Capy's case, rely on prose rules).
- **Built on.** BLOCKED with question and default, answer-by-reminder, checkpoints and `undo_run`, quiet hours, goal budgets.
- **Smallest first step (about 2 hours).** A `default_after_hours` field on `Goal`, default off; the goal tick answers with the default when the Inbox item is older than that and the goal's ticket has a start checkpoint, and adds one journal line "took the default after Nh".

### 5.3 Goal packs: a goal you install, with its check and schedule
- **Idea.** A pack can already carry tools, skills, UI and routines under one approved digest. Let it also carry goal templates (objective, `done_check`, constraints, budget, optional cadence), for example "keep this repository's dependency updates green". Installing it creates a standing goal that the person approved by digest and that proves itself with its own check. Capy automations are prompts, T3 has threads, DeepSeek goals have no shareable form; none ships a verified, installable "keep this true" unit.
- **Built on.** Packs and digests, goals with `done_check`, routines cadence, the Improvements Inbox (a suggestion can propose a goal pack).
- **Smallest first step (about 3 hours).** Allow `[[goals]]` in `pack.toml` (objective, check command, turns) shown in Preview and included in the digest; Install creates the goal paused, and the person starts it with one click. Test with a bundled example in `packs/`.

## 6. Open questions for the owner

1. **First-run permissions for other people.** Should a fresh install default to narrow (read-only, ask before writing) with the owner's profile set to full power, or keep one default for everyone? (Choice: narrow-by-default / one default.)
2. **Outside agents.** Should Neko expose its crew tools to agents Neko did not start (terminal Claude Code, Codex, chat assistants) over a loopback MCP endpoint with levels and one-time pairing? (Yes / no.)
3. **Silence means the default.** Is it acceptable for a goal's question to take its default after a deadline you set, when Undo is available? (Yes, per goal and off by default / no, always wait.)

## Not verified

- Capy: licence (none stated; closed assumed from 0 public repositories), founding date, star or user counts, the "25 concurrent agents" claim (a vendor-authored search snippet), and anything not in the docs pages fetched on 2026-11-20. Model names and prices in its docs were not checked against any other source.
- T3 Code: scheduling, orchestrator MCP and settlement facts for rows marked "(earlier note)" were last read on 2026-11-13, except the scheduled-task tools, which were re-checked today. Memory and goals are "not found in the docs read", not proof of absence.
- DeepSeek Harness: cost and usage visibility and phone or desktop notifications were not checked; the star counts are from the GitHub API on 2026-11-20 and look very large, so treat them as reported, not as a quality signal.

# Permissions, asked in the task (S1, 2026-11-26)

Neko never skips permissions. Every step Claude Code would ask about comes to Neko as a `can_use_tool` request
(`--permission-prompt-tool stdio`). Neko's policy (`crates/neko-core/src/policy.rs`) decides, or asks the person in the task.

## Why there are three modes

The first real run showed that `--permission-mode default` was not enough. Claude Code asks only about a step its own rules do not
already allow. The person's own settings allowed `Bash`, `Edit` and `Write` outright and set the default mode to `auto`, so the CLI
never asked anyone, and Neko's screen said "Ask before risky steps". Now each mode does what its name says.

## The three modes (Settings)

- **Follow Claude Code's own rules** (the default, and what a data folder with no stored mode reads as). Neko adds nothing: no
  `--permission-mode`, no settings. What the person's Claude Code settings allow runs without asking, as in Terminal. Anything
  Claude Code would ask about arrives as `can_use_tool` and is asked in the task. Settings shows one line read from the CLI's
  settings files (managed, the newest task's folder's `.claude/settings.local.json` and `.claude/settings.json`, then
  `~/.claude/settings.json`), for example "Your Claude Code settings allow Bash, Edit and Write without asking." Neko only reads
  them (`claude/rules.rs`).
- **Ask before risky steps**. Enforced by Neko whatever the person's settings say. Go ahead: reading inside the task's folder, and
  editing there when the folder is the task's own (a scratch folder or a git worktree). Ask: every shell command, reading or
  writing outside the folder, the network, a tool Neko does not know, and the first edit in a chosen folder that is not a
  repository.
- **Full access**. Everything goes ahead. Settings says so plainly. Neko still uses the same route; it just answers yes.

Claude Code's own bookkeeping tools (its plan, a subagent, a question, `ToolSearch`) are left to Claude Code in every mode.

Paths are resolved against the folder, with `..` and links followed, so a link that points out of the folder is outside.

## How "Ask before risky steps" is enforced: the hook

For a run in this mode only, Neko passes `--permission-mode default` and this `--settings` JSON (never written to a file):

```json
{"hooks":{"PreToolUse":[{"matcher":"*","hooks":[{"type":"command","command":"<neko-daemon>","args":["--hook"],"timeout":86400,"onFailure":"block"}]}]}}
```

- Claude Code runs `neko-daemon --hook` before every tool call, in exec form (no shell). Hooks from `--settings` are added to the
  person's own hooks, which still run.
- The child gets `NEKO_HOOK_PORT` and `NEKO_HOOK_SECRET` in its environment: the daemon's loopback port and a secret made for that
  run (32 random bytes). Hooks inherit Claude Code's environment.
- The helper reads the hook's JSON on stdin and posts it to `POST /v1/hook` with the run's secret as its bearer. A wrong or old
  secret is refused (the install token is not accepted either), and the helper prints a deny.
- The daemon applies the policy. A safe step: allowed at once. A risky step: the same stored request the Task view shows; the
  helper waits for the answer, then prints
  `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"|"deny","permissionDecisionReason":"…"}}` and
  exits 0. A deny's reason is what the agent reads.
- Claude Code fails open: a hook that gives no verdict lets the call run (measured, `runtimes.md`). So the helper prints a verdict
  on every path, a deny on any error; a run in this mode does not start when the helper is missing (`NEKO_HOOK_HELPER` in tests);
  and when the stream says `hook_no_verdict` the daemon stops the run at once: "Neko could not check a step, so it stopped the
  run.", naming the step that ran unchecked.
- A step the person denied (or no one answered) shows as "Denied", not failed.
- The request expires, and the step is denied, after a day less a minute (`NEKO_HOOK_EXPIRE_MS` in tests), when the run ends, or
  when Claude Code hangs up on the helper. `onFailure: "block"` blocks the call if the helper crashes or times out.
- When the person's ask rules make Claude Code ask too after the hook said yes, Neko answers that `can_use_tool` yes for the same
  tool call (by its `tool_use_id`), so nobody is asked twice.
- The secret is in the environment of every command the agent runs. With it a command can only put a request to the person; it
  cannot answer one.

## A plugin's tools

A plugin (`plugins.md`) is an MCP server Claude Code starts for the run. Its tools are `mcp__<plugin>__<tool>` and take the same route as
every other tool; nothing about a plugin lets a step through that another tool would ask about.

- Following Claude Code's own rules: Claude Code asks (unless the person's own rules allow the tool), and Neko asks in the task and in
  "Needs you". Ask before risky steps: the hook sees the call and the policy asks. Full access: it goes ahead.
- The request names the plugin: "Use search from acme-docs", why: "acme-docs is a plugin you approved in Neko. Its tools can reach
  whatever the plugin can."; a server that is not one of the run's plugins (from the person's own Claude Code settings) says so instead.
  The detail is the call's input, compact JSON, shortened. It is never destructive: Neko cannot tell what a plugin's tool does.
- "Always allow" is the same tool of the same server for the rest of the task (`tool:mcp__acme-docs__search`); another tool of the
  plugin asks again. Whether a cat has the plugin at all is its own matter (`plugins.md`: which cats).
- Tested with the fake CLI and the real daemon in every mode (`crates/neko-daemon/tests/plugins.rs`).

## Neko's own tools

`mcp__neko__*` (`subagents.md`) are Neko's tools for the agent: start, list and stop its own subagents. They only record work in Neko's
state, so `stream::action` makes them `Action::Internal` and they go ahead in every mode (a `can_use_tool` for one is allowed at once;
the hook says nothing). What a subagent then does is asked about like anything else. A parent's answer to a kitten's question
(`message_subagent`, `subagents.md` "Questions go up") is words, not a decision: it approves no step, and no tool of the agent's answers a
request.

One is not internal: `watch_command` (`waits.md`) runs a command later, unattended, again and again, so `stream::action` makes it
`Action::Watch` and the policy asks as for a command ("Run a check again and again", the exact command; "Always" scoped to `watch:<program>`,
apart from Bash's `command:<program>`). Full access lets it go ahead. Under "Follow Claude Code's own rules" the real CLI allows Neko's
tools up front (`--allowedTools mcp__neko__*`), so it is not asked there.

A **subagent never has more permission than its thread**: the mode is the one setting every run reads. Its "Always allow" answers are
scoped to its own task, so they start empty. Its requests are stored under its own task, answered in its view or in Needs you ("Subagent
1 asks", under its thread's title), and a stop of the thread expires them.

## A request

A stored row (`permissions`): what it wants (title), why Neko asks, the exact command or path, whether it is destructive, its
state, when it was asked and answered. It is also a part of the transcript, so it shows where it happened.

States: `pending` → `allowed`, `always_allowed`, `denied`, or `expired`. Only a pending request can be answered; the store refuses
a second answer (`already_answered`). When a run ends, its pending requests expire. When the CLI takes a request back, it
expires.

## Always allow

Scoped to the task, never wider: the same program for a command (`cargo` in `cd x && cargo test`), the same folder outside the
task's, the same host on the network, the same unknown tool, edits in the same plain folder.

A destructive step has no default answer and no "Always" button; an "always" answer to one counts once. Each part of a command
line (split at `&&`, `||`, `;`, `|`) is read by its program: `rm`, `rmdir`, `sudo`, `dd`, `kill`, `chmod -R`, `find -delete`,
`git push --force` (or `-f`, `+ref`, `--delete`), `git reset --hard`, `git clean` (not `-n`), `git branch -D` or `-d`,
`git checkout -- …`, `git restore`, `git stash drop`, and SQL that drops, truncates or deletes. `grep -r rm src` is not `rm`.

## In the app

A pending request is rune-ui's Confirmation (Allow once in ink, first; Always allow; Deny; Escape denies), with a quiet line under
it saying why Neko asks. A destructive one is the same Confirmation in its destructive form: Allow once in the danger material, Deny, no "Always" and no default answer. Once
answered it becomes one quiet line. The task's sidebar row shows a warning mark while it waits, and the window announces it.

## Tests

`crates/neko-daemon/tests/hooks.rs`, on a temporary HOME holding the person's settings (Bash, Edit and Write allowed, auto mode,
a hook of their own): mode 1 asks nothing for a command those rules allow; mode 2 asks the same command; a read is not asked;
allow, deny (shown as denied), always allow, a destructive command, expiry and the run ending while a step waits; the helper
refuses a wrong secret and says no when it cannot reach Neko; a missing helper stops the run from starting; `hook_no_verdict`
stops the run;
the settings files are byte for byte the same afterwards. `crates/neko-core/tests/conductor.rs` covers the routing.

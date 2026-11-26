# Runtimes: the agent CLIs Neko runs (S1, 2026-11-26)

Neko asks for no API key. It runs the agent CLIs already installed and signed in on the Mac, as child processes. Today that is
Claude Code. Codex is listed in Settings but not run yet.

## The contract

`crates/neko-core/src/runtime.rs`. Every runtime implements `Runtime`:

- `detect()`: found or not, the path, the version, signed in or not, and which login. It spends nothing.
- `start(StartSpec)`: the words, the folder, the permission mode and a conversation to resume. It returns a `Control` and a
  stream of `AgentEvent`s.
- `Control::steer` (more words while it runs), `Control::answer` (a permission decision), `Control::stop`.

The events are Neko's own: `Started` (conversation id, model), `TextDelta`, `TextDone`, `ThinkingDelta`, `ThinkingDone`,
`ToolStarted`, `ToolFinished`, `PermissionAsked` (with an `Action`: read, edit, command, network, other), `PermissionWithdrawn`,
`Usage`, and `Finished` (done, stopped, or failed with a reason). The stream ends with exactly one `Finished`. Stop ends it within
a second.

`tests/contract.rs` runs one suite against an in-process fake runtime and against Claude Code driven by the fake CLI. A new runtime
must pass the same suite.

## How Claude Code is found

`find.rs`. An app opened from the Finder has a short PATH. Neko looks in its own PATH, then in `~/.local/bin`, `~/.claude/local`,
`~/.npm-global/bin`, `~/.bun/bin`, `~/.volta/bin`, `~/Library/pnpm`, `/opt/homebrew/bin`, `/usr/local/bin`. If it is still not
found, or it is a `#!` script that needs node, Neko reads the PATH a login shell sets up (`$SHELL -ilc`, at most five seconds,
once). `NEKO_CLAUDE_BIN` names another binary (tests use the fake).

Detection runs `claude --version` and `claude auth status --json`. From the second it keeps only `loggedIn`, `authMethod`,
`apiProvider` and `subscriptionType`, never the account's name or address. The daemon caches what it found for up to a minute.

## How Claude Code is run

```text
claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages
       --permission-prompt-tool stdio [--permission-mode default] [--settings <hook JSON>] [--resume <session>]
```

- It runs in the task's folder, under the run guard (`run-guard.md`), with PATH set as above. "Ask before risky steps" adds
  `NEKO_HOOK_PORT` and `NEKO_HOOK_SECRET`; nothing else is added.
- The permission mode decides the rest (`permissions.md`): following Claude Code's own rules adds neither `--permission-mode` nor
  `--settings`; asking before risky steps adds `--permission-mode default` and the `PreToolUse` hook; full access adds
  `--permission-mode default`. Neko never passes `--setting-sources`, so the person's own settings, hooks and MCP servers apply.
- Neko first writes an `initialize` control request (as the Agent SDK does), then the person's words as a `user` message.
- Words stream from `stream_event` deltas. A message whose deltas never came gives its words whole from the `assistant` event.
- `tool_use` blocks start tool calls; `tool_result` blocks finish them. A subagent's own messages stay inside its call.
- A shell `tool_result` with `is_error` whose text begins "Exit code N" ran and exited non-zero: the step finished, with its code.
  Only a tool that could not run (denied, an error, cut off) failed.
- Thinking arrives with no words: an empty `thinking_delta`, a `signature_delta`, and the whole block with `"thinking": ""` and a
  signature. Claude Code's own transcript file holds the same empty blocks, so the words are not sent to it at all; Neko drops
  nothing. Empty pieces make no event and store no part.
- Each step Claude Code would ask about arrives as a `can_use_tool` control request and waits for Neko's `control_response`.
- The `result` line ends a turn: usage, `total_cost_usd`, the model from `modelUsage`, and whether it failed. How full the context
  is comes from the last top-level `assistant` message's `usage` (input, cache read and cache written); the window from
  `modelUsage[…].contextWindow` when it is there. Once every message sent has its result, Neko closes stdin and the CLI ends.
- A follow-up after a run is a new run with `--resume` and the conversation id the last run reported.

## The hook contract, as checked

Checked on 2026-11-26 against Claude Code's hooks documentation (https://code.claude.com/docs/en/hooks), its settings page
(https://code.claude.com/docs/en/settings) and permissions page (https://code.claude.com/docs/en/permissions), and
`claude --help` of 2.1.331:

- `PreToolUse` input on stdin: `session_id`, `transcript_path`, `cwd`, `permission_mode`, `hook_event_name`, `tool_name`,
  `tool_input`, `tool_use_id`.
- Output on exit 0: `hookSpecificOutput.permissionDecision` is `allow`, `deny`, `ask` or `defer`; `allow` skips the prompt, `deny`
  blocks the call and its `permissionDecisionReason` is shown to Claude. Nothing printed: the normal permission flow. Exit 2
  blocks. Any other exit is a non-blocking error, unless `onFailure` is `"block"` (2.1.295 or later).
- Deny and ask rules in the person's settings are evaluated whatever the hook returns: a deny rule still blocks, an ask rule still
  asks (and so reaches Neko as `can_use_tool`).
- With several hooks, deny beats defer beats ask beats allow. Matching hooks run in parallel.
- `timeout` is in seconds; the default for a command hook is 600. A timed-out command hook lets the call go ahead unless
  `onFailure` is `"block"`.
- `matcher` `"*"` matches every tool. With `args`, `command` is spawned directly, with no shell.
- `--settings` takes a file or a JSON string, applied above the user, project and local files and below managed settings. Hook
  entries merge across levels rather than replacing each other.
- Hooks inherit Claude Code's environment.
- `claude --help` lists `--permission-mode` choices `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`.
  `default` is not among them, yet the real run with `--permission-mode default` worked.

## Failures

Each becomes a notice in the task with what to do next: not installed (the install line), not signed in (`claude auth login`),
the usage limit (with the reset time from `rate_limit_event` or the result, in local time), the folder gone, a crash (exit code
and its last stderr line when it is plain words), a failure the CLI reported, 30 minutes passed, 5 minutes of silence.

## The fake CLI

`crates/neko-core/src/bin/neko-fake-claude.rs` speaks the same protocol from scripts picked by `[fake:NAME]` in the words: plain,
long, thinking, tools, permission, destructive, outside, slow, spawn, spawn-stay, crash, limit, signin, stall, hook-shell,
hook-twice, hook-read, hook-destructive, real (empty thinking, words between tools, a long path, a check that exits 1), and a
resumed conversation. Like Claude Code it reads `permissions.allow` from `$HOME/.claude/settings.json` and runs the `PreToolUse`
hooks given with `--settings` as the documentation says. Tests give it a HOME of their own. No test runs the real CLI.

## What the first real run confirmed (Claude Code 2.1.331)

1. ~~`-p` together with `--input-format stream-json` and no prompt argument.~~ Works.
2. ~~The `initialize` request.~~ Its reply disturbed nothing.
3. ~~Which steps Claude Code allows by itself.~~ Everything the person's own rules allow: with `Bash`, `Edit` and `Write` allowed
   and `auto` as the default mode, nothing reached Neko. Hence the three modes.
4. ~~Resume by session id.~~ A follow-up with `--resume` continued the conversation.
5. ~~`modelUsage` keys as model names.~~ The run's model came from them.
6. ~~The user's own settings still apply.~~ They do (see 3).
7. ~~Thinking words.~~ None are sent: thinking blocks are empty with a signature.
8. ~~A shell step's exit code.~~ Its result text begins "Exit code N", with `is_error`.

## Still not sure: check on a real run

1. The hook route as a whole: that 2.1.331 runs a `PreToolUse` hook given only through `--settings` in a `-p` stream-json run,
   with exec form `args` and `onFailure`; that a timeout of 86400 seconds is accepted; and that an invalid `--settings` value is
   refused loudly rather than ignored (if it were ignored, mode 2 would fall back to Claude Code's own asks).
2. That a `deny` with a reason reaches the model as the tool's error text, and an `allow` skips the person's own `auto` mode
   classifier.
3. Whether a mod or plugin that approves calls (`tool.check`) can approve a call the hook denied in this setup (the permissions
   page says it can without managed settings).
4. `--permission-mode default` is not in 2.1.331's list of choices; it worked, but may be an alias that goes away (`manual`?).
5. `modelUsage[…].contextWindow` in the `result` line (the session file's cost record has no such field). Without it the ring
   assumes a 200K window and says so.
6. Whether a second `user` message while a turn runs gets its own `result` (Neko closes stdin when the counts match).
7. `--include-partial-messages`: `message_start` ids matching the `assistant` message ids, so words are not shown twice.
8. `result.usage`: `cache_creation_input_tokens` counted as input.
9. Usage limit words: `rate_limit_event` with `status: rejected` and `resetsAt`, and the result text.
10. Not signed in: the result or stderr mentions `/login` or "not logged in".
11. `auth status --json`: the `authMethod` values for an API key and for Bedrock or Vertex.
12. A run stopped by SIGKILL resumes cleanly with `--resume`.

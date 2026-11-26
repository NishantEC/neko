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
       --permission-mode default --permission-prompt-tool stdio [--resume <session>]
```

- It runs in the task's folder, under the run guard (`run-guard.md`), with PATH set as above. No other environment is added.
- Neko first writes an `initialize` control request (as the Agent SDK does), then the person's words as a `user` message.
- Words stream from `stream_event` deltas. A message whose deltas never came gives its words whole from the `assistant` event.
- `tool_use` blocks start tool calls; `tool_result` blocks finish them. A subagent's own messages stay inside its call.
- Each step Claude Code would ask about arrives as a `can_use_tool` control request and waits for Neko's `control_response`
  (`permissions.md`).
- The `result` line ends a turn: usage, `total_cost_usd`, the model from `modelUsage`, and whether it failed. Once every message
  sent has its result, Neko closes stdin and the CLI ends.
- A follow-up after a run is a new run with `--resume` and the conversation id the last run reported.

## Failures

Each becomes a notice in the task with what to do next: not installed (the install line), not signed in (`claude auth login`),
the usage limit (with the reset time from `rate_limit_event` or the result, in local time), the folder gone, a crash (exit code
and its last stderr line when it is plain words), a failure the CLI reported, 30 minutes passed, 5 minutes of silence.

## The fake CLI

`crates/neko-core/src/bin/neko-fake-claude.rs` speaks the same protocol from scripts picked by `[fake:NAME]` in the words: plain,
long, thinking, tools, permission, destructive, outside, slow, spawn, spawn-stay, crash, limit, signin, stall, and a resumed
conversation. Every test and scene uses it. No test runs the real CLI.

## Not sure yet: check on the first real run

The shapes come from the documented SDK protocol and the earlier project's parser; the real CLI was never run on a task.

1. `-p` together with `--input-format stream-json` and no prompt argument (the Agent SDK omits `-p`).
2. The `initialize` request: whether 2.1 needs it, and that its reply does not disturb anything.
3. `--permission-prompt-tool stdio` with `--permission-mode default`: that every ask arrives as `can_use_tool`, and which steps
   Claude Code allows by itself (reads in the folder, some read-only commands) without asking.
4. The `control_response` shape: `{"behavior":"allow","updatedInput":…,"toolUseID":…}` and `{"behavior":"deny","message":…}`.
5. Whether a second `user` message while a turn runs gets its own `result` (Neko closes stdin when the counts match).
6. `--include-partial-messages` events: `message_start` ids matching the `assistant` message ids, so words are not shown twice.
7. `result.usage`: `cache_creation_input_tokens` counted as input; `modelUsage` keys as model names.
8. Usage limit words: `rate_limit_event` with `status: rejected` and `resetsAt`, and the result text.
9. Not signed in: the result or stderr mentions `/login` or "not logged in".
10. `auth status --json`: field names and the `authMethod` values for an API key and for Bedrock or Vertex.
11. A run stopped by SIGKILL resumes cleanly with `--resume`.
12. The user's own settings still apply (allow rules, hooks, MCP servers); Neko does not pass `--setting-sources`.

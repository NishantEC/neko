# Runtimes: the agent CLIs Neko runs (S1, 2026-11-26)

Neko asks for no API key. It runs the agent CLIs already installed and signed in on the Mac, as child processes. Today that is
Claude Code. Codex is listed in Settings but not run yet.

## The contract

`crates/neko-core/src/runtime.rs`. Every runtime implements `Runtime`:

- `detect()`: found or not, the path, the version, signed in or not, and which login. It spends nothing.
- `start(StartSpec)`: the words, the folder, the permission mode and a conversation to resume. It returns a `Control` and a
  stream of `AgentEvent`s.
- `Control::steer` (more words while it runs), `Control::answer` (a permission decision), `Control::stop`, `Control::interrupt` (end the turn, keep the conversation; the default is `stop`).

The events are Neko's own: `Started` (conversation id, model), `TextDelta`, `TextDone`, `ThinkingDelta`, `ThinkingDone`,
`ToolStarted`, `ToolFinished`, `PermissionAsked` (with an `Action`: read, edit, command, network, other), `PermissionWithdrawn`,
`Usage`, and `StepBegan` and `StepEnded` (a model call began or ended: how a steered message is known to be read), and `Finished` (done, stopped, interrupted by the person, or failed with a reason). The stream ends with exactly one `Finished`. Stop ends it within
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
claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --replay-user-messages
       --permission-prompt-tool stdio [--permission-mode default] [--settings <hook JSON>]
       --mcp-config <Neko's own server and this run's plugins> [--strict-mcp-config] --append-system-prompt <the cat's block and the subagent paragraph> --system-prompt-snapshot off [--resume <session>]
```

- The task's cat reaches the agent as `--append-system-prompt`, one argument, on every run; `--system-prompt-snapshot off` makes a
  resumed run take rules edited since. It changes no other flag (`cats.md`).

- Every run gets `--mcp-config <file>`: one JSON file of `mcpServers` made for this run alone (`<data>/plugins-run/<run id>/mcp.json`,
  mode 0600 in a 0700 folder), removed when the run ends. It has Neko's own server `neko` first (`neko-daemon --mcp`, `subagents.md`) and
  then the plugins the task's cat may use (`plugins.md`). The secret settings of a plugin are in the file's `env` and `headers` for the
  length of the run, read from the Keychain when the run starts. A plugin's tool is `mcp__<plugin>__<tool>` and asks through the same
  route as every tool (`permissions.md`).
- **`--strict-mcp-config` is passed** unless Settings ▸ Plugins has "Also give runs my own Claude Code MCP servers" on (`Settings.own_mcp`,
  off by default). Without it the CLI also loads the MCP servers in the person's own settings (measured: about 25 servers, 480 tools),
  which slows every run and gives it tools Neko did not choose. This changes the plugin host's first choice (`plugins.md`).
- Every run's system text (`--append-system-prompt`) is the cat's block, then the subagent paragraph, then, for a subagent, three lines
  about itself (`subagents.md`): one argument, the same bytes on every run.
- `NEKO_MCP_PORT` and `NEKO_MCP_SECRET` are in the CLI's environment on every run: how its `neko` server finds the daemon
  (`subagents.md`). Nothing secret is in argv or the config file.
- `NEKO_CLAUDE_MODEL` in the daemon's environment adds `--model <name>` (the QA rig sets `haiku` for real runs,
  `docs/knowledge/qa.md`). Unset or empty, no `--model` is passed and the CLI's own default applies. The fake reports the model
  it was given (`tests/model.rs`).
- It runs in the task's folder, under the run guard (`run-guard.md`), with PATH set as above. "Ask before risky steps" adds
  `NEKO_HOOK_PORT` and `NEKO_HOOK_SECRET`; every run adds `NEKO_MCP_PORT` and `NEKO_MCP_SECRET`; nothing else is added.
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
  `modelUsage[…].contextWindow` when it is there. Whether a `result` ends the run is the rule under "When a run ends" below.
- Words sent while a run goes are steered, queued or sent as an interrupt (`composer.md`). A steer is one more `user` line on stdin; an interrupt is `{"type":"control_request","request_id":"neko-interrupt-N","request":{"subtype":"interrupt"}}` (the Agent SDK's form), after which the turn's `result` ends the run and stdin closes; the words start a new run. The steer is measured on the real CLI (below); the interrupt is not yet.
- `--replay-user-messages` (valid only with stream-json in and out) makes the CLI echo each `user` line from stdin on stdout when it
  **consumes** it: `{"type":"user","isReplay":true,"message":{…}}`. The stream mapper turns a top-level echo into `Said::Consumed` and
  draws nothing (Neko stored the person's words when they were sent). Ordinary `tool_result` `user` lines carry no `isReplay` and
  work as before.

## When a run ends

A steer written mid-turn may be **folded** into the turn already running (the CLI reads it at its next step and ends with one
`result` for both messages) or taken in a turn of its own (another `result`). Counting results cannot tell the two apart: waiting for
a second result that never comes left a run "Working" until the five-minute silence limit failed it. So the session counts `sent`
(user lines written, counted before the write) and `consumed` (echoes). **At each `result`, stdin closes, and the run ends, when
`consumed >= sent` or the turn was interrupted; otherwise stdin stays open**, because the CLI will consume the waiting message in a
turn of its own and send another `result`. An interrupted turn ends the run whatever is unconsumed (the words that interrupted it
start a run of their own). The rule relies on the echo: a CLI that took a message without echoing it would leave the run to the
silence limit. The first message's echo comes before its turn's `result`, as in the probe below.

**Between that `result` and the process's exit the run is still going, but takes no words.** The CLI exits a moment after stdin
closes, not at once (the real one ends its MCP servers first; a real run took about a second), and the run is released only when its
process has exited and its stdout has ended. Two rules keep that moment harmless:

- **Words sent then wait for the next run.** `Control::takes_words` is false once stdin is closed. The daemon (`queue::send`) then
  holds the words as a queued message, whatever mode they were sent in (Steer, Queue or Interrupt), and the run that is ending starts
  the next one with them as soon as it is released, as any queued message (`composer.md`). A steer whose write fails because stdin
  closed in between is held the same way. Before this, the steer was refused with "Claude Code has finished this turn." and the words
  went nowhere: a real run (`real-make-cattain`, 2026-11-28) sent its follow-up in that second, the instant the last words had
  streamed.
- **A CLI still running ten seconds after that `result` is ended through the run guard.** A watcher thread starts when the run's last
  `result` closes stdin; if the process has not exited within the grace (`NEKO_RUN_END_GRACE_MS`, default 10000), it is stopped like
  any stop (its process group goes with it) and the daemon logs one line. The run ends as its result said (done, or the failure the
  result reported), never "stopped" or "crashed", and its words and usage are kept. So a finished run can never hold the thread for
  minutes. The fake CLI's `NEKO_FAKE_LINGER_MS` keeps it running that long after stdin closes, for the tests (`neko-core`'s
  `claude.rs`, `neko-daemon`'s `run_end.rs`).

Measured on Claude Code 2.1.331 with `--replay-user-messages`, a steer sent at 4.0 s during a 10 s shell loop:

```text
[ 1.9] user isReplay=true  'Run this shell command…'        first message consumed
[ 4.0] sent                'Also: end your final line with the word meow.'
[13.1] user (no isReplay)  tool_result                       an ordinary tool result
[13.1] user isReplay=true  'Also: end your final line…'      the steer consumed INSIDE the same turn
[14.4] result #1           '… meow'                          one result for both
```
- A follow-up after a run is a new run with `--resume` and the conversation id the last run reported. So is a wake: a thread that rests
  while its subagents work is resumed with their reports as one user message (`subagents.md`). So is an answer: a kitten that asked
  with `ask_parent` is resumed with its parent's words (`message_subagent`) as its next user message.
- **A run that asked ends `asked`.** A run whose agent called `ask_parent` or `ask_person` carries the question (`Run.ask`) and ends
  `asked` whatever its last words are; without the tool, last words that end in a question mark still make it `asked`, as before. How a
  kitten asks today on the real CLI in `-p`: it ends its turn with a question, or (told to in its few lines and the tool's
  description) it calls `mcp__neko__ask_parent` and ends its turn. Claude Code's own `AskUserQuestion` is left as it was (a
  bookkeeping tool Neko lets through); whether `-p` offers it to a kitten, and what it answers when no host fills in the answer, is
  not measured yet (below).
- A `rate_limit_event` becomes `AgentEvent::RateLimit {status, resets_at}`; the daemon holds back what it starts by itself while the
  status is not `allowed` or `allowed_warning` (`subagents.md`). `rejected` still ends the run as a usage limit.

## The hook, measured on Claude Code 2.1.331

Three small real runs (outside Neko, in a scratch folder) measured:

1. `--settings` with a `PreToolUse` hook (`matcher` `"*"`, `timeout` 86400) works on top of the person's own settings: their own
   hooks still ran, and the hook was called for every tool call.
2. The hook's stdin has exactly `cwd`, `effort`, `hook_event_name` (`PreToolUse`), `permission_mode`, `prompt_id`, `session_id`,
   `tool_input` (for Bash: `command`, `description`), `tool_name`, `tool_use_id`, `transcript_path`.
3. `permissionDecision` `deny` with a reason blocked a Bash call although the person's settings allowed Bash outright in `auto`
   mode. The stream then carries a `tool_result` with `is_error` and the text "PreToolUse:Bash hook error: <reason>", and the
   `result` lists it under `permission_denials`. `allow` let the call run. Neko shows a denied step as "Denied", not failed.
4. `timeout` is in seconds and holds past the default: a hook that slept 70 seconds, then allowed, was waited for.
5. **It fails open.** When the hook command could not run, Claude Code sent
   `{"type":"system","subtype":"informational","level":"warning","tag":"hook_no_verdict",…,"hook_no_verdict":{"hook_event":"PreToolUse","tool_use_id":…}}`
   and ran the tool. So: the helper prints a verdict on every path (deny on any error, its own timeout, a wrong secret, an
   unreachable daemon, a panic); the runtime refuses to start a run in this mode when the helper is missing or not executable;
   and the daemon stops the run at once when `hook_no_verdict` arrives in this mode, with the notice "Neko could not check a
   step, so it stopped the run." naming the step that ran unchecked.
6. Thinking words are not sent: a thinking block starts with `thinking: ""` and a `signature`, each `thinking_delta` has
   `thinking: ""` and `estimated_tokens`, and `system/thinking_tokens` lines count tokens. Those drive the live "Thinking…" line
   (`Change::Activity`, not stored).
7. Lines the parser passes over without a word: `system/hook_started`, `system/hook_response` (the person's own hooks; some
   print errors such as "command not found" under a short PATH, never shown), `system/session_state_changed`, `system/status`,
   `system/commands_changed` (very large), `system/informational`, `rate_limit_event`. `system/init` carries `permissionMode`
   (`auto`, from the person's settings, when no mode flag is passed), `model` and several hundred tools. The `result` line has
   `total_cost_usd`, `duration_ms`, `num_turns`, `usage`, `modelUsage` keyed by model name, and `permission_denials`.

The fake CLI sends all of these shapes, with placeholder content.

## The hook contract, as documented

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
work (`work:SECS[:step=MS][:deaf][:separate]`: steps a quarter second apart; a user message that arrives meanwhile is folded into the running turn, as the real CLI does: read at the next step, echoed with `--replay-user-messages`, one result for both; with `separate` it waits and gets a turn and a result of its own; an `interrupt` control request ends the turn at once, `deaf` ignores it), hook-twice, hook-read, hook-destructive, hook-no-verdict, delegate (the lines after it, played through the MCP servers `--mcp-config` named, really started: `mcp SERVER TOOL {json}`, `poll N`, `stay N`; steps may also be one line apart by ` ; `), woken (a message that begins `<neko-wake`), plugin-tool:SERVER:TOOL, real (empty thinking, words between tools, a long path, a check that
exits 1), and a resumed conversation. `NEKO_FAKE_RECORD` names a file it appends its arguments, the content of every `--mcp-config` file it was given (read at start, since the file goes with the run) every stdin line and, for `delegate`, every MCP tool call it made (`{"mcp_call": {server, tool, ms, error, text}}`) to. `plugin-tool:SERVER:TOOL` calls `mcp__SERVER__TOOL` through the permission route, or fails at once with "No such tool available" when the config gave no such server. Like Claude Code it reads `permissions.allow` from `$HOME/.claude/settings.json` and runs the `PreToolUse`
hooks given with `--settings` as the documentation says. Tests give it a HOME of their own. No test runs the real CLI.

## Plugins on a real run: not sure yet

`plugins.md` ends with the list: that Claude Code takes `--mcp-config` of this shape alongside the person's own servers, names the tools
`mcp__<name>__<tool>` unchanged and asks about them as it does about other tools, and how the Keychain behaves for the installed app. The
fake CLI proves Neko's side only.

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

1. ~~`onFailure: "block"` and exec form `args` in a `--settings` hook.~~ Measured on 2.1.331 with Neko's exact hook shape: the
   helper was started with `args` as its arguments (no shell); a deny blocked the step and an allow let it run; and with a helper
   that did not exist, the step was **blocked** ("failed; blocking because onFailure is "block""), not run. So this mode fails
   closed in Claude Code itself; the `hook_no_verdict` stop stays as a second guard.
2. That an invalid `--settings` value is refused loudly rather than ignored.
3. Whether a mod or plugin that approves calls (`tool.check`) can approve a call the hook denied in this setup.
4. `--permission-mode default` is not in 2.1.331's list of choices; it worked, but may be an alias that goes away (`manual`?).
5. `modelUsage[…].contextWindow` in the `result` line. Without it the ring assumes a 200K window and says so.
6. ~~Whether a second `user` message while a turn runs gets its own `result`.~~ Often not: it is folded into the running turn (2.1.331). Neko counts consumed messages instead of results ("When a run ends").
7. `--include-partial-messages`: `message_start` ids matching the `assistant` message ids, so words are not shown twice.
8. `result.usage`: `cache_creation_input_tokens` counted as input.
9. Usage limit words: `rate_limit_event` with `status: rejected` and `resetsAt`, and the result text.
10. Not signed in: the result or stderr mentions `/login` or "not logged in".
11. `auth status --json`: the `authMethod` values for an API key and for Bedrock or Vertex.
12. A run stopped by SIGKILL resumes cleanly with `--resume`.
13. `--append-system-prompt` with `--system-prompt-snapshot off` on a resumed run: the edited rules are what the model sees, and
    the person's own CLAUDE.md and settings still apply.
14. Whether a kitten in `-p` is offered `AskUserQuestion`, and what happens when it calls it with Neko's plain allow (no answers filled in). If
    it is offered and returns nothing useful, a kitten's request for it should be denied with words pointing at `ask_parent`.

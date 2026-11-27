# Subagents (2026-11-27)

A thread can split its work into numbered subagents, end its turn and rest. Each subagent's report wakes the thread with `--resume`. This
page is how that works and why. Protocol 13, store 10. The app's side (the card, the note, the waiting line) is in `app-shell.md` and
`design.md`; the scenarios are in `qa.md`.

**Kittens.** Neko's word for subagents, in everything the person reads: cats are the Cattains, and the helpers they and the threads
start are kittens ("Kitten 1", "Waiting on Kitten 1 and Kitten 2", "Woken by Kitten 1 · done"). Everything the model reads keeps
"subagent" (the tool names `start_subagent`, `subagents`, `stop_subagent`, their descriptions and results, the `<neko-wake
kind="subagent">` message and the appended paragraph), because the model understands that word best; the paragraph says once that the
app shows them as kittens. The protocol and the store keep `subagent` too. This page says subagent where it speaks of the mechanism and
kitten where it speaks of what the person sees.

## The model

- **A subagent is a task** with a parent, a number and a root (`Task.subagent`: `{parent, number, root}`). The number is `1`, `2`, and for
  Subagent 1's own subagents `1.1`, `1.2` (the person reads "Kitten 1", "Kitten 1.2"): a dot for each level down. It has no name and no face. It belongs to the thread that
  started it and is **not a thread**: the sidebar, New thread, the Briefing and the notifications never list it. Its folder is its root's
  folder (shared; a copy of its own is later).
- **Depth** is at most 3 levels; at the last level the agent is not offered the tool to start more. At most 24 subagents of one thread
  may be unfinished at once, as a guard against a loop.
- **Limits.** 3 subagents of a thread run at once, the rest are queued (status `new`). 4 runs at once across the daemon (the person's
  own threads count; only what Neko starts by itself waits). Past those, subagents start in the order they were made as room appears.
  While the account's limit is reached (a `rate_limit_event` whose status is not `allowed` or `allowed_warning`), nothing is started or
  woken by Neko until it resets (`resetsAt`, else five minutes).
- **Safety is unchanged.** A subagent has the thread's permission mode (read from Settings for every run, so the same). Its "Always
  allow" answers start empty: they belong to a task. Its requests go to Needs you, labelled "Kitten 1 asks" under its thread's title (the thread's row turns amber too: its `needs_you` flag is on while one of its
  subagents has a request waiting, and screens are told when that changes).
  Its run is guarded like any (`run-guard.md`). Stop on a thread stops everything under it.
- **A subagent is read, not talked to**, for now: `follow_up` and `remove_task` on one are refused with words. Removing the thread
  removes its subagents (refused while one works).

## Neko's own MCP server

Every run gets a stdio MCP server named `neko`: `neko-daemon --mcp` (`crates/neko-daemon/src/mcp_server.rs`). It is the model of the
hook helper (`permissions.md`): a thin process that keeps nothing.

1. **The config.** The run's `--mcp-config` file (`plugins.md`) always has `neko` first, `{"type":"stdio","command":"<neko-daemon>",
   "args":["--mcp"]}`, then the plugins the cat may use. A plugin may not be called `neko`. A run with no plugin has a config with `neko`
   alone.
2. **The secret.** The daemon makes 32 random bytes for each run. The CLI is started with `NEKO_MCP_PORT` and `NEKO_MCP_SECRET` in its
   environment; the server it starts inherits them (measured on the real CLI: an environment variable set on the CLI reaches its stdio
   servers). The secret is in argv nowhere and in no file. It is also inherited by every command the agent runs, so, like the hook's,
   it can only make a request as that run; it cannot make a request as another.
3. **The line.** The server answers JSON-RPC on stdin and stdout and forwards each `tools/call` to `POST /v1/mcp` on the daemon's
   loopback port with the secret as the bearer. The daemon finds the run by the secret (`401` otherwise: the install token is not a
   run's secret). The server gives the daemon 10 seconds and says so in words when it does not answer. It starts with `server/discover`
   refused as the real CLI expects.
4. **The tools**, `mcp__neko__*`. Every one answers in milliseconds and none waits for a subagent:

   | Tool | Does | Returns |
   | --- | --- | --- |
   | `start_subagent {title, prompt, placement?}` | makes the subagent, puts a card in the thread's transcript, starts it when there is room | "Subagent 1 ("Check links") is started. It is working now. Neko wakes you with its report when it ends. Do not poll or wait. If you have nothing else to do, end your turn now." (queued: "…queued behind other work…"). An error in words for a depth, ceiling, empty prompt, `own_copy` (not yet) or a thread that was stopped |
   | `subagents {id?}` | lists the thread's own subagents: number, title, state, last line; with `id` that report in full | the list, ending "Do not call this to wait: Neko wakes you with each report." |
   | `stop_subagent {id}` | stops that subagent and any it started | "Subagent 1 is stopped. You are woken with its report." |

   The server's `initialize.instructions` carry the same guidance but did not visibly reach the model in the real probe, so the
   guidance is also in the tool results above and in the system paragraph below.
5. **Permissions.** `mcp__neko__*` tools are `Action::Internal` (`claude/stream.rs`): they only record work in Neko's own state, so
   they go ahead in every mode, like Claude Code's own bookkeeping tools. Everything a subagent then does asks as usual.

## What the agent is told

`neko_core::subagents::instructions`, passed as `--append-system-prompt` on every run: the cat's block, then one paragraph (most work
needs no subagents; split only independent parts and write complete prompts; start with `mcp__neko__start_subagent`; end your turn
after starting them, do not poll, Neko wakes you with a `<neko-wake>` message; you cannot approve a subagent's steps; subagents share
your folder, so give two different files or start the second after the first reports; your own Agent tool is still fine for a quick
read-only search), then for a subagent three lines saying whose it is and that its final message is its report. The same bytes every
run, so a resumed run's cached prompt is not spoiled by wording that moved.

## Rest and wake

1. The thread's agent calls `start_subagent` (once or several times) and ends its turn. Its run ends `done`.
2. `subagents::ended` runs when any run's thread ends. A thread that is `done` or `asked` while a child is unfinished (queued, working,
   waiting) or a report is unread becomes **Waiting**: no run, **no process**. The thread's `thread_status` is `waiting` (`thread::thread_status`, where S1's derived status gets it), its row shows the still ring
   and "Waiting on Kitten 1" or "2 kittens working", and the conversation's last line says what for.
3. When a subagent's run ends, `subagents::report` stores its report once (`wakes` row, `pending`): its final words (the last agent
   words of its newest run; or the run's reason, or "It ended without saying anything."), the files its run changed (from its
   checkpoints, at most 30, with counts), and its branch. A report for a thread that was stopped is stored `discarded`.
4. The `neko-subagents` thread (asleep on a condition variable; it sets no timer while idle) then delivers pending reports by thread:
   - **The thread's run is going:** the reports are **steered in** as one more message to it (`Control::steer`); the note lands in that
     run.
   - **The thread rests:** after a **2 second batch** (from the first report seen for it; `NEKO_WAKE_BATCH_MS` shortens it in tests) the
     daemon starts a run that `--resume`s the thread's last session and sends one user message: a `<neko-wake kind="subagent" id="1"
     status="done" title="…">` element per report, with `<summary>` (up to 16 KB, then "Call mcp__neko__subagents with this id to read
     the rest."), `<files>` and `<branch>`. XML-significant characters in a report are escaped. The run's transcript has no person part
     for it: it has a **Wake** part per report (the note) first.
   - Held back while the limit news says reached, or while 4 runs are going.
5. Each report is settled `delivered` when it was put in front of the agent. A thread that is woken, reads the reports and ends its turn
   is `done` again, or Waiting again if more subagents still work. Subagent finishes never notify; the thread's final end notifies as a
   thread's does.
6. A subagent's folder is its thread's: the thread's run does not tidy the folder away while a subagent is unfinished or a report is
   unread, and a subagent's run never tidies it.

Cost: each wake is a cold-cache turn on the person's own subscription (the probe measured 21K tokens re-created on a small prompt). That
is why progress never wakes, reports are batched, and a report that arrives while the run still goes is steered rather than resumed.
Keep-alive (the same process answering a second turn; measured to work for 3 minutes, idle CPU 0.1 to 0.4 %) is later, for short waits,
once the guard has a rule for an idle process.

## Stop

`runs::stop` stops the thread's own run and then `subagents::stop_tree`: every subagent under it (any depth), found through the root
column. Their reports are discarded and silenced first, so nothing wakes a thread that was stopped. A subagent with a run is stopped
through its `Control` (the run guard ends its processes); one not yet started, and a thread that only waits, becomes `stopped` at once.
`stop_subagent` and a Stop on a subagent's own view stop that one and below it; its parent is woken with a report that says it was
stopped.

## A restart

A clean stop marks running runs interrupted and a crash does the same at the next start (`runs::recover`). Then
`subagents::recover` gives every subagent that ended with no report one (status `interrupted`, what it had said, or "Neko stopped while
it worked, before it said anything."), and the pass wakes the threads that wait. Queued subagents start again; a thread that was only
Waiting stays Waiting until the reports arrive. Nothing is lost: the subagent's own words are kept as for any interrupted run.

## Settings: the person's own MCP servers

Every run gets `--strict-mcp-config` unless the person turned on Settings ▸ Plugins ▸ "Also give runs my own Claude Code MCP servers"
(`Settings.own_mcp`, off by default; a `settings` row `own_mcp`). Why: a run without it also loaded the person's own Claude Code MCP
servers, which the probe measured as about 25 servers and 480 tools, and a run should have the tools Neko configured, as the tools Neko
copies do. It also keeps every run quick, which matters when one thread starts several. This replaces the plugin host's first choice
(`plugins.md`, `docs/decisions/0002-plugins-start-with-mcp.md`).

## Tests

- `crates/neko-core/tests/subagents.rs`: numbering, depth and the ceiling, reports (stored, pending, settled, discarded, found when
  missing), a report's words, Needs you for a subagent's request, a plugin's name, the migration from store 8.
- `crates/neko-core/src/subagents.rs`, `crates/neko-daemon/src/mcp_server.rs`, `http.rs`: the wake message, the tools by depth, the
  server's JSON-RPC, the `/v1/mcp` head.
- `crates/neko-daemon/tests/subagents.rs`, with the real daemon, the fake CLI and the real MCP hop: two subagents, rest with no process,
  one wake for both with the right `--resume`; a report steered into a running thread; Stop leaves no process; a restart interrupts
  and still wakes; 30 calls of `subagents` each under a second; 3 per thread; 4 across the daemon; a subagent's request in Needs you and
  denied; a subagent is read-only and goes with its thread; a reached limit holds the wake; the person's own MCP servers; a wrong secret.
- The rig: `subagents-wake`, `subagents-stop`, `subagents-restart`, `subagents-poll` (`qa.md`).

## What is not here yet

- `own_copy` placement (a worktree of its own), `after` (stacking), drafts, `message_subagent`, a subagent's question going up,
  steering, `wake_me` and watches. Each is a later slice.
- Keep-alive wakes.
- The Waiting thread's status in the Briefing's wording and a subagent in Review: a thread's receipt covers its own run only.

## Still not sure: check with the real CLI (one manual run, by the person, with the cheapest model)

1. That a real agent calls `mcp__neko__start_subagent` after a thread's words that ask for parallel work, then **ends its turn** and
   does not poll (the probe held 3 of 3 on a stand-in tool; this is the same wording, in the app).
2. That `--strict-mcp-config` together with `--mcp-config` lists `mcp__neko__*` in the CLI's `init` and the `neko` server `connected`.
3. That the real CLI starts `neko-daemon --mcp` with `NEKO_MCP_*` in its environment as the probe measured for a stand-in.
4. That `--resume` plus the unchanged `--append-system-prompt` works for a wake and the model reads the `<neko-wake>` message as a report.
5. That `mcp__neko__*` calls are not asked about by Claude Code in the person's default mode (Claude Code may ask for an MCP tool; if it
   does, it arrives as `can_use_tool` and Neko's policy allows it at once, never reaching the person).

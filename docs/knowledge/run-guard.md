# The run guard (S1, 2026-11-26)

Nothing an agent starts outlives its run. `crates/neko-core/src/guard.rs`.

## How

- The daemon starts the CLI through a guardian: its own binary with `--neko-run-guard <program> <args…>`. The guardian only
  guards: it never opens the store or the network.
- The guardian starts the CLI in a new process group, with the daemon's pipes as its stdio.
- The daemon keeps one end of a socket pair; the guardian has the other as fd 3. Stop closes the daemon's end. The daemon dying,
  even by SIGKILL, closes it too.
- Every 100 ms the guardian records each descendant with its process group and start time.
- When the CLI exits, is stopped, or the daemon's end closes: one last look at the tree, then SIGKILL to the CLI's group four times
  15 ms apart, then SIGKILL to every recorded descendant that is still the same process (same start time) and to the groups they
  lead, again until none is left (at most 2 s). A pid reused by someone else is never touched.
- The guardian catches SIGTERM, SIGINT and SIGHUP and cleans up first.

## Why each piece

- Agent CLIs run each shell command in a process group of its own, so killing the CLI's group missed a test runner it started.
- A process forking at the instant its group is signalled can escape one signal, so the group is signalled more than once.
- A guardian that died on the default signal action left its child running.

## Limits

A run may take 30 minutes. A run that says nothing for 5 minutes stops (time waiting for the person does not count). Both end
the run as failed with the reason. Tests shorten them with `NEKO_RUN_LIMIT_MS` and `NEKO_RUN_STALL_MS`.

A CLI still running ten seconds after the `result` that ended its run is ended the same way; that run is not failed, it ends as its
result said (`runtimes.md`, "When a run ends"; `NEKO_RUN_END_GRACE_MS`).

## Subagents

Each subagent is its own run with its own guardian and its own process group, so nothing above changes. Stop on a thread stops the run
it has and every subagent under it, at any depth (`subagents.md`): each one's guardian then ends its processes within the same
second. A thread that rests (no run, only Waiting) has no process at all, which is the point of waking by `--resume`. The tests count
the processes of the daemon's folder before and after a Stop of two working subagents and require none (`crates/neko-daemon/tests/subagents.rs`,
and the rig's own end check in the four subagent scenarios). Neko's own MCP server (`neko-daemon --mcp`) is a child of the CLI, so it is in
the CLI's tree and dies with it.

## A watch's checks

A command watch (`waits.md`) runs its check outside any run: `/bin/sh -c` in the thread's folder, in a process group of its own, at most
one at a time per watch. It is given a minute, then its whole group is ended; when it ends by itself its group is ended too, so nothing it
started is left. At the daemon's stop every running check's group is ended. A files watch starts no process (FSEvents).

The limits that keep a thread from starting too much: 3 subagents of a thread running, 4 runs across the daemon, 24 unfinished per
thread, depth 3. The 30 minutes and 5 minutes of silence above apply to each subagent's run on its own.

## Tests

`crates/neko-core/tests/guard.rs` (the run ends; the run is stopped) and `crates/neko-daemon/tests/runs.rs` (the daemon is
killed): a grandchild in its own process group, a real `sleep 600`, must die. Waits have deadlines; no fixed sleeps.

## Known gap

A descendant born and orphaned within one 100 ms look is never recorded, and survives if it left the CLI's group.

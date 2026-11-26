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

## Tests

`crates/neko-core/tests/guard.rs` (the run ends; the run is stopped) and `crates/neko-daemon/tests/runs.rs` (the daemon is
killed): a grandchild in its own process group, a real `sleep 600`, must die. Waits have deadlines; no fixed sleeps.

## Known gap

A descendant born and orphaned within one 100 ms look is never recorded, and survives if it left the CLI's group.

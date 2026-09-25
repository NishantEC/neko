# Direct developer Git — 2026-09-25

The diagnosed live reviewer failure came from Apple's `/usr/bin/git` launcher
attempting an xcrun cache write inside the read-only worker sandbox. The parent
task's controlled experiment established that the installed developer Git
binaries return the same status/diff without those diagnostics under the same
policy. A PATH prepend was insufficient because a login shell resets PATH.

`native_runner` now checks two fixed installed developer-tool candidates outside
the worker sandbox, canonicalizes them, and accepts only absolute executable
regular files with bounded/control-free paths. It rejects a symlink back to
`/usr/bin/git`. This lookup starts no child process or shell and does not need
xcrun/cache access. Every worker prompt receives the shell-quoted absolute Git
path and an instruction to report real failures; host Git commands use that path
as well. If neither developer installation exists, the existing launcher remains
the fallback and the prompt explains its possible failure. Sandbox policy and
network access are unchanged.

Verification: the prompt-preservation test first failed because no direct-path
instruction was present, then passed. All 31 native runner tests pass serially,
including a bounded malformed/nonexecutable/symlink-candidate test and an actual
`zsh -lc` invocation of the selected absolute executable in a fixture repository.
The latter emitted no stderr diagnostics, and an invalid Git subcommand still
returned an error. Existing 256-KiB prompt preservation, policy flags, process
timeouts, cancellation, and child cleanup tests also passed.

Fresh live-model completion after this change remains the parent task's check;
this evidence does not claim that result.

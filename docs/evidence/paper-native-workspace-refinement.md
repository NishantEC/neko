# Paper-native workspace refinement verification

Date: 2026-09-25

## Automated checks

- `cargo test -p neko workspace::tests` — passed: 5 tests, including the new
  compact workspace draft cases (folder-derived name, explicit name, and
  plain-language missing-folder error).
- `cargo build --release -p neko -p neko-daemon` — passed.
- `cargo test --workspace` — 525 passed, 6 ignored, 2 failed. Both failures
  are existing process-guardian tests in `neko-core::native_runner` that pass
  when invoked one at a time but fail in the parallel suite:
  `guardian_preserves_stdin_exit_code_and_cleans_up_background_children` and
  `supervisor_death_kills_guarded_descendants`. This change does not touch the
  runner. The suite therefore is not a clean parallel-suite pass.

## Native window probe

Launched the release binaries against isolated data:

```sh
NEKO_DATA_DIR=/tmp/neko-paper-native.H6hqrv \
NEKO_SHOW_WORKSPACE=1 NEKO_WORKSPACE_VIEW=workspaces \
target/release/neko
```

Observed from the live app: the workspace window was non-key, its native Glass
material installed, and its Space/full-screen collection behavior was verified.
The isolated daemon listened on its isolated socket.

The visual capture did **not** pass: `screencapture -o -l1867` returned
`could not create image from window`, and the workspace snapshot remained
unloaded in that non-key evidence session. No screenshot is claimed as proof.
This needs a macOS Screen Recording-authorized capture session plus a repair of
the isolated workspace snapshot delivery before native visual acceptance can be
called complete.

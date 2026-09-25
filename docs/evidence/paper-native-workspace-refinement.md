# Paper-native workspace refinement verification

Date: 2026-09-25

## Automated checks

- `cargo test -p neko workspace::tests` — passed: 5 tests, including the new
  compact workspace draft cases (folder-derived name, explicit name, and
  plain-language missing-folder error).
- `cargo build --release -p neko -p neko-daemon` — passed.
- `cargo test --workspace` — passed: 527 tests, 6 intentionally ignored.
  The guarded-process fixtures retain their coverage while using a realistic
  15-second startup bound for Cargo's full parallel suite; the production
  runner timeout contract is unchanged.

## Native window probe

Launched the release binaries against isolated data:

```sh
NEKO_DATA_DIR=/tmp/neko-paper-native-final \
NEKO_SHOW_WORKSPACE=1 NEKO_WORKSPACE_VIEW=workspaces \
target/release/neko
```

Observed from the live app: the workspace window was non-key, its native Glass
material installed, and its Space/full-screen collection behavior was verified.
The daemon listened on its isolated socket after its real application index
completed; the workspace then made its bounded first-connection retry and
rendered the loaded Tools & skills / workspace-choice surface.

`screencapture` remains denied by macOS Screen Recording policy, but a live
native-app accessibility capture succeeded. It shows the rendered 1240px
workspace window with the Paper-native sidebar, selected Tools & skills state,
workspace chooser, create-workspace entry point, and Watching status card.
No synthetic input was used in that evidence run.

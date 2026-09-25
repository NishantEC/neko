# Paper-native workspace refinement plan

> **Execution:** follow TDD for the new draft-normalization and compact-create
> state. Keep the daemon protocol unchanged. Validate the native evidence
> window without focusing it or synthesizing input.

## Task 1: encode compact workspace creation behaviour

**Files:** `crates/neko/src/workspace.rs`

1. Add a pure helper that derives a readable workspace name from a repository
   path and normalizes the compact creation draft.
2. Add unit tests first: a folder derives its final component, an explicit name
   wins, and a blank folder produces a plain-language validation failure.
3. Make `save_workspace` use the helper; require a repository but not a typed
   name when it can be safely derived.

## Task 2: replace the creation editor dump

**Files:** `crates/neko/src/workspace.rs`, `crates/neko/src/workspace/home.rs`

1. Add a `workspace_advanced` state and reset it on new/select.
2. Render a small Paper-shaped create card: name, repository folder, primary
   action, and an optional instructions disclosure.
3. Preserve the full settings form for an existing workspace.
4. Ensure primary/secondary controls use existing keyboard/focus primitives.

## Task 3: align the native main window to Paper

**Files:** `crates/neko/src/workspace/home.rs`

1. Rework sidebar density and labels to the Paper hierarchy; make the profile
   page reachable via the Agents label.
2. Refine Today, ticket cards, and the rail to Paper's cards, spacing, type
   hierarchy, and narrower readable conversation column.
3. Apply existing content fade helper to changed content regions; preserve
   system Reduce Motion behaviour.

## Task 4: verify

**Files:** no source unless a check reveals a defect

1. Run the targeted workspace tests, then `cargo test --workspace`.
2. Run `cargo fmt --check` and a release build.
3. Start an isolated data-dir native evidence window, capture only that window,
   and verify it stays non-key. Record the actual commands/results in an
   evidence report.

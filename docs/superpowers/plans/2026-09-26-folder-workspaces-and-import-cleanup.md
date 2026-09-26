# Folder Workspaces and Import Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Neko import any existing folder as a workspace while keeping generated folders out of the primary list, then align Paper, clean verified generated files, and reinstall with fresh Neko state.

**Architecture:** Preserve the persisted `Workspace.repository` field for wire and database compatibility, but treat its value as a canonical directory. Keep Git validation as a separate capability check at code-task creation; the daemon's existing worktree runner remains Git-only. Discovery supplies review metadata (`primary` or `other`) without deleting anything, and the native UI renders those two groups using the approved Paper `03e` layout.

**Tech Stack:** Rust workspace (`neko-core`, `neko-daemon`, `neko-protocol`, GPUI `neko`), SQLite, Paper MCP, macOS app bundle and Keychain.

**Working-tree constraint:** Execute on `main` per the user's earlier main-only instruction. The checkout is already dirty; stage and commit only files changed for each task, never sweep unrelated edits into a commit.

---

### Task 1: Folder validation and Git-only code tasks

**Files:**
- Modify: `crates/neko-core/src/workbench.rs` (workspace save, validation, tests)
- Modify: `crates/neko-daemon/src/workbench/import.rs` (selected workspace preflight)
- Modify: `crates/neko/src/workspace.rs` (folder labels and placeholder)
- Test: `crates/neko-core/src/workbench.rs` existing `repository_is_canonical_git_root_and_active_tasks_pin_it`

- [ ] **Step 1: Write the failing test.** Add this to the `workbench.rs` test module. Preserve the existing Git-root and active-task-history assertions as a separate test.

```rust
#[test]
fn folder_workspace_saves_without_git_but_cannot_start_a_code_task() {
    let db = Db::open_in_memory().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let saved = apply(&db, Command::SaveWorkspace {
        workspace: Workspace {
            id: String::new(), name: "Notes".into(),
            repository: folder.path().to_string_lossy().into(),
            instructions: String::new(), away_enabled: false,
        },
    }).unwrap();
    let workspace = &saved.workspaces[0];
    assert_eq!(workspace.repository, folder.path().canonicalize().unwrap().to_string_lossy());
    assert_eq!(
        apply(&db, Command::CreateTask {
            workspace_id: workspace.id.clone(), title: "Fix".into(), goal: "Fix".into(),
        }).unwrap_err(),
        "Code tasks require a Git repository",
    );
    assert!(load(&db).unwrap().tasks.is_empty());
}
```
- [ ] **Step 2: Verify red.** Run `cargo test -p neko-core folder_workspace -- --nocapture`; expect `SaveWorkspace` to fail with `Choose a directory inside a Git working repository`.
- [ ] **Step 3: Implement the split.** Add the function below to `workbench.rs`, and use it in `SaveWorkspace` and daemon import preflight. Keep `canonical_repository` as the Git validator. Before `CreateTask` pushes a task, resolve its workspace and call `canonical_repository(&workspace.repository)`; map a non-Git failure to the tested message. Change the native UI copy from `Local repository folder` to `Workspace folder`, and its placeholder from `/Users/you/Projects/repository` to `/Users/you/Projects/folder`.

```rust
pub fn canonical_workspace_directory(path: &str) -> Result<String, String> {
    required("Workspace folder", path, 4096)?;
    let directory = std::path::Path::new(path)
        .canonicalize()
        .map_err(|_| "Workspace folder does not exist")?;
    if !directory.is_dir() {
        return Err("Workspace folder must be a directory".into());
    }
    directory.to_str().map(str::to_owned)
        .ok_or_else(|| "Workspace folder path must be valid UTF-8".into())
}
```
- [ ] **Step 4: Verify green.** Run the focused test, `cargo test -p neko-core workbench`, and `cargo test -p neko-daemon import`; expect zero failures. Run `git diff --check` on the four touched paths.
- [ ] **Step 5: Commit only those paths** with `git add <each touched path>` and `git commit -m 'Allow folder workspaces while gating code tasks on Git'`.

### Task 2: One home skill, selectable home folder

**Files:**
- Modify: `crates/neko-core/src/setup_import.rs` (discovery and tests)

- [ ] **Step 1: Replace the prior incorrect regression.** Change `home_configuration_directory_is_not_a_workspace_or_duplicate_skill_root` so it expects exactly one global `Example` skill **and** one selectable Workspace candidate for the home path. Add `discover_source(..., "codex")` coverage using a Codex project inventory that lists the home folder; it must produce the same one-skill/one-folder result. Change `a_plain_directory_is_not_offered_as_an_importable_workspace` to assert `workspace.problem.is_none()`; rename it to describe the new behavior.

```rust
assert_eq!(skills.len(), 1);
assert_eq!(skills[0].scope, "global");
assert_eq!(preview.candidates.iter().filter(|item|
    item.kind == ImportCandidateKind::Workspace
        && item.workspace.as_deref() == home.path().to_str()
).count(), 1);
assert!(workspace.problem.is_none());
```
- [ ] **Step 2: Verify red.** Run `cargo test -p neko-core home_configuration_directory -- --nocapture`; expect the workspace candidate assertion to fail under the current `out.repositories.retain` patch.
- [ ] **Step 3: Implement the minimal fix.** Remove the home-folder `out.repositories.retain` block added in the previous turn. In `workspace_roots`, skip only `canonical == home_root` when adding skill scan roots; keep the home directory in `out.repositories` so it can be selected. Remove `has_git_working_tree_marker` from workspace candidate `problem`: an existing non-Git directory has no problem. Do not merge distinct skills by name alone.

```rust
let home_root = home.canonicalize().ok()?;
let canonical = PathBuf::from(repository).canonicalize().ok()?;
if canonical == home_root { return None; } // skip only the second skill-root scan
```
- [ ] **Step 4: Verify green.** Run the focused test, `cargo test -p neko-core setup_import`, and `git diff --check -- crates/neko-core/src/setup_import.rs`; expect zero failures.
- [ ] **Step 5: Commit only `crates/neko-core/src/setup_import.rs`** with `git commit -m 'Keep folder scopes without duplicate home skills'`.

### Task 3: Review generated folders without losing selection

**Files:**
- Modify: `crates/neko-core/src/setup_import.rs` (bounded review classification)
- Modify: `crates/neko/src/onboarding/setup.rs` (primary and collapsed other-folder groups)
- Test: the test modules in those same files

- [ ] **Step 1: Write failing classification tests.** Build temp paths for a normal project directory, a `Documents/Codex/YYYY-MM-DD/task` directory, a `neko-workbench-smoke-*` directory, and a Git worktree whose `.git` file points to a `worktrees/` metadata directory. Assert the Workspace candidate metadata has `review_group=primary` only for the normal project, `review_group=other` and a plain-language `review_reason` for the other three. Assert a missing path is not offered as selectable. Add a native state test asserting `Other folders` starts collapsed and expanding it leaves item selection unchanged.
- [ ] **Step 2: Verify red.** Run `cargo test -p neko-core setup_import` and `cargo test -p neko onboarding::setup`; expect failures from missing review metadata and collapsed state.
- [ ] **Step 3: Add a bounded pure classifier.** In `setup_import.rs`, classify only already-canonical existing directories. Match generated paths against the **user's home-rooted** dated Codex output parent, the exact Neko smoke prefix under the OS temp root, and a bounded `.git` file whose `gitdir:` target contains `/worktrees/`. Put `review_group` and `review_reason` in the candidate metadata; do not change candidate IDs or selection behavior. Skip nonexistent directory candidates while preserving a warning count, so source configs are not mutated.
- [ ] **Step 4: Render Paper-shaped groups.** In `Setup`, add `show_other_folders: bool` initialized `false`. For the Workspaces tab only, render `Projects` first, then a keyboard-operable `Other folders (N)` disclosure; show the `review_reason` on expanded rows. Keep skill/MCP/schedule grouping unchanged. Workspace rows should show their folder path, not `IN A WORKSPACE` or `Needs attention` merely for lacking Git. Keep each row independently selectable.
- [ ] **Step 5: Verify green.** Run `cargo test -p neko-core setup_import`, `cargo test -p neko onboarding::setup`, `cargo test -p neko-daemon import`, and `git diff --check` on touched files. Use `NEKO_DATA_DIR` to run `scripts/smoke-import.mjs` against a fresh isolated daemon; check that ordinary projects remain primary and generated paths are optional, not silently imported.
- [ ] **Step 6: Commit only the two touched files** with `git commit -m 'De-clutter imported workspace choices'`.

### Task 4: Align and clean the Paper design

**Design source:** Paper file `neko`, ID `01M3A0PQVJM1HSHZK3ZRM29JD4`, page `p-1-0`.

- [ ] **Step 1: Capture before-state.** List all 18 artboards and export screenshots of `03c` (`2BB-0`), `03e` (`2JF-0`), and `04` (`44-0`). Save the node IDs and content hashes in a short evidence note under `docs/evidence/`.
- [ ] **Step 2: Update `03e`.** Keep its two-column Paper layout and individual checkboxes. Change workspace copy to `Choose folders`, add a collapsed `Other folders` disclosure with a generated/worktree count, and show one non-Git folder as selectable. Remove wording that calls every folder a repository.
- [ ] **Step 3: Update `03c` and `04`.** Show one global skill rather than a duplicate home-scoped copy in `03c`; keep workspace-local items and individual selection. In `04`, show scopes as `Everywhere` and named folders, without a Git requirement. Do not replace the existing visual language.
- [ ] **Step 4: Audit before deleting.** Compare all artboards' names and screenshots against the six-step onboarding flow. Remove only exact artboard IDs demonstrably superseded by the updated states; if no artboard is clearly redundant, leave all 18 and clean labels/placement instead. Record every removed ID, or state `none`, in the evidence note. Do not delete `03a`–`03e` merely because they are substates.
- [ ] **Step 5: Verify visually.** Capture after-state screenshots for the three edited artboards, compare at native artboard size, and check readable text, selected/unselected states, and footer navigation. Open the Paper file for the user to inspect.

### Task 5: Inventory and remove only confirmed generated folders

**Files:** No repository source files. Inventory note under `docs/evidence/` only.

- [ ] **Step 1: Inventory exact candidates.** Use the current import preview only as a starting list. For each dated Codex output folder and `neko-workbench-smoke-*` folder, resolve `realpath`, confirm it exists, record `du -sh`, `find` file count, `.git` status, last modification, and process/open-file references. Also inspect any managed worktree against `git worktree list --porcelain`; do not infer that every `Other folders` item is deletable.
- [ ] **Step 2: Present a path-by-path deletion list.** Distinguish disposable smoke fixtures from Codex output that may contain user-created artifacts. Ask for explicit confirmation of exact paths. No deletion before the answer.
- [ ] **Step 3: Move only confirmed paths to a dated, exact Trash directory.** Do not use a broad glob or delete the parent `Documents/Codex`; avoid permanent `rm -rf`. Verify each path is absent from its original location and present in Trash. Record recoverability and total size moved.

### Task 6: End-to-end verification and clean reinstall

**Files:** `scripts/install-clean.sh` (existing, only edit if its behavior fails); `README.md` and `docs/architecture.md` for changed workspace semantics.

- [ ] **Step 1: Update docs.** Replace Git-required workspace claims with folder-first semantics; explain Git-only code tasks and the collapsed generated-folder review. Keep historical nuance in `AGENTS.md` intact.
- [ ] **Step 2: Run checks.** `cargo test --workspace`, `node scripts/smoke-import.mjs`, `node scripts/smoke-workbench.mjs`, and `git diff --check`; report exact passes, ignored tests, and unrelated failures.
- [ ] **Step 3: Inspect the live onboarding.** Use an isolated `NEKO_DATA_DIR` before touching the installed app. Confirm folder selection, primary/other grouping, scoped skills, keyboard disclosure, and non-Git task error in a real native window—not just a screenshot-only evidence mode.
- [ ] **Step 4: Install with fresh state.** Once checks and Paper review pass, inspect `/Applications/Neko.app`, its process IDs, and Neko data targets. Use the user's standing clean-reinstall instruction and run `scripts/install-clean.sh` with the already-configured signing identity. Verify the new bundle signature/version, launched daemon path, and empty/fresh onboarding state. State explicitly what data was removed and whether it is recoverable.

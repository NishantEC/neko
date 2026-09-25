# Unified local import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make first-run import automatically discover Codex, Claude, Paseo-compatible metadata, local skills, workspaces, MCPs, and paused schedules in one review/apply flow without implicit execution or authority.

**Architecture:** Preserve the daemon-owned discovery session and typed IPC boundary. Extend the redacted preview with stable selectable candidate records, add source adapters behind a read-only registry, and have onboarding trigger one discovery when the Import step opens. Apply validates the reviewed ledger and delegates existing workspace, MCP, skill, and schedule stores without enabling or granting anything.

**Tech Stack:** Rust workspace, serde protocol types, SQLite-backed `neko-core`, daemon workbench controller, GPUI onboarding UI, Rust unit/integration tests, Node smoke fixtures.

---

### Task 1: Extend the preview ledger and source-adapter seam

**Files:**
- Modify: `crates/neko-protocol/src/setup_import.rs`
- Modify: `crates/neko-core/src/setup_import.rs`
- Test: `crates/neko-core/src/setup_import.rs` tests
- Test: `crates/neko-protocol/src/setup_import.rs` tests if serialization coverage is needed

- [ ] **Step 1: Write the failing tests** for stable candidate IDs, source/scope metadata, and a read-only adapter context that cannot execute commands or read outside bounded roots.
- [ ] **Step 2: Run the focused tests and confirm they fail** with missing fields/adapter types.
- [ ] **Step 3: Add typed redacted records** for workspace, connection, skill, and schedule candidates while keeping `ServerConfig` and `Secret` daemon-only. Add source/scope fields with serde defaults for backwards-compatible stored previews.
- [ ] **Step 4: Add the internal `ImportSource` trait and `DiscoveryContext`** and route existing Codex/Claude connection and schedule discovery through adapters without changing their validation rules.
- [ ] **Step 5: Add a conservative Paseo compatibility adapter** that reads only documented portable local metadata when present and returns warnings for unsupported/live state; never import transcripts, credentials, grants, or enabled state.
- [ ] **Step 6: Run focused core/protocol tests, then commit** with `feat: add unified import preview ledger`.

### Task 2: Add skills and workspace candidates to discovery/apply

**Files:**
- Modify: `crates/neko-core/src/setup_import.rs`
- Modify: `crates/neko-daemon/src/workbench/import.rs`
- Modify: `crates/neko-protocol/src/setup_import.rs`
- Test: `crates/neko-daemon/src/workbench/import.rs` tests
- Test: `crates/neko-core/src/skills.rs` or a focused setup-import fixture test

- [ ] **Step 1: Write failing daemon tests** proving selected skills are recorded but disabled, selected workspaces are created idempotently, and unselected/unknown candidates are rejected.
- [ ] **Step 2: Run the tests and verify the expected failures.**
- [ ] **Step 3: Add skill/workspace selection IDs to `ImportCommand::Apply`** with serde defaults so existing clients remain decodable.
- [ ] **Step 4: Include bounded global/workspace skill records and discovered workspace metadata in `Discovery` and `ImportPreview`; preserve path and content hash, never copy source files during discovery.
- [ ] **Step 5: Apply selected skills through existing review/audit storage with `enabled=false`; apply selected workspaces through the existing idempotent workspace command; retain existing MCP/schedule behavior.
- [ ] **Step 6: Add retry/idempotence and zero-activation assertions, run daemon/core tests, and commit** with `feat: import skills and workspaces safely`.

### Task 3: Trigger automatic first-run discovery and render unified review

**Files:**
- Modify: `crates/neko/src/onboarding/setup.rs`
- Modify: `crates/neko/src/onboarding/state.rs` only if a once-per-entry guard belongs in pure state
- Test: `crates/neko/src/onboarding/setup.rs` state/UI command tests

- [ ] **Step 1: Write failing state tests** for one automatic discover command per Import-step entry, retry after error, and no duplicate command while a scan is busy.
- [ ] **Step 2: Run the focused tests and verify they fail.**
- [ ] **Step 3: Add the once-per-entry trigger** when the Import step becomes active; preserve the manual retry action and existing repository selection.
- [ ] **Step 4: Replace the fragmented import rows with a unified source/scope review grouped by source, showing metadata, warnings, disabled/untrusted status, and selection controls for workspaces, MCPs, skills, and schedules.
- [ ] **Step 5: Keep credentials and local-process trust as independent opt-in controls; add explicit copy explaining that discovery never starts tools or enables skills.
- [ ] **Step 6: Submit all selected candidate IDs in `Apply`, add post-apply status text, run onboarding tests, and commit** with `feat: auto-discover local setup in onboarding`.

### Task 4: Smoke coverage, documentation, and native verification

**Files:**
- Modify: `scripts/smoke-import.mjs`
- Modify: `docs/evidence/import-and-global-scopes.md`
- Modify: `README.md`
- Test: existing workspace test commands and smoke fixture files

- [ ] **Step 1: Add a fixture** containing Codex, Claude, Paseo-compatible metadata, global/workspace skills, and one unsupported item; assert preview redaction and source/scope grouping.
- [ ] **Step 2: Add smoke assertions** for automatic discovery state, selected apply, disabled MCP/skills, paused schedules, idempotent retry, and no spawned external tools.
- [ ] **Step 3: Run `cargo test --workspace` and the focused import smoke script; fix only failures caused by this feature.
- [ ] **Step 4: Build the app and daemon, run an isolated native onboarding instance with `NEKO_DATA_DIR`, and capture Import review plus post-apply states using the existing non-key evidence flow.
- [ ] **Step 5: Update README/evidence with verified boundaries and exact commands; commit** with `test: verify unified local import flow`.

### Final review and merge

- [ ] Dispatch a spec-compliance reviewer against the design and this plan.
- [ ] Dispatch a code-quality reviewer after spec compliance passes.
- [ ] Run the complete workspace test suite and inspect `git diff --check`.
- [ ] Merge the task commits into `main` only after both reviewers approve; report the exact test and native-proof boundary.

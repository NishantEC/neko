# Source-first local import implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Let users choose one detected local source, scan it on demand, select individual scoped items, and see duplicate status on each item.

**Architecture:** The daemon lists sources using bounded filesystem-presence checks. A scan retains only one source in an expiring session; the protocol preview carries source and per-item match status without credentials. The native setup step renders a source picker, scanning state, and item-level review; imports retain their current disabled/ungranted defaults.

**Tech Stack:** Rust, GPUI, serde wire protocol, daemon-owned SQLite, `cargo test`, native setup evidence capture.

---

### Task 1: Source-scoped discovery

**Files:** `crates/neko-protocol/src/setup_import.rs`, `crates/neko-core/src/setup_import.rs`, `crates/neko-daemon/src/workbench/import.rs`.

- [ ] Add a failing core test: choosing Codex reads Codex config/skills/schedules but not Claude or Paseo; choosing Claude does the inverse.
- [ ] Add a failing daemon test: listing sources does not create an import session, and a scoped scan preview contains only the chosen source.
- [ ] Add source catalog and scoped discovery, preserving the existing all-source helper for compatibility tests.
- [ ] Run focused core and daemon tests; then the import smoke script.

### Task 2: Per-item matching and provenance

**Files:** `crates/neko-core/src/setup_import.rs`, `crates/neko-daemon/src/workbench/import.rs`, `crates/neko-protocol/src/setup_import.rs`.

- [ ] Add failing tests for exact skill/MCP/workspace matches and same-name/different-scope nonmatches.
- [ ] Annotate individual preview candidates against stored Neko definitions and retain source provenance after apply. Never compare or serialize secrets in the preview.
- [ ] Skip exact existing imports while preserving their credentials and grants; do not merge same-name/different-definition items.
- [ ] Run focused tests and import smoke script.

### Task 3: Native source picker and per-item review

**Files:** `crates/neko/src/onboarding/setup.rs`, `scripts/smoke-import.mjs`.

- [ ] Add failing UI state tests for list → scan → review → source picker, and for selection scoped to one source.
- [ ] Render detected-source tiles first. Scan only on click, then show category tabs and independent candidate rows with scope and inline match status.
- [ ] Keep credentials, process trust, and schedules opt-in; skip the step without an import.
- [ ] Run focused app tests, full import smoke, build, and real native visual/interaction checks.

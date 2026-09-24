# Linear standing responsibility implementation plan

**Goal:** Watch assigned Linear issues and prepare local fixes only for evidenced low-risk bugs. This is the first bounded responsibility, not a claim that the general autonomous supervisor is finished.

**Architecture:** Reuse durable intake and the resident daemon. A read-only supervisor inspects source and returns a structured decision: prepare a fix, ask for a decision, or skip. A deterministic permission gate, not model prose alone, authorizes automatic edits. The same gate rechecks the latest workspace permission, connection freshness, assignment, issue revision, and plan identity at claim time.

**Tech stack:** Rust, serde, SQLite, existing Codex CLI adapter and GPUI workspace.

1. Add regression tests proving Away never authorizes an unassessed/manual task. Add serializable supervisor decisions and source revision fields with fail-closed defaults for older snapshots.
2. Implement the decision parser and permission gate in `neko-core/src/supervision.rs`. Require a bug, low risk, evidence, bounded relative files, concrete tests, no uncertainty or sensitive categories. Reject malformed responses; preserve them for manual review. Unit-test each denial independently and fresh assignment eligibility. Low priority is not low risk.
3. Wire assigned-issue planning to the supervisor prompt; persist its decision and human-readable plan. Mark removed assignments inactive after successful sync. Do not auto-build after changed issue content, failed/stale sync, revoked permission, or paused connection. Keep explicit manual approval distinct.
4. Expose the standing responsibility and decision rationale in the native workspace. Preserve existing data, worktrees, and external-write restrictions. Update README/architecture with the exact delivered boundary.
5. Run `cargo test --workspace --quiet -- --test-threads=4`, build GUI/daemon, and run `node scripts/smoke-workbench.mjs`. Add deterministic standing-responsibility coverage. No live Linear watch is claimed without a user-connected account/repository. No automatic PRs, merges, messages, or new connector installs.

Risk assessment is model judgment, not proof or filesystem confinement. The native runner's sandbox remains the write boundary; independent review and user handoff remain required. General goal-directed delegation, continuous conversation, arbitrary MCP provisioning, and OAuth migration are separate work, not implied by this gate.

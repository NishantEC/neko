# Assigned Linear bugs: local verification

Implemented in `codex/personal-agent`, on top of the uncommitted native
workbench foundation. This is one bounded responsibility, not a complete
general-purpose autonomous supervisor.

## Delivered boundary

- Assigned issues receive a read-only investigation and persisted structured
  assessment. Automatic local preparation requires a low-risk bug with
  concrete evidence, bounded ordinary source paths, tests, and no reported
  sensitive impact or uncertainty.
- Claim time rechecks standing permission, active assignment, unchanged issue
  revision/content, enabled connection, and a successful sync within 15 minutes.
  Manual tasks and malformed assessments remain behind explicit approval.
- The builder and independent reviewer receive the full assessment, file
  boundary, required tests, and escalation instructions. Work uses an isolated
  local worktree; this responsibility does not publish, push, merge, or message.
- A full intake queue records a separate notice, not a failed-sync error that
  would prevent already queued tasks from progressing.

## Fresh verification

- `cargo test --workspace --quiet -- --test-threads=4`: 854 passed, 5 ignored,
  zero failures. Breakdown: GUI 337, SDK 13, core 393 (+5 ignored), daemon 52,
  harness 53, protocol 6.
- `cargo build -p neko -p neko-daemon --quiet`: passed; existing warnings remain.
- `node scripts/smoke-workbench.mjs`: passed with a deterministic agent fixture,
  real daemon IPC and SQLite, isolated Git worktree, local build, independent
  review, cancellation/retry, and restart persistence. Low-risk assigned work
  progressed automatically; sensitive and manual work stayed awaiting approval.
  Original repository content remained unchanged.
- Scratch evidence:
  `/var/folders/6f/fd69tssd61g96wf5l3xqf2rw0000gq/T/neko-workbench-smoke-qtVfqs`;
  task `2e5a426a8c7993c864034438f00c94f5`.
- Policy tests and both review regressions were observed failing before their
  fixes. Independent follow-up review closed the intake-capacity and missing
  execution-scope findings; its nine workbench tests passed in both targets.
- `git diff --check`: passed.

## Native UI evidence

The isolated non-key native window reported `loaded=true`, `busy=false`,
`connected=true`, `watch=true`. A prior capture showed stale connecting chrome
even after the state arrived. Explicit window refresh after snapshot updates
fixed the observed repaint symptom; the exact upstream invalidation cause is
not proven. No synthetic keyboard/mouse input was used.

`2026-09-24-standing-responsibility-native.png` shows the loaded native
integration view, workspace, and a missing-Keychain-credential error. This is
expected for the deliberately credential-free fixture connection, not proof of
live Linear synchronization. The capture does not verify the watch-toggle view.

## Still unverified / not activated

No real Linear account or user repository was connected, and no live watch was
enabled. The new structured assessment path was fixture-tested, not tested
against a live model in this pass. An earlier live Codex planning/build/review
smoke predates this policy and is not evidence for its risk classification.

Risk classification remains model judgment, not proof. File-name screening is
not a filesystem sandbox or enforced diff allowlist. The runner sandbox is the
write boundary; read-only execution does not establish confidentiality between
workspaces. General adaptive delegation, arbitrary MCP provisioning, OAuth
migration, and a signed installed-app update are not part of this result.

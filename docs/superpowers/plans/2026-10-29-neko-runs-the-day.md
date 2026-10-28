# Neko runs the day — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scopes, routines that act and report, an Inbox page, and macOS notifications, replacing the three Paseo jobs.

**Architecture:** Two pure core modules: `neko_core::scopes` (reads the disk) and `neko_core::routines` (store, due times, prompt, parsing, seeds). One daemon thread runs due routines through `native_runner::run` with `provider: "claude"` in the scope folder. Routine definitions and runs live in their own settings rows. Web and native talk through new `Request` variants.

**Tech Stack:** Rust (neko-protocol, neko-core, neko-daemon), React/TS (web), SwiftUI (NekoKit).

**Spec:** `docs/superpowers/specs/2026-10-29-neko-runs-the-day-design.md`

## Global Constraints

- Routine rows: `routines_v1` (definitions and standing orders, written immediately), `routine_state_v1:<id>` and `routine_runs_v1:<id>` (written immediately, small). Nothing goes in `workbench_snapshot_v1`.
- At most one routine run at a time. Skip while `machine::latest()` has `holding` set. Timeout 15 min. Default model `sonnet`.
- Keep the last 50 runs per routine. Each `needs_you` item gets a global monotonic `seq` stored in `routine_seq_v1`.
- Default standing orders are copied verbatim from the spec.
- Quiet runs make no inbox item and no notification.
- `scripts/check.sh` must pass before every commit that touches Rust or Swift. Commit on `main` with conventional subjects.

## Review Focus

1. The agent reply has no JSON block, or a malformed one. The run is recorded as failed with its text kept; state does not advance `memory`.
2. A scope folder was deleted. The run fails with "Scope folder missing" and does not loop: the next due time still advances.
3. The daemon restarts mid-run. The `running` flag is stale, so it is cleared at startup and the routine is due again.
4. Clock or DST jumps in `Daily` cadence. The next due time is always > now (test with a now just past the slot).
5. Notification flood after a long sleep: the native side posts at most 5 notifications per poll and one summary for the rest.

---

### Task 1: Protocol types and requests

**Files:** Create `crates/neko-protocol/src/routines.rs`; modify `crates/neko-protocol/src/lib.rs` (mod, `Request`, `Response`).

**Produces:**
- `RoutineKind { Messages, Digest, PrSweep, Custom }`
- `Cadence { Every { minutes: u32, from_hour: u32, to_hour: u32 }, Daily { hour: u32, minute: u32 } }`
- `Routine { id, workspace_id, kind, name, instruction, cadence, enabled: bool, model: String, paseo_schedule: Option<String> }`
- `RoutineState { last_run_ms: Option<i64>, next_due_ms: Option<i64>, quiet_streak: u32, memory: serde_json::Value, running: bool, paseo_paused: bool }`
- `NeedsYou { seq: u64, title, detail, link: Option<String>, done: bool }`
- `RoutineRun { id, routine_id, started_ms, finished_ms: Option<i64>, ok: bool, quiet: bool, did: Vec<String>, needs_you: Vec<NeedsYou>, error: Option<String>, text: String }`
- `ScopeServer { name, origin: String /* file path */, level: String /* "scope" | "repo" */, repo: Option<String> }`
- `ScopeProposal { name, folders: Vec<String>, servers: Vec<ScopeServer>, problems: Vec<String>, workspace_id: Option<String> }`
- `RoutineView { routine, state, runs: Vec<RoutineRun> }` (runs newest first, at most 10)
- Requests: `Routines`, `SaveRoutine { routine }`, `RunRoutineNow { id }`, `SetRoutineEnabled { id, enabled }`, `ClearNeedsYou { routine_id, run_id, seq }`, `SetStandingOrders { workspace_id, text }`, `Scopes`, `RoutineNotifications { after: u64 }`
- Responses: `Routines { routines: Vec<RoutineView>, standing_orders: BTreeMap<String,String>, scopes: Vec<ScopeProposal> }`, `RoutineNotifications { items: Vec<(String /*scope name*/, NeedsYou)>, latest: u64 }`

- [ ] Round-trip serde test for `Routine` and `RoutineRun`; run `cargo test -p neko-protocol routines`; commit `feat(protocol): routine and scope types`.

### Task 2: `neko_core::scopes` discovery

**Files:** Create `crates/neko-core/src/scopes.rs`; register in `lib.rs`.

**Produces:** `pub fn discover(home: &Path, claude_json: Option<&serde_json::Value>) -> Vec<ScopeProposal>`

Rule: each directory `home/Documents/<top>` that has `.mcp.json` with ≥1 server, or ≥2 child repos with MCP config (child `.mcp.json` or `~/.claude.json` `projects[path].mcpServers`), is a scope named by `NAMES` (`work`→"Acme", `clientco`→"Client A", `web-v2`→"Client B", else the folder name). If a top folder itself has config only through `~/.claude.json` (like `web-v2`), it is a one-folder scope. Problems:
- `"<server> is set up separately in N repos under <top>; move it to <top>/.mcp.json"` for N ≥ 3.
- `"<server> is configured twice in <repo>"` when the same name appears in a repo's `.mcp.json` and its `.claude/settings.local.json` or `~/.claude.json` entry.

- [ ] Tests with a temp home: `tcc_scope_lists_scope_and_repo_servers`, `repeated_repo_server_is_a_problem`, `top_folder_without_config_is_not_a_scope`. Run `cargo test -p neko-core scopes`; commit `feat(core): discover scopes from MCP config on disk`.

### Task 3: `neko_core::routines` pure logic and store

**Files:** Create `crates/neko-core/src/routines.rs`; register in `lib.rs`.

**Produces:**
- `pub const DEFAULT_STANDING_ORDERS: &str` (spec text)
- `pub fn next_due(cadence: &Cadence, quiet_streak: u32, now_ms: i64, offset_secs: i32) -> i64`. `Every`: now + minutes × 2^min(quiet_streak, 2), moved to the next `from_hour` if outside the hour window. `Daily`: the next slot strictly after now.
- `pub fn build_prompt(routine: &Routine, scope_name: &str, orders: &str, memory: &Value) -> String` (ends with the output contract)
- `pub fn parse_reply(text: &str) -> Result<Parsed, String>`, where `Parsed { quiet, did, needs_you: Vec<(String, String, Option<String>)>, memory: Value }`. Takes the last ```` ```json ```` fence.
- `pub fn seeds(scopes: &[ScopeProposal]) -> Vec<Routine>` (3 per scope with a workspace id, per the spec table; `paseo_schedule` set on the "Asha" scope's routines only: `message-watch-adaptive` goes on Client A's messages, since that job watched Client A)
- Store: `load(db) -> Store`, `save_definitions(db, &Store)`, `load_state(db, id)`, `save_state`, `load_runs(db, id)`, `push_run(db, id, run)` (keeps 50), `next_seq(db) -> u64`. `Store { routines: Vec<Routine>, standing_orders: BTreeMap<String,String>, seeded: bool }`.

- [ ] Tests: `every_backs_off_and_respects_hours`, `daily_is_strictly_after_now`, `parse_takes_last_json_fence`, `parse_rejects_missing_block`, `seeds_three_per_scope`, `runs_keep_fifty`. Commit `feat(core): routines store, cadence and reply contract`.

### Task 4: Daemon runner and requests

**Files:** Create `crates/neko-daemon/src/routines.rs`; modify `main.rs` (spawn thread), `server.rs` (dispatch the requests).

- At start: clear stale `running`. On the first `Scopes`/runner tick, if `!store.seeded`: discover scopes, create missing workspaces through `state.workbench.command(Command::SaveWorkspaceWithFolders{..})` (new 32-hex id, `repository = folders[0]`), insert `seeds`, set `seeded`.
- Loop every 30 s: pick the enabled routine with the smallest `next_due_ms <= now` (`None` = due now); skip while held; run in a spawned thread guarded by an `AtomicBool` busy flag. Directory = the scope's first folder. On success, store the run, advance state (quiet → `quiet_streak += 1`, otherwise 0), assign `seq` to each needs-you item, and broadcast `Event::WorkbenchChanged`-equivalent so pages refresh. After the first `ok` run of a routine with `paseo_schedule` and `!paseo_paused`, run `paseo schedule pause <name>` (via `paseo` on PATH, 20 s timeout) and record the result.
- `RunRoutineNow` sets `next_due_ms = now`.

- [ ] Tests in `routines.rs`: `pick_due_chooses_most_overdue_enabled`, `stale_running_is_cleared`. Run `scripts/check.sh --quick`; commit `feat(daemon): run routines and answer routine requests`.

### Task 5: Inbox page (web)

**Files:** Create `web/src/pages/Inbox.tsx`; modify `web/src/pages/index.ts` (register `inbox`, group main, icon `Tray`, right after Home).

Polls `Routines` every 10 s. Per scope: open needs-you items (Done → `ClearNeedsYou`), the last 10 "did" lines, and a routine list (name, next run, last outcome, Run now, Pause/Resume, Edit dialog with instruction, cadence and model). Standing-orders textarea per scope (`SetStandingOrders`). Composed from `@/editorial` and `@/ui`.

- [ ] `npm --prefix web run build` passes and `npm --prefix web run typecheck` passes (or the project's equivalent). Commit `feat(web): inbox page for routines`.

### Task 6: Native notifications

**Files:** Create `native/NekoKit/Sources/NekoNative/RoutineNotifier.swift`; start it from `AppModel` (or the app delegate) once.

Polls `RoutineNotifications { after }` every 30 s using `DaemonClient`. Stores `after` in `UserDefaults` (`neko.routineNotificationSeq`). The first poll ever only records `latest`. Posts ≤5 `UNNotificationRequest`s and one summary for the rest. Click → `model.requestedPage = .home` plus opening the web `#/inbox` route.

- [ ] `scripts/check.sh` passes. Commit `feat(native): notify for routine items that need you`.

### Task 7: Docs, install, live check

- [ ] Add `docs/knowledge/neko-runs-the-day.md` and an index line in `CLAUDE.md`.
- [ ] `bash scripts/install-native.sh`. Trigger `RunRoutineNow` for Client A messages, read its run, and confirm the Paseo schedule paused.
- [ ] Commit `docs: routines, scopes and the inbox`.

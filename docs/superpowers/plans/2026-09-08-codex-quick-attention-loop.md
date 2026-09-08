# Codex Quick Attention Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the existing `⌥Space` palette show all local Codex tasks and
urgent Codex approvals from a warmed local snapshot, with safe review and
resolve actions, while preserving all existing Paseo/launcher behaviour.

**Architecture:** `neko-daemon` owns one supervised `codex app-server` stdio
child. A transport actor reduces JSON-RPC responses and notifications into an
immutable `neko_core::codex::Snapshot`; fast providers read only that snapshot.
The protocol carries one combined attention count so the existing client and
Dock badge can react without knowing whether Codex or Paseo produced it.

**Tech Stack:** Rust 2024; existing `serde_json`, `std::process`,
`std::sync::{mpsc, Arc, RwLock}`, GPUI, local Codex app-server JSON-RPC.

---

## File structure

| File | Responsibility |
| --- | --- |
| `crates/neko-core/src/codex.rs` | Provider-neutral Codex task/approval models, JSON reducers, snapshot and search providers. No process I/O. |
| `crates/neko-daemon/src/codex.rs` | Supervised `codex app-server` child, JSONL request/reply correlation and snapshot updates. |
| `crates/neko-daemon/src/main.rs` | Starts the Codex supervisor after the daemon state is ready. |
| `crates/neko-daemon/src/server.rs` | Holds the snapshot/control channel, registers providers, combines attention counts and routes action requests. |
| `crates/neko-core/src/agents.rs` | Adds a task-agnostic tile predicate and preserves Paseo provider behaviour. |
| `crates/neko-protocol/src/lib.rs` | Adds serializable combined-attention/event data only; no Codex credentials or raw JSON crosses this boundary. |
| `crates/neko/src/panel.rs` | Recognizes Codex task tiles, renders stale/unavailable state and offers review actions. |
| `README.md`, `docs/architecture.md`, `AGENTS.md` | Document the local app-server process, Codex capabilities and first-run degradation. |

### Task 1: Add pure Codex snapshot models and reducers

**Files:**

- Create: `crates/neko-core/src/codex.rs`
- Modify: `crates/neko-core/src/lib.rs`
- Test: `crates/neko-core/src/codex.rs`

- [ ] **Step 1: Write reducer tests before adding the module.**

Add tests that parse a minimal app-server `thread/list` result, retain the
most useful task fields, ignore a malformed entry, and make an approval
disappear after a matching `serverRequest/resolved` notification:

```rust
#[test]
fn thread_list_keeps_local_task_metadata() {
    let snapshot = Snapshot::from_thread_list(&json!({"data": [{
        "id": "thr-1", "name": "Fix ranking", "cwd": "/work/neko",
        "modelProvider": "openai", "updatedAt": 42,
        "status": {"type": "working"}
    }]}));
    assert_eq!(snapshot.tasks[0].id, "thr-1");
    assert_eq!(snapshot.tasks[0].title, "Fix ranking");
    assert_eq!(snapshot.tasks[0].status, TaskStatus::Working);
}

#[test]
fn resolved_notification_removes_only_its_approval() {
    let mut snapshot = Snapshot::with_approval("thr-1", "request-1");
    snapshot.apply_notification(&json!({
        "method": "serverRequest/resolved",
        "params": {"threadId": "thr-1", "requestId": "request-1"}
    }));
    assert!(snapshot.approvals.is_empty());
}
```

- [ ] **Step 2: Run the targeted tests and verify they fail.**

Run: `cargo test -p neko-core codex::tests`

Expected: compilation failure because `neko_core::codex` does not exist.

- [ ] **Step 3: Implement closed task and approval types.**

Create only the stable, UI-relevant shape; preserve unknown app-server fields
by ignoring them rather than mirroring Codex's entire schema:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus { Working, Waiting, Idle, Failed, Unknown }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub provider: Option<String>,
    pub updated_at: i64,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub thread_id: String,
    pub request_id: String,
    pub title: String,
    pub detail: Option<String>,
    pub kind: ApprovalKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub tasks: Vec<Task>,
    pub approvals: Vec<Approval>,
    pub generation: u64,
    pub refreshed_at_unix_ms: i64,
    pub available: bool,
}
```

Implement `Snapshot::from_thread_list`, `apply_notification`, and a
`replace_tasks` method that increments `generation` only when visible task or
approval state changes. Parse JSON through `serde_json::Value`; no filesystem
read of `~/.codex/sessions` is permitted.

- [ ] **Step 4: Run the reducer suite.**

Run: `cargo test -p neko-core codex::tests`

Expected: PASS.

- [ ] **Step 5: Export the module and commit.**

Add `pub mod codex;` to `crates/neko-core/src/lib.rs`.

Run: `cargo fmt --check && cargo test -p neko-core codex::tests`

Commit:

```bash
git add crates/neko-core/src/codex.rs crates/neko-core/src/lib.rs
git commit -m "Add Codex task snapshot models"
```

### Task 2: Build the supervised app-server actor

**Files:**

- Create: `crates/neko-daemon/src/codex.rs`
- Modify: `crates/neko-daemon/src/main.rs`
- Test: `crates/neko-daemon/src/codex.rs`

- [ ] **Step 1: Write a fake-process protocol test.**

Factor JSONL line handling behind a `LineTransport` trait so the test never
starts Codex. Pin initialization order and request correlation:

```rust
#[test]
fn bootstrap_initializes_before_listing_threads() {
    let mut fake = FakeTransport::replying([
        r#"{"id":1,"result":{"codexHome":"/tmp/codex"}}"#,
        r#"{"id":2,"result":{"data":[]}}"#,
    ]);
    bootstrap(&mut fake, &snapshot()).unwrap();
    assert_eq!(fake.sent_methods(), ["initialize", "initialized", "thread/list"]);
}
```

- [ ] **Step 2: Run the test and verify it fails.**

Run: `cargo test -p neko-daemon codex::tests::bootstrap_initializes_before_listing_threads`

Expected: compilation failure because the daemon module is absent.

- [ ] **Step 3: Implement the actor with stdio only.**

Use `Command::new(resolved_codex()).args(["app-server", "--stdio"])` with
piped stdin/stdout and inherited stderr. Send newline-delimited JSON only from
one writer thread. The bootstrap sequence must be exactly:

```rust
call("initialize", json!({
    "clientInfo": {"name": "neko", "title": "neko", "version": env!("CARGO_PKG_VERSION")},
    "capabilities": {"experimentalApi": true}
}))?;
notify("initialized", json!({}))?;
call("thread/list", json!({
    "limit": 100,
    "sortKey": "updated_at",
    "sortDirection": "desc",
    "archived": false
}))?;
```

The reader routes replies by numeric JSON-RPC id and calls
`Snapshot::apply_notification` for notifications. On EOF, set
`available = false`, retain prior task rows, sleep with a bounded 250ms → 5s
backoff, then rebuild the child. Never bind TCP, run `codex app-server daemon
start`, or read a token file.

- [ ] **Step 4: Add failure/restart tests.**

Add tests proving malformed JSON is ignored, child EOF marks the snapshot
unavailable without clearing known tasks, and a subsequent valid thread list
marks it available again.

Run: `cargo test -p neko-daemon codex::tests`

Expected: PASS.

- [ ] **Step 5: Start the actor from the daemon and commit.**

Give `AppState` an `Arc<RwLock<neko_core::codex::Snapshot>>`; construct it in
`AppState::new`, then call `codex::spawn(state.codex.clone(), state.clone())`
from `main.rs` after the existing clipboard thread starts.

Run: `cargo fmt --check && cargo test -p neko-daemon codex::tests`

Commit:

```bash
git add crates/neko-daemon/src/codex.rs crates/neko-daemon/src/main.rs crates/neko-daemon/src/server.rs
git commit -m "Supervise local Codex app server"
```

### Task 3: Expose Codex tasks as fast palette results

**Files:**

- Modify: `crates/neko-core/src/codex.rs`
- Modify: `crates/neko-daemon/src/server.rs`
- Modify: `crates/neko/src/panel.rs`
- Test: `crates/neko-core/src/codex.rs`, `crates/neko-daemon/src/server.rs`, `crates/neko/src/panel.rs`

- [ ] **Step 1: Write provider and tile tests.**

Test empty-query ordering (waiting, working, recent), fuzzy matching against
title/cwd/provider, and that a Codex result reaches the existing tile strip:

```rust
#[test]
fn waiting_then_working_tasks_lead_an_empty_query() {
    let provider = CodexTasksProvider::with_snapshot(snapshot_with([
        task("idle", TaskStatus::Idle, 30),
        task("working", TaskStatus::Working, 20),
        task("waiting", TaskStatus::Waiting, 10),
    ]));
    let ids: Vec<_> = provider.search("", 0).into_iter().map(|c| c.item.id).collect();
    assert_eq!(ids, ["waiting", "working", "idle"]);
}
```

In `panel.rs`, add a test that a `SearchItem { kind: "codex-task", ... }`
with a live or waiting badge is moved into `agent_tiles` and not duplicated in
rows.

- [ ] **Step 2: Run failing tests.**

Run: `cargo test -p neko-core codex::tests::waiting_then_working_tasks_lead_an_empty_query && cargo test -p neko panel::tests::codex_task_is_a_tile`

Expected: FAIL because `CodexTasksProvider` and the tile predicate do not yet
exist.

- [ ] **Step 3: Implement `CodexTasksProvider`.**

Make it read `Arc<RwLock<Snapshot>>` only. Use `kind: "codex-task"`, section
label `Agents`, `Glyph::AgentLive` for working/waiting and `Glyph::Agent` for
idle, `source: Some("Codex")`, and a visible stale suffix when
`snapshot.available` is false. Its primary action enters `codex-task` mode;
its `⌘K` actions are `review` for waiting tasks and `open-in-codex` only when
a documented app-server-supported handoff exists. Do not invent a `codex://`
URL.

Register it immediately before the existing Paseo `AgentsProvider` in
`AppState::with_test_providers`. Update `split_agent_tiles` to treat
`"codex-task"` as a task tile alongside the existing `"agent"` kind.

- [ ] **Step 4: Run fast-path tests.**

Run: `cargo test -p neko-core codex::tests && cargo test -p neko-daemon server::tests && cargo test -p neko panel::tests::codex_task_is_a_tile`

Expected: PASS; no test starts a real Codex child.

- [ ] **Step 5: Commit.**

```bash
git add crates/neko-core/src/codex.rs crates/neko-daemon/src/server.rs crates/neko/src/panel.rs
git commit -m "Show Codex tasks in the summon palette"
```

### Task 4: Add backend-neutral urgent approval cards

**Files:**

- Modify: `crates/neko-core/src/codex.rs`
- Modify: `crates/neko-daemon/src/codex.rs`
- Modify: `crates/neko-daemon/src/server.rs`
- Modify: `crates/neko-protocol/src/lib.rs`
- Modify: `crates/neko/src/main.rs`
- Test: matching unit tests in the files above

- [ ] **Step 1: Write the combined-count and stale-resolution tests.**

```rust
#[test]
fn combined_attention_counts_paseo_and_codex() {
    let state = AttentionCounts { paseo: 2, codex: 3 };
    assert_eq!(state.total(), 5);
}

#[test]
fn resolving_an_old_request_does_not_resolve_a_new_one() {
    let snapshot = snapshot_with_approval("thr", "new-request");
    assert!(snapshot.can_resolve("thr", "new-request"));
    assert!(!snapshot.can_resolve("thr", "old-request"));
}
```

- [ ] **Step 2: Run the tests and verify they fail.**

Run: `cargo test -p neko-daemon combined_attention_counts_paseo_and_codex && cargo test -p neko-core resolving_an_old_request_does_not_resolve_a_new_one`

Expected: FAIL because `AttentionCounts` and guarded approval resolution are
absent.

- [ ] **Step 3: Implement attention aggregation and actions.**

Add `AttentionCounts { paseo: usize, codex: usize }` inside `AppState` behind
one lock. Replace direct `Event::AttentionChanged { count }` broadcasts with
`set_paseo_attention` and `set_codex_attention`; each broadcasts only when
the total changes. Keep the existing client event shape unchanged.

Implement `CodexControl::resolve_approval(thread_id, request_id, decision)`
as a daemon-owned action channel. The control path first checks that the
snapshot still contains the exact pair, then sends the documented app-server
server-request response. A missing pair returns `ProviderError("approval was
already resolved")`; it does not retry or select a newer request.

Define the seam in `neko_core::codex` so providers remain testable without a
real process:

```rust
pub trait CodexControl: Send + Sync {
    fn resolve_approval(&self, thread_id: &str, request_id: &str, approve: bool)
        -> Result<(), ProviderError>;
}
```

- [ ] **Step 4: Render approval actions and verify.**

Use the existing destructive-confirm action menu for decline; show Codex's
title/detail before an approve action. Update the `Event::AttentionChanged`
consumer in `main.rs` only if it assumes every count comes from Paseo.

Run: `cargo test -p neko-core codex::tests && cargo test -p neko-daemon server::tests && cargo test -p neko panel::tests`

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/neko-core/src/codex.rs crates/neko-daemon/src/codex.rs crates/neko-daemon/src/server.rs crates/neko-protocol/src/lib.rs crates/neko/src/main.rs crates/neko/src/panel.rs
git commit -m "Surface Codex approvals as urgent work"
```

### Task 5: Deliver the minimal Codex task quick view

**Files:**

- Modify: `crates/neko-core/src/codex.rs`
- Modify: `crates/neko-daemon/src/codex.rs`
- Modify: `crates/neko-daemon/src/server.rs`
- Modify: `crates/neko/src/modes.rs`
- Modify: `crates/neko/src/panel.rs`
- Test: `crates/neko-core/src/codex.rs`, `crates/neko/src/panel.rs`

- [ ] **Step 1: Write the scoped-mode test.**

The first project must not promise a persistent workspace; its task action
opens a compact read-only recent-activity mode. Test the mode reference keeps
the provider and task id together:

```rust
#[test]
fn codex_task_mode_never_routes_a_paseo_id() {
    let task = TaskRef { backend: "codex".into(), id: "thr-1".into() };
    assert_eq!(task.provider_id(), "codex-task");
    assert_eq!(task.id, "thr-1");
}
```

- [ ] **Step 2: Run the failing test.**

Run: `cargo test -p neko codex_task_mode_never_routes_a_paseo_id`

Expected: FAIL because `TaskRef` is not yet a mode subject.

- [ ] **Step 3: Implement bounded activity retrieval.**

After capability negotiation confirms `experimentalApi`, have the actor ask
`thread/turns/list` with summary items for only the selected task. Cache the
most recent 40 visible entries in the snapshot. If the capability is rejected,
render task metadata plus an honest `History is available in Codex` note; do
not parse a session JSONL fallback.

Extend `ActiveMode::subject` to carry `TaskRef`, and add a mode-only
`CodexTaskProvider` that reads the snapshot. It must never execute a request
on the search path.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRef { pub backend: String, pub id: String }

impl TaskRef {
    pub fn provider_id(&self) -> &'static str {
        match self.backend.as_str() { "codex" => "codex-task", _ => "conversation" }
    }
}
```

- [ ] **Step 4: Run core and GPUI tests.**

Run: `cargo test -p neko-core codex::tests && cargo test -p neko panel::tests`

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/neko-core/src/codex.rs crates/neko-daemon/src/codex.rs crates/neko-daemon/src/server.rs crates/neko/src/modes.rs crates/neko/src/panel.rs
git commit -m "Add compact Codex task quick view"
```

### Task 6: Start a Codex task from an explicit palette location

**Files:**

- Modify: `crates/neko-core/src/commands.rs`
- Modify: `crates/neko-core/src/codex.rs`
- Modify: `crates/neko-daemon/src/codex.rs`
- Modify: `crates/neko-daemon/src/server.rs`
- Modify: `crates/neko/src/modes.rs`
- Modify: `crates/neko/src/panel.rs`
- Test: `crates/neko-core/src/codex.rs`, `crates/neko/src/panel.rs`

- [ ] **Step 1: Write the no-implicit-directory tests.**

```rust
#[test]
fn start_request_requires_a_visible_workspace_path() {
    assert_eq!(StartTask::new("Fix it", None).unwrap_err(), "choose a project first");
    assert_eq!(StartTask::new("", Some(PathBuf::from("/work/neko"))).unwrap_err(), "type the task first");
}

#[test]
fn start_request_uses_the_selected_path_verbatim() {
    let request = StartTask::new("Fix ranking", Some(PathBuf::from("/work/neko"))).unwrap();
    assert_eq!(request.cwd, PathBuf::from("/work/neko"));
}
```

- [ ] **Step 2: Run failing tests.**

Run: `cargo test -p neko-core codex::tests::start_request`

Expected: FAIL because `StartTask` has not been defined.

- [ ] **Step 3: Implement the `New Codex task` mode.**

Add a command row that enters `new-codex-task`. The typed search value is the
task prompt; its rows are explicit project paths from the local Codex snapshot
plus the current client project when available. Every row shows the full path.
The first delivery has no silently-created worktree: its only start operation
encodes the selected `cwd` into `thread/start`, waits for its `thread.id`, and
then starts the first turn with the prompt:

```rust
let thread = call("thread/start", json!({"cwd": start.cwd}))?;
call("turn/start", json!({
    "threadId": thread.id,
    "input": [{"type": "text", "text": start.prompt}]
}))?;
```

Reject an empty prompt, a missing path, or a path that is not a local
directory before sending either request. The full worktree picker belongs to
the persistent workspace project, where it has room to show branch and base.

```rust
pub struct StartTask { pub prompt: String, pub cwd: PathBuf }

impl StartTask {
    pub fn new(prompt: &str, cwd: Option<PathBuf>) -> Result<Self, &'static str> {
        if prompt.trim().is_empty() { return Err("type the task first"); }
        let Some(cwd) = cwd else { return Err("choose a project first"); };
        cwd.is_dir().then_some(Self { prompt: prompt.trim().into(), cwd })
            .ok_or("selected project is unavailable")
    }
}
```

- [ ] **Step 4: Run the mode and actor tests.**

Run: `cargo test -p neko-core codex::tests && cargo test -p neko panel::tests && cargo test -p neko-daemon codex::tests`

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/neko-core/src/commands.rs crates/neko-core/src/codex.rs crates/neko-daemon/src/codex.rs crates/neko-daemon/src/server.rs crates/neko/src/modes.rs crates/neko/src/panel.rs
git commit -m "Start Codex tasks from the summon palette"
```

### Task 7: Verify, document, and record the new boundary

**Files:**

- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/adding-a-provider.md`
- Modify: `AGENTS.md`
- Create: `docs/evidence/codex-quick-attention-loop-report.md`

- [ ] **Step 1: Add documentation assertions.**

Document that the palette reads a warmed, local snapshot; that Codex uses a
stdio child process; that missing/sign-out Codex leaves the launcher usable;
and that task history needs app-server capability support. State clearly that
the persistent Neko workspace and third-party work inbox are the next two
parent projects, not hidden parts of this commit.

- [ ] **Step 2: Run the full automated checks.**

Run:

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```

Expected: all tests pass and Clippy emits no warnings.

- [ ] **Step 3: Run one live local-Codex verification.**

1. Start a Codex task in another local workspace.
2. Launch neko and verify it appears as a `Codex` tile without searching.
3. Trigger a real approval; verify its exact reason appears in **Needs you**.
4. Decline it; verify the row and Dock attention count clear without a
   restart.
5. Stop the app-server child; verify task rows become stale and the launcher,
   clipboard and Paseo rows remain usable.

Record only sanitized titles, counts and timings in the evidence report; do
not capture prompts, repository names, branch names, paths or screenshots.

- [ ] **Step 4: Commit documentation and evidence.**

```bash
git add README.md docs/architecture.md docs/adding-a-provider.md AGENTS.md docs/evidence/codex-quick-attention-loop-report.md
git commit -m "Document Codex quick attention loop"
```

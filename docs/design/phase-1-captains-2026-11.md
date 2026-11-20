# Phase 1, captains: exact build spec (2026-11)

Phase 1 of `docs/plan/capy-parity-2026-11.md` (42 items) mapped onto the code as it is on 2026-11-20. The eight decisions in the brief are fixed.
This file says which files, types and functions change, removes the design choices, and splits the work into six batches. Where a decision
is silent or the code disagrees with it, the smallest choice is written down as **GAP-CHOICE Gn**; all of them are listed at the end so a reviewer
can accept or overrule each. Names were checked against the tree with grep on 2026-11-20. Examples use placeholders (Asha, Acme, ENG-123, miso).

Design target: the v5 mockup (`/tmp/neko-mockups/index.html`, outside the repository; screens Home, Captain, Session, New captain, Settings; light and dark).
Faces: starter set `black-glasses` (black cat, glasses, sage tile), `persian-glasses` (Persian, glasses, sage), `tabby-headphones` (orange tabby, red headphones, teal).
The generated PNGs stay out of the repository until the licence note (`/tmp/neko-design-refs/figma-ai-licence.md`, summary in section 9) is accepted.

## Rulings (2026-11-20, orchestrator): these override the body below

- **R1 Captain chat is not the Home chat engine.** The chat engine is one conversation (`neko_chat_v2`, ~35 call sites); do not refactor it. A captain has its own thread: rows `captain_messages` (captain_id, seq, role, text, at_ms; append-only), and each captain turn is an agent run through the same runner routines use, with a captain prompt built from RULES.md, PLAN.md, DIGEST.md, the last messages and the crew tools on the MCP bridge (sessions it creates carry its `captain_id`). Overrides decision 4 and anything in B2 that touches `neko_chat_v2`.
- **R2 Wake is a new small queue.** A row per pending wake (`captain_wakes`: captain_id, reason, task_id, at_ms), written when one of its sessions finishes a turn, fails or needs the person; the captain tick takes them in order, coalescing several into one turn. The per-task command queue is not reused.
- **R3 No ledger migration.** A captain's cost is the sum of `usage_events` whose `task_id` belongs to its sessions, plus its own turns recorded with `task_id = "captain:<id>"`. No new column. Chat turns stay unrecorded as today.
- **R4 Naming.** Accept G2 (`NeedsYouItem` as a projection over Inbox and goal data).
- **R5 Faces.** Ship the in-code SVG cats until the owner accepts the Figma licence conditions (`/tmp/neko-design-refs/figma-ai-licence.md`); then add the PNGs with an "AI-generated" note.
- **R6** All other GAP-CHOICEs (G1, G3–G28) are accepted as written.

## 0. Findings that change the work (read first)

These are places where the brief's decision does not match the code. Each is handled in the section named.

1. **There is no conversation id in the chat engine (decision 4).** `neko_core::neko_chat` is one conversation: one settings row `neko_chat_v2`
   (`neko_chat.rs:158`, `load`/`save`), one pending turn at a time ("Only one Neko turn runs at a time"), one `Controller::chat_active` slot
   (`workbench.rs`, `send_chat`, `spawn_chat_worker`), and the whole conversation is attached to `Snapshot::conversation` (`neko-core/src/workbench.rs:362`).
   About 35 call sites use `neko_chat::load(db)` and friends. The only existing "conversation ids" are the HTTP chat route (`neko`, `task:<id>`,
   `web.rs::valid_conversation`, `server.rs::chat_log/chat_feed/chat_send`) and the runtime-pin keys (`home:<profile>:<workspace>`, `task:<id>`,
   `runtime_selection::home_id/task_id`). `captain:<id>` therefore needs a real refactor, not a reuse: section D4 (**G10**). This is the largest risk in Phase 1.
2. **The command queue cannot wake a chat (decision 4).** `neko_core::command_queue` is per task (`neko_cmdq_v1:<task_id>`): messages to a running agent.
   There is no wake machinery for a chat. The captain wake is new (section D4, **G11**): a small row plus a polling thread like `start_goals`.
3. **The usage ledger has no captain column and does not record chat turns (decision 6).** `usage_events` (`db.rs:257`) has `task_id` and `kind` only, and the doc
   (`docs/knowledge/usage-ledger.md`) says Home chat turns are not recorded. `Db::migrate` has no `ALTER TABLE` precedent (only `CREATE TABLE IF NOT EXISTS`).
   Section D6 adds a column with a guarded migration and records captain turns (**G14**).
4. **`NeedsYou` already exists** (`neko_protocol::routines::NeedsYou`, the Inbox item, with `seq`, `default`, `expires_ms`, `done`, `reason`). The decision's
   `NeedsYou { question, default, deadline_ms, state }` is therefore a projection with another name (`NeedsYouItem`, **G2**), and the answering path already
   exists (`Request::AnswerNeedsYou`, `inbox_reply::apply`). Section D5.
5. **F2 (crew files per workspace) was never built** (`crew_files.rs` does not exist), so decision 2 replaces it with no migration. Its text, limits and write
   rules carry over (section D2).
6. **Goals already have what C1 needs**: `Blocked { question, default_answer, inbox_seq, asked_ms }`, `Goal::conversation_id`, deadline defaults (F7). The captain
   reuses them; it does not add a second question store.
7. **The allow-list test sits between "Rune first" and "web last" (decision 7).** `apps/neko-rune/tests/allow_list.rs` only lets Rune send what `web/src` sends plus
   `RUNE_EXTRAS`. Rune ships first, so B4 adds the new request names to `RUNE_EXTRAS` with a reason and B6 removes them when the web sends them (**G24**).

## 1. Decision 1: the Captain record

### 1a. Files and names (confirmed)

- NEW `crates/neko-protocol/src/captains.rs`, registered in `crates/neko-protocol/src/lib.rs` beside `pub mod packs;`: the wire types below.
- `crates/neko-protocol/src/lib.rs`: `Request` gets `Captains(captains::CaptainCommand)` (append after `UsageCsv`, same style as `Packs(packs::PackCommand)` at `:456`);
  `Response` gets `Captains(Box<captains::CaptainsReply>)` (append next to `Packs(packs::PackListing)` at `:1256`).
- NEW `crates/neko-core/src/captains.rs`, registered in `crates/neko-core/src/lib.rs`: row storage and the pure rules (validation, ordering, state, digest parsing, wake diff).
  Model: `crates/neko-core/src/command_queue.rs` (rows + an index, `read`/`write` helpers over `Db::get_setting/set_setting`).
- NEW `crates/neko-daemon/src/captains.rs`: `pub fn handle(state: &AppState, command: CaptainCommand) -> Response`, called from
  `crates/neko-daemon/src/server.rs::handle_request` beside `Request::Packs(command) => crate::packs::handle(state, command)` (`:1302`). Register the module in `main.rs`.
- `crates/neko-core/src/workbench.rs::load` (after `snapshot.queued_messages = ...`, `:376`): `snapshot.captains = captains::list_active(db)?; snapshot.captains_rev = captains::rev(db)?;`
  and `encode` (`:566`, beside `compacted.queued_messages.clear()`): clear both. Same "attached for readers, never written into the snapshot row" rule as `queued_messages` and goals.
- `crates/neko-protocol/src/workbench.rs::Snapshot`: `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub captains: Vec<Captain>` and
  `#[serde(default, skip_serializing_if = "is_zero_u64")] pub captains_rev: u64`.

### 1b. Types

```rust
// neko-protocol/src/captains.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(transparent)]
pub struct FaceRef(pub String);                 // a preset id such as "black-glasses"; G3

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ModelPolicy {
    pub preferences: RuntimePreferences,         // the captain's own turns and the default for its sessions; all None = the stored default runtime
    pub per_session: bool,                       // true: the captain may pick a model for each session it starts (G13)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Captain {
    pub id: String,                              // workbench::new_id()
    pub name: String,                            // 1..=24 chars, trimmed, shown lowercase in the sidebar
    pub face: FaceRef,
    pub workspace_id: String,                    // must exist; the captain only works inside it
    pub area: String,                            // short label, <= 24 chars ("Web")
    pub model_policy: ModelPolicy,
    pub daily_limit_micro_usd: Option<u64>,      // None = no limit of its own; Some(n) with 1..=1_000_000_000_000
    pub purpose: String,                         // <= 600 bytes; its first line seeds RULES.md
    pub pinned: bool,
    pub order: i32,
    pub created_at_ms: i64,
    pub archived: bool,
}
```

`CaptainCommand` (the six in the brief, plus the ones the screens need; **G1**):

```rust
pub enum CaptainCommand {
    List { #[serde(default)] include_archived: bool },            // answers Captains(Listing)
    Create { name, face, workspace_id, area, model_policy, daily_limit_micro_usd, purpose },  // answers Listing
    Update { id, name: Option<String>, face: Option<FaceRef>, area: Option<String>, model_policy: Option<ModelPolicy>,
             daily_limit_micro_usd: Option<Option<u64>>, purpose: Option<String> },           // workspace_id is never changed (G16)
    Archive { id },                                                // destructive: confirmed in clients
    Pin { id, pinned: bool },
    Reorder { ids: Vec<String> },                                  // the new order of the active captains, exactly the active set
    // Added for the screens (B1..B3):
    Suggest,                                                       // J7: a proposed first captain (G22)
    Overview { id },                                               // B3: needs, sessions, automations, files, settled, cost
    Summaries,                                                     // B3: one row per captain for the sidebar and Home
    Files { id }, WriteFile { id, kind: CaptainFileKind, text: String },   // B1
    Assign { task_id: String, captain_id: Option<String> },        // B2: Move to / Remove from crew
    Conversation { id },  Send { id, text, message_id: Option<String>, interrupt: bool },  // B2
    Needs { captain_id: Option<String>, include_settled: bool },   // B3
    NeedLater { seq: u64, hours: Option<u32> },  NeedDismiss { seq: u64 },                 // B3
}
```

`CaptainsReply` is an enum of the answers (`Listing(Vec<Captain>)`, `Suggestion`, `Overview(Box<..>)`, `Summaries`, `Files`, `Conversation`, `Needs`, `Done`).
`Response::Captains` is boxed (one big `Response` size lint). Destructive requests are `Archive` and `WriteFile` with empty text (a clear); clients build both only inside `confirm(`.

### 1c. Storage

- One row per captain `neko_captain_v1:<id>` (JSON of `Captain`) and one index row `neko_captain_index_v1` (ordered ids, archived included). Reading a captain
  that is in the index but whose row is gone drops it from the index. No new SQL table: `Db::set_setting` gives identical-write skipping and the `ContentStamp` bookkeeping for free (CLAUDE.md, resource limits).
- `neko_captains_rev_v1`: a counter bumped by `captains::bump(db)` on every captain change, file write, conversation save, need state change and wake. Written with `set_setting_deferred`
  (readers see it at once, disk within 5 s). It is the only thing clients watch (section D4, **G6**): the held `WorkbenchIfChanged` already wakes Rune and the web on a changed digest.
- Everything stored is `#[serde(default)]`; enum variants appended; no migration for existing data (nothing exists yet).

### 1d. Behaviour (exact)

- `Create`: trims `name`; 1 to 24 characters; unique case-insensitively among non-archived captains ("There is already a captain called miso"); `workspace_id` exists in the snapshot
  ("That workspace no longer exists"); `face` matches `[a-z0-9-]{1,32}`; `area` at most 24 characters; `purpose` at most 600 bytes with no NUL; limit in range or `None`;
  at most 16 active captains (**G4**). `order` = highest order + 1. Creates `<data>/captains/<id>/` with `RULES.md` whose text is `purpose.trim()` (empty purpose: no file). Bumps rev. Answers `Listing`.
- `Update`: only the given fields change; `workspace_id` cannot change (the crew, repository checkout and rows are tied to it; **G16**). Renaming keeps uniqueness. Bumps rev.
- `Archive`: sets `archived`, `pinned = false`. From then: no wake turn, no new session by it (`create_task` refuses with "This captain is archived"), hidden from `List` unless `include_archived`.
  Running sessions are not cancelled; their `captain_id` stays. There is no restore in Phase 1 (**G5**); its files stay on disk.
- `Pin`: sets `pinned`. `Reorder`: `ids` must equal the active set; renumbers `order` 0..n in that order; otherwise "The list changed; reload and try again".
- `Suggest` (J7, **G22**): picks the workspace with the most tasks created in the last 30 days (ties: most recently touched), a name from `captains::CAT_NAMES`
  (`miso, tofu, nori, pixel, biscuit, mochi, ...` not used yet), a face from `hash(name) % presets`, `area` = the workspace's name clipped to 24, a one-sentence purpose from the repository's folder name.
  A read; nothing is created. With no workspaces it answers an error "Add a workspace first".

### 1e. Tests (one behaviour per line)

1. `captains::validate_name` rejects empty, 25 characters, only whitespace; accepts 24.
2. A second captain with the same name in other case is refused; an archived one does not block the name.
3. `Create` with a missing workspace is refused and writes no row.
4. A seventeenth active captain is refused with the cap message.
5. `Reorder` with a missing or extra id changes nothing.
6. `Pin` then `Archive` leaves `pinned == false`.
7. `Update` cannot change `workspace_id` (the field is not in the request) and keeps `created_at_ms`.
8. `List` is ordered by `order` and hides archived; `include_archived` shows them last.
9. A row whose index entry is gone is not listed; an index entry with no row is pruned on read.
10. `Snapshot::captains` is attached by `load` and absent from the stored snapshot row (`encode` clears it).
11. An old snapshot JSON without `captains` or `captains_rev` loads unchanged.
12. `Suggest` is deterministic for the same data and picks the busiest workspace.
13. Every field round-trips through serde with defaults (an empty JSON object parses).

### 1f. Risks

- A captain name shown in lowercase in the sidebar but stored as typed: compare uniqueness case-insensitively with Unicode lowercase; the 24 limit counts characters not bytes.
- `Snapshot::captains` grows the digest input: 16 small objects, negligible against the 6.5 MB the digest already hashes.
- `FaceRef` as a free string means an unknown id reaches the UI: clients draw the hash-chosen default for unknown ids (test in each client).

## 2. Decision 2: captain files

### 2a. Files and names

- NEW `crates/neko-core/src/captain_files.rs` (register in `lib.rs`). It is F2's `crew_files.rs` renamed and widened (see `docs/design/super-agent-features-2026-11.md`, F2 (a) to (b)); use that text for the write rules.
  Uses `neko_protocol::support_dir()` and `artifacts::valid_id`. `docs/design/super-agent-features-2026-11.md` F2 gets a one-line "Superseded by `phase-1-captains-2026-11.md` section 2" at its top.
- Types in `neko-protocol/src/captains.rs`: `CaptainFileKind { Rules, Crew, Plan, Digest }`, `pub const MAX_CAPTAIN_FILE: usize = 16 * 1024;`,
  `CaptainFile { kind, text, updated_ms, prev }`, `CaptainFiles { id, files: Vec<CaptainFile> }`.
- Path: `captain_files::dir(id) -> Option<PathBuf>` = `support_dir().join("captains").join(id)` only when `valid_id(id)` and the captain row exists. File names `RULES.md`, `CREW.md`, `PLAN.md`, `DIGEST.md`; previous copy `<name>.prev`.

### 2b. Write and read rules

- `write(id, kind, text)`: at most 16 KiB (`Err("Keep it under 16 KB")`), no NUL (`Err("That text has a character Neko cannot keep")`). Order: create the directory (0700); if the file exists, copy it over `<name>.prev`
  (temp file plus rename); write the new text to a temp file in the same directory and `rename` onto the file. Empty or whitespace-only text removes the file after the `.prev` copy. One process-wide `Mutex` serialises writes.
  Unlike F2 the files are not made read-only: the captain writes `PLAN.md` and `DIGEST.md` itself (the speed-bump argument of F2 does not apply).
- `read(id)`: a missing file reads as empty; a file over 16 KiB (hand-edited) is cut at 16 KiB on a character boundary for prompts and the UI, with a note in the UI.
- `CaptainCommand::WriteFile` is the person's path (UI). The captain's path is a crew tool (D3): `captain_write { file: "plan" | "digest" | "rules" | "crew", text }`. Which files the captain may write (**G8**):
  `plan` and `digest` in any turn; `rules` and `crew` only in a turn the person started (a `Send`, not a wake or an answer), because RULES and CREW are what the person trusts the captain to follow, and text an unattended wake read from a session
  (untrusted) must never be able to rewrite them. The tool checks the lease's turn (`scope.run_id` names `captain:<id>:<turn>`; the turn's `ChatMessage::source` says `wake`/`answer`/none).

### 2c. Injection (exact sites)

| File | Goes into | Site |
| --- | --- | --- |
| `CREW.md` | every ticket run of a session with `captain_id` (scout, builder, reviewer, splitter, supervisor, coordinator, goal turns) | `crates/neko-daemon/src/workbench.rs:1095`, `let rules = neko_core::rulebook::text(&snapshot);` append `captain_files::crew_section(task.captain_id)`; every later `prompt(...)` call uses that variable |
| `CREW.md` | a routine run of a routine with `policy.captain_id` | `crates/neko-daemon/src/routines.rs::run_body` (`:868`), the `orders` string passed to `build_prompt` |
| `RULES.md`, `PLAN.md` (and a pointer to `DIGEST.md`'s text) | the captain's own chat turns | `workbench.rs::converse` (`:1673`), the `instruction` string passed to `neko_chat::prompt_with_checkpoint` (its signature does not change) |
| nothing | the goal judge, the curator, extraction calls, the naming sweep | not crew agents |

- `crew_section(captain_id) -> String`: empty when `CREW.md` is empty; otherwise exactly `"\n\nYour crew's rules\n" + crew.trim_end() + "\n"` (F2's heading, kept so one wording exists).
  Text is pasted into the prompt, never linked, so read-only runs under `sandbox-exec` are not a problem (this settles F2's open risk).
- `captain_section(captain, files) -> String` for the captain's own prompt: purpose line, then "Your rules", "Your plan" with the file texts, "Your crew's rules (what every session you start is told)", the latest `DIGEST.md`, and the tools paragraph (D3). New golden `crates/neko-core/tests/prompts/captain-chat.md`.
- With no `captain_id` and no captain files every existing prompt and golden is byte-identical (`chat-full-power.md`, `chat-read-only.md`, `goal-turns.md`, `routine-*.md`).

### 2d. DIGEST.md format (**G9**)

`DIGEST.md` is the captain's narration. Fixed shape so a client can show it and a parser can read it: three headings in order `## Needs you`, `## New`, `## In progress`; one line per item, `- <sentence>`,
where the sentence may hold markdown links that become chips: `[#214](https://github.com/acme/acme-web/pull/214)` (PR chip) and `[export batching](task:<task_id>)` (session chip).
`captain_files::parse_digest(text) -> DigestSections` ignores anything else, takes at most 8 lines per section. The overview's **Needs you** group is built from the structured items (D5), not from this file;
the file's `## Needs you` section is shown only in the Files tab and used as the captain's own scratch copy. One source of truth for "what is open" stays the Inbox data.

### 2e. Tests

1. An invalid id (`../x`, empty, 65 chars, `a/b`) never creates a directory.
2. 16,384 bytes writes; 16,385 fails with "Keep it under 16 KB" and leaves the old file.
3. A second write keeps the first as `.prev`; a third replaces `.prev` with the second.
4. A NUL byte is refused.
5. Empty text removes the file and keeps `.prev`.
6. `crew_section` is empty with no file and has the exact heading and text otherwise.
7. A builder, reviewer and scout prompt for a session of captain A contain A's CREW text; a session of captain B and a captainless session do not.
8. A routine run prompt for a routine with `policy.captain_id` contains the section after the standing orders.
9. A maximal CREW file still leaves the scout prompt under `256 * 1024` (the existing bound near `workbench.rs:3613`).
10. With no captains, every golden in `crates/neko-core/tests/prompts/` is unchanged (`cargo test -p neko-core --test prompts`).
11. `parse_digest` reads the three sections, caps lines at 8, extracts a PR chip and a session chip, ignores prose outside the headings.
12. `captain_write` for `rules` in a wake turn is refused; in a person turn it writes and keeps `.prev`; `plan` and `digest` write in both.
13. `WriteFile` for a captain that does not exist is refused; `Files` for one with no files answers four empty files.
14. Deleting the captain row (not possible in Phase 1) or archiving leaves the files readable.

### 2f. Risks

- Prompt size: three role prompts plus 16 KB CREW each; the bound test (9) is the guard.
- Captain's own `PLAN.md` and `DIGEST.md` are model-written and re-read next turn: a runaway rewrite loop is bounded by the wake limits in D4 (40 turns a day, 60 s apart).
- A hand-edited file above 16 KB is cut for prompts: the UI shows a note "The file on disk is larger than 16 KB; only the first 16 KB is used."

## 3. Decision 3: sessions carry `captain_id`

### 3a. Files and names

- `crates/neko-protocol/src/workbench.rs:123` `Task`: add `#[serde(default, skip_serializing_if = "Option::is_none")] pub captain_id: Option<String>` after `request`. Rows in `workbench_tasks` stay readable; no migration.
- Struct-literal sites of `Task` that need `captain_id: None` (find them with `grep -rn --include='*.rs' "request: None" crates apps` and keep the ones that are `Task` literals): `crates/neko-core/src/workbench.rs:1895` (`create_task`),
  `neko-core/src/{chief_of_staff.rs:207, display_names.rs, neko_chat.rs:1817, calendar.rs:433, pull_requests.rs:530, triage.rs:114, agent_profiles.rs:421, supervision.rs:252}`,
  `neko-core/tests/decision_context.rs:37`, `neko-daemon/src/workbench.rs:3368` and `:3400`, `neko-daemon/src/workbench/publish.rs:349`, `neko/src/workspace/home.rs:2803`,
  `apps/neko-rune/src/turns.rs`, `apps/neko-rune/src/link/tree_model.rs:415` and the test builders in `server.rs:2076` and `:2279`. The compiler lists the rest.
- `crates/neko-protocol/src/crew.rs`: `CrewRole::Captain { captain_id: String, person_turn: bool }`; `TOOLS` grows to 13 (`captain_write`, `ask_person`; the array length in `pub const TOOLS: [&str; 11]` changes);
  `allows`, `tools`, `label` ("captain <id>") updated; `crates/neko-core/src/mcp_host/bridge.rs:84` area gets the two tool schemas; the Codex `enabled_tools` list is built from `tools()` (check `mcp_host` when editing).
- `crates/neko-daemon/src/workbench/crew.rs`: `origin_of` (`:638`) returns `captain:<id>` for the role (the existing per-origin delegation cap of 8 then counts per captain: `rows::check_cap`);
  `visible()` (`:91`) shows a captain only tasks with its `captain_id`; `crew_create` (`:353`) stamps the id and applies the rules below; `crew_steer`/`crew_cancel` check `crew_task` (own crew only);
  new `crew_captain_write` and `crew_ask_person` (D2, D5).
- `crates/neko-daemon/src/workbench.rs::converse` (`:1673`): the lease role is `Captain{..}` for a captain conversation (see D4).

### 3b. Behaviour (exact)

- **Tools a captain gets**:

  | Tool | Captain, person turn | Captain, wake or answer turn |
  | --- | --- | --- |
  | `list_tasks`, `task_status` | its crew only | its crew only |
  | `create_task` | yes; workspace forced to the captain's; stamps `captain_id`; builds without plan approval (`start_when_planned`, as the full chat) | yes; same stamp; the session waits for plan approval, as a ticket's sub-task does (**G12**) |
  | `steer_task` | own crew | own crew |
  | `cancel_task` | own crew | no |
  | `wait_for` | yes (max 30 min) | no (a wake turn must end quickly) |
  | `checkpoint_diff` | own crew | own crew |
  | `captain_write` | plan, digest, rules, crew | plan, digest |
  | `ask_person` | yes | yes |
  | `schedule`, `save_artifact`, `show_artifact`, `undo_run` | no | no |

  There is no merge tool for any captain: nothing merges unless the person asks (A21, `pull-requests-after-they-open.md`).
- **`create_task` for a captain** also accepts `model: { provider, model, reasoning_effort }` only when `captain.model_policy.per_session`; it is written as a pin for `task:<id>` into the same snapshot
  that saves the task (`snapshot.conversation_runtime["task:<id>"] = RuntimePreferences{..}`, validated with `runtime_selection::validate_preferences`); with `per_session == false` a given `model` is refused ("This captain uses one model for its sessions").
  Without `model` the session pins `captain.model_policy.preferences` when any axis is set, else nothing (the stored default). The session row and header show who chose ("Claude, chosen by miso") from `task_runtimes`/`RuntimeSelection::automatic`.
  `create_task` refuses when the captain is over its daily limit (D6) or archived, and keeps the delegation cap of 8 open sessions per captain.
- `Goal::conversation_id` for a goal a captain attaches is `captain:<id>` (so the Inbox item and the captain row can find it).
- `CaptainCommand::Assign { task_id, captain_id }`: sets or clears `Task::captain_id` through `store::apply` inside one `db.atomic`; refuses when the task is in another workspace than the captain
  ("A captain only looks after its own workspace", **G16**), when the captain is archived, or the task is Cancelled/Deleted. Records the session's current fingerprint so assigning does not wake the captain (D4).
  Appends a task event "Moved to miso" / "Removed from miso's crew".
- Sessions with no captain stay in the Sessions list; nothing about them changes.
- Routines (F17): `RoutinePolicy` gets `#[serde(default)] pub captain_id: Option<String>` (**G18**: inside the existing policy struct, which already has `Default` and `#[serde(default)]`, so the ~15 `Routine` literal sites listed in
  `docs/design/super-agent-features-2026-11.md` section 0 need no edit). `routines::run_body` adds the captain's CREW section, and `usage_ledger::routine_run_event` records `captain_id` (D6).
  Setting it: `SaveRoutine` as today (the editor gets a "Captain" picker in B5); a routine with an archived or unknown captain behaves as captainless.

### 3c. Tests

1. `Task` JSON without `captain_id` loads; a task with `None` serialises without the key.
2. A captain's `create_task` stamps `captain_id`, forces its workspace (a different `workspace_id` argument is refused) and records origin `captain:<id>`.
3. A captain cannot see, steer or cancel a task of another captain or a captainless task (`task_status` answers "No task ... that this run can see").
4. A wake-turn `create_task` leaves the task waiting for approval; a person-turn one sets `start_when_planned`.
5. `model` is refused when `per_session` is false and written as a `task:<id>` pin when true; a model outside the catalog is refused by the same validator as `SetRuntime`.
6. The ninth open session of one captain is refused (delegation cap), while another captain can still create.
7. `Assign` to a captain of another workspace is refused; to the right one sets the id and adds the event; `None` clears it.
8. `Assign` does not enqueue a wake (the fingerprint is recorded first).
9. `CrewRole::Captain.allows` matches the table (a `rstest`-style loop over `TOOLS`), including `cancel_task` only in a person turn.
10. `Goal::conversation_id == "captain:<id>"` for a goal made through a captain's `create_task`.
11. A routine with `policy.captain_id` carries the id through `SaveRoutine` and back through `Routines`.
12. Old routine JSON without the field loads with `None`.

### 3d. Risks

- 13 struct-literal sites; do them in one commit so the tree compiles.
- The tool list length is asserted in places (`TOOLS: [&str; 11]`); `cargo test -p neko-protocol -p neko-daemon` finds them.
- `crew_create` is long already (`:353` to `:420`); put the captain branches in a new file `workbench/captain_crew.rs` called from it, as `run_checkpoints.rs` was split off.

## 4. Decision 4: captain chat, wake

### 4a. The chat engine gets a conversation key (**G10**)

Files: `crates/neko-core/src/neko_chat.rs` (the singleton storage), `crates/neko-daemon/src/workbench.rs` (`chat_active`, `send_chat`, `spawn_chat_worker`, `converse`, `start`, `busy`, `Command::CancelChat`),
`crates/neko-daemon/src/server.rs` (`chat_log`, `chat_feed`, `chat_send` at `:1425` to `:1460`), `crates/neko-daemon/src/web.rs::valid_conversation` (`:561`), `crates/neko-core/src/runtime_selection.rs::validate_scope` (`:48`).

- `pub enum Conv { Home, Captain(String) }` in `neko_chat.rs`; `Conv::setting()` is `neko_chat_v2` for Home and `neko_chat_v2:captain:<id>` for a captain (its own row, same 200-message and 256 KB bounds as Home).
- Every storage function gets an `_in(db, &Conv, ...)` twin holding the code (`load_in`, `save_in`, `begin_scoped_turn_with_id_in`, `begin_next_queued_turn_in`, `prioritize_queued_turn_in`, `discard_queued_in`, `pending_turn_in`, `link_ticket_in`, `recover_interrupted_in`);
  the existing names stay as `_in(.., &Conv::Home, ..)` so the ~35 existing call sites do not change. Functions that take only a turn id (`finish_turn*`, `persist_parts`, `attach_actions`, `settle_action`, `decide_call`) find their conversation with
  `Conv::of_turn(db, turn_id)`: Home first, then the index of captains that have a conversation (`neko_captain_chat_index_v1`). Turn ids are unique across conversations (`new_id()`), so there is no collision.
- `Snapshot::conversation` stays Home only. A captain conversation is read with `CaptainCommand::Conversation { id }` (the last 200 messages as `ChatMessage`, plus `active: Option<turn id>`), refreshed when `captains_rev` changes.
  `persist_parts` bumps the rev only when it writes (it already throttles to "when `emit` says a write is due").
- `ChatMessage` (`neko-protocol/src/workbench.rs:751`) gets `#[serde(default, skip_serializing_if = "Option::is_none")] pub source: Option<String>`: `None` for a person's message, `"wake"` for a crew event, `"answer"` for an answer to a needs-you item.
- Controller: keep `chat_active: Mutex<Option<Active>>` for Home; add `captain_active: Mutex<BTreeMap<String, Active>>` (one turn at a time per captain; different captains and Home run in parallel; **G27**:
  they are not ticket workers, but they respect the machine steward, below). `Active` gains `started_ms: i64` (also used by D6). New `Controller::send_captain(id, text, source, interrupt, message_id)` copies `send_chat`'s shape
  (`begin_..._in`, optional interrupt, launch worker if idle) and `spawn_captain_worker` copies `spawn_chat_worker` with the conversation parameter. `Command::CancelChat { turn_id }` looks in both maps. `busy()` counts both.
  `start()` (`:620`) recovers and restarts queued turns for every captain (`recover_interrupted_in` per conversation).
- `converse` is parameterised by `Conv`: for a captain it reads history from `load_in`, uses the captain's workspace as `chosen`, the captain's `model_policy.preferences` as the conversation pin (`runtime_selection` id `captain:<id>`, accepted by `validate_scope`),
  `instruction = captain_section(..) + the full-power instruction`, lease `format!("captain:{id}:{turn}")` with `mcp.grant_crew(&lease, CrewRole::Captain{..})`, `writable: false` (a captain does not edit files: it directs sessions; **G10**).
  The run directory is the captain's workspace repository. Memory and capability-question shortcuts (`is_capability_question`, `neko_memory::chat_intent`) are skipped for captains.
- Server: `chat_log("captain:<id>")`, `chat_feed`, `chat_send` map to the same functions, so the web gets the existing AI SDK stream route (`/api/chat/captain:<id>/stream`) for free in B6; `valid_conversation` accepts `captain:<valid_id>` (the id passes `artifacts::valid_id`
  and names an existing captain).
- A captain's turn cost is summed in `converse`'s `progress` closure from `USAGE` events (`spend::event_cost_usd`) and recorded at turn end (D6).

### 4b. Wake (**G11**)

- NEW `crates/neko-daemon/src/workbench/captains.rs`: `impl Controller { pub(super) fn start_captains(self: &Arc<Self>) }`, called from `Controller::start` after `start_goals` (`workbench.rs` start, near `self.start_goals();`).
  One thread, `catch_unwind` like the others, a tick every 3 s.
- **Events are found by diffing, not by hooking the many status writers.** `neko_core::captains::fingerprint(task, goal, open_need_seqs, pr) -> Fingerprint { status: String, goal_turns: u32, needs: Vec<u64>, pr: Option<String> }`
  (no tool-event timestamps, so a chatty run never wakes anyone). The last handled fingerprints per captain live in row `neko_captain_seen_v1:<id>` (a map task id to fingerprint), written with `set_setting_quiet` only when something changed.
  `neko_core::captains::diff(old, new) -> Vec<WakeReason { at_ms, task_id, kind }>` with kinds: `TurnEnded` (a rest status from an active one, or `goal_turns` rose), `Failed`, `NeedsPerson` (status `AwaitingApproval`, or a new open item for the task, or a blocked goal),
  `PrChanged` (checks failing or merged or closed), `Deadline` (a need's `deadline_ms` passed unanswered), `Limit` (D6). A session seen for the first time is recorded without waking (so upgrading, `Assign` and the captain's own `create_task` never storm).
- Wake queue: row `neko_captain_wake_v1:<id>` = `{ pending: Vec<WakeReason> (max 20, coalesced by (task, kind), oldest dropped), last_turn_ms, turns_today: (day, n) }`.
- **Launch rule** (all must hold, checked each tick per captain): `pending` non-empty and its oldest entry at least 20 s old (collects a burst); `now - last_turn_ms >= 60 s`; no turn of this captain active; the captain is not archived;
  `turns_today < 40` (**G11**: bounds spend and loops); `machine::latest()` has no `holding` (the steward; `comfort_wait` included) and no `cap` below the running agents; the global daily ceiling is not reached (`spend::today(..).reached()`);
  the captain is not over its own limit (D6). Otherwise the entries wait; nothing is dropped except by the 20 cap.
- **The wake turn** is queued as a message with `source: "wake"` through `send_captain`. Its text (at most 2 KB) is built by `captains::wake_text(reasons, snapshot)`: one line per event ("ENG-123 'Rework the export batching' finished its turn: ReadyForReview; PR #214 checks passed")
  inside the existing untrusted-data wrapper (`neko_chat::UNTRUSTED_DATA_RULE`), because session output is untrusted. The captain's instruction says: read your files; act with your tools; update `PLAN.md` and `DIGEST.md`; call `ask_person` when only the person can decide;
  reply with exactly `QUIET` when there is nothing the person must know. When the reply, trimmed, equals `QUIET`, the wake message and the reply are removed from the stored conversation (a counter row notes the quiet turn for the activity strip); any other reply stays and the wake message shows as one muted line
  ("Heard from 2 sessions") in clients. The person's own messages always run first: a `Send` is prioritised ahead of queued wake messages (`prioritize_queued_turn_in`).
- **Answer turn**: answering a captain-raised need (D5) queues a message with `source: "answer"` and text `Answer to "<question>": <text>`; it is not debounced and not counted against the 40.
- A person's message to a captain is `source: None`, bypasses debounce and caps (cost still counts).

### 4c. Tests

1. `Conv::Home` functions behave exactly as before (the existing `neko_chat` tests pass untouched).
2. A captain conversation row is separate: a message in captain A does not appear in Home or captain B.
3. Two captains and Home can each have one pending turn at once; a second message to one queues behind its own turn only.
4. `Conv::of_turn` finds the conversation of a pending turn in each place.
5. `CancelChat` with a captain turn id stops only that turn.
6. `valid_conversation("captain:<id>")` is true for a valid id and false for `captain:../x`.
7. `validate_scope` accepts `captain:<id>` for an existing captain only.
8. Rebooting the daemon with a pending captain turn recovers it as interrupted, exactly as Home does (`recover_interrupted_in`).
9. `diff` yields `TurnEnded` on Building to ReadyForReview, `Failed` on Failed, `NeedsPerson` on AwaitingApproval, and nothing for new tool events.
10. A session first seen produces no wake; so do `Assign` and the captain's own `create_task`.
11. Three events within 20 s produce one wake turn naming all three; a fourth 30 s later waits until 60 s since the last turn.
12. The 41st wake turn in a local day does not launch; the pending list keeps its entries.
13. A held machine (`holding`), a reached global ceiling or an over-limit captain prevents launch and keeps the entries.
14. A person message jumps ahead of queued wake messages.
15. A reply of exactly `QUIET` removes both messages; `QUIET and more` stays.
16. The wake text wraps session output in the untrusted-data rule and is at most 2 KB.
17. A wake turn cannot write `RULES.md` or `CREW.md` (test 12 of D2); a person turn can.
18. An archived captain's pending wakes are dropped and no turn starts.
19. `captain:<id>` leases carry `CrewRole::Captain` and `create_task` from them links the ticket to the captain turn (`link_ticket_in`).
20. Golden `captain-chat.md` pins the captain prompt (`NEKO_UPDATE_PROMPTS=1 cargo test -p neko-core --test prompts`, review the diff).

### 4d. Risks

- The biggest change of Phase 1: the `_in` twins touch the core of chat storage. Land it as its own commit with no behaviour change for Home (tests 1 and 8 prove it), then add captains on top.
- `converse` is a 400-line function with `snapshot.conversation` assumptions (it finds the pending turn there: `:1706`); read the pending turn through `load_in` for captains instead of `snapshot.conversation`.
- `artifacts::save` and `crew.rs:383/515` use `neko_chat::pending_turn(&db)` to find "the" running reply; with several conversations running that is ambiguous. For leases named `captain:` use the turn in the lease's `run_id`; for `chat:` keep Home.
- A wake loop (captain writes DIGEST, which changes nothing a fingerprint reads) cannot retrigger itself: its own writes are not fingerprint inputs. A captain that keeps creating sessions is bounded by the cap of 8 open and the limit.
- Cost of the polling tick: `store::load` is a copy at the same `ContentStamp` (~1 ms); fingerprints are computed only for captains with sessions.

## 5. Decision 5: Needs-you as a flag (**G2, G7**)

### 5a. What is stored where

The source of truth stays the Inbox: `neko_protocol::routines::NeedsYou` items in routine runs (`seq` global, `all_items_with_routine`), including goal questions
(`routines::raise_goal_item`, pseudo-routine `goals:<workspace>`, link `goal:<id>`). Phase 1 adds:

- **Captain-raised items**: `routines::raise_captain_item(db, captain, title, detail, link, default, expires_ms)` modelled on `raise_goal_item` (`routines.rs:540`), pseudo-routine id `captain:<captain_id>` (a `CAPTAINS_ROUTINE_PREFIX` beside `GOALS_ROUTINE_PREFIX`; the routine is created disabled). Link `captain:<id>` or `captain:<id>:task:<task_id>`.
  Called by the crew tool `ask_person { question, detail, default, deadline_hours, task_id }` (the question is cut with `inbox::one_question`, the default with `inbox::clean_default`; a repeat of an open item with the same question refreshes it, which the Inbox rules already do).
- **Later**: one row `neko_needs_later_v1` = map `seq -> until_ms` (deferred write, bumps rev). An item in it is hidden from Open lists, from the phone push (`phone.rs` selection of items to push), the notifier (`Request::RoutineNotifications`) and the morning digest until `until_ms`; then it is Open again. Later never extends age-out (`inbox` rules keep counting from `touched_ms`).
- **Dismiss**: closes the item with a new reason constant `inbox::DISMISSED = "Dismissed"` (the existing `inbox::close`); Undo is the existing `ReopenNeedsYou`.
- **Settled**: any item with `done == true` closed within `inbox::RECENT_CLOSED_DAYS` (3 days) whose reason is not `Dismissed` (answered by you or on the phone, PR merged or closed, expired, aged out, resolved by a routine, done by you).
  A dismissed item is `Dismissed` and is listed under Settled with that word.

```rust
// neko-protocol/src/captains.rs
pub enum NeedState { Open, Later { until_ms: i64 }, Dismissed, Settled }
pub struct NeedsYouItem {
    pub seq: u64, pub captain_id: Option<String>, pub task_id: Option<String>, pub goal_id: Option<String>,
    pub question: String, pub detail: String, pub default: Option<String>,
    pub deadline_ms: Option<i64>, pub state: NeedState, pub created_ms: i64, pub closed_ms: Option<i64>, pub reason: Option<String>,
    pub link: Option<String>,
}
```

- `captain_id` of an item (`captains::item_captain`): link `captain:<id>`; link `task:<id>` or the goal's ticket -> `Task::captain_id`; link `goal:<id>` with `goal.conversation_id == "captain:<id>"`; the pseudo-routine id of a routine with `policy.captain_id`. Otherwise none.
- `deadline_ms`: for a goal's blocked question `asked_ms + default_after_hours * 3_600_000` when the goal has a deadline default on (not `deadline_off`, `real_default`); otherwise the item's `expires_ms` (its own `deadline_hours` for captain items). **G23**: Phase 1 shows the deadline ("Default at 9am") and, for a captain item, wakes the captain at the deadline (`WakeReason::Deadline`);
  it never answers by itself (the v5 Settings row "Take the default when I don't answer" stays out until a captain-level switch exists; the goal deadline default of F7 is unchanged).
- Needs that come from a session's status, not the Inbox (a plan awaiting approval, a PR ready to merge) are not Inbox items. They show as the session's state "Waiting: approve the plan" (D6/B2) and, for PRs, in the overview's **Ready to merge** group; they have their own buttons (Approve, Merge as today) and no Later/Dismiss (**G7**).

### 5b. Requests and the answering path

- Use default and Answer: the existing `Request::AnswerNeedsYou { seq, text }` (`text: None` takes the default). The web already sends it, so Rune may too without an extra. `inbox_reply::target_of` (`inbox_reply.rs`) gets two targets:
  `Target::Captain { captain_id, question }` (link `captain:<id>…`): the answer queues a captain message `source: "answer"` (D4) and closes the item with `inbox::ANSWERED`; `Target::CaptainLimit { captain_id }` (link `captain-limit:<id>`, D6): default "Hold until tomorrow"; words like "$2" raise that captain's daily limit by that amount (`dollars_in`).
  `action_label` gets the matching labels ("tell miso 'jobs queue'", "hold miso until tomorrow") for the phone reminder.
- Undo of a dismiss or an answer-that-was-wrong: the existing `ReopenNeedsYou`. Done by you: the existing `ClearNeedsYou`.
- New: `CaptainCommand::NeedLater { seq, hours }` (default 4, range 1 to 72, plan C5) and `CaptainCommand::NeedDismiss { seq }` (closes as Dismissed, answers `Done`); both bump the rev and are refused for an item that is closed ("That item is closed or gone", same text as `find_open`).
- Read: `CaptainCommand::Needs { captain_id, include_settled }` answers `Vec<NeedsYouItem>` ordered Open (oldest first), Later (soonest first), then Settled/Dismissed (newest first). `captain_id: None` returns every item, so the Sessions list in the sidebar can mark rows that need the person.

### 5c. Tests

1. A blocked goal's item projects to `NeedsYouItem` with the goal's default and a deadline from `asked_ms + hours`; with `deadline_off` the deadline is `None`.
2. `ask_person` raises one item under `captain:<id>`; the same question again refreshes it (no second item).
3. `item_captain` resolves from a `captain:` link, a `task:` link, a goal's ticket, a goal's `conversation_id`, and a captain routine; and is `None` otherwise.
4. `NeedLater` hides the item from Open, the phone selection, `RoutineNotifications` and the morning digest; after `until_ms` it is Open again.
5. `NeedDismiss` closes with reason "Dismissed"; `ReopenNeedsYou` brings it back open and kept.
6. A closed item is Settled for 3 days and then absent; a dismissed one reads `Dismissed`.
7. `AnswerNeedsYou` on a captain item queues one `answer` wake carrying the text, closes the item as Answered, and an immediate second answer is "closed or gone".
8. `AnswerNeedsYou` with no text takes the default; with no default it is refused ("No default to send" style message).
9. A limit item's default holds the captain; "$2" raises the limit by 2,000,000 micro-dollars.
10. A deadline passing on an open captain item produces one `Deadline` wake and the item stays open.
11. `Needs { captain_id: None }` lists items of every captain and of none.
12. Existing `inbox_reply` tests (goal, extend, spend, task, improve targets) pass unchanged.

### 5d. Risks

- Several clients may answer at once: `inbox_reply::apply` already serialises with a static mutex; the new targets go through it.
- The phone and notifier filter must not hide a Later item from the Inbox page's own "Open" count in a way that disagrees with the sidebar badge: both read `captains::open_count` (one function).

## 6. Decision 6: cost from the ledger (**G14, G15**)

### 6a. Files and names

- `crates/neko-core/src/db.rs::migrate` (`:231`): after the `CREATE TABLE` batch call `ensure_column(conn, "usage_events", "captain_id", "TEXT NOT NULL DEFAULT ''")`, which reads `PRAGMA table_info(usage_events)` and runs
  `ALTER TABLE usage_events ADD COLUMN ...` only when the column is missing; also `CREATE INDEX IF NOT EXISTS idx_usage_events_captain ON usage_events (captain_id, at_ms)`. A new database gets the column by the helper too (the `CREATE TABLE` text is left unchanged so old and new share one path).
  `record_usage_event` (`:477`) and `usage_events_where` (`:496`) add the column.
- `crates/neko-core/src/usage_ledger.rs`: `UsageEvent.captain_id: String`; `ticket_run_event(.., captain_id)`; `record_ticket_run(.., captain_id: &str)` called at `workbench.rs:901` with `claim.task.captain_id`; `routine_run_event` takes the routine's `policy.captain_id`;
  NEW `record_captain_turn(db, captain_id, turn_id, started_ms, now_ms, runtime, cost_micro) ` writing `kind = "captain"`, `run_key = "captain:<turn id>"`, `task_id = "captain:<id>"`, `source` from `estimate_source`.
  NEW `Db::usage_for_captain_since(captain_id, since_ms) -> rusqlite::Result<Vec<UsageEvent>>`, `captains::cost_today`, `captains::cost_history` (reuses `by_day`).
- `crates/neko-daemon/src/workbench.rs`: `Active { task_id, cancelled, started_ms }`; `Controller::in_flight_micro(&Snapshot) -> BTreeMap<task_id, u64>` = for each active worker, `spend::micro_usd(spend::task_usd_since(task, started_ms))`.
- The CSV (`usage_ledger::to_csv`) and the History page keep their columns; add `captain_id` as the last CSV column (existing readers ignore trailing columns; the doc line in `usage-ledger.md` is updated).

### 6b. Behaviour

- **Session cost** (header, rows): `usage_ledger::sum_for_task(db, task_id)` (finished runs) plus the in-flight run's events since its claim start. Shown "$0.62" only when `Show cost` is on (client setting, **G21**); a session whose runs reported nothing shows "estimated" instead of a number (the ledger never invents numbers).
- **Captain cost today** = ledger rows with this `captain_id` since the local day start (`spend::day_start_local`) + in-flight cost of its active sessions + its own in-flight turn. Per-day history reuses `usage_ledger::by_day` over the captain's rows.
- **Daily limit** (A20): a refresh every 10 s in the captains thread computes `over: BTreeSet<captain_id>` (cost today >= `daily_limit_micro_usd`) into `Controller::over_limit` (memory only). Effects while over:
  (1) `tick_with` (`workbench.rs:727`) removes the captain's queued sessions from `positions` through a new `admission::hold_over_limit_captains(&snapshot, &over, positions)` (a pure function like `cap_writable_per_repository`; running sessions are not cancelled, they finish their turn);
  (2) `create_task` and wake launches refuse (D3, D4);
  (3) one Inbox item per captain per local day: "miso reached $5 today. Pause its crew until tomorrow?" with default "Hold until tomorrow" (link `captain-limit:<id>`, `Target::CaptainLimit`, D5), recorded in row `neko_captain_limit_v1:<id>` = `{ day, seq }` so it is raised once.
  At local midnight `over` empties by itself; "Hold until tomorrow" therefore only closes the item. This is on top of the global ceiling (`Snapshot::daily_budget_cents`, `spend::today`): a captain below its limit still obeys the global one.
- Sessions of a captain-less workspace are unaffected.

### 6c. Tests

1. Opening an old database adds the column once; a second open is a no-op; old rows read `captain_id == ''`.
2. A ticket run for a session with a captain records that id; one without records `''`.
3. A routine run records its policy's captain id.
4. `record_captain_turn` is idempotent on `run_key` (INSERT OR IGNORE) and records `estimated` when no `USAGE` event arrived.
5. `cost_today` sums only today's rows of that captain plus in-flight; a row of yesterday is excluded (use a fixed timezone as `usage_ledger` tests do).
6. `by_day` for a captain returns every day of the range with zeros.
7. `hold_over_limit_captains` removes only the over-limit captain's queued sessions and keeps order of the rest.
8. A running session of an over-limit captain is not cancelled.
9. The limit item is raised once per day per captain and again the next day.
10. `usage_ledger` CSV has `captain_id` last and the existing CSV tests are updated, not loosened.
11. `Captains(Summaries)` returns the same `cost_today_micro` as `cost_today` and `None` limit shows no limit.

### 6d. Risks

- The first `ALTER TABLE` on a live database: it is a metadata-only change in SQLite and runs under the existing migrate; test on a copy of a real data directory before B3 ships.
- `Controller::in_flight_micro` reads events that can be shortened or evicted under pressure (`workbench::save` doc); it only adds the running claim's events (new, not evicted), so the error is bounded.
- Home chat remains unrecorded; the Usage page still says "agent runs". Captain turns are recorded, so captain cost can exceed the page's per-day total of tickets and routines by design (note in the Usage doc).

## 7. Decision 7: UI order, and where new requests must be added

1. Daemon and protocol first (B1 to B3); every batch ends with `scripts/check.sh` green.
2. Rune (B4, B5), the installed app, built to v5. The Rune rules (`apps/neko-rune/tests/allow_list.rs`):
   - New request names appear **only** in `apps/neko-rune/src/link/` (`write.rs` `Call`, `reply.rs` `Reply`, a new `link/captains.rs` for the data types); no other module may name `Request::` or `CaptainCommand::` (`only_the_link_module_names_daemon_requests`).
   - Edit `PREFIXES` (`[&str; 13]` becomes 14) and the "sneaky" list (`"CaptainCommand as"`) and `SUBCOMMAND_PREFIXES` (`[&str; 11]` becomes 12) to include `CaptainCommand`.
   - Because the web does not send them yet, add each used name to `RUNE_EXTRAS` with the reason "captains: the web sends these in B6": `Captains`, `Summaries`, `Overview`, `Files`, `WriteFile`, `Assign`, `Conversation`, `Send`, `Needs`, `NeedLater`, `NeedDismiss`, `Suggest`, `Create`/`Update`/`Archive`/`Pin`/`Reorder`/`List` as far as not already present (`Create` is there for goals). **G24**: B6 deletes the entries the web now sends (the test that complains about unused extras only fires when Rune stops sending, so the list must be cleaned by hand).
   - Destructive: `Archive` and a clearing `WriteFile` are added to `DESTRUCTIVE` (`("Archive", "Call::ArchiveCaptain")`, `("WriteFile", "Call::ClearCaptainFile")`) and built only inside `confirm(` in `task_actions.rs`.
   - Already allowed because the web sends them: `AnswerNeedsYou`, `ClearNeedsYou`, `ReopenNeedsYou`, `ApproveTask`, `CancelTask` (through `Call::Stop`), `SaveRoutine`, `Routines`, `SetDailyBudget` (check the name with `grep SetDailyBudget web/src`).
3. Web (B6): the web sends `request({ Captains: { ... } })` and `command(...)` call sites in `web/src/` (the scan reads the first key of `request(`), so every captain request is spelled in `web/src` and the mirror test passes without an entry.

## 8. Decision 8: faces (**G3, G20**)

- Ship an in-code cat set: six presets (the three starters `black-glasses`, `persian-glasses`, `tabby-headphones`, plus `grey-scarf`, `cream-bowtie`, `calico-welding`) on flat tiles (sage, teal, sand, rose, sky, lilac), drawn as vector in the client, never from files.
  Rune: `apps/neko-rune/src/faces.rs` using `rune::Icon { name, view, body }` bodies with explicit fills (the Neko mark is already a filled icon, `icons.rs:53`); sizes 16, 24, 44, 80 with hand-tuned variants (K3: below 20 px no glasses detail and no state glyph).
  Web: `web/src/captains/Face.tsx` with the same SVG strings from one shared source file `web/src/captains/faces.ts` (Rune copies the path data; a test compares the preset ids of the two so they cannot drift).
- Check before B4 starts: that Rune's icon rasteriser keeps non-`currentColor` fills and multiple shapes. If it does not, draw faces as `rune::image` of SVG bytes (as `samples::svg()` does in `scenes_panel.rs`), same art, different carrier.
- Default face for a new captain: `hash(name) % 6` (`captains::default_face(name)`, shared by `Suggest`).
- Generated PNGs: not committed. After the licence conditions in section 9 are accepted they may be added as optional extra presets under `assets/faces/generated/`, never replacing the SVG fallback.

## 9. Licence note (summary; full text with citations in `/tmp/neko-design-refs/figma-ai-licence.md`)

Figma's AI Terms (effective 2026-10-30) give the customer "all right, title, and interest in Customer's Input and Output"; the Terms of Service (2026-03-11) put no limit on publishing, and no attribution clause exists. Figma gives no warranty, the copyright indemnity covers Organization and Enterprise plans only,
the image model behind `generate_image` is not named, and the Acceptable Use Policy (2026-10-07) forbids passing AI output off as human-made or stripping its metadata. Answer: Neko may commit and ship the PNGs if the account is the owner's own, metadata is kept and the repository says they are AI-generated, the files are not licensed as MIT-original work, and each image is checked for lookalikes. Until the owner accepts, ship the SVG cats.

## 10. Phase 1 items to the batches

| Batch | Items |
| --- | --- |
| B1 | A1, A8 (files), J7 (the `Suggest` read), the data half of A3 |
| B2 | A10, A17, A18, A19, A27 (daemon half), F17 (field), the data half of A28 |
| B3 | C1, C2 (parse), C4, C5, C8, C9, A12, A20, G1, B2 (state), E19 (data) |
| B4 | A3, A4, A7, A13, K3, K6, K7, K8, K10, A27 (view), B2 (glyphs), E19 (group), A12 (badge) |
| B5 | A2, A28, C3, C15, B35, B36, B37, G1 (header, footer), J7, J8, J11, K1, K2, K4, K5, F17 (picker) |
| B6 | the same screens on the web, `identity.sh` pairs, docs index |

## 11. Batches

Rules for every batch: `scripts/check.sh` (clippy always-a-defect lints, Rust tests, Swift tests, ~70 s) and `bash scripts/privacy-check.sh` clean; Rune batches also run `apps/neko-rune/scripts/check.sh`; web batches run `pnpm -C web typecheck`.
A batch is one or more commits on `main` (the project commits on main, no PRs); each commit leaves the tree compiling. Each batch adds its section to one new doc, `docs/knowledge/captains.md`, and B1 adds its index line to `CLAUDE.md`.
Estimates are agent-days on the plan's scale (S 0.5, M 1.5, L 3); they sum to about 41, below the plan's 60 because the settled items are counted once.

### B1. Captain records, files, requests (about 5 days)

- **Scope:** sections 1 and 2 (files and write rules; no injection yet). `Request::Captains` with `List`, `Create`, `Update`, `Archive`, `Pin`, `Reorder`, `Suggest`, `Files`, `WriteFile`.
- **Files touched:** `neko-protocol/src/{captains.rs (new), lib.rs, workbench.rs (Snapshot fields)}`, `neko-core/src/{captains.rs (new), captain_files.rs (new), workbench.rs (attach and encode), lib.rs}`, `neko-daemon/src/{captains.rs (new), server.rs (one arm), main.rs}`,
  `docs/knowledge/captains.md` (new), `CLAUDE.md` (one index line), `docs/design/super-agent-features-2026-11.md` (F2 superseded note).
- **Done when:** tests 1.1 to 1.13 and 2.1 to 2.5, 2.11, 2.13, 2.14 pass; `daemon --request '{"Captains":{"Create":{...}}}'` creates a captain and `Files` lists four files; an old data directory loads with no change to the stored snapshot row (compare the row bytes before and after a load).
- **Gate:** `scripts/check.sh`; `bash scripts/privacy-check.sh`.

### B2. Sessions carry the captain; captain chat; wake (about 12 days, the riskiest batch)

- **Scope:** sections 3 and 4 and the injection of section 2c. Split into three commits in this order: (a) `Conv` refactor with no behaviour change, (b) `captain_id`, `CrewRole::Captain`, tools, `Assign`, `RoutinePolicy::captain_id`, CREW injection, (c) captain conversation, `Send`, `Conversation`, wake thread, `QUIET` handling, the captain golden prompt.
- **Files touched:** `neko-core/src/{neko_chat.rs, runtime_selection.rs, captains.rs, captain_files.rs, mcp_host/bridge.rs}`, `neko-core/tests/prompts.rs` and `tests/prompts/captain-chat.md`, `neko-protocol/src/{workbench.rs (Task, ChatMessage, RoutinePolicy in routines.rs), crew.rs}`,
  `neko-daemon/src/{workbench.rs, workbench/crew.rs, workbench/captain_crew.rs (new), workbench/captains.rs (new), server.rs (chat_*), web.rs (valid_conversation), routines.rs, captains.rs}`, the 13 `Task` literal sites listed in 3a.
- **Done when:** tests 3.1 to 3.12 and 4.1 to 4.20 and 2.6 to 2.10, 2.12 pass; with no captains every golden is unchanged; in a dev data directory a captain created in B1 starts two sessions from a `Send` ("split this into two sessions") and a finished session wakes it within about 25 s, with one reply or a silent `QUIET`;
  the Home chat and a captain chat run at the same time without blocking each other (manual check with the scripted chat harness `web_harness` or `SCRIPTED_CHAT`).
- **Gate:** `scripts/check.sh`; `bash scripts/privacy-check.sh`; `NEKO_UPDATE_PROMPTS=1 cargo test -p neko-core --test prompts` reviewed (only `captain-chat.md` is new).

### B3. Needs-you flag, actions, settled, state, cost, limit (about 9 days)

- **Scope:** sections 5 and 6, plus `Summaries` and `Overview`: `CaptainSummary { captain, state (Working/Waiting/Failed/Idle), open_needs, last_line, cost_today_micro, working, ready_to_merge }` and `CaptainOverview { needs, sessions, ready_to_merge, automations, files, settled, cost_today_micro, limit }`,
  with `SessionRow { task_id, title, state, step: Option<(u32,u32)>, model, cost_micro, needs_open, pr: Option<PrChip>, age_ms }` and `captains::session_state(task, unseen, needs, now)` (Working, Waiting(reason), Failed, Idle(since), Settled; **G19**: Settled = finished (ReadyForReview with no open PR, Completed, Cancelled, or Failed and seen), read (not in `UnseenTasks`), idle at least 1 h, no open need, no open PR waiting; any later activity returns it).
  Steps (**G17**): `captains::steps(task)` parses checklist lines (`- [x]`, `- [ ]`) from `task.plan`; with none, no step line. Check against two real plans before building the Rune line.
- **Files touched:** `neko-core/src/{db.rs (migration), usage_ledger.rs, captains.rs, inbox.rs (DISMISSED), spend.rs (helper)}`, `neko-protocol/src/{captains.rs, ledger.rs (CSV column doc)}`, `neko-daemon/src/{inbox_reply.rs (targets, labels), routines.rs (raise_captain_item), phone.rs and morning_digest.rs and the notifier filter (Later), workbench.rs (Active.started_ms, tick_with), workbench/admission.rs (hold_over_limit_captains), workbench/captains.rs (limit refresh, deadline wake), captains.rs}`, `docs/knowledge/usage-ledger.md`, `docs/knowledge/phone-push.md` (Later).
- **Done when:** tests 5.1 to 5.12 and 6.1 to 6.11 pass; on a copy of a real data directory the migration runs once and the Usage page still renders; a captain raising `ask_person` shows one Inbox item, `AnswerNeedsYou` wakes it with the answer; an over-limit captain's queued session stays queued while another captain's starts.
- **Gate:** `scripts/check.sh`; `bash scripts/privacy-check.sh`.

### B4. Rune: faces, sidebar, captain view, overview (about 9 days)

- **Scope:** A3, A4, A7, A12, A13, K3, K6, K7, K8, K10, E19, the view half of A27, B2 glyphs. Start only after (1) the v5 screens are confirmed as the target by the owner, (2) the faces carrier check of section 8 passes.
- **Files touched (apps/neko-rune):** `src/faces.rs` (new: six presets, tiles, sizes, default face), `src/captains.rs` (new: pure state, groups, Needs rows, tested like `groups.rs`), `src/link/{captains.rs (new), write.rs (Calls), reply.rs (Reply::Captains..), mod.rs}`,
  `src/view/{sessions.rs (the column: Home, Search, Calendar, Captains with +, Sessions, Automations, footer), captain.rs (new: header, narrated chat, tabs Overview/Files/Automations), captain_chat.rs (new), needs.rs (new, shared rows), ui.rs (group bar K8, chips K10, state glyph K6)}`,
  `src/{app.rs (captain selection, captain data and its refetch on `captains_rev`), views.rs (rename Routines to Automations in `title()` only), theme.rs (K7: accent only for needs-you text, counts and the face dot), motion.rs (K6 pulse), scenes*.rs (new scenes), task_actions.rs (confirm Archive and clear file)}`,
  `tests/{allow_list.rs (section 7), captains.rs (new), sidebar.rs, structure.rs (new scenes enter the six rules)}`.
- **Behaviour to pin in tests:** the sidebar lists active captains pinned first then by `order`, each with face, lowercase name, age and the open question in the accent colour; an accent dot on the face and a count appear exactly when `open_needs > 0` (and nowhere else uses the accent, K7);
  the face glyph is stepped dots while Working, slower while Waiting, coral while Failed and still under Reduce Motion; hidden below 20 px; a captain with no sessions shows no group bars; Needs you rows offer Use the default, Answer..., Later, Dismiss (keys 1 to 3 pick the answer options, J/K move) and send `AnswerNeedsYou`, `NeedLater`, `NeedDismiss`;
  Dismiss shows an Undo toast that sends `ReopenNeedsYou`; the overview groups are Needs you, Working, Ready to merge, Automations, Files, Settled in that order, empty groups take no space; "Move to" in a session's menu sends `Assign`; unknown face ids draw the default.
- **Done when:** the new tests pass; the headless render scenes for the sidebar (with 4 captains), Captain overview and Captain chat match the v5 mockup at 1440x900 in light and dark by a side-by-side picture review; `apps/neko-rune/scripts/check.sh` green; memory with and without captains is within 5% of today (the P3 measure method).
- **Gate:** `apps/neko-rune/scripts/check.sh`; `cd apps/neko-rune && cargo test`; `bash scripts/privacy-check.sh`. `scripts/check.sh` still runs (the daemon did not change).

### B5. Rune: Home, session, new captain, settings, the five craft items (about 10 days)

- **Scope:** A2, A28, C3, C15, B35, B36, B37, G1 (header, footer), J7, J8, J11, F17 (picker), and the five craft items K1 to K5.
- **Files touched (apps/neko-rune):** `src/view/{home.rs (greeting with counts, captain tiles with last line, Needs you, Settled today), compose.rs and composer.rs (the Home composer routes to a captain, "Let Neko choose"), session.rs (header: crumb captain / title, step, cost, Stop, ..., "From miso" line; turn summaries; step list), new_captain.rs (new dialog: Face, Name, Looks after, Model, Daily spend limit, What is it for?, Cancel / Create captain), settings.rs and settings_ai.rs (Captains page, On its own, Spending: Daily limit via `SetDailyBudget`, Show cost client-side), onboarding.rs (last step offers `Suggest`), routines.rs (Captain picker in the editor)}`,
  `src/{account.rs (cost today), icons.rs (K1), layout.rs and theme.rs (K2 grid audit), shortcuts.rs and palette.rs (K5), view/ui.rs (K4 hover preview and right-click menu on every row)}`, `tests/{captains.rs, polish.rs, structure.rs, session.rs, setup.rs}`.
- **The five craft items, as testable rules:** K1 one 16 px icon set with filled variants for active (the test at the foot of `icons.rs` that bans other stroke widths covers new icons); K2 every region on a 4/8 rhythm (`structure.rs` rule plus a lowered ceiling on raw numbers);
  K3 faces at 16, 24, 44, 80 (a size test per preset); K4 hover preview card (latest line, cost, state) after 400 ms and a right-click menu on session, captain and need rows; K5 `⌘K` everywhere, `J/K` in every list, `1` to `3` for overview tabs and for answer options, listed in the shortcut sheet.
- **Done when:** the new captain dialog creates a captain with the six fields and the first RULES line equals the purpose; the Home composer sends to the chosen captain's `Send`; the session header shows captain, step "4 of 7", cost and Stop; the account footer shows "$6.20 today" from `Summaries`; first run offers "Let Neko choose a captain"; the v5 screens Home, Session, New captain and Settings match by picture review in light and dark.
- **Gate:** `apps/neko-rune/scripts/check.sh`; `cd apps/neko-rune && cargo test`; `bash scripts/privacy-check.sh`; `scripts/check.sh`.

### B6. Web parity (about 6 days)

- **Scope:** the same screens in `web/` (ADR 0001 keeps the web UI the first-class interface until Rune reaches parity; ADR 0002 builds Rune beside it), the web side of the allow-list mirror, the identity measure, docs.
- **Files touched (web/src):** `shell/AppSidebar.tsx` and `AppSidebar.parts.tsx` (captains list), `pages/Home.tsx`, new `pages/Captain.tsx` and `captains/{Face.tsx, faces.ts, Overview.tsx, Needs.tsx, Files.tsx, NewCaptain.tsx, api.ts}`, `work/TaskDetail.tsx` (header, step line), `work/Thread.tsx` (captain chat uses the existing AI SDK chat on `captain:<id>`),
  `pages/setup/*` (Captains page in Settings), `router.ts` and `shell/tabs.ts` and `shell/commands.ts` (palette entries), `shell/attention.ts` (face dot). `apps/neko-rune/tests/allow_list.rs`: delete the `RUNE_EXTRAS` entries the web now sends. Docs: `docs/knowledge/captains.md` completed, `docs/architecture.md` (the captain record and the request), `CLAUDE.md` index line checked.
- **Done when:** `pnpm -C web typecheck` and `pnpm -C web build` pass; `scripts/verify/*` style harness scenes exist for the captain page (`scripts/verify/captains.mjs`, driving a seeded data directory like `scripts/verify/crew.mjs`); `apps/neko-rune/scripts/identity.sh` pairs for Home, Captain and Session are added to `identity-pairs.json` and the measure is no worse than the current pairs; the allow-list test passes with the shortened `RUNE_EXTRAS`.
- **Gate:** `pnpm -C web typecheck`; `scripts/check.sh`; `apps/neko-rune/scripts/check.sh`; `bash scripts/privacy-check.sh`.

## 12. Cross-cutting risks and how a batch must handle them

- **Storage budget (CLAUDE.md):** captains, wake, seen, later, limit and rev rows are all small; the `captains_rev` write is deferred; `neko_captain_seen_v1` and wake rows use `set_setting_quiet`. A reviewer measures the daemon's disk writes for five idle minutes with a captain present
  (`proc_pid_rusage`, `ri_diskio_byteswritten`, as the resource-limits section prescribes) and must see no per-tick write.
- **Idle CPU:** the captains thread uses the cached `store::load` (same `ContentStamp`); no per-tick database scan beyond one indexed query every 10 s per captain.
- **Prompt injection through session output:** the wake text is wrapped as untrusted; `RULES.md`/`CREW.md` are not writable from a wake; no captain has a merge tool; a captain cannot see another captain's sessions.
- **Public repository:** no real names, repositories or hosts in code, tests, docs or the data the tests seed; placeholders only (Asha, Acme, acme-web, ENG-123, miso, tofu, nori).
- **Hard rules stay:** nothing merges unless the person asks (A21); `Archive` and clearing a file are confirmed; defaults are never taken by themselves in Phase 1.

## 13. GAP-CHOICE list (for review)

- **G1** `CaptainCommand` has more variants than the six in the brief (`Suggest`, `Overview`, `Summaries`, `Files`, `WriteFile`, `Assign`, `Conversation`, `Send`, `Needs`, `NeedLater`, `NeedDismiss`); the six stay as written. They are one enum under one `Request::Captains`.
- **G2** The decision's `NeedsYou` type is named `NeedsYouItem` because `neko_protocol::routines::NeedsYou` (the Inbox item) already exists; it is a projection of Inbox and goal data, not a second store.
- **G3** `FaceRef` is a free string `[a-z0-9-]{1,32}`, not a daemon-validated enum, so new faces ship without a daemon release; clients draw the hash-chosen default for an unknown id.
- **G4** At most 16 active captains; names unique, case-insensitive.
- **G5** No restore of an archived captain in Phase 1 (rows and files are kept).
- **G6** Captains are settings rows (`neko_captain_v1:<id>`, index row) attached to `Snapshot` for readers; a single deferred counter `captains_rev` makes the held `WorkbenchIfChanged` wake clients when captain data (files, conversation, needs) changes. No new SQL table, no new push event.
- **G7** Needs-you state: the Inbox item is the record; Later is a small row; Dismiss is a close reason ("Dismissed"); Settled is any closed item within 3 days. A plan awaiting approval or a PR ready to merge is a session state / group, not an Inbox item, and has no Later or Dismiss.
- **G8** Captain-writable files: `PLAN.md` and `DIGEST.md` in any turn; `RULES.md` and `CREW.md` only in a turn the person started. The person edits all four in the UI.
- **G9** `DIGEST.md` has a fixed three-heading format with markdown links as chips; the overview's Needs you comes from structured items, not the file.
- **G10** The chat engine gets a `Conv` key with `_in` twins (Home keeps its names), a per-captain row `neko_chat_v2:captain:<id>`, a per-captain active turn map, and `Snapshot::conversation` stays Home only. A captain run is not writable (it directs sessions, it does not edit files).
- **G11** Wake by polling fingerprints every 3 s (no hooks in the status writers); debounce 20 s, at least 60 s between turns, 40 turns per captain per local day, pending list of 20; wake and answer messages are flagged `ChatMessage::source`; a reply of exactly `QUIET` removes the exchange.
- **G12** A session a captain starts in an unattended (wake or answer) turn waits for plan approval; in a turn the person started it builds without approval, as the full chat does.
- **G13** `ModelPolicy { preferences: RuntimePreferences, per_session: bool }`; a per-session model is a `task:<id>` runtime pin validated like `SetRuntime`; the captain's own model is the `captain:<id>` pin.
- **G14** Cost: a `captain_id` column on `usage_events` (guarded `ALTER TABLE`, the first in this codebase), captain turns recorded as `kind = "captain"`, in-flight cost added from the running claim's events (`Active.started_ms`). Home chat stays unrecorded.
- **G15** At the daily limit queued sessions of that captain are held (running ones finish), wakes and `create_task` refuse, one Inbox item per day offers "Hold until tomorrow" or a raise ("$2"); the global ceiling still applies on top.
- **G16** `Assign` and `create_task` keep a captain to its own workspace; `workspace_id` of a captain cannot be changed.
- **G17** The step list is parsed from checklist lines in `task.plan`; no steps line if there are none. Needs a check against real plans before B5.
- **G18** `captain_id` for routines lives inside `RoutinePolicy` to avoid the ~15 `Routine` literal sites.
- **G19** Settled sessions: finished, read, idle at least 1 h, no open need, no open PR waiting; any later activity returns them.
- **G20** Faces are in-code vectors (six presets); the carrier (Rune `Icon` with fixed fills, else `rune::image` of SVG bytes) is verified at the start of B4. Generated PNGs wait for the licence conditions.
- **G21** "Show cost" is a client-local preference (Rune saved layout, web localStorage), not a daemon setting.
- **G22** `Suggest` picks the busiest workspace of the last 30 days and a name and face deterministically; it creates nothing.
- **G23** Phase 1 shows a need's deadline and wakes the captain at it, but takes no default by itself; the v5 Settings rows "Record first" (needs F1, not built) and "Take the default when I don't answer" are not shown until their features exist.
- **G24** Until B6, new request names sit in `RUNE_EXTRAS` with a reason; B6 deletes the ones the web sends.
- **G25** The captain's runtime-pin id is `captain:<id>`; `validate_scope` accepts it for an existing captain.
- **G26** Later defaults to 4 hours (range 1 to 72); no "tomorrow morning" option in Phase 1.
- **G27** Captain turns are not ticket workers: they do not take an admission slot, but they wait for the machine steward (`holding`, cap), the global daily ceiling and the captain's own limit.
- **G28** A captain sees only its own crew in `list_tasks` and `task_status`, never another captain's or a captainless session.

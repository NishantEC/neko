# Super-agent features: exact implementation spec (2026-11)

> F2 (crew files per workspace) is superseded by `phase-1-cats-2026-11.md` section 2 (files per cat).

Eight features, three build batches. The decisions in the brief are fixed; this file maps each to the code and removes the
design choices. Where a decision left a real gap the smallest choice is written down and marked **GAP-CHOICE Gn**; all of them
are listed at the end so a reviewer can accept or overrule each one. Names were checked against the tree on 2026-11-20.
Examples use placeholders (Asha, Acme, ENG-123); nothing here is private.

## Rulings (2026-11-20, set by the orchestrator for the owner to review): these override the body below

- **R1 Levels.** Only `read` and `steer` exist in this release. The `full` level is removed. `cancel_task` and `undo_run` are never available to an outside agent; it may only read the undo preview. (Overrides G14 and the `full` level in F3.)
- **R2 Pairing is native-only.** `PairCode` is accepted only on the Unix socket that the Rune app uses (`neko.sock`), and refused on the HTTP API, so no web plugin can mint a code. The web UI cannot pair in this release; Rune's Settings does it. (Overrides the pairing route in F3.)
- **R3 Caps.** Accept G19 as written: existing and seeded routines stay uncapped; new Custom and pack routines get `daily_cap` 10.
- **R4 Defaults.** The endpoint is OFF; a new pairing starts at `read`. Record mode (F1) is accepted as thin output (G2).
- **R5 Everything else** in the 45 choices (G1–G45) is accepted as written.

## 0. Rules every batch follows

- **Wire types.** New fields on stored or wire structs are `#[serde(default)]` (and `skip_serializing_if` when an empty
  value must not change an existing digest or snapshot byte). Old rows must load unchanged. Enum variants are appended.
- **Storage.** Nothing new goes into the workbench snapshot blob. Definitions and settings are small rows through `Db`'s
  methods (`set_setting`, `set_setting_quiet`, `set_setting_deferred`); anything that changes on a timer gets its own
  row (the timer-data rule in `CLAUDE.md`). The one new SQL table is `usage_events` (F5).
- **Requests.** Web code sends `request("Name")`, `request({ Name: {...} })` or `command("Goals", {...})`
  (`web/src/api.ts`). Every request a batch adds must be named in `web/src/` so the Rune allow-list mirror
  (`apps/neko-rune/tests/allow_list.rs`, `WEB_SENDS`) sees it. Destructive ones (marked below) are confirmed in the web UI
  and, in Rune later, only built inside `confirm(`.
- **Daemon dispatch.** New `Request` variants are matched in `crates/neko-daemon/src/server.rs::handle_request`; new
  `GoalCommand` variants in `crates/neko-daemon/src/workbench/goals.rs::goal_command` (the core `Command::Goals` arm in
  `crates/neko-core/src/workbench.rs` already answers "handled by the daemon").
- **Golden prompts.** A prompt change needs `NEKO_UPDATE_PROMPTS=1 cargo test -p neko-core --test prompts` and a reviewed
  diff (`crates/neko-core/tests/prompts/`). Where this spec says "no golden changes" the builder must confirm that.
- **Docs.** Each batch adds one section to the matching `docs/knowledge/*.md` (named per feature) and, for F3 only, a
  new `docs/knowledge/outside-agents.md` plus one index line in `CLAUDE.md`.
- **Gate.** `scripts/check.sh` (clippy always-a-defect lints, Rust tests, Swift tests) and `bash scripts/privacy-check.sh`.
- **Literal sites.** `Routine` has no `Default`; adding a field breaks every struct literal. Sites to update when F4 lands
  (listed once, here): `crates/neko-daemon/src/phone.rs:339`, `crates/neko-daemon/src/routines.rs:546` and `:1375`,
  `crates/neko-daemon/src/improve.rs:338`, `crates/neko-daemon/src/morning_digest.rs:166`,
  `crates/neko-core/src/routines.rs:547`, `:581`, `:775`, `crates/neko-core/src/calendar.rs:438`,
  `crates/neko-core/src/packs.rs:1629`, `:2488`, `:2539`, `crates/neko-core/src/improve.rs:239`,
  `crates/neko-protocol/src/routines.rs:260`, and the Rune test scene `apps/neko-rune/src/scenes_pages.rs:24`
  (Rune's own `save_routine` at `apps/neko-rune/src/pages.rs:650` uses `..view.routine.clone()`, so it keeps the policy).
  Each gets `policy: RoutinePolicy::default()` (a policy with `daily_cap: 0`, see G19).

---

## F1. Record-first

A proactive job is one that runs with nobody asking: a routine on its cadence, the skill curator. A person's "Run now" and a
goal turn are not proactive. A routine gets `mode: act | record`; `act` is today's behaviour. In `record` the job works out
what it would have done, writes one journal line "Would have: <text>" and changes nothing.

### (a) Files and names

- `crates/neko-protocol/src/routines.rs`: `Routine` (add `policy`), `RoutineRun` (add `recorded`).
- `crates/neko-core/src/routines.rs`: add `pub const RECORD_NOTE: &str` (the prompt note) and
  `pub fn record_line(parsed: &Parsed) -> String`; `Parsed` is the type `parse_reply` returns.
- `crates/neko-daemon/src/routines.rs`: `run_body` (where `spec`, `config` and the `outcome` are built; the record
  branch changes all three), `run_agent` (called with the record spec), `edit` / `merge_edit` (keep `policy` from the edit).
- New `crates/neko-daemon/src/routine_record.rs` holding the branch so `run_body` gets one call site (merge risk with F4's
  `routine_guard.rs`, see Batches).
- Curator: `crates/neko-core/src/skills/curator.rs` (`finish`, `note_run`, `due`, `is_on`; add `finish_dry`),
  `crates/neko-daemon/src/workbench/skill_curator.rs` (`curate`), `crates/neko-protocol/src/skills.rs`
  (`SkillCommand`, `Curation`).
- `crates/neko-core/tests/prompts.rs` + new golden `crates/neko-core/tests/prompts/routine-record.md`.

### (b) Fields, requests, rows

```rust
// neko-protocol/src/routines.rs
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoutineMode { #[default] Act, Record }

// RoutinePolicy is defined once, in B1 (F4), with all four fields; B2 only wires `mode`. See Batches.
pub struct Routine { /* existing */ #[serde(default)] pub policy: RoutinePolicy }
pub struct RoutineRun { /* existing */ #[serde(default, skip_serializing_if = "std::ops::Not::not")] pub recorded: bool }
```

- **G4** `mode` is stored in the routine's definition (`routines_v1`, inside `Routine::policy`). That row is not the snapshot
  blob and is the routine's own record; the per-routine rows (`routine_state_v1:<id>`, `routine_runs_v1:<id>`) hold timer
  data and history only. Default `Act`, so every existing row behaves as before.
- No new request: `SaveRoutine { routine }` carries it (the daemon's `edit` already replaces the whole routine through
  `merge_edit`, which takes `..edit`, so `policy` comes from the edit).
- Validation in `edit`: none beyond the enum.
- Curator (G5): row `neko_curator_mode_v1` (`"act"` default | `"record"`), quiet write. `SkillCommand::SetCuratorMode { record: bool }`
  (appended). `Curation` gets `#[serde(default)] pub record: bool` and `#[serde(default)] pub would_have: Vec<CuratorWould>`
  where `CuratorWould { at_ms: i64, skill: String, text: String }`, newest first, at most 20, stored in row
  `neko_curator_would_v1` (quiet), attached when `skills::load` reads (like `Curation::skills`), never stored in `neko_skills_v1`.

### Behaviour, exactly

Routine in `record` mode (`current.policy.mode == Record`, and the run was not `run_requested` by the person):

1. **G2** The spec is `writable: false`; the MCP config is `{"mcpServers": {}}` (no scope servers, no pack servers from
   `packs::routine_servers`, no Peekaboo); `chrome` is false and `routine.computer` is ignored. A read-only run with no
   tools cannot change anything, which is what "does nothing else" has to mean in code. The Codex fallback in `run_agent`
   uses the same read-only spec and empty config.
2. The prompt is the normal `build_prompt` output followed by `RECORD_NOTE`:
   "Record mode. Do not change anything and do not try to. You have no tools that act. Work out what you would have done this run,
   and put it in `did`, one line per action, each written as the action itself (for example "Reply to Asha about ENG-123").
   Put nothing in `needs_you`. Answer in the usual JSON." (Golden `routine-record.md` pins the note with a sample routine.)
3. **G3** On a good run: `parsed.needs_you` is dropped (not raised), `parsed.resolved` is ignored (no `resolve_items`),
   `parsed.memory` is not saved, `quiet_streak` is not changed. The stored run is
   `RoutineRun { ok: true, quiet: parsed.quiet, did: vec![record_line(&parsed)], needs_you: vec![], recorded: true, .. }`
   where `record_line` is `"Would have: "` + the `did` lines joined with `"; "`, cut at 600 characters (`MAX_LINE` in
   `neko-core/src/routines.rs`); an empty `did` gives `"Would have: nothing (quiet run)"`.
4. Cost is recorded as always (`cost_micro_usd`) and the usage ledger gets its row (F5).
5. A failed record run fails like any run (failure counting and alerts unchanged).

Curator in record mode (`neko_curator_mode_v1 == "record"`): `curate` still asks the model, then calls
`curator::finish_dry(db, data, &prepared, &answer, now_ms, &date) -> Result<Outcome, String>` which runs the same checks as
`finish` (base hash, `auto_safe`, proposal vs apply) and returns the `Outcome` without writing the skill, without
`propose_lesson`, without changing `curated_hash` (so act mode later still sees the candidate). `Applied(diff)` and
`Proposed(why)` each push a `CuratorWould { skill: name, text: "Would have: tidied <name> (<N> lines changed)" }` /
`"Would have: proposed a change to <name> (<why>)"`. `note_run` is still written so the pass waits a day.

### (c) Web UI

- **Routine editor** (`web/src/pages/Inbox.tsx`, `RoutineEditor`): a new block at the top of the scrollable body, above
  "What to do": `SegmentedControl` (aria-label "When it runs") with items `Act` and `Record only`; description under it:
  "Record only: Neko works out what this routine would have done and writes one line, "Would have: …". It changes nothing
  and sends nothing, and it cannot use your tools while it does." Saved with the rest through the existing `SaveRoutine`.
- **Routine row** (`web/src/setup/routine-status.ts::routineStatus`): a last run with `recorded` reads
  `["done", "Recorded"]`. In the row's meta (`RoutineRow` in `Inbox.tsx`) a recorded run shows its `did[0]` ("Would have: …")
  in the same place an acted run shows its first line. A routine in record mode shows a quiet chip "Record only" beside the name.
- **Skills curator** (`web/src/pages/setup/tools/Skills.tsx`, next to the existing "Turn off"/"Turn on" button that sends
  `SetCurator`): a second outline button `Record only` / `Act on my skills` (label by current mode) sending
  `skillsCmd("SetCuratorMode", { record: !curator.record })`; below the curator block, when `curator.would_have` is non-empty,
  a list titled "What the curator would have done" (text, skill, relative time), at most 5 shown, "Show all" for 20.
- Empty state: none needed (the list is hidden when empty). Error state: the existing `ErrorNote` the page uses for `skillsCmd`;
  the routine editor shows the daemon's message in the existing toast.

### (d) Rune follow-up (described only)

`apps/neko-rune/src/view/routines.rs`: the edit panel gets the same two-way control and the "Recorded" status word
(`RoutineDraft` in `apps/neko-rune/src/pages.rs` gains `mode`); `SaveRoutine` is already on the allow-list. The curator
control belongs in `apps/neko-rune/src/view/setup/` Skills. No new request, so `tests/allow_list.rs` needs only the
`SetCuratorMode` name added to `RUNE_EXTRAS` if Rune sends it before the web does.

### (e) Tests (one behaviour each)

1. A routine with no `policy` field in its stored JSON loads as `mode: Act` and runs exactly as before.
2. In record mode the run spec is read-only, has an empty MCP config and no browser or computer use.
3. In record mode a reply with `needs_you` items raises no Inbox item and saves no memory.
4. In record mode the stored run has `recorded: true` and exactly one `did` line starting "Would have: ".
5. An empty `did` in record mode stores "Would have: nothing (quiet run)".
6. A "Run now" on a record-mode routine acts normally (not recorded).
7. The record prompt equals the golden `routine-record.md`.
8. `finish_dry` returns `Applied` for a case `finish` would apply and leaves the skill file and `curated_hash` untouched.
9. In curator record mode a pass writes a `CuratorWould` entry and still calls `note_run`.
10. `Curation` round-trips with and without `record` / `would_have`.

### (f) Risks

- Record mode is useful only as far as the agent can see without tools: with no MCP servers it reasons from memory, open
  Inbox items and read-only files, so a mail-triage routine will say little. This is deliberate (G2); read-only per-server
  grants are a later feature and need a notion of read-only tools that does not exist today (`ScopeServer.level` is
  "scope" or "repo", not read/write).
- "Run now" bypassing record mode is a rule, not a bug: a person asking is not proactive.
- A routine edited from act to record mid-run: the run in flight uses the definition it started with except for the
  `current` re-read at the end; `run_body` already re-reads `current` after the run, so the stored run uses the mode read at
  the start (`routine.policy`), never `current.policy`.

---

## F2. Crew rules and plan files

Per workspace: `<data>/crews/<workspace-id>/rules.md` and `plan.md`, 16 KB each. Rules go verbatim into every agent prompt
for that workspace under the heading "Your crew's rules". The plan is shown in the UI; agents can read it; no Neko tool
writes it. Writes go through `CrewFile`; the previous file is kept as `<name>.prev`.

### (a) Files and names

- New `crates/neko-core/src/crew_files.rs` (register in `crates/neko-core/src/lib.rs`): paths, read, write, prompt section.
  Uses `neko_protocol::support_dir()` and `neko_core::artifacts::valid_id` for the id check.
- `crates/neko-protocol/src/lib.rs`: `Request` (new variants), `Response` (new variant); types in `crates/neko-protocol/src/crew.rs`.
- `crates/neko-daemon/src/server.rs::handle_request`: the two arms.
- Injection sites (each one line):
  - tickets, all roles (scout, builder, reviewer, splitter, supervisor, coordinator): `crates/neko-daemon/src/workbench.rs`
    where `let rules = neko_core::rulebook::text(&snapshot);` is built (near line 1091; every later `prompt(...)` call, including
    the reviewer's at ~1421, uses that variable). Append `crew_files::prompt_section(&workspace.id)`.
  - chat: `crates/neko-core/src/neko_chat.rs::prompt_with_checkpoint`, the `crate::rulebook::text(snapshot)` argument of the big
    `format!`: append `crew_files::prompt_section(scope)` when `scope` is `Some`.
  - routines: `crates/neko-daemon/src/routines.rs::run_body`, the `orders` string passed to `build_prompt`: append the section.
  - Not injected: the goal judge, the curator, extraction calls (not crew agents).

### (b) Types, requests, storage

```rust
// neko-protocol/src/crew.rs
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrewFileKind { Rules, Plan }
pub const MAX_CREW_FILE: usize = 16 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CrewFiles {
    pub workspace: String,
    pub rules: String, pub plan: String,
    pub rules_updated_ms: i64, pub plan_updated_ms: i64,
    /// The text of `rules.md.prev` / `plan.md.prev`, empty when there is none (the page's "Restore previous").
    pub rules_prev: String, pub plan_prev: String,
}
// neko-protocol/src/lib.rs, appended to Request:
CrewFile { workspace: String, kind: crew::CrewFileKind, text: String },   // the decision's request; answered with CrewFiles
CrewFiles { workspace: String },                                          // G6: the read the page needs
// appended to Response:
CrewFiles(crew::CrewFiles),
```

- **Path.** `crew_files::dir(workspace) -> Option<PathBuf>` is `support_dir().join("crews").join(workspace)`, returned only if
  `artifacts::valid_id(workspace)` (1 to 64 of letters, digits, `-`, `_`; no traversal). The daemon also checks the workspace
  exists in the snapshot before a write (`"That workspace no longer exists"`). Files: `rules.md`, `plan.md`; previous copies
  `rules.md.prev`, `plan.md.prev` (G9: the name is `<file name>.prev`).
- **Write** (`crew_files::write(workspace, kind, text)`): `text.len() <= 16 * 1024` bytes else
  `Err("Keep it under 16 KB")`; valid UTF-8 (it is a `String`), no NUL byte else `Err("That text has a character Neko cannot keep")`.
  Order: create the directory (0700); if the file exists, copy it over `<name>.prev` (write temp + rename); write the new
  text to a temp file in the same directory and `rename` onto the file. Empty or whitespace-only `text` removes the file after
  the `.prev` copy ("clear"). The plan file is then set to mode 0444 (G7); the rename replaces a read-only file fine.
  One process-wide `Mutex` serialises writes.
- **Read** (`crew_files::read(workspace) -> CrewFiles`): missing file reads as empty text; a file over 16 KB (edited by hand)
  is cut at 16 KB on a character boundary for prompts and the UI.
- **Prompt section** (`crew_files::prompt_section(workspace_id) -> String`): empty string when the rules file is empty or
  missing; otherwise exactly `"\n\nYour crew's rules\n" + rules.trim_end() + "\n"`, and, when the plan file is non-empty, one more line
  `"The crew's plan is at <absolute path of plan.md>. Read it before you start; it is the person's, so do not edit it.\n"`
  (G7: this is how "agents read it" is made true; no tool writes it). **G8** Because the ticket `prompt()` takes one `rules`
  string and places it under "Neko's rulebook (...)", the crew section arrives at the end of that block, with its own heading
  line; the heading text is exactly "Your crew's rules". With no crew files, every prompt and every golden is byte-identical to today.
- Events: none new. A write answers `Response::CrewFiles` with the stored state; other pages re-read on focus (no broadcast).

### (c) Web UI

Workspaces page, `web/src/pages/setup/Workspaces.tsx`, inside `WorkspacePanel` (the per-workspace expandable panel): a new
collapsible line "Crew" (same `Line` pattern as its siblings, label "Crew", value "Rules 412 B · Plan 1.2 KB" or "Not set").
Open, it holds two blocks:

- **Rules**: `Textarea` (rows 8, `aria-label` "Your crew's rules"), hint "Every agent working in this workspace reads this, word
  for word. Up to 16 KB." Buttons: `Save rules` (primary, disabled until the text differs) and `Restore previous` (outline, shown only
  when `rules_prev` is non-empty; it sends `CrewFile` with that text, which itself keeps the replaced text as the new `.prev`).
- **Plan**: same editor, `aria-label` "The crew's plan", hint "The plan agents read and do not change. Only you edit it here.", buttons `Save plan`, `Restore previous` (from `plan_prev`).
- Counter under each: "412 of 16,384 bytes", turning red over the limit (button disabled).
- Empty state, no files: the two editors empty with placeholder "Write the rules this crew works by: how to name branches, what to ask first, what never to touch." and "Write the plan: what this crew is working towards, in order."
- Error state: `ErrorNote` title "Couldn't save the rules" with the daemon's message (for example "Keep it under 16 KB").
- Loaded with `request({ CrewFiles: { workspace } })` when the line opens, and after each save.

### (d) Rune follow-up (described only)

`apps/neko-rune/src/view/setup/workspaces.rs`: a "Crew" section in the workspace editor with two text areas, the byte counter and
Restore previous, using `CrewFiles` and `CrewFile`; `CrewFile` overwrites text, so it is built only inside `confirm(` when the
new text is empty (a clear), plain otherwise.

### (e) Tests

1. `valid_id` failures (`../x`, empty, 65 characters, `a/b`) never create a directory.
2. Writing 16,384 bytes succeeds and 16,385 fails with "Keep it under 16 KB" and leaves the old file.
3. A second write keeps the first text as `rules.md.prev` and a third replaces `.prev` with the second.
4. A NUL byte is refused.
5. Writing empty text removes the file and keeps `.prev`.
6. `plan.md` is mode 0444 after a write and a later write still succeeds.
7. `prompt_section` is empty with no files and has the exact heading and text otherwise.
8. With rules set, the builder, reviewer and scout prompts for that workspace contain the section, and a prompt for another workspace does not.
9. With no crew files, `prompts.rs` goldens (`chat-full-power.md`, `chat-read-only.md`, `routine-*.md`) are unchanged.
10. A routine prompt for a workspace with rules contains the section after the standing orders.
11. `CrewFile` for a workspace that does not exist is refused; `CrewFiles` for one answers empty.

### (f) Risks

- Prompt size: the ticket prompt is bounded by `MAX_PROMPT`; 16 KB of rules is small against it, but a test must pin that a
  maximal rules file still leaves `prompt(...)` under `256 * 1024` (the existing test near `workbench.rs:3613` checks this for the scout).
- "Read-only to agents" is a convention for a writable run: such an agent can edit the file with a shell. Mode 0444 and the
  prompt wording are a speed bump, not a lock (G7). Neko's own tools never write it. Read-only runs run under `sandbox-exec`
  (`cli_workers::sandbox_profile`); whether the data folder is readable there must be checked, and if it is not, the plan text
  has to be pasted into the prompt instead of linked (a one-line change inside `prompt_section`).
- A hand-edited file bigger than 16 KB is cut for prompts; the UI shows the cut text and a note "The file on disk is larger than 16 KB; only the first 16 KB is used."

---

## F3. Outside-agent endpoint

A loopback-only MCP HTTP endpoint at `127.0.0.1:<port>/mcp` (default 4748), off by default (`outside_agents.enabled`). Pairing
with a 6-digit code gives a bearer token; the daemon keeps only its SHA-256. Levels `read`, `steer`, `full`; destructive tools
only at `full`. Every call journaled (tool, level, client name, no arguments). `RevokeOutsideAgent` removes a client.

### (a) Files and names

- New `crates/neko-daemon/src/outside.rs` (register in `crates/neko-daemon/src/main.rs`): the listener, the HTTP subset, JSON-RPC,
  pairing, token store, journal. It reuses `web.rs` helpers: `pub fn constant_time_eq`, `pub fn origin_allowed`,
  `pub fn host_allowed`, and the bounded request reader. `web.rs::read_request` is private; make it `pub(crate)` and add a
  `Req` accessor use (no behaviour change) rather than copying the HTTP parser.
- `crates/neko-protocol/src/crew.rs`: `CrewRole::Outside { level: OutsideLevel, client: String }`, `OutsideLevel`, `TOOLS` (one new tool, `goal_journal`, so the array is `[&str; 12]`), the `allows` / `label` arms.
- `crates/neko-core/src/mcp_host/bridge.rs`: `crew_definitions()` (private today) becomes `pub`; add `goal_journal`'s definition; add
  `pub fn crew_tool_json(names: &[&str]) -> Vec<serde_json::Value>` returning `{name, description, inputSchema}` for the MCP `tools/list`.
  The test `crew_tools_map_to_crew_actions_and_every_one_has_a_definition` (line ~308) must still pass.
- `crates/neko-daemon/src/workbench/crew.rs`: `visible` (new arm), `origin_of` (new arm), `crew_call` (route `goal_journal`),
  `crew_create` (Outside semantics, G13), `crew_cancel` (G14). New `fn crew_goal_journal`.
- `crates/neko-daemon/src/mcp_host.rs`: `Host::crew_lease(run_id, role)` is used as is: one lease per tool call with
  `run_id = "outside:<client id>"`, dropped when the call returns (that ends the capability, as for a run).
- `crates/neko-daemon/src/server.rs`: arms for the five requests below; start of the listener thread next to the web bridge start
  (the block around `crate::web::bind(port)` / `crate::web::serve(...)`, ~line 1460-1490).
- `crates/neko-protocol/src/lib.rs`: `Request` / `Response` variants.
- Web: `web/src/pages/setup/settings/Outside.tsx` (new), `registry.ts` (`SectionId` adds `"outside"`, `SECTIONS`, `SETTINGS`),
  `web/src/pages/setup/Settings.tsx` (the section map next to `phone: () => <PhoneSection />`).

### (b) Types, requests, rows

```rust
// neko-protocol/src/crew.rs
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutsideLevel { #[default] Read, Steer, Full }

// CrewRole gains:
Outside { level: OutsideLevel, client: String },
```

`CrewRole::allows` for `Outside`:

| Level | Tools |
| --- | --- |
| `read` | `list_tasks`, `task_status`, `goal_journal` |
| `steer` | read's, plus `steer_task`, `create_task` |
| `full` | every entry of `TOOLS` (adds `wait_for`, `cancel_task`, `schedule`, `save_artifact`, `show_artifact`, `checkpoint_diff`, `undo_run`) |

`label()` is `outside:<client>`. Destructive tools (`cancel_task`, `undo_run`) are in `full` only, by construction of this table.

```rust
// neko-protocol/src/lib.rs, appended to Request (each answered with Response::OutsideAgents unless noted)
OutsideAgents,                                           // the page's read
SetOutsideAgents { enabled: bool, port: Option<u16> },   // port 1024..=65535, not the web port; None keeps it
PairCode,                                                // answered with Response::PairCode
SetOutsideAgentLevel { client: String, level: crew::OutsideLevel },   // G10
RevokeOutsideAgent { client: String },                   // destructive: confirmed in the UI
// appended to Response:
PairCode { code: String, expires_ms: i64 },
OutsideAgents(crew::OutsideAgents),
// neko-protocol/src/crew.rs
pub struct OutsideClient { pub id: String, pub name: String, pub level: OutsideLevel, pub created_ms: i64, pub last_used_ms: i64 }
pub struct OutsideCall { pub at_ms: i64, pub client: String, pub level: OutsideLevel, pub tool: String }
pub struct OutsideAgents { pub enabled: bool, pub port: u16, pub listening: bool, pub error: Option<String>, pub clients: Vec<OutsideClient>, pub recent: Vec<OutsideCall> }
```

Rows (all through `Db`, none in the snapshot):

- `outside_agents.enabled`: `"on"` / `"off"`, absent means off. `outside_agents.port`: decimal, absent means 4748. (These are the
  decision's setting names, used verbatim as setting keys.)
- `outside_clients_v1`: JSON array of `{id, name, level, token_sha256 (hex), created_ms}`. `id` is 16 hex characters from
  `/dev/urandom`; the token is 32 random bytes, hex, shown to the client once and never stored (only `Sha256(token)`).
- `outside_seen_v1` (own small row, deferred write because it changes on use): map `client id -> last_used_ms`.
- `outside_journal_v1`: ring of the last 500 `OutsideCall` (tool name, level, client name; **no arguments**), `set_setting_deferred`.
  G16.
- Pairing state is memory only: one active code `{code, expires_ms (now + 300_000), bad_attempts}`; a new `PairCode` replaces it.

### Behaviour, exactly

**Listener.** `outside::spawn(state)` starts one thread that every 5 s (G18) reads the two settings: enabled and not bound ->
`TcpListener::bind(("127.0.0.1", port))` (never any other address; no parameter widens it, as `web::bind`); disabled or the port
changed -> drop the listener; a bind error is kept for `OutsideAgents.error`. Accept loop non-blocking with a 200 ms sleep, one
thread per connection, at most 16 at once, `Connection: close`, header timeout 10 s, body limit 256 KB, read timeout 35 min only
for `tools/call` of `wait_for` (the rest 30 s).

**Gates, in order, on every request** (reject with the status shown, before any parsing of the body):

1. Peer address is loopback (`peer.ip().is_loopback()`), else 403.
2. `Host` is exactly `127.0.0.1:<port>` or `localhost:<port>` (G12; DNS-rebinding defence as `web::host_allowed`), else 403.
3. `Origin`, when present and non-empty, must be the web UI's origin: `http://127.0.0.1:<web port>` or `http://localhost:<web port>`
   (`web::origin_allowed(origin, web_port)`, G17), else 403. No `Origin` (a normal non-browser client) passes.
4. Method and path: `POST /pair` and `POST /mcp` only; anything else 404 (GET /mcp is 405).
5. `/mcp` needs `Authorization: Bearer <token>`: hash it, compare against each stored hash with `web::constant_time_eq`; none -> 401.

**`POST /pair`** (no bearer; gates 1-4 apply). Body `{"code":"123456","name":"Claude Desktop"}`, `name` 1 to 60 printable characters.
A wrong or expired code returns 401 `{"error":"That code is not valid"}`; **G11** the fifth wrong attempt on the active code burns
it, and after any burn pairing answers 429 for 60 s. A right code is consumed (one use), a client is created with
`level: read`, and the answer is `{"token":"<64 hex>","level":"read","note":"Ask the person to raise the level in Neko if you need more."}`.
Codes are 6 digits from `/dev/urandom`, uniformly (no modulo bias), valid 5 minutes.

**`POST /mcp`** is MCP Streamable HTTP, stateless, JSON responses only (no SSE, no `Mcp-Session-Id`):

- `initialize` -> `{"protocolVersion": <the client's if it is one of "2025-03-26","2025-06-18","2025-11-25", else "2025-06-18">, "capabilities": {"tools": {}}, "serverInfo": {"name": "neko", "version": <crate version>}, "instructions": "Neko's crew tools. The level of this pairing decides which exist."}`.
- `notifications/initialized` and other notifications -> 202, empty body.
- `ping` -> `{}`.
- `tools/list` -> the tools of the client's level, from `bridge::crew_tool_json(role.tools())`.
- `tools/call` `{name, arguments}`: take `host.crew_lease("outside:<id>", CrewRole::Outside{level, client: name})`, call
  `controller.crew_call(lease.token(), name, &arguments.to_string(), &artifacts)` (the same function the stdio bridge uses, so
  idempotency, the profile-revision fence, audit lines and caps all apply), drop the lease. `Ok(text)` -> `{"content":[{"type":"text","text":text}]}`;
  `Err(e)` -> `{"content":[{"type":"text","text":e}],"isError":true}`. A tool outside the level is refused by `CrewRole::allows`
  with the existing message "<tool> is not available to this run (...)".
- Unknown method -> JSON-RPC error `-32601`. Malformed JSON -> `-32700`. Batch arrays -> `-32600` (not supported).
- After the call, one `OutsideCall` is appended to `outside_journal_v1` (also for refused tools) and `outside_seen_v1` is updated.

**Crew semantics for `Outside`** (in `workbench/crew.rs`):

- **G13** `visible`: the active agent profile's workspaces only (the `ReadOnly`-without-scope rule:
  `snapshot.agent_profiles.owner(&task.workspace_id) == snapshot.agent_profiles.active_profile_id`).
- `origin_of`: `format!("outside:{client_id}")` (counts toward the existing delegation cap of 8 open tasks per origin).
- `crew_create`: the generic (non-`Full`, non-`Task`) branch, with two differences for `Outside`: `workspace_id` is required unless
  there is exactly one workspace (it never falls back to `pending_turn`), and a `goal` argument is ignored. The ticket's
  `request` is not set (an outside agent's text is never presented as the person's words) and it waits for plan approval
  (`start_when_planned` is not inserted); the audit event reads "Opened by outside:<client>: ...".
- `steer_task`: unchanged (it already uses `role.label()` as the source).
- **G14** `cancel_task` at `full` runs exactly as it does for the full chat (`crew_cancel`, an audit event on the task). The
  codebase has no confirmation step for `cancel_task` today, so "the existing confirmation path" is, for this tool, the
  person's own choice to raise the client to `full` (a confirmed action in the UI). `undo_run` at `full` returns the existing
  preview-only answer (`can_undo`, `would_undo_runs`, `confirm_with`) and changes nothing; the person confirms with the
  existing Undo button on the ticket. Both facts are shown in the pairing UI.
- **G15** `goal_journal` (new crew tool, read): arguments `goal_id` or `task_id`, optional `limit` (default 20, max 50); returns
  `{goal_id, status, objective, turns, journal:[{turn, done, next, note, at_ms}], ledger:[{command, exit_code, passed, tail}]}` from
  `neko_core::goals::load_all`; visible only if the goal's workspace is visible. For the other roles `allows` is unchanged
  (`Full` gets it because it allows all of `TOOLS`; `ReadOnly` and `Task` match arms are explicit and do not list it). The
  full-chat instruction in `neko_chat.rs` lists tools by name; do not edit it (no golden change); the tool is simply there.

`SetOutsideAgentLevel` to `full` and `RevokeOutsideAgent` are the destructive requests of this feature (confirmed in the UI).
`SetOutsideAgents { enabled: true }` is not destructive. Revoking deletes the client row, its `seen` entry, and answers at once;
an in-flight call finishes (its lease is already issued), the next request is 401.

### (c) Web UI

New Settings section **Outside agents** (id `outside`, group "Neko", after Phone; summary "Let an agent on this Mac, outside Neko, look at your tasks and steer them."; keywords outside, mcp, claude desktop, pair, token, endpoint),
`web/src/pages/setup/settings/Outside.tsx`, built from `Section`, `SettingRow`, `Toggle`, `Pick`, `Panel`, `ErrorNote` like `Phone.tsx`:

- `SettingRow` "Let outside agents connect", `Toggle` off by default, description "Opens 127.0.0.1:4748 on this Mac only. Nothing outside this Mac can reach it."
  Sends `SetOutsideAgents { enabled, port: null }`.
- `SettingRow` "Port": `Input` (numeric, placeholder 4748), "Save" button; refused values show "Choose a port from 1024 to 65535, not the one Neko's web page uses."
- When on and listening: a quiet line "Listening on http://127.0.0.1:4748/mcp" with `Copy`.
- Button **Pair an agent** (enabled only while listening): sends `PairCode`; shows a `Panel` with the code in large type
  ("482 910"), "Valid for 4:59" counting down, and the steps: "In the agent, send this code to http://127.0.0.1:4748/pair as {"code": "…", "name": "…"}, then use the token it gives back as a bearer token for http://127.0.0.1:4748/mcp." Button `New code`. At expiry: "That code expired."
- **Paired agents** list (one `Row` each): name, `Pick` level (`Read`, `Steer`, `Full`), "Last used 3 min ago" or "Never used", `Revoke` button (destructive). Choosing `Full` opens a confirm dialog: title "Give {name} full control?", body "It will be able to cancel tasks, schedule work, save files as artifacts and ask for runs to be undone. Undoing still needs your click.", buttons `Cancel` / `Give full control`. `Revoke` dialog: "Revoke {name}?", body "It stops working at once.", buttons `Cancel` / `Revoke`. Level tooltips: Read "Can list tasks, see their status and read goal journals."; Steer "Can also send messages to tasks and open new ones (they wait for your approval)."; Full "Can use every crew tool."
- **Recent calls** (collapsed list, last 20): "12:04 · list_tasks · read · Claude Desktop". No arguments are shown because none are stored.
- Empty state: `EmptyBlock` "No agents paired" / "Turn this on, then choose Pair an agent." Error state: `ErrorNote` title "Couldn't open the port" with `OutsideAgents.error` (for example "Address already in use"); load errors: "Couldn't read the outside-agent settings".
- Polling: `request("OutsideAgents")` on open, every 5 s while the page is visible and after each action.

### (d) Rune follow-up (described only)

`apps/neko-rune/src/view/settings_system.rs` (or a new `settings_outside.rs`): the same toggle, pairing code card and client list. The
destructive requests (`RevokeOutsideAgent`, `SetOutsideAgentLevel` to Full) only inside `confirm(` in `task_actions.rs`; the
allow-list test then needs the five names (they are in `WEB_SENDS` once the web ships). The code must be rendered large and
selectable, never written to a log or toast.

### (e) Tests

1. With the setting absent no listener is bound and `OutsideAgents.listening` is false.
2. Enabling binds `127.0.0.1` only; the bound address is never `0.0.0.0`.
3. A request whose `Host` is another name is refused 403.
4. A request with `Origin: http://evil.example` is refused; with the web UI's origin and no bearer it is 401; with no `Origin` and a good bearer it passes.
5. A non-loopback peer is refused (unit test of the gate with a fake peer address).
6. A pair code is six digits, works once, and fails after five minutes (injected clock).
7. Five wrong codes burn the active code and the next pairing attempt is 429 for 60 s.
8. A new pairing has level `read`, and the stored row holds only the SHA-256 of the token (the token string is not found in the row).
9. `tools/list` at `read` returns exactly `list_tasks`, `task_status`, `goal_journal`; at `steer` adds `steer_task`, `create_task`; at `full` returns all 12.
10. `tools/call cancel_task` at `steer` is refused with the "not available" message and journaled; at `full` it cancels.
11. `tools/call create_task` at `steer` opens a task that waits for approval, with no `request` set and origin `outside:<id>`.
12. A ninth open outside-created task is refused by the delegation cap.
13. `undo_run` at `full` changes no files and returns `confirm_with`.
14. Every call appends one journal entry containing tool, level and client name and no argument text.
15. `SetOutsideAgentLevel` changes the next `tools/list`; `RevokeOutsideAgent` makes the old token 401.
16. `goal_journal` returns at most 50 entries and refuses a goal in an invisible workspace.
17. Malformed JSON gives `-32700`; an unknown method `-32601`.
18. `CrewRole::Outside` round-trips and `label()` is `outside:<client>`.

### (f) Risks

- **The pairing code is the whole trust root.** Anything that can call `PairCode` can mint a client: the web token is held by
  every web plugin (plugins "run inside Neko with access to Neko's own API"). Mitigations in this spec: new clients are `read`;
  `full` is a separate confirmed action; the journal shows every call. A hostile enabled plugin could still pair a `read` client
  and read task titles. A stronger fix (an app-only confirmation for `PairCode`) is outside the decisions; flagged to the reviewer.
- 6 digits over 5 minutes is brute-forceable by a local process without the lockout (G11); the lockout bounds it to 5 guesses per code.
- `cancel_task` has no confirmation in the codebase (G14), so `full` means a cancel is one tool call away.
- `save_artifact` takes an absolute file path and copies that file into Neko's store; at `full` an outside agent can therefore
  read any file the daemon can read into an artifact. That is why it is `full` only; the UI text names it.
- The endpoint serves plaintext HTTP on loopback; any local process can speak to it but needs a token. Tokens are bearer secrets
  held by the client; Neko cannot show them again.
- `wait_for` holds a connection thread up to 30 min; the 16-connection cap bounds it.
- A client that pairs and is never revoked keeps working across daemon restarts (rows persist) until revoked.

---

## F4. Guard rails

Per routine: `daily_cap` (rolling 24 h), `run_only_when` (a shell gate), `thread` (`fresh` | `standing`).

### (a) Files and names

- `crates/neko-protocol/src/routines.rs`: `Routine::policy` (new `RoutinePolicy`), `RoutineState` (new `starts_ms`, `skip_ms`), `RoutineRun` (new `skipped`).
- `crates/neko-core/src/routines.rs`: new pure helpers (`cap_reached`, `prune_window`, `standing_thread`), unit-tested without a daemon;
  `RECORD_NOTE` stays with F1.
- New `crates/neko-daemon/src/routine_guard.rs`: the gate evaluation and the skip record. Call site: the top of
  `crates/neko-daemon/src/routines.rs::run_body`, before the prompt is built. `tick` is unchanged except that it passes
  `manual` (the `run_requested` value it read from `picked` before clearing it) through `run_one` to `run_body`
  (a new `bool` parameter; the tests in `routines.rs` that call `run_one` get `false`).
- `crates/neko-daemon/src/routines.rs::edit` (validation), `views` (nothing: `RoutineState` already travels).
- Web: `web/src/pages/Inbox.tsx` (`RoutineEditor`, `RoutineRow`), `web/src/setup/routine-status.ts`.

### (b) Fields

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct RoutinePolicy {
    pub mode: RoutineMode,                 // F1 (type declared here, behaviour in B2)
    /// Runs allowed in any rolling 24 hours. 0 means no cap. G19.
    pub daily_cap: u32,
    /// Shell command; exit 0 means run. Empty means none.
    pub run_only_when: String,
    pub thread: RoutineThread,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoutineThread { #[default] Fresh, Standing }

// RoutineState (own row routine_state_v1:<id>), both #[serde(default, skip_serializing_if = "Vec::is_empty")]
pub starts_ms: Vec<i64>,   // start times of runs in the last 24 h, pruned on every start and every skip; at most 1000
pub skip_ms: Vec<i64>,     // times of skipped runs in the last 24 h, at most 200
// RoutineRun
#[serde(default, skip_serializing_if = "is_zero_u32")] pub skipped: u32,   // >0 marks a skip row: how many skips it stands for
```

- **G19** `daily_cap` is `u32` where `0` means no cap, and the stored default for every existing routine is `0`
  (serde default), so nothing existing is throttled. The decision's "default 10 per rolling 24 h" is applied when a routine is
  **created** as `Custom` (a person's new routine in the UI, a pack routine at install) via `RoutinePolicy::for_new_custom()`
  which sets `daily_cap: 10`; seeded routines (`routines::seeds`: Messages, Digest, PrSweep, Reflect, MacCare) keep `0`
  because they are meant to repeat (Messages is `Every 20 min` over 8-23h, about 45 runs a day; a cap of 10 would silently
  starve them). Max `1000`.
- **G24** The cap counts run **starts** in `starts_ms`; a skip does not count; the 24 h window rolls (`now - 86_400_000`).
- Validation in `edit` (`SaveRoutine`): `daily_cap <= 1000` else `Err("Runs per day must be 0 to 1000")`; `run_only_when.len() <= 1024`
  else `Err("Keep the command under 1 KB")`; trim the command.

### Behaviour, exactly

Order at the top of `run_body` (inside the thread `tick` spawns; `BUSY` is held; `routine_state.running` is true):

0. `manual == true` (the person pressed Run now): skip all guards (**G21**, the same rule the spend ceiling uses). Record the start in `starts_ms`.
1. **Daily cap.** Prune `starts_ms` to the last 24 h. If `daily_cap > 0 && starts_ms.len() >= daily_cap`: skip with reason
   `Skipped: daily cap reached (<len>/<cap>)` (so exactly "Skipped: daily cap reached (10/10)" at the default).
2. **run_only_when.** If non-empty, run `/usr/bin/sandbox-exec -p '(version 1)(allow default)(deny network*)' /bin/sh -c <command>`
   in the workspace's first scope folder (the same `folder` `run_body` runs the agent in), `env_clear` plus `PATH`, `HOME`, `TMPDIR`,
   `LANG`, `CI=1`, stdin null, through the existing `output_within(command, Duration::from_secs(10))` in `routines.rs` (**G23**).
   Exit 0: go on. Non-zero: skip with `Skipped: "<command, cut at 80>" exited <code><: first line of its output, cut at 120>`.
   Spawn failure or no `sandbox-exec`: skip with `Skipped: could not run the check (<error>)`. Timeout (`output_within` returns Err
   "timed out after 10s"): skip with `Skipped: the check timed out after 10 s`. A missing folder is already an error path below.
3. Past both: push `now` into `starts_ms` (saved in the same state write that follows), then the run goes on (F1's record branch,
   standing thread below, the agent).

**A skip** (`routine_guard::record_skip(state, routine, reason, now)`): not a failure and not a run. It pushes `now` into `skip_ms`
(pruned to 24 h), sets `running = false`, `next_due_ms = next_due_local(cadence, quiet_streak, now)`, leaves `failures`,
`quiet_streak`, `memory` and `last_run_ms` alone, and sets `note = Some(reason)`. **G20** The journal line is a `RoutineRun`
with `did: [reason]`, `ok: true`, `quiet: true`, `skipped: 1`; if the newest stored run is already a skip row with the same
reason started within the last 24 h, its `skipped` is incremented and its `finished_ms` moved instead of adding a row, so a
15-minute cadence cannot push real runs out of the 50 kept. `push_run`'s "keep runs with open items" rule is unaffected.

**Inbox skip count.** The Inbox row shows `skip_ms` entries newer than 24 h: "Skipped 3 times in the last 24 hours".

**Standing thread** (`thread == Standing`, **G22**): Claude workers run with `--no-session-persistence` (`cli_workers.rs`) and
`supports_resume` is false for Claude, so there is no session to resume. Instead `routines::standing_thread(runs: &[RoutineRun]) -> serde_json::Value`
builds `{"earlier_runs": [{"at": <local time>, "did": [...up to 6 lines, each cut at 200 chars], "needs_you": [titles]}...]}`
from the last 5 non-skipped runs of that routine (oldest first), at most 6 KB serialized, and `run_body` inserts it as the key
`standing_thread` in the `memory` object passed to `build_prompt` (alongside `with_owner_replies`). The saved `routine_state.memory`
is what the agent returns, so the thread never accumulates in the row. `fresh` is today's behaviour. No `build_prompt` change; no golden change.

### (c) Web UI

`RoutineEditor` (`web/src/pages/Inbox.tsx`) gets a block headed "Guard rails" under "Catch up after downtime":

- `Input` "Runs per day" (`inputMode="numeric"`, placeholder "10"), hint "At most this many runs in any 24 hours. Leave empty or 0 for no limit." Clamped 0 to 1000 on blur (`clampInt`). A new routine created in the UI starts at 10.
- `Input` "Only run when" (monospace, placeholder "test -f NEEDS_REVIEW"), hint "A shell command that runs in the workspace folder with no network access. The routine runs only if it exits 0. It gets 10 seconds."
- `SegmentedControl` "Conversation": `Fresh each run` | `Keep one thread`, hint "Keep one thread gives each run a short summary of the last five runs of this routine."
- Save sends `policy: { mode, daily_cap, run_only_when, thread }` inside the routine of `SaveRoutine`.
- `routine-status.ts`: a latest run with `skipped > 0` reads `["backlog", <did[0]>]` ("Skipped: daily cap reached (10/10)"). `RoutineRow` meta adds "Skipped N times in the last 24 hours" when `state.skip_ms` has entries newer than 24 h, as a quiet link-less text under the row (the Inbox page, DESIGN.md §8 one-line rule).
- Error state: validation messages arrive as the existing toast. Empty state: none.

### (d) Rune follow-up (described only)

`apps/neko-rune/src/view/routines.rs` edit panel: the three fields; the row reads the same skip words from `RoutineState::skip_ms`
and `RoutineRun::skipped` (`apps/neko-rune/src/pages.rs` `RoutineDraft` gains the policy fields).

### (e) Tests

1. A stored routine without `policy` loads with `daily_cap: 0` and is never capped.
2. With cap 3 and three starts in the last hour, the fourth due run is skipped with exactly "Skipped: daily cap reached (3/3)" and `starts_ms` is unchanged.
3. A start 25 hours old does not count toward the cap.
4. A skip leaves `failures`, `memory`, `quiet_streak` unchanged and sets `next_due_ms` from the cadence.
5. Ten consecutive skips with the same reason leave one skip row with `skipped: 10`, and real runs are not evicted.
6. "Run now" ignores the cap and the gate.
7. `run_only_when` exit 0 runs; exit 3 skips with `exited 3` and its first output line; a command sleeping 15 s skips with "timed out after 10 s" in about 10 s.
8. The gate runs in the workspace folder (`pwd` prints it) and cannot reach the network (a `curl` to a local test listener fails).
9. `standing_thread` over 8 runs includes the last 5, oldest first, under 6 KB, and no skipped rows.
10. With `thread: fresh` the prompt is byte-identical to today's; with `standing` it contains `standing_thread`.
11. `edit` refuses `daily_cap: 1001` and a 1,025-byte command.
12. A new Custom routine gets `daily_cap: 10`; a seeded Messages routine gets 0.

### (f) Risks

- G19 deviates from "default 10" on purpose: applying 10 to existing rows would silently throttle seeded repeating routines.
  The reviewer should confirm.
- `sandbox-exec` is deprecated but present on every macOS Neko supports and is already relied on (`cli_workers::worker_command`).
  `(deny network*)` also blocks loopback, so a gate that checks a local server will fail; the hint says "no network access".
- `output_within` kills only the child, not a process group; a gate that daemonizes could outlive 10 s. Use the group kill from
  `workbench/goals.rs::run_shell` if simple (move its kill into a shared helper); otherwise accepted.
- The gate runs a person-written command on each due tick; a heavy gate on a 15-minute cadence is the person's own cost.
- Coalescing skip rows means the per-run history has fewer rows than due times; the count is visible in `skipped`.

---

## F5. Usage ledger

Append-only table `usage_events`: one row per completed run. Written once at completion, kept 365 days, pruned daily. The
Usage page gets a history by day and by provider, and CSV export. The `USAGE` task events remain the live source.

### (a) Files and names

- `crates/neko-core/src/db.rs`: `Db::migrate` (add the table and index), new methods `record_usage_event`, `usage_events_since`, `prune_usage_events`.
- New `crates/neko-core/src/usage_ledger.rs` (register in `lib.rs`): row type, pure aggregation (`by_day`, `by_provider`),
  `to_csv`, `estimate_source`. (The existing `neko_core::usage` module is the provider-quota meter; do not reuse the name.)
- Writers:
  - ticket runs: `crates/neko-daemon/src/workbench.rs::execute` (the function that calls `execute_with_worktree`): note `Instant::now()` and
    `store::now_ms()` at entry; after the call, `usage_ledger::record_ticket_run(&self.db, &claim.task.id, started_ms, runtime, ok)`.
    The `claim.runtime` (`Option<AgentRuntime>`) gives provider and model. The usage numbers are the sum of the task's `USAGE ` events
    with `at_ms >= started_ms` (`neko_core::spend::task_usd_since` shows the parse; add `usage_ledger::sum_usage(&Task, since_ms)` returning
    cost, input tokens, output tokens, whether any event existed). `run_key` is `"ticket:<task id>:<started_ms>"`.
  - routine runs: `crates/neko-daemon/src/routines.rs::run_body`, right after `core::push_run` for both ok and failed runs (not for a skip, not for a quota hold):
    provider `meta.runtime` ("claude" / "codex"), model `routine.model` or "sonnet" (empty for codex), cost `meta.cost_micro`, `run_key = "routine:<run.id>"`.
  - not written (**G27**): Home chat turns, the goal judge and curator one-shot calls, independent reviews separately (a review's `USAGE` events are inside the ticket's run, so they are counted there).
- Prune: `crates/neko-daemon/src/workbench/parking.rs::parking_tick` (it already calls `checkpoint_housekeeping()`), at most once per local day, using a quiet row `usage_pruned_v1` holding the day string (G29).
- Requests: `crates/neko-protocol/src/lib.rs`, `crates/neko-daemon/src/server.rs::handle_request`.
- Web: `web/src/pages/setup/Usage.tsx` (new `HistorySection` next to `SpendSection`).

### (b) Table, types, requests

```sql
CREATE TABLE IF NOT EXISTS usage_events (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    run_key        TEXT    NOT NULL UNIQUE,                         -- G26: one row per run, even if recorded twice
    at_ms          INTEGER NOT NULL,                                -- when the run completed
    provider       TEXT    NOT NULL,
    model          TEXT    NOT NULL DEFAULT '',
    input_tokens   INTEGER,                                         -- NULL when not reported
    output_tokens  INTEGER,
    cost_micro_usd INTEGER,                                         -- NULL when not reported
    source         TEXT    NOT NULL CHECK (source IN ('reported','estimated')),
    duration_ms    INTEGER NOT NULL DEFAULT 0,
    task_id        TEXT    NOT NULL DEFAULT '',                     -- the ticket; for a routine run, the routine id
    kind           TEXT    NOT NULL DEFAULT 'ticket'                -- 'ticket' | 'routine'
);
CREATE INDEX IF NOT EXISTS idx_usage_events_at ON usage_events (at_ms);
CREATE INDEX IF NOT EXISTS idx_usage_events_task ON usage_events (task_id);
```

Added inside the existing `execute_batch` in `Db::migrate`; `IF NOT EXISTS` is the migration (the file has no schema version).
Inserting is `INSERT OR IGNORE` on `run_key`. These writes do **not** bump `data_revision` or the `ContentStamp` (nothing `load`
reads), so they wake no poller.

- **G25** `source` is `reported` when the provider gave a cost or tokens for the run; `estimated` otherwise (Codex reports no
  price): tokens and cost stay NULL, so "estimated" means "a run happened and the provider reported nothing"; it never invents numbers.
- **G26** `run_key` (an addition to the decision's column list) makes "written once" true when a path records twice.

```rust
// neko-protocol/src/lib.rs, appended to Request / Response
UsageHistory { days: u32 },     // 1..=365, clamped
UsageCsv { days: u32 },
UsageHistory(ledger::LedgerHistory),
UsageCsv { csv: String },
// neko-protocol/src/workbench.rs (or a new ledger.rs): wire types
pub struct LedgerDay { pub day: String /* local YYYY-MM-DD */, pub runs: u32, pub estimated_runs: u32, pub input_tokens: u64, pub output_tokens: u64, pub cost_micro_usd: u64 }
pub struct LedgerProvider { pub provider: String, pub runs: u32, pub estimated_runs: u32, pub input_tokens: u64, pub output_tokens: u64, pub cost_micro_usd: u64 }
pub struct LedgerHistory { pub days: Vec<LedgerDay> /* oldest first, every day in range, zeros included */, pub providers: Vec<LedgerProvider> /* by cost then runs */, pub total_runs: u32, pub since_ms: i64 }
```

Aggregation is in Rust: read rows with `at_ms >= now - days*86_400_000`, bucket by `spend::day_start_local`. CSV (**G28**) columns,
header included: `time,provider,model,input_tokens,output_tokens,cost_usd,source,duration_s,task_id,kind`; `time` is RFC 3339 local;
empty cells for NULL; cells quoted when they hold `,`, `"` or a newline (quotes doubled); a cell that starts with `=`, `+`, `-` or `@`
gets a leading `'` (spreadsheet formula guard). The web builds the download from `csv` (a `Blob`), file name `neko-usage-<from>-to-<to>.csv`.

Retention: `prune_usage_events(before_ms)` deletes `at_ms < now - 365 days`; run from `parking_tick` once a day.

### (c) Web UI

`Usage.tsx`, new `Section` "History" between "Spend" and "Models" (title tooltip "One line per finished agent run, kept for 365 days. The live figures above come from the runs themselves; this is the history that outlives them."):

- Range: `SegmentedControl` `7 days` | `30 days` | `90 days` | `365 days`, default 30.
- Figures (`Figure`): "Runs", "Reported cost" (`$1.23`, or "none reported"), "Tokens in / out" (compact: `1.2M / 340k`), "Estimated runs" (runs where the provider reported nothing; tooltip explains).
- Chart: `BarChart` (the page already imports Recharts `Bar`, `BarChart`, `XAxis`) of runs per day; tooltip shows runs, cost and tokens for that day.
- "By provider": `BarList` (label "Runs by provider"), each item "claude · 41 runs · $3.10".
- Button `Export CSV` (outline, with the existing icon set's download icon), sends `UsageCsv` for the chosen range.
- Empty state: `EmptyBlock` icon `Gauge`, title "No runs recorded yet", description "Neko adds one line here each time an agent run finishes. History starts from the day this version was installed."
- Error state: `ErrorNote` title "Couldn't read the usage history".
- Loaded with `request({ UsageHistory: { days } })` on open and when the range changes; no polling.

### (d) Rune follow-up (described only)

`apps/neko-rune/src/view/settings_system.rs` (Rune's Usage lives with machine health): the same range switch, three figures, a
per-day bar strip drawn with Rune primitives, the provider list and an Export action that writes the CSV to the Downloads folder
through the existing download path used by the web panel (`apps/neko-rune/src/web_panel.rs`). Requests `UsageHistory` / `UsageCsv`.

### (e) Tests

1. `migrate` is idempotent: running it twice on a database that already has the table changes nothing, and an old database gets the table.
2. A ticket run that reported `USAGE` events inserts one row with summed cost and tokens and `source: reported`.
3. A run with no `USAGE` events inserts `source: estimated` with NULL tokens and NULL cost.
4. Recording the same `run_key` twice leaves one row.
5. A failed or cancelled run is still recorded once.
6. A routine run inserts a row with the routine's id as `task_id` and `kind: routine`; a skipped run and a quota hold insert none.
7. `by_day` returns one entry per day in range with zeros for empty days, in local days across midnight.
8. `by_provider` sums runs, tokens and cost per provider and counts estimated runs.
9. `prune_usage_events` deletes rows older than 365 days and keeps the rest; `parking_tick` prunes at most once per day.
10. `to_csv` quotes commas and quotes, and guards a leading `=`.
11. Inserting a row does not change `Db::content_stamp` or `data_revision()`.
12. `UsageHistory { days: 0 }` and `{ days: 9999 }` are clamped to 1 and 365.

### (f) Risks

- The decision says "one row per completed run"; a ticket's planning, build and review phases are one `execute` claim, so a goal turn is one row. Replies and retries are separate claims, so separate rows. Documented in the knowledge doc.
- `USAGE` events are sums across the claim's events with `at_ms >= started_ms`; clocks are the daemon's own, so this is safe, but a run that overlaps another claim of the same task (not possible; one worker per task) would double count.
- Rows are in the same SQLite file as everything else; at a few hundred runs a day for 365 days this is under 10 MB.
- Chat spend is not in the ledger (G27), so ledger totals will be below the "Spend today" figure for days with heavy chat use. The page text says "agent runs".

---

## F6. Goal receipt

When a goal reaches DONE the daemon writes one markdown artifact "Receipt: <objective>", with sections Objective, Checks, Changes,
Journal, Cost and time, Pull request, every value redacted first. The morning digest's "Finished" line links it.

### (a) Files and names

- `crates/neko-protocol/src/goals.rs`: `Goal` (new `receipt`, `receipt_tries`), `MAX_JOURNAL` lives in core (below).
- `crates/neko-core/src/goals.rs`: `MAX_JOURNAL` (20 -> 50 stored, **G30**), `attach` (readers still get the last 20 for an active goal, 6 when finished), new `pub fn receipt_markdown(input: &ReceiptInput) -> String` (pure, tested).
- New `crates/neko-core/src/redact.rs` (register in `lib.rs`): `pub fn scrub(text: &str) -> String` (**G31**, no regex dependency; neko-core has none).
- `crates/neko-daemon/src/workbench/goals.rs`: `evaluate` (after the `commit(...)` that sets `GoalStatus::Done`), `goal_command::MarkDone`, and a sweep in `tick_concurrently` for Done goals with no receipt; new `fn write_receipt(&Controller or db, goal_id)`. It needs the controller for `checkpoint_diff`; the goal thread has only the `Arc<Mutex<Db>>`, so use the core calls directly: `neko_core::checkpoints::diff_of(&db, task_id, &dir, None)` with `dir` from `run_checkpoints::folder_of(task)` (already `pub(super)`; the goals module is a sibling).
- `neko_core::artifacts::save(&artifacts::root(), Save { title, file, workspace_id: Some(goal.workspace_id), task_id: goal.ticket_id.clone(), note: "Goal receipt".into(), .. })`; the markdown is written to a temp file `<temp>/neko-receipts/<goal id>.md` and removed afterwards (`save` copies the file; `md` is a text kind: `artifacts::kind_of`, `is_text`).
- Digest: `crates/neko-protocol/src/digest.rs::DigestLine` (new `link`), `crates/neko-core/src/digest.rs::compose` (the "Goal done" line), `crates/neko-daemon/src/morning_digest.rs` (nothing: `gather` passes goals as they are), `web/src/work/digest.tsx::Group`.
- Ledger cost: `neko_core::usage_ledger` (F5) `sum_for_task(db, task_id)`; F6 therefore comes after F5 in B1.

### (b) Fields and rows

```rust
// neko-protocol/src/goals.rs
pub struct ReceiptRef { pub artifact_id: String, pub version: u32, pub at_ms: i64 }
// Goal (all #[serde(default)]):
pub receipt: Option<ReceiptRef>,
pub receipt_tries: u8,
// neko-protocol/src/digest.rs, DigestLine:
#[serde(default, skip_serializing_if = "Option::is_none")] pub link: Option<String>,   // "artifact:<id>"; G35
```

`Goal` rows are settings rows (`neko_goal_v1:<id>`), so the new fields need no migration. `DigestLine { text, default }` literals in
`neko-core/src/digest.rs` (`plain`, `capped`) gain `link: None`.

### Behaviour, exactly

**When.** After a goal's status becomes `Done` through the judge (`evaluate`, `Verdict::Done`) or through `MarkDone` (**G33**, the person's
"done" counts), and not already `receipt.is_some()`. Never inside the `commit` closure or while the db guard is held: both call sites write
the receipt after the save. A failure never changes the goal: log, push a journal line "Could not write the receipt: <error>", `receipt_tries += 1`.
The goal tick retries a Done goal with `receipt.is_none() && receipt_tries < 3 && updated_ms within 24 h`.

**Content** (`receipt_markdown`; headings exact):

```
# Receipt: <objective, first line, cut at 120>

Goal finished <local date time>. <judge reason, or "You marked it done.">

## Objective
<objective>

## Checks
### `<command>` exit <code | none> (<time>, <source>)
<last 20 lines of the check's stored output, in a fenced block>
(one block per entry of the goal's evidence ledger, newest last; "No check was run." when there is none)

## Changes
Files changed: <N>, +<added> -<removed> (whole task: first run's start to last run's end)
- <path> (up to 30 paths; "and N more")
("No checkpoints were taken for this goal." when the ticket has none)

## Journal
- turn <n>: <done> | next: <next> | <note>      (the last 50 entries, one line each)

## Cost and time
Cost: $<x.xx> reported across <n> runs (<m> runs reported no price) | "no price reported"
Tokens: <in> in, <out> out (reported runs only)
Agent time: <h m> running, <turns>/<max turns> turns
Source: usage ledger (figures for this goal's ticket only)

## Pull request
<url> (<state if known>)        <- the whole section is omitted when the ticket has no pull request
```

- **G32** "Last 20 lines of output" is the last 20 lines of the ledger entry's `tail`, which the ledger already bounds to 1,200 bytes (`TAIL_BYTES`); fewer than 20 lines is normal.
- **G30** "Journal (last 50 lines)": the stored journal grows from 20 to 50 entries (`MAX_JOURNAL = 50`); `attach` trims what readers see in the snapshot to the last 20 (6 when the goal is finished, as today), so snapshot size does not grow; the receipt reads the stored log (`neko_core::goals::log(db, subject)`).
- Changes: `neko_core::checkpoints::diff_of(&db, task_id, &dir, None)` returns `files`, `added`, `removed`; the patch text is dropped. Parked folder: reads use the source checkout's objects, as `checkpoint_diff` does.
- Cost: sum of `usage_events` rows with `task_id == goal.ticket_id` (F5): `cost_micro_usd`, tokens, count where `source = 'estimated'`; time from `goal.wall_ms` and `goal.turns`.
- Pull request: `task.pull_request.url` when present.

**Redaction** (`redact::scrub`, applied to the whole markdown after it is built, so every value passes it; **G31**): word scan over
whitespace-separated tokens, all case-insensitive on the keyword part.
1. `KEY=value`: a token matching `[A-Za-z_][A-Za-z0-9_.-]*=<non-empty>` becomes `[redacted]` (the whole token).
2. A token containing `token`, `secret` or `password` (so `api_token`, `PASSWORD:`, `client-secret=`): that token and, when it ends with `:` or `=` or is followed by a token that is not a keyword itself, the next token become `[redacted]`.
3. `Bearer` plus the next token become `[redacted]`.
4. Beyond the decision, to be safe (G31): a token that starts with `ghp_`, `gho_`, `github_pat_`, `sk-`, `xoxb-`, `xoxp-`, or `AKIA` and is at least 16 characters, and a line pair `-----BEGIN ... PRIVATE KEY-----` to `-----END ...-----` become `[redacted]`.
Idempotent (`scrub(scrub(x)) == scrub(x)`); tokens inside fenced blocks are scrubbed the same way; punctuation around a token (`"`, `,`, `)`) is kept.

**Digest link (G35).** `compose`'s "Goal done: <objective>" line gets `link: goal.receipt.map(|r| format!("artifact:{}", r.artifact_id))`. The phone
text (`to_text`) is unchanged (no link in plain text); `title` unchanged.

### (c) Web UI

- `web/src/work/digest.tsx::Group`: a line with `link` starting `artifact:` renders its text followed by a link-style button `Receipt` (`text-[13px] underline-offset-2`), `href={artifactHref(id)}` from `web/src/canvas/artifacts.ts` (opens the artifact tab); the existing `Line` interface gets `link: string | null`. Lines without a link are unchanged.
- `web/src/work/goals.tsx::Ledger` (the goal's opened detail): for a Done goal with `goal.receipt`, a first row `Receipt` with a link "Open the receipt" (`artifactHref(goal.receipt.artifact_id)`); for a Done goal without one and `receipt_tries >= 3`, a quiet line "No receipt could be written."
- Empty and error states: none (the link appears only when it exists). The artifact itself renders in the existing markdown viewer (`md` kind).

### (d) Rune follow-up (described only)

`apps/neko-rune/src/view/work.rs` (goal lines): a "Receipt" action opening the artifact through the existing artifact path in the
context panel (`ArtifactFile` / the viewer registry); the Home digest card in `apps/neko-rune/src/view/home.rs` renders the `Receipt` link from `DigestLine::link`.

### (e) Tests

1. A goal that the judge rules DONE has one artifact titled "Receipt: <objective>" with the six section headings (Pull request only when there is one), linked from `goal.receipt`.
2. `MarkDone` writes a receipt too, and a second call writes none (idempotent).
3. The Checks section lists each ledger entry with command, exit code and at most 20 lines of output.
4. The Changes section shows the whole-task file count and +/- from the checkpoint rows; with no checkpoints it says so.
5. The Journal section holds the last 50 entries, and `MAX_JOURNAL = 50` is kept in the log row while a snapshot reader sees 20.
6. Cost and time come from `usage_events` for the ticket: reported cost summed, estimated runs counted separately.
7. `scrub` redacts `API_KEY=abc123`, `Bearer abc.def`, `password: hunter2`, `my_token xyz`, `ghp_...` and a PEM block, and leaves ordinary prose untouched.
8. `scrub` is idempotent.
9. A planted `AWS_SECRET_ACCESS_KEY=...` in a check's output does not appear in the stored artifact file.
10. A failing artifact save leaves the goal Done, adds the journal line, and the tick retries up to 3 times.
11. The digest's "Goal done" line carries `link: artifact:<id>` when a receipt exists and none otherwise; `to_text` is unchanged.
12. `DigestLine` round-trips with and without `link`.

### (f) Risks

- Redaction by pattern cannot be complete: a secret with no keyword, no `KEY=` form and no known prefix passes. The check output is the main risk (it is arbitrary command output). The spec's extras (G31) cut the common cases; the reviewer should decide if the receipt should omit raw output by default.
- Over-redaction (any `x=y`) will mangle benign lines such as `left=3`; chosen on purpose.
- The artifact is stored in the data folder and shown on the canvas; it is not published anywhere.
- No `ArtifactsChanged` event is broadcast from the goal thread (**G34**; `server::broadcast_event` needs an `AppState` the goal loop does not hold). An open artifact list shows the receipt on its next load; the digest link works at once.
- The changes stat reads git objects of the folder or its source checkout; if both are gone the section says "No checkpoints were taken for this goal." rather than failing.

---

## F7. Deadline default

Per goal `default_after_hours: Option<u32>`, off by default. Applies only when the goal is BLOCKED on a question that has a
default and its ticket has a start checkpoint. At the deadline the daemon answers with the default, writes "Took the default after
<N> h: <default>", records an undo target (the pre-answer checkpoint) for 7 days. `UndoDefault { goal }` undoes it (confirmed in the UI). No checkpoint: nothing is answered.

### (a) Files and names

- `crates/neko-protocol/src/goals.rs`: `Goal`, `Blocked`, `GoalCommand` (two variants; **G1** they are sub-commands of the existing `Goals` command rather than top-level requests, so the web sends `command("Goals", { UndoDefault: { goal: id } })` like every other goal action).
- `crates/neko-daemon/src/workbench/goals.rs`: `step` (the `GoalStatus::Blocked` arm, today `_ => Ok(())`), `block` (sets the new fields), `goal_command` (`Answer` is refactored into a shared helper; new arms), `tick_concurrently` (expire old undo targets), `/goal` parsing `slash`/`Slash::Set`/`create`/`Spec`.
- `crates/neko-core/src/goals.rs`: `new_goal` (takes the optional hours), validation, helper `deadline_due(goal, now) -> bool`.
- `crates/neko-core/src/checkpoints.rs` is read through `row(db, task_id)`, `preview_undo`, `undo` (unchanged).
- `crates/neko-daemon/src/workbench/run_checkpoints.rs::checkpoint_command` is reused (it does the folder, the in-use hold and the ticket event).
- Web: `web/src/work/goals.tsx`.

### (b) Fields

```rust
// Goal
#[serde(default)] pub default_after_hours: Option<u32>,          // 1..=168; None is off
#[serde(default)] pub default_taken: Option<DefaultTaken>,
// Blocked
#[serde(default)] pub asked_ms: i64,                // when the question was asked (set in block())
#[serde(default)] pub real_default: bool,           // G36: the judge supplied the default (not the generic filler)
#[serde(default)] pub deadline_off: bool,           // do not auto-answer this question (set by an undo, or when no checkpoint exists)
pub struct DefaultTaken { pub at_ms: i64, pub after_hours: u32, pub question: String, pub answer: String, pub undo_run: u32, pub expires_ms: i64, pub inbox_seq: Option<u64> }
// GoalCommand, appended
SetDefaultAfter { id: String, hours: Option<u32> },   // G1/G39: None turns it off; hours 1..=168
UndoDefault { goal: String },                         // destructive: confirmed in the UI
```

`/goal` gains `--default-after <hours>` (parsed in `slash`, stored through `Spec`/`new_goal`). `GoalCommand::Create` gains
`#[serde(default)] default_after_hours: Option<u32>`. The goal-level setting is part of the person's request, so a goal a model
proposes (`by_person == false`) ignores it. Rows are the existing `neko_goal_v1:<id>` settings rows: no migration.

- **G36** `block()` fills a missing judge default with the generic "Use your best judgement and keep going." (`inbox::clean_default`).
  Only a default the judge actually gave (`clean_default(...)` returned `Some`) sets `real_default`. The generic filler never triggers a deadline answer.

### Behaviour, exactly

**Deadline tick** (`step`: `GoalStatus::Blocked => blocked_deadline(db, env, goal)`):

1. Return unless `goal.default_after_hours` is `Some(h)`, `goal.blocked` is `Some(b)` with `b.real_default && !b.deadline_off`, and
   `env.now_ms() >= b.asked_ms + h * 3_600_000`. A blocked goal with `asked_ms == 0` (asked before this build) is never auto-answered.
2. Read `neko_core::checkpoints::row(db, ticket_id)`. **G40** If the ticket is gone or `row.runs` has no run with a non-empty
   `start.commit`: do not answer; set `b.deadline_off = true` and push one journal line "Waited <h> h for you. No checkpoint exists to undo to, so the default was not taken."; save. (Once, not every tick.)
3. `undo_run = row.next_run + 1` (**G37**): the next run's start checkpoint is the state of the folder right before the post-answer work, which is the
   "pre-answer checkpoint". It does not exist yet; it is created when the answer's turn begins.
4. Apply the answer through the shared helper extracted from `GoalCommand::Answer`:
   `fn apply_answer(db: &Db, goal: &mut Goal, text: &str, now: i64, journal_note: String) -> Result<String, String>`
   (takes `goal.blocked`, sets `Running`, `in_turn = false`, `reason`, closes the Inbox item with `routines::close_goal_item`, pushes the journal
   entry with the given note, saves). The note here is exactly `Took the default after <h> h: <default answer>` (`h` is the configured hours).
   It is done inside `commit(db, &id, |g| g.status == Blocked && same question, ...)` so a person's answer racing the tick wins.
5. In the same commit set `goal.default_taken = Some(DefaultTaken { at_ms: now, after_hours: h, question, answer, undo_run, expires_ms: now + 7 * 86_400_000, inbox_seq })`.
6. The goal's next tick issues the turn as after any answer.

**Expiry.** `tick_concurrently` clears `default_taken` when `now >= expires_ms` (journal line "The undo for the default answer expired"). Checkpoints themselves expire at 14 days (`checkpoints::KEEP_MS`), so a 7-day target is always inside them; `enforce_cap` (50 refs per ticket) can still drop an old run: then `UndoDefault` says so (below).

**`UndoDefault { goal }`** (new `fn undo_default(&self, goal_id)`, handled at the top of `goal_command` before it takes the `db` guard, because
`checkpoint_command` locks `self.db` itself; it takes `GOAL_LOCK` for its whole length):
1. Load the goal; refuse unless `default_taken` is `Some` and not expired ("There is no default answer to undo", "That undo is no longer available (kept 7 days)"), and the goal is not finished ("That goal already finished").
2. If a run numbered `undo_run` exists in `checkpoints::row(...)`, call `cp::preview_undo(&self.db, ticket, &dir, UndoOptions { run: undo_run, through_later: true, ..Default::default() with pull_request_open }, running)`; a refusal returns its `checkpoint:<code>: <message>` text unchanged, **before anything changes** (the UI offers the existing remedies: stop the run, snapshot, revert commit; those are the checkpoint requests).
3. Pause the goal (`Paused`, reason "Undoing the default answer."), then if the run exists call `self.checkpoint_command(CheckpointCommand::Undo { task_id, run: undo_run, through_later: true, snapshot_unsaved: false, revert: false })` (a safety snapshot is taken first, as for any undo). If the run does not exist yet (no turn has started since the answer), only the goal state is restored (**G37**). If it existed but is gone, return "The checkpoint for that run is no longer kept, so the files cannot be restored" and leave the goal Paused with that reason.
4. Restore the question: `goal.blocked = Some(Blocked { question, default_answer: answer, inbox_seq: <new>, asked_ms: now, real_default: true, deadline_off: true })` raised again through the same Inbox call `block()` uses (`routines::raise_goal_item`), `status = Blocked`, `in_turn = false`, `default_taken = None`, journal "Undid the default answer: <answer>" (the ticket also gets the checkpoint event from `checkpoint_command`). `deadline_off` stops the same question from being auto-answered again.

### (c) Web UI

- `web/src/work/goals.tsx::GoalAnswer` (shown for Blocked goals): under the answer row, a quiet line "If you do not answer, Neko takes the default after [Off ▾]" with `Select` options `Off`, `1 hour`, `4 hours`, `8 hours`, `24 hours`, `3 days` (the stored number of hours; a value outside the list shows as "N hours"), sending `command("Goals", { SetDefaultAfter: { id, hours } })`. A deadline is only offered when `blocked.real_default`; otherwise the line reads "Neko has no default for this question, so it will wait for you." The line is also in the goal's opened detail (`Ledger`) for any live goal, because the hours can be set before it blocks.
- After a deadline answer, the goal line's opened detail (`Ledger`) shows a row "Neko took the default after 8 h: <answer>" with the button **Undo** (destructive, `variant="outline"`), visible while `default_taken` is present and not expired, and in the goal's right-click menu as "Undo the default answer" (`goalActions` gets a `destructive` entry). Clicking opens the confirm dialog: title "Undo the default answer?", body "Neko will put the folder back to how it was before it acted on the default, and ask you the question again. Work done since will be undone.", buttons `Cancel` / `Undo the default`. A refusal (`checkpoint:` codes) shows under the row with the existing wording the Runs list uses (`web/src/work/Runs.tsx`).
- Journal line "Took the default after 8 h: <default>" appears in the existing journal list (it is a normal `JournalEntry.note`).
- Empty/error: the toast the page already uses for `Goals` command errors; no extra empty state.

### (d) Rune follow-up (described only)

`apps/neko-rune/src/view/work.rs` goal line: the same select inside the answer card and an Undo action inside `confirm(` (it is destructive); `UndoDefault` and `SetDefaultAfter` are `Goals` sub-commands, which the allow-list counts when the web's sources name them.

### (e) Tests

1. A blocked goal with `default_after_hours: Some(1)`, a judge-given default and a start checkpoint is answered 1 hour after `asked_ms`, with the exact journal line "Took the default after 1 h: <default>".
2. Before the deadline nothing happens; with `default_after_hours: None` nothing ever happens.
3. A blocked goal whose default is the generic filler is never auto-answered.
4. With no checkpoint the goal stays Blocked, `deadline_off` is set, and one journal line explains it (not repeated on the next tick).
5. A person's `Answer` arriving first wins; the tick then does nothing.
6. `default_taken.undo_run` equals `next_run + 1` at answer time and `expires_ms` is 7 days out.
7. `UndoDefault` after the next turn ran restores the folder to that run's start and returns the goal to Blocked with the same question and `deadline_off`.
8. `UndoDefault` before any turn started restores only goal state.
9. `UndoDefault` refuses with the `checkpoint:` code when the ticket is busy, and changes nothing (goal not paused, folder untouched).
10. `UndoDefault` refuses after expiry, and the tick clears an expired target.
11. `UndoDefault` on a finished goal is refused.
12. `/goal fix it --check "make test" --default-after 8` stores 8; `SetDefaultAfter { hours: Some(0) }` and `{ hours: Some(200) }` are refused; `None` turns it off.
13. A goal a model proposed ignores `default_after_hours`.
14. Old goal rows without the new fields load and behave as before.

### (f) Risks

- The undo restores **files** to the start of the first run after the answer, which also undoes any later runs (`through_later: true`). If a pull request was opened or commits were pushed meanwhile, the existing refusals (`pushed`, `foreign_commits`) apply and the goal is left as is; the UI shows the refusal. The remedy the checkpoint feature already offers is a revert commit.
- `enforce_cap` (50 refs per ticket) can drop the run's start before 7 days on a very busy ticket; the undo then says so (step 3).
- A goal that is already Done before the person looks cannot be undone (step 1); the receipt still records the "Took the default" line in its journal.
- Auto-answering is an action Neko takes on the person's behalf. It is opt-in per goal, only for judge-given defaults, and checkpointed; the deadline can never be shorter than 1 hour.
- Time zone: all times are epoch ms; no local-time logic.

---

## F8. Goal packs

`pack.toml` may contain `[[goals]]` with `objective`, `check`, `turns`, optional `cadence`. Preview lists them, the digest covers
them, Install creates each goal paused, the person starts one with one click. Example pack `packs/goal-keep-green/`.

### (a) Files and names

- `crates/neko-core/src/packs.rs`: `Manifest` (new `goals: Vec<GoalDecl>`), `GoalDecl`, `parse_dir` (validation, the "declares more than Neko accepts" line, info), `PackInfo` building, `digest_of` (unchanged function; the new info fields are inside it), `power_changes` (goal texts), `Journal` (new `created_goals`), `preflight`, `apply` (new "goals" step after "routines", before the state row), `PackState` (new `goal_ids`), `Tx` rollback, `drop_leftovers`, `remove`, test `STEPS` (now 7: add `"goals"` after `"routines"`), a helper `goal_key(pack, objective)`.
- `crates/neko-protocol/src/packs.rs`: `PackContents` (new `goals: Vec<String>`), `PackInfo` (new `goal_texts: Vec<GoalText>`), new `GoalText`.
- `crates/neko-core/src/goals.rs`: new `pub fn delete(db, id)`; `new_goal` unchanged.
- `crates/neko-protocol/src/goals.rs`: `Goal` (new `origin`, `cadence`, `next_offer_ms`; B1 pre-declares them, see Batches).
- `crates/neko-daemon/src/workbench/goals.rs`: `goal_command::Resume` (create the ticket for a ticket-less goal), `start_goals`/tick (the cadence re-offer sweep).
- `crates/neko-core/src/digest.rs`: nothing (the digest of a pack is the pack digest above; the morning digest covers goals through the existing goal lines). 
- Example: `packs/goal-keep-green/pack.toml`.
- Web: `web/src/pages/setup/packs/ReviewPack.tsx`, `PackDetail.tsx`, `parts.tsx`, `web/src/setup/packs.ts` (types).

### (b) Format, fields, rows

`pack.toml`:

```toml
[[goals]]
objective = "Keep the project's tests passing. When they fail, find the cause, fix it, and run the check again."
check = "make test"          # optional; empty or "none" means no check
turns = 20                    # optional, 1..=200, default 20 (DEFAULT_MAX_TURNS)
cadence = { Daily = { hour = 9, minute = 0 } }   # optional; same Cadence type routines use
```

```rust
struct GoalDecl { objective: String, #[serde(default)] check: String, #[serde(default)] turns: Option<u32>, #[serde(default)] cadence: Option<Cadence> }
// neko-protocol/src/packs.rs
pub struct GoalText { pub objective: String, pub check: String, pub turns: u32, pub cadence: String /* "Daily 09:00" or "" */ }
// PackInfo:  #[serde(default, skip_serializing_if = "Vec::is_empty")] pub goal_texts: Vec<GoalText>
// PackContents: #[serde(default, skip_serializing_if = "Vec::is_empty")] pub goals: Vec<String>   // the goal keys
// Goal (protocol/goals.rs), all #[serde(default)]:
pub origin: Option<String>,          // "pack:<pack>:<key>"
pub cadence: Option<Cadence>,        // re-offer schedule (neko_protocol::routines::Cadence)
pub next_offer_ms: Option<i64>,
```

Both new `PackInfo` / `PackContents` fields are skipped when empty, so the digest of a pack without goals is **byte-identical** to
today's and already-approved packs do not turn into "Changed on disk". A pack with goals adds the warning
`"Declares goals whose checks run shell commands in a workspace folder"` (in `info.warnings`, so inside the digest).

**G41** A goal's stable key is `goal_key(pack, objective)`: the first 6 words of the objective, lowercased, non-alphanumerics
dropped, joined with `-`, cut at 48 characters; two goals with the same key in one pack are refused ("The goal “...” repeats another goal's name").
The goal's `origin` is `pack:<pack>:<key>` (a `:` can be in neither part, like routine ids).

Validation in `parse_dir`: at most 8 goals (added to the existing `m.mcp.len() > 8 || ...` line); `objective` 1..=4096 bytes and
`check` <= 2048 bytes (the `MAX_OBJECTIVE`, `MAX_CHECK` of `neko_protocol::goals`); `turns` 1..=200; cadence valid by the same rule as a routine's
(`Daily`: hour < 24, minute < 60; `Every`: minutes >= 15 and the hour bounds).

**Install** (`apply`, new step "goals" run after "routines" and before the state row; `(env.hook)("goals")?` follows it):
- **G44** The workspace is the one chosen for routines (the one whose folder is home, else the first). With no workspace at all,
  `preflight` refuses: "Add a workspace before installing a pack that has goals; nothing was changed".
- **G42** For each declared goal with no existing goal of the same `origin`: `neko_core::goals::new_goal(workspace, objective, check, "", turns, now)`, then
  `status = Paused`, `paused_reason = "From the pack <pack>. Read the check, then Start."`, `ticket_id = None`, `turns = 0`, `in_turn = false`,
  `origin`, `cadence`, saved with `goals::save` (not `create`: `create` is for goals with a ticket). Id pushed to `tx.j.created_goals` before the write, and to `goal_ids` in the state row.
  No ticket exists yet, so nothing can run and nothing is queued.
- Rollback (`Tx`/`Journal.created_goals`): `goals::delete` each created id; `recover` finishes or rolls back from the journal like routines.
- **Update (G45)**: a new declared goal is added paused; a goal that already exists (same `origin`) is left exactly as it is (the person may have started or changed it). A change to
  any goal text counts in `power_changes` as "Goal <key> changed" and needs approval (a check is a shell command), exactly as an enabled routine's text change does.
- **Remove (G45)**: `goal_ids` goals that are active are first `Stop`ped (status `Stopped`, journal "Stopped: the pack was removed"), then deleted if they have no ticket or a finished status; goals with a ticket in flight keep their record as `Stopped` and are listed as orphans `goal:<id>` (`CleanUp` deletes them). `neko_core::goals::delete` removes the row, the index entry, the clock entry and the log row (only when no other goal shares the subject).

**Start** is `GoalCommand::Resume { id }` (existing). New behaviour in `goal_command::Resume` for a paused goal with `ticket_id == None`:
create the ticket exactly as `goals::create` does for a goal without a ticket (`CreateTask { workspace_id, title (first line of the objective, 80), goal: text with "Done when this passes in the ticket's folder: <check>" }`, `task.request = neko_chat::ticket_request(objective)`, `start_when_planned.insert(task_id)`), set `ticket_id`, `turns = 1`, `in_turn = true`, `status = Running`, journal "Started from the pack <pack>"; refuse with the existing "A Git repository is required" style error if the ticket cannot be made. This is the "one click". The check runs only now, after the person pressed Start (consistent with "its shell check is never run until the person resumes").

**Cadence (G43).** A goal with `cadence` and `status == Done` and `next_offer_ms <= now` (set when it finished: `next_due_local(cadence, 0, finished_ms)`; the sweep sets it if None) is **re-offered**: a fresh paused ticket-less clone (new id, same `origin`, `cadence`, objective, check, turns; `next_offer_ms = None` on the clone) and one Inbox item "Goal due again: <objective first 100>" (default "Start it", linked `goal:<new id>` through `routines::raise_goal_item`); the finished goal's `cadence` is cleared so it re-offers once. Nothing starts without the click. A goal that is `Stopped` does not re-offer. The sweep runs in the goal loop's tick (`start_goals`) once per tick over `load_all`, cheap.

Digest. "The digest covers them" is the pack digest: `goal_texts` is inside `PackInfo`, so `Preview`'s `digest` changes if any goal's objective, check, turns or cadence changes, `Install` recomputes it from the fetched tree, and "Changed on disk" fires if an installed pack's `pack.toml` changes a goal.

### (c) Web UI

- **ReviewPack** (`web/src/pages/setup/packs/ReviewPack.tsx`, the preview shown before Install): a block "Goals" (hidden when none) after "Routines": per goal a row with the objective (2 lines max, full text in the `title`), the check in `<code>` (or "No check: done needs a file or a link that exists"), "up to 20 turns", the cadence in words ("Offered again Daily 09:00"), and the sentence "Installed paused. Nothing runs until you press Start." The warning "Declares goals whose checks run shell commands in a workspace folder" appears with the other warnings.
- **PackDetail** (installed): block "Goals" listing the pack's goals from the snapshot (`goals.filter(g => g.origin?.startsWith("pack:" + name + ":"))`) each as the existing `GoalLine` plus, for a Paused goal with no ticket, a primary button **Start** (sends `command("Goals", { Resume: { id } })`). Errors: the toast; "That workspace has no Git repository" style message from the daemon.
- **Packs list/summary** (`parts.tsx`): "2 goals" beside "3 tools · 1 routine".
- Empty state: none (the block hides). Error state: the page's existing `ErrorNote` for pack commands; Install failure text for the no-workspace case is the daemon's message above.

### (d) Rune follow-up (described only)

`apps/neko-rune/src/view/setup/packs.rs`: the review sheet lists goals with their checks and the Start button on an installed pack's goal
(`Resume` is a `Goals` sub-command already allowed). `PackInfo::goal_texts` is read by the existing preview types.

### Example pack: `packs/goal-keep-green/pack.toml`

```toml
name = "goal-keep-green"
title = "Keep it green"
version = "0.1.0"
description = "One goal that keeps a project's tests passing. It installs paused; you read the check and press Start."
homepage = ""
licence = "MIT"

[[goals]]
objective = "Keep the tests of the project in this workspace passing. When they fail, find the cause, fix it, and run the check again."
check = "make test"
turns = 20
cadence = { Daily = { hour = 9, minute = 0 } }

[permissions]
summary = [
  "Adds one goal, paused, that you start yourself",
  "When you start it, runs the check command (make test) in the workspace's folder and lets an agent edit files there to make it pass",
  "Offers the goal again every morning at 09:00; it never starts by itself",
]
```

No real repository, no names. The check is a placeholder a person edits by editing the goal's check after install, or by forking the pack.

### (e) Tests

1. A pack with `[[goals]]` parses; one without parses with `goal_texts` empty and **the same digest as before** (a stored digest from a fixture pack is unchanged).
2. Changing a goal's `check` changes the preview digest; `Install` with the old digest is refused.
3. Install creates one Paused goal per declaration with `ticket_id: None`, `turns: 0`, the right `origin`, `paused_reason`, and no ticket in the snapshot.
4. A crash injected after the "goals" step (`STEPS` now includes `"goals"`) rolls the goals back: none remain.
5. Install with no workspace is refused before anything changes.
6. More than 8 goals, `turns: 0`, `turns: 201`, an oversize check, a repeated key and an invalid cadence are refused.
7. Update adds a new goal paused and leaves an existing (started) goal untouched; a changed goal text needs approval.
8. Remove stops active goals and deletes the rest; an in-flight one is kept as an orphan and `CleanUp` removes it.
9. `Resume` on a pack goal with no ticket creates the ticket (`start_when_planned`), sets `turns: 1`, `Running`, and the first check runs only after that.
10. `Resume` on a pack goal in a workspace without a Git repository fails with the existing message and leaves the goal Paused.
11. A Done goal with a `cadence` is re-offered once as a new paused clone plus one Inbox item; a Stopped one is not; nothing starts.
12. `goals::delete` removes row, index, clock and the shared log only when unshared.
13. The shipped `packs/goal-keep-green` previews cleanly (no warnings other than the goals one) and installs in a fixture.
14. The pack's preview warning for goals is inside the digest.

### (f) Risks

- A goal check is a shell command that runs with the person's own environment once they press Start (documented behaviour for any goal). The pack path is the first time someone else's text becomes a check, which is why the digest covers it, the preview shows it in full, install is paused, and any change needs approval.
- `Goal.cadence` re-offer creates one new paused goal per due time; if the person never answers, only one open clone exists at a time (the sweep does nothing while a paused clone of the same `origin` exists).
- Pack goals are workspace-bound to the home/first workspace (G44); a person with several workspaces may want another one: they can stop and recreate with `/goal` (no re-targeting in this feature).
- Changing the install `STEPS` array breaks the crash-injection tests until they are updated (listed in (e)).

---

## Build batches

Order inside B1: F5 (ledger) -> F4 -> F6 (reads the ledger) -> F7. Order of batches: B1, then B2, then B3 (B2 rebases on B1; B3 is independent but rebases last).

### B1: F4, F5, F6, F7 (daemon, shared goals module)

Files touched:
- `crates/neko-protocol/src/routines.rs` (RoutinePolicy incl. the F1 `mode` type, RoutineState, RoutineRun), `goals.rs` (receipt, default fields, **and the three F8 inert fields `origin`, `cadence`, `next_offer_ms`** so B2 does not re-edit the struct), `digest.rs`, `lib.rs` (UsageHistory, UsageCsv + ledger wire types)
- `crates/neko-core/src/db.rs`, `lib.rs` (`usage_ledger`, `redact`), `usage_ledger.rs` (new), `redact.rs` (new), `routines.rs` (helpers), `goals.rs` (MAX_JOURNAL, `attach`, receipt_markdown, validate/new_goal), `digest.rs`
- `crates/neko-daemon/src/routine_guard.rs` (new), `routines.rs` (`run_one`/`run_body` signature and call site, `edit` validation, ledger write), `workbench.rs` (`execute`), `workbench/goals.rs`, `workbench/parking.rs`, `server.rs` (two arms), `main.rs` (module)
- `web/src/pages/Inbox.tsx`, `web/src/setup/routine-status.ts`, `web/src/pages/setup/Usage.tsx`, `web/src/work/goals.tsx`, `web/src/work/digest.tsx`
- Every `Routine` literal site listed in section 0.
- Docs: `docs/knowledge/neko-runs-the-day.md` (F4), `neko-maintains-itself.md` (F5 spend note), `goals-that-keep-working.md` (F6, F7), `morning-digest.md` (receipt link).
- Tests: `crates/neko-daemon/src/workbench/goals_tests.rs`, `goals_more_tests.rs` (extend), new `routine_guard` tests in the module, `crates/neko-core/src/usage_ledger.rs` tests.

### B2: F1, F2, F8

Files touched:
- `crates/neko-protocol/src/crew.rs` (CrewFileKind, CrewFiles), `lib.rs` (CrewFile, CrewFiles), `packs.rs`, `skills.rs` (SetCuratorMode, Curation)
- `crates/neko-core/src/crew_files.rs` (new), `lib.rs`, `routines.rs` (RECORD_NOTE, record_line), `neko_chat.rs` (one argument), `packs.rs`, `goals.rs` (`delete`), `skills/curator.rs` (`finish_dry`), `tests/prompts.rs` + `tests/prompts/routine-record.md`
- `crates/neko-daemon/src/routine_record.rs` (new), `routines.rs` (`run_body` call site, `orders` append), `workbench.rs` (the `rules` line), `workbench/goals.rs` (`Resume`, cadence sweep), `workbench/skill_curator.rs`, `server.rs` (two arms)
- `packs/goal-keep-green/pack.toml` (new)
- `web/src/pages/Inbox.tsx` (mode control), `web/src/setup/routine-status.ts` ("Recorded"), `web/src/pages/setup/tools/Skills.tsx`, `web/src/pages/setup/Workspaces.tsx`, `web/src/pages/setup/packs/ReviewPack.tsx`, `PackDetail.tsx`, `parts.tsx`, `web/src/setup/packs.ts`
- Docs: `docs/knowledge/neko-runs-the-day.md` (record mode), `neko-maintains-itself.md` (curator record), `packs.md` (goals), a new section in `the-chat-directs-the-crew.md` or `neko-runs-the-day.md` for crew files.

### B3: F3 alone

Files touched:
- `crates/neko-protocol/src/crew.rs` (CrewRole::Outside, OutsideLevel, TOOLS + goal_journal, wire types), `lib.rs` (five requests, two responses)
- `crates/neko-core/src/mcp_host/bridge.rs` (`crew_definitions` public, `goal_journal`, `crew_tool_json`)
- `crates/neko-daemon/src/outside.rs` (new), `web.rs` (`read_request` and helpers `pub(crate)`), `main.rs` (module + start), `server.rs` (arms + start), `workbench/crew.rs` (Outside arms, `goal_journal`)
- `web/src/pages/setup/settings/Outside.tsx` (new), `registry.ts`, `web/src/pages/setup/Settings.tsx`
- Docs: new `docs/knowledge/outside-agents.md`, one line in `CLAUDE.md`'s index, a row added to the tools table in `the-chat-directs-the-crew.md`.

### Files touched by more than one batch (merge risk)

| File | Batches | Risk and how to contain it |
| --- | --- | --- |
| `crates/neko-protocol/src/routines.rs` | B1, B2 | F1's `mode` and F4's fields share `RoutinePolicy`. B1 defines the whole struct including `RoutineMode`; B2 adds nothing here but `recorded` use. Land B1 first. |
| `crates/neko-protocol/src/goals.rs` | B1, B2 | F6/F7 and F8 all extend `Goal`. B1 adds F8's three fields as inert data; B2 only reads them. |
| `crates/neko-protocol/src/lib.rs` | B1, B2, B3 | Each appends enum variants at the end of `Request` and `Response`. Put each batch's variants under its own `// F5`, `// F2`, `// F3` comment to avoid one conflict hunk. |
| `crates/neko-protocol/src/crew.rs` | B2, B3 | B2 adds `CrewFileKind`/`CrewFiles` at the end; B3 edits `CrewRole` and `TOOLS`. Different hunks. |
| `crates/neko-core/src/lib.rs` | B1, B2 | One `pub mod` line each (`usage_ledger`, `redact` / `crew_files`). Trivial but certain to conflict textually. |
| `crates/neko-core/src/routines.rs` | B1, B2 | B1 helpers (cap, standing thread); B2 `RECORD_NOTE`. Separate functions; both touch the `seeds()`/literal sites only through the section-0 list (B1). |
| `crates/neko-core/src/goals.rs` | B1, B2 | B1 edits `MAX_JOURNAL`, `attach`, `new_goal`; B2 adds `delete`. Separate functions. |
| `crates/neko-core/src/packs.rs` | B1, B2 | B1 only edits the one `Routine { .. }` literal at `:1629` (and tests `:2488`, `:2539`) to add `policy`; B2 does the real work. Sequence B1 first. |
| `crates/neko-daemon/src/routines.rs` | B1, B2 | Both change `run_body` (F4 guard at its top, F1 record branch at the agent step, F4 standing thread in the memory argument, F2 `orders` append) and B1 changes `run_one`'s signature. Contain each in its own module (`routine_guard.rs`, `routine_record.rs`) with one call site; B2 rebases on B1. |
| `crates/neko-daemon/src/workbench/goals.rs` | B1, B2 | The heaviest overlap: B1 touches `step`, `evaluate`, `block`, `goal_command` (Answer refactor, MarkDone, UndoDefault, SetDefaultAfter), `tick_concurrently`; B2 touches `goal_command::Resume` and the loop's tick. Land B1, then B2 on top. |
| `crates/neko-daemon/src/server.rs` | B1, B2, B3 | New match arms in `handle_request` (and B3's listener start near the web bridge start). Add each arm as its own `request @ (...)` block. |
| `crates/neko-daemon/src/main.rs` | B1, B2, B3 | One `mod` line each. |
| `crates/neko-daemon/src/workbench.rs` | B1, B2 | B1 edits `execute`; B2 edits the one `let rules = ...` line. Different functions. |
| `web/src/pages/Inbox.tsx` | B1, B2 | One dialog, `RoutineEditor`: B1 adds the "Guard rails" block; B2 adds the mode control above "What to do". Land B1, then B2. |
| `web/src/setup/routine-status.ts` | B1, B2 | Two new branches (skipped, recorded) in `routineStatus`. |
| `docs/knowledge/neko-runs-the-day.md`, `neko-maintains-itself.md`, `goals-that-keep-working.md`, `the-chat-directs-the-crew.md` | B1, B2, B3 | Docs only; append a dated section each, never rewrite. |

No file is edited by all three batches except `protocol/lib.rs`, `server.rs` and `main.rs`, where each batch adds its own isolated block.

---

## Every GAP-CHOICE

| Id | Feature | The smallest choice made |
| --- | --- | --- |
| G1 | F7 | `UndoDefault { goal }` and `SetDefaultAfter` are sub-commands of `GoalCommand` (sent through `command("Goals", ...)`), not top-level requests. |
| G2 | F1 | Record mode runs the agent read-only with no MCP servers, no pack servers, no Chrome and no computer use. |
| G3 | F1 | Record mode drops `needs_you`, ignores `resolved`, saves no memory, leaves `quiet_streak`; the run stores exactly one "Would have: ..." line. |
| G4 | F1 | `mode` lives in the routine definition row (`routines_v1`, inside `Routine::policy`); that is "the routine's own row" (per-routine rows hold state and runs). |
| G5 | F1 | The curator's mode is a quiet row `neko_curator_mode_v1` with `SkillCommand::SetCuratorMode`; record writes `neko_curator_would_v1` (last 20) and `Curation::would_have`. |
| G6 | F2 | A read request `CrewFiles { workspace }` and `Response::CrewFiles` (the decision names only the write). |
| G7 | F2 | "Agents read the plan" = one prompt line with the file's path; `plan.md` is written 0444 as a speed bump. No Neko tool writes it. |
| G8 | F2 | The crew section is appended to the end of the rulebook block in ticket prompts (own heading), to the standing orders in routine prompts, and to the rulebook argument in chat prompts (only when the chat has a workspace scope). |
| G9 | F2 | Limits are 16,384 bytes of UTF-8, no NUL; empty text clears the file after keeping `<name>.prev` (`rules.md.prev`). |
| G10 | F3 | `SetOutsideAgentLevel { client, level }` and `SetOutsideAgents { enabled, port }` exist (the decision has no way to change a level or switch the endpoint on). |
| G11 | F3 | Five wrong codes burn the active code; pairing then answers 429 for 60 s. |
| G12 | F3 | The `Host` header is also checked (`127.0.0.1:<port>` / `localhost:<port>`), as the web bridge does. |
| G13 | F3 | An outside client sees the active profile's workspaces; its `create_task` has no `request`, waits for approval, ignores `goal`, needs an explicit workspace unless there is one, and counts toward the delegation cap under origin `outside:<id>`. |
| G14 | F3 | `cancel_task` at `full` runs as the full chat's does (the codebase has no confirmation step for it); `undo_run` returns the existing preview and the person confirms with the Undo button. |
| G15 | F3 | The read tool for journals is a new crew tool named `goal_journal`, so `TOOLS` has 12 entries. |
| G16 | F3 | The call journal is a deferred row `outside_journal_v1` (last 500), last-used times a separate row `outside_seen_v1`. |
| G17 | F3 | "The web UI's origin" is `http://127.0.0.1:<web port>` / `http://localhost:<web port>` (the web bridge's port, not 4748). |
| G18 | F3 | Enabling, disabling and a port change take effect within 5 seconds (the listener thread polls the settings). |
| G19 | F4 | `daily_cap` 0 means no cap; the stored default for existing and seeded routines is 0 and the decision's 10 is applied to newly created Custom and pack routines. |
| G20 | F4 | Consecutive identical skips coalesce into one run row with a count, so skips cannot push real runs out of the 50 kept. |
| G21 | F4 | "Run now" bypasses the cap and `run_only_when` (the person is asking; same rule as the spend ceiling). |
| G22 | F4 | A `standing` thread is the last 5 runs' summaries placed in the prompt's `memory` object (Claude workers cannot resume sessions). |
| G23 | F4 | The gate runs in the spawned run thread, via `sandbox-exec` with `(deny network*)`, 10 s through `output_within`, with a cleared environment plus PATH, HOME, TMPDIR, LANG, CI. |
| G24 | F4 | The cap counts run starts in `RoutineState::starts_ms` over a rolling 24 h. |
| G25 | F5 | `estimated` means the provider reported nothing; tokens and cost stay NULL. |
| G26 | F5 | An extra unique `run_key` column makes "written once" hold. |
| G27 | F5 | Only ticket runs (`execute`) and routine runs are recorded; Home chat turns and one-shot judge/curator calls are not. |
| G28 | F5 | CSV export is a request answered with text; the web builds the file; leading `= + - @` cells are guarded. |
| G29 | F5 | Pruning runs from `parking_tick`, at most once a local day (row `usage_pruned_v1`). |
| G30 | F6 | `MAX_JOURNAL` rises to 50 in storage; snapshot readers still get 20 (6 when finished). |
| G31 | F6 | Redaction is a hand-written word scan (no regex dependency), over-redacts any `x=y`, and also covers `ghp_`/`sk-`/`xox`/`AKIA` tokens and PEM blocks. |
| G32 | F6 | "Last 20 lines" means within the 1,200-byte output tail the ledger already stores. |
| G33 | F6 | `MarkDone` writes a receipt too; a failed write never blocks the goal and is retried up to 3 times by the tick. |
| G34 | F6 | No `ArtifactsChanged` event is broadcast from the goal thread. |
| G35 | F6 | `DigestLine` gets an optional `link` (`artifact:<id>`); the plain-text digest stays unlinked. |
| G36 | F7 | Only a default the judge actually supplied counts ("a question that has a default"); the generic filler never triggers a deadline answer. |
| G37 | F7 | The undo target is run number `next_run + 1` at answer time (the next run's start checkpoint); if no run began yet, undo restores only goal state. |
| G38 | F7 | `UndoDefault` previews first, pauses, restores with `through_later: true`, then returns the goal to Blocked with the same question and no further auto-answer. |
| G39 | F7 | `default_after_hours` is set by `/goal --default-after`, the `Create` command field and `SetDefaultAfter`; range 1 to 168. |
| G40 | F7 | With no start checkpoint nothing is answered; one journal line says so and the deadline is switched off for that question. |
| G41 | F8 | A pack goal's stable key is the first six words of its objective (`goal_key`); `origin` is `pack:<pack>:<key>`. |
| G42 | F8 | Installed goals are ticket-less and paused; pressing Start (`Resume`) creates the ticket then. |
| G43 | F8 | A goal's `cadence` re-offers it as a fresh paused clone plus one Inbox item; nothing ever starts by itself. |
| G44 | F8 | Goals go to the routines' workspace choice (home folder's, else first); no workspace refuses the install; at most 8 goals per pack. |
| G45 | F8 | Update only adds goals and requires approval for changed goal text; Remove stops, then deletes, with in-flight goals kept as orphans. |

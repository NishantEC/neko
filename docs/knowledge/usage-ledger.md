# The usage ledger (2026-11-20)

Spec: `docs/design/super-agent-features-2026-11.md`, F5. Table `usage_events` (created by `Db::migrate`'s
`CREATE TABLE IF NOT EXISTS`, which is the migration): one append-only line per finished agent run, kept 365 days.
Code: `crates/neko-core/src/usage_ledger.rs`, `Db::record_usage_event` / `usage_events_since` /
`usage_events_for_task` / `prune_usage_events`, wire types `crates/neko-protocol/src/ledger.rs`.

- **What is a row.** A ticket run is one `execute` claim in `workbench.rs` (planning, build and review are inside it;
  a reply or a retry is another claim, so another row), recorded whatever way it ends. A routine run is one run an
  agent made (not a skip, not a usage-limit hold, not one that failed before any agent started). Not recorded: Home
  chat turns and one-shot calls (the goal judge, the curator), so ledger totals sit below "Spend today" on heavy chat
  days. The page says "agent runs".
- **`run_key`** (`ticket:<task>:<started_ms>` or `routine:<run id>`) is unique and the insert is `INSERT OR IGNORE`, so
  a run recorded twice is one row.
- **`source`.** `reported` when the provider gave a cost or tokens (the sum of the claim's `USAGE` events from its
  start); `estimated` when it reported nothing (Codex reports no price). Tokens and cost then stay NULL: the ledger
  never invents numbers. An empty provider in `AgentRuntime` is the signed-in Codex account and is stored as `codex`.
- **Writes are not changes.** Nothing `load` reads lives in the table, so writes do not bump `data_revision` or the
  `ContentStamp` and wake no poller. Do not put a ledger value into the snapshot.
- **Pruning.** `parking_tick` -> `usage_housekeeping` -> `prune_if_due`, once per local day (quiet row
  `usage_pruned_v1` holds the day).
- **Requests.** `UsageHistory { days }` and `UsageCsv { days }` (read-only, `days` clamped to 1..=365). History is
  computed in Rust from whole local days ending today (the first bucket is that day's midnight, so a range is whole
  days, zeros included). CSV columns `time,provider,model,input_tokens,output_tokens,cost_usd,source,duration_s,task_id,kind`;
  a cell that starts with `=`, `+`, `-`, `@`, tab or CR gets a leading `'`, and cells with a comma, quote or newline are quoted.
  The web builds the download from the `csv` text.
- Web: `HistorySection` in `web/src/pages/setup/Usage.tsx`. The Rune client follows later (the allow-list mirror picks
  the two requests up from the web's sources).

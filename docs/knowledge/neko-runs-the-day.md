## Neko runs the day: scopes, routines, the Inbox (2026-10-29)

Owner: Neko should "take care of all of these" (the Paseo digest, message
watch and PR sweep), keep "different MCPs for different scope directories",
and "there is no restriction. It can do whatever it wants". Design:
`docs/superpowers/specs/2026-10-29-neko-runs-the-day-design.md`.

- **Scopes** (`neko_core::scopes::discover`): a top folder under
  `~/Documents` is a scope when it has its own `.mcp.json`, two or more repos
  with MCP config, or a `~/.claude.json` project server no other folder has
  (that is what makes `web-v2` and `ui-kit` scopes). Names come from
  `NAMES` (`work` → Acme, `clientco` → Client A, …). It also reports
  setup mess: a server copied into 3+ repos, or a repo copy of a scope
  server. `settings.local.json`'s `enabledMcpjsonServers` only *enables* the
  parent's servers, so it is not a duplicate. Each scope becomes a workspace
  (created once by `routines::ensure_seeded`). Every other workspace,
  "Asha" (`/Users/example`) included, is a scope of its own.
- **Connections are not imported, and each run is scoped** (2026-10-29,
  owner: Gmail comes through an MCP server per scope, never the claude.ai
  connectors, which are not scoped). A routine runs Claude Code in the scope
  folder via `native_runner::run_claude_scoped`: `--strict-mcp-config` with a
  config built by `scopes::mcp_config` (each folder's `.mcp.json`, then its
  `~/.claude.json` project entry; the home scope alone also gets the global
  `~/.claude.json` servers). It is written 0600 into the run's private
  scratch folder and deleted with it. Existing sign-ins still work (verified:
  slack-client-a loaded this way read the profile without re-auth). Email and
  calendar are `workspace-mcp` with `--tools … gmail calendar`: `google-client-a`,
  `google-work`, and `google-personal` (home scope, `~/.google/`, registered
  with `claude mcp add-json -s local` from `~`). A global server such as
  `sentry` therefore reaches only the home scope; put it in a scope's
  `.mcp.json` for that scope's routines to use it.
- **Routines** (`neko_core::routines`, daemon `routines.rs`): three per scope,
  seeded once: Messages (every 20 min for Client A/Acme, 60
  elsewhere, 08–23, back-off ×2/×4 while quiet), Morning brief (10:30) and PR
  sweep (11:00). The reply must end in a ```json block (`quiet`, `did`,
  `needs_you`, `memory`); `memory` is handed back next run. Rows:
  `routines_v1` (definitions and standing orders), `routine_state_v1:<id>`,
  `routine_runs_v1:<id>` (last 50) and `routine_seq_v1`. None of them is in the
  workbench snapshot. The runner wakes every 30 s, runs one at a time, skips
  while the machine steward holds, and has a 15-minute timeout. Seeding
  schedules first runs by cadence (no burst at install), and runs per
  workspace (`Store::seeded_workspaces`), so a scope folder added later gets
  its routines. A repeating routine never starts outside its hours (a night
  wake waits for 08:00); Run now (`run_requested`) runs even a paused
  routine, or queues behind a run in flight. A run's writes use
  `Db::set_setting_quiet`, so they do not wake `WorkbenchIfChanged`.
- **Failures reach the owner:** a Daily routine's first failure, or a
  repeating one's second in a row, becomes a needs-you item (so a
  notification). A failed Daily routine retries once after 30 min. Whatever
  fails, `run_one` never leaves a routine marked running. `memory` over 16 KB
  is not kept, and instructions (16 KB) and standing orders (8 KB) are
  bounded, so the prompt cannot outgrow `MAX_PROMPT`. Runs with open items
  are kept past the 50-run trim.
- **Polling is cheap:** scopes read only workspaces and folders
  (`workbench::load_workspaces`), and discovery is cached on the mtimes of
  `~/.claude.json` and `~/Documents` (10 min at most). A half-written
  `~/.claude.json` keeps the previous result.
- **What the first day showed (2026-10-30):** the personal scope reported
  work-org PRs (its "my PRs" crossed orgs), so its hint now limits it to
  repositories example-owner owns; two routines raised the same item, so an item
  whose title matches an open one in the same workspace is dropped
  (`open_titles`); the PR sweep listed conflicts instead of rebasing, so it
  is told to work in a /tmp worktree, resolve clear conflicts, test, push and
  request reviewers from history. Briefs and sweeps get 30 minutes
  (`LONG_RUN_TIMEOUT`). Seeded instructions are versioned
  (`INSTRUCTIONS_VERSION`) and rewritten when it rises.
- **Open items in the prompt** (2026-11-01): exact-title dedupe let the same
  problem through in other words ("Enable Gmail API" three ways, Jira twice;
  35 open items, 13 repeats or stale). Each run's prompt now lists the
  scope's open items (`open_items`, ≤40) and says never to report them again;
  the reply's `resolved` list closes items it finds no longer true
  (`resolve_items`, matched by title, across the workspace).
- **Browser** (2026-10-30): `Routine::browser` (default on) adds
  `--chrome`, so the run drives the owner's own Chrome through the Claude in
  Chrome extension (installed in the Default profile, native host
  `com.anthropic.claude_code_browser_extension`). It loads alongside
  `--strict-mcp-config` (verified: the only server was `claude-in-chrome`).
  Chrome is signed in to every account, so it is the one thing a scope shares;
  the prompt tells the run to use it for consent screens (approve while the
  MCP server that issued the link is still running) and web-only work, in its
  own tab. Chrome must be open. The owner's everyday browser is Dia, so his
  logins mostly live there, not in Chrome.
- **Computer use** (2026-10-30): `Routine::computer` (default on) adds
  Peekaboo (openclaw/Peekaboo, MIT, `brew install openclaw/tap/peekaboo`,
  4.9.7) to the run's scoped config as `peekaboo mcp`
  (`scopes::with_computer_use`). It is a capability of the Mac, not a data
  source, so every scope may have it. Permissions (Screen Recording,
  Accessibility, Event Synthesizing) are held by Peekaboo's Bridge app, so
  Neko needs no grants of its own. Verified from a strict-config run:
  permissions read, apps listed, front window read. Chosen over
  munim-computer-use, mac-computer-use, macos-computer-use, macpilot-mcp and
  mac-use-mcp (all far smaller); Codex's own computer use cannot be loaded by
  Claude Code. The prompt prefers MCP, CLI or browser first, and background
  actions on a snapshotted window. **Never credentials:** the first live use
  (enabling the Gmail API in Dia) met the wrong Google account; Peekaboo's
  clicks errored and one submitted an empty password and opened the locked
  Passwords app. Both browser and computer notes now forbid touching password,
  sign-in, two-factor or security dialogs (stop and report the account
  instead) and blind retries after a tool error.
- **Learning standing orders** (2026-11-06): a fourth seeded routine per
  scope, `RoutineKind::Reflect` ("Learn standing orders", daily 21:30, no
  browser or desktop). It reads the scope's runs (what was done, which
  needs-you items the owner cleared or left) and Home chat through
  `neko-daemon --request`, and when a clear pattern shows a rule he wants it
  rewrites the scope's standing orders itself (`SetStandingOrders`): every
  sentence of his kept, at most two rules added or adjusted a day, never
  loosening his own, each change in `did` with its evidence. The prompt now
  carries the workspace id, and `{neko}` in a routine's prompt becomes the
  daemon's path. `ensure_seeded` adds any seeded kind a scope lacks, so
  existing scopes got Reflect without reseeding.
- **Mac care** (2026-11-06): the home scope alone gets `RoutineKind::MacCare`
  (daily 04:00, no browser or desktop). It runs the shared
  `~/.agents/bin/mac-care` report and applies it when free space is under 60
  GB or a step frees over 1 GB; it clears only regenerable caches and build
  output, lists the owner's own big items (Trash, Downloads, transcripts) in
  one needs-you item when space stays under 30 GB, and never deletes those.
  The disk was 94% full (26 GB free) with ~90 GB in caches when this landed.
- **Nothing about one person is in the code** (2026-11-06, owner: a friend
  installing Neko must be able to adopt it with little setup). Prompts name the
  owner with `{owner}`, filled from the account's full name (`owner_name`);
  scopes are found in any common code folder (`scopes::ROOTS`: Documents,
  code, Projects, Developer, …) and named after the folder until the
  workspace is renamed; a home-folder "Personal" scope is created when no
  workspace covers home; chat-server scopes (Slack/Teams/Discord) check
  messages every 20 min. Everything specific to a person (their Slack server
  names, orgs, GitHub login, what a project needs) lives in the workspace's
  own Instructions, which routines read as "scope notes". The Paseo cutover
  is no longer seeded. `mac-care` ships in the app (`scripts/mac-care`,
  written to `<data>/bin/mac-care`), and `neko-daemon --protocol` prints the
  request shapes, so the full-power chat needs no source checkout.
- **Review before push** (2026-11-06): Messages, Morning brief, PR sweep and
  the full-power chat are told (`REVIEW_BEFORE_PUSH`) that a non-trivial code
  change gets a fresh reviewer subagent (the Agent tool) before it is pushed
  or reported done, as Neko's own tasks get an independent reviewer; clean
  rebases, conflict-free merges and one-line fixes are exempt.
- **Standing orders** are per workspace. The default
  (`DEFAULT_STANDING_ORDERS`) is "act without asking, report after". They go
  into every run's prompt.
- **Paseo cutover:** after a routine's first good run, its
  `paseo_schedule` (Client A Messages → `message-watch-adaptive`, the
  Asha brief → `daily-work-digest`, the Asha sweep → `weekly-pr-sweep`)
  is paused with `paseo schedule pause <id>`; the result shows on the routine
  (`state.note`). The Sibling app schedules stay in Paseo.
- **Inbox** (`web/src/pages/Inbox.tsx`, `#/inbox`): needs-you items across
  scopes (Done clears them), then per scope what Neko did, the routines (Run
  now, pause, edit), setup notes and standing orders. It polls `Routines`
  every 10 s.
- **Notifications** (`RoutineNotifier.swift`): the app polls
  `RoutineNotifications { after }` every 30 s and posts at most 5 notifications
  per poll, plus a summary for the rest. The first poll only records the
  latest seq. A click opens `#/inbox`. Only an installed `.app` bundle
  notifies.
- Not built yet: learning rules from approvals, moving MCP config between
  folders (it is only reported), phone delivery.

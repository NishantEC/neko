## Rune parity batch C: Setup (2026-11-17)

Code: `apps/neko-rune/src/view/setup/` (one file per section, `state.rs` the rules, `words.rs` the pure logic, `dialogs.rs`,
`kit.rs`), `src/link/setup.rs` (`SetupData`, `SetupAction`), scenes in `src/scenes_setup.rs`, tests in `tests/setup.rs`.
Behaviour is the web's `pages/setup/*.tsx`, `tools/*.tsx` and `setup/lib.ts`; visuals are `docs/design/rune/polish-spec.md`
and the existing Rune pages. Pictures: `docs/design/rune/batch-c-*` (synthetic); the pairs on the owner's copied data are
`bc-*` in `/tmp/neko-polish` (never committed; `polish-pairs.sh` makes them).

### Shape

- The sessions column's third nav row is **Setup** (was Workspaces): its count is `SetupData::attention` (connections that are on
  and broken, suggested watches, skill and memory proposals, guidance waiting for a yes) and it shows no count at 0. `FullView::Setup`,
  web id `setup`, key `view-setup`.
- A full view with the same 200 point rail component as Settings (`ui::rail`, now shared): Workspaces, Watching, Tools, Skills, Memory,
  Profiles, Schedules. A section is a `Page { count, trailing, body, title, back }`; the header is the 20 semibold title with a faint count,
  the column is 880. Rows are 32 points (`kit::list_row`), their actions are 28 point icon buttons that show on hover or focus
  (`group_opacity`) and every one is also in the row's right-click menu (`ui::with_menu`). Deep links: `App::open_setup(section)`,
  `App::open_connection(id)` (Tools on that connection's detail). `NEKO_RUNE_VIEW=setup NEKO_RUNE_SETUP=<section>` for pictures.
- Data: `Summary::setup` is a `SetupData` cut from each snapshot (workspaces with folders, connections, grants, watches, suggested watch ids
  from Neko's conversation, import candidates, skills, memory, proposals, guidance, decisions, profiles, schedules). Writes answer with the new
  workbench, so every page is live without its own poll; only Workspaces' scope details come from `Routines`.
- Requests: `SetupAction` (one enum, `wire()` builds the web's exact payloads; a test per family). Names are `Command::Mcp` /
  `McpCommand::*`, `Skills`, `AgentProfiles`, `Schedules`, `DecisionContext`, `SetupImport`, `SaveWorkspaceWithFolders`, `SaveMemory`,
  `DeleteMemory`, `DecideMemoryProposal`, `SetMemoryOptions`. The mirror rule covers them (the web's `mcp("Wake", ...)` helpers are named in
  `web/src/setup/lib.ts`); the allow-list's `PREFIXES` and `SUBCOMMAND_PREFIXES` gained the six sub-command enums. Only `SetToolGrant` and
  `SetWorkspaceToolGrant` are `RUNE_EXTRAS`: the web's Tools page shows tools read-only, the old native app had the grant switch.

### Rules worth knowing

- **Destructive means confirm.** Delete watch, dismiss a suggestion, delete schedule, forget memory, forget guidance, and pausing a connection whose
  tools were allowed (the daemon revokes the grants) are built only in `state.rs`'s `confirm_*` functions and shown in a dialog with a coral button;
  `tests/allow_list.rs::setup_destroying_requests_are_only_sent_from_a_confirmation` reads the sources, `tests/setup.rs` drives each one (nothing is
  sent before yes, Cancel sends nothing). Removing a folder in the workspace dialog and the registry's Cancel are staged edits, not requests.
- **A form waits for its own answer.** `setup_call_awaited` records the one request a dialog waits on (`SetupState::awaiting`); only its answer closes the
  dialog or sends the next request of a confirmation (turning a watch on is Save enabled, then Wake); another request's answer changes nothing; the
  dialog's buttons are off until then, so a double press cannot make two workspaces; a refusal (`setup_failed`, from `page_refused`) gives them back and
  the daemon's words show in the dialog (`pages.notes["setup:form"]`) or beside the row (`setup:<id>`).
- **Turning a watch on is a decision.** The switch asks (what it allows, every 10 minutes, which tools, which workspace); if its tools are not usable it
  opens Tools instead. Saving a schedule always pauses it. A skill proposal with a published audit cannot be applied, from its detail, its hover action
  or its menu, until the audit link was opened here and the review confirmed.
- **Grants.** A tool's "allowed to run unattended" switch is per workspace: `SetToolGrant` for a connection that belongs to a workspace, `SetWorkspaceToolGrant`
  for a global one in the workspace picked in the Tools header. Only a usable connection can be granted.
- **Registry search** (browse MCP servers) shells out to `/usr/bin/curl` on a thread (`state::curl_registry`; tests and pictures inject `SetupState::fetch`):
  batch A's `link/http.rs` reaches only the web's `/api/*` routes, and the public registry is not one. The registry address is the web's.
- Empty lists: one quiet line and one action (`kit::empty`), tested for every section.

### Not done / differences on purpose

- Watching: the web's "What should Neko keep an eye on?" box and "Use an idea" menu are replaced by the New watch dialog plus "Suggest watches" (the web's
  `SendMessage` flow, Neko's reply shows under the header with Hide). Watches are grouped by workspace (the web groups Active, Paused, Suggestions);
  workspaces with no watch have no group. "Check all now" is not offered.
- Workspaces: no "N open" tickets (the client keeps no workspace id per task row) and no per-row "Copy" menu entries (no clipboard seam yet).
- Tools: Skills is its own rail section (a tab of the web's Tools page). A connection's OAuth client id is read from a field on the detail page.
- Real Rune window: not run in a real window, only headless and against the isolated daemon (harness), as batch A.

### Packs (2026-11-17)

Packs is the first rail section (`view/setup/packs.rs`, `pack_dialog.rs`; state in `state.rs`'s last `impl App`). The listing is not in the workbench: `SetupState::packs` is filled by `Reply::Packs` (every pack request but a preview answers with it), asked for on opening the section, on opening Add pack and after each action. Requests are `SetupAction::Pack*` (`wire()` is `Request::Packs(PackCommand::..)`, one test per family). **Approval is of what was shown**: a preview always comes first and its `digest` goes back with Install, Update (`Some(digest)`) and Approve; the preview step is one dialog (`Dialog::Pack { tab, preview, intent: Install | Update | Review }`), its buttons sit outside the scrolling body, and it waits for its own answer (`setup_call_awaited`). "Changed on disk" shows an amber glyph and a Review action (`installed:<name>`). The plugin switch in the detail uses the Plugins page's HTTP path (`plugin_set`, id = last part of `plugin_dir`). Remove is built only in `confirm_remove_pack`. Allow-list: `PackCommand::` is a prefix; the eight commands are pinned in `PACK_REQUESTS` until the web page's calls are found by the scan. Tests: `tests/packs.rs` (22, headless). No isolated-daemon test: the daemon side was being rewritten alongside.

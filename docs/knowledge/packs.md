## Packs: one installable unit for tools, skills, UI and routines (2026-11-17)

Code: `crates/neko-core/src/packs.rs` (everything, with tests), `crates/neko-daemon/src/packs.rs` (the handler; `main.rs` calls
`packs::recover` at start), `crates/neko-protocol/src/packs.rs` (wire types), `packs/` (bundled catalog), Rune
`apps/neko-rune/src/view/setup/` (Packs is the first Setup rail section), web `web/src/pages/setup/Packs.tsx` and
`web/src/pages/setup/packs/`, `scripts/verify/packs.mjs`.

**Owner decision (2026-11-17): SpaceO is secondary.** `packs/spaceo` ships only as an optional catalog example. Nothing installs it, prefers
it or nudges anyone towards it; Peekaboo and `scopes::with_computer_use` are untouched. Packs exist so anyone who wants a bundle can add it.

### The format (`pack.toml`)

`name` (`[a-z0-9-]`), `title`, `version`, `description`, `homepage`, `licence`; `[requires] binaries` (name + install hint); `[[mcp]]`
(name, `command` by plain program name, `args`, `env` = names of variables the person must supply); `[[skills]] path` (a folder with
`SKILL.md`); `[ui] web_plugin` (a folder with `manifest.json` + entry) and `luau` (native cards and views the Rune client runs in a sandbox: [luau-plugins.md](luau-plugins.md)); `[[routines]]` (kind must be `Custom`,
instruction, cadence); `[health] command`; `[permissions] summary` (required, plain words shown before install).

### Approval is tied to what was shown (the digest)

`Preview` returns `digest`: SHA-256 over the `PackInfo` (permissions, every command and argument, health, env names, warnings, the plugin's
declared pages/viewers/fences/prompt **and a hash of every plugin file**, **a hash of every listed `.luau` file's contents** (`PackInfo.luau_hash`; editing one sends the pack to "Changed on disk"), routine texts, a hash of the skills, requirement names and hints; not
what was found where). `Install { source, digest }` recomputes it from the tree it fetched and refuses on a mismatch; there is no `approve`
flag. The approved digest is stored in the state row. An interpreter command (sh, bash, zsh, fish, python*, node, deno, bun, perl, ruby,
osascript, curl, wget, env) is allowed but flagged "Runs a script interpreter: <cmd>" in `info.warnings`; a plugin adds "Runs inside Neko with
access to Neko's own API". Both are inside the digest.

- **Update** previews the new version; if commands, args, health, env, permissions, warnings or the plugin differ, or the text of a routine
  the person switched on differs, `Update` is refused ("Update needs approval") until `Update { name, digest }` carries the new digest.
  `PackPreview.changes` lists what changed. The UIs always preview first and send the digest they showed.
- **Changed on disk.** Every re-read of an installed pack (health check, `routine_servers`, the listing) verifies the installed folder still
  has the approved digest. A mismatch sets status `Changed`, nothing runs, and the person reviews it (`Preview { source: "installed:<name>" }`)
  and approves exactly that digest with `Approve { name, digest }`.

### Storage and ownership

- Installed packs live in `<data>/packs/<name>/` (the whole folder, `.git` skipped). State is one row per pack, `pack_state_v1:<name>`,
  plus `packs_index_v1` (names), written together in one `Db::atomic`; never in the workbench snapshot. The row records the owned connection
  ids, skill folders and canonical `SKILL.md` paths, plugin folder, routine ids, the approved digest and `orphans`.
- Skills go to `<data>/skills/pack-<pack>-<leaf>/`, the plugin to `<data>/plugins/<pack>-<leaf>/` (installed **switched off**: a `.disabled`
  file; the person enables it with the Plugins enable path; an update keeps it on only if the plugin's code hash is unchanged). Each folder
  and the pack folder carry a `.neko-pack` marker with the pack name. A folder is deleted only if the record names it AND it sits directly
  in that root AND the marker matches; a folder the person replaced is moved aside and kept as an orphan. Skills are locked from the curator;
  `skills::discover` skips dot-folders.
- Routine ids are `pack:<pack>:<slug>` (a `:` can be in neither part, so two packs never collide); install/update refuse if any routine
  already holds the id and it is not this pack's. Routines are created disabled (workspace: the one whose folder is home, else the first).
  A pack's own routines get its tool servers in their MCP config (`scopes::with_pack_servers`, `packs::routine_servers`: only routines in
  the record, only while the files verify).
- The connection is global (`workspace_id` empty), enabled and trusted (the person's approval is the trust) and has `pack: Some(name)`
  (`McpConnection.pack`). **`allow_discovered_tools` skips pack connections**, so a pack's tools are granted to no workspace until the
  person grants them in Tools. Env values go to the Keychain under the connection id, never to files or answers. MCP launches already run
  with `env_clear` and only PATH plus the declared env; health checks run with `env_clear` plus PATH, HOME, TMPDIR, LANG.
- A program is named, never pathed: `which()` looks in `$PATH`, `~/.local/bin`, `~/.cargo/bin`, `/opt/homebrew/bin`, `/usr/local/bin`,
  `/usr/bin`, `/bin`, never inside a pack. Links anywhere in a pack are refused and every entry is re-opened with `O_NOFOLLOW` at copy time;
  limits 2000 files / 20 MB (a remote clone is also stopped at 200 MB / 5,000 files).

### Requests (`Request::Packs`, answered `Response::Packs` / `PackPreview`)

`List`, `Preview {source}` (changes nothing), `Install {source, digest, env}`, `Remove {name}`, `Update {name, digest?}`, `Health {name}`,
`Approve {name, digest}`, `CleanUp {name}`. A source is a folder, `catalog:<name>`, `installed:<name>` (preview only) or an `https://`/`ssh://`/
`git@` URL (shallow clone into `<data>/.packs-tmp`, protocols limited, no prompts, deleted afterwards). The catalog is
`<data>/source/packs` (the app's source clone; `packs/` triggers no reinstall) plus `NEKO_PACKS_DIR`; tests pass their own dirs.

### All or nothing, and what is never swallowed

- **Install and Update** write a `Journal` (swaps, locks, connections, credentials, routines, the previous state row) to
  `<data>/packs/<name>.journal` before step 1 and after every step. Work happens in order: copy into `<data>/.packs-stage/<tag>` and write the
  Keychain **outside** the database lock; then, under the lock, preflight (ownership), rename staged folders into place (the old one is moved
  to `.bak-<tag>-<leaf>`), write connections, routines and the state row. `Tx` rolls back on any error and in `Drop` (a panic); once the
  journal says `committed`, only cleanup is left and nothing is rolled back.
- **Crash**: `packs::recover` (daemon start) rolls back any uncommitted journal, finishes a committed one, restores or sweeps `.bak-*`
  folders with no journal, and deletes staging.
- **Nothing is swallowed.** Rollback returns warnings (shown in the error); a backup that cannot be restored is kept and recorded in the
  record's `orphans`; the commit phase deletes a moved-aside copy only if it is the pack's own, else it is an orphan; `drop_leftovers`
  never deletes the Keychain secret of a connection it keeps because a watch uses it (orphans `connection:<id>` and `keychain:<id>`).
  The UIs show orphans as one quiet line with "Clean up" (`CleanUp`, also done by Remove).
- **Remove** refuses while a watch uses the pack's connection (names it); keeps a copy in `<data>/cleanup-archive/packs/<name>-<ms>/`
  best effort (no size limits; a failure is logged and the removal goes on); db part under the lock, files and secrets outside; the state
  row goes last so an interrupted removal can be run again.
- **Health** stamps its result only if the installed version under the lock still equals the one that was checked.
- Statuses: Healthy, NeedsSetup (a required program is missing; the connection is created later by the next `Health`), Failing (non-zero
  or 30 s timeout; stdout tail kept), Changed (see above). Install does not fail when the check fails.
- Tests never run a real program: `Env` (`which`, `run`, `fetch`, `hook`) is replaced; failures, panics and a simulated crash are injected
  through `Env::hook` after each step; the Keychain is touched only when env values are given (never in tests).

### Not done

Luau files are run only by the Rune client, from `Request::PackUi` (Healthy and digest-matching packs only). No publisher signing beyond the person's approval of the digest (permissions, every command, the plugin's
declared power). A remote source's newer version is only noticed on Update/preview (List cannot clone). A routine run by Claude gets the
pack's server through Claude's MCP config, which cannot be given a cleared environment; Neko's own MCP launches can.

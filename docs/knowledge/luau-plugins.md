## Luau plugins: native cards and views from approved packs (2026-11-17)

Code: `apps/neko-rune/src/luau/` (`mod.rs` lifecycle, `host.rs` the `host` table and the sandbox, `nodes.rs` host nodes),
`src/link/luau.rs` (what a plugin may ask the daemon), `src/view/luau.rs` (cards, the view page, status rows), `src/scenes_luau.rs`;
daemon side `crates/neko-core/src/packs.rs` (`ui_sources`, `luau_hash_of`), `Request::PackUi`; example `packs/hello-luau/`; tests
`apps/neko-rune/tests/luau.rs` (34) and the daemon's `packs::tests`; pictures `docs/design/rune/luau-*.png` (`scripts/snap-luau.sh`).
Decision: [ADR 0002](../decisions/0002-rune-for-everything.md), plugin slots (b) full views and (d) Neko cards. Pack format:
[packs.md](packs.md).

### Where Luau comes from (and nowhere else)

Only from installed, approved packs: the `[ui] luau = ["ui/card.luau", ...]` files of `pack.toml`.

- **The digest hashes each listed `.luau` file's contents** (`PackInfo.luau_hash`, left out of the digest when empty so packs without Luau
  keep the digests they were approved with). Preview adds a warning ("Draws panels in Neko's own window with sandboxed Luau code"), an
  update that changes the files needs approval ("The native panels (Luau) changed"), and editing an installed `.luau` file sends the pack to
  "Changed on disk". Each file is read without following a link, as text, at most 128 KB, and a pack lists at most 50.
- **`Request::PackUi { name }` -> `Response::PackUi(Vec<PackUiFile { path, source }>)`** is the only way Rune gets the sources. The daemon
  answers only while the pack is **Healthy** and the files it reads *now* hash to the approved digest (the bytes returned are the bytes
  hashed). Anything else is an error sentence. It is in `RUNE_EXTRAS` (`tests/allow_list.rs`). **Rune never reads a pack folder**
  (`rune_never_reads_a_pack_folder_itself` scans the plugin code for file access).
- **When**: every 30 s (`SYNC_EVERY`) and at once after any pack answer (`apply_reply` of `Reply::Packs`): `link::luau::sync` asks the
  pack list and then `PackUi` for each Healthy pack that lists Luau files. A pack that is not Healthy or not unchanged is not asked for; one
  the daemon refuses is unloaded at once, and its rows say why. A daemon that cannot be reached changes nothing.

### The file

One table (a file returning anything else does not start):

```lua
return {
  slot = "card",            -- "card" | "view"
  id = "needs-you-count",   -- [a-z0-9-], at most 40, unique within the pack
  title = "Needs you",      -- at most 60
  icon = "inbox",           -- optional, a name from src/icons.rs (`icons::by_name`); unknown -> the dot
  init = function(host) return { } end,   -- optional: the first state (a table)
  render = function(state, host) ... return ui.col{ ... } end,
}
```

`rune-luau`'s own contract is `render(view)`; `ADAPT` (in `luau/mod.rs`) wraps the file's table: `state` is one table the plugin mutates,
`saveHotState`/`loadHotState` carry it across a reload by copying the saved keys *into* it (so tasks `init` started keep their table).
`require("./other")` reaches a sibling listed in the same pack; a file gets its own `Sources` (itself plus the listed files it names in a
`require`, `sources_for`), so a file that does not compile stops only the plugins that use it. A file with no valid `slot`/`id`/`title`/`render`
does not start and the Plugins page says which.

### The host (`host.rs`, one frozen table; also `require("@neko/host")`)

| Call | Behaviour |
| --- | --- |
| `host.read(name, args)` | A daemon **read**: `name` must be in `link::luau::READS` (`Workbench`, `Routines`, `MorningDigest`, `MachineHealth`, `UnseenTasks`; the calendar is deliberately not one), each one fixed request; any other name (`DeleteTask`, `Packs`, ...) is `nil, "X is not a read Neko offers plugins"` and nothing is sent. Answers are projections (`Workbench` is `{working, needs_you, tasks=[{id,key,title,status,glyph,wait,needs_you,updated_ms}]}`, newest first, 100 at most, never the snapshot; `Routines` is `[{id,name,enabled,cadence,state,glyph,last,last_ms}]`, `MorningDigest` is `{day,at_ms,headline,finished,stuck,decisions}` without the calendar line and the spend) and are refused above 256 KB. `Workbench` and `Routines` are kept for 2 s on the live wire, because the daemon builds the whole snapshot for each. Runs on a worker thread (`Host::async_function`); returns a table or `nil, err`. **Four a second per plugin**; the fifth is `nil, "rate limited..."`. Callable only from a task (a handler, `task.spawn`), never while drawing. |
| `host.ask(text)` | Drafts `text` after what the person has in Neko's ask box, opens the column and gives the box the keyboard. **Never sends** (`luau_apply` only calls `into_composer`). At most 4000 characters. |
| `host.open(route)` | Goes to a route Rune has (`deeplink::clean_route` then `App::route_known`: tasks, artifact, settings, setup, pull-requests, a plugin page, a Luau view `luau_<pack>:<id>`); anything else is a toast, never a browser. |
| `host.storage.get(key)` / `.set(key, value)` | The daemon's plugin storage over HTTP (`/api/storage/<ns>/<key>`) in the namespace **`luau_<pack>`** (not the bare pack name: a web plugin may be called that, and this must not read its data; a pack name has no underscore and its own web plugin is `<pack>-<leaf>`). Keys follow the daemon's rule. **256 KB per pack** across keys (`storage_set` lists the namespace's sizes first; overwriting counts the new size only). Two writes and eight reads a second. |
| `host.now()` | Unix milliseconds. |

### Nodes and colours (`nodes.rs`, the setup chunk in `host.rs`)

Stock `@rune/ui` (`row col box text spacer virtualList each`) plus `ui.input{key,value,on_change,placeholder}`, `ui.button{label,on_press,primary}`,
`ui.image{src,w,h}` (a base64 PNG/JPEG/GIF/WebP data address under 256 KB, or a file inside the artifact store read through
`readers::Confined`), `ui.glyph{state}` (Neko's status glyph; accepts `in-progress`, `Building`, `Completed`, ...), and
`ui.text{ "words", muted = true, size = 12 }` as well as `ui.text("words", props)`.

**Every node draws with Neko's theme tokens.** The palette is `host::colour` over the current theme (`text strong muted faint window card
sidebar border hover active selected ring iris amber sky mint coral`, plus `rune-luau`'s own names), so a theme change recolours plugins.
`rune-luau` reads a `#rrggbb` string anywhere a colour goes, so the setup chunk wraps each constructor in Luau and **errors on a string that
starts with `#` in `fill hover press border color hoverGroupColor`**, checked on the runtime value (so `"#" .. "ff0000"` and a frozen style
table are caught too); `ui.hex` is replaced by an error. `ui` is frozen after.

**Inputs**: the text lives in `Luau::inputs` (keyed plugin + key), the node mirrors it per frame through a thread-local, and `on_change` is
called from the next tick (`script.call("__neko_change")`). A plugin that built an input is drawn with `render_uncached` every frame (the canvas
spike's memoization bug: a memoized subtree hands the runtime a stale field and undoes keystrokes); all plugins are drawn that way (a theme
change and a changed input then need no memo bookkeeping).

### The sandbox, and what holds a plugin

`rune-luau`'s: a 250 ms watchdog on every entry (render, handler, task step), a 64 MB heap, no `io`, no `os` but `os.clock/time/date`,
no network, no files, `require` inside the files it was given (what `rune-luau` leaves in: `loadstring`, `getfenv`, `debug.info` and
`traceback`; none of them reaches outside the VM). Added here, each one from the review's question "can a plugin freeze the UI?":

- **A CPU budget** (`Plugin::watch_cpu`, `CPU_SHARE` 0.5): a plugin whose own code (renders, handlers, tasks) takes more than half of every
  second for two seconds in a row is stopped. The watchdog bounds one entry; a task that starts a 200 ms step every frame is what this is for.
  A stopped plugin is not polled at all until Reload.
- **A render over 60 ms** (`SLOW_RENDER_MS`) stops the plugin like an error does.
- **Arguments and storage values cross into Rust with a budget** (`host::walk`: 20,000 values, 12 levels, the byte limit; a table of millions
  of entries or one that holds itself stops at the budget). The library's own `Data` conversion has no limit and no cycle check, so nothing of
  a plugin's goes through it unbounded: **host nodes take plain props only** (a table-valued prop of `button`, `image`, `glyph` or `input` is an
  error in the setup chunk), and `saveHotState` returns a copy with a node budget and no cycles (`ADAPT`).
- **`print`, `warn` and `error` are cut** to 1 KB a line in the setup chunk: the library keeps the last 256 log lines outside the 64 MB heap.
- **`onHover` and `onVisibleRows` do not exist** for plugins (an error in the setup chunk): only a press, which is a person choosing something,
  can reach `host.ask` and `host.open`; they also work at most once every 400 ms. A task, `init` or a render that calls them gets `nil, err`.
- Pictures are size-checked from the header before decoding (1 megapixel for a data address, 4 for an artifact file), and at most 16 are kept.
- Storage: the check and the write are one locked step per process, two writes and eight reads a second.

**When a plugin errors or is killed** (any `Error` log line: a render error, a handler error, the watchdog, the heap, the budgets) its slot
shows one quiet line, "<title> stopped: <reason>", and Reload (a clean start from the same files: `init` runs again, stored data stays); the
card is not drawn while it is stopped; the rest of the app never notices.

### The slots

- **Card**: "Cards" section of the Neko column below Digest (`view/luau.rs::cards`): collapsible (`cards:closed`), open by default, at most 3 and
  "and N more" (`cards:all`). A card is a 32 pt header (icon, title, right-click menu Reload / Hide / Open pack) and a body up to 160 pt that
  scrolls; no fill, no border, spacing only. Hide is remembered (`luau-hidden` pref); "Show all" is in Plugins.
- **View**: listed under the account menu's "Plugin pages" next to the web's pages and in Plugins (Open), route `luau_<pack>:<id>`
  (`deeplink::clean_route` accepts it); it replaces the session like a plugin page (`App::plugin_view` holds a `PluginPage` whose plugin id
  starts with `luau_`): the page header (the title), then the content in the 880 pt column.
- **Status**: every declared file is a row (mint check "Loaded" or coral cross "Stopped: reason") in the pack's page (Setup, Packs, "Native
  panels", with Reload) and in Plugins ("Native plugins (Luau)").

### Example and tests

`packs/hello-luau` (catalog only; nothing installs it): the card "Needs you" (count of sessions that need the person and the first three,
`host.read("Workbench")`, a press opens the session, a button drafts a question) and the view "Routines at a glance" (`host.read("Routines")`, a
filter input). No MCP, skills or routines. The scenes `luau-*` draw those same files over made-up data (`scenes_luau::demo_wire`: a fake wire
with the clock stopped at the demo's now).

`tests/luau.rs` covers: PackUi only for healthy packs (and a refusal unloads), no folder reads, `host.read` refuses write names and sends
nothing, four reads a second, projections, `host.ask` drafts and sends nothing, `host.open` refusals, no escape from the sandbox (io, os, fetch,
require out, frozen host), raw colours refused in every spelling, the watchdog on a render and on a handler, the heap, a stopped plugin and
Reload, metadata checks, a bad sibling stopping only its users, hot reload with state kept, the example card and view rendering headless and the
filter typing, the cards section (3, "and N more", fold, menu, hide), storage against a loopback server (namespace, 256 KB, key rules), the status
rows. Daemon tests: the digest covers `.luau`, editing one sends the pack to Changed, `ui_sources` refuses unhealthy or changed packs.

### Decisions and limits

- Rune sources only from `PackUi`; a pack's `.luau` files are *not* hot-loaded from the data folder (unlike the spike's folder watch).
- A pack that changes on disk or stops being healthy keeps running the copy the person approved until the next sync (at most 30 s, at once after
  any pack answer in Setup); only then is it unloaded.
- Not done: viewer/fence registration for Luau (slot (a)) and session actions (slot (c)): the spike's gap stands. No `ui.canvas`.
- The storage namespace differs from the brief's "pack name as plugin id" on purpose: `luau_<pack>`. A web plugin a person made by hand and
  named exactly `luau_<pack>` would share that storage (a pack-installed one cannot: `<pack>-<leaf>`); the daemon does not reserve the prefix.
- The pack's own `permissions.summary` is the author's words, not enforced; what is enforced is the list of reads, so "reads your sessions" is
  true and "reads your calendar" cannot be.
- Two plugins that both want the keyboard: only one text field has it, as everywhere in Rune; plugin input element keys are hashed from the
  plugin's address (`nodes::input_element_key`).

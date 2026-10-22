# One registry: plugins are not a separate world

Owner, 2026-10-22: "plugins and all are a different thing in our heads, it should
submerge." A page you (or an agent) write must not be a weaker, separate kind of
page from the ones Neko ships. This is how that is built.

## One function, one host

`web/src/host.ts` defines `Host`. Neko's own pages (`pages/index.ts`) and the pages in
`<data dir>/plugins/<name>/` register with the same `host.registerPage`, and both read the
same registry (`web/src/registry.ts`), which the sidebar, ⌘K and the router use.

| `host.…` | What it is |
| --- | --- |
| `React`, `request`, `onEvent` | the app's own copies; `request` is the function the app uses against `POST /api/request` |
| `registerPage(spec)` | `{ id, title?, icon?, keywords?, group?, component \| load \| wrap }` |
| `registerCommands([...])` | ⌘K entries, namespaced per plugin, removed when it is switched off |
| `inspector` | `open({ title, children })` / `close()` |
| `ui`, `icons` | Neko's components and Phosphor icons (`pluginKit.ts`) |
| `storage` | `get(key, fallback)`, `set`, `delete`, `list`: JSON kept by the daemon |
| `navigation` | `navigate(route)`, `hrefFor(route)`, `useRoute()`, `HOME` |
| `addSection(s)` / `addTab(t)` | add a section to Settings / a tab to the task sheet |

Every page is rendered with its host as the `host` prop and through `useHost()`. If a built-in
needs something from the shell, it is added to the host, so a user page can reach it too.
(Page bodies still import `@/ui/...` and `@/editorial` directly, which is what `host.ui`
re-exports; the rule that nothing else is private is by convention, not enforced.)

`web/src/plugins.tsx` is only the loader: it lists `GET /api/plugins` every 3 s, imports the
modules that changed (`?v=<mtime>`) and hands what each registered to the registry.

## Where a page appears

- **New id** → a page of its own at `#/<plugin>:<id>`, in the sidebar group the page's
  `group` (or the manifest's `"group": "main" | "work" | "system"`) names, default `main`, with
  a small "yours" mark and a "Go to" entry in ⌘K. There is no "Plugins" sidebar group.
- **A built-in's id** (`home`, `tasks`, `prs`, `tree`, `workspaces`, `watching`, `schedules`,
  `memory`, `profiles`, `tools`, `settings`, `usage`, `ui`, `customisations`) → it **replaces**
  that page (`component` / `load`), or **wraps** it: `wrap: (Original, host) => Component`.
  The replaced page keeps the built-in's route, title, icon and group unless the spec says
  otherwise. The sidebar row reads "Customised" and a strip above the page says so.
- **Reset to original**: the strip (and the Customisations page) switches the override off
  without touching the plugin. The list of reset ids is the app's own storage key
  `_neko/overrides-off`, so it survives reloads and browsers. "Use my version" switches it on again.
  Disabling or removing the plugin ends the override too.
- A page that throws shows its error (`PluginBoundary`) instead of taking the app down.

## Storage

`host.storage` is per plugin (the folder name) and backed by
`<data dir>/plugin-data/<id>/<key>.json`. The app's own namespace is `_neko`.

| Endpoint | |
| --- | --- |
| `GET /api/storage/<id>` | `{ keys: [{ key, size }] }` |
| `GET /api/storage/<id>/<key>` | the JSON value; an unset key is `200 null` with `X-Neko-Storage: unset` (a 404 is logged as an error by browsers, a 204 as an aborted request) |
| `PUT /api/storage/<id>/<key>` | a JSON body, at most 1 MiB; at most 256 keys per namespace; written atomically |
| `DELETE /api/storage/<id>/<key>` | removes it (404 when unset) |

`<id>` follows the plugin-name rule (letters, digits, `-`, `_`, ≤ 64); `<key>` is letters,
digits, `.`, `_`, `-`, ≤ 128, no leading dot. A symlinked namespace or value is refused.
`web_tests.rs` covers names, bodies, limits, symlinks, 401/403 and "refused before the body is read".

## Styles

`<data dir>/custom.css` is served at `GET /custom.css` (empty when absent, ≤ 1 MiB, a symlink is
not followed) and reported by `GET /api/customisation` (`plugins_dir`, `css_path`, `css_exists`,
`css_version` = mtime). The shell keeps one `<link id="neko-custom-css">` after every other
stylesheet (a `MutationObserver` re-appends it when a page chunk adds CSS) and re-points it when the
version changes, so a save applies within the 3 s poll. It can restyle anything through the
app's CSS variables: `--background`, `--foreground`, `--strong`, `--card`, `--popover`,
`--primary`, `--muted-foreground`, `--border`, `--input`, `--ring`, `--radius`, the status
colours (`--success`, `--warning`, `--destructive`, `--info`) and the dark values under `.dark`.

## Customisations (`#/customisations`)

Replaces the Plugins page; `#/plugins` redirects. It lists every user page and override
(Running / Off / Error, the daemon's or the module's last error), with enable/disable, reload, remove,
details (the manifest) and copy-path actions, "Reset to original / Use my version" for each override,
and the paths of the pages folder and `custom.css`. **Change Neko** replaces "Build a plugin with
Neko": one chat message built from `pages/customisations/prompt.ts` (shown before it is sent, kept
under the daemon's 4096-byte limit; measured by the dialog and by `scripts/verify/customise.mjs`/`shell.mjs`)
that tells Neko to pick the lighter route:

- **A page, override or style of yours** (plugins folder / `custom.css`): live in seconds, only yours.
- **The app's source** in the neko checkout (`self_update.checkout`, `web/src` for the UI) for what a page
  cannot reach (chat, shell, daemon, a fix everyone should get). A UI-only change that lands on `main` goes
  live through self-update; the previous web UI is kept as `web-live.prev`.

Settings → Neko updates has **Roll back the web UI** (`Command::RollBackWebUi` →
`self_update::roll_back_web`: `web-live` returns to `web-live.prev`, or is dropped so the app's own UI
is served; open pages are told with `Event::WebUpdated`).

## What stays gated

User pages run in the browser with the bridge token, exactly as before: the same Host / Origin /
Sec-Fetch-Site / token gate as everything else decides who may call the daemon, and a page can do
what `request(...)` can do. The daemon gained storage and the stylesheet only; no new command
authority, no filesystem reach beyond `plugin-data/` and `custom.css`. "Open folder" is a copied path
(there is deliberately no daemon "reveal in Finder" endpoint). `web.rs` and the updater are on the
self-update `SENSITIVE` list, so a change to them waits for the owner's click.

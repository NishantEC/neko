# Writing a Neko plugin

Printed by `neko-daemon --plugin-guide`, so an agent on any Mac can follow it
without the Neko source. A plugin is a folder in Neko's plugins folder
(`<data dir>/plugins/<name>/`, where `<name>` is lowercase letters, digits,
`-` or `_`). It goes live in every open Neko page within seconds of being
written. There is no build step and no bundler.

## 1. The two files

`manifest.json`:

```json
{ "title": "STL viewer", "description": "One sentence a person would read", "version": "0.1.0", "entry": "index.js", "group": "main" }
```

`group` is `main`, `work` or `system` (the sidebar section). Raise `version`
on every change.

`index.js` is an ES module whose default export receives the host:

```js
export default function register(host) {
  const { React, ui, icons } = host;
  const h = React.createElement;
  function Page() {
    return h("div", { className: "mx-auto w-full max-w-3xl px-6 py-10 flex flex-col gap-6" },
      h(ui.PageHeader, { title: "STL viewer" }),
      h(ui.Panel, null, h(ui.Button, { onClick: () => ui.toast.success("Saved") }, "Save")));
  }
  host.registerPage({ id: "stl-viewer", title: "STL viewer", icon: "Cube", component: Page });
}
```

React, Neko's components and icons come from the host: never import them.
`host.ReactDOM` and `host.jsxRuntime` are the app's own react-dom and JSX
runtime, for libraries that render through React (see section 3).

## 2. What the host gives you

- `registerPage({ id, title, icon, component | wrap })`: a new `id` is a page of
  its own, marked as the user's. A built-in id (`home`, `inbox`, `tasks`, `prs`,
  `tree`, `workspaces`, `watching`, `schedules`, `memory`, `profiles`, `tools`,
  `settings`, `usage`) replaces that page, or wraps it with
  `wrap: (Original) => function Wrapped(props) { … h(Original, props) … }`. The
  user can reset it to the original.
- `registerCommands([{ id, title, run }])`: entries in the ⌘K palette.
- `storage.get(key, fallback)`, `set`, `delete`, `list`: JSON kept by the
  daemon (1 MiB, 256 keys per plugin). Use it, never `localStorage`.
- `request(json)`: one request to the Neko daemon, the same JSON as
  `neko-daemon --request` (see `neko-daemon --protocol`). Call no other site.
- `navigation.navigate(route)`, `hrefFor(route)`, `useRoute()`;
  `theme.get()` / `theme.subscribe(fn)`; `addSection({ id, title, component })`
  in Settings; `addTab({ id, title, component })` on a task (it gets `{ task }`);
  `inspector` (the panel on the right); `onEvent(fn)` for daemon events.
- `ui`: `PageHeader`, `Panel`, `Button`, `Input`, `Textarea`, `Select*`,
  `Switch`, `Checkbox`, `Badge`, `StatusBadge`, `Tabs*`, `Table*`, `Dialog*`,
  `DropdownMenu*`, `Tooltip*`, `Progress`, `Skeleton`, `Spinner`, `Empty*`,
  `toast`. Build from these; restyle only through the app's CSS variables
  (`--background`, `--foreground`, `--primary`, `--card`, `--border`,
  `--radius`), never new colours, so every theme keeps working.

## 3. Libraries (three.js, a canvas library, a chart library…)

Neko's pages only run scripts from Neko itself: a CDN `import` is refused.
Put the library's ES module build inside the plugin folder and import it by a
relative path:

```sh
mkdir -p vendor && curl -fsSL https://unpkg.com/three@0.170.0/build/three.module.js -o vendor/three.module.js
```

```js
import * as THREE from "./vendor/three.module.js";
```

A React library (Excalidraw, React Flow…) must use the app's React, never a
copy of its own: pre-bundle it once in a scratch folder with esbuild
(`--bundle --format=esm --splitting --minify`), resolving `react`,
`react-dom` and `react/jsx-runtime` to a small shim that re-exports
`globalThis.__nekoPlugin_<name>` (set from `host.React`, `host.ReactDOM` and
`host.jsxRuntime` in `register` before the page module loads, e.g. with
`registerPage({ load: () => import("./page.js") })`). Point the library's
asset path (fonts, workers, `.wasm`) at the plugin folder, since anything from
another site is refused. `.woff2`, `.ttf`, `.otf` and `.wasm` are served with
their proper types.

Pin an exact version, keep the library's licence file next to it
(`vendor/LICENSE-three`), and prefer a single-file ESM build. WebGL, `<canvas>`,
Web Workers loaded from the plugin folder and WebAssembly all work. Network
calls go through `host.request`, not `fetch` to other sites.

## 4. Changing a plugin safely

1. Before you change an existing plugin, copy its current files into
   `previous/` inside the plugin folder (replace what was there). If the new
   version fails to load, every open Neko page loads `previous/` instead and
   says so, so a broken change never leaves the user with nothing.
2. Write the new files and raise `version`.
3. Open the page: `neko-daemon --open-page <id>` opens Neko signed in at
   `#/<id>` in the default browser (any open Neko page also picks the change up
   within seconds).
4. Read how it loaded: `neko-daemon --request '{"PluginStatus":{"id":"<name>"}}'`.
   `ok: true` means it loaded and rendered; otherwise `problem` says what
   failed, and `rolled_back: true` means the previous version is showing. Fix
   and repeat until it is `ok`. A plugin that never reports was not loaded by
   any page yet: open it again.
5. Look at it as the user will, if you can see the screen (browser or desktop
   tools): light and dark themes, an empty state, a long list.

## 5. What a plugin cannot do

It cannot change Neko's chat, its shell, the daemon or other people's Neko.
Those are changes to Neko's own source (`web/src` for the interface), made on
a branch and reviewed, and they reach the app through its self-update.

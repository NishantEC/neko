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
on every change. A plugin that adds a viewer or a reply fence also declares it
here, so the model is told it exists (section 4):

```json
{ "title": "STL viewer", "version": "0.1.0", "entry": "index.js",
  "viewers": [{ "id": "stl", "title": "STL viewer", "kinds": ["stl"] }],
  "fences": ["stl"],
  "prompt": "Save a 3D print as .stl; it opens in the STL viewer." }
```

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
- `registerViewer(spec)` and `registerFence(spec)`: draw an artifact kind on
  the canvas, and a reply's fenced block (section 4). `artifacts.readText(props)`,
  `artifacts.readBytes(props)` read what a viewer shows; `artifacts.show(id, version?)`
  opens one beside the chat or in its tab.
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

## 4. Viewers and reply fences

An artifact's **kind** is its file extension (`stl`, `glb`, `csv`…). The canvas
and the artifact's tab draw each version with the viewer registered for its
kind: Neko's own built-ins (page, image, PDF, document, code, table, 3D model,
sketch) and yours are one registry. The highest `priority` wins a kind (0 by
default); at equal priority the one registered last wins, so a plugin's viewer
replaces a built-in for the same kind.

```js
export default function register(host) {
  const { React } = host;
  function Stl(props) {          // { artifact, version, kind, url, text?, theme, onEdit?, inline? }
    const [size, setSize] = React.useState(null);
    React.useEffect(() => { host.artifacts.readBytes(props).then((b) => setSize(b.byteLength)); }, [props.url, props.text]);
    return React.createElement("p", { className: "p-4 text-sm" }, size === null ? "Opening…" : `${size} bytes`);
  }
  host.registerViewer({ id: "stl", title: "STL viewer", kinds: ["stl"], component: Stl });
  host.registerFence({ language: "stl", viewer: "stl" });
}
```

- A viewer gets `url` (the file, same origin, read it with `fetch` or
  `host.artifacts.readBytes`) or, inside a reply's fence, `text`. `theme` is
  `{ mode, appearance, id, name }` for code that draws its own colours.
  `load: () => import("./viewer.js")` instead of `component` keeps a heavy
  library out of the way until a file of that kind opens. Set `edits: true`
  and the viewer gets `onEdit(text, note)`, which saves the next version of a
  text kind and resolves to its number.
- `registerFence({ language, viewer | component })`: a reply's
  ```` ```<language> ```` block is drawn by the viewer (its text, or the
  artifact it names: `artifact <id> v<n>`) or by a component that gets
  `{ code, language }`. Blocks stay code while the reply streams, and any
  language without a fence stays code.
- **Declare them in `manifest.json`** (`viewers`, `fences`, and a `prompt` of
  one or two sentences, 400 characters at most). Neko adds them to the
  "Installed viewers" section every full-power chat and routine is given, so
  agents save files of that kind and use the fence. A viewer the model is not
  told about is one nobody will use. `neko-daemon --plugin-guide` ends with
  what is installed on this Mac now.
- Do not scan the page's DOM for links or code blocks to replace, and do not
  serve and poll your own iframe: register a viewer or a fence.

## 5. Changing a plugin safely

Make a small working version and load it before refining it; never build a
mock preview, the installed plugin is the preview. When you report, say what
you could not verify (`ok: true` means it loaded and rendered, not that it
looks right), and put back any state you changed while testing.

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

## 6. What a plugin cannot do

It cannot change Neko's chat chrome, its shell, the daemon or other people's
Neko; it can add viewers and fences that draw inside them (section 4). Other
changes are to Neko's own source (`web/src` for the interface), made on a
branch and reviewed, and they reach the app through its self-update.

## 7. A plugin, or an artifact?

A plugin is a new part of Neko: a page, tool or viewer the person comes back
to. A thing you make *for* the person to look at (a 3D model, a sketch, a
table, a document, a screen design, a report, a one-off dashboard) is an
**artifact**, not a plugin: write one file of its natural kind (.glb,
.excalidraw, .csv, .md, .html with inline CSS and scripts, .svg, .png) and run
`neko-daemon --request '{"SaveArtifact":{"title":"…","file":"/abs/path.glb","workspace_id":null,"artifact_id":null,"note":"…","show":true}}'`.
It opens in its viewer beside the chat (or in its own tab), where the person
switches versions and, for html, points at elements to comment. Save again
with `"artifact_id"` for the next version; `{"ShowArtifact":{"id":"…"}}` shows
one again. If Neko has no viewer for a kind it is kept as a download, and a
viewer for it is a good plugin. html artifacts run in an opaque origin with no
network; other kinds are served as files and never run, so none of them can
use `host`, `request` or anything a plugin gets.

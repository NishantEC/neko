# DeepSeek Harness: how agent output becomes UI

Studied 2026-11-08 from shallow clones: `deepseek-ai/deepseek-harness` at
`fcc0ad46` (2026-11-04), `guiyidu-ui/dsh-plugin-3d-viewer` 1.1.0,
`GHJIVHIDD/dsh-plugin-canvas` 0.1.1, `walkzzz/dsh-plugin-blender-3d`. Nothing
was built or run. This report follows `deepseek-harness.md`, which covered the
prompts, the Cordis graph and the Creator loop; none of that is repeated here.

> Quoted code is from DeepSeek Harness (Copyright (c) 2026 DeepSeek, MIT),
> dsh-plugin-3d-viewer (Copyright (c) 2026 guiyidu-ui, MIT) and
> dsh-plugin-canvas (Apache-2.0). dsh paths are relative to `packages/`.

**In short.** The owner's example is three community plugins that never refer
to each other: `blender-3d` writes an STL/GLB, the model writes a
```` ```3d ```` fence holding its path, and `3d-viewer` rewrites that fence in
the DOM into a Three.js view whose button opens a sidebar tab. dsh's own
machinery is more disciplined: typed tool results rendered per tool name, a
file-suffix renderer registry, `dsh-resource://` file links, and read-only
introspection of the live slot tree. The **agent never opens a panel**; it
links files and the user clicks.

---

## 1. The output pipeline, end to end

**A tool result is structured and has two renderings.** `defineTool` takes
`output.schema` and `output.render`. `render` produces the text the model
sees; the structured value goes into the session log for clients to use. From
`deliverables/tool-present/src/index.ts`:

```ts
render: (_args, value) => [{ type: 'text', text: value.files.map(file => `Presented ${file.path}`).join('\n') }],
...
session.append('deliverables/presented', { turn, callId: exec.callId, files })
```

**Clients pick a renderer by the tool's wire name.** The keyed slot
`tool.call.toolview` is "dispatched by wire name, including custom tools …
unclaimed keys use the generic row" (`client/ui-tool/src/client/contract/slots.ts:17-27`).
`ui-deliverables` claims `key: 'present'`, registers the
`deliverables/presented` event as a chat row type, adds a card to
`conversation.chat.turnTail`, and links file names in the closing prose
(`client/ui-deliverables/src/client/index.ts`). The prompt side,
`deliverable-file-references`, says "link every mention of an existing file …
to its full path", images inline as `![…](<path>)`.

**How a click becomes a panel.** `client/ui-chat/src/client/apply.ts:258-275`
explains it: "Files open in the right Sidebar, not in a desktop application:
the content stays in the product, beside the conversation that produced it."
The steps:

1. `openFile` builds `dsh-resource://file/session/<id>/<path>`. A relative path is resolved at click time; a line number travels as a navigation parameter, so the same tab is revealed and scrolled.
2. `ctx.sidebarRight.openResource(address)` hands the address to the tab-type registry (`client/ui-sidebar-right/src/client/tab-registry.ts`), which works like VS Code's editor resolver. Each type declares glob `patterns` and an optional `canOpen` veto. Ties are ranked by priority band (`extension` > `builtin` > `fallback`), then by match length.
3. The document tab then chooses a body from `ctx.documentPreviews` (`client/ui-sidebar-documentpreview/src/client/document/registry.ts`). Each entry declares `{ id, extensions, binaryExtensions?, priority, loading: 'text-pages' | 'bytes-complete' | 'renderer' }`, and its component sits in the keyed slot `sidebar.right.tab.document` under the same `id`. The built-in entries cover text, markdown, html, image, pdf, office, excel and code. Matching is by **file suffix only**; there is no MIME sniffing.

**Code fences have no registry.** The markdown renderer special-cases only
`math`. Every other language goes to the highlighted `CodeBlock`
(`client/ui-primitives/src/markdown/render.tsx:419-447`). That is why the 3D
viewer resorts to a DOM hack.

**Who opens what.** Only the user. No tool opens the sidebar (`terminal_open`
is the only "open" tool), and `present` cards appear after the turn ends.

**When the file changes.** A document body watches its file through
`useResource`. A new version shows "The file has changed, showing the previous
content.", or reloads by itself when auto-refresh is on
(`TextPreview.tsx:188-200`). Renderers register every related file they read
through `addResource`, such as an HTML page's relative assets, so a change to
any of them marks the tab dirty.

**The way back to the agent.** The composer exposes `insertQuote(text)` and
`insertReference(ref, span)` (`client/ui-conversation/src/client/contract/input.ts:180-186`);
`conversation.chat.text-interaction` gives plugins selection gestures; a
client half gets `host.call(method, args)` to its own host half. Creator's rule
(`references/user-actions.md`) is "one operation, two callers": one host
method, called by the UI through `ctx.remote.commands.execute()` and by the
agent as a same-shaped tool; authority-granting actions stay user-only.

## 2. The slot model and the plugin format

**Slot kinds.** A slot is one of `'single' | 'list' | 'keyed' | 'chain' | 'hoc'`
(`client/ui-slots/src/index.ts:101`), scoped `root` or `session`. Some 110
slots are declared. The ones that matter for output:

| Slot | Kind | Role |
|---|---|---|
| `conversation.chat.node` | keyed by event kind | a chat row type of your own (register the event with `uiConversation.events`) |
| `tool.call.toolview` | keyed by tool name | how a tool call looks |
| `conversation.chat.turnTail` | list | cards after a turn |
| `conversation.view` | list | a view tab beside Chat/Trajectory (canvas uses it) |
| `sidebar.right.pane.tab` and `.title` | keyed by tab type | a right-sidebar tab |
| `sidebar.right.tab.document` | keyed by preview id | a body for a file suffix |
| `sidebar.right.tab.document.actions`, `.unpreviewable` | list | toolbar actions; the fallback when a file can't be shown |
| `conversation.composer.dock`, `conversation.input.*`, `shell.overlay`, `settings.section`, `sidebar.panellist`, `contextMenu.content` | | the chrome |

**The bundle.** `package.json` carries:

- `dsh.bundle.patch` (a `cordis.patch.yml` that inserts `{ id, name }`);
- `dsh.client.{ platform, immediately, inject, external }`;
- `exports["./client"]`.

The template's `client.js`:

```js
window.__ModuleLoader__.load({ id: '@local/my-decoration', factory(require) {
  const React = require('react');
  return { inject: ['slots'], apply(ctx) {
    ctx.slots.inject('conversation.composer.dock', () => ctx.slots.register({ name: 'conversation.composer.dock', id: 'my-decoration', order: 5 }, Decoration));
  } };
} });
```

**Lifecycle.** Every registration is a `ctx.effect` and is disposed with its
context. Client HMR applies enable and disable without a page reload
(`client/hmr/README.md`). `dsh.client.inject` is a *hard* dependency, so
optional services go through `ctx.inject([...], cb)`.

**How a plugin tells the model what it can do.** Three ways:

- tools, with a description, a `render` and a `presentCall`;
- `ctx.systemPrompt.section({ name, order })`;
- skills (`SKILL.md` with `name`, `description`, `whenToUse`) in scanned roots.

No manifest field ships a skill. `blender-3d` copies its skill into
`~/.dsh/skills` when it loads (`lib/index.js:42-49`).

## 3. How the agent "looks into everything"

`cordis_inspect_list` lists providers. `cordis_inspect_query { platform,
provider, method, input }` runs a query. Queries are read-only and "cannot
invoke business Service methods or modify the runtime".

| Platform | Provider.method | Returns |
|---|---|---|
| host | `Service.listService`, `Event.listEvents` | a compact directory, or one exact contract with its referenced types |
| host | `Config.listConfigs` | live loader entries, paged (≤100), with patch ids and config status; for one entry, its config as JSON Schema plus `packageDir` (README and built `lib/`) |
| host | `Tool.listTools` | every tool schema this agent can call right now |
| client (needs an open page) | `Slots.listSubTree { root }` | the live slot and factory tree; for one slot, its catalog entry and occupants |
| client | `Theme.listTokens` | `--dsw-alias-*` tokens and the light/dark rules |
| client | `Builtin.listBuiltins` | `ctx`, `React`, `host.call`, `styles.insert`, `console` |

It **cannot** see the DOM, logs, what is on screen, or pixels. Pixels come only
from `stagehand_screenshot` (experimental browser-use), and
`references/verification.md` bans launching a browser yourself. The loop:
inspect the slot, write, install, read `application`/`warnings`, confirm the
occupant with `Slots.listSubTree`, screenshot if a browser is attached, report
what was not verified. "Looks into everything" means typed catalogs plus a
live topology, not the screen.

## 4. The three community plugins

**3d-viewer.**

- **Host half** (`lib/index.js`): `GET /model3d/<absolute path>` via `ctx.webServer.register`; same origin, no `..`, obj/stl/gltf/glb, ≤60 MiB. No patch file; the row is inserted by hand.
- **Client half** (`lib/client.js`): a document-wide `MutationObserver` replaces `pre > code.language-3d` and `a[href^="/model3d/"]` with a Three.js view (from a CDN; ≤4 views; stops after 40 mounts in 8 s).
- **Panel**: `tabs.register({ id, kind, multiple: false, keepMounted: true, title, guide })` plus keyed `sidebar.right.pane.tab`/`.title`. The view's "侧栏预览" button calls `sidebarRight.openTab(PANEL_KIND, { revealIfOpened: true })`; the panel lists the session's models.
- **What the model knows: nothing.** No prompt section, tool or skill; the README says "在消息里让助手输出" (have the assistant write it).
- It breaks Creator's own rules ("Do not write DOM outside your component") and skips `documentPreviews`, so clicking a `.glb` link opens no 3D view.

**blender-3d.** Tools `blender_model`, `blender_scene` and `blender_setup` run
a Python bridge (trimesh + manifold3d, or real Blender) and export
STL/OBJ/PLY/GLTF. It has a skill and no UI. The only link to the viewer is the
model writing a path into a `3d` fence.

**canvas.**

- **Host half**: `/canvas-api/*` plus `canvas_preview` (`mode: render | annotate | clear`; `html | file | url`; `annotations [{ x, y (percent), note }]`, `notes`), in memory per session.
- **Client half**: a `conversation.view` entry `{ id: 'canvas', order: 13 }` polls `/canvas-api/state` every 2 s into an iframe without `allow-same-origin`.
- The model learns from the tool description, which tells it to send the user to the tab. Annotations go one way only, and a host-served iframe is what the practices advise against.

---

## 5. Neko: the gaps

| dsh has | Neko today |
|---|---|
| suffix → viewer registry (`documentPreviews`) | `KINDS` is fixed at 6 (`crates/neko-core/src/artifacts.rs:22`). `ArtifactPanel` draws html/svg/png and offers the rest as a download. A plugin cannot add a viewer. |
| per-tool and per-event renderers | `DISPLAY_KINDS` is fixed (`crates/neko-core/src/ui_message.rs:36`), mirrored by the `ReplyData` switch (`web/src/chat/reply-views.tsx:208-218`). Other fences go to Shiki in `MarkdownPre` (`web/src/chat/t3/ChatMarkdownRenderer.tsx:57`). |
| addressable sidebar tabs | `inspector` is one anonymous panel, last opener wins (`web/src/shell/inspector.tsx:66`). Only Home opens it, on `ArtifactsChanged` with a `message_id` (`pages/Home.tsx:46`). |
| tools, prompt sections and skills from plugins | A manifest has `title/description/version/entry/group` and nothing for the model. Agents learn only from `--plugin-guide`, and §5 says a plugin "cannot change Neko's chat". |
| `cordis_inspect_*` | `PluginStatus` (load and crash) and `--open-page`. Nothing lists pages, viewers or tabs, or what is on screen. |
| composer `insertQuote` | `ComposerHandle.reference`, but only the html comment bridge uses it; plugins cannot. |

Neko is already **ahead** in two places. The agent's save opens the canvas
automatically, and comment mode carries a pick back to the agent. Neither
community plugin does either.

## 6. Design, ranked

**1. A viewer registry (~1 day).** `host.registerViewer({ id, title, kinds:
["stl","glb"], priority, component })` in `host.ts`, with a store in
`registry.ts`. `ArtifactFrame` and `ArtifactTab` ask it before their built-in
html/svg/png branches. The component receives:

- `{ artifact, version, url, loadBytes(), reference(selector, text), onVersion }`;
- `reference` is a generalised `ComposerHandle.reference`.

The daemon accepts `artifacts::KINDS` plus extensions a manifest viewer
declares (item 2), stored as opaque bytes (size check only, served
`application/octet-stream`, `nosniff`, `ARTIFACT_CSP`); the trusted
same-origin plugin `fetch`es them, so the artifact sandbox is unchanged. Ship a
3D viewer (three.js vendored per plugin-guide §3) as the example. This is the
owner's example.

**2. Manifest capabilities and the prompt (~half day).** Add three fields:

- `viewers: [{ id, kinds }]`;
- `fences: ["3d"]`;
- `prompt`: one paragraph, at most 400 chars.

The daemon reads the manifests and appends a generated, plugin-labelled
"Installed viewers" section to `super_instruction` and the builder prompt
("`.glb`/`.stl`: SaveArtifact it; it opens in *3D viewer*"). This is dsh's
"the tool owns its prompt section", and fixes the 3D viewer's blind model.

**3. Agents open and update panels (~2-3 h).** Give `SaveArtifact` a `show:
"beside" | "tab" | "none"` field, and add `ShowArtifact { id, version, show }`.
Both emit an `Event` that every page obeys, not only Home. Following the latest
version already works through `ArtifactsChanged`. Add "pinned or follow
latest" to the panel.

**4. `InspectApp` (~1 day).** The web app reports
`ReportUi { pages, viewers, fences, commands, settings_sections, task_tabs, tabs, active_tab, inspector: { owner, title }, theme, plugin_problems, recent_console_errors }`.
on change (debounced, in memory, like `ReportPlugin`). `InspectApp { what }`
answers from it with `age_ms`, or says "no page open". Pixels: `--open-page`
plus a screenshot via the shared Chrome, never one the agent launches. Add
dsh's rule: registration alone does not establish what the user sees.

**5. Fence renderers (~half day, web only).** `host.registerFence({ lang,
component })`, consulted first in `MarkdownPre`. Keep the code block until the
fence closes, as `math` does in dsh. Leave `DISPLAY_KINDS` alone; plugin fences
are not proposals. It is lower in the ranking because a fence holding a path is
weaker than an artifact: the file is not copied, has no version and is not
contained.

**6. Plugin-guide changes (~20 min).** Rewrite §5 and §6: plugins may add
viewers and fences, never chat chrome. Name the anti-patterns: DOM scanning
(3d-viewer), host-served polled iframes (canvas), CDN libraries.

**Not worth copying:** sidebar address globs (Neko's tabs are keyed by hash
route), five slot kinds (three registries cover it), graph HMR.

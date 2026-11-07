# Everything renders: viewers, panels and an app the agent can see

Date: 2026-11-08. Owner: in DeepSeek Harness "if I ask it to create a 3D
object it creates it and opens it in a side panel that is a 3D viewer … and
much more complex stuff; it creates plugins in a format where the AI is able
to look into everything." Research: `docs/research/deepseek-harness-outputs.md`,
`docs/research/outputs-hermes-herdr-opendesign.md`. Builds on
`docs/superpowers/specs/2026-11-06-artifact-canvas-design.md` and DESIGN.md.

## Principle

An output has a **kind**; a **viewer** is registered for kinds; the **model is
told** which viewers exist; the **agent can open** what it made; and the agent
can **see** the live app to check its work. Built-in viewers and plugin
viewers are one registry (as pages already are).

## Phase A: everything renders

1. **Any artifact type.** `SaveArtifact` accepts any file up to the size cap;
   the kind is its extension (normalised: `glb`, `gltf`, `stl`, `obj`,
   `excalidraw`, `md`, `csv`, `json`, `png`/`jpg`/`webp`/`gif`, `pdf`, `svg`,
   `html`, source-code extensions…). The daemon stores bytes, serves them with
   the right MIME (html keeps its sandboxed route and bridge; others are
   served as files, never executed). `gltf` that references sibling files is
   accepted as a folder or as `.glb`; v1 may require `.glb`.
2. **Viewer registry.** `host.registerViewer({ id, title, kinds: [..], component, priority? })`
   in `web/src/host.ts`; the canvas panel and artifact tabs pick the highest
   priority viewer for the artifact's kind, falling back to "Download". Built-in
   viewers registered the same way: html (existing iframe), image, pdf
   (browser viewer), markdown (rendered), code/json/text (highlighted, read
   only), csv (table), **3d** (three.js, orbit, grid, fit, wireframe; the
   vendored three.js and loaders move from the owner's 3d-viewer plugin into
   the web bundle as a dependency, licence kept) and **excalidraw** (the
   Excalidraw build from the Canvas plugin, read/write: saving writes a new
   artifact version). A viewer gets `{ artifact, version, url, theme, onEdit? }`.
3. **Fences in replies.** `host.registerFence({ language, viewer | component })`:
   a chat reply's fenced block whose language is registered renders through it
   (e.g. ```` ```3d ```` with a path or artifact id, ```` ```mermaid ````, ```` ```csv ````).
   Unknown languages stay code blocks.
4. **Manifest declares, the model learns.** A plugin manifest may add
   `"viewers": [{ "id", "kinds", "title" }]`, `"fences": ["lang"]` and
   `"prompt": "one or two sentences telling an agent when to use this"`. The
   daemon builds an **Installed viewers** section from built-ins plus enabled
   plugins (kinds, fences, prompt lines) and appends it to the full-power chat
   instruction and routine prompts, and `--plugin-guide` prints it too.
5. **The agent opens things.** `SaveArtifact` gains `show: bool` (default true
   from the full-power chat); a new `ShowArtifact { id, version? }` and
   `OpenTab { route }` request are broadcast as events and every open Neko page
   obeys (opens the canvas panel or the tab). The full-power instruction says:
   make visual output an artifact of its natural kind (a 3D model as `.glb`, a
   sketch as `.excalidraw`, a table as `.csv`), then show it.

## Phase B: the agent sees and hears the app

1. **InspectApp.** The web app reports to the daemon (in memory, like
   `PluginStatus`): registered pages, viewers, fences, plugins and their
   health, open tabs, the active route, the open artifact and version, and
   the theme. `InspectApp` returns it; `neko-daemon --request '"InspectApp"'`.
2. **Artifact errors back.** The html bridge reports console errors and
   uncaught exceptions (`postMessage`), viewers report load failures; the
   daemon keeps the last per artifact version; `Artifact { id }` includes
   them; the instruction says to check before saying it works.
3. **Comments target stamped ids.** Before serving html, the daemon stamps a
   stable `data-neko-id` on elements that lack an id; comment picks send that
   id plus a selector, and the composed message ends with a scope fence:
   change only the referenced element unless asked otherwise.
4. **Artifacts talk back.** An element with `data-neko-send="text"` in an
   artifact posts that text to the conversation it belongs to (validated
   source; shown to the user as a sent message).

## Non-goals

Pixel screenshots of the app from the daemon (agents can use the browser or
computer-use tools), arbitrary plugin code inside artifact iframes, a vector
editor.

## Anti-goals

An artifact or viewer reaching Neko's token or API beyond what the bridge
allows; owner-specific anything; a viewer the model cannot learn about.

# How agent output becomes UI: Hermes, herdr-app, Open Design (2026-11-08)

Shallow clones, read only, nothing built or run: `NousResearch/hermes-agent`
63f68522 (includes the official Electron app `apps/desktop`), `nesquena/hermes-webui`
f328d3f, `fathah/hermes-desktop` 1710248 (2026-10-13), `herdrdev/herdr-app` 7fe7ac3,
`herdrdev/herdr-core` cd12ddd, `nexu-io/open-design` 87a4f91. This covers only what
`hermes-agent.md` and `herdr.md` did not: the path from output to panel.

> Quotes: Hermes Agent © 2025 Nous Research, MIT. herdr © the herdr authors,
> Apache-2.0. Open Design © nexu-io, Apache-2.0. Quoted verbatim; "…" marks cuts.

---

## 1. Hermes

### Output pipeline
- **The model is told what renders, per surface.** `agent/prompt_builder.py:685`
  `PLATFORM_HINTS["desktop"]`:
  > "Inline widget/chart (living IN the chat): write an HTML file, then put
  > ::preview{file=\"path.html\"} alone on its own line (plugins can register more
  > ::name{...} directives). … Widgets talk back: data-hermes-send=\"prompt\" on any
  > clickable element (or window.hermes.send(\"prompt\")) sends that prompt as a hidden
  > user turn — answer it by updating the widget's file, not with prose."

  Files go out as `MEDIA:/abs/path` on every surface. The webui gets no hint at all
  (`prompt_builder.py:713-724`).
- **Official desktop (`apps/desktop/src/components/assistant-ui/`).** Rendering has
  four layers:
  1. `markdown-text.tsx:788` overrides the code block. `lib/artifact-detect.ts`
     *promotes* a large fence to an artifact card in the right rail:
     HTML ≥160 chars, SVG ≥2000 chars, code ≥48 lines.
  2. Other fences go to `embeds/registry.tsx` (`LAZY_FENCE = {listing, mermaid, svg}`,
     loaded lazily) or to Shiki.
  3. `::name{…}` directives resolve against a plugin registry
     (`lib/transcript-directives.ts`).
  4. `MEDIA:` files go to built-in players or a plugin's `transcript.toolMedia`
     renderer.

  `::preview` (`inline-preview-directive.tsx`) is a `srcdoc` iframe with
  `sandbox="allow-scripts"`, a theme prelude and a size-reporting script. Messages
  carry a per-mount token, and intents are length-capped and throttled.
- **Panels the agent opens.** The `desktop_ui` toolset (`toolsets.py:135`) includes
  `desktop_preview` open/close/read, `focus_pane`, `annotate_preview`, `show_tip` and
  `set_theme`. Each emits a renderer event such as `preview.open` or `pane.reveal`
  (`tools/desktop_ui.py`).
- **Staying linked.** `electron/preview-file-watch.ts` re-renders a preview when its
  file changes. Comment mode (`lib/preview-annotate/pack.ts:101`) writes "I left a
  comment on <url> … Address it and keep the scope narrow."
  `right-rail/preview-console.tsx:182` puts the preview's console logs into the
  composer.
- **webui (`static/ui.js`).** Typed SSE events: `token`, `tool`, `tool_complete`,
  `todo_state`, `approval`, … (`docs/rfcs/session-sse-contract-v1.md`).
  - A fence switch (`ui.js:10730`) handles mermaid (from a CDN), diff, a json/yaml
    tree and a csv table.
  - `MEDIA:` is routed by extension to img, pdf.js, html, diff, csv and Excalidraw
    viewers (`ui.js:4085-4446`).
  - When a message settles, each referenced file is frozen into a content-addressed
    store and `&snap=<sha>` is added to its URL (`_stampMediaSnapshots`, `ui.js:2111`),
    so old messages keep showing old bytes.
  - The workspace panel (`static/workspace.js`) collects artifacts from mutation tool
    calls (`ARTIFACT_MUTATION_TOOLS`). `refreshOpenPreviewIfMutated` (line 540)
    reloads the open preview when a tool edits that file. Edits made in the panel are
    saved to disk but never sent to the agent.
- **fathah desktop** adds little: a web preview that opens itself on browser-tool URLs
  (`hooks/useChatIPC.ts:340`) and an element picker that writes to the composer.

### Extension model
- **Desktop:** `plugin.js` is an ESM default export. There is no JSON manifest.
  `ctx.register({id, area, render, data})` takes these areas (`src/sdk/areas.ts`):
  - `panes`, `routes`, `sidebar.nav`
  - `palette`, `keybinds`, `themes`
  - `statusBar.*`, `titleBar.*`
  - `transcript.directives`, `transcript.toolMedia`
  - `chat.empty`, kanban card badges

  The guide (`skills/…/hermes-agent/references/desktop-plugins.md:108`) says:
  > "Unclaimed/malformed directives fall back to plain text … After registering one,
  > TELL the model it exists (a bundled skill or the user's instructions) — it won't
  > discover the name on its own."

  `lib/transcript-directives.ts` states the safety rule: "The guard is the CLAIM, not
  its position."
- **webui:** extensions are opt-in injected scripts (`api/extensions.py`,
  `hermesExt.register`, message actions, turn events). Dashboard plugins become
  iframe tabs (`manifest.json` `{name,label,tab:{path,label}}`). Neither can add a
  renderer.

### Introspection
- `drive_preview` (`tools/drive_preview_tool.py:66`, "ALWAYS start with
  action='elements'") returns refs and deltas for elements in the preview, which can
  be a localhost dev server.
- The optional skill `inspecting-hermes-desktop-dom` reads the app's own DOM,
  computed styles and console over CDP, in dev builds only.
- The webui's `mcp_server.py` exposes only project and session housekeeping.

---

## 2. herdr-app (herdr 2)

### Output pipeline
Agents emit **terminal output and nothing else**. There are no message parts and no
artifacts.
- The window adds Kitty graphics, path/URL links (`src/links.rs`) and OSC 7501 status.
- Rich viewing happens in **mods**. `TextView.markdown`, `Editor` (read-only code with
  syntax highlighting) and `image()`/`svg()` are the renderers. Per `defaults/AGENTS.md`
  §UI:
  > "A viewer opens beside the shell as a pane:
  > `pane.split {direction, mod, props: { file }}`."
- **Agents open panels with commands.** `herdr-app do <command>` covers `tab.new`,
  `pane.split {mod, props}`, `mod.open` and `openOverlay` windows, and keys can send
  `<mod>.<command>` to a mod's `nextCommand()`. The app opens nothing by itself.
- **Live linking is the mod's job:**
  - `watchFolder(dir)` returns changed paths; the `files` and `changes` mods loop on
    it (`defaults/mods/files/main.js:54`).
  - `render` re-runs whenever spaces, agents, focus, theme or `shared()` state
    change.
  - Selections do not flow back to an agent. A mod can `hc send` text into an agent's
    terminal, but no shipped mod does.

### Extension model
- **Every panel is a mod**, including sidebar, tabs, toast, confirm and diff:
  - `mods/<name>/main.js` (a class extending `View`, GPUI builders, no DOM)
  - `mod.json` `{ "place": "right"|"left"|"top"|"bottom"|"none", "size": 320, "focusable": true }`
  - Layouts (`layouts/*.json`) then decide placement, and saving a file hot-reloads
    every copy of the mod.
  - Mods are trusted: `exec`, `fetch` and fs.
- **How the AI learns what exists:**
  - On every start the app regenerates `~/.config/herdr-app/AGENTS.md` (9.4k words)
    plus `herdr.d.ts` and `gpui-kit.d.ts`; "the type files here are the truth".
  - `herdr-app commands` lists every command and its arguments, even with no window
    open.
  - `herdr-app --skill` routes the agent: "Use `herdr-app` only to change what the
    person sees."

### Introspection (the strongest of the three)
`src/SKILL.md` gives the agent four commands:
- `herdr-app ls` shows spaces, tabs, terminals and pane IDs.
- `herdr-app status` shows layout, focus, overlays, and file and mod errors
  (`window.rs:3767`).
- `herdr-app check <mod>` loads the mod and renders it once, headless.
- `herdr-app shot` saves the app's own last frame, even when covered, with no
  permission needed.

> "Never call a mod done without a passing check. Then confirm it is shown correctly
> in the running window with `shot`; don't claim a visual check you haven't made."

A broken save keeps the last good build and reports the error in `status`: "a
still-working screen is not proof the edit worked."

---

## 3. Open Design

### Output pipeline
- **Output modes.** Agents that can write files put real files in the project.
  Plain-stream models emit `<artifact identifier type title>`, which
  `apps/web/src/artifacts/parser.ts` parses as it streams.
- **SSE events** (`packages/contracts/src/sse/chat.ts`): `text_delta`, `tool_use`,
  `tool_result`, `artifact_focus`, `live_artifact`, `live_artifact_refresh`,
  `next_steps`.
- **Tags in prose that become UI:** `<question-form>`, `<od-card>`, `<od-next>`,
  `<od-focus>` (`apps/daemon/src/prompts/system.ts:618`).
- **Choosing a viewer.** A sidecar `<name>.artifact.json` sets `kind`
  (`html|deck|react-component|markdown-document|svg|diagram|mini-app|…`); otherwise
  the extension decides. `artifactRendererRegistry`
  (`apps/web/src/artifacts/renderer-registry.ts`) entries declare
  `canRender(ctx)`, `supportsStreaming` and an optional `renderPartial`.
  `FileViewer.tsx:2060` handles image, video, sketch, code, pdf, pptx and xlsx.
- **How the preview opens.** The agent emits
  `<od-focus key="<per-turn nonce>" open="index.html"/>`. The server checks the file
  is inside the project and non-empty, then sends `artifact_focus`. The nonce means
  text the agent merely *read* cannot steer the preview. From
  `artifact-focus-marker.ts:298`:
  > "Changing an existing file is delivering it. … "Did I create this file" is not the
  > question — "what should the user look at now" is."

  Fallback: `auto-open-file.ts` ranks the turn's written files.
- **The iframe.** `shouldUrlLoadHtmlPreview` picks URL or `srcdoc`. `buildSrcdoc`
  (`runtime/srcdoc.ts:357`) injects `<base>`, fonts and bridges for selection,
  inspect, edit, tweaks, deck, snapshot and size. Sandbox is
  `allow-scripts allow-forms allow-downloads`. CSP has `connect-src 'none'`.
  `annotateMissingOdIds` (`edit-mode/bridge.ts`) adds stable `data-od-id`s **before**
  rendering, so comments target ids, not brittle selectors.
- **Live reload.** A chokidar watcher per project feeds
  `GET /api/projects/:id/events` → `file-changed`, with a 180 ms debounce. Reloads
  from the app's own writes are suppressed.
- **Comments to the agent.** `od:comment-target {elementId, selector, text,
  position, htmlHint, style}` becomes an `<attached-preview-comments>` block
  (`apps/web/src/comments.ts:481`):
  > "Hard scope: change ONLY the elements identified below by selector / position /
  > pod members. Do NOT modify sibling sub-pages, parent layout, global CSS, design
  > tokens, or unrelated rules even if you notice issues there — surface those as a
  > follow-up note in your reply instead…"

  Drawn marks attach a screenshot path. Inspect tweaks (`od:inspect-set`) are written
  straight to the file with no agent turn (`saveInspectToSource`).
- **Exports and sketches.** HTML, zip, PDF, and PPTX built from one screenshot per
  slide (`runtime/exports.ts`, `daemon/src/deck-export.ts`). Excalidraw saves
  `*.sketch.json` and `sketch.png`, which reach the agent as file paths.
- **Weak spots:** the bridge does not check `ev.origin` (`srcdoc.ts:1790`), and nothing
  reads the `EDITMODE` markers the prompt asks for.

### Extension model
- Plugins are `open-design.json`, with 479 manifests under `plugins/_official`. A
  manifest contributes:
  - skills, context and pipelines (`until: critique.score`, at most 10 rounds);
  - form and oauth surfaces, connectors and MCP;
  - capabilities such as `prompt:inject` and `fs:write`.
- Skills, design systems and `craft/*.md` add prompt layers.
- **Plugins cannot add renderers:** "agent/plugin output is data; OD owns the
  renderer" (`docs/plugins-spec.md` §10.3).
- The model sees only *active* skills, assembled in `composeSystemPrompt`. It reaches
  the app through `od` CLI wrappers and the `live_artifacts_*` MCP tools.

### Introspection
- One render per task through `od export --format image` (`core-slim.ts:265`).
  `official-system.ts:109`: "**Do not loop.** One render check is the budget."
- Host-side checks: `lint-artifact.ts` (reported back as `<artifact-lint>`), a syntax
  repair loop, the five-judge Critique Theater and `qa/deck-layout.ts`.
- Preview console errors go **only to analytics**
  (`contracts/src/runtime/preview-observability.ts`). The agent cannot see the live
  DOM.

---

## 4. Neko today, and what to take

**Neko today:** reply views are a closed list (`DISPLAY_KINDS`, six kinds,
`crates/neko-core/src/ui_message.rs:36`; a fixed switch in
`web/src/chat/reply-views.tsx:207`; schemas taught by the hard-coded `REPLY_VIEWS`,
`neko_chat.rs:718`). `SaveArtifact` copies versions in and links them to the pending
turn; the canvas opens in the inspector, comment picks become a composer chip, and
json/md/pdf only download. Plugins (`web/src/host.ts`) get pages, ⌘K, Settings
sections, task tabs and `inspector`, but cannot render a message part, an artifact
kind or a slot. Mermaid is stubbed (`web/src/stubs/mermaid.ts`). Agent introspection
is `PluginStatus` plus `--open-page`: no UI state, screenshot or artifact errors.

**Already ahead:** immutable copied versions (Hermes's webui added snapshots later);
artifacts open on an explicit save, not heuristic promotion (which Hermes itself
describes as "substantial fences get promoted whether or not the model asked"); the
`sandbox` CSP directive on direct open; the `event.source` check Open Design lacks;
`previous/` rollback (herdr's last-good build).

### Ranked recommendations

| # | Change | Where in Neko | Effort |
|---|---|---|---|
| 1 | **Reply-view registry.** `host.registerReplyView({kind, description, schema, component})`, mirrored as manifest `views: [...]` so the daemon knows them. `ReplyStream` types `neko-<kind>` only if built in or claimed (Hermes: "the guard is the CLAIM"; unclaimed stays text); `REPLY_VIEWS` gets one generated line per view, since the model "won't discover the name on its own". Mermaid or a timeline become opt-in plugins. | `ui_message.rs`, `neko_chat.rs`, `reply-views.tsx`, `host.ts`, `plugins.tsx` | 1–1.5 days |
| 2 | **`UiState` + `--shot`.** Open tabs, active tab, inspector contents, plugin problems, recent web console errors (reported like `ReportPlugin`); `neko-daemon --shot <route>` via shared Chrome. Add herdr's "don't claim a visual check you haven't made" to the guide. | `neko-protocol`, `server.rs`, `plugins.tsx`, `main.rs`, `plugin-guide.md` | ~1 day |
| 3 | **Artifact errors reach the agent.** Bridge posts `window.onerror`/`console.error`; canvas stores them on the version (`ReportArtifact`); `Artifact {id}` returns `errors`; instruction: check after saving. | `web::COMMENT_BRIDGE`, `ArtifactFrame`, `neko_core::artifacts`, `super_instruction` | ~half day |
| 4 | **Sturdier comments.** Stamp `data-neko-id` before serving (Open Design's `annotateMissingOdIds`) so picks survive the next version; send computed style and an HTML hint; wrap in a "Hard scope…" fence. | `web.rs`, `canvas/artifacts.ts::referenceText` | ~half day |
| 5 | **Artifacts talk back.** `[data-neko-send]` click → `neko-send {prompt}` → a capped, marked user turn; instruction: "answer by saving a new version, not with prose" (Hermes). | bridge, `ArtifactPanel.tsx`, `super_instruction` | ~half day |
| 6 | **Viewers and slots.** `host.addArtifactViewer({kind, component})` (json/md/pdf stop being download-only) and `host.addSlot({slot: "statusBar"|"chatEmpty"|"tabStrip.end"})`, after Hermes's `areas.ts`; each returns a remover. | `host.ts`, `extensions.ts`, `ArtifactPanel.tsx`, `StatusBar.tsx` | ~1 day |
| 7 | **Panel requests.** `OpenTab {href}`, `ShowArtifact {id, v, where}` as client events, so task builders (not only a pending Home turn) can show results. Tool calls, so no nonce needed. | `neko-protocol`, `server.rs`, `shell/tabs.ts` | ~half day |
| 8 | **Self-describing surface.** `--plugin-guide` appends the live registry; daemon writes `host.d.ts` to the plugins folder on start (herdr). | `main.rs`, `host.ts`, plugins dir | ~half day |
| 9 | **Inline artifacts** (Hermes `::preview`): a small artifact drawn in the reply via the same `/artifact` frame plus a size-reporting bridge. | `ArtifactCard.tsx`, bridge | ~half day |

**Later:** a file-watched "live" artifact (Open Design's `live_artifact_refresh`; wait
for demand, copy-in is safer); PNG/PDF export through the shared Chrome; Open Design's
"One render check is the budget" as a cost guard once #2 exists.

**Do first:** #3 and #4, a day together and entirely inside the canvas, then #1.

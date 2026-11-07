## Everything renders (2026-11-08)

Owner, after DeepSeek Harness: "if I ask it to create a 3D object it creates it
and opens it in a side panel that is a 3D viewer". Spec:
`docs/superpowers/specs/2026-11-08-everything-renders-design.md` (Phase A
built; Phase B, the agent seeing the app, is not). Research:
`docs/research/deepseek-harness-outputs.md`,
`docs/research/outputs-hermes-herdr-opendesign.md`. Builds on
`the-canvas-beside-the-chat.md`.

- **Any artifact type.** `neko_core::artifacts::kind_of` is the extension,
  lowercased, 1–16 letters or digits, aliases folded (`htm`→html,
  `jpeg`→jpg, `markdown`→md, `yml`→yaml). No extension is refused. Known kinds
  are checked (`check_contents`: text kinds UTF-8, svg has `<svg`,
  json/excalidraw/gltf parse, png/jpg/gif/webp/pdf/glb signatures); unknown
  kinds are kept as bytes. Stored as `v<n>.<kind>`. 5 MB cap and path
  containment unchanged. `artifacts::mime(kind)` is the served type: only
  html is `text/html` (with the bridge, as before); svg is an image under the
  sandbox CSP; code and markup a browser could run (`js`, `xml`, `sh`…) are
  `text/plain`; unknown (`xhtml`, `zip`) are `application/octet-stream`.
  `nosniff` is on every reply. **PDF** gets `web::PDF_CSP` (`default-src
  'none'; object-src 'self'`, no `sandbox`): Chrome's PDF viewer refuses to run
  in a sandboxed document. So a PDF loads same-origin and unsandboxed, framed
  without a `sandbox` attribute; accepted (reviewed 2026-11-08) because the
  browser's viewer, not the page, runs it and the policy allows no script,
  connection or form. A download keeps `ARTIFACT_CSP`. PDF rendering was not
  exercised by the headless verify.
- **`EditArtifact { artifact_id, version, content, note }`** keeps UTF-8 text
  as the next version in the kind of `version`, the one the viewer showed
  (unset: the newest), read under the save lock (`artifacts::save_text`: text
  kinds only, same checks and cap; editing a png version is refused even if
  the newest is a sketch). The sketch viewer saves through it.
- **Showing.** `SaveArtifact.show: Option<bool>`; unset means "a Home turn is
  pending" (only the full-power chat can save files, so that is where the
  person is looking). `ShowArtifact { id, version? }` and `OpenTab { route }`
  answer `Shown { pages }`, the number of open web pages that got the event
  (`server::broadcast` now returns it; 0 means nobody saw it). Events
  `ShowArtifact` / `OpenTab`; the GPUI client ignores them. The web shell
  mounts `obeyAgentRequests` (`canvas/artifacts.ts`). **Only one page acts:**
  the focused one, else the one used last (each page writes its id to
  localStorage `neko.lastFocusedPage` on focus or becoming visible, and hands
  it back on `pagehide`); the others show a toast with "Show here" / "Open
  here". Without this every open window jumped. ShowArtifact sets the canvas
  target and, when Home is not the active route, opens the artifact's tab;
  OpenTab sets the hash after `tabHash` re-checks it.
  `neko_core::viewers::tab_route` refuses schemes, `//`, `..`, spaces and
  anything outside `[A-Za-z0-9-_:/?=&.%]`. Home no longer opens the canvas on
  `ArtifactsChanged`; it opens on `ShowArtifact`, so `show: false` stays off
  screen.
- **Viewer registry** (`web/src/canvas/viewers.ts`). `host.registerViewer({ id,
  title, kinds, component | load, priority?, edits? })`; a plugin's id becomes
  `<plugin>:<id>` and is taken back on dispose like its commands. `viewerFor
  (kind)`: highest priority, then the latest registered (a plugin replaces a
  built-in at equal priority). Props: `{ artifact, version, kind, url, text?,
  theme, onEdit?, commenting?, onPick?, onCancel?, inline? }`;
  `host.artifacts.readText/readBytes(props)` read either source. A plugin
  viewer renders inside `PluginBoundary` (crashes reach `PluginStatus`).
  `ArtifactFrame` falls back to Download when no viewer claims the kind.
- **Built-ins** (`canvas/builtin-viewers.tsx`, registered from
  `pages/index.ts` before plugins load; keep in step with
  `neko_core::viewers::BUILTIN`): html page (the old iframe + comment
  bridge), image (svg still framed), pdf (plain iframe), markdown (the reply
  renderer, so fences inside work), and lazy chunks for csv/tsv
  (`CsvViewer`, RFC 4180 parser, reply table frame, 2000 rows), code
  (`CodeViewer`, Shiki, JSON pretty-printed), 3D (`ModelViewer`) and the
  sketch (`SketchViewer`).
- **3D** is `three@0.170.0` as a web dependency (licence in
  `THIRD_PARTY_LICENSES/three-MIT.txt`), ported from the owner's 3d-viewer
  plugin: theme colours through a canvas, render on change only, and
  `packGlb`, which turns a .glb or a self-contained .gltf into one in-memory
  .glb with images as `data:` URLs, because the app CSP refuses `fetch` of
  `blob:`/`data:` (three's ImageBitmapLoader) while `<img>` may load `data:`.
  A .gltf with sibling files is refused with a message (save as .glb).
- **Sketch** is `@excalidraw/excalidraw@0.18.1` (React 19 works despite its
  peer range; licence `THIRD_PARTY_LICENSES/excalidraw-MIT.txt`). Fonts:
  `vite.config.ts` copies `dist/prod/fonts/*` without Xiaolai (12 MB CJK) into
  `dist/excalidraw/fonts/`; `viewers/excalidraw-assets.ts` is imported first
  and sets `EXCALIDRAW_ASSET_PATH = "/excalidraw/"`. **Gotcha:** Excalidraw
  always appends esm.sh as a second `url()` for every face and adds every
  registered face to `document.fonts`; Chrome logged a CSP violation per
  refused source (230 for one sketch) even though the local font loaded. The
  same module wraps `window.FontFace` to drop cross-origin sources and the
  unshipped Xiaolai ones (falling back to `local()`, which never fetches).
  "Save as v<n+1>" is enabled once the scene version changes; the baseline
  is reset during render keyed on the text (not in an effect, which would run
  after Excalidraw's first mount `onChange`).
- **Fences** (`chat/fences.tsx`, used by `MarkdownPre`): once a reply settles,
  a block whose language has a fence is drawn by it; streaming blocks stay
  code. A viewer fence gets the block's text only when the language is one of
  the viewer's kinds (`csv`, `tsv`), or an artifact when the block is
  `artifact <id> [v<n>]` or a bare 12-hex id (a one-word csv stays text) and
  the artifact's kind is one the viewer draws; anything else stays code (a
  ```3d naming a csv, or holding prose). Fence viewers mount only while within
  400 px of the screen (`WhileOnScreen`, IntersectionObserver) and unmount
  when they leave, keeping their height: browsers cap live WebGL contexts at
  about 16, and a long chat may hold more ```3d blocks. Built-in: csv, tsv, 3d. **Mermaid is not drawn**: the
  library is ~30 MB of chunks (`stubs/mermaid.ts`), not cheap.
- **The model learns.** `neko_core::viewers::capabilities(manifest)` bounds
  `viewers` (≤8, kinds via `kind_of`), `fences` (≤8), `prompt` (one line,
  ≤400 chars); the plugin listing passes them through (`PluginInfo`).
  `installed_section(neko, plugins_dir)` (enabled plugins only, read on every
  call) says how to save and show, lists built-ins then plugin viewers, the
  fences, and each plugin's prompt. Appended to `super_instruction` (new
  `viewers` argument), inserted in routine `build_prompt` (with `{neko}`
  replaced by the caller), and printed after the guide by `--plugin-guide`.
  The instruction now says: an artifact of its natural kind (.glb, .excalidraw,
  .csv, .md, .html), shown, not described.
- **Bundle:** main chunk 642 KB → 650 KB (gzip 185 → 188 KB). ModelViewer
  ~600 KB, the Excalidraw chunk ~1.1 MB plus its 1.8 MB font-subsetting chunk,
  all loaded only when such an artifact opens. `dist/` 30 → 46 MB with source
  maps.
- **Verify:** `NEKO_WEB_ONLY=viewers,canvas node scripts/web-verify.mjs`
  (`scripts/verify/viewers.mjs` builds a tetrahedron .glb itself, saves csv,
  md and excalidraw, draws on the sketch and saves v2, checks the seed reply's
  ```csv fence, ShowArtifact from another tab, OpenTab and its refusal, and
  that fonts come from Neko). Rust: `artifacts::tests`, `viewers::tests`,
  `server` artifact test, `web_tests::every_other_kind_is_served_inert…`.
- **Not built (Phase B):** `InspectApp`, artifact errors reported back,
  stamped comment ids, artifacts that send messages. Pixel checks of the 3D
  view in WKWebView were not run (headless Chrome only).

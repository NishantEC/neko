## The canvas beside the chat (2026-11-06)

Owner: a canvas "like Figma, how Open Design does it", in Neko itself, not a
plugin page; the chat is the centre and what it makes appears beside it.
Spec: `docs/superpowers/specs/2026-11-06-artifact-canvas-design.md` (the
authority for what is in and out of v1).

- **Artifacts are files Neko owns** (`neko_core::artifacts`):
  `<data dir>/artifacts/<id>/meta.json` (`neko_protocol::Artifact`: id, title,
  kind, workspace_id, message_id, versions `[{n, file, at_ms, note,
  message_id}]`) and `v<n>.<ext>`. The id is 12 hex characters. An agent writes
  the file anywhere and sends `SaveArtifact { title, file, workspace_id,
  artifact_id, note }` (`neko-daemon --request`); the daemon *copies* it in, so
  the canvas never follows a path outside the store. Checks: absolute path,
  regular file, 5 MB (read with `take(MAX + 1)`, so a growing file is refused
  too), kinds html (htm), svg, png, pdf, json, md, and contents that match the
  kind (UTF-8 text, an `<svg`, JSON that parses, the PNG and PDF signatures).
  **Since 2026-11-08 any kind is kept** (the extension; glb, csv, excalidraw…)
  and drawn by the viewer registered for it, the agent can show what it made,
  and Home opens the canvas on `ShowArtifact` rather than on every save:
  `everything-renders.md`.
  Ids are 1-64 of `[A-Za-z0-9_-]`; `version_file` canonicalizes and proves the
  version is a file inside the artifact's own folder, so an edited
  `meta.json` or a planted symlink leads nowhere. Saves take one lock, so
  version numbers never collide. `Artifacts { workspace_id }` lists newest
  first; `Artifact { id }` is one record.
- **Linked to the reply running at that moment.** `server::save_artifact`
  takes the pending Home turn (`neko_chat::pending_turn`; one Neko turn runs at
  a time): its id becomes `message_id` (the first reply stays on the artifact;
  each version keeps its own), its workspace is used when the agent named
  none, and `neko_chat::show_artifact` publishes a `data-neko-artifact` chunk
  keyed by the artifact id into the reply in flight, so a second version in the
  same reply updates the card instead of adding one. It is *not* a
  `DISPLAY_KINDS` block: a model cannot write one by itself. Then
  `Event::ArtifactsChanged { id, version, message_id }` goes to every client
  (the GPUI client ignores it; Swift ignores all events).
- **Rendering is isolated.** `GET /artifact/<id>/<n>` (`latest` for the
  newest; same Host/Origin/Sec-Fetch-Site/token gates as every route; never
  asks the daemon, it reads the store like `/plugins/`) replies with
  `web::ARTIFACT_CSP` *instead of* the app's policy (`write_head` skips the
  default when a reply brings its own): the spec's directives plus
  `form-action 'none'`, `base-uri 'none'` and `sandbox allow-scripts`. The
  `sandbox` directive matters for **Open in browser**: a tab showing
  the artifact on its own would otherwise run its scripts with Neko's origin
  and could POST a `text/plain` form at `/api/request`. With it, the
  artifact has an opaque origin everywhere. The web app frames it in
  `<iframe sandbox="allow-scripts">`, never `allow-same-origin`.
  `?download=1` serves the file as saved, as an attachment named from the
  title. Assets an artifact references by URL do not load (an opaque origin
  carries no cookie): artifacts are single files, with CSS, scripts and
  images inline or as `data:`.
- **Comment mode.** html versions get `web::COMMENT_BRIDGE` before the last
  `</body>` (or at the end). The parent posts `{type:"neko-comment-mode", on}`
  with target `"*"` (the only way to address an opaque origin); the bridge
  outlines the hovered element, swallows the design's own clicks while on, and
  posts `{type:"neko-pick", selector, text (120 chars), rect}`; Esc posts
  `neko-comment-cancel`. The bridge answers only `parent`, and the canvas reads
  only messages whose `event.source` is its own iframe's `contentWindow`. A
  pick becomes a chip in the composer (`ComposerHandle.reference`, a ready
  `DraftAttachment` labelled "Canvas") whose reference text is appended to the
  message: `[artifact <id> v<n>, element <selector>: "<text>"]`. **Since
  2026-11-08** the pick also carries the element's stamped `data-neko-id` and
  opening tag, the message ends with a Hard scope line, the bridge reports the
  page's errors and sends `data-neko-send` clicks: `everything-renders.md`, Phase B.
- **Web.** `web/src/canvas/`: `artifacts.ts` (types, URLs, `useArtifacts`,
  `useArtifact`, the canvas target store `openCanvas`/`closeCanvas`/
  `useCanvas`, `referenceText`), `ArtifactPanel.tsx` (toolbar: version
  switcher, Comment, Download, Open in browser; `ArtifactFrame` draws html/svg
  live, png as an image, the rest as a download), `ArtifactCard.tsx` (the card
  on a reply). The artifact part sits among the reply's work (it is saved by a
  tool call), so `NekoTurn` collects cards from all parts and draws them after
  the reply. Home opens the canvas in the shell's inspector (new `fill` option:
  the body does not scroll, the slot is full height) when an
  `ArtifactsChanged` carries a `message_id`, and keeps it until closed.
  An artifact also opens in a tab of its own (`#/artifact?id=<id>&v=<n>`,
  `canvas/ArtifactTab.tsx`, keyed `artifact:<id>` by `shell/tabs.ts`) from the
  card's "Open in a tab", a link in a reply, or the right-click menu (shared by
  the card and the panel: open in a tab, beside the chat, download, open in
  browser, copy link). **No gallery page (2026-11-06, owner):** the `#/canvas`
  page that laid every artifact out as frames was removed with its
  `Artifacts` list hook; the daemon's `Artifacts` request stays for agents.
- **Instruction.** `super_instruction` says anything visual is an artifact
  (one self-contained file, `SaveArtifact`, mention id and version in a line,
  save with `artifact_id` to add a version, how to read a reference) and the
  design rule: the workspace's `DESIGN.md` first, otherwise a design-systems
  skill if one is installed, and look at the result before saving. The
  read-only chat instruction does not mention artifacts (it cannot write files).
- **Not built (v1 non-goals):** PNG/PDF export, real-time collaboration, a
  vector editor, a free-form board, drag editing. A `design-systems` skill is
  not shipped with Neko; the instruction uses one only when installed.

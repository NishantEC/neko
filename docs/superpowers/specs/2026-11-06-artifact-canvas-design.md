# The canvas beside the chat

Date: 2026-11-06. Owner, same day: a canvas "like Figma, how Open Design does
it", integrated into Neko itself, not a plugin page; Neko should feel like an
agent harness (DeepSeek Harness, Open Design) where the chat is the centre and
what it makes appears beside it. Tickets stay: Neko still creates and manages
them, as one capability among others.

## What the owner gets

1. Asks in Home chat: "design the onboarding screen for support-fe".
2. The full-power turn writes the design (HTML, using the scope's
   `DESIGN.md` or a design-system skill), and it opens on a **canvas panel
   beside the conversation**, live, while the reply says what it made.
3. They click **Comment**, click an element in the design, type "bigger,
   and use the brand blue", send. The chat gets the message with the
   artifact, version and the element they pointed at; the agent edits the
   design; a new version appears on the canvas.
4. Versions are kept (v1, v2…), switchable; Download HTML; Open in browser.
5. A **Canvas** page shows every artifact as a frame on a board (like
   Figma's pages of frames), filterable by workspace; opening one returns to
   its conversation.

## Decided

- **Artifacts are files Neko owns.** `<data dir>/artifacts/<id>/` holds
  `meta.json` (`id`, `title`, `kind`, `workspace_id`, `message_id` of the reply
  that made it, `versions: [{n, file, at_ms, note}]`) and `v<n>.<ext>`. Kinds in
  v1: `html` (designs, documents, dashboards) and `svg`; the store keeps any
  file, and the canvas shows other kinds as a download.
- **How an agent makes one:** it writes the file anywhere, then
  `neko-daemon --request '{"SaveArtifact":{"title":…,"file":"/abs/path","workspace_id":…|null,"artifact_id":null|"<id to add a version to>","note":"…"}}'`.
  The daemon copies the file into the store (size cap 5 MB, html/svg/png/pdf
  /json/md), records the version, links it to the chat turn running at that
  moment (the pending Home reply), broadcasts an event, and replies with the
  id and version. `Artifacts { workspace_id: Option }` lists them;
  `Artifact { id }` returns meta. The full-power chat instruction and the
  plugin guide's neighbour docs say when to use it ("anything visual: make it
  an artifact").
- **Rendering is isolated.** The daemon serves an artifact version at
  `/artifact/<id>/<n>` with its own CSP (`default-src 'self' data: blob:;
  script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self'
  'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:;
  connect-src 'none'; frame-ancestors 'self'`) and the web app shows it in
  `<iframe sandbox="allow-scripts">` (no `allow-same-origin`): the design's
  own scripts run, but it has an opaque origin, so it cannot read Neko's token,
  storage or API. The route needs the same auth gate as other web routes.
- **Comment mode.** For html artifacts the served page gets a small injected
  bridge script (appended before `</body>`): when the parent posts
  `{type:"neko-comment-mode", on}`, it highlights the hovered element and, on
  click, posts back `{type:"neko-pick", selector, text (first 120 chars),
  rect}`. The parent validates `event.source` is that iframe. The composer is
  prefilled with a reference chip; the sent message text carries
  `[artifact <id> v<n>, element <selector>: "<text>"]` so the agent knows
  exactly what to change.
- **Where it shows.** Home chat gets a right-hand canvas panel (reuse the
  existing resizable inspector panel if it fits) that opens when a reply
  produces an artifact and stays until closed; a chat reply shows an artifact
  card (title, version, Open) inline. A new built-in **Canvas** page
  (`#/canvas`, main group) lays artifacts out as frames.
- **Design quality.** The full-power instruction tells the agent: for a
  visual artifact, read the workspace's `DESIGN.md` if any, otherwise use the
  `design-systems` skill (Open Design's systems and craft rules, imported into
  Neko's skills folder), and check the result by viewing it before saving.

## Non-goals (v1)

PNG/PDF export (later, through a headless browser), real-time collaboration,
a vector editor (Penpot/Graphite territory), an infinite free-form board
(the Canvas page is a grid of frames; Excalidraw sketches come in as their
own artifact kind later), editing designs by dragging.

## Anti-goals

An artifact able to reach Neko's API or token. A canvas that only works for
this owner (everything here is generic). Breaking the existing chat parts
(AI SDK message parts, `docs/knowledge/chat-is-the-ai-sdk-s.md`).

## Rune P3b: the web panel, layout pressure, durable artifact links (2026-11-16)

Code: `apps/neko-rune/` (`src/web_panel.rs`, `src/viewers/web.rs`), `web/` (embed mode), `crates/neko-daemon` (four reads, one
write-side field). Decision and phases: `docs/decisions/0002-rune-for-everything.md`. Builds on
[P3a](rune-p3a-the-context-panel.md). The web UI stays the default; the web change is additive behind `?embed=1`.

### 1. Layout pressure (`layout.rs`, `view/context.rs`)

- **The rule.** window − sessions − panel − Neko under **480** (`SESSION_COMFORT`) collapses Neko to its 44 point strip; it comes
  back when the pressure ends (the panel closes, the window widens). The state is untouched: it is a decision of `Layout::fit`.
  A person's own expand or collapse (`set_neko_open`, a drag of Neko's edge) is `Layout::neko_choice` and wins until the next launch
  (`restored` starts at `None`, nothing is saved). The old hard minimum (320 for the session) still applies after it: the panel
  narrows, then Neko collapses even if the person opened it, then the panel hides. The old "under 1100 starts as the icon" rule is
  inside the new one (1141 with the defaults).
- **Icon tabs.** The panel under **440** wide draws its tabs as icons (Canvas ◇, Diff ±, Files ▤, Terminal ›_) with tooltips; the
  segments keep their keys, roles and names, so "Termi…" cannot appear. `ICONS_BELOW` in `view/context.rs`. An expanded panel passes
  the window's width, so it is always labelled.
- **Snaps (1360x840, panel open, dark).** `docs/design/rune/p3b-pressure-before-dark.png` (session 279 points, tabs "Termi…"),
  `p3b-pressure-after-dark.png` (session 575, Neko its icon, icon tabs), `p3b-icon-tabs-dark.png`. Tests: `layout.rs` (the edge is
  exactly 1561 with the defaults, restore, manual choice), `tests/panel.rs::layout_pressure_*`, `a_wide_panel_shows_the_words`.

### 2. Durable artifact → session link (daemon)

- `Artifact::task_id: Option<String>` (serde default, skipped when none). A ticket run's `save_artifact` bridge call records it
  from the **lease's crew role** (`CrewRole::Task { ticket_id }`, passed by `Controller::crew_call` to the server's closure) and is
  **no longer linked to a Home reply that happens to be running**; a chat's (`Full`) or a hand's (`--request`) save records none.
  The next version of an artifact keeps the first's ticket.
- **Finding, and the one deviation: a ticket run could not call `save_artifact` at all.** `CrewRole::Task` allowed list, status,
  wait, create and checkpoint_diff only (`the-chat-directs-the-crew.md`, and a test asserted it). The P3a note said the lease token
  "already names its task", true but unreachable. To make the spec's mechanism real, `Task` now also allows **`save_artifact`**
  (`neko-protocol/src/crew.rs`; `show_artifact` stays the chat's). One line, easy to revert; the owner should confirm it.
- `ArtifactsForTask { task_id }` answers `Response::Artifacts` (the existing record type; there is no separate summary type),
  newest first. The Canvas asks when a session is chosen and again when its task row's `updated_at_ms` moves, and merges the answer
  with what was shown in this window by artifact id (`App::task_artifacts`), so a session's artifacts survive a restart.
- Daemon tests: `a_ticket_runs_artifact_records_its_ticket_and_a_chat_or_manual_save_records_none` (ticket, chat, hand, read-only
  refused, an older record without the field still reads), `web_embed` tests. Rune: `what_a_sessions_own_runs_saved_joins_the_canvas_list_once_…`.

### 3. Viewer memory and threading (`viewers/mod.rs`)

- Bytes are one `Arc<[u8]>`: `Loaded` holds it, `ViewerInput::bytes` and `shared()` clone the `Arc`, the CSV index is built over the
  same allocation (`CsvData::bytes`), hayro reads it through a `Shared` wrapper. Test: `viewers_share_the_panels_bytes_and_never_copy_them`.
- `ViewerState::load(tag, job, make)` runs `job` off the UI thread (`Runner`: a thread per job, `ViewerStates::use_threads(waker)`;
  inline in headless runs; `hold()`/`run_held()` for tests) and builds the viewer's content from its answer on the UI thread. Image
  decode (`image_view::decode`), the PDF parse **and each page render** (`pdf::parse`, `render_page`, pages asked by `PdfDoc::want`)
  and the CSV row index run there. Until then the viewer draws `image_view::skeleton` (the picture's shape from its header when known).
- Results live in the state of `(artifact id, version)`; `release` (the viewer left the screen) drops the state's jobs, so a late
  answer is kept by nobody (test `a_result_for_a_version_that_left_the_screen_is_dropped`); a PDF page drawn for a zoom the reader
  left is dropped by its zoom key.

### 4. The web panel

- **Embed mode (`web/src/shell/embed.ts`, `EmbedShell.tsx`, `canvas/ArtifactEmbed.tsx`).** `?embed=1` is read once, from the hash
  query (the daemon's sign-in redirect keeps the fragment and drops the real query) or `?embed=1`. Routes: `#/artifact/<id>/<n>?embed=1`
  (also `artifact?id=&v=`) and any plugin page `#/<plugin>:<page>?embed=1`. Only the routed content renders: no tab strip, rail,
  sidebar, status bar, ⌘K, onboarding; it neither follows an agent's `ShowArtifact`/`OpenTab` nor reports itself to `InspectApp`.
  The page is transparent (`html[data-embed]`); `theme=dark|light` in the hash query paints that appearance and a `hashchange`
  updates it live (`theme.ts`, `theme-boot.js`). Comment mode and `data-neko-send` dock the app's own `NekoComposer` under the
  artifact the first time one has something to say (nothing is sent until the person sends it; an offer no longer navigates the
  embedded page to Home). "Open in browser" is hidden in embed (it would open without the sign-in). Check: `scripts/verify/embed.mjs`
  (artifact html, sketch, plugin page, no chrome, transparent, theme change, comment + composer, normal route unchanged).
- **Daemon (`web_embed.rs`).** `SignedWebUrl { route }` is the address `--open-page` builds (`http://127.0.0.1:<port>/?token=…#/<route>`),
  route checked by `viewers::tab_route`; **the web bridge refuses it** (`web::api_request`), it is for the Unix socket.
  `PluginViewers` lists each enabled manifest's `viewers` (`{plugin, viewer_id, kinds, title}`). **Plugin pages are not in manifests**
  (a plugin calls `host.registerPage` in JS), so the web page reports them: `ReportPluginPages` (sent by any web window, including the
  embedded one, once plugins are listed, debounced) is kept in `<data>/plugin-pages.json` and answered by `PluginPages`, minus plugins
  that are gone or switched off. Until some web window has loaded the plugins once, the list is empty.
- **`WebPanel` (`web_panel.rs`).** `wry` 0.57 builds a `WKWebView` as a child of the Rune window on its raw handle (`App::on_window`
  keeps the `WindowHandle`; `with_raw_handle` + `build_as_child`), transparent, no devtools, **incognito** (the sign-in cookie is not left
  on disk). The decisions are pure and tested without a window; the native part sits behind `Factory`/`Native`.
  - *Position:* the slot's `on_layout_tx` rect, cut to the region (`fit`, `viewers::web::region`: the panel body, or a plugin page's body)
    every layout pass. Frame, not clip: wry has no clip container, so a partly scrolled view is resized to the visible part.
  - *Hide-on-overlay:* `Overlays` counts the elements of the window's overlay layer each build (every open popover, menu, select, dialog,
    the ⌘K palette and toast is one), and `sync` runs from the layout pass: while it is above 0 the view is `set_visible(false)` and the
    slot shows its own surface in the theme's colour. **A deliberate approximation:** any open overlay counts, not only those whose rect
    meets the panel (a hidden-too-often view is better than one drawn over a menu). wry cannot capture a frame, so there is no snapshot
    image; the blank themed surface is the specified fallback.
  - *Theme:* `?theme` is baked into the signed address; later changes call `evaluate_script` to set `theme` in the hash (no reload).
  - *Navigation lock:* `decide(url, port)`: only `http://127.0.0.1:<the signed port>` (no user name, parsed with the `url` crate, not
    matched as a string) and the inert `about:blank`/`about:srcdoc` load; another http(s) address opens in the default browser and is
    cancelled; everything else (file, javascript, data, blob, …) is refused. `window.open`/`target=_blank` never open a second view. The
    daemon's address is only taken if it is itself `http://127.0.0.1:<port>` (`signed_port`).
  - *IPC:* none. The web UI talks to the daemon itself.
  - *Lifetime:* created lazily when the panel first wants a route and its signed address is in; reused for another artifact; destroyed
    when the context panel closes (and a plugin page is not open), or **after 5 minutes off screen** (`OFF_SCREEN_LIFETIME_S`, a 30 s
    wake while hidden). A creation failure (wry panics on an invalid handle) is caught and shown as a line in the slot.
- **Registry (`viewers/web.rs`).** `web-artifact` (html htm) and `web-excalidraw` at priority 5 replace `web-pending`;
  `web-plugin:<plugin>:<viewer>` at 10 for every kind a manifest declares (`replace_group`, so a removed plugin stops claiming kinds).
  All of them are the slot; the web page picks its own viewer. A file of the working folder that is html is shown as code, not as a page.
- **Plugin pages (slot b).** "More" has a **Plugins** heading with one entry per reported page (`App::plugin_pages`); choosing one
  opens `App::plugin_view`: the middle region is the page's whole content, `<page-route>?embed=1`. Choosing a session or a full view leaves it.

### Review (one fresh reviewer) and what is left

Fixed: a ticket run may add a version only to an artifact its own ticket made, and cannot name a workspace
(`artifacts::keep_locked`, tested in the daemon); the context panel's web page waits while a plugin page owns the one view.
**Open, not fixed (the feature freeze):** the navigation handler runs for every frame, so a page that sets `location` to another site
opens the browser without a click (open it only from the new-window handler, debounced); the view follows the layout rect, not the
drawn rect, during collapse animations (`on_drawn_layout_tx`); a view shown again can sit at its old rect for one frame; Download
(`<a download>`) does nothing in the view; `ArtifactsForTask` rescans every `meta.json` per task update; one thread per viewer job
(a pool would bound it); `Reply::SignedUrl` derives `Debug` with the token; `WebHost::close` is never called, so a rotated token
needs a restart. Pre-existing and not from this work: `scripts/verify/tabs.mjs` "selected text offers Ask Neko about this" fails in
the full `web-verify` run with or without `embed.mjs`.

### Licences

New crates are MIT or Apache-2.0 (`wry` 0.57, `url`, `objc2-web-kit` also Zlib) with one exception: **`option-ext` 0.2.0 is MPL-2.0**,
pulled in by `wry` → `dirs` → `dirs-sys` and not avoidable by a feature. It is an unmodified dependency, as `resvg`/`usvg` already
are (P3a); no GPL, LGPL or AGPL came in. Say so if the "MIT/Apache only" rule is meant to cover transitive MPL.

### Not verified, on purpose

The rules say a real window may only open when the keyboard and mouse have been idle for 180 s; the Mac was in use for the whole
session (HIDIdleTime 0 each time it was read). So **nothing here has run in a real window**: a `WKWebView` created by wry on Rune's
handle, hide/show against a real menu, IME/focus in the view, and the two memory measurements (the panel with a web view open, and
five minutes after closing it) are **not measured**. What was verified instead: the whole decision layer by unit tests and by
Rune's headless runtime with a recording fake view (made once, bounds, hide under each overlay kind, destroy on close, no view for an
unsigned or foreign address), the web side in the shared Chrome by `embed.mjs`, and the snaps below. One thing read in wry's source for
the memory question: its macOS `Drop` removes the view from the window but deliberately `retain`s the `WKWebView` and its
controller (`wkwebview/mod.rs`, "prevents a segfault"), so destroying a view may not return all of its memory; measure it once by hand.
Also: a click inside the web view puts keyboard focus there, so ⌘K and Esc reach Rune only after the person clicks back into Rune.

### Snaps (`docs/design/rune/p3b-*`, 1360x840, dark)

`pressure-before`, `pressure-after`, `icon-tabs`; `web-menu-hidden` (More open over the panel: the view is hidden, the slot is its
surface); `plugin-more` (the Plugins heading), `plugin-page` (a plugin page's slot); and the pages the web view loads, rendered in the
shared Chrome by `embed.mjs`: `web-html-artifact`, `web-excalidraw`, `web-plugin-page-content`, `web-comment-composer`.
Headless Rune snaps have no native view, so they show the slot where it would draw.

### Numbers (debug build, headless, 1360x840 at 2x; peak RSS of `--snap`)

Baseline Diff tab 131.5 MB; PNG +3.1 MB; 100,000-row CSV +22.9 MB; 16-page PDF +40.7 MB (the P3a perf-build deltas were +3, +21.5,
+37.8: the debug build and the run-to-run noise are larger than what sharing the bytes saves, which the test above proves
structurally). Frame work for these viewers now happens on a thread: the decode, the parse, the page renders and the index no longer
run inside a frame.

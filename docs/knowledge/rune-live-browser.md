## Rune live Browser tab: watch a page of a Chrome on this Mac (2026-11-18)

Code: `apps/neko-rune/src/browser/` (`discover.rs`, `cdp.rs`, `mod.rs`, `tab.rs`), `src/view/context_browser.rs`, `Tab::Browser` in
`src/panel.rs`. Decision: `docs/decisions/0002-rune-for-everything.md` (P3, "Browser (live)"). Builds on
[P3a](rune-p3a-the-context-panel.md) (tabs, auto-open) and [P3b](rune-p3b-the-web-panel.md) (decodes off the UI thread, layout pressure).

Agents drive Chrome over CDP. This tab lets the person **watch** a page while they do. It never controls, creates, closes or reads
anything else: there is no key, click or navigation to the page, and no other tab's content is read.

### What it connects to (`discover.rs`)

- Order: `NEKO_BROWSER_CDP` when set (then only it), else `http://127.0.0.1:9333` (the shared Chrome of `~/.agents/BROWSER.md`) and
  `http://127.0.0.1:9222`. The first whose `GET /json/version` answers wins; its `webSocketDebuggerUrl` is the one browser-level
  websocket.
- **Loopback only.** Addresses are parsed with the `url` crate; a host other than `127.0.0.1`, `::1` or `localhost` is refused, in
  the address given and in the websocket address Chrome answers with; no user name, no `https`/`wss` (no TLS is built). The HTTP
  GET is hand-written over `std::net::TcpStream` (300 ms to connect and to read, one write, a 1 MB cap, `Content-Length` and
  `chunked` understood). `tungstenite` 0.30 (`default-features = false, features = ["handshake"]`) is the only websocket code; no
  async runtime. `base64` 0.22 (already in the tree) decodes frames; the existing `image` crate decodes the JPEG.
- It talks to Chrome, **not to the daemon**. Rune's request allow-list for the daemon (`link/`, `WEB_SENDS`) is untouched; no `Call` is
  added. `tests/allow_list.rs::the_browser_module_talks_to_chrome_on_the_loopback_and_never_to_the_daemon` keeps it so: `src/browser/`
  is the only code besides `link/http.rs` that may name a `TcpStream`, it must parse a loopback address before it connects, and it
  may not mention `/api/`, `neko_protocol`, `crate::link`, a `Call`, `std::fs`, or `https://`.

### The allow-list (`cdp.rs`)

`pub const CDP_METHODS: [&str; 7]`: `Target.setDiscoverTargets`, `Target.getTargets`, `Target.attachToTarget`,
`Target.detachFromTarget`, `Page.startScreencast`, `Page.stopScreencast`, `Page.screencastFrameAck`. `Engine::send` is the one
function a command leaves through; it refuses (and logs the method's name, nothing else) any other. Never `Browser.close`,
`Target.createTarget`/`closeTarget`, `Input.*`, `Runtime.*`, `Network.*`, `Storage.*`, `DOM.*`. Tests: the unit test over a list of
refused names, the allow-list test above (every literal at a `self.send(` is on the list), and the integration test
`tests/browser_cdp.rs` where a fake Chrome records every method it receives.

### How a frame gets to the screen

One thread (`neko-browser`) holds ONE websocket in flatten mode. `Target.setDiscoverTargets {discover: true}` keeps the page list
(`targetCreated`, `targetInfoChanged`, `targetDestroyed`); pages are targets of type "page" except `devtools://`, `chrome://`,
`chrome-extension://` and `about:blank`. "Newest" is the page whose address or title changed last (`BrowserView::upsert` stamps
it); a person's choice (`Follow::Page`) wins while that page exists. The thread attaches (`flatten: true`), then
`Page.startScreencast {format: "jpeg", quality: 70, everyNthFrame: 1, maxWidth, maxHeight}` with the image area's size at the
window's scale rounded up to a multiple of 64 (`screencast_side`; asked again only when the area grew). On each
`Page.screencastFrame` the JPEG is decoded **on that thread**, stored as the latest frame (a frame never shown is replaced, never
queued), the window is woken (one build asked for at a time, coalesced by an atomic flag in `lib.rs`), and the frame is
acknowledged. The window moves the frame into `Pixels` (`BrowserTab::pull`). Switching pages detaches the old session first
(`Page.stopScreencast`, `Target.detachFromTarget`), then attaches the new one.

### Lifecycle

- Connected only while the Browser tab is the visible panel tab **and** the window is active (`App::browser_on`, told to the thread
  every frame by `browser_tick`). A tab never shown opens no socket. The tab never opens the panel and is never the default tab
  (`first_tab` skips it unless it is all there is).
- Hidden or window inactive: `Page.stopScreencast` and `Target.detachFromTarget` at once, the picture is dropped; the socket is
  closed after 60 s hidden.
- Chrome gone: `Status::Lost`, retry after 1 s, doubling to 30 s. No browser found: `Status::None` and the "Look again" button
  (also looked for again when the tab is next shown).
- The shown page closes: "That page closed." for 3 s, a chosen page is followed no more (back to the newest).

### Privacy

Frames live in memory only: never written to disk, never sent to the daemon, an agent or a log. Page titles and addresses are never
logged (the one log line names a refused method). "Open in your browser" hands only an `http` or `https` address to the app's
`opener`. The scenes (`panel-browser`, `panel-browser-none`) use a generated gradient and two made-up pages ("Acme docs",
"Asha's board" at `*.acme.test`); no capture of a real page is committed.

### The tab

`Tab::Browser` (offered, like Details, for a session whose details are loaded), fifth in the order Canvas, Diff, Files, Terminal, Browser, Plan, Details; "Browser"; `on_request()`; the existing
globe icon (`icons::GLOBE`, 16 px, stroke 1.5) when the panel is under 440 wide. Top bar 32 high (globe, title 13.5, host 12.5
muted, a picker, "Open in your browser" as 32 point squares), "Following the newest page" in 12 faint, the frame fitted to the
panel's width with its aspect, radius 8, centred. "No browser to watch" (320 wide, "Look again"), "Lost the browser. Trying
again…", "Waiting for the page to draw…". Tests: `tests/browser_tab.rs`, the structure crawl (both scenes are in `SCENES`).

### Verified live

Once, on 2026-11-18, against the shared Chrome (`shared-chrome acquire`, port 9333): a page of our own in our own browser context
(`data:text/html`, placeholder words, a counter and a colour changing every 400 ms; Playwright `connectOverCDP`, context closed and
the lease released after). The Rune app ran headless in real time (`--scene panel-browser-none --realtime --snap`, which runs
`on_ready` and the poll hook, so the real thread connected by itself; the scene starts with "No browser to watch" and nothing
synthetic is on screen). Results: the tab went from the staged "none" state to Connected, the page's title and "data:" showed in the
bar, "Following the newest page" appeared, and the live frame was drawn: counter 32 in one snapshot and 59 in a later one, the box
colour different in each. The page was 1000 x 700; the screencast was asked for at the panel's width at scale 2, and the frame was drawn 396 x 278 points,
the panel's width, with its aspect. The page only changes about 2.5 times a second, and Chrome sends a frame per change, so this
measured the path rather than the ceiling (not measured: sustained frame rate on a page that animates at 60 Hz). Not checked in a
real window with a person's hand on the picker (the picker and "Open in your browser" are covered by `tests/browser_tab.rs`).
Screenshots stayed in `/tmp`; none is committed.

# neko architecture

The map, not the territory. Every section points at the code that owns the
detail. `AGENTS.md` has the full decision record for anything below that looks
arbitrary; several of these decisions cost days to learn.

## Five crates, and the boundary they exist to enforce

```
crates/
  neko-protocol/  wire types + framing. Depends on serde and nothing else.
  neko-core/      daemon-owned logic: SQLite, indexes, ranking, launching, providers
  neko-daemon/    a thin binary hosting neko-core behind a Unix socket
  neko-client/    SDK: a persistent, auto-reconnecting connection to the daemon
  neko/           the GPUI app: window, panel, hotkey capture, onboarding, themes
```

The rule the split exists to enforce: **`neko` depends on `neko-client` and
`neko-protocol` only, never on `neko-core`.** The app physically cannot reach
past the daemon boundary to open SQLite or read the app index itself. Check it
with `grep neko-core crates/neko/Cargo.toml` — no hit.

## Two resident processes

```
   ⌥Space
     │
     ▼
┌─────────────────────┐   length-prefixed JSON   ┌──────────────────────────┐
│ neko (client)       │  ◄────────────────────►  │ neko-daemon              │
│  GPUI window        │   ~/Library/Application   │  SQLite (single writer)  │
│  panel + text field │   Support/neko/neko.sock  │  app index (Spotlight)   │
│  live OS hotkey     │                           │  clipboard capture loop  │
│  themes, onboarding │                           │  icon extraction thread  │
└─────────────────────┘                           │  6 Providers             │
                                                  └──────────────────────────┘
```

Both stay resident. The window is opened once at startup and only ever hidden
and re-shown, never closed — that is what makes a warm summon a few
milliseconds rather than a window creation. The daemon survives the client
being killed; a fresh client opens a new socket connection to the same live
daemon with no handshake.

`crates/neko/src/daemon_launcher.rs` spawns `neko-daemon` unconditionally at
client startup. A redundant spawn is a no-op: the daemon's `bind_singleton`
(`crates/neko-daemon/src/server.rs`) pings the existing socket and exits
quietly, or takes over a stale socket file if nothing answers.

`socket_path()` and `database_path()` (`crates/neko-protocol/src/lib.rs`) are
fixed per-user paths, not per-checkout. Two builds of neko on one machine talk
to whichever daemon started first.

### Why the daemon owns SQLite

One writer, no locking protocol, no migration races. The database holds the
clipboard history, settings (hotkey combo, onboarding flags, theme id) and app
launch counts. Multiple clients can connect at once — the daemon serializes
every write behind its own `Mutex<Db>`. `crates/neko-core/src/db.rs`.

### Why the hotkey is registered in the client

The daemon owns and persists the *setting*
(`crates/neko-core/src/hotkey.rs`). The live OS registration happens in the
client (`crates/neko/src/hotkey_client.rs`), for two reasons. `global-hotkey`
needs a live run loop, and the client already has GPUI's foreground executor
polling for events — routing a keypress through IPC would add a socket
round-trip to the exact path warm summon is measured on. And only a live
registration attempt can prove a third-party app is not already holding the
combo, which only the client can make.

`HotkeyController::rebind` registers the candidate *before* unregistering the
old combo, so a failed rebind never loses the working hotkey.

## The wire protocol

`crates/neko-protocol/src/lib.rs`, ~450 lines, no logic. A frame is a u32-LE
byte length followed by a JSON payload (`write_frame`/`read_frame`). Local IPC
only, so JSON's debuggability beats the bytes a binary codec would save.

```rust
enum Frame {
    Request  { id: u64, request:  Request },
    Response { id: u64, response: Response },
    Event(Event),          // server-initiated, not correlated to any request
}
```

Two properties worth knowing before reading either side:

**Responses can arrive out of order.** The daemon spawns a thread per
*request*, not per connection, so a 1.5s file search cannot queue every
following keystroke behind it. `neko-client` correlates by request id in a
`pending` map, never by arrival order.

**One request can be answered more than once.** `Response::SearchResults`
carries `complete: bool`, and `Response::ends_request()` is the single place
that rule is written down. See "Two-phase search" below.

Only three request variants matter for extending the app, and all three are
generic over provider:

| Request | Meaning |
| --- | --- |
| `Search { query, limit, provider }` | `provider: None` is the merged root list; `Some(id)` scopes to one provider — this is the mode seam |
| `Activate { kind, id, action }` | `kind` routes to the provider; `action: None` is its primary action, `Some(id)` a secondary one |
| `GetTheme` | There is deliberately no `SetTheme` — committing a theme is `Activate { kind: "theme", .. }` |

## Providers

`crates/neko-core/src/provider.rs`. Object-safe by construction, so the daemon
holds a plain `Vec<Box<dyn Provider>>`.

```rust
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;             // SearchItem::kind, and the Activate route
    fn section_label(&self) -> &'static str;  // "Applications", "Clipboard", …
    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate>;
    fn activate(&self, id: &str) -> Result<(), ProviderError>;

    fn defers_for(&self, query: &str) -> bool { false }        // slow for this query?
    fn search_cancellable(&self, …, cancel: &Cancel) -> …      // defaults to search()
    fn perform_action(&self, id, action_id) -> …               // defaults to an error
}
```

Six are registered, in `AppState::with_test_providers`
(`crates/neko-daemon/src/server.rs`):

| id | Source of truth | File |
| --- | --- | --- |
| `app` | `mdfind`, plus a plain scan of the sealed system volume | `crates/neko-core/src/apps.rs` |
| `file` | `mdfind`, prefix match only, bounded and cancellable | `crates/neko-core/src/files.rs` |
| `clipboard` | SQLite, filled by a polling capture loop | `crates/neko-core/src/clipboard.rs` |
| `settings` | `/System/Library/ExtensionKit/Extensions/*.appex`, scanned once | `crates/neko-core/src/settings.rs` |
| `command` | A compiled-in table | `crates/neko-core/src/commands.rs` |
| `theme` | `neko_protocol::BUILTIN_THEMES` | `crates/neko-core/src/themes.rs` |

**The panel knows nothing about any of them.** `crates/neko/src/panel.rs` has
no `match` on provider identity anywhere. Everything a row needs to render —
section header, action verb, icon, badge, subtitle, secondary actions, whether
confirming it enters a mode — is data the provider set on the `SearchItem`.
That is what makes adding a seventh cheap; see
[adding-a-provider.md](adding-a-provider.md).

Two source-of-truth notes that repeatedly surprise people:

- Spotlight does not index the sealed system volume, which is why `apps.rs`
  unions `mdfind` with a plain directory scan. Finder needs a named allowlist
  on top of that, because it lives loose in `/System/Library/CoreServices`
  beside ~112 background agents that no static plist signal separates it from.
- File search matches names that **start with** the query, not names that
  contain it. A leading wildcard forces Spotlight off its index: measured
  ~13 seconds for one character, versus well under a second as a prefix.

## Ranking: `search::allocate`

`crates/neko-core/src/search.rs`. Every provider returns
`Candidate { score, item }` and owns its own matching. The one cross-provider
contract is that a higher score is better, on roughly the scale `fuzzy_score`
produces — a small dependency-free subsequence scorer with contiguous-run and
word-boundary bonuses.

`allocate` merges them into one bounded response in three passes:

```
1. Reserve   every provider with any candidate takes one slot, before
             anything else is allocated. Including the first-registered
             one — a "primary provider wins ties" assumption is itself a
             hard-coded provider identity, and it silently produced an
             empty Applications section once.

2. Interleave  spend the rest of `limit` one slot at a time on the highest
               remaining candidate anywhere. Clipboard alone has a cap
               (`clipboard_max_slots`, half the limit) so one long paste
               cannot take nine of ten rows.

3. Order     stable-sort the sections by their own top candidate's score.
             Registration order is only the tie-break. Clipboard's recency
             boost is discounted for this comparison: freshness ranks rows
             within Clipboard, it should not decide which section leads.
```

Two per-provider bonuses exist and are deliberately gated:
`app_category_score` and `settings_category_score` both require a real prefix
match before applying. An ungated version promoted coincidental matches
("Xcode" for "code") over relevant files. Constants in that file were tuned
against a real corpus, not a fixture.

The client does the same reservation again in **pixels**:
`panel::fit_within_budget` splits results into contiguous same-`kind` runs,
reserves a header-plus-one-row for every section after the first, and rolls
unused budget forward. A response with room for a section is necessary but not
sufficient for a screen with room to draw it.

## Two-phase search

`files::FileProvider` shells out to `mdfind`, measured 55–990ms and bounded at
1.5s. The other five answer in microseconds. One combined response meant
nothing rendered until the slowest finished.

The daemon now partitions providers by `defers_for(query)` and answers twice:
the fast group immediately with `complete: false`, then the whole set with
`complete: true`. A query nobody defers for is still exactly one frame.
Keystroke to first render went from a median of 108.50ms to 0.36ms
(`docs/evidence/instant-search-report.md`).

Two consequences in the client:

- **Superseded searches are abandoned, not ignored.** The next `Search` on a
  connection cancels the previous one (`server::supersede_previous_search`),
  and `files.rs` kills the `mdfind` child rather than letting it finish into a
  discarded result. An abandoned query that keeps running competes with the one
  you actually want.
- **Late results append.** `panel::merge_late_results` keeps what is already on
  screen in its exact order and appends only what the deferred provider added.
  If making room would cost the selected row, the late section is not shown at
  all until the next keystroke — someone who has arrowed to row seven is about
  to press Enter.

## Commands and modes

A **command** is a provider row with `SearchItem::enters_mode: Some(id)`.
`panel::Root::confirm` checks that field first, and if it is set the row never
reaches `Request::Activate` at all — the client transitions into a mode
instead.

A **mode** is client-side UI state. `crates/neko/src/modes.rs` holds the pure
part (`ModeChrome`: id, scoped provider id, footer title, placeholder, whether
it has a detail pane); `panel.rs` drives the I/O. While a mode is active, every
keystroke searches with `Request::Search { provider: Some(id) }`. Escape or the
back arrow exits and restores the query you had typed before entering, verbatim.

Two exist: Clipboard History (a detail pane, list column 264pt) and Themes
(no detail pane — the preview *is* the panel, so a second column would take
496pt away from the thing being previewed).

The `⌘K` actions menu is populated from the selected row's
`SearchItem::actions`, which already arrived with the last search response — no
round-trip to open it. A destructive action needs a second Enter to run;
moving the selection disarms it.

## Themes

`crates/neko/src/theme.rs`. Colour is a swappable table; geometry is not.

- Geometry, spacing and type stay `pub const`. A theme cannot move a row or
  resize the panel.
- Colour lives on `Palette`, 20 `Rgba` fields. Every paint site reads
  `theme::active()`, which is one relaxed atomic load and a slice index into
  `&'static [Theme]`. No lock, no allocation, no `Arc` on the render path.
- A theme supplies *independent* colours as a `Spec`; one `const fn build()`
  derives every dependent token. A theme cannot redefine what a token means.

Seventeen ship. Light ones also move the real `NSWindow`'s `NSAppearance`,
because the native material behind the panel renders in it — a cream panel over
a `darkAqua` blur reads as a dark halo. See
[adding-a-theme.md](adding-a-theme.md).

## The window, and why it looks the way it does

`crates/neko/src/main.rs` opens one `WindowKind::PopUp` window and configures
it in a fixed order. Each of these lines is load-bearing.

**Fixed 760pt wide, never resized at runtime.** Entering a mode does not resize
the `NSWindow`. gpui's own private `viewport_size` — the size it lays out and
paints the root element against — stops resyncing after a window has been shown
and hidden a few times, and no public API forces it. Four workarounds were
tried and all failed. So the window is created at
`theme::PANEL_WIDTH_WITH_DETAIL_PX` for the process's whole lifetime and the
panel fills it. `docs/evidence/mode-resize-seam-fix-report.md`.

**No shadow at all is drawn by neko, and AppKit's is disabled.**
`material::disable_native_shadow` turns off the automatic window shadow, and
the panel `div` carries no `.shadow_lg()`. Both were invisible for most of the
project's life because the window was exactly the panel's width; once it was
not, both spilled into the margin as a black halo. Two overlapping sources, two
separate fixes, one symptom reported three times.

**`NSTitledWindowMask` is cleared, and the rendering view is re-made first
responder in the same call.** gpui creates this window titled no matter what
`WindowOptions.titlebar` says, and AppKit draws its own ~1pt highlight on the
top edge of a titled window. Clearing bit 0 removes it —
`material::clear_titled_style_mask`. But `setStyleMask:` makes AppKit rebuild
the frame view **and resets the first responder with it**, and gpui only calls
`makeFirstResponder:` once at window creation. Miss the re-assert and keyboard
input dies silently while every property still reads back correct. Anything
that mutates this window's style mask must do the same, and must be verified by
actually typing — `Window::dispatch_keystroke` enters below AppKit's responder
chain and reports success on a build a real keypress cannot reach.
`docs/evidence/titled-window-first-responder-fix-report.md`.

**Native material, not gpui's `Blurred`.** `crates/neko/src/material.rs`
installs `NSGlassEffectView` where the OS has it, falling back to
`NSVisualEffectView(.popover)`, then to plain opaque. The invariant that makes
this work: background views go into `window.contentView()`, the real
`NSWindow`'s root view — **never** as a subview of gpui's rendering view, which
would silently eat every pixel gpui draws. Every install is read back and
verified rather than trusted (`material::verify_installed`).

**Multi-display.** The window opens once, so `upper_third()` can only place the
first summon. `display_placement::reposition_to_cursor_display` moves it to the
`NSScreen` under the cursor before each later summon, reading
`NSEvent.mouseLocation()` — permission-free, unlike asking which window is
active, which needs the same Accessibility grant as the hotkey.

**Spaces.** gpui sets `CanJoinAllSpaces | FullScreenAuxiliary` for any
`WindowKind::PopUp`. `crates/neko/src/spaces.rs` reads the bits back at every
launch rather than assuming.

## The gpui dependency

`crates/neko` depends on the `wingleeio/zed` fork of gpui, pinned by rev, with
one local patch applied on top by `scripts/setup-gpui-patch.sh`.

- **Why the fork.** It has `paint_backdrop_blur`, `EdgeFade`, a real mac
  implementation of `start_window_move()`, and a fix for a
  `windowDidBecomeKey:` self-deadlock that reliably hung the published crate
  after one or two real activations.
- **Why the patch.** The fork throttles any non-key window to ~30fps with no
  exemption for a pending `on_next_frame` callback — something a caller is
  explicitly waiting on. Warm summon regressed to ~26–40ms. No public API opts
  out, so the fix is a dependency patch, not a neko-side change. After it: mean
  8.56ms. `docs/evidence/gpui-inactive-window-throttle-fix-report.md`.
- **What it costs.** The fork pulls GPL-3.0-or-later code in unconditionally.
  See the README's licence section.

## Where to look next

| Question | File |
| --- | --- |
| How a row is rendered | `crates/neko/src/panel.rs` |
| How the text field edits, selects, and pastes | `crates/neko/src/text_field.rs` |
| How the daemon dispatches a request | `crates/neko-daemon/src/server.rs`, `handle_request` |
| What the first-run arc does | `crates/neko/src/onboarding/` (`state.rs` is pure, `view.rs` does the I/O) |
| Why any of the above is the way it is | `AGENTS.md`, and the report it points at in `docs/evidence/` |

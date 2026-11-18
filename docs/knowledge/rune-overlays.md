## Rune overlays: the activity pill, the launcher and the stop-all key (2026-11-18)

Code: `apps/neko-rune/src/presence.rs` (the pill: logic and view), `src/launcher.rs` (the panel), `src/overlays.rs` (the gate, what is
made, the shortcuts, the wiring), the generic card in `src/view/palette.rs`, `palette::narrow`, `Do::wants_window`. Tests:
`tests/overlays.rs`, `the_overlays_keep_their_shape` in `tests/structure.rs`, the unit tests in the three files. Decision:
`docs/decisions/0002-rune-for-everything.md` (P4). Behaviour spec: the Swift app's `Presence.swift`, `Palette.swift`,
`PaletteGeneration.swift`, `StopAllWork.swift`.

### The gate, and why (until P5)

The Swift app is still the default client. It owns the summon hotkey (Carbon, id 1), ⌘⇧Esc (Carbon, id 2) and its own pill. Two clients
registering the same global keys would fight over them and two pills would sit in the same corner. So **all three overlays are OFF
unless the Rune client is started with `--overlays` or `NEKO_RUNE_OVERLAYS=1`** (`overlays::gate`). Off, `overlays::plan(false)` is empty,
`install` makes no overlay and no window, `attach` registers no shortcut, the poll's `tick_presence` returns at once, the summon request
(`GetHotkey`) is not sent and Settings does not offer the pill's switch. `tests/overlays.rs` and `overlays::tests` pin each of these.
**P5 flips the default** (the gate becomes "on unless `--no-overlays`") **and retires the Swift pill, palette hotkey and ⌘⇧Esc**.

### Rune APIs used (pinned rev `bc05f81`)

| Piece | API |
| --- | --- |
| Pill | `App::overlay(OverlaySpec, Pill)` returning an `OverlayHandle<Pill>`; `OverlayOptions { level: Status, input: Regions, keys: None, prewarm: true, display: Primary }`; `ConfiguredOverlay::release_gpu_when_hidden`; `El::hit()` on the capsule only; `OverlayHandle::{show, hide, update, place, set_align}` |
| Launcher | `App::window(WindowSpec::from_app("launcher", app).configured_options(WindowOptions::launcher_panel()))` (a hidden, retained, non-activating `NSPanel`, borderless, transparent); `AppHandle::{open, hide, place(DisplayTarget::UnderPointer, OverlayPlacement::Edge), update}`; `App::{windowless, keep_running, on_focus, init}` |
| Shortcuts | `AppHandle::{register_shortcut, unregister_shortcut}` with `Shortcut` parsed from `cmd+shift+escape` and from the daemon's combo (`overlays::accelerator`); `ShortcutError` is logged, never fatal |
| Motion | `LoopSpec::{opacity, rotation}` on the glow and the ring's arc; `Cx::system_reduced_motion()` |
| Screen | `objc2-app-kit` `NSScreen` and `NSEvent` (two feature flags and `objc2-core-foundation`, nothing new in the tree) for the menu bar, Dock and visible height |

### The activity pill (`presence.rs`)

A port of `PresenceController` and `PresenceCapsule`, in two halves so the logic is tested without a window.

- **Facts** (`Facts::of(&Summary)`): the tasks (id, status, title), `replying` (any pending conversation message, as Swift: `.contains`, not
  only the last), `tool_waiting` (a pending message with a tool call awaiting approval). `state_from(now, previous)` is Swift's
  `PresenceState.from` line for line: a tool waiting first ("Neko needs your OK for a tool"), then the first Queued/Planning/Building/
  Reviewing task ("Preparing a fix: <title, 40 characters> +N more"; Building is creating, Reviewing is checking, the rest thinking, with
  `friendlyTaskStatus`'s words), then "Neko is replying", then, for one summary only, how a task that was running just ended
  (Ready to review, Plan ready, Needs attention; they linger).
- **Tracker** (`Tracker::observe`/`tick`/`set_enabled`) is Swift's `update`/`show`/`hide`: the first summary is only remembered; facts equal
  to the last are not a new summary; an outcome stays until its 6 s timer ends and is not replaced by "nothing"; a working state replaces it
  and cancels the timer; turning the preference off hides the capsule. Time is the caller's, so the tests step it.
- **Drawing** (`presence::view`): the capsule is 36 high (a 16 point glyph, 10 around it; widened on hover to 12 / 16 with the label,
  capped at the window), dark glass (Rune's dark theme always, as Swift forces a dark scheme) with a 1 point border, a glow in the
  activity's meaning colour (iris working, amber needs-you, sky ready, coral failed) that breathes while the activity moves. Working states
  show the ring with a rotating arc (`progress_ring`'s pieces), the others their status glyph. **Reduce Motion**: no rotation, no breathing.
  One window, 600 by 68, so a 40 character title and "+99 more" fit; everything outside the capsule is clear and passes clicks through
  (`Regions`: the only `hit()` element is the capsule; `a_press_on_the_capsule_is_the_overlays_only_click_target` reads
  `Runtime::hit_regions`).
- **Where**: the corner of the main screen, 20 points from the right and bottom edges of its *visible* area (Swift places from
  `visibleFrame`; Rune places from the screen edge, so `pill_margins` adds the Dock's and the menu bar's insets from `NSScreen`).
  Every Space (the overlay panel's own behaviour). Shown and hidden, never recreated; with `release_gpu_when_hidden` a hidden pill holds
  no window and no surface at all.
- **A press** brings this window forward and selects the task the capsule names (Swift only opens the workspace): `Pill::open_neko`.
- **The preference.** Swift keeps it in UserDefaults (`neko.presence.enabled`, default on, set in its Preferences). That is not a
  daemon setting, so this client keeps its own: `rune::prefs` key `presence` in the `neko-rune` state (`app::load_presence` and `write_presence`,
  next to the layout's: the prefs store is `app.rs`'s alone, `tests/allow_list.rs`), written only by the real window. Settings, "Summon & startup", has Swift's row ("Show activity while Neko works") when the
  overlays are on. The two clients' values are independent until P5.
- **Development.** `NEKO_PRESENCE_DEMO=1` (or `expanded`) with `NEKO_PRESENCE_DEMO_AFTER=<seconds>` puts a sample capsule on screen, as
  Swift's variable does.

### The launcher (`launcher.rs`)

`Palette.swift`'s summon panel with Rune's ⌘K palette as its content.

- A `launcher_panel()` window, **760 by 500**, on every Space, made once (prepared hidden at startup, shown and hidden after) and placed on
  the screen under the pointer with the top edge at the menu bar plus 15% of the visible height (Swift: `maxY - height - 0.15 * h`).
  It takes keys while shown (`AppHandle::open`: a non-activating panel takes keys without activating the app, so the frontmost app keeps
  its place; Swift activates Neko) and returns them on hide.
- **Content.** The panel is a window of its own with its own state type, so it keeps a copy of `App::palette_all()` taken at the show and
  filters it with the same `palette::narrow` the window's palette uses; the card is the palette's own (`view::palette::surface`,
  `search_field`), made generic over the state. A row's press, or Return, sends `App::palette_run(row)` to the main window and then hides
  the panel; only the rows that open something (a session, a page, a dialog: `Do::wants_window`) also bring that window forward.
- **The summon combo** is the daemon's: `GetHotkey` (the request Settings' summon row sends) is asked when the link first comes up and the
  answer is registered with `register_shortcut`; a changed answer moves the registration (a refusal keeps the old one). A combo changed in
  another client is heard when Settings is open (it re-asks), not live. The shortcut toggles the panel.
- **Escape** hides. **A click outside** is the panel losing the keys (`on_focus(false)`): there is no global mouse monitor as in Swift, and
  a click on the desktop does not always move the keys, so Escape and the summon key are the sure ways. A loss before the panel has had the
  keys in this show is the system settling and is ignored.
- **The generation guard** (`Presentation`, Swift's `PaletteGeneration`): every show and hide advances a counter shared by the window that
  summons and the panel; a hide queued by a focus loss carries the number it was queued under, so one delivered after the next show finds a
  newer number and does nothing (`a_stale_hide_after_a_new_show_does_nothing`).

### The stop-all key

⌘⇧Esc is registered with `register_shortcut` and calls `App::stop_all_key`: if something is running it asks the window's own
confirmation (`ask_stop_all`, "Stop all N running agents?") and brings the window forward; only a yes sends `Call::StopAll`
(`CancelAllWork`). Nothing running: nothing opens (Swift beeps).
**Swift does not ask** (`StopAllWork.run` cancels at once), the web does, and Rune may build this request only inside a `confirm(`
dialog (`tests/allow_list.rs`), so this client asks, like the web. The request is `CancelAllWork`, already in `WEB_SENDS`; no
`RUNE_EXTRAS` entry and no protocol change were needed for any of the three overlays (`GetHotkey` is the Settings row's).

### Shortcut conflicts

While the Swift app runs it holds the same Carbon keys. `register_shortcut` then fails; the client prints
`neko-rune: <what> was not registered: ... another app holds it` to stderr, keeps the old registration if it had one, and carries on
(`overlays::tests::..._a_refused_shortcut_is_logged_not_fatal`).

### Memory and checks on this Mac

Real check, 2026-11-18, release build of `neko-rune`, against an isolated harness daemon (`web_harness` over a seeded data folder under
`/tmp`; never the live daemon), Mac idle, 12 to 20 s after launch (`ps -o rss`, `footprint`):

| Run | RSS | Footprint |
| --- | --- | --- |
| Without `--overlays` | 112 MB | 311 MB |
| With `--overlays`, pill hidden (the launcher panel is prepared hidden; the pill has no window) | 116 MB | 326 to 328 MB |
| With `--overlays`, pill showing | 118 MB | 340 MB |

So the overlays cost about **15 MB** of footprint at rest (the retained launcher panel) and about **12 MB** more while the pill shows
(its surface is given back when it hides). The 300 MB baseline is the window's own settling at launch (ADR 0002's note on drawables).

- **The pill took no focus.** Finder was frontmost; 9 s later the pill appeared at the bottom right over it and `lsappinfo front` still
  said Finder.
- **Shortcut registration did not conflict** with the installed Swift app that was running: both `alt+Space` (the daemon's combo) and
  ⌘⇧Esc registered (`neko-rune: ... registered` on stderr). Carbon lets a second process register the same chord, so while both
  clients run, a press can reach both. This is another reason for the gate until P5. The keys were not pressed in the check: the Swift
  app would have answered too, and stop-all is destructive against the live daemon.
- The launcher panel and the stop-all key were checked headless (the panel's content, keys, generation guard and effects; stop-all
  through the dialog), not by pressing the real chord.

### Blocked: the agent pointer

The ADR lists "the agent pointer": a marker on screen where an agent's computer-use action lands. **Not built.** No code in Neko reports
computer-use coordinates: only the Peekaboo MCP is wired (`crates/neko-core/src/scopes.rs:170`), and its tool calls are opaque
transcript parts. What would unblock it is a **coordinate source from tool-call events**: the daemon extracting (x, y, display) from a
computer-use tool call's arguments or result into a typed event the clients can subscribe to (or the agent runtime posting one). With
that, the pointer is a second `OverlaySpec` (`OverlayInput::None`, `DisplayTarget::Native(display)`, `OverlayPlacement::Rect`) moved by
`OverlayHandle::place`.

### Still to do in P4

Notifications and the menu bar item (the ADR's other P4 pieces).

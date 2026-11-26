# The app shell (S0, 2026-11-26)

`apps/neko`: the desktop app on Rune. A thin client: the state in `state.rs` is what the daemon last said plus what is on screen
(open task, draft, sidebar). Everything else is the daemon's.

## Files

| File | What it does |
| --- | --- |
| `lib.rs` | `run()`: headless runs return first; then the menu prep, the saved frame, the Dock registration, `rune::run_windows`. `poll` drains the link, follows the width, sets the title, keeps the frame, replaces the menu. `on_key`: ⌘N, ⌃⌘S, Escape. |
| `state.rs` | `App` and what each event changes. No drawing, no I/O. |
| `view.rs` | The window: sidebar, content band, Home, the task view. |
| `link.rs` | The supervisor thread: find or start the daemon, connect, reconnect, an inbox the window drains. |
| `window_place.rs` | Where the window was: frame from placement, restore onto the displays there are now, write once still. |
| `menu.rs`, `about.rs`, `dock.rs` | The menu bar, the About panel, the Dock registration. |
| `scenes.rs` | Placeholder data for headless pictures. |
| `ui/` | rune-ui's copies. Never edited here. |

## The window

- **One band.** 52 points across the sidebar and the content (`Titlebar::new(52)`). Rune centres the 14 point traffic lights in it
  (top-left 16, 19), so the band's centre line is y = 26. Everything in the band sits on that line (`tests/structure.rs`).
- **Dragging.** Both halves of the band are `.window_drag()`; controls in it keep their presses. Double-click zooms (AppKit's own).
- **The lights' room** is `Cx::titlebar_inset()`: where the lights end, 0 in full screen.
- **Folding.** Under 1000 points wide the sidebar folds. ⌃⌘S, or the band's button, shows or hides it; that choice lasts until the
  window crosses 1000 again. The minimum window is 760 × 560.
- **Where it was.** The normal frame is written to `~/Library/Application Support/dev.neko.app/prefs.json` (Rune's prefs, apart from
  the daemon's data) once it has been still for half a second, and at close and quit. The next launch puts it back, clamped to the
  displays present. Full screen, zoomed and minimised frames are never saved. First launch: 1200 × 800, centred.
- **Title.** The open task's title, else "Neko".
- **Closing.** ⌘W closes the window and the app keeps running (`keep_running`); the Dock icon brings it back. ⌘Q quits.

## Menus

Neko (About Neko with version and commit, Services, Hide, Hide Others, Show All, Quit), File (New Task ⌘N, Close Window ⌘W),
Edit (Rune's standard items; AppKit adds Start Dictation and Emoji & Symbols), View (Hide/Show Sidebar ⌃⌘S, Enter/Exit Full
Screen ⌃⌘F), Window (Minimize, Zoom, Bring All to Front), Help (Neko Help, registered as the help menu). Raw AppKit does three
things Rune does not: register the Help menu, remove Rune's Window ▸ Close (File has it), keep AppKit's own Enter Full Screen out.
The model is pure and tested (`menu.rs`): no key is taken twice.

## Keyboard

- Tab goes once round every control in reading order: Hide sidebar, New task, Home, each task list (one stop each), the composer.
  `App::tab_visits_controls(true)` makes this so whatever the Mac's Keyboard Navigation setting is.
- In a task list the arrow keys move (`focus_group`), Return opens the row.
- In the composer Return sends, Shift+Return makes a new line. Escape gives focus back.
- ⌘N goes Home with the keyboard in the composer.

## Accessibility

`tests/a11y.rs` reads the AccessKit tree of every scene at both window shapes: every focusable thing has a name, list rows have
their position and selected state, lists and fields are labelled, each page has one level-1 heading, `main` and `navigation`
landmarks exist. Reduce Motion goes to rune-ui's theme (`Theme::with_reduced_motion`); the system's light/dark is read every frame
(`tests/structure.rs` switches it live).

## The link

The supervisor thread reads `daemon.json` and `token`, opens a `Link`, and pushes `Connected`, each message, then `Lost` into the
inbox. When no daemon answers it starts `neko-daemon` (from `NEKO_DAEMON_BIN`, else beside the app), at most every 3 s, in its own
process group, output appended to `<data>/daemon.log`, with a thread waiting on it. Retries back off to 2 s. On `Connected` the
window asks `subscribe` and `list_tasks`. Words sent while the daemon is away stay in the composer with a quiet line saying so.

## Data folder

`NEKO_DATA_DIR`, else `~/Library/Application Support/neko`. Headless runs and tests never touch it, nor the prefs, nor the Dock.

## Not done in S0

- **Spell check in the composer.** rune-ui's `PromptInput` builds its text field itself and has no way to turn on
  `El::spellcheck`. Upstream fix: a `PromptInput::spellcheck(bool)` that calls `.spellcheck(on)` on its `text_input`.
- **Unnamed hit areas.** rune-ui's 40 point hit areas (`hit(<key>)`) show to assistive technology as unnamed buttons around the
  named control. The rule allows that one pattern; upstream they should be `a11y_hidden` or take the control's name.
- No app icon yet.

## Check in the real app (headless cannot)

The lights on the band's line, dragging by the band, double-click zoom, full screen with the sidebar folded, quit and relaunch on
one display and after unplugging a second, the Dock menu, Help's search field, one Close item (in File), About's commit.

# The app shell (S0, 2026-11-26)

`apps/neko`: the desktop app on Rune. A thin client: the state in `state.rs` is what the daemon last said plus what is on screen
(open task, draft, sidebar). Everything else is the daemon's.

## Files

| File | What it does |
| --- | --- |
| `lib.rs` | `run()`: headless runs return first; then the menu prep, the saved frame, the Dock registration, `rune::run_windows`. `poll` drains the link, follows the width, sets the title, keeps the frame, replaces the menu. `on_key`: ⌘N, ⌃⌘S, Escape. |
| `state.rs` | `App` and what each event changes. No drawing, no I/O. |
| `view.rs` | The window: sidebar (each task's state as its mark), content band, Home and its folder chip. |
| `task_view.rs` | The task view: the conversation, steps, permission requests, run cards, the composer and its context ring. |
| `settings_view.rs` | Settings (⌘,): the agent CLIs, which one new tasks use, the permission mode. |
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

- Tab goes once round every control in reading order: Hide sidebar, New task, Home, each task list (one stop each), the composer,
  the folder chip. In a task: copy buttons, fold lines, request answers, then the composer and its context ring.
  `App::tab_visits_controls(true)` makes this so whatever the Mac's Keyboard Navigation setting is.
- In a task list the arrow keys move (`focus_group`), Return opens the row.
- In the composer Return sends, Shift+Return makes a new line. Escape gives focus back.
- ⌘N goes Home with the keyboard in the composer. ⌘, opens Settings (Neko ▸ Settings…).

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

## The Task view (S1, revised after the first real run)

- **One reading column.** Every block (the person's words aside, at the right) shares one left edge and one width: steps, words,
  the answer, requests, the card, the composer. Surfaces (fold lines' hover, the card, a request, the composer) start on the edge;
  an open "Worked for" shows its steps after it on the same edge, not indented (tested at both window shapes).
- **The conversation** is rune-ui's Conversation: pinned to the newest content until the person scrolls up, then a "Jump to
  latest" pill. The person's words are a Message at the right with a copy button that shows on hover or focus. The agent's
  ("Neko") are Markdown on the page, no bubble, selectable; only a run's final answer has a copy button.
- **Steps** are quiet single lines that fold, built on rune-ui's FoldLine. A shell step is its command, on one line, cut in its
  middle past 72 characters (the whole command is in the fold); other tools read as phrases ("Read src/app.rs", "Edited
  ~/notes.md", "Searched for “Settings”"). Paths inside the task's folder are relative; under home they start `~/`. A duration
  under a second is not shown. A command that exited non-zero keeps the finished mark with "exit 1" beside it; the red mark is
  only for a tool that could not run; a denied step says "Denied".
- **Thinking**: a thought with no words shows nothing. While the agent thinks and nothing else arrives, one live line says
  "Thinking…" (rune-ui's shimmer, still under Reduce Motion); while it waits on something else, "Working…". A thought with words
  folds as "Thought for 12s" ("Thought" under a second).
- **Finished runs fold** behind one "Worked for 29s" line (or "Stopped after", "Interrupted after"), in every run of a task; its
  final answer and any notice stay open. The person can open it; the choice lasts for that run while the app is open. A run with
  nothing to fold shows the same line without a chevron. The live run is always open.
- **The run card** shows only when the run changed files, has a branch, or failed: its state, files with + and −, the branch. No
  duration (the line has it) and no money.
- **The context ring** sits in the composer's footer: rune-ui's Context (a small bar and the share; a ring is an upstream ask),
  named "Context 31% used", reached by Tab, opened by Return or a press. Its panel is Neko's own (`ctx-panel`): context used,
  tokens in, out and cached for the task, and the cost as reported with how to read it. Escape closes it and focus goes back to
  the ring. Nothing else in the app shows money; the sidebar's foot no longer has a day total.
- **Permission requests**: see `permissions.md`.
- **The composer** (`reply`): Return sends a follow-up; while it works Send is Stop and Escape stops (also when the focus is
  elsewhere in the task; with the context panel open, Escape closes it first).
- The sidebar row's mark: hollow (new), dot (working), warning (needs you), check (done), cross (failed), slash (stopped or
  interrupted); the row's description says it in words. The window title follows the task. Finishing, failing and needing you
  are announced (`Tx::announce`).
- **Home's folder chip** opens the Mac's open panel (Rune's `native-file-dialog`, rfd); the choice is kept in the prefs
  (`last-folder`) and dropped if the folder is gone. Without a folder a task gets a scratch folder.

## Not done in S0

- **Unnamed hit areas.** rune-ui's 40 point hit areas (`hit(<key>)`) show to assistive technology as unnamed buttons around the
  named control. The rule allows that one pattern; upstream they should be `a11y_hidden` or take the control's name.
- No app icon yet.

## Check in the real app (headless cannot)

The lights on the band's line, dragging by the band, double-click zoom, full screen with the sidebar folded, quit and relaunch on
one display and after unplugging a second, the Dock menu, Help's search field, one Close item (in File), About's commit.

## The composer

- **Spell check is on** (`PromptInput::spellcheck(true)`, rune-ui 0.8.1). The system's checker marks a word about a second after
  typing stops.
- **Send with nothing to send** is a raised button at rest with a faint arrow: off, but still a button. With words it is ink.

# The app shell (S0, 2026-11-26)

`apps/neko`: the desktop app on Rune. A thin client: the state in `state.rs` is what the daemon last said plus what is on screen
(open task, draft, sidebar). Everything else is the daemon's.

## Files

| File | What it does |
| --- | --- |
| `lib.rs` | `run()`: headless runs return first; then the menu prep, the saved frame, the Dock registration, `rune::run_windows`. `poll` drains the link, follows the width, sets the title, keeps the frame, replaces the menu. `on_key`: ⌘N, ⌃⌘S, Escape. |
| `state.rs` | `App` and what each event changes. No drawing, no I/O. |
| `view.rs` | The window: the warm ground, the theme and its type scale, the content band, the panels that ask (rename, remove, retire, undo anyway, new cat). |
| `sidebar.rs` | The inset sidebar panel: New task, Home, Needs you, workspace sections of two-line task rows led by each cat's portrait, the Cats section. |
| `home_view.rs` | Home: the greeting, the composer with its cat and workspace pickers, the house of cats, Needs you and In progress. |
| `task_view.rs`, `turn.rs` | The task view: the conversation, turns led by their cat, steps on a rail, requests led by the asking cat, the receipt, the composer and its context ring. |
| `faces.rs` | The cats' portraits: a drawn face per face id on its own ground, the working ring and state dots (`design.md`). |
| `cat_view.rs`, `review_view.rs` | A cat's page; Review. |
| `settings_view.rs` | Settings (⌘,): the agent CLIs, which one new tasks use, the permission mode, workspaces, notifications. |
| `rounds.rs`, `rounds_view.rs` | Rounds (⌘3): the words for schedules and times, the New round / Edit panel's form; the page of cards and the panel (`rounds.md`). |
| `needs_view.rs` | Needs you (⌘2): one list of what waits for the person, answered in place (`needs-you.md`). |
| `notify.rs`, `deeplink.rs` | The notification model and its poster (a recorder outside the real window); `neko://task/<id>`. |
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
- **Title.** The open task's title and its workspace's name ("Fix the build — acme-api"), else "Neko". A title is one short line: the first line of the person's words, cut at a word
  before 60 characters (`neko_protocol::title`, used by the daemon when it stores a task and by the app when a task arrives, so
  the sidebar, the band and the window's title always agree).
- **Closing.** ⌘W closes the window and the app keeps running (`keep_running`); the Dock icon brings it back. ⌘Q quits.

## Menus

Neko (About Neko with version and commit, Services, Hide, Hide Others, Show All, Quit), File (New Task ⌘N, Close Window ⌘W),
Edit (Rune's standard items; AppKit adds Start Dictation and Emoji & Symbols), View (Home ⌘1, Needs You ⌘2, Rounds ⌘3, Hide/Show Sidebar ⌃⌘S, Enter/Exit Full
Screen ⌃⌘F), Window (Minimize, Zoom, Bring All to Front), Help (Neko Help, registered as the help menu). Raw AppKit does three
things Rune does not: register the Help menu, remove Rune's Window ▸ Close (File has it), keep AppKit's own Enter Full Screen out.
The model is pure and tested (`menu.rs`): no key is taken twice.

## Keyboard

- Tab goes once round every control in reading order: Hide sidebar, New task, Home, Needs you, Rounds, the workspaces list (one stop), each
  section's own buttons, Add workspace…, the cats (one stop), New cat…, the composer, the cat picker and the workspace picker in its footer, each cat's tile in the house. In a task: copy buttons, fold lines, request answers, then the composer and its context ring.
  `App::tab_visits_controls(true)` makes this so whatever the Mac's Keyboard Navigation setting is.
- In the workspaces list the arrow keys move through section headers and rows (`focus_group`), a typed name goes to it, Return
  opens a row or folds a section; Left and Right fold and open; ⌥⌘↑ and ⌥⌘↓ move a section (`workspaces.md`).
- In the composer Return sends, Shift+Return makes a new line. Escape gives focus back.
- ⌘1 is Home, ⌘2 Needs you (`needs-you.md`), ⌘3 Rounds (`rounds.md`).
- ⌘N goes Home with the keyboard in the composer, in the workspace of the task in view. ⌘, opens Settings (Neko ▸ Settings…).

## Accessibility

`tests/a11y.rs` reads the AccessKit tree of every scene at both window shapes: every focusable thing has a name, list rows have
their position and selected state, lists and fields are labelled, each page has one level-1 heading, `main` and `navigation`
landmarks exist. Reduce Motion goes to rune-ui's theme (`Theme::with_reduced_motion`); the system's light/dark is read every frame
(`tests/structure.rs` switches it live).

In a real window, Rune at 53f6d36 stops sending structural changes to the tree once an assistive client is attached: at once in
an inactive window behind another app, and after enough change in an active one (new list rows, a request card in a long
conversation, an open menu's items). See "Finding" in `docs/knowledge/qa.md`. Not fixed: it needs Rune (an upstream report is
drafted there).

## The link

The supervisor thread reads `daemon.json` and `token`, opens a `Link`, and pushes `Connected`, each message, then `Lost` into the
inbox. When no daemon answers it starts `neko-daemon` (from `NEKO_DAEMON_BIN`, else beside the app), at most every 3 s (Retry
starts it at once), in its own process group, output appended to `<data>/daemon.log`, with a thread waiting on it. Retries back off
to 2 s. On `Connected` the window asks `subscribe`, `list_tasks` and `list_runtimes`.

## Startup order

1. `main`: the menu prep, the saved frame (one small prefs read), the Dock registration, `rune::run_windows`. Nothing here or in
   `init` touches the daemon.
2. The first frame: the shell, the sidebar ("Starting Neko…" at its foot, "Loading…" where the tasks go), Home with a usable
   composer.
3. `on_ready` (after the first frame): `start_link` starts the link thread and returns at once. Finding or starting the daemon, the
   hello and the first lists happen on that thread; the window drains what arrives.
4. Return before the daemon answers keeps the words in the composer (`App::queued`) with a quiet line; they are sent on
   `Connected`. After 6 s without an answer (`SLOW_AFTER`) a calm line says Neko has not started, with Retry.

`tests/startup.rs` holds this: with a daemon that takes 5 s, the first frame is drawn in under a second (measured about 75 ms),
`begin_link` returns in microseconds, every frame stays quick, and a task typed meanwhile is sent and opens. A daemon that never
comes up gets the line and Retry. `lib.rs`'s own test fails if `run` or `init` reaches the link before `on_ready`.

The 2.8 s "setup in `main`" Rune once reported was macOS checking a freshly built 150 MB debug binary on its first exec (about 2 s
before `main` runs, measured with `--list-scenes` on a fresh copy), not the daemon. A second launch of the same binary does not pay it.

## Data folder

`NEKO_DATA_DIR`, else `~/Library/Application Support/neko`. Headless runs and tests never touch it, nor the prefs, nor the Dock.

## The Task view (S1, revised after the first real run)

- **One reading column.** Every block (the person's words aside, at the right) shares one left edge and one width: steps, words,
  the answer, requests, the card, the composer. Surfaces (fold lines' hover, the card, a request, the composer) start on the edge;
  an open "Worked for" shows its steps after it on the same edge, not indented (tested at both window shapes).
- **The conversation** is rune-ui's Conversation: pinned to the newest content until the person scrolls up, then a "Jump to
  latest" pill. The person's words are a soft well at the right (`turn::person`); its copy button shows on hover or focus and takes no room while hidden. The cat's
  (led by the cat's face and name, `cats.md`) are Markdown on the page, no bubble, selectable; only a run's final answer has a copy button.
- **Steps** are quiet single lines that fold, built on rune-ui's FoldLine, hung on a hairline rail in the margin with neutral marks (a tick in the third ink, a dash for denied). A shell step is its command, on one line, cut in its
  middle past 72 characters (the whole command is in the fold); other tools read as phrases ("Read src/app.rs", "Edited
  ~/notes.md", "Searched for “Settings”"). Paths inside the task's folder are relative; under home they start `~/`. A duration
  under a second is not shown. A command that exited non-zero keeps the finished mark with "exit 1" beside it; the red mark is
  only for a tool that could not run; a denied step says "Denied".
- **Thinking**: a thought with no words shows nothing. While the agent thinks and nothing else arrives, one live line says
  "Thinking…" beside the turning mark a running step has (still under Reduce Motion); while it waits on something else, "Working…", with the step it is on. The line never shows the cat again (the turn's head already has its portrait, name and ring); its name for assistive technology still says who ("miso is thinking"). A thought with words
  folds as "Thought for 12s" ("Thought" under a second).
- **Finished runs fold** behind one "Worked for 29s" line (or "Stopped after", "Interrupted after"), in every run of a task; its
  final answer and any notice stay open. The person can open it; the choice lasts for that run while the app is open. A run with
  nothing to fold shows the same line without a chevron. The live run is always open.
- **The receipt** shows only when the run changed files, has a branch, failed, or undo has something to say: the cat and the sentence
  ("miso changed 3 files", + and −), each file with its own counts (the run's diff, asked once, `App::receipts`), the undo line,
  the branch, Review, Undo or Redo, Reveal in Finder and Copy branch. No duration (the line has it) and no money.
- **The context ring** sits in the composer's footer: rune-ui's Context with its ring face, named "Context 31% used, 61.4k of 200k
  tokens", reached by Tab, opened by Return, Space or a press. Its popover is the kit's: context used and window, tokens in, out
  and cached, the cost when reported, and "API-equivalent, as Claude Code reports it. Not charged on a subscription." only for a
  subscription login. Escape closes it and focus goes back to the ring. Nothing else in the app shows money.
- **Permission requests**: see `permissions.md`.
- **Settings**: the line saying what the person's Claude Code rules allow sits right under the Permissions heading, in sight
  without scrolling in the default window (tested).
- **The composer** (`reply`): Return sends a follow-up; while it works Send is Stop and Escape stops (also when the focus is
  elsewhere in the task; with the context panel open, Escape closes it first). While a request waits in the task, Escape answers
  it no and the agent carries on: when it appears the keyboard leaves the composer, which shows Send (not Stop) and says "Escape
  denies it" until the request is answered. In the Needs you list Escape goes back, whatever the task behind it is doing.
- After a task is sent from Home the keyboard is in its composer. After the last Needs you item is answered it is on the
  sidebar's Needs you row.
- **The band** keeps the task's title: in a content area under 700 points (the smallest window with the sidebar shown) the folder
  line is left out and the workspace's name shortens first; the title keeps at least 140 points.
- A sidebar row leads with its cat's portrait in its state: a turning ring while it works, an amber dot when it needs the person
  (a request waits, or the run ended with a question), a red dot when it failed; under the title, "miso · working · 12 min". The
  row's description says the cat and the state in words. When the run ended with a question the composer says "Answer miso's
  question".
- **The task's folder** shows once, quietly, at the band's right: the newest run's working folder (else the chosen folder),
  shortened with `~` and cut in its middle, the branch beside it, and a Reveal in Finder button (`Tx::reveal_path`). The window title follows the task. Finishing, failing and needing you
  are announced (`Tx::announce`).
- **Home's workspace picker** is a pop-up menu in the composer's footer: General, the workspaces, "Add folder…" (the Mac's open
  panel, Rune's `native-file-dialog`, rfd). The choice is kept in the prefs (`last-workspace`). See `workspaces.md`.
- **The task's band** leads with its cat's portrait (a press opens the cat's page), then the title (18 points, semibold: it leads
  and gets the room first), the run counter ("Step 4", or "5 steps · 2m 14s"), the workspace's name, the folder and branch, and a
  menu with All changes and Remove task…. The folder line is the quietest thing in the band: 12 points in the faintest ink that
  still reads, the path cut in its middle to 168 points (220 with the sidebar folded), the branch to 112.
- **Page changes are cuts, then a fade in.** A page that replaces another (Home, a task's conversation, Review, a cat's page, Needs
  you, Settings) is gone in that frame and the new one fades in on a clean page (`view::page_swap`). Rune fades a removed element
  out over a quarter second by default, so the conversation and Review were drawn one over the other when Review opened
  (QA defect 10); `tests/page_swap.rs` takes frames while the swap moves and fails if both are drawn.
- **Review's files** are rune-ui's Diff (0.10): `.stats` for a folded file's counts, `.hide_zero(true)` (a file that only gained
  lines shows no "−0"), `.selectable(true)` and `.foot(..)` for the note, Show all, Copy diff, Copy path and Reveal.
- **Settings** has three pages, Agents, Workspaces and Notifications, switched by rune-ui's SegmentedControl.

## Not done yet

- No app icon yet.

## Check in the real app (headless cannot)

The lights on the band's line, dragging by the band, double-click zoom, full screen with the sidebar folded, quit and relaunch on
one display and after unplugging a second, the Dock menu, Help's search field, one Close item (in File), About's commit.

## The composer

- **Spell check is on** (`PromptInput::spellcheck(true)`, rune-ui 0.8.1). The system's checker marks a word about a second after
  typing stops.
- **Send with nothing to send** is a raised button at rest with a faint arrow: off, but still a button. With words it is ink.

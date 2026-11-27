# The app shell (S0, 2026-11-26)

`apps/neko`: the desktop app on Rune. A thin client: the state in `state.rs` is what the daemon last said plus what is on screen
(open task, draft, sidebar). Everything else is the daemon's.

## Files

| File | What it does |
| --- | --- |
| `lib.rs` | `run()`: headless runs return first; then the menu prep, the saved frame, the Dock registration, `rune::run_windows`. `poll` drains the link, follows the width, sets the title, keeps the frame, replaces the menu. `on_key`: ⌘N, ⌃⌘S, Escape. |
| `state.rs` | `App` and what each event changes. No drawing, no I/O. |
| `view.rs` | The window: the warm ground, the theme and its type scale, the content band, the panels that ask (rename, remove, retire, undo anyway, new cat). |
| `sidebar.rs` | The inset sidebar panel: New thread, Needs you, Briefing, Rounds; the Cattains (only the cats at work, absent when none is); Threads with View and New thread, grouped by project (or one list), two-line thread rows led by a status mark; Add project…. |
| `threads.rs`, `status_mark.rs` | The words and the order of the threads (age, second line, groups, the hold that keeps a working row in its place) and the status mark (`threads.md`). |
| `new_thread_view.rs`, `new_thread.rs` | New thread, the screen the app opens on: the headline, the composer, the chips (Project, New worktree / In the folder, branch); its state (the place, the branch, the held send, the quiet "Started" line) (`threads.md`). |
| `task_view.rs`, `turn.rs` | The task view: the conversation, turns with no head, steps on a rail, requests led by a small dot, the receipt, the composer and its context ring. |
| `faces.rs` | The cats' portraits: a drawn face per face id on its own ground, the working ring and state dots (`design.md`). |
| `cat_view.rs` | A cat's page. |
| `pane.rs`, `pane_view.rs`, `review_view.rs` | The right pane: its state and words (open or shut, width, the tabs, each thread's overview), its drawing (header with tabs, Overview, a kitten's tab, the sheet), and the Changes tab (the files as rune-ui's Diff, the source switch, Undo, Redo, Copy diff). |
| `settings_view.rs` | Settings (⌘,): the agent CLIs, which one new tasks use, the permission mode, workspaces, notifications. |
| `rounds.rs`, `rounds_view.rs` | Rounds (⌘3): the words for schedules and times, the New round / Edit panel's form; the page of cards and the panel (`rounds.md`). |
| `briefing.rs`, `briefing_view.rs` | The Briefing (⌘4): the words for a window, a round line and the totals, the state it keeps; the page (`briefing.md`). |
| `plugins.rs`, `plugins_view.rs` | Plugins: the words for a state, a check and an ask; the Add sheet's state, read live with the daemon's own `parse_mcp_json`; Settings ▸ Plugins, the Add sheet, the approval block Needs you shares, a cat's plugins (`plugins.md`). |
| `subagents.rs`, `subagents_view.rs` | Subagents: which tasks are one, what a thread's row says, the card, the wake note, the waiting line, the read-only foot, the breadcrumb ("Subagents in the app"). |
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

Neko (About Neko with version and commit, Services, Hide, Hide Others, Show All, Quit), File (New Thread ⌘N, Close Window ⌘W),
Edit (Rune's standard items; AppKit adds Start Dictation and Emoji & Symbols), View (New Thread ⌘1, Needs You ⌘2, Rounds ⌘3, Briefing ⌘4, Hide/Show Sidebar ⌃⌘S, Enter/Exit Full
Screen ⌃⌘F), Window (Minimize, Zoom, Bring All to Front), Help (Neko Help, registered as the help menu). Raw AppKit does three
things Rune does not: register the Help menu, remove Rune's Window ▸ Close (File has it), keep AppKit's own Enter Full Screen out.
The model is pure and tested (`menu.rs`): no key is taken twice.

## Keyboard

- Tab goes once round every control in reading order: Hide sidebar, New thread, Needs you, Briefing, Rounds, the Cattains (one stop, only when a cat is at work), Threads' View and New thread, the projects list (one stop), each
  section's own buttons, Add workspace…, the composer on New thread, then its chips in order (Project, New worktree / In the folder, the branch). In a task: copy buttons, fold lines, request answers, then the composer and its context ring.
  `App::tab_visits_controls(true)` makes this so whatever the Mac's Keyboard Navigation setting is.
- In the workspaces list the arrow keys move through section headers and rows (`focus_group`), a typed name goes to it, Return
  opens a row or folds a section; Left and Right fold and open; ⌥⌘↑ and ⌥⌘↓ move a section (`workspaces.md`).
- In the composer Return sends, Shift+Return makes a new line. On New thread ⌘Return starts the thread without opening it (`threads.md`). Escape gives focus back.
- ⌘1 is New thread, ⌘2 Needs you (`needs-you.md`), ⌘3 Rounds (`rounds.md`), ⌘4 the Briefing (`briefing.md`; ⌘↩ marks it read).
- ⌘N goes to New thread (the sidebar's New thread row is selected while it shows) with the keyboard in the composer, in the project of the thread in view. ⌘1 does the same. ⌘, opens Settings (Neko ▸ Settings…).

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
2. The first frame: the shell, the sidebar ("Starting Neko…" at its foot, "Loading…" where the tasks go), New thread with a usable
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
  latest" pill. The person's words are a soft well at the right (`turn::person`); its copy button shows on hover or focus and takes no room while hidden. The agent's
  words (a turn has no head: no portrait, name or "done" row) are Markdown on the page, no bubble, selectable; only a run's final answer has a copy button.
- **Steps** are quiet single lines that fold, built on rune-ui's FoldLine, hung on a hairline rail in the margin with neutral marks (a tick in the third ink, a dash for denied). A shell step is its command, on one line, cut in its
  middle past 72 characters (the whole command is in the fold); other tools read as phrases ("Read src/app.rs", "Edited
  ~/notes.md", "Searched for “Settings”"). Paths inside the task's folder are relative; under home they start `~/`. A duration
  under a second is not shown. A command that exited non-zero keeps the finished mark with "exit 1" beside it; the red mark is
  only for a tool that could not run; a denied step says "Denied".
- **Thinking**: a thought with no words shows nothing. While the agent thinks and nothing else arrives, one live line says
  "Thinking…" beside the turning mark a running step has (still under Reduce Motion); while it waits on something else, "Working…", with the step it is on. The line shows no cat. A thought with words
  folds as "Thought for 12s" ("Thought" under a second).
- **Finished runs fold** behind one "Worked for 29s" line (or "Stopped after", "Interrupted after"), in every run of a task; its
  final answer and any notice stay open. The person can open it; the choice lasts for that run while the app is open. A run with
  nothing to fold shows the same line without a chevron. The live run is always open.
- **The change line** (`turn::change_line`) ends a run's fold when it changed files or failed: `Changed 3 files · +19 −7 · Review · Undo`
  in the small step, the second ink, with the figures in green and red and Review and Undo (or Redo) as ghost buttons (keys `run-<id>`,
  `-facts`, `-review`, `-undo`, `-redo`). A failed run starts with "Could not finish" in red, a stopped or interrupted one says how far
  it got, an undone one says "now undone" and offers Redo; the one quiet sentence undo sometimes has (`Offer::line`) is under it. A
  kitten's line has no buttons. It replaced the receipt card (the files, the branch, Reveal and Copy branch moved to Overview); no
  duration (the fold line has it) and no money.
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
- After a thread is started from New thread with Return the keyboard is in its composer. After the last Needs you item is answered it is on the
  sidebar's Needs you row.
- **The band** keeps the task's title: in a content area under 700 points (the smallest window with the sidebar shown) the folder
  line is left out and the workspace's name shortens first; the title keeps at least 140 points.
- A sidebar row leads with a status mark, not a cat's face (`threads.md`): a turning ring while it works, an amber dot when it needs
  the person (a request waits, or the run ended with a question), a red dot when it failed and has not been opened since. The title
  is bold while there is activity the person has not seen; opening the thread (and anything that arrives while it is in view and Neko
  is in front) reads it. Under the title: `Needs you: …`, `Failed: …`, else the agent's last line. The row's description says the state,
  the line and the age in words. When the run ended with a question the composer says "Answer the question".
- **The task's folder** is in the pane's Overview (the newest run's working folder, else the chosen one, shortened with `~` and cut in its
  middle, the branch, Reveal in Finder and Copy branch), not in the band. The window title follows the task. Finishing, failing and
  needing you are announced (`Tx::announce`).
- **New thread's Project chip** is a pop-up menu in the chip row under the composer, with the other chips: No project, the projects, "Add folder…" (the Mac's open
  panel, Rune's `native-file-dialog`, rfd). The choice is kept in the prefs (`last-workspace`). See `workspaces.md`.
- **The task's band** leads with the title (a task a cat was given shows the cat's name as quiet text, `task-cat`, that opens its page) (18 points, semibold: it leads
  and gets the room first), the run counter ("Step 4", or "5 steps · 2m 14s"), the workspace's name, a
  menu with All changes (the Changes tab on All runs) and Remove thread…, and the pane button (below) while the pane is shut. The
  band has no folder or branch line any more.
- **Page changes are cuts, then a fade in.** A page that replaces another (New thread, a task's conversation, Review, a cat's page, Needs
  you, Settings) is gone in that frame and the new one fades in on a clean page (`view::page_swap`). Rune fades a removed element
  out over a quarter second by default, so the conversation and Review were drawn one over the other when Review opened
  (QA defect 10); `tests/page_swap.rs` takes frames while the swap moves and fails if both are drawn. The pane's tab bodies swap the
  same way (`page_swap` on the body that is replaced, not on a container that stays), and the same test holds them.
- **The Changes tab's files** are rune-ui's Diff (0.10): `.stats` for a folded file's counts, `.hide_zero(true)` (a file that only gained
  lines shows no "−0"), `.selectable(true)` and `.foot(..)` for the note, Show all, Copy diff, Copy path and Reveal.
- **Settings** has six pages, Agents, Workspaces, Notifications, Cats, Briefing and Plugins, switched by rune-ui's SegmentedControl. ⌘, then the page switch reaches Plugins (`settings-page-5`); "Open in Settings" on a plugin's Needs you card and a plugin's name on a cat's page open it at that plugin, with the keyboard on its first control.

## Plugins (app side)

- **Settings ▸ Plugins** is a card per plugin: a plug tile, the name (and its version when it has one), where it came from, its state in words (a badge: amber where the person is needed, red for "Can't be read", green only for On), the switch once approved. While a plugin is not approved the card holds the approval block (exactly what will run, then Approve and Decline). Then its declared settings as fields (text: input and Save; secret: masked field and Set, then "Set" with Change and Remove; yes/no: a switch; choice: segmented up to three, else a select; folder: a path with Choose…), who may use it ("Every cat" or "Only some cats" with a switch per cat; none chosen sends nothing, the last cat cannot be turned off), and Test with its result and Remove… (asks first).
- **The Add sheet** (`Add MCP server…`) reads the paste while it is typed with the daemon's own code. Add stays off until it reads. A single server without a name offers a Name field. Values in `env` and `headers` are never drawn, only names; the paste lives only in the sheet's text area and is dropped when the sheet closes. `Add plugin folder…` uses the Mac's folder panel exactly as Add workspace… does (`tx.open_folder`, so the QA driver answers it with `resolve_file_dialog`).
- **Pending** is per plugin: `Ui.testing` ("Testing…", up to ten seconds, cleared if the link is lost) and `Ui.busy` (a change out). A refusal is a line on the card; one from the Needs you list brings the card back with the words.
- **A secret** is held only while it is typed, sent once and dropped; Change is its own state (`Ui.changing`), so a stray edit event never turns a set secret back into a field. Tested: the value is in no accessibility name, text or description.
- **Needs you** gives a plugin the plug tile (with the amber dot) in place of a cat's portrait, groups it under "Plugins", has no Open task (it has Open in Settings), and Approve and Decline answer nothing on Return (Space presses), as on a destructive request. Its notification has no buttons, is made only while Neko is not in front, and a click opens the list at the card.
- **A tool step** of a plugin reads "acme-docs · search", never `mcp__acme-docs__search`.

## Kittens in the app

A kitten (the app's word for a subagent: a helper a thread starts, numbered, with no name and no face) is a task (`Task.subagent`: its parent, its number, the thread at the top) and never a thread. `subagents.rs` holds the words and the state, `subagents_view.rs` the drawing.

- **Not listed.** The sidebar (`tasks_in`, the Cattains), a cat's page and counts, and every notification skip tasks with `subagent`; a kitten finishing says nothing (its thread's end does), but its permission request reaches Needs you like any other. When a kitten is open its thread's row is the chosen one.
- **The thread's row** (`threads::second_line`, `status_mark.rs`) says what it waits on as its second line, after Needs you and Failed and before the agent's last words: "Waiting on Kitten 1" (one live kitten and the thread waiting), "Kitten 1 working", "N kittens working", or "Reading the reports" (waiting, none live). Its mark is the working ring held still in the second ink (`Mark::Waiting`), the right side its age, and its description says "waiting". A kitten's request turns the thread's mark amber and the line reads "Needs you: Run a command · Kitten 1". Waiting is not active, so the thread is not in the Cattains.
- **The card** stands where `PartBody::Subagent` is: one raised card, a row (`subagent-card-<number>`) per kitten, `Kitten 1`, its title, its state at the right (Queued, Working, Needs you, Waiting, Done, Failed, Stopped, Interrupted, Has a question), "Shared folder" and its last line (the newest agent words' first line, else the newest step, from the transcript parts this window has of it; a thread's transcript asks once for the transcripts of its kittens it has not read, `ask_subagent_lines`), and Open (`subagent-open-<number>`, name "Open Kitten 1"). Kitten parts with only Neko's own tool steps between them are one card. In a finished run the cards are outside the "Worked for" fold, after it.
- **The wake note** (`PartBody::Wake`) stands before the run it started, outside its fold: "Woken by Kitten 1 · done" with a hairline each side, then a closed fold "Report from Kitten 1" (key `wake-report-<seq>`, its button `…-trigger`) holding the summary, the files and the branch.
- **The waiting line** (`waiting-line`) is the last line of a waiting thread: "Waiting on Kitten 1 and Kitten 2" with each name a link (`waiting-link-<number>`) to that kitten; "Reading the reports" when none is live. The composer of a waiting thread offers Stop, and Stop (or Escape) stops the thread and its kittens.
- **A kitten opened** has no composer: a quiet line (`subagent-readonly`) "This is Kitten 1's own conversation, to read. Say what you want changed in Fix login.", Back (`subagent-back`, to its thread; to its parent for a deeper one) and, while it works, Stop (`subagent-stop`, "Stop Kitten 1"). Its turns have no head and its requests read "Kitten 1 asks" with no portrait, answered in place. Escape goes back to the thread (it denies a waiting request first) and never stops the kitten.
- **The band** reads `Fix login / Kitten 1` (deeper, `Fix login / Kitten 1 / Kitten 1.2`): `band-title` is the heading (its name the whole breadcrumb), `crumb-thread` a button to the thread, `crumb-subagent` plain words; there is no cat and no task menu (a kitten leaves with its thread). The window's title is the same breadcrumb.
- **Needs you** reads "Kitten 1 asks" led by the number on a quiet tile, "in Fix login" as its second line and "Open Kitten 1"; the notification's body starts "Kitten 1 asks:".
- **Settings ▸ Plugins** starts with the switch `own-mcp`, "Also give runs my own Claude Code MCP servers", off by default, which sends `SetSettings` with `own_mcp` toggled and the other fields kept, and one line saying what on and off mean.

Scenes: `subagents-cards`, `subagents-wake`, `subagents-wake-open`, `subagents-waiting`, `subagent-view`, `subagent-asks`, `needs-subagent` (`tests/subagents.rs` holds them).

## Not done yet

- No app icon yet.

## Check in the real app (headless cannot)

The lights on the band's line, dragging by the band, double-click zoom, full screen with the sidebar folded, quit and relaunch on
one display and after unplugging a second, the Dock menu, Help's search field, one Close item (in File), About's commit.

## The composer

- **Spell check is on** (`PromptInput::spellcheck(true)`, rune-ui 0.8.1). The system's checker marks a word about a second after
  typing stops.
- **Send with nothing to send** is a raised button at rest with a faint arrow: off, but still a button. With words it is ink.
- **While the agent works** (S5, `composer.md`, `composer.rs`): the PromptInput knows one way to send, so the keys that choose a mode
  are read by `reply-block`, the column round it, with `on_key_preview` (before the text field sees them): Return, ⌥Return and ⌘Return
  send as steer, queue and interrupt, ↑ and ↓ recall, Escape stops. The button is Stop for an empty box and Send for words (the
  composer's phase follows the words). Above the box, in this order: `waiting` (the messages waiting, keys `waiting-N`, `waiting-N-send`,
  `-edit`, `-discard`, `waiting-fold`), then `activity-bar` (and `activity-list` when open: `activity-run`, `activity-kitten-N` with
  `-stop`, `activity-command-N`); the toolbar has the `send-mode` chip and its menu; under the box `reply-hints`. The keyboard stays in the
  box after a send. A message the person steered into a run is stored with no run, at a time the run was going; the conversation
  draws it after that run's block (one run is one block, with one fold and one key), not inside it.
- **Settings ▸ Agents** has "While the agent works": Return steers (default) or queues.

## The right pane (S3, 2026-11-27)

A thread's own pane, `pane.rs` (state, words) and `pane_view.rs` (drawing). It belongs to the thread in view (a kitten opened in the main
view shows its thread's pane) and to no other page: New thread, Settings, Needs you, Rounds, the Briefing and a cat's page have none.

- **Where it sits.** Docked, it is the right region of rune-ui's Split (`window-right`): 380 points at first, 320 to 560 and at most half
  the window, dragged by its edge (a hairline; double-click goes back to 380, the arrow keys move it), collapsing when dragged past the
  least. The conversation keeps at least 440. The pane is the sidebar's panel material, full height, with the hairline at its edge. It is
  open from a window 1240 points wide until the person closes it; between 1100 and 1240 it is shut until opened. The choice
  (`pane-open`) and the width (`pane-width`) are in the prefs. Under 1100 points it is a rune-ui Sheet ("Details") over the conversation's
  right edge, shut until the band's button opens it, closed by Escape, its close button or a press outside; it never opens by itself. Crossing 1100
  shuts the sheet.
- **The header** is on the band's 52 point line: rune-ui's Tabs in the Pane variant (the chosen tab semibold with a 2 point underline;
  tabs that do not fit fold into "More tabs", the chosen one staying), then the pane button (`pane-hide`, ⌥⌘B). While the pane is shut
  the same button (`pane-show`) is the band's last control, after the thread's menu.
- **Tabs, in this order:** Overview (always first), Changes (with its file count), then a tab for each kitten opened, in the order they were
  opened (`pane-tabs-0`, `-1`, `-2` …). Overview and Changes cannot be closed; a kitten's tab has its ×, and ⌘W on a focused tab closes it
  (the tab's own key, which the File menu's Close Window ⌘W lets through first: the window closes only when no tab has focus). The
  tab shown is kept per thread while the app is open. ⌘⌥1 … ⌘⌥9 choose a tab by its place (and open the pane), ⌘⇧G and ⌘D the Changes tab, ⌘⌥B shows or hides the pane
  (View ▸ Show/Hide Pane, View ▸ Changes carry the first and the third).
- **It opens by itself** once per thread (this session) when the first kitten starts or the first changes land in the thread being
  looked at, unless the person shut it (`App::pane_proactive`); not as a sheet.
- **Overview** (`pane-overview`) reads top to bottom in five seconds, a section only when it has something: **Needs you** (an amber dot,
  what is asked, how long ago; under the pointer or focus the age gives way to Allow once and Deny for a permission, the
  kitten's asks included; a press goes to the kitten's tab), **Kittens** (the tree, 12 points a step: the status mark, "Kitten 1.1", its title, the model and "Shared folder",
  the state word and its age at the right; under the pointer a live one offers Stop; a press opens its tab), **Changes** (`+19 −7 · 3 files`, the first five files each
  with its figures and "Show all N" for the rest, Review, and Undo last run or Redo, with undo's one quiet line), **Folder and branch**
  (`~/…` cut in the middle, Reveal in Finder, Copy branch), **Waiting on** (the kittens the thread rests on), **Usage** (the context
  ring's numbers in a line; the cost stays in the ring's popover, the only place that shows money). Nothing to show: one quiet line.
  The data is the daemon's `get_overview` (the folder, the branch, the files all runs changed), asked when Overview shows and again
  when a run of the thread or of one of its kittens ends (`RunChanged`); Needs you, the tree and Usage come from what the window has.
- **Changes** (`changes-tab`): a segmented switch **This run**, **All runs**, **Uncommitted** (`changes-source-0..2`), the summary, the
  files as Diff, and under them Undo, Redo and Copy diff (`changes-foot`). Review on a run's change line opens it on that run; ⌘⇧G
  opens it on the newest run that changed something, else on all runs. Uncommitted for a folder that is not a repository says what the
  daemon says. Copy diff copies every file's lines (it asks for the ones not opened and copies when the last arrives).
- **A kitten's tab** (`pane-kitten`): the kitten's title and state, Stop while it lives and "Open in main view", the brief folded
  (`pane-kitten-brief`), and, once it is done, its report first (`pane-kitten-report`), then its conversation read-only (the same blocks as the main view, `task_view::kitten_items`, with the
  keys of whatever depends on a part's place prefixed `pane-` so no two elements of the window share one; its first words, the brief, are not said twice). Its
  requests are answered in the tab. There is no composer: steer it through the thread.
- **Open on a kitten's card** and the names in the waiting line open its tab; Command-click on Open, or "Open in main view", takes the
  kitten into the main view (breadcrumb, read-only) and takes its tab away.
- The Changes tab's badge, Overview's sections and the tab strip are checked by `tests/pane.rs`; the daemon's side by
  `crates/neko-daemon/tests/undo.rs`; the journeys by the rig's `pane-overview`, `pane-undo`, `pane-subagent-tab` and `pane-narrow`.

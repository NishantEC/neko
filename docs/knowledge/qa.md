# The QA rig (2026-11-26)

Scripted runs of the real app against a real daemon, each recorded (video, step pictures, accessibility trees) with a written
verdict. It does not replace the tests: it walks whole journeys the way a person would and keeps the evidence.

## Commands

```text
scripts/qa/run.sh first-task                  one scenario, fake CLI, headless
scripts/qa/run.sh first-task --window         the same in a real window
scripts/qa/run.sh real-smoke --cli real       the installed claude, your own login: real model calls
scripts/qa/all.sh [--window] [--real]         every scenario; one summary.md; non-zero exit on any failure
```

Set `NEKO_TARGET` to your own build cache. Results go to `$NEKO_QA_OUT` (default `/tmp/neko-qa`)
`/<UTC time>-<scenario>[-window][-real]/`: `video.mp4`, `steps/NN-name.png`, `tree/NN-name.json`, `log.txt` (driver, daemon,
app, ffmpeg; home shown as `~`), `result.json` and `report.md`. The scripts refuse an output folder inside the repository. Each
run waits for `~/.agents/bin/machine-ok` to say OK, makes a temporary folder (data, a HOME of its own for the fake, the scenario's
folders), starts a real `neko-daemon` there, and afterwards checks that every process it started is gone (by the PIDs it recorded,
and anything still running from its folder) and removes the folder. Anything left is stopped by PID and fails the run.

## The rounds scenarios

`rounds` makes a round from the Rounds page (mochi, acme-api, Run now only), runs it now, checks the task is mochi's in acme-api and
says where it came from, opens the history, switches the round off and removes it (the task stays). `rounds-due` starts the daemon
with `NEKO_DEBUG_CLOCK=1` and moves the scheduler's clock with the `clock` step (`{"do": "clock", "minutes": 361}`, which sends
`debug_clock`): the round starts by itself and its step waits in Needs you (ask mode); the next time is skipped with its reason;
pause all holds the third; after the request is denied a later time makes a second task. The step works only against a debug
daemon started with that variable; a release daemon refuses it (`rounds.md`).

## The briefing scenarios

`briefing` makes a round (every day, a minute from now, so the daemon is started with `TZ=UTC` and the step's words say `{{utc+1}}`, the UTC time N minutes from now), a task that finishes and one that waits on a request (ask mode), waits for the round to start by itself, opens the Briefing (⌘4) and checks its sections and the first sentence of each answer, answers the request from the page, marks it read with ⌘↩, checks "You're caught up" and that New thread's Briefing line is gone, then finishes a new task and checks the line comes back. `briefing-morning` sets the morning time three hours ahead in Settings ▸ Briefing, moves the scheduler's clock past it (`clock`) and checks one notification, "Your briefing: 1 finished, 1 needs you", in the notification log, none more when the clock moves on the same day, and the same counts on the page. Headless runs have no system to post to: the driver gives the app a poster that writes to the run's `notify.jsonl` in the form the window runs' log has, so `notified`, `withdrawn` and `posts` (`{max, titled}`: only the posts whose title starts so) are checked the same way in both.

## The sidebar scenarios

The driver has no window to focus, so Neko is never "in front" there; it sets `App::read_unfronted` so the thread in view is read as its words arrive, as it is in a real window that is in front. A real window behind another app leaves what arrives unread (`tests/threads.rs`). The driver also reads the saved grouping, as the real window does.

Headless only (they use the driver's `order` check: the named elements are on screen from top to bottom in that order).

`sidebar-threads` adds two projects, makes a thread that ends with a question and a newer one in acme-api and a slow one in mochi-notes,
opens the first while the slow one streams, and checks the group order (acme-api, mochi-notes, No project last), that the thread that
needs the person sits above the newer one, that the streaming thread is unread (its row says so) and working, and that opening it reads
it and it stays read while it streams. `sidebar-status` switches Settings to ask mode and makes, in acme-api, a finished thread, a thread
that crashes (marked unread from its menu while another thread is open, so it shows as failed), one that waits on a request and one that
works; it checks each row's words and second line, that a thread that finishes while another works does not move the working row even
after the working thread streams on, and that opening the failed thread clears Failed. `sidebar-group-none` checks View ▸ Group by None
(one list, newest activity first, each row naming its project, no headings), that the choice survives a quit and relaunch, and that
Project brings the groups back.

## The plugin scenarios

Both start the daemon (a debug build) with `NEKO_FAKE_RECORD` (what the fake CLI was given, including each `--mcp-config` file's content,
read at start because Neko removes the file when the run ends) and `NEKO_SECRETS_FILE` (a file store for plugin secrets, so no run touches
the login keychain; a release daemon ignores it). A scenario names the fake MCP server as `$fake_mcp`; the `add-plugin-folder` step sends
`add_plugin_folder` to the daemon (the app's folder panel is covered by the app's own tests).

`plugins` pastes an `mcpServers` block with the fake server and a secret into Settings ▸ Plugins (⌘, then the Plugins page), checks the
plugin waits and shows in Needs you (⌘2) as a plug with exactly what will run and the names it is handed, no value on screen, approves it,
Tests it (Connected: 3 tools), changes the secret (the new value reaches the test store and is never shown), runs a task whose fake CLI
calls `mcp__acme-docs__search` (the request in Needs you names the plugin; Allow; the step reads "acme-docs · search"), and checks the
recorded config had the server with the secret in `env`, that Claude Code was given `--mcp-config`, that no file is left in
`plugins-run/` after the run, and that no secret is anywhere in the data folder.

`plugins-changed` adds a plugin folder and approves it, runs a task that gets the server, then edits the folder's `neko-plugin.json` by hand
(`run` step): Needs you asks again showing the change, the next task does not get the server (its tool fails at once and nothing asks),
approving the new manifest brings it back and the third task's server has the changed argument.

## The kitten scenarios

All four start the daemon with `NEKO_FAKE_RECORD` and use the fake CLI's `[fake:delegate]` script: the steps after the tag, one line apart by ` ; `, are played through Neko's real MCP server (`start_subagent`, `poll N`), and each kitten's own words pick its script (`[fake:long:4]` streams four seconds, `[fake:slow]` runs until stopped).

`subagents-wake`: a thread starts two kittens and ends its turn. Two cards appear; the thread's row says two kittens work and that it is waiting; the conversation ends on "Waiting on Kitten 1 and Kitten 2" with a link each; the sidebar has one row. Both reports wake the thread once: two notes ("Woken by Kitten N · done"), final words reading both, the thread Done, no waiting line left. The record has exactly one `--resume` start and one message holding two `<neko-wake>` reports.

`subagents-stop`: two kittens that run until stopped. The composer of the waiting thread offers Stop; one press ends both kittens and the thread within seconds, nothing is waited on, an opened kitten says it stopped and has no Stop, and no resumed run was started. The rig's end check proves no process is left.

`subagents-restart`: one kitten that runs until stopped; the app quits, the daemon stops, both start again. The kitten reads Interrupted with its words kept, the thread is woken by "Woken by Kitten 1 · interrupted", ends Done, and the record's wake message says `status="interrupted"`.

`subagents-own-copy`: in a git project the thread starts two kittens with `placement: own_copy`; each card reads "Own copy · branch neko/…"; both write a file and commit, the thread is woken, merges both branches (the fake runs `NEKO_FAKE_WAKE_SH`, a squash merge for each branch a report names) and ends. Both branches and both copies exist, each branch has its own file, the merge is the thread's own run's change ("Changed 2 files"), the person's checkout is untouched (`git_same`), and Undo on the thread's run leaves every kitten branch's commit and both copies alone. `subagents-stacked`: Kitten 2 `after` 1, both in a copy of their own, Kitten 1 busy a few seconds; Kitten 2's card says Queued and "After Kitten 1" while Kitten 1 works, then it runs from Kitten 1's branch (it is a descendant, it holds `a.txt`). `subagents-drafts`: a draft's card has its prompt, Start and Discard and the thread does not wait; the prompt is replaced (click in the box, ⌘A, type), Start runs it, and the record shows the kitten read the edited words and never the original. `subagents-limits`: five kittens; three Working and two Queued, the row and the activity bar say "3 kittens working, 2 waiting their turn", then all five run and report. `subagents-depth`: three levels (1, 1.1, 1.1.1) nest in Overview; the record of what each run was offered (`tools LABEL` steps) shows `start_subagent` at depths 0 to 2 and not at 3, where the attempt fails with "No such tool available". The fake CLI's new steps (`write`, `sh`, `tools LABEL`, the `steps` tag, `NEKO_FAKE_WAKE_SH`) are in its header. Fake daemons in the rig start with `NEKO_HEAT=ok`.

`question-answered-by-parent`: Kitten 1 asks its parent with `ask_parent`; the thread is woken ("Kitten 1 has a question"), the fake parent answers with `message_subagent` (`NEKO_FAKE_ON_QUESTION=1=answer:…`), "The agent answered Kitten 1" shows, the kitten resumes and reports, the thread never needs the person; the record has one `kind="question"` wake and the kitten read the answer once. `question-to-person`: the fake parent passes the question up (`1=up`, `ask_person` naming the kitten); the thread needs the person and ends on "Kitten 1 asks (via the agent)", the card says "Asks you (via the agent): …", Needs you leads with the same words; the answer typed in Needs you resumes the thread, which passes it down, and the kitten reads it. `subagent-request`: asking before risky steps, Kitten 1's command goes straight to Needs you as "Kitten 1 asks" in the thread's title; the thread's tools (`tools d0`) hold nothing that answers a request; Allow once there lets the kitten go on and its report wakes the thread. The fake's `NEKO_FAKE_ON_QUESTION` is in its header.

`subagents-poll`: one kitten that runs until stopped, then twenty calls of the `subagents` tool. The record has twenty MCP calls, each under a second, so the tool never blocks; the thread ends its turn at once and waits; Stop ends it.

The band of a kitten is a row (key `band-title`, its name the whole breadcrumb), so selectors for it use `label` or `name`, not `text`; the thread's final words are matched on "I read 2" because Markdown isolates the "(s)".

## The composer scenarios

Headless, with `NEKO_FAKE_RECORD`; they use the fake CLI's `work` script (`work:SECS:step=MS`: a step per message, a user message that
arrives is folded into the running turn and read at the next step, as the real CLI does, so one result ends the run; an interrupt ends the turn) and check what reached the CLI by counting lines of the record
(`"stdin":{"message"` is a user message; `"subtype":"interrupt"` the interrupt; `resume` a resumed start).

`steer`: Return while the thread works. The row "Steered, waiting to be read: …" has no actions, goes when the next step reads it, the
agent's words say it read the message, and the record has two user messages and one start (one process took both). The fake folds the
steer into the running turn (one result for both), so the thread must end by itself, shortly after the work, and the record shows the CLI
was started with `--replay-user-messages`: a run waiting for a second result would sit at Working and fail the scenario's last wait.
`queue`: ⌥Return twice. Two Queued rows with Send now, Edit and Discard; the record still has only the first words; Discard drops the
second; when the turn ends the first starts a resumed run ("You said: Second message"), nothing waits, the discarded one never ran
(two user messages, one resume). `interrupt`: ⌘Return with new words. The CLI's interrupt request is in the record (once), the first
run folds as "Interrupted after 2s", the new words run at once and the thread ends done in the same conversation (one resume), long before
the job's 120 seconds. Typing the words first shows the mode chip and the key hints (`07-typed.png`).

The same three are checked without the app, against the real daemon, in `crates/neko-daemon/tests/composer.rs` (steer, queue order and
Discard, interrupt, a CLI that does not hear the interrupt, a second interrupt, Stop holding the queue, Send now, a restart, every mode
at rest).

## The New thread scenarios

Headless. Two driver steps serve them. `daemon` sends a request (JSON) straight to the daemon; `save_as` keeps a round or task id
usable later as `$name`, and `expect_error` expects the refusal. `choose-folder` with `"send": true` presses Return at once, before a
frame has heard the daemon, which is the folder race (`threads.md`).

`new-thread`: the start screen (headline, composer, Project chip), choosing a folder, a thread started in that project, then the
folder-race step: Add folder… and Return at once must start the thread in the new project, not in No project. `new-thread-background`:
⌘Return twice; the screen stays, the box is empty with the keyboard in it, "Started <title>" shows, and both threads run.
`new-thread-in-folder`: a git project shows the chips; In the folder makes no worktree and no `neko/` branch and leaves the checkout
unchanged. `no-cat`: a thread's recorded arguments have no `<neko-cat>` block (the subagent paragraph is still appended, so the
check is "no neko-cat", not "no `--append-system-prompt`").

Changed to the new truth: `cats.json` is cut to the cats' own lifecycle (rules saved, New cat…, Retire asks, Bring back);
`cat-rules-cannot-skip-checks` gets its miso task from a round run now, since nothing assigns a cat; `real-smoke` lost its cat steps
(never run it unasked: it spends money).

## The settle scenarios (S8)

Headless, in No project, threads made with the `daemon` step (`create_task`, kept with `save_as`). A thread that has said its words is not
yet a thread at rest (its run is still ending), so each scenario waits for "Worked for" or for the row to stop saying "working" before it
settles or archives: a thread that works is refused, correctly.

`settle-undo`: two finished threads; ⌘⇧S settles the open one: a toast (a polite `status` named Settled) with Undo, the thread out of the
list and still open, the group ending in a Settled fold that says 1 thread; ⌘Z takes it back (no fold, no toast, newest above the older);
settled again the toast goes by itself after five seconds and the fold stays; pressing the fold opens it in place; ⌘⇧S brings the thread
back with no toast. `auto-settle` starts the daemon with `NEKO_DEBUG_CLOCK=1`: an idle thread, a pinned one and one that ends with a
question; the clock moves on eight days (`clock`, 11520 minutes): the idle one is settled (fold of 1), the pinned one stays in Pinned, the
asking one stays listed and still needs you. `archive-wake`: Archive from a row's menu (toast, the row leaves, no fold of either kind);
View ▸ Show archived adds the Archived fold; it holds the thread; opening it and sending a message runs it again (a second "Worked for"),
the fold goes and the thread is a row of its group. Two driver facts: a checked menu item has the role `menuitemcheckbox`, and a
`create_task` thread's title includes its `[fake:…]` tag.

`pane-overview` waits, in its step 6, for Overview to say "5 files changed" with five files, not only for the run card's buttons: the card's
Review and Undo appear when the run's end checkpoint is written, a moment before Overview has asked again and been answered, and the old
`assert` straight after could see Overview as it was while the run was still writing (it failed once with 2 files, passed on rerun). The app
now asks for the overview again at that moment (`App::pane_run_changed`, when a run's undo is ready, not only when its status is done). The
check is the same, only waited for; 14 reruns, 8 of them with the machine loaded, passed. The race was not seen again, so the cause is the
one described, not one reproduced.

## How it drives and records

- **Headless (default).** `apps/neko/examples/neko-qa.rs` builds the real app (`neko_app::app()`: its view, state, keys, `poll`
  and link) in Rune's headless runtime, the same one the end-to-end test uses, connected to the daemon. Rune's own `--script`
  replays on a virtual clock and cannot wait for a daemon or a CLI, so the driver steps the scenario itself: each frame advances on
  the wall clock; a wait polls its condition every frame up to its deadline. Every frame is rendered on the GPU
  (`Renderer::render_runtime_to_offscreen`) and streamed into ffmpeg at 8 fps; pictures are 2x PNGs; trees are
  `Runtime::accessibility_json`. Clicks land on the centre of the element's visible part after it stops moving; keys go through
  `Runtime::key`. The prefs live in the run's folder (`RUNE_DIRS_ROOT`), so a quit and relaunch keeps folds and choices.
- **The folder panel.** It cannot open headless. Rune's headless runtime holds the app's own request for it and
  `Runtime::resolve_file_dialog` answers it, so "Add workspace…" and the Project chip's "Add folder…" run the app's own code with the path
  (`add-workspace`, `choose-folder`). In a window the panel would take focus, so those folders are added before launch with the
  daemon's `add_workspace` request, and a chosen one is written as the composer's last choice in the run's prefs.
- **`--window`.** `run.sh` builds a development bundle with `scripts/build-app.sh --dev "$NEKO_TARGET/qa-bundle"` (never the
  owner's `bundle-dev`) and renames it `Neko QA.app`, so its Dock item reads "Neko QA" and is found by that label (the owner's
  copies read "Neko" or "Neko Dev"). Only a bundle gets notifications, the Dock badge, the link and a proper Dock item. It is
  started with `open -n` and `--env` for `RUNE_DIRS_ROOT`, `NEKO_DATA_DIR` and `NEKO_NOTIFY_LOG` in the run's folder; its PID is
  found by the exact path of its executable and it is stopped by that PID. Only scenarios that need it (`"bundle": true`:
  notifications, the badge, links) use the bundle; `NEKO_QA_ALL_BUNDLE=1` forces it for every window run. The others run the bare
  binary, hard-linked as `neko-qa-win` (its own Dock item), because under a window script the bundle's accessibility tree read
  empty through System Events even with the window active (2026-11-26, cause not found). The owner's copy and data are never
  touched.
- **Active, not inactive.** Window runs open the window active (it takes focus while the run lasts), because that is how a person,
  and VoiceOver, uses it: an inactive window's accessibility tree does not follow its changes (below). Only a scenario that needs
  Neko not in front (`"inactive": true`, the notifications scenario) opens it with `RUNE_OPEN_INACTIVE` and `open -g`, and checks
  it took no focus; that scenario answers by keyboard, never by reading the tree. The
  scenario becomes a `--window-script` that Rune replays on the wall clock (typing, keys, taps on fixed keys). Waits become pauses.
  Script marks tell the driver to capture the window (`screencapture -l`, that window only, even behind others), to check an
  expectation by what the window says to assistive technology (`scripts/qa/ax-texts.js`), or to press a control whose key is made
  at run time by its accessible name (`scripts/qa/ax-press.js`, AXPress: no pointer, no focus). The video is the window's captures
  at 2 fps, never a screen rectangle. At the end the Dock menu is read once (`dock-menu.applescript`) and must have Quit and not
  "Application Not Responding"; the launch time and Rune's first-frame time go in the report.
- **CLI.** `--cli fake` runs `neko-fake-claude` with a HOME of the run's own and a short PATH, free and deterministic. `--cli real`
  runs the installed `claude` with the person's environment minus `CLAUDE*`, `NEKO_*` and `ANTHROPIC_API_KEY`, and
  `NEKO_CLAUDE_MODEL` (`NEKO_QA_MODEL`, default `haiku`) so it asks for the cheapest model; tokens and the reported cost go in
  `result.json`. Real runs may touch only the scenario's temporary folder. Claude Code still keeps its own session files under the
  person's `~/.claude`, as any run does.

- **Notifications and the badge.** With `NEKO_NOTIFY_LOG` set, the app appends one JSON line per notification it posts or takes
  back (title, body, buttons, whether the system took it) and per Dock badge count (`notify::log_event`). Window checks `notified`,
  `badge`, `withdrawn` and `posts` read that file; the system's notifications are never read. A post the system refused while it
  asks the person for permission is marked "needs the person once", not a failure. `open-link` opens `neko://task/<id>` with
  `open -a <the QA bundle>`, so no other Neko on the Mac takes it (this brings Neko QA to the front); `cold-link` quits the app and
  starts it from the link. The `notifications` scenario is window-only (`"headless": false`) and posts at most four.
- **Needs you.** `stale-answer` sends a second answer to an answered question straight to the daemon (`follow_up` naming the run)
  and expects it refused.

## Scenarios

`apps/neko/qa/*.json`, one per journey: `name`, `title`, `about`, `cli`, `window`, `size`, `setup` (folders, optionally git; the
fake's Claude Code settings; daemon environment) and `steps`. A step is an action (`click` on a selector, `type`, `key`
(`cmd+n`, `shift+tab`), `sleep`, `snap`, `choose-folder`, `add-workspace`, `tab-to` (Tab until a control has focus, checking a
focus ring at every stop), `allow-until`, `quit-app`, `start-app`, `stop-daemon`, `start-daemon`, `remove`, `git-record`, `run` (a command in the run's folder: an edit by hand), `stale-answer`, `clock`, `daemon`) or a
check (`wait` with a deadline, `assert`). A step with `expect` is an expectation in the report; `soft` records a failure and goes
on; `window: false` leaves a step to headless. Selectors match an element's `key` (with `*`), `text`, `label`, `label_is`, `name`
(text, label or description), `role`, `focused`, `in` (an ancestor). Checks: `see`, `gone`, `count`, `order`, `focus` (with a drawn ring
unless `ring: false`), `title`, `daemon`, `file`, `no_file`, `sh`, `git_same`, `all`, `any`.

## House (the app's design)

The scenarios run unchanged on House except one expectation: the run card is no longer titled "Changes", it reads "Changed 2
files" (`git-worktree`, "The run card shows the changes"). The band still has the workspace's name (`task-workspace`) and the
folder line, which is quieter and drops its branch under 900 points. The rig found one crash House had introduced (the old cat
picker's stack underflowed with no cats yet); the picker is gone now.

Defect 10 (opening Review cross-faded the conversation and Review, both drawn at once) is checked by `apps/neko/tests/page_swap.rs`,
which takes frames while the swap moves, not by a rig step: the rig's pictures are taken after things settle, so they cannot see it.

## What it cannot do (and why)

- Rune's `--script` and `--window-script` replay on a clock: they cannot wait for an event. Headless runs use the driver; window
  runs use pauses, so a window run is slower and its checks happen when the pause ends.
- A real window does not expose element keys. Window checks use the words the window gives to assistive technology; a check that
  names only keys is recorded as "picture only". The row descriptions (a task's state words) are not visible this way.
- Video of a real window is made from window captures at 2 fps, not a screen recording, so nothing around the window is filmed.
- The folder panel is never shown (see above). Real-window clicks on fixed keys go through Rune's script input, not the system's
  pointer: the system's event path is not exercised.
- Reading the window's accessibility tree takes seconds, so window runs leave long pauses after presses and checks.
- A real window run cannot click the system's notification permission prompt: it records it and needs the person once.

## Finding: the real window's accessibility tree goes stale (Rune at 53f6d36)

Measured on 2026-11-26 with the bare binary, a temporary data folder, a window script that types a task at about 10 s, and
`RUNE_A11Y_LOG=1` (Rune logs each activation and each tree update it sends):

| Window | Assistive client first asks | What happened |
| --- | --- | --- |
| inactive (`RUNE_OPEN_INACTIVE`, behind the frontmost app) | at 6 s, after the app connected | Rune sent 1 to 3 nodes every frame and never the task opening (dozens of nodes); the tree stayed on Home while the window drew the task |
| inactive | at 3 s, before the app connected | the window stopped advancing: still "Starting Neko…", the script's typing not shown |
| active | at 3 s | after one task opened the tree followed: the task's controls were there |
| inactive | at 12 s, nothing changed after | the tree was right (it was built at activation) |

And in the full window runs with the window **active** (2026-11-26 17:00 UTC): first-task, git-worktree and context passed, but
ask-mode (the second request's buttons, after the conversation had scrolled), workspaces (the sidebar still said "No tasks yet"
after three tasks) and workspace-picker (an open menu's items) read a stale tree too. So it is not only inactive windows: after a
while, structural changes stop reaching the tree in an active window as well.

Neko's code is not involved as far as we can tell: it never touches the tree root, and the same build behaves differently by window
state and by how much has changed. In Rune, updates go out only from `a11y_turn` at the end of a turn;
`RuntimeCore::a11y_update` returns nothing while `paint.drawn.len() != tree.nodes.len()`, and takes a "scrolled only" shortcut
when its stamp says only scroll offsets and text fills moved. An inactive window that commits without painting every node (Rune's
covered path, `covered_frame`, lands frames without painting and marks live content not visible) never sends its structural
changes; content that grows inside a scroll view pinned to its bottom may be taken for a scroll. Those are the likely causes; we did
not patch Rune to prove them.

**Real VoiceOver users** are likely affected: in an active window, after enough change (a long conversation, more tasks), VoiceOver
would read controls that are gone and miss new ones (a permission request's answers, new sidebar rows, menu items). This is not
fixed: it needs Rune.

**The rig** opens window runs active (above), which is how people use the window, and keeps the failures it sees as evidence.
**Upstream report (not filed):** "rune 53f6d36, macOS (AccessKit SubclassingAdapter): once an assistive client has activated the
adapter, structural changes to the view stop being sent. With a window opened inactive (`RUNE_OPEN_INACTIVE`) behind another app,
`RUNE_A11Y_LOG` shows only 1-3 node updates per frame and the tree never shows the new screen; if activation happens before the
app's first data arrives, the window stops advancing until it is activated. In an active window it happens later: new rows in a list,
a permission card added to a conversation that sticks to its bottom, an open dropdown menu's items, are missing from the tree while
drawn. Expected: the tree follows the view in every window state. Suspected: `a11y_update`'s early return when `paint.drawn` and
`tree.nodes` differ (frames that commit without painting), and the `scrolled_only` shortcut taking growth in a stick-to-bottom
scroll view for a scroll." Narrowest workaround for Neko: none in the app at this rev (no public way to force a full tree).

## The right pane scenarios (S3)

Headless, 1280 by 800 (the pane open) unless said. `pane-overview` makes a run change 5 files in a git project and checks Overview's
Changes (figures, five files, folder, branch, usage), the one change line and no receipt card or band folder line, then Review: the
Changes tab with This run selected, All runs, Uncommitted, and back to Overview. `pane-undo` is `review-undo`'s undo, redo and
follow-up checks with the buttons in Overview. `pane-subagent-tab` runs a kitten that asks for a command in ask mode: Overview lists
it and its request, Open puts it in a tab with its Stop and its request, the request is answered there, its report comes first and wakes the thread,
and Open in main view reaches the breadcrumb. `pane-narrow` runs at 1000 by 700: the pane is shut with its button in the band, the button opens the sheet,
Escape closes it, and the band keeps the title's 140 points throughout. `review-undo`, `undo-plain-folder`, `git-worktree`,
`subagents-restart` and `subagents-stop` moved to the Changes tab and the tab (the old checks are unchanged). The driver learned two
selectors for these: `"selected": true` (a chosen item: `focus_selected`) and `"min_w": 140` (at least this wide).

## The search scenarios (S9)

Headless, against the real daemon. The palette's rows have the role `option` to the driver (`list-box-option` in the accessibility tree), and the
selector `"in": {"key": "palette"}` keeps a check to the palette's own rows (the sidebar has rows of the same names).

`search-words`: two threads finish (the fake `plain` script says "Hello from the fake Claude Code. Nothing was changed."). ⌘K lists both under
Recent with the actions after; typing "nothing" (a word only the agent said) lists exactly the two, each row showing the words around the match;
Down and Return open the older one and close the palette; ⌘K and "garden" find one by its title and leave the other out; Escape closes.
`search-filters`: a thread finishes in acme-api, one in mochi-notes ends with a question; `status:needs`, `project:acme`, `project:mochi
status:needs` keep what they say, and `status:sleepy` says it is not a status. `search-new-thread`: with a project added, "tidy the garden shed
[fake:plain] project:acme" and Tab: the palette closes, New thread says "in acme-api" and its composer holds the words without the filter;
Return starts the thread and it runs in acme-api. Not checked by the rig: the scroll to a found message in a long conversation (a thread
the fake makes is one screen; `apps/neko/tests/search.rs` checks that the reveal is asked for and waits for the transcript).

## The Cattain scenarios (S10)

Headless, 1300 × 860 so the pane is docked and open, with `NEKO_FAKE_RECORD` (the fake CLI records its arguments, the folder it started in
and stdin).

`cattain-thread`: Settings ▸ Cattains opens miso's own thread, which opens on who it is and is not a row under Threads. RULES.md is written
in the Files tab (from the Overview's file chip) and saved, and is on disk in `$data/cattains/cat_miso/`. A message to miso
(`[fake:cattain-notes]`, which writes NOTES.md and DIGEST.md) runs in miso's folder, its system text holds RULES.md in a `<neko-cattain>`
block, its turn is led by miso's face, and the Digest tab shows what it wrote.

`cattain-files`: mochi's RULES.md and CREW.md are saved; New thread in mochi's crew (Overview) opens New thread with the crew chip; the
thread's band says "In mochi's crew"; its system text holds CREW.md in a `<neko-crew>` block and nothing of RULES.md; its turns have no face.

`cattain-migrate`: the app quits and the daemon stops; `run` replaces the store with one as Neko left it before Cattains (`$daemon --data-dir
$data --make-store-at 16`, a debug-only flag that makes the store migrated no further and stops; then three cats with rules and two old
threads written with `/usr/bin/sqlite3`); both start again. The old threads are untouched and no thread was made for a Cattain nobody
opened; mochi's RULES.md and CREW.md hold its old rules, miso's RULES.md is on disk and keeps its previous version in the store; Settings ▸
Cattains lists all three; the old thread that had mochi is in mochi's crew. A scenario names the daemon as `$daemon`.

Changed to the new truth: `cats` makes and retires Cattains in Settings ▸ Cattains and the Profile, and writes RULES.md in the Files tab;
`cat-rules-cannot-skip-checks` writes miso's CREW.md (a round's thread is in miso's crew); `no-cat` saves miso's files with
`save_cattain_file` and checks no Cattain's block reaches a thread with no cat.

## A Cattain runs its crew (S12)

Headless, 1300 × 860, with `NEKO_FAKE_RECORD` and a short crew batch (`NEKO_CREW_BATCH_MS`). The fake CLI's `[fake:mcp]` script calls
`start_crew_thread` and `message_crew` through Neko's real MCP server; a fake woken with `<neko-wake kind="crew">` calls `crew` and
rewrites DIGEST.md and NOTES.md in its folder (the record keeps `{"digest": …}`).

`crew-three`: in miso's own thread the person asks for three flaky tests, one thread each; the fake starts three crew threads
(`[fake:work:6]` to `:8`). Three rows nest under miso in the sidebar (`cattain-thread-*`), three list in its Overview, and miso's thread
ends on "Waiting on its crew"; the store has three tasks with `started_by = 'cat_miso'`. As they end, three "Woken by Fix test …" notes
appear, DIGEST.md says "Fix test three: done", the record has at most two crew wake messages, and the Digest tab shows the digest with its
age.

`crew-needs-you`: in "Ask before risky steps" miso starts two crew threads, the second `[fake:hook-shell]`; miso's script stays in its turn
(`stay 8`) so the request's news is steered into it. miso's sidebar row carries the amber count 1 (`cat-cat_miso-count`), its Overview has
Needs you above Crew, the note "Run the tests needs you" appears, the record has exactly one `needs_you` crew wake and the permission is
still pending; the person's Allow once in Needs you lets the crew thread finish, the count goes, and miso hears that turn end too.

`crew-merge-only-when-asked`: miso starts one crew thread, which ends; miso is woken and rewrites its digest, and the record has no
`<neko-from>` message yet. The person writes "Merge it" in miso's thread and the fake passes it on with `message_crew` (by the thread's
title); by the record's order miso's `<neko-from>` words reach the crew thread only after the person's message. The crew thread shows
"Started by miso" and both miso's brief and miso's message led by "From miso".

## Making a Cattain by asking (S11)

Headless, 1300 × 860, with `NEKO_FAKE_RECORD`. The fake CLI's `[fake:mcp]` script calls `list_cattains` and `make_cattain` through Neko's
real MCP server, as it does the kitten tools; the record holds each call's answer (`mcp_call`, with `error`).

`make-cattain`: the person asks a new thread for a Cattain for the iOS work. The note "Made a Cattain: yuzu · iOS work" (a `note`) with Open
and the toast "Made yuzu" (a `status`) with Undo appear; the record has "3 of at most 16 Cattains are at work" and "Made a Cattain: yuzu (iOS
work)"; exactly one RULES.md under `$data/cattains/` holds the seeded line. Open (`cattain-made-*-open`) opens yuzu's own thread on who it is
(its name in the band, "iOS work" as the band's purpose, the intro). Settings ▸ Cattains shows "made in" on yuzu's line and "Agents may make
Cattains".

`make-cattain-undo`: the toast's Undo retires yuzu: the note reads "… · retired", the toast is gone, the store has yuzu retired
(`/usr/bin/sqlite3`), and Settings ▸ Cattains offers Bring back.

`make-cattain-limit`: thirteen `create_cat` requests (the `daemon` step) make sixteen at work. The agent's `make_cattain` is refused: the
step reads "Could not make a Cattain · iOS work" once the fold is open, there is no note and no toast, the record has the error with "at
most 16 Cattains at work, and 16 are", and the store still has sixteen active.

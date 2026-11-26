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

`briefing` makes a round (every day, a minute from now, so the daemon is started with `TZ=UTC` and the step's words say `{{utc+1}}`, the UTC time N minutes from now), a task that finishes and one that waits on a request (ask mode), waits for the round to start by itself, opens the Briefing (⌘4) and checks its sections and the first sentence of each answer, answers the request from the page, marks it read with ⌘↩, checks "You're caught up" and that Home's line is gone, then finishes a new task and checks the line comes back. `briefing-morning` sets the morning time three hours ahead in Settings ▸ Briefing, moves the scheduler's clock past it (`clock`) and checks one notification, "Your briefing: 1 finished, 1 needs you", in the notification log, none more when the clock moves on the same day, and the same counts on the page. Headless runs have no system to post to: the driver gives the app a poster that writes to the run's `notify.jsonl` in the form the window runs' log has, so `notified`, `withdrawn` and `posts` (`{max, titled}`: only the posts whose title starts so) are checked the same way in both.

## How it drives and records

- **Headless (default).** `apps/neko/examples/neko-qa.rs` builds the real app (`neko_app::app()`: its view, state, keys, `poll`
  and link) in Rune's headless runtime, the same one the end-to-end test uses, connected to the daemon. Rune's own `--script`
  replays on a virtual clock and cannot wait for a daemon or a CLI, so the driver steps the scenario itself: each frame advances on
  the wall clock; a wait polls its condition every frame up to its deadline. Every frame is rendered on the GPU
  (`Renderer::render_runtime_to_offscreen`) and streamed into ffmpeg at 8 fps; pictures are 2x PNGs; trees are
  `Runtime::accessibility_json`. Clicks land on the centre of the element's visible part after it stops moving; keys go through
  `Runtime::key`. The prefs live in the run's folder (`RUNE_DIRS_ROOT`), so a quit and relaunch keeps folds and choices.
- **The folder panel.** It cannot open headless. Rune's headless runtime holds the app's own request for it and
  `Runtime::resolve_file_dialog` answers it, so "Add workspace…" and the picker's "Add folder…" run the app's own code with the path
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
focus ring at every stop), `allow-until`, `quit-app`, `start-app`, `stop-daemon`, `start-daemon`, `remove`, `git-record`, `run` (a command in the run's folder: an edit by hand), `stale-answer`, `clock`) or a
check (`wait` with a deadline, `assert`). A step with `expect` is an expectation in the report; `soft` records a failure and goes
on; `window: false` leaves a step to headless. Selectors match an element's `key` (with `*`), `text`, `label`, `label_is`, `name`
(text, label or description), `role`, `focused`, `in` (an ancestor). Checks: `see`, `gone`, `count`, `focus` (with a drawn ring
unless `ring: false`), `title`, `daemon`, `file`, `no_file`, `sh`, `git_same`, `all`, `any`.

## House (the app's design)

The scenarios run unchanged on House except one expectation: the run card is no longer titled "Changes", it reads "miso changed 2
files" (`git-worktree`, "The run card shows the changes"). The band still has the workspace's name (`task-workspace`) and the
folder line, which is quieter and drops its branch under 900 points. The rig found one crash House had introduced (Home's old cat
picker stack underflowed with no cats yet), now gone with the picker's rewrite.

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

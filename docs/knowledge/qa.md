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
- **`--window`.** The app binary is hard-linked as `neko-qa-win` (its own Dock item) and launched with `RUNE_OPEN_INACTIVE` (no
  focus taken; checked), `RUNE_DIRS_ROOT` and `NEKO_DATA_DIR` in the run's folder; the owner's copy and data are never touched. The
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

## Scenarios

`apps/neko/qa/*.json`, one per journey: `name`, `title`, `about`, `cli`, `window`, `size`, `setup` (folders, optionally git; the
fake's Claude Code settings; daemon environment) and `steps`. A step is an action (`click` on a selector, `type`, `key`
(`cmd+n`, `shift+tab`), `sleep`, `snap`, `choose-folder`, `add-workspace`, `tab-to` (Tab until a control has focus, checking a
focus ring at every stop), `allow-until`, `quit-app`, `start-app`, `stop-daemon`, `start-daemon`, `remove`, `git-record`) or a
check (`wait` with a deadline, `assert`). A step with `expect` is an expectation in the report; `soft` records a failure and goes
on; `window: false` leaves a step to headless. Selectors match an element's `key` (with `*`), `text`, `label`, `label_is`, `name`
(text, label or description), `role`, `focused`, `in` (an ancestor). Checks: `see`, `gone`, `count`, `focus` (with a drawn ring
unless `ring: false`), `title`, `daemon`, `file`, `no_file`, `sh`, `git_same`, `all`, `any`.

## What it cannot do (and why)

- Rune's `--script` and `--window-script` replay on a clock: they cannot wait for an event. Headless runs use the driver; window
  runs use pauses, so a window run is slower and its checks happen when the pause ends.
- A real window does not expose element keys. Window checks use the words the window gives to assistive technology; a check that
  names only keys is recorded as "picture only". The row descriptions (a task's state words) are not visible this way.
- Video of a real window is made from window captures at 2 fps, not a screen recording, so nothing around the window is filmed.
- The folder panel is never shown (see above). Real-window clicks on fixed keys go through Rune's script input, not the system's
  pointer: the system's event path is not exercised.
- Reading the window's accessibility tree takes seconds, so window runs leave long pauses after presses and checks.

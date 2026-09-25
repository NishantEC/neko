# Six-step native setup

The old permission-only arc is replaced by six visible steps: welcome, Mac,
import, tools, responsibility and first look. Permission prompts remain native
Mac substates. Source design: `docs/design/onboarding-{1..6}.jsx`.

## Implemented boundaries

- Completion closes the setup window only after a positive daemon response;
  native window closure clears its shared slot without marking completion.
- Shortcut capture is serialized through persistence, cancellation invalidates
  in-flight conflict lookup, and Escape releases only capture-owned busy state.
  Tab and Shift-Tab traverse controls outside recording.
- Import selections, credential consent and process trust are explicit. Changing
  selections or rescanning clears consent. Imported schedules remain paused.
- Tool definitions may be global, while grants are per workspace. Skills remain
  reviewed, hash-pinned instructions rather than implicit permission grants.
- The first brief is an actual scoped chat turn. Selecting another workspace
  clears the displayed brief; render also independently checks its workspace.
- Schedule editing preserves an existing workspace binding regardless of later
  sidebar selection. New/unbound drafts use the selected workspace.

## Verification

351 app tests passed in the final serial suite, including 12 onboarding tests,
the schedule scope regression and actual GPUI chat/memory command routing.
Independent spec and quality reviews found
and closed the shortcut cancellation, focus traversal, import-lock and scope
findings. Import/schedule IPC smoke evidence is recorded separately.

Real native windows were launched with an isolated `NEKO_DATA_DIR`,
`NEKO_SHOW_ON_LAUNCH=1`, `NEKO_SHOW_SETUP=1` and one-based `NEKO_SETUP_STEP`.
Startup logs confirmed Welcome and Import respectively, both `key=false`.
Window-only captures:

- `/tmp/neko-setup-final-welcome.png` (window 35369; heading line-height tightened afterward)
- `/tmp/neko-setup-final-import.png` (window 35418)

These captures prove native rendering, not clicks/typing. No keyboard focus was
taken, no real clipboard/Accessibility settings were changed, no production
credentials were imported, and the installed application was not replaced.
The proposed under-three-minute first-run target has not been timed with a user.

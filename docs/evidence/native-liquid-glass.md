# Native Liquid Glass redesign — 2026-10-05

The preceding craft pass was too dense and flat. This revision follows Paper
21A's open structure and supports both system appearances, using 21B as the
dark reference. The original 20A artboard remains intact.

## Implementation

- Native source-list sidebar, toolbar, safe-area rules and stable split-pane
  sizing are preserved. No replacement window or inspector was introduced.
- Shared adaptive colors, 15pt reading text, 28pt introductions, 32pt page insets,
  28pt section gaps and wider reading lanes replace the forced dark palette.
- The shared Home/agent composer uses `glassEffect` on macOS 26, with a material
  fallback on older supported systems. Inner controls do not add another glass
  layer. Glass receives no ancestor opacity. Content cards remain opaque.
- Appearance is a local preference; changing it does not reset conversation
  identity, editor state, pending submissions or authority.
- Home, agent conversations, agent list/board and management surfaces use the
  same spacing and hierarchy. Saved management text stays inert and verbatim.

## Baseline

- Source baseline: `a9d2a34`; installed code before this change: `e6fbc50`.
- Baseline and first integration run: 98 XCTest cases and 11 Swift Testing tests
  passed. The optional real-daemon test was skipped without `NEKO_TEST_DAEMON`.
- Logs: `/tmp/neko-liquid-baseline-swift.log`, `/tmp/neko-liquid-swift.log`.

## Verification boundary

Paper screenshots verify spacing, type hierarchy, contrast, alignment and
artboard fit. They approximate glass and do not prove native material rendering.
Installed-app verification is recorded below after building and installing.
No daemon logic, execution authority, reset or worktree lifecycle was changed.

## Installed build and final checks

- Main implementation: `2139612`. Final UI refinements and installed source:
  `69a5a90d875e38951b6dc8fb91e8accf5ec1ae92`.
- Final native suite: 98 XCTest cases and 11 Swift Testing tests passed. The
  optional real-daemon test remained skipped without `NEKO_TEST_DAEMON`.
  `git diff --check` passed. No Rust source changed in this revision.
- The signed release build completed in 41.74 seconds. The canonical
  `/Applications/Neko.app` passed `codesign --verify --deep --strict`; its
  client and daemon binaries exactly matched the build output. Its
  `NekoGitCommit` matches the final UI source above.
- Immediately before installation: 69 tickets, zero Planning/Building/Reviewing
  tickets, zero Queued tickets, and zero pending/queued Home messages.
  The installer preserved data and credentials. Final process inspection found
  exactly one client and one daemon, both under `/Applications/Neko.app`.
- Logs: `/tmp/neko-liquid-swift-final.log`, `/tmp/neko-liquid-build.log`, and
  `/tmp/neko-liquid-install.log`. These are local temporary logs, not committed
  evidence artifacts. Installation reported Launch Services unregister warnings
  for old backup bundles but completed successfully.

## Live screen observations

Native accessibility actions and app screenshots were checked on macOS 26.5.1.
Screenshots were reviewed in the chat; no screenshot files are claimed here.

- Home was checked in light and dark appearance. A temporary unsent draft
  survived appearance changes and navigation; it was then removed. The composer
  was empty with Send disabled at handoff. Nothing was submitted during QA.
- A completed agent conversation showed its result, independent review,
  evidence disclosure and reply composer. Its native Back action worked.
  All agents List/Board switching, populated rows, search empty state and Clear
  search were checked. Final list notes retain colored state icons while using
  neutral text. Search was cleared and the original Board preference restored.
- Tools connections, Skills workspace guidance, Watching, Memory/Working style,
  Profiles, Workspaces, Schedules, and Settings General/About were inspected.
  The manual connection sheet was opened without entering or saving data. The
  final build keeps Add connection in a fixed visible footer; validation still
  disables it for an empty form. The sheet was closed afterward.
- System/Light/Dark controls worked from Settings and the sidebar menu. System
  appearance was restored and confirmed in saved preferences. Home was left open.
- Paper variants 21A Daylight (`7X3-0`) and 21B Evening (`835-0`) remain in file
  `01M3YBQPKND2A0GY02E3Z13H1G`, alongside preserved 20A (`7ID-0`). The app follows
  21A's open structure with adaptive appearance; Paper only approximates glass.

## Remaining verification limits

Physical divider dragging and narrow-window resizing were not exercised. The
macOS 14/15 material fallback and Reduce Transparency/Increase Contrast settings
were not checked on screen. Onboarding was compiled but not entered against
live data. Live model execution and compaction behavior were not changed or
retested. Reset and worktree cleanup remain separate work; existing tickets and
worktrees were preserved. Commits are local to main; no remote push was made.

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

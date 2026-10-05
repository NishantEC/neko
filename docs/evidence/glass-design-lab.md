# Native Liquid Glass design lab

Verified on 2026-10-05 in `/Applications/Neko.app`, built from `a806a71` on local
`main`. Open **Design lab** in the native sidebar, then choose **Inline** or
**Context** in the toolbar. This is an interactive SwiftUI/AppKit preview.

## Design and implementation

Both layouts put agents and Plugins inside one glass composer. Inline keeps a
compact agent group in the bottom row; Context exposes each agent and its status
above the editor. The system sidebar, native popovers/sheets and existing AppKit
text editor are reused. The beads and send control use static shading. The glass
surface uses the existing macOS 26 availability-gated `glassEffect` helper, with
the existing material fallback on earlier macOS versions.

The preview model has no AppModel, daemon IPC or provider dependency. Messages,
plugin selection and sample agents are local preview state. The sample plugin
library does not install or execute plugins. Existing editor paste/drop handling
can still import real attachments into the shared local attachment store; this
page does not claim filesystem isolation. Production composers are unchanged.

## Actual installed-app screenshots

| Layout or state | Screenshot |
| --- | --- |
| Inline, dark | [Open](glass-design-lab/inline-dark.png) |
| Context, dark | [Open](glass-design-lab/context-dark.png) |
| Inline, light | [Open](glass-design-lab/inline-light.png) |
| Context, light | [Open](glass-design-lab/context-light.png) |
| Native agent popover | [Open](glass-design-lab/agents-dark.png) |
| Sent message above composer | [Open](glass-design-lab/send-visible-dark.png) |

Captured from the installed window on real Mac hardware running macOS 26.5.1
(25F80), at 1280 × 820 points / 2560 × 1640 pixels. Reduce Transparency,
Increase Contrast and Reduce Motion were off. No explicit glass-intensity
override was observed. The material appearance depends on OS, backdrop and
accessibility settings; these images are evidence for this environment.

## Verified interactions

1. Open both layouts using the native toolbar. Typed draft survives switching.
2. Open the agent popover and Colours detail sheet, then return with Done.
3. Open Plugins, toggle Repository tools off/on and observe the count change
   from two to one and back. Open the sample library sheet. Escape dismisses
   the popover.
4. Type two lines using Shift-Return, then send using Return. The editor clears,
   Send disables and the submitted message stays fully above the glass overlay.
   Live testing found the original end anchor ignored overlay clearance; the
   installed correction includes that clearance in the scroll target.
5. Reset removes submitted sample text, restores draft/attachment/plugins,
   scrolls to the top and keeps the selected layout. Test text was cleared.

Both composer layouts were visually inspected in light and dark appearance.
Live app appearance switching exposed stale mixed styling in native toolbar and
popover surfaces, particularly when returning from Light to System. A client
relaunch restored coherent dark rendering; the original System preference was
restored and the daemon PID remained unchanged. This appearance-switching issue
is not fixed by the design lab. The light captures retain the observed darker
toolbar controls. The existing crowded sidebar footer is also visible.

## Validation and limits

`swift test` passed before the preview and after the scroll correction: 105
XCTest tests plus 11 Swift Testing tests. Release build and deep strict code-sign
verification passed. Rust code was unchanged; the Rust suites were not rerun.
Zero tickets were Planning, Building or Reviewing immediately before each
installation. The final process check found one installed Neko app and one
daemon. Changes were committed locally; no remote push was performed.

No live provider/plugin execution, automated latency measurement, narrow-window
layout, VoiceOver session, accessibility-setting variants or pre-macOS-26
fallback rendering was verified. This preview proves native rendering and the
listed local interactions, not a completed plugin host or production rollout.

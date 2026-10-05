# Inline task control and Metal pearl revision

2026-10-06. Installed source commit: `577a4c0` on local `main`.

## What the count means

The old “3 agents” was fixed illustration text. The revision shows **Neko** for
an unsplit task, and **N subtasks** when approved child records exist. The native
preview computes membership from the actual protocol snapshot shape, using
distinct child ticket IDs for this parent only. Unapproved proposals, missing
tasks, duplicates and the owner do not add to that count. Activity counts derive
from task statuses, not measured process liveness. Planner/builder/reviewer are
phase roles, not three additional children. The existing split proposal can
produce two or three subtasks; this is not a global worker cap.

The Design lab scenario menu supplies One task, Split task and Review sample
snapshots. It does not connect this control to live tasks or start workers.
Production chat and plugin behavior remain unchanged.

## Rendering

`pearlOrb` is a stitchable Metal color shader in the installed `Neko.metallib`.
It models a softly lit opaque nacre body with broad highlights and restrained
colour bands. It replaces the dark metallic radial gradient. Pointer uniforms
move the light across the bead; the whole button supplies hover/press feedback.
These uniforms are driven by events, with no TimelineView, timer or idle loop.
The shader affects only decorative beads; the composer retains system glass.
Reduce Motion disables pointer light movement and scale animation. Reduce
Transparency or Increased Contrast selects a solid-colour fallback. These
fallback branches were inspected in source, not exercised through system settings.

## Evidence

| Native app capture | Result |
| --- | --- |
| [Inline, one task](pearl-task-control/inline-single.png) | Neko label and new pearl material |
| [Inline, split task](pearl-task-control/inline-split.png) | Two child beads and derived “2 subtasks” label |
| [Waiting subtask](pearl-task-control/waiting-subtask.png) | Native detail sheet explains the dependency |

The installed app's accessibility state confirmed all three scenarios:

- One task: `Preview task team, Neko, Building`.
- Split task: `Preview task team, 2 subtasks, 1 working · 1 waiting`.
- Review: `Preview task team, Neko, In review`.

The task popover exposed a separate owner row and both child rows; selecting
the waiting child opened the captured sheet, and Done returned to the composer.
The popover itself was not visible in the returned window capture, so its
visual placement is not verified. Physical pointer calls failed with
`Computer Use server error -10005: noWindowsAvailable`; user input was requested
to confirm an unlocked desktop. **Hover movement, press animation and their
rendering on screen remain unverified.** No hover screenshot or performance
claim is presented. The app was left on Inline / One task with the sample draft.

## Build and tests

Baseline: 105 XCTest + 11 Swift Testing tests passed. After changes: 109 XCTest
+ 11 Swift Testing tests passed. Four new focused tests cover phase-vs-task
identity, duplicate/missing/unrelated children, proposals that do not create
workers, and working versus waiting/settled statuses.

The Metal compiler succeeded. Loading the installed library through Metal
confirmed its exported `pearlOrb` function. Release build and deep strict signing
verification passed. No Rust behavior changed; Rust suites were not rerun.
Installation ran only after a live socket snapshot showed zero tickets in
Planning, Building or Reviewing. No remote push was performed. Light appearance,
narrow windows, VoiceOver and GPU/typing-latency profiling were not repeated for
this revision. The prior appearance-switching issue remains outside this change.

# Native toolbar hit testing — 2026-10-05

Page titles and primary actions now occupy the native macOS toolbar. Content
respects its safe area, so the top row remains useful and receives mouse input.
The shared detail viewport clips scrolling content below the toolbar.

## Reproduction and cause

On the previous installed build, coordinate mouse clicks on All agents' List
control and an agent's Back control had no effect. Accessibility activation
worked. Custom page headers were drawn underneath the transparent native
toolbar by top-safe-area overrides; hiding its background did not remove its
hit region.

## Verification

Verified on the signed release build installed at `/Applications/Neko.app`,
using CUA screenshots and coordinate mouse clicks. Screenshots are recorded in
the task's tool results. No task or configuration was saved during these checks.

| Surface | Observed result |
| --- | --- |
| All agents | Mouse clicks switch List/Board with the sidebar expanded and collapsed. The original top row contains the title, search and controls. |
| Search | Mouse focus and typing `StreamChat` filtered Needs you from 67 to 8. Clearing restored the list. |
| Work options | Mouse click opens the options menu; dismissed without changing options. |
| Sidebar | Mouse clicks collapse and restore the native sidebar. |
| Agent conversation | Mouse Back and actions menu work; Back still works after scrolling. Long titles truncate without covering actions. |
| Agent actions | Screenshot and accessibility tree show separate Accept locally, Stop and menu controls; no grouped duplicate labels. |
| Home | Screenshot shows native title/subtitle and transcript below the top row. |
| Workspaces, Memory, Profiles | Native top-right plus buttons open the matching sheets by mouse; cancelled without saving. |
| Settings | Mouse clicks select all seven tabs. Title occupies the native top row. |
| Tools & skills | Mouse clicks switch Connections/Skills and open Add connection. Menu dismissed without adding a connection. |
| Watching, Schedules | Screenshots confirm native titles and unobscured content below them. |

The computer-use tool returned `noWindowsAvailable` for coordinate actions on
one long, quoted agent window title. The same build's shorter-title agent
passed mouse Back/menu checks. The long-title agent was verified visually and
through accessibility; do not claim its coordinate mouse checks passed.

## Build and review

- `swift test --package-path native/NekoKit`: 77 XCTest tests and 11 Swift
  Testing tests passed before and after the change.
- Signed release build passed; existing Swift actor-isolation warnings and a
  Rust dead-code warning remain.
- `git diff --check` passed. Independent read-only diff review found no blockers.
- No Rust behavior changed; Rust test suites were not rerun for this UI change.
- Installer idle checks found 68 tickets, zero active tickets, and zero pending
  or queued chats before replacing the app and restarting the daemon.

## Boundaries

Approve, Accept locally, Stop and Home's conditional Stop reply were not
executed against real work. macOS 14/15 fallback code compiled but was not run
on those OS versions. No automated AppKit hit-testing regression test is
claimed; the before/after coordinate mouse reproduction is the UI evidence.

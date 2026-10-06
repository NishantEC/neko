# Neko: Orbit visual style study

Review [Neko · Visual styling only in Paper](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-5-0).

The user clarified on 2026-10-06 that Orbit is a reference for design elements, not product structure or workflows. Preserve Neko's existing navigation, screens, control inventory and behavior. The earlier workspace redesign and flow diagrams are archived and unapproved.

## What to borrow

| Visual detail | Apply to existing Neko components |
| --- | --- |
| Quiet dark surfaces | Neutral charcoal, clear separation between canvas and controls, no colored wash over content. Keep the existing system appearance support. |
| Typography | System font, strong primary text, readable secondary text, restrained semibold labels. Retain the shared 28/20/15/13/12 scale where each role already belongs. |
| Spacing and alignment | Consistent 8/16/24 rhythm, comfortable reading line height, aligned icon and trailing-action slots. Adjust component spacing without moving destinations or introducing panes. |
| Fine structure | Quiet hairline separators, consistent corner relationships, modest elevation on controls rather than boxes around every message. |
| Personality and state | Keep Neko's existing pearl; use small semantic color accents with text labels. Preserve pointer-driven behavior and accessibility fallbacks. No new cast of specialist agents. |

## Two treatments of the same components

**01 · Quiet ink** uses an opaque composer, flat content and quiet dividers, closest to Orbit's visual surface treatment.

**02 · Native glass** keeps the same component geometry, text and controls, with a subtle glass rim and depth on the composer and controls. This is the recommended visual direction for Neko. The Paper gradient and pearl are static cues for the existing native material/shader, not an implementation recipe for a fake blur.

The comparison contains existing message, task-row, attachment, Ask/Plan, model/effort/speed and Send component specimens. Their arrangement is a style sheet, not a proposed screen layout. Model, status, file and conversation values are sample content. Neither version proposes tabs, inspectors, terminals, navigation changes or new workflow steps.

![Component styling comparison](../evidence/orbit-visual-style-2026-10-06/component-comparison.png)

## Verification

Paper screenshots were reviewed after the initial component pass and after the final comparison. An inherited black text color was corrected before duplication. The final sheet has readable primary/secondary text, aligned controls, consistent spacing and no clipped content. Export dimensions and SHA-256 are recorded in [the Paper mapping](../evidence/orbit-visual-style-2026-10-06/paper.json).

The current source and the installed-selector evidence informed the existing composer controls. The initial study changed design/documentation only; its Paper review did not establish native rendering or behavior.

## Selected implementation · 02 Native glass

The user selected Native glass. The SwiftUI implementation uses real system glass on the existing composer, opaque content surfaces, shared attachment chips and a circular action. Conversation spacing follows the 24/8/16 rhythm. The existing navigation, native sidebar, controls and runtime behavior remain intact. Reduce Transparency and Increased Contrast replace only the composer background, preserving editor identity. Design lab shares the production component styling.

Installed from `7fec3e9` on 2026-10-06. Source verification: 131 XCTest cases passed, and the Swift Testing runner reported 11 tests passed with its optional real-daemon test skipped. Independent read-only review found no actionable issues. Native QA caught and fixed an extra glass container that obscured Design lab controls; light/dark rendering and local composer interactions were then checked. See [installed verification and screenshots](../evidence/native-glass-visual-style-2026-10-06.md) for the exact checks and remaining limits.

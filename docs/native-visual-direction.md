# Native visual direction — CleanMyMac reference

2026-09-29: user requested CleanMyMac's elegance, color, dimensionality and playful character as the reference for native Neko. This refines the earlier monochrome Paper direction; it does not change daemon or approval semantics.

## Observed in the installed app

Inspected My Clutter, Smart Care and Performance via native UI. No scan, cleanup, permission change or file deletion was initiated.

- Saturated, section-specific color fields: teal clutter, violet/magenta Smart Care, amber performance.
- Large dimensional illustrations with soft lighting and broad negative space.
- Small sculptural navigation icons; glass selection surfaces rather than hard boxes.
- Translucent recommendation cards, rounded controls and strong text hierarchy.
- Section transitions pass through a softened color field before settled content. The underlying shader/rendering implementation was not inspected and must not be inferred from screenshots.

## Translation to Neko

Keep Neko's own logo/mascot and the existing conversation-led product architecture. Do not reuse CleanMyMac artwork or make every work screen a decorative hero.

1. Welcome and empty states: expressive mascot, dimensional lighting, one clear next action.
2. Today: colored ambient perimeter and a compact personal greeting; readable neutral conversation surface.
3. Tickets, approvals and tools: calm legible content, consistent color accents, native controls and progressive disclosure.
4. Quick panel: compact translucent utility surface with subtle character, not a miniature dashboard.
5. Motion: short contextual transitions; honor Reduce Motion, Reduce Transparency and increased contrast. No continuous decorative animation while reading or coding.

## Implemented direction and design reference

Paper file `01M3A0PQVJM1HSHZK3ZRM29JD4`, page `p-1-0` now contains
`Native · Today · Opal glass` (`311-0`) and `Native · Tickets · Kanban`
(`33Y-0`). Existing boards were retained. Kanban example cards are explicitly
labelled examples, not live tasks.

The native client uses a saturated plum/amethyst/pink MeshGradient greeting,
the original Neko mascot, a quiet reading surface, and actual macOS 26
`GlassEffectContainer`, `glassEffect`, `glass` and `glassProminent` controls.
Older macOS versions use native material/bordered-control fallbacks. Reduced
transparency and increased contrast remove decorative gradients; Reduce Motion
disables the entrance spring. The treatment is not a claim of custom shader parity
with CleanMyMac.

Tickets are four Kanban columns: Needs approval, In progress, Ready to review,
and Done. Cards open the plan, result, independent review and activity evidence.
Status changes remain permission-gated actions, not arbitrary drag-to-build.
The real native preview showed a fixture moving from Needs approval through
In progress to Ready to review. Final current-source packaging and all-screen
visual acceptance are still outstanding.

Visual critique: the board keeps four clear stages and readable, compact cards;
color belongs primarily to navigation and state accents rather than tinting every
paragraph. The Today hero provides the more expressive brand moment.

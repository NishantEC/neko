# Native Liquid Glass workspace

References: [Paper 21A · Daylight and 21B · Evening](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-1-0), derived from 20A's conversation-first structure. Date: 2026-10-05. Paper approximates the material; the installed macOS app is the rendering reference.

## Problems observed in the installed app

The live audit covered Home, an actual completed agent conversation, All agents Board, Workspaces, Watching, Schedules, Tools, Memory, Working style, Profiles and General Settings. It found inconsistent content widths, long unbroken metadata, repetitive card borders, oversized secondary controls, and large suggestion cards ahead of active watches. Working style exposed long source observations and raw Markdown in its default view. Management section headers merged their buttons into accessibility secondary actions.

The first craft pass became too dense and flat: opaque composer backing and a forced dark appearance hid the native material. This revision restores spacious typography, open content and functional glass. Native NavigationSplitView, source-list selection, the real macOS toolbar and the stable split-pane sizing contract stay in place. Existing drafts, task authority, command evidence, cancellation and persistence are behavioral constraints.

## Shared contract

- System font: 28pt display, 20pt title, 15pt semibold section heading, 15pt interface and conversation body, 13pt secondary text and 12pt code.
- Dynamic light/dark surfaces shared by `N` and `Ink`: paper `#F7F8FA` or dusk `#202329`, native blue actions and readable semantic status colors. Appearance follows the system by default; Settings and the sidebar offer System / Light / Dark.
- Management content is bounded to 880pt; conversations to 760pt. Both use 32pt outer insets and 28pt section gaps; the board remains horizontally scrollable.
- The floating composer uses real `glassEffect` on macOS 26, with a material fallback on macOS 14–15. Its editor and inner buttons do not stack another glass surface. Native sidebar and toolbar own their platform materials. Content cards stay opaque. Background color fields are static and quiet; Reduce Transparency removes decorative background fields.
- Native segmented pickers own focus, keyboard selection and state. Search keeps matching control geometry and exposes a labelled clear action.
- Long evidence, source text and secondary settings remain available through explicit disclosures. No stored detail is discarded to simplify the view.

## Page treatment

| Surface | Treatment |
| --- | --- |
| Home and agent chat | Shared reading width, separated speaker/result groups, folded work history and a roomy floating glass composer |
| All agents | Aligned status/title/metadata lanes; calmer cards and informative empty columns |
| Workspaces, Profiles, Schedules | Spacious introductions, clear section identity, separate reachable actions, useful empty states |
| Memory and Working style | Grouped controls, human-readable source context, folded decision details |
| Tools and Watching | Readable rows, active watches first, secondary suggestions and check details folded |
| Settings | Native tabs, consistent grouped forms, useful loading/empty/check states |
| Onboarding | Same quiet surfaces and native controls, scrollable step content, optional permission choices preserved |

## Verification

See `docs/evidence/native-liquid-glass.md` for this revision's tests, installation and screen verification. `native-polish.md` records the superseded craft pass. A source review or unit test is not evidence of physical mouse, divider drag, live model or provider-compaction behavior.

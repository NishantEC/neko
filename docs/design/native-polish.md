# Native app craft pass

Reference: [20A · Complete chat — conversation first](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-1-0). Date: 2026-10-05.

## Problems observed in the installed app

The live audit covered Home, an actual completed agent conversation, All agents Board, Workspaces, Watching, Schedules, Tools, Memory, Working style, Profiles and General Settings. It found inconsistent content widths, long unbroken metadata, repetitive card borders, oversized secondary controls, and large suggestion cards ahead of active watches. Working style exposed long source observations and raw Markdown in its default view. Management section headers merged their buttons into accessibility secondary actions.

The approved direction remains conversation first. Native NavigationSplitView, source-list selection, the real macOS toolbar and the stable split-pane sizing contract stay in place. Existing drafts, task authority, command evidence, cancellation and persistence are behavioral constraints of this pass.

## Shared contract

- System font: 24pt display, 17pt page title, 13pt semibold section heading, 13pt interface body, 14pt conversation, 12pt secondary text and code.
- One graphite surface ramp, shared by `N` and `Ink`; status colour is accompanied by a label or symbol.
- Management content is bounded to 820pt; conversations to 720pt. Both use 28pt outer insets with smaller inner groups, while the board remains horizontally scrollable.
- Native segmented pickers own focus, keyboard selection and state. Search keeps matching control geometry and exposes a labelled clear action.
- Long evidence, source text and secondary settings remain available through explicit disclosures. No stored detail is discarded to simplify the view.

## Page treatment

| Surface | Treatment |
| --- | --- |
| Home and agent chat | Shared reading width, clear speaker/result hierarchy, compact work history and composer |
| All agents | Aligned status/title/metadata lanes; calmer cards and informative empty columns |
| Workspaces, Profiles, Schedules | Short introductions, compact section identity, separate reachable actions, useful empty states |
| Memory and Working style | Grouped controls, human-readable source context, folded decision details |
| Tools and Watching | Compact native lists, active watches first, secondary suggestions and check details folded |
| Settings | Native tabs, consistent grouped forms, useful loading/empty/check states |
| Onboarding | Same quiet surfaces and native controls, scrollable step content, optional permission choices preserved |

## Verification

See `docs/evidence/native-polish.md` for final tests, installation and screen verification. A source review or unit test is not evidence of physical mouse, divider drag, live model or provider-compaction behavior.

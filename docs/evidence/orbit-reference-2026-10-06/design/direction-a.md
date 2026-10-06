> Archived and unapproved: this layout exploration exceeded the user’s visual-only request. See [the active visual study](../../../design/orbit-visual-style.md).

Review **A · Focused conversation** in [Paper](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-3-0).

Completed three static 1440×900 native SwiftUI/AppKit main mockups and one 1440×1821 shared state sheet on the existing page. No implementation edits, daemon calls/restarts or installation. Only Direction A nodes were created/changed; Direction B nodes were left untouched. HEAD remains `aef9df8`; the pre-existing untracked Orbit reference directory remains untouched.

## Artifacts

| Main artifact | Paper ID | Outside-UI annotation | Local PNG |
| --- | --- | --- | --- |
| A · Focused conversation · Home | `9J0-0` | `AZR-0` | [a-home.png](a-home.png) |
| A · Focused conversation · Task review | `9YH-0` | `AZS-0` | [a-task-review.png](a-task-review.png) |
| A · Focused conversation · Home · Light | `BES-0` | `CI4-0` | [a-home-light.png](a-home-light.png) |
| A · Focused conversation · Shared states | `BL3-0` | Captions outside each UI crop | [a-states.png](a-states.png) |

Machine-readable component IDs, capability mapping and ownership: [JSON map](direction-a.json). Home is at canvas x=1520/y=0; task is x=4560/y=0. Annotations sit directly below their boards at y=980. The follow-up adds light Home at x=1520/y=1240, its annotation at x=1520/y=2220, and shared states at x=4560/y=1240. These seven A roots/subtrees are owned by this task.

## Design decisions

1. **Native shell:** 236pt source-list sidebar, 52pt unified titlebar, 40pt retained-tab strip. System Sans-Serif at 28/20/15/13/12; existing Paper dark tokens and native rounded controls. Context stays visible without importing Orbit’s fictional specialist roster, online counts or permissions.
2. **Home:** a readable 760pt conversation, one sample task needing review and a direct Review result action. Compact AppKit-style composer follows the transcript in normal layout. No fixed/sticky position or surrounding dock surface.
3. **Task review:** current result, explicit Accept locally, saved independent-review/command evidence, collapsed earlier history, task reply composer. A 288pt contextual inspector carries workspace/profile, next-run preference, changed-file entries and working-folder reveal. The conversation remains 760pt wide with the inspector open.
4. **Sample boundary:** every task ID, conversation, filename, verdict, command receipt and connection state is illustrative. Small external annotations explicitly say so. `Exit 0` and the displayed review pass are mock data, not claims that this design task ran tests.
5. **Native behavior:** Paper presents static geometry, not live macOS blur, focus, keyboard or resize behavior. Glass belongs only on control surfaces; message/result/evidence surfaces are opaque. Existing native appearance, accessibility, safe-area and stableSplitPane behavior must survive implementation.

## Capability / implementation boundary

| Proposed control or content | Reuse contract |
| --- | --- |
| Review result / task navigation | `AppModel.openAgent` and full-page `TicketDetail`; retain originating page, workspace, filters and drafts. |
| Home Ask/Plan, attachments and send | Existing `SharedComposer`, `ComposerView`, scoped draft store and SendMessage/InterruptAndSendMessage/CancelChat adapters. Plan formats a request; it does not grant execution permission. |
| Task reply | Existing ReplyToTask adapter. Keep this separate from Home’s queue/send semantics. |
| Accept locally | CompleteTask records acceptance. It does not apply a patch, merge, publish or deploy. This distinction remains visible in the inspector. |
| Automatic runtime control | Existing ComposerRuntimeScope/Picker and real daemon catalog/settings. No model names, paid tiers, quota estimates or provider availability were invented. |
| Independent review / saved evidence | Existing TicketOutcomeReadback, TicketCommandEvidence and task events. Render receipt shortening/incompleteness when present; do not treat history as current proof. |
| File rows / working folder | Existing TaskChanges data and Finder reveal. A per-file detail presentation is new composition; do not imply a universal file editor. |
| Persistent tabs | **New native navigation state**, reusing existing Home/task destinations. A retained open-tab collection and selection state are not implemented in current AppModel. Switching tabs must preserve scoped drafts; this mockup does not claim app-restart persistence already exists. |
| Optional inspector | **New mounted layout**, reusing snapshot, TaskChanges, profile and runtime data. Current TicketInspector is retained source and is not mounted by the active shell; do not assume it already supplies this exact inspector. |

Source contract: [native capability map](../native-capability-map.md). Reference study: [Orbit behavior map](../source-behavior-map.md); visually inspected `01-home-idle.jpg` and `15-chat-initial.jpg` with view_image. Applied better-ui and swiftui-pro design/navigation guidance; read Paper guide, target-page context, tokens and System Sans-Serif font availability before design styling.

## Visual verification

Reviewed Paper screenshots after shell, conversation, task result, inspector/composer and external annotations. Corrected inherited dark text, a close glyph, reading width, composer position and redundant copy. Final 1× screenshots show consistent icon/action lanes, readable hierarchy/contrast, no clipped content and both main artboards at exactly 1440×900. External annotation heights use fit-content. Native interaction/runtime QA remains outside this design-only task.

Current Paper PNG export paths are recorded in the JSON map. The revised dark exports have `(1)` filename suffixes in Downloads; the `/tmp` copies above are current.

## Wide desktop reference follow-up

Inspected `121-desktop-home.jpg` (1440×900) with view_image and its accessibility capture. The wider structure shows the persistent top tabs, left source navigation, central work area, separate right inspector and a terminal/output region beneath the central content. This supports Direction A’s existing sidebar/tab/optional-inspector hierarchy; its conversational reading lane remains the chosen emphasis.

The terminal prompt is displayed as generic text, not a textbox. The parent also verified that it does not accept commands. Direction A has **no interactive shell input**: its command area represents saved, read-only evidence only. Do not add a prompt, caret, Run button or terminal command entry based on this reference. No Paper or PNG changes were needed for this follow-up.

Parent-reported coverage: all 12 scenes checked and 121 captures. This follow-up inspected capture 121 only; it does not independently certify the whole reference set.



## Review follow-up · 2026-10-06

Removed the complete composer footer groups on dark Home and Task review, and removed the duplicate Return/Shift Return sentence from the illustrative task result. The light clone and all new state crops contain no visible send-shortcut hint. Optional Shift Return help belongs in accessibility metadata only, offscreen. This is a design change; source keyboard behavior was not edited.

### Shared state sheet

Eight UI crops are on one board, each with a separate design caption stating what is shown or hidden. All task text, model conditions, attachment names and command receipts are illustrative; `Passed` and `Exit 0` are not results produced by this task.

| State | Visible controls and content | Hidden / disabled boundary | Current source backing |
| --- | --- | --- | --- |
| Empty / start | Ask / Plan, attachment, Automatic, empty editor | Send disabled; no transcript, result or Stop | [Composer state](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/SharedComposer.swift:49) |
| Active / stop | Reply in progress; Stop with an empty draft | Home changes to Queue after typing; ticket replies redirect rather than queue | [Primary action](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/SharedComposer.swift:55), [Home adapter](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TodayView.swift:230) |
| Needs a question answered | Actual question, filled reply, enabled Send and existing Stop | Approve hidden while waitingReason is present; no new permission UI | [Question guard](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TicketsView.swift:547), [Reply adapter](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TicketThread.swift:171) |
| Ready for review | Result, independent review, saved-evidence disclosure, reply, Accept locally, Stop | Start again hidden; older history folded; acceptance is CompleteTask, not publication | [Toolbar guards](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TicketsView.swift:552) |
| Interrupted / error | Saved stop reason, Start again, existing worktree reveal, reply | Stop and acceptance hidden for Failed/Cancelled. Home failed replies use Retry instead | [Stop reason](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TicketsView.swift:18), [Start again](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TicketsView.swift:556), [Home Retry](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TodayView.swift:555) |
| Unavailable model | Refresh models, Neko decides, unavailable choice, validation, Discard, Check & apply | Unavailable choice and Check & apply disabled until a valid dirty selection. No silent fallback or invented model name | [Model rows](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/ComposerRuntimeModelList.swift:42), [Validation](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/RuntimeSelectionState.swift:35), [Apply controls](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/ComposerRuntimePanel.swift:56) |
| Shortened evidence | Exit status, saved output, explicit retained-output notice | Missing output cannot be recovered here; no shell input. Incomplete/malformed records must use the separate incomplete warning, not a passed badge | [Receipt notices](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/TicketCommandEvidence.swift:45) |
| Draft preserved | Same unsent text and removable attachment after returning to its ticket | No restore banner or app-restart persistence claim. Revision-safe clear still applies | [Scoped store](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/SharedComposer.swift:13) |

The model crop consolidates the real model-list/validation/action content for state comparison; full picker navigation, search, check-cost help and independent effort/speed selection still need to remain in implementation. Stop on question/review comes from the existing native toolbar guard; it is not a new permission action.

### Light appearance parity

Cloned revised Home with identical content, layout, sidebar, tabs and controls. Used native `N.canvas` #F7F8FA, `N.panel` #FFFFFF, secondary text #606875, tertiary #666E7A, and adaptive attention #895600 from [DesignSystem.swift](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/DesignSystem.swift:346) and [status colors](/Users/nish/Documents/neko/native/NekoKit/Sources/NekoNative/DesignSystem.swift:7). #1D1D1F approximates dynamic labelColor; separators/selection approximate Color.primary at 8%/6.5%. #F0F1F3 is a static control-material approximation. Native system appearance remains the implementation authority.

All recoloring is scoped to the light clone. Shared dark tokens and B nodes were not changed by this task. Content/evidence remain opaque; the design does not claim to reproduce live AppKit vibrancy or Liquid Glass.

### Final visual review and exports

1. Dark Home and Task review: complete footer removal, consistent spacing and type, legible contrast, aligned controls and no clipped content at 1440×900.
2. Light Home: corrected cloned SVG strokes and composer outline; same hierarchy and content at 1440×900, without dark-mode token leakage.
3. Shared states: reviewed each row plus the whole board; aligned composer lanes where comparable, distinguished enabled replies from empty disabled Send, retained readable outside-UI captions. All eight crops and final annotation fit 1440×1821.
4. Re-exported all four boards to PNG and verified file dimensions. Notes/JSON identify current exports and owned nodes. No implementation edits, daemon actions or runtime tests. HEAD aef9df8; parent-owned reference captures remain untouched.

Next action (~1 minute): open the shared state sheet and compare the question and review action rows.

## Final copy correction

State 03 now asks “Apply this shortcut in every workspace or just neko?” with the draft answer “Only neko. Keep my other workspaces unchanged.” Removed the locally discoverable macOS-version question and filler. The question represents a user-owned scope choice; Neko should investigate discoverable local facts autonomously. Existing controls and permission boundaries were unchanged.

Screenshot verdict: spacing, typography, contrast and alignment remain clear; both sentences fit without clipping, and no extra repetitive copy was added. Re-exported the state sheet to the current 1440×1821 PNG linked above. Light Home, other states, B and implementation were not edited.

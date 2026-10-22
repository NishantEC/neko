# Vendored from hushbacks

The files under `web/src/ui/`, `web/src/editorial/`, `web/src/chat/` and
`web/src/styles/` that start with `// from hushbacks packages/ui/src/<path>` were
copied from the owner's own project, hushbacks (`~/Documents/hushbacks`,
`packages/ui`), by `web/scripts/vendor.mjs`, which rewrites `@workspace/ui/...`
imports to the local `@/ui/...`, `@/editorial`, `@/chat/...` paths. hushbacks is
read-only from here; to take another component run
`node web/scripts/vendor.mjs components/<name>.tsx [ui|editorial|chat]`.

Taken: `ui/` — accordion, alert, alert-dialog, avatar, badge, breadcrumb, button,
button-group, card, chart (+ `charts/` Bar, Distribution, Line, Pie), checkbox,
collapsible, command, context-menu, dialog, dropdown-menu, empty, field,
form-portal-context, hover-card, icons (phosphor re-exports), input, input-group,
kbd, label, popover, progress, radio-group, resizable, scroll-area, select,
separator, sheet (+ variants), sidebar, skeleton, sonner, spinner, status-badge,
switch, table, tabs, textarea, timeline, toggle, toggle-group, tooltip,
`lib/utils.ts`, `hooks/use-mobile.ts`.
`editorial/` — EditorialHeading, FieldLabel, GlowTile, MonoLabel, PageHeader,
Panel, SectionHead. `chat/` — attachments, clipboard, conversation,
message, suggestion, json-viewer (message-markdown was replaced by T3 Code's
ChatMarkdown, below). `styles/` —
component-surfaces.css, motion.css (and `globals.css`, rewritten for Neko's palette).

Not taken: osmo-ui, skiper-ui, aceternity-ui, magic-card, neon-gradient-card,
meteors, warp-background and the other decorative effects; thinking-orbs.

## Changes made to the copies

- `use-mobile.ts`: no editor preview-viewport context.
- `sidebar.tsx`: `data-active` is present only when active (a bare `data-active`
  variant would otherwise match `data-active="false"`).
- `chat/types.ts` (new): local `Role`, `FileUIPart`, `SourceDocumentUIPart` in
  place of the Vercel `ai` package types (`message.tsx`, `attachments.tsx`).
- `chat/message.tsx`: `MessageResponse` renders T3 Code's ChatMarkdown.
- `chat/conversation.tsx`: `ConversationScrollButton` renders its children.
- `chat/json-viewer.tsx`: 24px rows; the row is a div so its copy button is not
  nested inside a button.
- Written for Neko: the behaviour of `chat/chat-input.tsx` (no GCS upload, no
  speech), `chat/work-log.tsx` (over Neko's `activity` and `tool_calls`),
  `chat/decisions.tsx`, `chat/turns.tsx`, `chat/TaskConversation.tsx`. Their
  markup is now T3 Code's (below).

## The app shell, copied from hushbacks `apps/spa` @da20dfaeb (2026-10-22)

`button.tsx`, `select.tsx` and `sidebar.tsx` are hushbacks' originals again (the
earlier spacing patch is gone): `<body class="sky">` and `styles/sky-materials.css`
supply the running app's sizes exactly as hushbacks does. Each file below starts
with `// from hushbacks <path> @da20dfaeb`:

| Neko file | hushbacks `apps/spa/src/...` | Adapted |
| --- | --- | --- |
| `styles/sky-materials.css` | `components/Sky/sky-materials.css` | Colours on Neko tokens (pebble, well, ink, status); no scene, grain, glass blur, sidebar gradient, swatches, builder hues or composer rules; `default` buttons keep Neko's blue primary |
| `shell/AppSidebar.tsx`, `shell/AppSidebar.parts.tsx` | `components/UnifiedAppSidebar/index.tsx`, `UnifiedAppSidebar.parts.tsx`, `AccountFooter.tsx` (ThemeSwitch), `MobileTopBar.tsx` | Hash links; Forms rail → live Tasks rail; workspace switcher → product name; account row → connection row; Settings sub-nav lists Neko's sections; ⌘K search row and page groups added; no usage well, notification bell or admin |
| `shell/PageContainer.tsx` | `components/PageContainer.tsx` | Passes extra props (`data-slot`) to the scroll surface |
| `pages/setup/settings/settings-surface.css` | `pages/Settings/settings-surface.css` | Targets Neko's kit slots |
| `setup/kit.tsx` (SetupPage, Section, Panel, PanelFooter, Row, SettingRow) | `components/PageContainer.tsx`, `pages/Settings/settings.ui.tsx` | Neko's props kept; the section heading sits above its panel |
| `pages/setup/Settings.tsx` layout | `routes/_app.settings.tsx` | Breadcrumb bar, 52rem settings surface, phone/collapsed pill nav; search stays at the top of the column |
| `editorial/PageHeader.tsx` | `packages/ui/.../editorial/PageHeader.tsx` | No eyebrow, plain sans heading at hushbacks' size |
| `work/builder/BuilderViewTabs.tsx` (+ `.css`), `BuilderPanelTabs.tsx`, `useFusedTabIndicator.ts`, `builderHeaderLayout.ts` | `pages/Builder/components/...` | View value is a string; builder surfaces on Neko tokens |
| `work/TaskBuilder.tsx` | `pages/Builder/index.tsx`, `components/BuilderDesktopHeader.tsx`, `FormBuilderMode.tsx`, `hooks/useChatPanelSize.ts`, `hooks/useCollapsedChat.ts`, `useBuilderViewMode.ts` | Task conversation in the chat panel; Changes / Plan / Details views; task actions at the header end; chat 35% default (45% max) |

Not copied: the Dashboard list patterns (form tiles, which Neko's lists are not), the
form-specific builder views, presence, publish and share controls.
`button.tsx` size variants and `select.tsx` `data-[size=sm]` height follow what
hushbacks' running app renders (its `Sky` layer: control 40 px, row 36 px, xs 32 px)
instead of the raw primitives' 32/28/24 px; `sidebar.tsx` menu-button sizes do too.
Re-vendoring these files must keep those values. See
`docs/design/web-ui-from-hushbacks.md` → "Spacing, measured".

# From T3 Code (MIT)

Source: pingdotgg/t3code `origin/main` at `3b95e7831`, MIT, Copyright (c) 2026
T3 Tools Inc.; licence text in `THIRD_PARTY_LICENSES/t3code-MIT.txt`. Each file
carries a `// from t3code <path> @3b95e7831 (MIT)` header saying what changed.
Markup and classes are T3's; colours are Neko's tokens (`chat/t3/t3-chat.css`
maps T3's token names onto them, no T3 palette is imported); the data layer is
Neko's workbench.

| Neko file | T3 Code source (`apps/web/src/…`) |
| --- | --- |
| `chat/t3/WorkLog.tsx` | `components/chat/WorkLog.tsx` (unchanged) |
| `chat/t3/ComposerSurface.tsx` | `components/chat/ComposerSurface.tsx` (unchanged) |
| `chat/t3/ComposerBanner.tsx` | `components/chat/ComposerBanner.tsx` (Scroll is a plain div) |
| `chat/t3/ComposerPrimaryActions.tsx` | `components/chat/ComposerPrimaryActions.tsx` (send / queue / stop only) |
| `chat/t3/ComposerPendingApproval.tsx` | `components/chat/ComposerPendingApprovalPanel.tsx`, `…Actions.tsx`, the approval drawer in `ChatComposer.tsx` |
| `chat/t3/ChatMarkdownRenderer.tsx` (+ lazy `ChatMarkdown.tsx`) | `components/ChatMarkdown.tsx` (root, react-markdown, pre / table / link renderers) |
| `chat/t3/MarkdownCodeBlock.tsx` | `components/chat/MarkdownCodeBlock.tsx` (Shiki web bundle, lazy, dual theme) |
| `chat/t3/MarkdownTable.tsx` | `components/chat/MarkdownTable.tsx` |
| `chat/t3/MessageCopyButton.tsx` | `components/chat/MessageCopyButton.tsx` |
| `chat/t3/CommandBlock.tsx` | `components/CommandBlock.tsx` |
| `chat/t3/TimelineRowParts.tsx` | `components/chat/TimelineRowParts.tsx` |
| `chat/t3/TimelineUserMessageRow.tsx` | `components/chat/TimelineUserMessageRow.tsx` (UserTimelineRow, CollapsibleUserMessageBody) |
| `chat/t3/TimelineAssistantRows.tsx` | `components/chat/TimelineAssistantRows.tsx` (AssistantTimelineRow, AssistantMessageMeta) |
| `chat/t3/TimelineWorkRows.tsx` | `components/chat/TimelineWorkGroupRows.tsx`, `TimelineWorkEntryRow.tsx`, `TimelineToolActivityIcon.tsx`, the turn fold in `MessagesTimeline.tsx` |
| `chat/t3/TimelineSystemDivider.tsx` | `components/chat/TimelineSystemDivider.tsx` |
| `chat/t3/t3-chat.css` | `index.css` (token names, live-activity utilities, `.chat-markdown` rules) |
| `chat/chat-input.tsx` (chrome) | `components/chat/ChatComposer.tsx`, the composer stack in `ChatView.tsx` |
| `chat/queue-tray.tsx` | `components/chat/QueuedRunsControl.tsx` |
| `chat/ChatInputMenu.tsx` (rows) | `components/chat/ComposerCommandMenu.tsx` |

Not copied: the Tiptap prompt editor (`ComposerPromptEditorTiptap.tsx`, ~2000
lines plus a dozen `@tiptap/*` packages; a textarea carries its classes and Neko
keeps its own key handling, drafts and menus), `MessagesTimeline`'s LegendList
virtualisation, and everything tied to T3's server state (citations, forks,
changed-files cards, subagents, worktrees, find-in-thread, model picker).

## The task side sheet, from hushbacks `apps/spa` @5d9a64b11 (2026-10-22)

The task opened beside the Tasks list is hushbacks' response details sheet
(commit 24a7b801e, "one motion system, rebuilt sheets and a response details
sheet"). `ui/sheet.tsx` and `sheet.variants.ts` were already current.

| Neko | hushbacks |
| --- | --- |
| `work/TaskSheet.tsx` | `pages/Builder/components/ResponseDetailPanel.tsx` |
| `work/TaskSheet.helperComponents.tsx` | `ResponseDetailPanel.helperComponents.tsx` (ResponseDetailBody) |
| `work/TaskSheet.hooks.ts` | `ResponseDetailPanel.hooks.ts` (useSwapFade) |
| `work/TaskSheet.skeletons.tsx` | `ResponseDetailPanel.skeletons.tsx` + `RouteSkeletons/primitives.tsx` (Bar, Pill) |
| `hooks/useOpenAfterMount.ts`, `lib/motion-tokens.ts` | same paths, unchanged |
| `styles/app-sheet.css` | `index.css` (`.app-sheet` exit), `ResponsesView.css` (`response-swap`, here `sheet-swap`) |

Adapted: the response became the task (title, a sky status pill, repository, PR,
updated, ID; the task's actions and ⋯ menu under the meta row; Conversation /
Changes / Details tabs; the reply box above the footer). Added copy link and
full page to the header. ← → move between tasks as upstream; j / k too, as on the
list. Docked in the list's stage from 1024 px (upstream 1280 px) and 520 px wide
(upstream 468 px), since a conversation needs the room. The "Delete response"
footer button has no counterpart; Delete stays in the ⋯ menu.

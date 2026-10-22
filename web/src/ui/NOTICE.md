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
message, message-markdown, suggestion, json-viewer. `styles/` —
component-surfaces.css, motion.css (and `globals.css`, rewritten for Neko's palette).

Not taken: osmo-ui, skiper-ui, aceternity-ui, magic-card, neon-gradient-card,
meteors, warp-background and the other decorative effects; thinking-orbs.

## Changes made to the copies

- `use-mobile.ts`: no editor preview-viewport context.
- `sidebar.tsx`: `data-active` is present only when active (a bare `data-active`
  variant would otherwise match `data-active="false"`).
- `chat/types.ts` (new): local `Role`, `FileUIPart`, `SourceDocumentUIPart` in
  place of the Vercel `ai` package types (`message.tsx`, `attachments.tsx`).
- `chat/message-markdown.tsx`: no KaTeX / math plugins.
- `chat/message.tsx`: Neko's table and prose styling (hairline rows, 12px tables).
- `chat/conversation.tsx`: `ConversationScrollButton` renders its children.
- `chat/json-viewer.tsx`: 24px rows; the row is a div so its copy button is not
  nested inside a button.
- Not vendored, written for Neko on hushbacks' patterns: `chat/chat-input.tsx`
  (no GCS upload, no speech), `chat/work-log.tsx` (WorkLog / ToolLine / ToolFold /
  ToolFailed over Neko's `activity` and `tool_calls`), `chat/decisions.tsx`,
  `chat/queue-tray.tsx`, `chat/turns.tsx`.

## Local deviations (spacing, 2026-10-22)

`button.tsx` size variants and `select.tsx` `data-[size=sm]` height follow what
hushbacks' running app renders (its `Sky` layer: control 40 px, row 36 px, xs 32 px)
instead of the raw primitives' 32/28/24 px; `sidebar.tsx` menu-button sizes do too.
Re-vendoring these files must keep those values. See
`docs/design/web-ui-from-hushbacks.md` → "Spacing, measured".

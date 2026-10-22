# Neko's web UI, built on hushbacks' component library

Decided 2026-10-22. The web UI (`web/`, served by the daemon, hosted in the
native window and the shared Chrome) is where Neko grows: agents can see,
change and verify it themselves. This page records what is taken from
hushbacks (`~/Documents/hushbacks`, internal name "enform") and why.

## Source

- `packages/ui` — Radix/base-ui + Tailwind v4 + shadcn-style components.
- `docs/design-principles.md` — the design language (read it).
- `apps/spa/src/components/agentchat` — the builder's agent chat (WorkLog,
  ToolLine, ToolFold, AskUserCard, MessageList).

It is the owner's own project, so its code may be copied; each vendored file
keeps a provenance header and `web/src/ui/NOTICE.md` lists them. **Not taken:**
`components/ui/osmo-ui`, `skiper-ui`, `aceternity-ui`, `magic-card`,
`neon-gradient-card`, `meteors`, `warp-background` and the other decorative
effects (third-party showcase code with its own terms, and visual noise for an
operations tool).

## What is taken

| Area | Components |
| --- | --- |
| Tokens | `styles/globals.css` token set, `component-surfaces.css`, `motion.css`, smooth shadows. Palette stays Neko's dark neutral; only structure is borrowed. |
| Primitives | button, button-group, badge, status-badge, card, kbd, tooltip, dialog, alert-dialog, dropdown-menu, context-menu, popover, hover-card, command (cmdk), collapsible, accordion, tabs, separator, switch, checkbox, radio-group, select, input, textarea, label, field, input-group, empty, skeleton, spinner, progress, scroll-area, sheet, resizable, sonner, table, timeline, toggle(-group), breadcrumb |
| Editorial | Panel, SectionHead, PageHeader, MonoLabel, EditorialHeading, FieldLabel, GlowTile (AmbientAurora only if wanted) — the eyebrow (mono) → heading (serif, italic emphasis) → subtitle voice, panels divided into rows |
| Chat | `ai-elements` Conversation (use-stick-to-bottom), Message (+actions, branch), message-markdown (Streamdown), Suggestion, Attachments; `ChatInput` composer; `json-viewer` for tool arguments; `charts` for usage |
| Agent chat patterns | WorkLog/ToolLine/ToolFold/ToolFailed, AskUserCard → Neko's approval/decision card, MessageList |

`thinking-orbs` is skipped (licence not checked); Neko keeps its own shimmer.

## What Neko needs on top

Pages (Swift today → web): Home, Tasks (list / board / tree + task chat),
Pull requests, Workspaces, Watching, Schedules, Tools & skills (MCP), Memory,
Profiles, Settings (+ Rulebook, runtime, budget), Usage & machine, ⌘K palette,
Onboarding. New for the self-evolving direction: a Plugins page (installed
plugins, enable/disable, "ask Neko to build one").

## Rules for everyone building here

1. Compose from `web/src/ui` and `web/src/editorial`; do not hand-roll a button,
   dialog or panel. If a primitive is missing, vendor it from hushbacks.
2. Pages register in `web/src/pages/index.ts` (lazy); one line per page.
3. Every page must have a `web-verify` screenshot step and no console errors.
4. Accessibility: keyboard reachable, visible focus, labelled controls.
5. No new dependency outside the approved list without a reason in the commit.
6. Colour comes from tokens (`--background`, `--foreground`, `--border`, …);
   never raw hex in a component.

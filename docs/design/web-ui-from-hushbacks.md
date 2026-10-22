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

## What was built (foundation, 2026-10-22)

Everything below lives in `web/src/`; `web/src/ui/NOTICE.md` lists each vendored
file and what was changed in the copy. `node web/scripts/vendor.mjs <path>
[ui|editorial|chat]` takes another file from hushbacks with the import rewrites
and the provenance header.

- **Tokens** (`styles/globals.css`, `component-surfaces.css`, `motion.css`):
  hushbacks' token names, smooth shadows (`shadow-plugin`), component surfaces,
  motion and the squircle corners. Dark is Neko's neutral (`#0f0f0f` canvas,
  `#121212` sidebar, purple `--bubble`); light follows the system preference.
  `src/theme.ts` puts `.dark` / `.light` on `<html>` (sidebar menu, ⌘K).
  `--primary` is Neko's blue (send / Approve), not hushbacks' neutral.
- **Fonts**: `@fontsource-variable/inter` (body) and `fraunces` (headings only,
  `--font-serif`), bundled. Mono (eyebrows, code) is the system mono. The daemon's
  CSP (`default-src 'self'`) refuses `data:` fonts and `blob:` images, so
  `assetsInlineLimit` is 0 and pasted images are previewed as data URLs.
- **Primitives, editorial, chat**: see NOTICE. Home is built only from them.
- **Shell**: `shell/AppShell.tsx` (sidebar with Home, pages by group, a Plugins
  section from the manifests, a Connected indicator, theme menu), `shell/
  CommandPalette.tsx` (⌘K: "Go to <page>", actions pages register with
  `registerCommands`, theme, reload plugins), `router.ts` (hash router).
- **Gallery**: `/#/ui` renders every vendored component with Neko sample data;
  `scripts/web-verify.mjs` screenshots each section in both themes.

### Deviations from the plan

- **Base UI, not Radix.** hushbacks' components import `@base-ui/react` (with
  `cmdk`, `vaul`, `react-resizable-panels`); `radix-ui` is installed but only its
  odd files use it, and none were needed. Triggers take `asChild` or `render`.
- `sonner` the npm package is installed for pages that want it, but the vendored
  `ui/sonner.tsx` is hushbacks' own Base UI toast (`toast.success(...)`).
- `vaul` / `drawer` was not vendored (nothing uses it); the dependency is there.
- `mermaid` is aliased to a stub (`src/stubs/mermaid.ts`): Streamdown would
  otherwise emit ~30 MB of diagram chunks nobody opens. Math/KaTeX is dropped from
  `message-markdown`.
- Streamdown ships its own table controls (copy as CSV/TSV, download); they sit
  above the table instead of below it as in the old UI.
- ThinkingOrb is replaced by a pulsing dot and the shimmering verb.
- Bundle: main chunk 403 KB (126 KB gzip) -> 653 KB (203 KB gzip), CSS 10 KB ->
  169 KB (27 KB gzip). The gallery (recharts), Streamdown's code highlighter and
  every Shiki grammar are lazy chunks; `dist/` is ~13 MB without sourcemaps.

## Adding a page

1. Create `web/src/pages/Tasks.tsx` with a default export (any React component).
   Compose it from `@/ui/*`, `@/editorial` (`PageHeader`, `Panel`, `SectionHead`)
   and `@/chat/*`; read data with `useWorkbench()` and `command(...)` from `@/api`.
2. Register it, one entry in `web/src/pages/index.ts`:

   ```ts
   page({
     id: "tasks", title: "Tasks", icon: ListChecks, keywords: ["board", "agents"],
     group: "work", load: () => import("@/pages/Tasks"),
   }),
   ```

   That gives it the route `#/tasks`, a sidebar entry (under `main`, `work` or
   `system`) and a "Go to Tasks" entry in ⌘K. For ⌘K actions while the page is
   open: `useEffect(() => registerCommands([{ id, title, run }]), [])` from
   `@/shell/commands`.
3. Add it to the gallery if it introduced a component, and add a step to
   `scripts/web-verify.mjs` that opens the page, screenshots it in both themes and
   calls `a11y(...)`.
4. `pnpm --dir web build && node scripts/web-verify.mjs`, then read the
   screenshots in `/tmp/neko-web/`.

Plugins (`<data dir>/plugins/<name>/`) are unchanged; they should style with the
tokens (`var(--foreground)`, `var(--border)`, ...), as `web/plugins/pr-board` now does.

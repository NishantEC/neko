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
Onboarding. New for the self-evolving direction: Customisations (your pages,
overrides of built-ins and styles, "Change Neko"); see `one-registry.md`.

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
- **Shell**: `shell/AppShell.tsx` (sidebar with Home and every registered page by
  group, yours marked "yours", a Connected indicator, theme menu), `shell/
  CommandPalette.tsx` (⌘K: "Go to <page>", actions pages register with
  `registerCommands`, theme, reload customisations), `router.ts` (hash router).
  Built-in and user pages are one registry (`registry.ts`, `host.ts`).
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

Pages in `<data dir>/plugins/<name>/` register through the same host as the built-ins
(`one-registry.md`); they should style with the tokens (`var(--foreground)`,
`var(--border)`, ...), as `web/plugins/pr-board` does.

## Shell, composer, plugins and first run (2026-10-22)

Everything below lives in `web/src/{shell,chat,onboarding,pages/plugins}` and
`crates/neko-daemon/src/web.rs`; `scripts/verify/shell.mjs` checks it in a real
browser (dark and light, no console errors, `a11y(...)`).

### Composer (`chat/ChatInput*`, used by Home through `NekoComposer`)

A port of the native composer (`SharedComposer`, `ComposerMenus`,
`ComposerRuntime*`, `SlashCommands`), same wording and same commands:

- **Keys**: Return sends (queues while a reply runs), Shift+Return a new line,
  ⌘Return `InterruptAndSendMessage` (and does not also approve a decision),
  ⌥↑ takes the last queued message back. The button morphs Send / Queue / Stop.
- **`/` menu** (`ChatInputSlash.ts`, `ChatInputMenu.tsx`): `/plan /stop /remember
  /forget /recall /memory /tickets /clear /models /permissions /setup /help`.
  Opens **above** the composer, opaque, seven rows then scrolls; ↑ ↓ choose, ↵ or
  Tab accept, Esc closes. `/remember` and `/forget` become the chat messages the
  daemon answers in code; `/stop` asks first.
- **`@` menu**: workspaces, enabled connections (from the snapshot) and files from
  `GET /api/files` (below), replacing the trailing `@word`.
- **Ask / Plan**: Plan wraps the text with the same sentence as the native app.
- **Attachments**: paste, drop or choose; the browser POSTs the bytes to
  `POST /api/attachments` and puts the answer, `![Image](file://…)` or
  `[name](file://…)`, in the message, exactly what the native composer writes.
  4096-byte limit counts them, like the daemon.
- **Runtime picker** (`ChatInputRuntime.tsx`): model, effort, speed and automatic
  paid speed for one conversation (`home:<profile>:<workspace|*>`), from
  `{AgentModels:{refresh}}`, saved with `Workbench.SetConversationRuntime`, read back
  from `conversation_runtime[_revisions]`. A daemon refusal is shown verbatim.
  The daemon runs one short model check before it saves a pin.
- **Drafts** (`ChatInputDrafts.ts`) are kept per scope (workspace + profile) while
  you navigate, and a send clears only the revision it sent.

### Shell (`shell/`)

- **Sidebar**: live chips in the right-hand slot of a row (Home: `Working 12s` /
  amber `Needs you`; a task row: Needs you, Review, Working, Next; count badge on the
  `tasks` page), a Tasks section, derived by `shell/status.ts` from the snapshot (one
  mapping, the same stages as `TaskStage.swift`). A task opens `#/tasks?task=<id>`;
  the Tasks page reads that query (`taskHref()` in `shell/status.ts`).
- **Attention** (`shell/attention.ts`): the document title is `(N) Neko` while
  something needs you, and a toast names what newly needs you (never what was
  already waiting at load).
- **Status bar** (`shell/StatusBar.tsx`): machine hold/cap (`snapshot.machine`),
  what is waiting on you, shortcut hint.
- **`?`** opens the shortcut overlay; pages describe their own keys with
  `registerShortcuts(title, rows)` (`shell/shortcuts.tsx`).
- **Confirmations**: `await confirm({ title, description, action, destructive })`
  (`shell/confirm.tsx`). Anything destructive asks first (stop all work, remove a plugin).

### Inspector (`shell/inspector.tsx`)

A resizable panel on the right (`react-resizable-panels`, width remembered, Esc
closes). Pages use it two ways:

```tsx
// declarative: the content stays live with your state
<Inspector open={!!selected} onClose={() => setSelected(null)} title={selected?.title} subtitle="Task">
  <TaskDetail task={selected} />
</Inspector>

// imperative, from a page or a hook
const inspector = useInspector();      // { open({ title, subtitle, children }), close(), isOpen }
```

One inspector is open at a time. Plugins get `host.inspector.open/close`.

### ⌘K (`shell/CommandPalette.tsx`, `shell/commands.ts`)

Actions (new task, stop all work, approve what is waiting, shortcuts, first-run
tour, reload plugins, plus whatever the open page registers), Go to every page and
plugin page, Recent tasks (↵ opens, ⌘C or the button copies the task's ID),
appearance. Page authors:

```ts
useRegisterCommands([{ id: "tasks.new", title: "New task", icon: Plus, shortcut: "⌘N",
                       keywords: ["create"], group: "Actions", run: newTask }], []);
registerCommands([...])   // same, without a hook; returns the remover
```

`shell/actions.ts` has the shared implementations (`newTask`, `stopAllWork`,
`approveFirstDecision`, `copyText`).

### Customisations page (`#/customisations`, group "system"; `#/plugins` redirects)

Lists what is yours (name, manifest version, entry, status Running/Off/Error, the
last error from the daemon or from the module), switch to enable/disable, per-plugin
reload, copy folder, remove (with confirmation), details in the inspector, Reset to
original for each built-in you replaced, and **Change Neko**: a dialog sends Neko one
chat message (the constant `CHANGE_NEKO_PROMPT` in `pages/customisations/prompt.ts`,
shown in the dialog before it is sent) asking it to open a task that either writes a
page or style of yours or changes the app's own source. There is no Plugins sidebar
group: your pages sit in the normal groups. See `one-registry.md`.

Plugin layout, `<data dir>/plugins/<name>/` (`name`: letters, digits, `-`, `_`, at most 64):

```json
{ "title": "PR board", "description": "Open pull requests by stage", "version": "0.1.0", "entry": "index.js", "group": "work" }
```

`entry` is an ES module whose default export is `register(host)`; `host` is the object
Neko's own pages register with (`React`, `request`, `onEvent`, `registerPage`,
`registerCommands`, `inspector`, `ui`, `icons`, `storage`, `navigation`, `addSection`,
`addTab`; see `one-registry.md`). `group` is optional (`main`, `work`, `system`).
A `.disabled` file in the folder switches the plugin off.

Daemon endpoints (all behind the same Host / Origin / Sec-Fetch-Site / token gate as
`/api/request`; headers are authorised **before** any body is read):

| Endpoint | |
| --- | --- |
| `GET /api/plugins` | list: `id, title, entry, version (mtime), manifest_version, group, disabled, path, description, error` |
| `GET /api/plugin-folder` | `{ path }` |
| `GET\|PUT\|DELETE /api/storage/<id>[/<key>]` | `host.storage`: JSON values under `<data dir>/plugin-data/<id>/` (≤ 1 MiB, ≤ 256 keys, symlinks refused) |
| `GET /custom.css` · `GET /api/customisation` | `<data dir>/custom.css` (empty when absent) and where it and the plugins folder live, with the css mtime |
| `POST /api/plugins/<name>/enable` · `/disable` | writes/removes the `.disabled` marker; answers with the listing |
| `DELETE /api/plugins/<name>` | removes the folder (a symlinked folder is never followed); answers with the listing |
| `POST /api/attachments` | raw body (≤ 32 MB), `X-Neko-Filename` (percent-encoded); stored as `<data dir>/attachments/<sha256>[.ext]`; answers `{ name, reference, is_image }` |
| `GET /api/files?q=&workspace=` | up to five file names starting with `q` in the workspace's folders (Spotlight, the native composer's query) |

### First run (`onboarding/`)

Welcome, permissions (Full Disk Access guidance, Accessibility, the clipboard-history
switch is real), a workspace folder, the agent runtime, an optional import, done.
Shown when `GetOnboardingState` says it is not complete; "Skip setup" from any step
after the first; ↵ continues, Esc goes back. `/setup`, `/permissions` and ⌘K reopen
it (`openOnboarding(step)`). The test daemon marks first run finished unless started
with `NEKO_WEB_ONBOARDING=1`; `shell.mjs` resets it through `SetOnboardingComplete`.

### Verifying

`NEKO_WEB_ONLY=shell node scripts/web-verify.mjs` (see `scripts/verify/README.md`). The
test daemon runs no model, so `shell.mjs` answers `AgentModels` and a pinned
`SetConversationRuntime` itself (the real daemon runs a model check first) and patches
the snapshot to show tasks in every stage and a Mac under load; everything else is real.

## Spacing, measured (2026-10-22)

Source: hushbacks `origin/main` at `da20dfaeb` (apps/spa, packages/ui; the local
checkout was 50 commits behind and is not the reference). Colours stay Neko's;
headings stay plain sans.

**The finding.** The primitives are the same (`button` default is `h-9 sm:h-8` in
both), but hushbacks' app shell is `<SidebarProvider className="sky …">` and
`components/Sky/sky-materials.css` re-sizes every control inside it. Read from the
primitives alone, the two look alike; in the running app hushbacks' controls are
30-40% taller. Neko had no equivalent layer, so it rendered the raw 32/28/24 px sizes.

| Thing | hushbacks (running app) | Neko before |
| --- | --- | --- |
| Button `default` / `lg` | **40 px**, 16 px side padding (`--sky-control-height`) | 32 px, 11 px padding |
| Button `sm` | **36 px**, 14 px padding (`--sky-row-height`) | 28 px, 9 px padding |
| Button `xs` | **32 px**, 10 px padding | 24 px, 7 px padding |
| Button `xl` | 44 px, 18 px padding | 40 px |
| Icon button `icon` / `icon-sm` / `icon-xs` | 40 / 36 / 28 px | 32 / 28 / 24 px |
| Input, select trigger | 40 px, 14 px padding; `sm` trigger 36 px | 40 px; `sm` trigger 32 px |
| Tabs list / tab | 40 px / 34 px, 14 px tab padding | segmented chips ~32 px |
| Sidebar nav item | **40 px** (`sky-control`), `px-3`, `gap-3`, 16 px icon, `text-sm` | 32 px, `p-2`, `gap-2` |
| Sidebar recent rows | 36 px (`sky-row`), `px-3` | 32 px |
| Sidebar header / footer | `px-3 pt-3 pb-2`, 36 px logo row | `px-4 pt-4 pb-3` |
| Page column (`PageContainer`) | `max-w-[1200px]`, `px-6 md:px-10`, `pt-12 md:pt-16 pb-24` | `max-w-3xl`-ish, `px-6 py-10` |
| Settings panel row | `min-h-14` (56 px), `px-6 py-3`; panel header `px-6 pt-5 pb-4` | `px-5 py-4` |
| Settings stack gap | 12 px between panels, `space-y-8` between sections | `gap-8` / `gap-4` |
| Chat composer | `p-1.5`, `rounded-2xl`, textarea `min-h-15` (60 px), 15 px text, send button 36 px | `min-h-6` textarea, 14 px, 32 px send |
| Chat turn rhythm | `gap-2` inside a turn, 16 px+ between turns, user bubble `px-4 py-3` | already `mt-7`/`mt-10`, `px-4 py-3` (no change needed) |
| Tool/step line | `py-1`, 12.5 px | `min-h-7`, 12 px (no change needed) |
| Dialog / popover radius | 26 / 16 px (Sky) | 14-16 px (left alone: shape, not spacing) |
| Body | Inter, browser 16 px base, `text-sm` (14) used per component | 14 px base, 1.625 line height (left alone) |

What was deliberately not copied: the pill radius (24 px) and squircle corners, the
sky gradient/glass surfaces, and `text-[56px]` hero headings. Those are look, not
spacing, and the owner asked for spacing and density only.

**Superseded the same day** by "The shell, copied" below: the owner asked for
hushbacks' own code rather than an imitation, so the Sky layer itself (pills and
squircles included) now ships and the primitives are upstream again.

## The shell, copied (2026-10-22)

"Instead of rebuilding stuff on our own, copy paste from hushbacks." The app shell
is now hushbacks' own code from `apps/spa` @da20dfaeb, data swapped, colours on
Neko's tokens, no serif, italic emphasis or mono eyebrows. File-by-file table and
what was adapted: `web/src/ui/NOTICE.md` → "The app shell".

- `<body class="sky">` + `styles/sky-materials.css`: control sizes (40/36/32 px),
  24 px pill radius, pebble secondary buttons, well inputs, dialog/menu radii.
  Neko's `default` button stays blue.
- `shell/AppSidebar.tsx`: UnifiedAppSidebar — inset, icon-collapsible (collapses
  below 900 px), header mark / name / trigger, ⌘K row, live Tasks rail, pages under a
  hairline with the sliding pebble, Settings' sections revealed under Settings,
  your pages (in their groups, marked "yours"), theme segment, connection row.
- `shell/PageContainer.tsx`: every scrolling page sits in hushbacks' 1200 px column
  (`px-6 md:px-10`, `pt-12 md:pt-16 pb-24`); setup pages narrow it with `width`.
- Settings: hushbacks' settings route — breadcrumb bar, 52rem flat settings surface.
  The section list is in the sidebar (or the bar's pills when it is collapsed).
- A task opened full page (`#/tasks?task=…&view=full`) is hushbacks' builder frame
  (`work/TaskBuilder.tsx`): chat panel left, Changes / Plan / Details as fused view
  tabs on the stage, actions at the header end. Beside the list it is hushbacks'
  response details sheet (`work/TaskSheet.tsx`, see NOTICE.md).
- Tasks and Agent tree keep their own full-height frames (as hushbacks' builder does
  not use PageContainer); their headers are unchanged.
- A field with an icon inside it needs `ps-9!`: the Sky input rule sets the padding.

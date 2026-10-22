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

### Plugins page (`#/plugins`, group "system")

Lists what is installed (name, manifest version, entry, status Running/Off/Error, the
last error from the daemon or from the module), switch to enable/disable, per-plugin
reload, copy folder, remove (with confirmation), details in the inspector, and
**Build a plugin with Neko**: a dialog sends Neko one chat message (the constant
`BUILD_PLUGIN_PROMPT` in `pages/plugins/prompt.ts`, shown in the dialog before it is
sent) asking it to open a task that writes the plugin into the plugins folder.

Plugin layout, `<data dir>/plugins/<name>/` (`name`: letters, digits, `-`, `_`, at most 64):

```json
{ "title": "PR board", "description": "Open pull requests by stage", "version": "0.1.0", "entry": "index.js" }
```

`entry` is an ES module whose default export is `register(host)`; `host` has
`React`, `request`, `onEvent`, `registerPage`, `registerCommands` (⌘K) and `inspector`.
A `.disabled` file in the folder switches the plugin off.

Daemon endpoints (all behind the same Host / Origin / Sec-Fetch-Site / token gate as
`/api/request`; headers are authorised **before** any body is read):

| Endpoint | |
| --- | --- |
| `GET /api/plugins` | list: `id, title, entry, version (mtime), manifest_version, disabled, path, description, error` |
| `GET /api/plugin-folder` | `{ path }` |
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

# Intelligent UI: what ChatGPT/Codex and Claude do, and what Neko builds

Research from 2026-10-14 (official OpenAI, Anthropic and MCP docs; third-party
sources marked). Neko's bar: **every capability below at least as good as theirs**,
built on Neko's own design system (`docs/design/principles.md`).

## The facts that shape the work

- **MCP Apps is the shared widget standard.** It is SEP-1865 (Final), extension
  `io.modelcontextprotocol/ui`, spec `specification/draft/apps.mdx` in
  `github.com/modelcontextprotocol/ext-apps`. Claude, ChatGPT, VS Code, Goose and
  Copilot render it.
  - ChatGPT's Apps SDK now uses it: `_meta.ui.resourceUri`, plus `openai/outputTemplate`
    and `text/html+skybridge` as aliases, plus an optional `window.openai`.
  - One host implementation renders both families of widget.
- **ChatGPT dropped the side-by-side Canvas** (28 May 2026) for inline writing and
  code blocks. Codex now lives inside the ChatGPT desktop app (9 Jul 2026).
- **Claude draws "inline visuals"**: generated HTML/SVG charts, diagrams and widgets
  inside a reply. It also has persistent **artifacts**: docs, apps, dashboards with
  live data, versions and key-value storage.

## MCP Apps, as a host must implement it

- **UI resource.** Declared up front as `ui://…`, with MIME `text/html;profile=mcp-app`.
  A tool points at it with `_meta.ui.resourceUri`; the flat `ui/resourceUri` is
  deprecated.
- **Sandbox.** Render in a sandboxed web view whose content security policy comes from
  `_meta.ui.csp`: `connectDomains`, `resourceDomains`, `frameDomains`,
  `baseUriDomains`. **No network by default.** Permissions such as camera, microphone,
  geolocation and clipboard-write must be declared.
- **Bridge.** MCP JSON-RPC over `postMessage`.
  - UI→host: `ui/initialize`, `tools/call`, `resources/read`, `ui/message`,
    `ui/update-model-context`, `ui/open-link`, `ui/download-file`,
    `ui/request-display-mode`, `sampling/createMessage`.
  - UI→host notifications: `ui/notifications/size-changed`, `ui/notifications/initialized`.
  - Host→UI: `ui/notifications/host-context-changed`, `ui/notifications/tool-input` and
    `ui/notifications/tool-result`.
- **Display modes.** `inline`, `fullscreen` and `pip`. A tool's `visibility` is `model`,
  `app` or both.
  - The host may require consent before the UI calls a tool.
  - Every UI tool still returns usable plain text for hosts without the extension.
- **Data split.** `structuredContent` and `content` go to the model and the widget;
  `_meta` goes to the widget only.
- **ChatGPT's design rules.**
  - Inline cards have at most 2 actions and no nested scroll; carousels show 3–8 items.
  - Use system colours and fonts, with brand colour on accents only.
  - Meet WCAG AA.

## Capabilities to match (✓ = they have it)

| Capability | ChatGPT/Codex | Claude | Neko before this work |
|---|---|---|---|
| Diff review with line comments that feed the agent's next turn; revert per hunk/file; scopes | ✓ | ✓ accept/reject per file | read-only diff |
| Tool widgets (MCP Apps) inline, fullscreen and PiP | ✓ | ✓ | missing |
| Live HTML/SVG visuals and charts in replies | ✓ visualizations | ✓ inline visuals | bar chart only |
| Artifact viewer: HTML, PDF, images, docs; select an area to revise it; versions | ✓ | ✓ | missing |
| PR/CI panel: checks, "Fix" drafts a message, linked chats | ✓ | ✓ auto-fix CI | PR link only |
| Goal progress row; "needs you / ready" system notifications | ✓ | tasks pane | partial |
| Per-task terminal the agent can read, with environment actions | ✓ | ✓ | command log only |
| Preview browser where the agent checks its own change | ✓ in-app browser | ✓ | missing |
| Side chats that don't pollute the main thread | — | ✓ `/btw` | missing |
| Window capture ("appshot") as context | ✓ | — | missing |
| Syntax-highlighted code | ✓ | ✓ | missing |
| Interactive answers (choices, forms, plans) in agent tickets | ✓ | ✓ | dead buttons in tickets |

## Rules for building these in Neko

- **Use the shells and tokens** in `native/NekoKit/Sources/NekoNative/DesignSystem/`.
  `DesignSystemGuardTests` must pass, and `scripts/check.sh` must pass.
- **The sandbox is a security boundary.**
  - No network unless `_meta.ui.csp` allows it.
  - No file access.
  - No navigation away from the widget.
  - Tool calls initiated by a widget go through Neko's MCP bridge, with the same scope
    and consent rules as agent calls.
- **Prove visual work offscreen** (`scripts/render-previews.sh`, which takes
  `NEKO_RENDER_*` inputs). Never synthesize OS input, and never make a window key.
- **Make protocol additions additive** and `#[serde(default)]` in `neko-protocol`.

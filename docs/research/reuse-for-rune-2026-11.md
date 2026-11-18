# What Neko reuses for the Rune UI (2026-11-16)

Three research passes covered the Rune ecosystem, crates for screen content, and platform and agent plumbing. Licences come from crates.io and the projects' LICENSE files as of 2026-11-16. **Rule:** MIT, Apache, BSD or Zlib only. MPL only as an unmodified dependency. Never GPL, LGPL or AGPL. Builders of each phase (docs/decisions/0002) must check this list before writing anything themselves.

## Already there: use it, don't add a crate

| Need | Provided by |
|---|---|
| Chat markdown (tables, code, diff and mermaid fences), sidebar/session list (5k virtual rows), SplitView, tab strip, Tree/FileTree (50k), Table (100k), DiffView, Combobox, Menu, Popover, Dialog, Toast, Select, Switch, Segmented, Progress, Accordion, Tooltip, Keybind, rich editor, shortcut recorder | `rune-kit` (MIT) |
| Syntax highlighting, 80 languages | `rune-highlights` |
| Springs and layout animation | `rune-motion` (effects: `rune-fx`, optional) |
| Notifications (macOS, Windows) | `rune-notify` |
| Window frame, relaunch | `rune-mac` |
| Global hotkey | Rune's shortcuts module (Carbon), or Neko's `global-hotkey` (bump 0.8 → 0.10) |
| Single instance, URL handling | Rune `open_url/instance.rs` (verify) |
| Live design tuning, process watch | `rune-knobs`, `procwatch` |
| Theme base | `rune-theme-opencode` (swap tokens) or `Kit::from_palette` |
| Markdown parse | `pulldown-cmark` (in kit) |
| SVG | `resvg` (in kit; 0.45 vs latest 0.50) |
| MCP | `rmcp` (in Neko; bump `=3.4.1` → 3.10.x) |
| SQLite, keychain, accessibility, objc2 | already in Neko |

## Add these (★ = pick)

| Area | Crate | Licence | Plug-in | Effort |
|---|---|---|---|---|
| Embedded web view (HTML artifacts, JS plugins, Excalidraw) | ★ `wry` 0.57 (`build_as_child`) | Apache/MIT | child of the Rune window via raw handle; macOS, Windows, Linux X11 | M; check Rune's handle and z-order first |
| Diffs: side by side, word level | ★ `similar` | Apache-2.0 | logic → DiffView | ½ day |
| Agent shell terminal | ★ `alacritty_terminal` (`vt100` for read-only) | Apache-2.0 (MIT) | grid → text runs + rects | 3–5 days (2) |
| 3D beyond GLB | ★ `stl_io`, `tobj`, `glam`, plus the spike's Compositor pass (wgpu backend, not Metal) | MIT/Apache | shared wgpu device | 1–2 days |
| Sketch board | ★ `roughr`, `kurbo`, `rstar` + our own Excalidraw-JSON serde model | MIT/Apache | shapes | 1–2 weeks for a useful board; or keep Excalidraw in `wry` |
| Images, GIF, WebP | ★ `image`, `gif`, `webp-animation` | MIT/Apache | `El::pixels` | 1 day |
| Huge CSV | ★ `csv` + offset index → virtual Table | MIT | logic | 1 day |
| PDF | ★ `hayro` (pure Rust); `pdfium-render` if selection/search needed (ships a binary) | Apache/MIT | `El::pixels` | spiked |
| ⌘K fuzzy | ★ `frizbee` | MIT | logic | ½ day |
| Dates, recurrence, .ics | ★ `jiff`, `rrule`, `icalendar` | MIT/Apache | logic | 1 day |
| Big documents | `ropey` | MIT/Apache | logic | only if needed |
| Audio | `rodio` | MIT/Apache | — | low |
| Launch at login | ★ `smappservice-rs` or `objc2-service-management`; `auto-launch` cross-platform | MIT | — | small |
| Status item (if Rune's macOS tray is missing) | `tray-icon` + `muda` | MIT/Apache | — | small |
| Calendar and Reminders from Rust | ★ `objc2-event-kit` | Zlib/MIT/Apache | replaces the Swift bridge later | M |
| Packaging, dmg/msi/deb | ★ `cargo-packager` (`rcodesign` as a CLI only, MPL) | MIT/Apache | build | M |
| Browser use with a live view | ~~`chromiumoxide`~~ → `tungstenite` + a 7-method watch-only CDP client (2026-11-18: chromiumoxide compiles the whole protocol; too heavy for what a watch needs) | MIT/Apache | Rune only | S |
| Computer use, cross-platform | keep Peekaboo (MIT) on macOS; ★ `xcap` + `enigo` + `accessibility-sys`/`uiautomation`/`atspi` | Apache/MIT | daemon | M |
| Rich clipboard | `arboard` (`clipboard-rs` for watching) | MIT/Apache | — | small |
| Memory search | ★ `tantivy`, `sqlite-vec` (alpha), `fastembed` | MIT/Apache | daemon | M |
| One protocol for all agent CLIs | `agent-client-protocol` | Apache-2.0 | daemon | L; check which CLIs speak ACP |
| Direct model calls (if ever) | `genai` | MIT/Apache | daemon | — |

## Build ourselves (nothing good exists)

- **Streaming markdown for agent replies.** Re-parse only the open tail block and cache the finished ones. About 1 day.
- **Charts** (Usage, metrics). Draw on kurbo and Rune shapes. 2–3 days; `plotters` is stale.
- **Calendar grid, kanban board, tool-call cards, permission prompts.** Not in rune-kit. Use OpenCode's `packages/ui` (MIT, TypeScript) as the design reference; rune-kit already mirrors its names.
- **Video.** No clean MIT path. Use AVFoundation through a small native bridge, or skip it.

## Reference apps (read, don't copy blindly)

- `parkers0405/neoism`: MIT, a Rust agent workspace with agent, session, palette, tabs, tree, settings and terminal. The closest match to Neko.
- `kitlangton/psychopomp`: MIT, by a Rune author; rune-kit with markdown and mermaid.
- `anomalyco/hex`: MIT, a settings screen.
- `anomalyco/isocast`: Apache, a screen recorder.
- The OpenCode desktop recreation shown in Rune's README is not public.

## Never use

- **AGPL:** `mupdf`.
- **GPL:** Zed's `rope`, `text`, `markdown`, `fuzzy`, `terminal`, `language`; `ffmpeg-next` builds.
- **LGPL:** `user-notify`.
- **Copyleft, avoid:** `freenet/river` (LGPL) and `hazel` (FSL).
- **Doesn't fit:** `three-d` uses OpenGL, so it can't share Rune's wgpu device.
- **Stale:** `rend3`, `rdev`, `single-instance`, `anthropic`.

## Watch

- **objc2 versions:** Rune pins 0.5.2 and Neko uses 0.6, so the build will contain duplicates.
- **resvg:** 0.45 vs 0.50.
- **Pre-release crates:** `hayro`, `roughr`, `sqlite-vec`, `notify` 9, `ropey` 2.
- **`wry` on Linux:** Wayland needs a GTK window, which Rune's winit window isn't.

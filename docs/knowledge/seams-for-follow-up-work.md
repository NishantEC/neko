## Seams for follow-up work

- **Onboarding UI**: built — see "Onboarding" above.
- **Clipboard history**: built — capture, storage, and restore all live in
  the daemon; see "Clipboard history" above. Onboarding's steps 06-07 own the
  *ask* (`clipboard_history_enabled` daemon setting). The two-column
  detail-pane mode (`PANEL_WIDTH_WITH_DETAIL_PX`, screen 12) is now also
  built — see "Commands and modes" below. Still open: image capture (seam
  documented in `clipboard.rs`'s module doc comment, and in "Commands and
  modes"'s own note on what an `Image` `SearchItem`/detail-pane variant
  would cost). No macOS permission prompt was observed gating general
  pasteboard reads on the verification machine (unlike Accessibility,
  there's no `AXIsProcessTrusted`-equivalent "is trusted" API for the
  pasteboard) — `read_current`'s SAFETY comment covers the "detect and
  degrade" reasoning for the day one shows up: a blocked read and an empty
  one both come back as `None`/`nil`, so treating `None` as "nothing to
  capture this tick" already is the honest degrade path, not a placeholder
  for a request flow still to build.
- **Commands and modes**: built — see "Commands and modes" above. One
  command exists (Clipboard History); a second command's own accounting
  (what it has to implement vs. what it gets for free) is in
  `crates/neko/src/modes.rs`'s module doc comment. Still open: the mockup's
  "All Types" filter control (no real filtering logic exists to back it —
  see "Commands and modes"), real window-scoped visual evidence and warm
  summon latency / daemon idle memory re-measurement (this task's client
  could not be launched — the captain was using this machine), and image
  entries in the clipboard-history detail pane (a real scope increase: blob
  storage, thumbnails, size bounds, a new `SearchItem`/`Icon` shape — a
  separate captain decision per the launch brief, not attempted here).
- **Dynamic window resize**: partially built — see "v1 simplification"
  below for the still-true per-keystroke case. A mode transition's own
  width change is real but, since "Mode view resize seam" above, is no
  longer a window resize at all — the real `NSWindow` is fixed at
  `PANEL_WIDTH_WITH_DETAIL_PX` for the process's whole lifetime; only the
  panel `div`'s own centering and the native backdrop's frame change.
- **`Provider` abstraction**: built — see "Provider abstraction" above.
  `agent.rs`'s old `AgentProvider` placeholder is deleted, superseded by
  this. Agent capability, whenever it's built, is `impl Provider` plus one
  line in `AppState::new` — no protocol or panel changes needed, per that
  section's "registering a fourth provider" accounting.
- **File search**: built as the provider abstraction's proof — see
  "Provider abstraction" above, "File search" subsection. Still open:
  configurable scope (currently fixed to `~/Documents`/`~/Desktop`/
  `~/Downloads`, no settings UI — the seam mirrors `clipboard_history_enabled`'s
  existing daemon-setting pattern) and real per-file icons (painted
  `Glyph::File`/`Glyph::Folder` for now — deliberately not a second
  icon-extraction path; wire through `icons::ensure_cached_icon` in a
  background pass once the parallel `neko-icon-cache` task's own fixes are
  in).
- **System Settings pane search**: built — see "Provider abstraction"
  above, "System Settings pane search" subsection. Still open: real
  per-pane icons (shares the System Settings app icon for now, same
  deliberate scope cut as file search's painted glyphs — Apple's private
  iconography stack, not `NSWorkspace.iconForFile`, is what would be
  needed) and multi-word queries ("keyboard shortcuts") — a `fuzzy_score`
  limitation shared by every provider, not specific to this one.
- **Native window material**: built — see "Window material" above. Still
  open: a real screenshot proving live compositing/legibility against a
  busy backdrop, blocked on this machine's standing capture-safety rule
  (same section) rather than on any code gap.
- **Menu frost backdrop and the results-list edge fade**: built — see "Menu
  frost backdrop and the results-list edge fade" above,
  `docs/evidence/menu-frost-and-edge-fade-report.md`. Still open, and the
  concrete next step now that `main` depends on a `gpui` fork that ships
  the real primitive: build the `window.paint_backdrop_blur` version of the
  menu frost (`crates/gpui/src/window.rs:3992`) and compare it head-to-head
  against this task's native-AppKit sibling-below material, keeping
  whichever reads closer to comet's own result; `gpui::EdgeFade`
  (`crates/gpui/src/window.rs:682`) could similarly replace `edge_fade.rs`'s
  hand-rolled gradient, lower-risk since neither side of that swap needs
  native bridging.
- **Two-phase search**: built — see "Two-phase search: results as you type"
  above. Still open: nothing on the latency itself, but two things worth
  knowing. A *second* deferred provider would work without any code change
  (`defers_for`/`search_cancellable` are per-provider and `allocate` is
  already provider-count-agnostic), but has never been exercised — the
  daemon would still emit exactly two frames, with both deferred providers
  in the second, rather than one frame each; splitting further is a real
  design choice nobody has had to make yet. And `merge_late_results`'s
  "decline the late section rather than displace the selection" rule
  currently drops those results until the next keystroke; a scrollable root
  list (see "v1 simplification" below — it is still budget-fit and
  non-scrolling) would remove the need for that trade entirely.
- **Themes**: built — see "Themes" above. Seventeen built-ins, a `Themes`
  command/mode with live preview, daemon-persisted. Still open: per-theme
  geometry (Sherbet from `data/neko-cozy-theme` wants radii 16/8/6 → 22/12/8,
  which the current contract forbids on purpose — a theme is colour and surface
  only), gradient palettes (Ember and Catnap ship as flat fills; their
  `--grad-from`/`--grad-to` tokens are unused, since a `linear_gradient` on the
  panel is a rendering change rather than a colour one), Nightlight's
  per-section hues, user-supplied palettes (would need a format, validation,
  and an answer for "what happens when a loaded theme fails contrast" —
  `themes_all_pass_wcag_aa` only gates compiled-in ones), and a window-scoped
  capture of *onboarding* in a non-default theme (it compiles and reads the
  same tokens, but was not re-screenshotted).
- **Preferences**: built — see "Preferences: neko's own settings, and the
  mode stack" above. Three settings (summon hotkey, launch at login, search
  folders). Still open: the clipboard-history toggle (deliberately left out
  of this pass though it is nearly free — one `RowSpec` over the existing
  `clipboard_history_enabled` key), a real folder picker (needs suppressing
  the click-outside dismissal while a native panel is open), a visual pass
  on the hotkey capture screen, and `SMAppService` once neko is packaged as
  a `.app`.
- **Real icons**: built — see "Icons: real SVGs, and the one that stays
  painted" above. `assets::NekoAssets` is now this app's `AssetSource`, so
  **any future asset — an icon, a bundled font, an image — has a home**;
  adding one is a row in `ICONS` and a constant, not a pipeline. Still open:
  real per-file and per-System-Settings-pane icons (both still deliberate
  scope cuts for the *daemon-side* reasons in "File search" and "System
  Settings pane search" above — extraction cost and Apple's private
  iconography stack, neither of which this change touches); a light-theme
  capture; a latency/memory measurement of the sprite-atlas cost (argued nil
  after first paint from reading `paint_svg`, never benchmarked); and
  `file`/`folder`/`link` never being seen on screen (`docs/evidence/
  icons-svg-report.md` §6). One thing worth knowing before reaching for
  `svg()` elsewhere: it is an **alpha mask**, so anything genuinely
  multi-colour (`Glyph::Palette`) has to stay painted.
- **Agents**: seeing them is built (see "Agents: what is running right now"),
  and **starting** them now is too (see "Starting an agent: the New Agent
  command"). Still open: reaching a directory Paseo has never seen (the mode
  offers registered projects only), choosing a different tool or model than the
  one last used there, live confirmation of the `PASEO_AGENT_ID`/
  `PASEO_WORKSPACE_ID` environment strip (it would cost a second real spawn),
  any on-screen evidence of the mode, and the ~1–3s the CLI's own Electron boot
  puts between Enter and the panel closing — a "starting…" tell, or Paseo's own
  local RPC port, would each remove it.
- **Dragging the panel**: built — see "Dragging the panel: snapping and
  highlighted guides" above. Still open, in order of how likely each is to
  matter: **nobody has performed a real drag** (synthetic input is forbidden
  here, so the live behaviour rests on a source-level argument plus
  `snap.rs`'s own tests); the cross-display path (close the overlay, reopen
  it sized to the new screen) has never run, this machine having one display;
  the muted guide weight has never been photographed, needing two targets
  within 16pt on one axis; the dragged position is deliberately **not**
  persisted, since `reposition_to_cursor_display` resets it on the next
  summon; and the ~27ms overlay open could be removed by keeping one window
  alive between drags, which is exactly what "torn down on mouse-up, every
  path" currently forbids.
- **A real menu-bar `NSStatusItem`**: see "Onboarding" above — GPUI 0.2.2 has
  no usable status-item API; this is raw AppKit bridging, its own task.
- **Text field selection and paste**: built — see "Text field editing
  shortcuts" above. Still open: mouse selection (click-drag) and IME
  composition (marked text) — both real, separate pieces of work this
  task's own launch brief scoped out (single-line, standard-shortcut-set
  only, no general text editor).
- **`CORE_SERVICES_ALLOWED_APPS`'s 5-name allowlist** (`apps.rs`, see
  "Application discovery" above) is pinned to this machine's OS build. A
  future macOS release could rename, remove, or add a loose bundle in
  `/System/Library/CoreServices` that a person would want launchable; no
  static signal was found this task could build a version-proof general
  rule on instead (the investigation, and why each candidate signal failed,
  is in that constant's own doc comment) — re-verify the list by hand after
  any major OS upgrade, the same way `docs/evidence/cargo-license.txt`/
  `cargo-tree.txt` get re-verified after a dependency bump.
- **Multi-display, disconnected-daemon, and Spaces window behavior**: built
  — see "Window placement, disconnection, and Spaces" above. Still open:
  real evidence on an actual two-display setup (this task's sandbox only had
  one physical display — the math and the live single-display execution
  path are both proven; the cross-display *visual* isn't).
- **The `windowDidBecomeKey:` deadlock**: fixed and empirically re-verified
  — see "A known, upstream-fixed-but-unreleased deadlock in the real summon
  path" above, "Resolved 2026-08-20". **The dependency decision itself is
  made** (`fm/neko-gpui-fork-migration`, "The GPUI dependency decision") —
  the fork, accepting the GPL-3.0 exposure. Still open: whether the
  originally-hypothesized ~4GB-per-summon memory growth is real under
  sustained, realistic load (re-verification so far was 3 short cycles, not
  a sustained-load test) and whether it explains the captain's original
  19.96GB report (still not provable from available evidence either way).
  **A new, real cost surfaced instead**: warm summon latency regressed
  roughly 6–8x on the fork (`AGENTS.md`'s own "Summon latency" section,
  `docs/evidence/gpui-fork-migration-report.md` §8) — not root-caused, a
  real follow-up.

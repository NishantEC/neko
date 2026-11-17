# 0002. Rune for everything: a native client beside the web UI

Status: **accepted 2026-11-16 (direction, layout and phases); P0 built the same day.**
Supersedes [0001](0001-one-interface.md) where they conflict (see "What 0001 still says").
The layout below is the owner's decision of 2026-11-16 and **replaces tabs**.

## Context

Neko has had three interfaces over one daemon and 0001 chose one: the React web UI inside a thin
Swift shell. The owner then chose where the interface goes next: **Rune** (MIT, a GPU-rendered Rust
UI framework by Anomaly, `github.com/anomalyco/rune`), on macOS first and cross-platform later.
Three spikes on the branch `spike/rune` (all at Rune `4ddf696`) measured whether it can carry
Neko's screens:

| Question | Result |
| --- | --- |
| **Chat at Neko's scale** (500-task list, a 4,000-word reply streamed at 60 tokens/s) | Frame interval p50 17.6 ms, p95 25.6 ms while streaming; updates 3.5 ms p50. Isolated 190 ms stalls three or four times a run, cause unknown. Idle 58.6 MB; one long reply peaks near 245 MB and settles at 123 MB; a retained 4,000-word message costs about 7.8 MB |
| **Real daemon** | Over the Unix socket with `neko_protocol::Frame`, 25 tasks loaded in 6 ms (the HTTP route answers 401 without the web session) |
| **A web view in a Rune window** | A `WKWebView` child composites above Rune's surface, matches its layout rect and takes clicks, focus and IME. Rune menus draw under it, so it must be hidden while a menu or overlay is open (shown to work) |
| **Overlays** (agent pointer, activity capsule, launcher) | All pass. Summon latency 4.5 to 8.4 ms median. Idle 106 to 109 MB; a full-display transparent surface costs about 70 MB at rest and up to 240 MB more while animating |
| **Canvas** | 3D draws on Rune's own wgpu device at 0.21 ms GPU for 48k triangles (needs the wgpu backend, not Metal). A sketch board is about 460 lines for the basics and Excalidraw parity is XL. Luau plugins load, hot-reload in 0.30 ms and are sandboxed, but only host-built nodes can be placed (no viewer/fence registration, no JS libraries). PDF through `hayro` renders a page in 1 to 3 ms |
| **Churn** | `dev` took 753 commits in 7 days; at a week-old revision none of the five patch targets and not the `tooling` feature existed. Rune takes no pull requests |

## Decision

The whole UI moves to Rune, on macOS first. Rune screens are built **beside** the React web UI, not
instead of it:

- **`web/` stays the default and keeps working** until each Rune screen reaches parity. Nothing in
  the installed app, the installer or the self-updater changes in P0.
- **Screen by screen, a parity rule.** A Rune screen replaces its web counterpart only when it does
  everything the web screen does today (the features, the states, keyboard use, the calm rules of
  DESIGN.md) and a headless test says so. Until then the Rune screen says "coming" and offers the
  web UI at the same place. The default flips once, in P5, not screen by screen.
- **The daemon stays the only writer.** The Rune client reads the same snapshot the web UI holds and
  (from P1) sends the same requests the web UI sends. It never opens the database.
- **HTML artifacts, JS plugins and Excalidraw** live in an embedded web view inside Rune (P3);
  3D, PDF, images and CSV are native; native plugins are Luau.
- **What to reuse instead of building** is in
  [docs/research/reuse-for-rune-2026-11.md](../research/reuse-for-rune-2026-11.md) (`rune-kit`
  components, `wry` for the web view, `frizbee` for fuzzy search, and a licence never-use list: no
  GPL, LGPL or AGPL). Builders of every phase read it first.

## The layout (owner, 2026-11-16): four regions, no tabs

Left to right in one window:

1. **Sessions column.** Every agent run or task across workspaces, grouped by state (Needs you,
   Running, Done today), each group collapsible with a count. A row is one line: status glyph,
   faint id, title, quiet meta at the right. "+ New" at the foot of the list. Under a quiet divider:
   Calendar, Routines and Settings, and a "More" menu for other full views (Usage, plugin pages).
2. **Session view** in the middle. One line on top (title, runtime and goal), the agent's turns
   below, a composer at the bottom with a steer-mode menu (steer now, after this turn, interrupt
   and restart). A full view chosen from the column's foot replaces this region.
3. **Context panel**, collapsible, closed when there is nothing to show. It shows what the session
   produces, one viewer at a time: Diff, Files, Terminal, Browser (live), Screen (live), Canvas.
4. **Neko column**, always there, collapsible to an icon. The super agent: a one-line crew status
   ("3 agents working · 1 needs you"), the Inbox, the digest and an ask box.

The edges between regions drag to resize, each within a range; dragging the context panel or the
Neko column narrower than half its minimum collapses it. The session keeps at least 320 points: in a
small window the context panel narrows first, then Neko collapses to its icon, then the context
panel hides (widening the window brings them back). **DESIGN.md §1, "One window, opened as tabs", is superseded for the
Rune app**: tabs came from the 2026-11-05 study of a tabbed app, and the owner has decided against
them. They stay true of the web UI while it exists. DESIGN.md is rewritten for the Rune app in P1;
until then its colour, type, status-glyph and calm-page rules (§2, §3, §6, §7, §8 rules 1 to 4 and
7 to 13) carry over and the Rune theme implements them. The rail and the six places (§8 rule 5)
do not.

## Plugin slots

The same four slots serve built-in features and third-party ones, so Neko's own screens are written
as plugins would be. Native plugins are Luau (`rune-luau`); web plugins run in the embedded web
view (`wry`, P3).

- **(a) Context-panel viewers, by kind.** A plugin registers a viewer for an artifact or output
  kind (a diff, a table, a 3D model, a PDF, a sketch) and the panel shows it for sessions that
  produce one. Built-ins: the six above.
- **(b) Full views under the sessions column.** A plugin adds a page that replaces the session view
  and is listed in "More". Built-ins: Calendar, Routines, Settings and the pages in "More".
- **(c) Session tools and actions.** A plugin adds actions to a session (a button in its top line,
  an entry in its menu, a steer mode, a slash command in the composer).
- **(d) Neko cards.** A plugin adds a card to the Neko column (a digest block, a metric, a
  question with a default).

**Built 2026-11-17 (slots (b) and (d) only):** cards and views from installed, approved packs, see
[the knowledge note](../knowledge/luau-plugins.md). Slots (a) and (c) are not built.

The spike's gap applies: stock Luau UI places only host-built nodes, with no viewer or fence
registration. Slot (a) needs that registration added on Neko's side before plugins can use it.

## Phases

0. **P0, the foundation (done 2026-11-16).** `apps/neko-rune/`: its own Cargo workspace with the
   patch table Rune requires; Rune pinned by git rev with a bump script; the read-only daemon
   link with a live snapshot, reconnect with backoff and calm errors; the Neko theme (light and
   dark, following the system) and the status glyph set; the four-region shell with live sessions
   and counts and placeholders elsewhere; headless snapshots in light and dark, including one with
   live rows from the test harness daemon (`docs/design/rune/`); `run.sh` to run it beside the
   installed app.
1. **P1, the core (built 2026-11-16, see [the knowledge note](../knowledge/rune-p1-the-core.md)).** The session view (turns, streaming chat, the composer sends, steer modes), the
   Neko column (Inbox, digest, ask), the sessions column's actions (new, approve, answer) and
   DESIGN.md for Rune. Parity with Home, Inbox, Tasks (list and board), task detail and chat.
2. **P2, the rest of the pages (built 2026-11-16, see [the knowledge note](../knowledge/rune-p2-the-pages.md)).** Calendar, Routines, Settings and the pages in "More" (Usage, Workspaces, Plugins, and a link to the web UI).
3. **P3, the context panel and plugins** (P3a built 2026-11-16, see [the knowledge note](../knowledge/rune-p3a-the-context-panel.md): the panel, Diff, Files, Terminal, Canvas, the viewer registry and the native viewers, on the wgpu backend; Browser, Screen (live) and Luau plugins are still to do; the web view, plugin pages, layout pressure and the durable artifact link are P3b, built 2026-11-16, see [its note](../knowledge/rune-p3b-the-web-panel.md)). Diff, Files, Terminal, Browser and Screen (live), Canvas;
   an embedded web view for HTML artifacts, JS plugins and Excalidraw; native 3D, PDF, image and
   CSV viewers; Luau native plugins and the four slots above.
4. **P4, overlays.** The quick panel and ⌘K palette (`frizbee`), the activity capsule, the agent
   pointer, notifications, the menu bar item.
5. **P5, switch the default and ship.** The Rune app becomes the default; the web UI and the Swift
   shell shrink to what is left; the installer ships it. Not before every screen has reached
   parity.

## Risks and what we do about them

- **Upstream churn.** About 750 commits a week, and the build contract changed within one. Rune is
  pinned by git rev (`apps/neko-rune/RUNE_REV`, repeated on each `rev =` line of its `Cargo.toml`
  and checked by `scripts/bump-rune.sh --verify`). `scripts/bump-rune.sh` tries a newer rev:
  rewrites the pin and the patch table from Rune's own root `Cargo.toml`, checks the features we
  use still exist, builds, runs every test and reports what broke (restoring the old pin when it
  fails). Bump at most weekly and only on a green run. It moved P0 from `4ddf696` to `bc05f81`,
  which built unchanged.
- **No upstream pull requests.** A fix we need is a patch in `apps/neko-rune/patches/`, applied to a
  clean checkout of the pinned rev by the build (`scripts/patched-rune.sh`), and an issue upstream.
  The set stays small; none today.
- **Web views draw above Rune.** Rune menus and overlays draw under a native web view. The web view
  is hidden while a menu or overlay is open, as the spike showed works; the context panel's
  viewers that need a web view (P3) are built on that.
- **Memory.** Idle is 50 to 110 MB; full-screen overlays cost 70 to 240 MB. The footprint is
  watched (P0's four-region window measured a 62 MB footprint and 0% CPU over 20 s idle, against a
  test daemon; an earlier build showed about 300 MB in its first seconds while drawables settle), a long chat message costs about 8 MB so the session view bounds
  what it keeps, and overlays are made once and reused.
- **The Rune check is slower than the daemon's.** About 25 s warm with the dependency cache, about
  3 minutes cold. `scripts/check.sh` runs it only when that cache exists, or with `--rune`.
- **Two UIs to keep consistent.** Both read one daemon and one wire protocol. The Rune client
  reuses `neko-protocol`'s types by path, so the wire cannot drift from it.

## What 0001 still says

0001's inventory of what is Swift- or GPUI-only, its retirement of the GPUI client and of the
Swift screens that duplicate the web, and its duty list for the thin Swift shell all stand. What
this record changes is the destination: the one interface becomes Rune (after P5) instead of the
web UI. Until P5, 0001's rule that a new screen goes in `web/` holds unless the owner directs
otherwise; Rune screens are built to reach parity, not to add features the web lacks.

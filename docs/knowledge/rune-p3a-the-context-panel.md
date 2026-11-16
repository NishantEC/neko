## Rune P3a: the context panel and the viewer registry (2026-11-16)

Code: `apps/neko-rune/`. Decision and phases: `docs/decisions/0002-rune-for-everything.md`. Two daemon reads were added
(`TaskFolder`, `ArtifactFile`); the web UI is untouched and stays the default. Browser and Screen (live) and the web-view
viewers are P3b; the placeholder tab kinds for them are gone from the panel.

### What the panel is

The right-hand region (420 default, 320 min, 640 max, resizable). A 36 point header holds a segmented control over **only the
tabs that have content for the selected session**, in the order Canvas, Diff, Files, Terminal; an Expand button; a Close button.
Expand makes the panel fill the session region and its own (the sessions and Neko columns stay); Esc or the button restores it,
and closing the panel ends it.

| Tab | Has content when | Shows |
| --- | --- | --- |
| Canvas | the daemon showed an artifact for the session | one row per artifact (kind icon, title, "v3" faint, relative time), a version select "v3 ▾", the chosen artifact in its viewer |
| Diff | a run changed a file (`CheckpointRun::files > 0`) | the whole-task diff in the kit's `DiffView`, a file list on top (path 12.5 muted mono, +N −M at the right, a press scrolls to the file), a select for one run |
| Files | the session has a working folder (`TaskFolder`) | the folder in the kit's `FileTree`, changed files marked with a 6 point Amber dot, a file opens in its viewer inside the tab with a back arrow |
| Terminal | the session ran a shell command | the receipts the session view already shows as tool cards: `$ command` (12.5 mono), then the output through `vt100` (12 mono) |

**Auto-open** (`panel.rs`, `first_tab`, `App::panel_on_select`, `App::panel_sync`). Choosing a session opens the panel when any
tab has content and closes it when none has. The first tab is Canvas if an artifact was shown in the last 10 minutes, else Diff
if there is a run diff, else Terminal, else Files (the first tab with content when only an old artifact exists). A daemon
`ShowArtifact` event opens the panel on Canvas with that artifact, switching to the session that showed it before. The person's
close sticks for that session until a new `ShowArtifact` arrives; a new snapshot, leaving and coming back do not reopen it.
The person's tab choice stands while that tab still has content. The P1 and P2 demo scenes mark their sessions closed
(`scenes.rs::panel_closed`) so their committed pictures still show a closed panel.

### What belongs to a session (a finding)

The daemon links an artifact to a **workspace** and to the **Neko-chat reply** that was running when it was saved
(`Artifact::workspace_id`, `message_id`; `save_artifact` takes `neko_chat::pending_turn`), and to no task or session. A task's
own turns have ids `brief`, `a<n>`, … that no artifact names, so "the artifacts whose conversation is this session's" is empty
for every task. The client therefore lists what the daemon **showed** (`ShowArtifact`, pushed as a socket event to every
connection) while the session was the one open, and an artifact shown again later goes back to the session that first showed it
(`App::show_artifact`). The list lives for the run of the app; it is not guessed from a workspace, and nothing was added to the
wire to make it durable. **Follow-up needing a write-side change:** the lease token of an agent's `save_artifact` bridge call
already names its task (`crew_call`), so `SaveArtifact` could record `task_id`; then `Artifacts` could list a session's
artifacts after a restart.

### Daemon: two read requests

`neko-protocol`/`neko-daemon` (additive, read-only; the next installed-app reinstall carries them):

- `TaskFolder { task_id } -> TaskFolder { path: Option<String> }`: the task's `worktree` as a canonical absolute path when the
  task is not parked and the folder exists now; `None` otherwise; an unknown task is an error. (`server::task_folder`.)
- `ArtifactFile { id, version: Option<u32> } -> ArtifactFile { path, kind, size }`: `neko_core::artifacts::version_file`, which
  proves the version is a file inside the artifact's own folder (a planted symlink or an edited `meta.json` leads nowhere).
- The Canvas also reads `Artifact { id }` (existing) for the title, kind and versions.

All three are in `tests/allow_list.rs` (READS). Daemon tests: `a_task_folder_is_the_folder_that_exists_and_nothing_for_a_parked_or_unstarted_ticket`,
`an_artifact_file_is_the_stored_version_inside_the_artifacts_folder`. The client learns of `ShowArtifact` from the socket:
`Transport::ask` now hands every pushed event to a callback as it arrives (a held `WorkbenchIfChanged` can wait 20 s), and the
link emits `LinkEvent::ShowArtifact`.

### The client reads files itself, confined (`readers.rs`)

The daemon's word is data, not permission. `Confined::new(root)` canonicalizes the root; every read: refuses any `..`
component, canonicalizes the path (following links), requires the result inside the root (an absolute path elsewhere and a
symlink that leaves are refused), refuses anything under a `.git` (a folder or a worktree's file), requires a regular file and
reads at most 5 MB with `take(MAX + 1)` (a larger file is `Refused::TooLarge`: the tab says "Too large to preview" and offers
"Reveal in Finder", `open -R`). The roots are the task's folder and `<data dir>/artifacts`. A listing is breadth first (shallow
files first), at most 20,000 files, skips `.git`, and does not enter a symlinked folder. Nothing here writes
(`tests/allow_list.rs::the_panel_reads_files_only_through_the_confined_readers` also fails if any other module opens a file).
Tests: `readers.rs` (`..`, absolute outside, escaping symlink to a file and to a folder, `.git`, 5 MB edge, listing cap),
`tests/panel.rs` (the artifact store, the folder, through the app), `tests/harness.rs` (the real folder of a seeded task).
File reads run off the UI thread (`App::background`; inline in tests and headless runs).

### The viewer registry (plugin slot (a), `src/viewers/`)

`ViewerSpec { id, title, kinds, priority, build: fn(&ViewerInput, &mut ViewerState) -> El }`, `register`, `resolve(kind)`:
highest priority wins, ties go to the last registered; built-ins register at 0, later phases (Luau, web) at 10. Any text up to
2 MB that no viewer names falls to the code viewer (`resolve_for`); an unknown kind shows the fallback (name, size, "Reveal in
Finder", "Open in web UI"). `ViewerState` is kept by `(artifact id, version)` (a file is `("file:<task>:<path>", 0)`): camera,
page, zoom and scroll stay, and **what decoding made is released as soon as the viewer is not on screen**
(`ViewerStates::release_except`, called every frame by `App::panel_tick`); the loaded bytes (`Panel::open`, one at a time) go
with it, and a narrow window that hides the panel counts as off screen.

| id | kinds | how |
| --- | --- | --- |
| image | png jpg jpeg webp gif | `image` decode (a picture over 4096 on a side is scaled once) into `El::pixels_fit`; an animated GIF through the `gif` crate, frames set on the app's frame clock (`Pixels::set` wakes only the window showing it; `Tx::wake_at` for the next frame) |
| svg | svg | `resvg` 0.45 (the version Rune pins; one copy in the tree), rasterised about 1,200 px wide; no text (resvg's font features are off) |
| markdown | md markdown | the kit's `Markdown` |
| code | rs ts tsx js py go json toml yaml yml sh txt log css sql, any text ≤ 2 MB | the kit's `syntax` (rune-highlights, first 128 KiB coloured), line numbers, one virtual row per line |
| table | csv tsv | `csv` reads once into a record-offset index (8 bytes a record); only the rows on screen are parsed and built (`virtual_rows` with a version); header above the rows, footer "100,000 rows · 4 columns" |
| pdf | pdf | `hayro`; pages are rows; a page is drawn when it comes on screen at 2 pixels per shown point; at most 6 drawn pages (the ones furthest from the reader go first); counter "9 / 16 · 100%"; ⌘+ ⌘− ⌘0 zoom (0.5 to 3) |
| model3d | glb gltf stl obj | `gltf`, `stl_io`, `tobj`, `glam`; the spike's compositor pass on Rune's shared wgpu device (`model3d_gpu.rs`): matcap shading, two-sided triangles, grid floor at the model's lowest point, drag orbits, scroll zooms, double press fits, W toggles the wireframe |
| web-pending | html htm excalidraw | "Opens here in the next update" and "Open in web UI" (P3b replaces it) |

**One deviation, on purpose: the table viewer is not the kit's `Table`.** `Table::build(s, kit)` needs the window's `&App`; a
registered viewer is a function of its `ViewerInput` and `ViewerState` only (the spec's signature), so a plugin's viewer can stand
beside it. The table viewer uses the kit's `TableStyle` (same metrics, header font, rules, stripes) and `virtual_rows`, which is
what makes 100k rows cheap. If a later phase gives the registry the app state, swapping in `Table` is local to `table.rs`.

### The 3D backend decision (measured)

Rune's default macOS backend (Metal) has no depth buffer and panics on a depth-stencil pipeline, so the 3D pass needs
`GpuBackend::Wgpu`. The rule: if Wgpu's p95 frame CPU on the P1 streaming benchmark is within 20% of Metal's, the whole app
switches. Measured headless on the P1 commit (`--scene stream`: a 4,000-word reply streaming at 60 words a second beside the
22-session list; 600 frames at 60 fps, 1360x840 at 2x, perf profile, `RUNE_GPU_BACKEND` metal vs wgpu, six alternating runs):

| Backend | p50 frame CPU | p95 frame CPU (median of 6; range) |
| --- | --- | --- |
| Metal (default) | 0.762 ms | 1.06 ms (0.99 to 1.24) |
| Wgpu | 0.622 ms | 0.79 ms (0.79 to 0.88) |

Wgpu's p95 is 25% below Metal's, so it is within the 20% and **the whole app runs on Wgpu** (`lib.rs::run`: `prewarm_gpu_on`,
`.gpu_backend(Wgpu)`, `.compositor(Viewer3d)`). The offscreen readback route was not needed. With the panel and the compositor
in, the same scene measures p50 0.44 ms, p95 0.57 ms. **Not measured:** a real window (idle CPU and memory with the surface on
wgpu), because measuring one means opening a window and this work never takes focus; do it once by hand before P5.

### Numbers (perf build, headless, 300 frames at 1360x840 at 2x; RSS peak by `/usr/bin/time -l`)

| Scene | Peak RSS | Delta | Frame CPU p50 |
| --- | --- | --- | --- |
| Diff tab (no viewer; the baseline) | 49.0 MB | | 0.13 ms |
| Canvas, a 640x400 PNG | 52.2 MB | +3 MB | 0.10 ms |
| Canvas, a 100,000-row CSV (3.5 MB) | 70.5 MB | **+21.5 MB** | 0.14 ms |
| Canvas, a 16-page PDF, 6 pages drawn | 86.8 MB | **+37.8 MB** | 0.10 ms |
| Canvas, a 7,680-triangle GLB | 52.8 MB | +3.8 MB | 0.12 ms |

3D frame time, the spike's 48,000-triangle model (`--scene p3a-model3d-48k`): the 3D pass takes **0.042 ms of GPU** (p95
0.044), the compositor's CPU is 0.027 ms (p95 0.035), the whole frame 0.12 ms of CPU. The spike measured 0.21 ms of GPU for
the same triangle count with a plainer shader. The peak footprint of a headless run is about 275 MB at rest (the offscreen
surfaces); the deltas above are what the viewers add. A viewer's content is released when the panel closes (tested:
`a_viewer_holds_nothing_once_it_is_off_screen`); the peak RSS above is of a run that keeps it open.

### Other changes worth knowing

- **Mono is monospaced now.** `AppSpec::families` was `("Inter", "Inter")`, so every `Family::Mono` (commands, paths, output,
  the kit's code blocks) drew in Inter. It is `("Inter", "ui-monospace")` (SF Mono), which the spec's "12.5 mono" needs. A new
  type slot `mono_meta` (12.5) is in `theme.rs`, and `Neko::ansi(index)` maps the terminal's 16 colours to the theme's meanings
  (red Coral, green Mint, yellow Amber, blue Iris, cyan Sky, the greys) and gives xterm's cube for 16 to 255.
- **`vt100` and long output.** vt100's scrollback can only be viewed one screen back, so a command's output runs through a
  terminal big enough to hold it (rows = lines wrapped at 120 columns, at most 2,100) and every row is read. More than 2,000
  lines keep the first 1,000 and the last 1,000 with "…N lines hidden" between; more than 200 commands keep the newest 200 with
  "…N earlier commands hidden" above. The session view clips an event to 12 KB, so real output is far below the cap.
- **`gif` 0.14** (one copy with `image`'s); crates added: `vt100`, `image`, `gif`, `resvg`, `csv`, `hayro`, `gltf`, `stl_io`, `tobj`,
  `glam`, `wgpu`, `bytemuck`. All MIT or Apache-2.0 except `resvg`/`usvg` (MPL-2.0, unmodified, already Rune's).
- `samples.rs` generates a sample of every kind (PNG, GIF, SVG, Markdown, code, CSV of any length, PDF of any page count, a
  torus-knot GLB, STL, OBJ) in code, so there is no licence to carry and no binary in the repo.

### Testing and snapshots

`tests/panel.rs` (21 tests through the headless `Runtime`: the rules, each tab, each viewer, expand and Esc, release, what the
readers refuse), `tests/harness.rs` (two more against the isolated daemon: an artifact of every built-in kind saved over the
socket and shown, the 100k-row CSV and the 16-page PDF among them; the seeded task's real folder with changed files marked, a
symlink and `..` refused), unit tests in each viewer. Snapshots: `apps/neko-rune/scripts/snap.sh` renders the `p3a-*` scenes
(`NEKO_RUNE_SIZE=1360x840 NEKO_RUNE_SCALE=1 NEKO_RUNE_AT=0.5`), `snap-harness.sh` adds `p3a-harness-canvas-*` and
`p3a-harness-table-*` from the daemon's own store; all are in `docs/design/rune/p3a-*` in light and dark: auto-open
(`auto-canvas`, `auto-diff`, `auto-closed`), each tab (`diff`, `files`, `file-open`, `terminal`, `canvas`), each viewer (`gif`,
`svg`, `markdown`, `code`, `table` (100,000 rows, scrolled to the middle), `pdf` (16 pages, on page 9), `model3d` (wireframe on),
`model3d-solid`, `web`, `fallback`, `too-large`) and `expanded`.

### Not done in P3a

HTML and Excalidraw viewers (the placeholder is P3b's to replace), the live Browser and Screen tabs, Luau viewers, a PDF's text
selection and search, SVG text, a durable session-to-artifact link (see the finding above), a measurement in a real window, and
horizontal scrolling of a zoomed PDF beyond the panel's width (it scrolls, but only by the person's wheel).

# Rune canvas spike: 3D, sketch, Luau plugins, PDF

Rune at `4ddf696` with its patch table; stable Rust 1.97.1. Four apps in one Cargo workspace. Screenshots are in `shots/`.

## 1. 3D in Rune: works, zero-copy

The model is drawn on Rune's own wgpu device, in Rune's command buffer. An offscreen target (4x MSAA plus depth) is sampled by Rune's UI pass at an `El::effect` slot, so the frame never reaches the CPU. The test model is a generated 48,064-triangle GLB, loaded with `gltf` 1.4. Orbit, zoom, fit-to-view and hidden-line wireframe all work.

Measured headless (`--script`), 270 frames at 1800x1280 px. GPU time comes from timestamp queries on the 3D pass.

| Route | Triangles | GPU, 3D pass (mean) | Compositor CPU | `Runtime::frame` CPU |
| --- | --- | --- | --- | --- |
| Shared device | 48k | 0.21 ms (p95 0.33) | 0.049 ms | 0.118 ms |
| Shared device | 494k | 0.68 ms | 0.056 ms | 0.123 ms |
| Readback into `El::pixels` | 48k | not timed | 1.62 ms, blocking | 2.71 ms |

- An orbit event costs about 0.4 ms of work, against an 8.3 ms frame at 120 Hz.
- Rune's default macOS Metal backend panics on any depth-stencil pipeline (`gpu/metal/pipeline.rs:171`). A 3D compositor must opt into `GpuBackend::Wgpu`.
- Missing for three.js parity: STL, textures/PBR and compressed assets. Effort M, or L with textured PBR.

## 2. Sketch board: basics in ~1 hour, Excalidraw parity is XL

Built in about 460 lines: rectangle, ellipse, arrow, freehand, text in a real input field, select, move, delete, and JSON save and load. Two `Runtime` tests pass, and a scripted headless run draws, types and moves shapes.

- Easy: shapes, drag handlers, text fields with IME, headless screenshots.
- Hard or missing:
  - No text measurement from handlers, so text hit-boxes are estimated.
  - No text inside a `Shape`.
  - `Color` literals are linear, so intended dark grey renders mid-grey.
- Effort: a useful single-user board (pan and zoom, resize, undo, arrow binding) is L. Excalidraw parity is XL.

## 3. Plugins with rune-luau: pages and host calls work

What ran:
- A plugin folder added while the app ran was loaded without a restart.
- Hot reload on save took 0.30 ms and kept state.
- `host.request` ran off the UI thread via `Host::async_function`.

Sandbox, all tested:
- `io`, `os.execute`, `os.getenv`, `fetch` and HTTP are nil.
- Globals are read-only, and `require` is confined to the plugin folder.
- The 250 ms watchdog stopped an infinite loop, and the memory cap stopped an allocation loop. The app survived both.

Gaps:
- Stock `@rune/ui` has only `row col box text spacer virtualList each hex`. Input, image and canvas had to be host-written nodes.
- Against `docs/design/plugin-guide.md`:
  - No `registerViewer`/`registerFence` equivalent, which is how 3D, sketch and PDF plug in today. This is the biggest gap.
  - No JS library ecosystem.
  - Every `ui` kit component would be host Rust.
  - Rollback, `--tree`/`--snap` verification and stronger isolation are better than today's JS contract.

AI test: two fresh sub-agents, given only the plugin docs, each wrote a working plugin on the first try: a PR list with filter and selection, and a usage chart with image and counters. The docs were written for this spike, so they are tuned to it, and the sample is small.

## 4. PDF and HTML

- PDF: `hayro` 0.9 (MIT/Apache-2.0, pure Rust, CPU) renders about 1-3 ms a page at 2x. A real 16-page PDF with embedded fonts took 43 ms. Pages show via `El::pixels_fit`. Text selection, search and encrypted PDFs were not covered.
- HTML artifacts: Rune has no web view, so they need an embedded platform web view (see the chat spike).

## Top problems

1. A plugin can only place host-built nodes. New renderers (viewers, fences) and JS libraries have no path.
2. Custom 3D needs the wgpu backend, not the default Metal one. The cost of that for the rest of the UI is unmeasured.
3. Plugin text input breaks under `Script::view` memoization: a stale snapshot undoes keystrokes. The workaround is `render_uncached`, which re-enters the VM on every view.

Not measured: input-to-photon latency, three.js and Excalidraw baselines, STL loading.

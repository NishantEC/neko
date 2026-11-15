# Rune overlay spike: pointer overlay, capsule, launcher

Rune at `4ddf696`. Release build. Screenshots are in `screenshots/`.

## Pass/fail

| Part | Result |
|---|---|
| **Agent pointer overlay** | Pass. AppKit read-back: level 25, collection behaviour `0x40151` (all Spaces and full-screen), `ignoresMouse=true`, `canBecomeKey=false`, never key. The dot, label and six-dot spring trail move from stdin `x y label` lines with no view rebuild per move. |
| **Activity capsule** | Pass for window and layout. Same panel kind and flags, placed under the menu bar. Real Liquid Glass is not confirmed: a single-window capture shows only the neutral tint. |
| **Launcher** | Pass for the 200-item filter, Up/Down, Enter and the `toggle` command. The Carbon global hotkey registered without error, but an end-to-end key press was not proven (the synthetic chord did not fire). |

## Measurements (20 summons per run)

**Summon latency.** Time from the request until the window reports visible:

| Run | Median | p95 |
|---|---|---|
| 1 | 4.5 ms | 6.5 ms |
| 2 | 4.9 ms | 7.5 ms |
| 3 (noisy) | 8.4 ms | 13.2 ms |

Add up to 8.3 ms for scanout. Rune draws a hidden first frame at about 0.8 ms and a second about 255 ms later. Which frame a person sees first is not known: presentation timestamps were not measured.

**Memory and CPU:**

| State | Footprint |
|---|---|
| Idle | 106–109 MB (RSS about 73 MB) |
| After 20 summons | 110–111 MB |
| Pointer animating, peak | 348 MB |
| After animating, settled | 139 MB |

Idle CPU is about 0.25%.

**Build.** Cold 75.8 s with an empty target dir (211 crates). A no-op build takes 0.2 s and a one-file edit 28–39 s. The binary is 33.6 MB.

## Problems

1. **Observability.**
   - There is no frame hook for overlays.
   - There are no getters for level, Spaces behaviour or mouse transparency.
   - `OverlayHandle` has no `is_visible()`.

   These were read through objc2 (`src/diag.rs`) and frames were timed through a `Compositor`.
2. **Structure.**
   - Hooks are `fn` pointers, so the hotkey needs an `AppHandle` stored in state.
   - Each host has its own state type.
   - Overlays must hang off an `App`.
3. **Surface cost.** A full-display 2x transparent surface costs about 70 MB at rest and about 240 MB more while animating.

Also: the launcher panel takes keyboard focus when shown, which is expected for a launcher.

Not checked: real Spaces switching, click-through with real clicks, and multi-display.

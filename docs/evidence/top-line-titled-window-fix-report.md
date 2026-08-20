# The top line — AppKit's titled-window rim, removed

`fm/neko-top-line-titled`, on `cda3765`. Fixes the line across the top of the
summon panel the captain has now reported three times. Two prior tasks
attributed it wrongly (the native window shadow, then the transparent margin
left by the fixed-width window); both of those were real, separate defects
with real fixes, but neither was this line. `data/neko-truth-pass/report.md`
§2.4 root-caused it with four escalating single-variable tests; this task
applied the fix and verified everything else about the window survived it.

## 1. Cause

`main.rs` opens the summon window with `titlebar: None`. gpui's mac backend
(`gpui_macos/src/window.rs`, `MacWindow::open`) reads that as "no
`TitlebarOptions`" and takes the `else` branch:

```rust
} else {
    style_mask = NSWindowStyleMask::NSTitledWindowMask
        | NSWindowStyleMask::NSFullSizeContentViewWindowMask;
}
```

`titlebar: Some(..)` does not help — the *other* branch also sets
`NSTitledWindowMask`. **There is no `WindowOptions` value that produces an
untitled window**, which is why this is cleared natively after creation
rather than at it, contrary to the ordinary preference.

AppKit draws its own ~1pt top-edge highlight on a **titled** window, composited
above everything the app paints. Nothing in neko draws it — proven in §2.4 of
the truth-pass report by replacing the panel `div` with a flat opaque black
rect and still measuring it, and by measuring it identically under all three
material paths (Glass, `NEKO_FORCE_MATERIAL=popover`, `=opaque`).

## 2. The fix

`material::clear_titled_style_mask` clears **only** bit 0 on the real
`NSWindow`, through the same `raw-window-handle` walk `material.rs` and
`spaces.rs` already use. `material::verify_titled_cleared` reads the live mask
straight back and asserts the bit is gone *and* that
`NSNonactivatingPanelMask` (bit 7) and `NSFullSizeContentViewWindowMask`
(bit 15) both survive — the same "verified, not trusted" pattern
`verify_installed` / `verify_shadow_disabled` / `spaces::verify` establish.

Called first in `main.rs`'s window-init closure, **before** `material::install`:
`setStyleMask:` makes AppKit rebuild the window's frame view, so doing it first
means every native view installed afterwards goes into the final one, and the
shadow/Spaces/material readbacks below it all observe the settled window.

Both halves are logged on every launch, next to the existing readbacks:

```
neko: window style mask before: 0x8081
neko: window style mask after: 0x8080 — NSTitledWindowMask cleared (verified),
      non-activating panel and full-size content view intact
```

The pure bit arithmetic (`style_mask_without_titled`,
`style_mask_is_untitled_panel`) is split out and unit-tested against the real
observed `0x8081`, including the two "a `setStyleMask:` that dropped the wrong
bit" regressions.

## 3. Evidence

Release binaries, isolated `HOME`, empty `PATH` from a directory with no
`neko-daemon` sibling (so the client's spawn attempt fails cleanly and the real
daemon is never started), `NEKO_SHOW_ON_LAUNCH=1` with **no**
`NEKO_EVIDENCE_ACTIVATE` — `neko: key window false (at capture)` on both runs.
Window-scoped capture only (`screencapture -o -l<windowID>`).

Column read at the panel's horizontal centre, device pixels (2x backing scale):

| row | before | after |
|---|---|---|
| y=0 | **(66,66,66)** | (19,19,19) |
| y=1 | **(43,43,43)** | (19,19,19) |
| y=2..7 | (19,19,19) | (19,19,19) |

Every other edge reads a flat `(19,19,19)` on both runs — left, right and
bottom were never affected. 4× nearest-neighbour crops of the top edge:
`top-line-titled-before-4x.png`, `top-line-titled-after-4x.png`.

## 4. What survived, each checked rather than assumed

- **Rounded corners — kept.** The brief's own tie-break was "keep the corners";
  they are unchanged. Corner rounding here is drawn by GPUI's `.rounded()` and
  the material view's own `cornerRadius=16`, never by AppKit's titled frame.
  Alpha readback of the top-left 40×40 box: 254 fully/partly transparent pixels
  before, 213 after — the same radius with a slightly sharper antialias ramp,
  since AppKit's frame mask is no longer compounding with the material's own.
  All three sampled corners (TL/TR/BL) agree. 4× crops:
  `top-line-titled-corner-before-4x.png`, `top-line-titled-corner-after-4x.png`.
- **Spaces / full-screen reachability — kept.** `spaces::verify` reads back
  `bits=0x101` (`CanJoinAllSpaces | FullScreenAuxiliary`) on both binaries;
  collection behavior is a separate property from the style mask.
- **Non-activating panel — kept.** Bit 7 is present in the post-clear readback
  `0x8080`, asserted by `verify_titled_cleared` itself, and both evidence runs
  logged `key window false`. gpui's own window subclass overrides
  `canBecomeKeyWindow` to `YES` unconditionally, independent of style mask, so
  the real summon path is unaffected either way.
- **Disabled window shadow — kept.** `neko: native window shadow disabled
  (verified)` on the post-fix run, after the mask change.
- **Window material — kept.** `NSGlassEffectView at contentView.subviews()[0]
  … cornerRadius=16`, plus the menu overlay at `[1]`, both verified post-fix.

## 5. No performance or memory regression

`NEKO_BENCH=40` on each release binary, same isolated harness, back to back:

| | cold | warm mean | warm median | warm p90 | warm range | RSS at end |
|---|---|---|---|---|---|---|
| before | 32.51ms | 9.19ms | 8.82ms | 16.04ms | 0.96–16.56ms | 63.6 MB |
| after | 37.90ms | 9.02ms | 8.82ms | 16.23ms | 0.94–16.43ms | 62.1 MB |

Identical medians; means differ by less than the run-to-run noise this window's
~8ms `CVDisplayLink` tick cadence already produces (`AGENTS.md`, "Summon
latency"). The single cold sample is one measurement each and moves in both
directions across runs — an earlier 15-sample pair read 33.03ms before /
31.65ms after.

## 6. Not done

No synthetic input of any kind, no region or full-screen capture, no real
`neko-daemon` launched, no runtime window resize, no palette/geometry/spacing/
type change.

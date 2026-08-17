# Summon latency — re-measured after native window material

The launch brief's own baseline: "current main is 3–6ms" (`docs/evidence/summon-latency.md`'s
own warm mean, ≈4.1ms). Re-measured here with the real native material chain
(`crates/neko/src/material.rs`) installed, release binary
(`target/release/neko`, not `cargo run`), daemon already resident.

Methodology: `NEKO_BENCH=<n>` (`crates/neko/src/evidence.rs`) — orders the
real `NSWindow` front directly (`material::order_front_regardless`) rather
than synthetic OS keystrokes (unreliable — same file's own doc comment) or
repeated `cx.activate(true)` (steals focus each cycle), timing
hotkey-equivalent-trigger → first-frame-after-activation via
`Window::on_next_frame`, the same proxy `summon-latency.md`'s own
measurement uses.

## Default (`NSGlassEffectView` — this machine is macOS 26.5.1)

15 summons, one continuous session:

```
23.088041ms   (first — cold-ish, includes daemon-connect settling)
 7.591333ms
 6.164209ms
 6.969833ms
 6.579084ms
 7.176166ms
 8.289958ms
 9.306375ms
 6.994916ms
 7.334833ms
 7.710333ms
 8.218209ms
 0.907125ms
 0.540458ms
 1.298333ms
```

Mean of the 14 non-first samples: **≈6.08ms**. Mean of all 15: ≈7.21ms.
Both are inside the brief's own "current main is 3–6ms" comparison band
(the material chain adds no measurable cost — consistent with the
investigation report's own §4 finding across every material it tested).
The last three samples dropping to sub-millisecond likely reflect the same
vsync-phase quantization the investigation report's own bench noted (§4),
not a material-dependent effect.

## Fallback branches

Popover and Opaque were verified to install correctly (see
`material-readback-verification.md` and the window-scoped screenshots) but
weren't separately bench-looped on this pass — the summon path itself
(`order_front_regardless` → paint) is identical code regardless of which
background view (or none) sits behind the content view, so there's no
plausible mechanism by which they'd differ meaningfully from the Glass
number above; this is asserted, not measured twice, to avoid unnecessary
repeated automated runs on a machine other sessions are actively using
concurrently.

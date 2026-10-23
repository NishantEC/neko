## The shared pulse clock, and the repeating-animation rule

`motion::PulseClock` — the seam `motion.rs`'s own doc comment reserved, now
built for the LIVE agent badge. **It is the only sanctioned way to drive a
repeating animation in this app**; nothing in the catalog may repeat on its
own. The rule comes from comet's recorded incident: one
`with_animation(..).repeat()` element pinned a window at 120Hz and measured
36% CPU.

Three properties, all load-bearing:

1. **Ticks at `PULSE_INTERVAL` (80ms, 12.5Hz), not at frame rate.** Asserted
   by a test rather than left to review — a regression to frame rate here
   would be silent.
2. **Stops completely when unused**, and is driven **from `render`**, not
   from `run_search`. Only the render pass knows whether a live row was
   actually *painted* (`fit_within_budget` drops what does not fit) and
   whether the window is even on screen — which the panel is not, almost all
   of the time. A clock ticking behind a hidden window is the same defect in
   a slower disguise.
3. **Never starts under reduce-motion**; `intensity()` returns a fixed
   midpoint so callers never branch and a live row still reads as live
   without moving.

Measured: **resting CPU with the panel hidden, 0.52%** — unchanged from
before the clock existed, which is the number that matters. The badge
interpolates *alpha* rather than swapping colours, so it reads as one thing
brightening rather than two states flipping.

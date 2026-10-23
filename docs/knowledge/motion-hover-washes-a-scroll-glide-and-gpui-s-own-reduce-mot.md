## Motion: hover washes, a scroll glide, and gpui's own reduce-motion flag

Ported from comet's `crates/ui/src/motion.rs` (MIT), which pins the identical
gpui rev — see `components/vendor/MANIFEST.md` for what was taken, what was
declined, and the two things that had to change.

**gpui's `.hover()` cannot fade.** It applies its style on the frame the pointer
enters and removes it on the frame the pointer leaves, with no interpolation and
no hook to add one — so every one of this app's fourteen hover sites snapped.
`motion::HoverFades` is the wall-time tween that fixes it, and `HoverWash`
(`hover_bg`/`hover_text`/`hover_border`) is the one-line form each site uses.
**It replaces `.hover()` rather than sitting beside it**: an element carrying
both would snap to the end colour and then fade a second, invisible one
underneath.

Three parts are non-obvious and each is pinned by a test:

- **A reversal re-anchors at the current value**, not at an endpoint. A pointer
  leaving mid-fade must come back down from where the wash actually got to;
  re-anchoring at 1.0 would flash.
- **The blend is premultiplied.** Every wash here rises out of fully
  transparent, and a straight component mix interpolates the *hidden* channels
  too — a light wash over a dark panel visibly darkens on the way up.
- **The frame counter is a liveness stamp.** A result row unmounts on every
  keystroke and never receives its leave event, so without pruning the next
  element to reuse that key inherits a full-strength wash with nothing under the
  pointer. Going a whole frame unread is the only available proof it is gone.

**The counter is rate-limited (`MIN_TICK_INTERVAL`), which comet does not need
and this app does.** One process-wide store, two windows rendering
independently: two advances inside one real frame would let each window prune
the other's live entries. Rate-limiting makes the rule hold for any number of
surfaces without any of them knowing about the others.

**`set_target_at` is the render-safe form.** A hover arrives as an event; a
*selection* is re-derived every frame, and driving it through the event form
would re-anchor the fade every frame and freeze it one step from the start. The
Preferences tab fill crossfades through this. It is a crossfade and
`SELECTION_FADE` says so — sliding would need one indicator positioned by a
measured x, and nothing can measure tab widths before layout.

**Keyboard scrolling glides.** `ScrollHandle` moves instantly and has no
animated variant, so `motion::ScrollGlide` asks `scroll_to_item` where it *would*
land, puts the handle back, and walks it there — the destination comes from gpui,
never from geometry re-derived here. A glide **gives up its claim** when the
handle no longer reads back as what it last wrote: a wheel gesture or a fresh
search has overtaken it, and finishing would drag the view to a destination
nobody wants, the same rule a superseded search follows. `SCROLL_GLIDE` is fixed
duration over the whole distance, never percent-of-remaining, so one Down and ten
Downs land in the same beat.

**`main.rs` now calls `cx.set_reduce_motion(motion::system_reduce_motion())` at
startup.** gpui's `AnimationElement` already checks `App::reduce_motion` and
renders a single static frame, but **nothing in gpui populates it from the OS** —
it defaults to `false`. One call makes every `with_animation` in this app honour
the setting structurally, including any added later by somebody who never reads
`motion.rs`. The hand-threaded `reduced` flags stay for the helpers that skip
`with_animation` entirely, which is stronger than snapping: a skipped animation
schedules no frames at all.

**Measured, and this is the property that matters.** Idle CPU with the panel
visible: **0.42% mean / 0.40% median, identical to the pre-motion build** — both
tweens request frames only while something is moving. Across a driven
fifteen-row scroll burst CPU rises to ~14% and returns to **0.46%**, flat. Both
tweens are driven from `render` rather than from the event that starts them,
because that is the one place that runs exactly once per frame and only while
the window is on screen.

**Not photographed, and a still image could not show it**: a 150ms fade. The
evidence is the state-machine tests (arrival, reversal, reduced motion, pruning,
multi-surface ticking, settling, the premultiplied blend) plus the CPU
measurements above.

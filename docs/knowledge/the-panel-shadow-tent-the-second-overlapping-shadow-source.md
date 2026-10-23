## The panel shadow tent — the second overlapping shadow source

`fm/neko-panel-shadow-tent`, fixing exactly the gap the correction just
above describes: the captain reported the same "black tent" a third time,
on a binary that already contained `fm/neko-double-panel`'s fix. Full
investigation, the single-variable test, alpha-channel measurements, and
the verification methodology: `docs/evidence/panel-shadow-tent-fix-report.md`.
Summary:

**Root cause**: `panel::Root::render`'s panel `div` also carried
`.shadow_lg()` — GPUI's own box-shadow, drawn as real, blurred, low-alpha
black pixels in the same scene, entirely independent of the AppKit window
shadow the prior task disabled. It's been in this file since the original
spine commit (`8888e33`); for most of the project's life the real `NSWindow`
was exactly the panel's own width, so the blur had nowhere to spill and was
invisible by construction — the identical reason the AppKit shadow was
invisible before `235bf88`. Once the window became permanently
`PANEL_WIDTH_WITH_DETAIL_PX` while the root-list panel stayed the narrower
`PANEL_WIDTH_PX`, both shadow sources gained the same `theme::
PANEL_ROOT_INSET_PX` margin to bleed into. Disabling the AppKit shadow
removed one of the two; this task's own subject was the one still standing.

**Confirmed by a single-variable test, not assumed**: `.shadow_lg()`
commented out, nothing else changed, window-scoped captures of the empty-
query root panel over a dark backdrop, before/after, alpha channel read
directly (this window is genuinely transparent, so alpha `0` means nothing
painted there at all). Before: a soft gradient from `0` up to `~17/255`
(~6.7%) right next to the panel's own edge, filling roughly the outer
two-thirds of the 40pt margin. After: alpha is exactly `0` everywhere
outside the panel's own opaque fill — a hard, clean edge. Prediction stated
in advance, confirmed by measurement.

**The fix**: `.shadow_lg()` is removed outright, not shrunk. It was already
fully contained within the 40pt margin (never spilling past the window's
own edge) — the problem was never its size, it was that it painted
anything at all into a region every other decision in this codebase
already treats as strictly transparent (the native backdrop material is
deliberately narrowed to the panel's own width; the AppKit auto-shadow was
disabled for painting into this exact margin). The translucent Liquid
Glass material already reads as an elevated surface through real vibrancy
with no drawn shadow needed; the opaque fallback keeps its pre-existing
`.border_1()` for edge definition, which has no spill risk. Clipboard mode
(760pt panel, 0pt margin — `render_dismiss_margin`'s `margin_width` is
exactly `0` there) was never able to leak this shadow either way, before or
after; the fix is unconditional, so both modes are affected identically.

**Scope note added by `fm/neko-top-line-titled`**: this fix and
`fm/neko-double-panel`'s are both correct and both stand — between them they
removed the two shadow sources spilling into the margin. Neither, however,
was the **top line** the captain has separately reported three times, even
though the two were sometimes discussed as the same complaint: that line is
AppKit's titled-window rim on the panel's own top edge, still present on
binaries containing both fixes. See "The top line — AppKit's titled-window
rim" below.

**Not obtained this task**: a live window-scoped screenshot of clipboard
mode itself. `evidence.rs`'s only non-synthetic-input hook that can drive a
mode transition (`NEKO_SHOW_QUERY`+`NEKO_SHOW_CONFIRM`) holds the summon
window active several seconds longer than a plain `NEKO_SHOW_ON_LAUNCH`
capture does, and on this shared, multi-agent machine that longer exposure
reliably lost real window activation to contention before capture —
confirmed directly with a temporary, reverted diagnostic
(`window.is_window_active()` read `false` at capture time on every
attempt, including immediately after an explicit re-`activate_window()`),
and separately confirmed the underlying application/search state was
completely correct throughout the same attempts (a second temporary,
reverted diagnostic on the daemon's own `Response::Search`). Ruled out
before concluding this: display sleep, timer starvation, and the backdrop
window's presence. The evidence report above has the full account, including
what was tried; the code-level argument above (a provably `0`-width margin
in that mode) stands in for the missing screenshot. `evidence.rs` itself
was not changed by this task.

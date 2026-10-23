## The bottom corners the scroll fade squared off

Reported as "the corners aren't rounded at the bottom, I think it's the blur
and the fade" — the second half of that was exactly right, and the
measurement named it in one number.

**gpui's content mask is a rectangle.** `overflow_hidden` on a rounded parent
clips its children to the parent's *bounding box*, never to its rounded
shape, so any child that paints into a corner paints square there. The bottom
scroll fade (`edge_fade.rs`) is a full-width `paint_quad`, and the root list
runs to the panel's own bottom edge — so it filled both bottom corners with
its own colour.

**The number that identified the layer**: the corner pixel measured alpha
**163**, and `163/255 = 0.64`, which is precisely the `neutral` theme's
`panel_alpha`. That is the fade's gradient at full strength sitting where the
corner's transparency should have been — not the native material, not the
window shadow, not a clipping bug. Colour sampling settled in one probe what
eyeballing would have argued about.

**Rounding the quad was the first fix and it was only half of it.** Giving
the bottom quad the panel's own radius took the corner alpha from 155 to 0
and fixed the *shape* — but the corner still read fuller than the two at the
top, by about 4px at the last two rows. A radius sweep from 1.0x to 1.75x
moved it **not at all**, which is what ruled the shape out as the remaining
cause.

**The real cause is alpha, and it explains a second complaint at the same
time.** The band is `surface_panel_translucent` painted *over*
`surface_panel_translucent`, so at full strength the composite is
`a + a(1-a)` — 0.64 becomes **0.87** for `neutral`. That costs two things,
both of which were reported: the bottom band is measurably less transparent,
so the material blurs visibly less there; and the extra coverage pushes the
corner's antialiased edge outward, which reads as squarer.

**There is no version of a panel-coloured fade that works on a translucent
panel.** Ending the band above the corner only moves the 0.64→0.87 step into
the middle of the panel, where it reads as a line rather than a fade. This
file already has the finding from the other side: an early attempt at a fade
here was *invisible*, panel-colour on panel-colour. It is visible today
precisely because it double-opaques.

So the bottom fade comes off wherever a list meets the panel's own edge
(`without_bottom_fade`), and the **scrollbar** — which did not exist when the
fade was added — carries "there is more below" without touching a single
pixel's alpha. The top fade stays (no corner, under a hairline), and so does
the transcript's, where the composer sits below and there is no rounded edge.

Measured on a scrolled list: all four corners identical at every inset
(45/27/22/19/17/15/14/12), where the fade left the bottom pair 4px proud.
`docs/evidence/edge-fade-{square-corner-before,corner-matched-after}.png`.

**Two general rules.** A painted quad inside a rounded container carries its
own radii — nothing above it will do that. And **a fade painted in the
surface's own colour is a no-op on an opaque surface and an opacity change on
a translucent one**; there is no third behaviour, so on glass it is the wrong
tool no matter how it is shaped.

## The root list scrolled all along; the keyboard just never moved it

Reported as "why does this not scroll — if I go down it will just go down
only". Both halves were real and neither was the obvious one.

**`overflow_y_scroll()` and `track_scroll(&self.root_scroll)` were already
wired up**, so a mouse wheel worked. **Nothing ever called
`scroll_to_item`.** Arrowing down advanced `selected` straight off the bottom
of the panel while the view sat still — from the outside, indistinguishable
from a list that cannot scroll. `Root::scroll_selection_into_view` is now
called from every place `selected` moves.

**From the selection sites, not from `render`.** A render-time scroll would
fight a wheel gesture, dragging the view back to the selection every frame
while somebody is trying to look somewhere else.

**`root_list_child_index` is the arithmetic that makes it land right.**
`scroll_to_item` works in the container's own *child* index, and the root
list's children are not one-per-result: a connection banner, an accessibility
banner and a section header for each new `kind` are interleaved. Getting the
offset wrong scrolls to the wrong row, which is worse than not scrolling, so
it is a pure function with its own tests — including the stale-index case,
since `selected` and `results` are updated in separate steps.

**The bottom fade is the same `scroll_edge_fade` the mode list uses**, and it
works here now for the reason it did not before: an earlier attempt put a
gradient on this list while it was purely budget-fit, and it was invisible —
a fade over blank space is panel-colour on panel-colour. Gated on the scroll
handle's own offset each frame, it appears exactly when there is something
below the fold. The `"+N more"` cue stays for the *typed*-query case, which is
still fitted rather than scrolled: there is nothing to scroll to there, and a
fade would promise a gesture that does not work.

**It scrolls to the row *past* the selection, never to the selection.**
`scroll_to_item` moves the minimum distance to reveal its target, so aiming
it at the selected row parks that row flush against the edge you are
travelling toward — you arrow down and the thing you just selected is the
last thing visible, with no sight of what is next. `ScrollBias` aims one row
further in the direction of travel, leaving exactly one row of lookahead;
vim calls the same idea `scrolloff`. Both ends clamp, and that is the
behaviour rather than defensiveness: at the last row there is nothing beyond
it to reveal.

**`sync_mode_scroll_to_selection` is gone**, and finding it was the point of
doing this properly. It already existed, already scrolled the mode list to
the selection, had no lookahead, and ran *after* the new call — so it
silently overwrote it. Two functions for one job is how a fix gets undone by
the code next to it.

**`NEKO_SELECT_DOWN=<n>` is the hook that made this provable.** The thing
worth showing is a keyboard behaviour and this repo does not synthesise OS
input, so it drives the real `SelectNext` handler N times — without it the
only evidence would be the index test, which says nothing about whether the
call is wired to the key at all.

**That hook needed a settle wait, and the reason generalises.** With no
query hook set there is no wait above it, so the first presses landed while
the initial search was still in flight — and a response arriving mid-loop
re-resolves the selection by identity, silently swallowing however many
presses had already happened. The same command produced two different
captures before that was found. **An evidence hook that drives input has to
wait for the thing it is driving to exist**, or it measures the race instead
of the behaviour. `docs/evidence/root-list-bottom-fade.png` (the fade over a
row below the fold) and `root-list-scroll-lookahead.png` (after nine Downs:
the view has moved, the selection is on `Preferences`, and `Schedules` is
visible beneath it).

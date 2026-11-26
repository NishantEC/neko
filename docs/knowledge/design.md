# Neko's design: House (2026-11-26)

Neko is a house of named cats who do the person's work. Each cat has a face of its own; the face leads everything the cat does: its
rows, its turns, its requests, its receipts, its page. Warm paper on a warm window ground, large confident type, one identity colour.
This is the app's one design: there is no look switch.

## Where it lives

| File | What |
| --- | --- |
| `themes/neko.json` | The colours (warm neutrals; a deep teal in the blue ramp's place), radius 9.6, continuous corners, Figtree. |
| `view.rs` | `theme()` with the type scale (`TYPE_SCALE`), `raised` and `well` surfaces, `state_ink`. |
| `faces.rs` | The portraits: a drawn face per face id, the working ring, the state dots, the `NEKO_CAT_ART` override. |
| `sidebar.rs`, `home_view.rs`, `turn.rs`, `cat_view.rs`, `review_view.rs` | The inset panel, Home and the house, a turn's pieces and the receipt, a cat's page. |

## Type scale (Figtree)

| Step | Size / line | Weight | Tracking | Use |
| --- | --- | --- | --- | --- |
| xxl | 40 / 46 | 700 | −1.2 | The greeting, a cat's name on its page |
| xl | 24 / 30 | 600 | −0.4 | Page titles |
| lg | 18 / 24 | 600 | −0.2 | The task's title in the band, a cat's name on its tile, Home's line under the greeting (400) |
| md | 15 / 23 | 400 | 0 | The agent's answers and the person's words |
| base | 15 / 21 | 400 | 0 | Chrome, row titles |
| sm | 13 / 18 | 400 | 0 | Second lines, states, receipts |
| xs | 12 / 16 | 400 | 0 | Row meta, the run counter |

Nothing that matters is under 12 points. The theme file has no per-step sizes yet, so `view::theme` sets them.

## Colour

One identity colour, the teal (`ring`, `accent`): focus, the working ring, the word "working", the run counter. Amber says the
person is needed, red that something could not go on, green and red are the + and − of changes. Finished things carry no colour.
Contrast is measured from the theme by `tests/contrast.rs`: every ink on every surface at least 4.5:1, the rings and dots 3:1.

Raised surfaces (a busy cat's tile, the open sidebar row, a receipt, a request, a Needs you card) are the floating surface in
light; in dark they sit one small step above the page and are lit from above (an inner top edge and a faint gradient), not lifted
by a lighter fill.

## Faces

The daemon gives each cat one of eight face ids (`neko_protocol::cats::FACES`); a new cat gets the first no active cat has, so a
person's own cats get faces from the same set. `faces.rs` draws each on a 48 point square: shoulders cut by the foot, a head with
its own silhouette (tall ears and a narrow head for the siamese, small ears and a broad ruffed head for the white long coat),
markings (tabby M, ginger stripes, tuxedo blaze and bib, calico patches, siamese points), and eyes of its own colour. Each face has
its own ground colour, so cats are told apart at 20 points by colour and at 76 by face. Whiskers, eye rims and highlights are drawn
only at 36 points and up.

| Face id | Words | Ground |
| --- | --- | --- |
| tabby | Tabby | #a9cfe6 |
| ginger | Ginger | #2f9c94 |
| tuxedo | Tuxedo | #e48a6c |
| calico | Calico | #f3abba |
| siamese | Siamese | #bba8ea |
| void | Black | #8fb27f |
| snow | White | #7894c6 |
| smoke | Grey | #e4b23e |

A portrait in its state (`portrait_in`) takes the same room in every state: a turning ring while the cat works (at least 2.5
points of stroke, a still arc under Reduce Motion), an amber dot when it needs the person, a red dot when it could not go on.

A cat's page puts its face on a wash of its ground (`faces::wash`). The ground is mixed into the page until the wash sits a fixed
step from the page (contrast 1.09:1 in light, 1.16:1 in dark), so the white cat's deep blue is no heavier than the grey cat's
yellow (a fixed share of the ground made the cool grounds heavy).

### The `NEKO_CAT_ART` override (development builds only)

`NEKO_CAT_ART=<folder>` holding `<face id>.png` (`ginger.png`, `calico.png`, …) draws those pictures in place of the drawn faces,
for faces that have one. It is read only in a debug build (`cfg!(debug_assertions)`); a release build never reads it. No artwork is
in the repository: the drawn faces are what Neko ships.

## The harness's cyan rectangle

The thin cyan rectangle round the traffic lights in the renders is not in the app: `--chrome` (`scripts/snap.sh` passes it) tells
Rune to draw the macOS traffic-light positions and report overlapping elements, and the rectangle is that overlay. Renders
without `--chrome` have neither the lights nor the rectangle, and the delivery renders are taken without it. No element in the
band is focused at rest; the app lets Tab visit the window's own controls (`tab_visits_controls`), and a ring there is then the system's. It could not be checked in a real window here (no windows in this run).

## Motion

The working ring turns (2.4 s a turn); the run counter's dot breathes; a receipt, a request and a Needs you card settle in (8 points
up and in); a tile's colours spring as its cat's state changes. Under Reduce Motion the ring and dot stand still and entrances fade.

## Upstream asks (rune-ui)

Neko builds these in its own code until rune-ui has them: a portrait with ring and status dot; a turn header slot on `Message`; a
well-style person bubble; a leading slot on `PromptInput`; a two-line `SidebarItem` with an image, a raised selection and a
trailing slot; neutral step marks on `Tool` and `Confirmation`; a run receipt with a file list and actions on `Task`; a figures slot on `Diff`'s file header (Review draws its own card to hide a zero side); an identity
ramp and per-step type sizes in the theme file.

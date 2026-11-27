# Neko's design: House (2026-11-26)

Neko's agents do the person's work; a cat is a named agent with standing rules, and a face means a cat (later a Cattain), so an
ordinary thread has no face: its turns have no head, and its requests and Needs you cards are led by a small dot. Warm paper on a warm window ground, large confident type, one identity colour.
This is the app's one design: there is no look switch.

## Where it lives

| File | What |
| --- | --- |
| `themes/neko.json` | The colours (warm neutrals; the deep teal is the theme's `identity` ramp and role, `blue` is the kit's blue), the type steps (`type.<step>`), the scrim (`number.scrim`), radius 9.6, continuous corners, Figtree. |
| `view.rs` | `theme()` (the theme file as it is), `raised` and `well` surfaces, `state_ink`. |
| `faces.rs` | The drawn face per face id, its ground, the wash and the `NEKO_CAT_ART` override, given to rune-ui's `Portrait` (the tile, the working ring, the state dots). |
| `sidebar.rs`, `new_thread_view.rs`, `turn.rs`, `cat_view.rs`, `review_view.rs` | The inset panel, New thread, a turn's pieces and the receipt, a cat's page. |

## Type scale (Figtree)

| Step | Size / line | Weight | Tracking | Use |
| --- | --- | --- | --- | --- |
| xxl | 40 / 46 | 700 | −1.2 | A cat's name on its page |
| xl | 24 / 30 | 600 | −0.4 | Page titles, New thread's headline |
| lg | 18 / 24 | 600 | −0.2 | The task's title in the band, a cat's name on its tile, the Briefing's headline (400) |
| md | 15 / 23 | 400 | 0 | The agent's answers and the person's words |
| base | 15 / 21 | 400 | 0 | Chrome, row titles |
| sm | 13 / 18 | 400 | 0 | Second lines, states, receipts |
| xs | 12 / 16 | 400 | 0 | Row meta, the run counter |

Nothing that matters is under 12 points. The steps are in the theme file (`type.<step>.{size, line-height, weight, tracking}`, rune-ui 0.10).

## Colour

One identity colour, the teal (the theme's `identity`; `ring` and `accent` use it): focus, the working ring, the word "working", the run counter. Amber says the
person is needed, red that something could not go on, green and red are the + and − of changes. Finished things carry no colour.
Contrast is measured from the theme by `tests/contrast.rs`: every ink on every surface at least 4.5:1, the rings and dots 3:1.

A run earns a change line only when it changed files or failed: `Changed 3 files · +19 −7 · Review · Undo`, one quiet line (the small step, the second ink, the figures green and red, Review and Undo ghost buttons; the dots between are the faint ink). A run that changed nothing gets no line (no "worked on a
branch", no branch name); what undo has to say about it is one quiet line under the answer. The files, the folder and the branch are in the right pane. The reply composer stands alone:
who answers is in the band and in its placeholder. Opening a page (Briefing, Needs you) puts the keyboard on its first item
without drawing the focus ring; the ring shows once a key moves focus, as in Mac apps.

**One ground in dark.** The window has one dark ground (`neutral.975`): the theme's `page.bg` and `surface.bg` are the same colour in
dark, so the content area is not a lighter slab beside the sidebar; the sidebar's panel is what floats on the ground (it reads as an
overlay, lifted a fixed step: `sidebar::panel_bg`, the ground mixed 8.8% toward white, taken from the ground, never from the page).
In light the page stays a lighter paper on the warmer ground, which is how Mac apps read in light; a lighter page there also holds
the content in a clear area beside the panel, so light is unchanged.

Raised surfaces (a cat's tile, the open sidebar row, a receipt, a request, a Needs you card) are the floating surface in
light; in dark they sit a small step above the panel (`view::raised_fill`, the ground mixed 10.5% toward white) and are lit from
above (an inner top edge and a faint gradient), not lifted by a lighter fill. Wells and the composer are the theme's translucent
`input.bg` over the ground.

## New thread

The screen the app opens on (`new_thread_view.rs`; behaviour in `threads.md`). One column, centred, with the headline a little above
the middle of the content area: "What should we work on?" ("... in <project>?"), plain text in the xl step, semibold. Then the
composer, 640 points wide, four lines at rest and up to twelve as it grows, placeholder "Describe the work", and under it one row of
chips (Project; for a repository, New worktree / In the folder and the branch a worktree starts from), each a quiet pop-up in the
chip step the composer's footer used. Under the chips, only when they apply, the notice line, the daemon-slow line and the
Briefing's line. Nothing else: no greeting, no date, no house of cats, no lists of threads (the sidebar has them). Keyboard
focus is in the composer but no focus ring is drawn until a key moves focus, as on the Briefing and Needs you. The scenes
are `new-thread`, `-typed`, `-folder`, `-in-folder`, `-place-menu`, `-branch-menu`, `-adding`, `-started`, `-busy` and `sparse-new-thread`.

## Turns, requests and cards

A turn has no head: the words are the agent's, with no portrait, name or "done" row. A request is led by a small amber dot (red if
destructive) and says "The agent asks" ("Kitten 1 asks" for a subagent); a Needs you card is led by the same dot (a plug for a
plugin). A cat's portrait appears only where a cat is the subject: its page, a round's card and the Cattains. A task a cat was given shows
the cat's name as quiet text in the band.

## Sidebar rhythm

One vertical rhythm for the whole sidebar, in steps of the theme's 4 point unit (`sidebar.rs`: `ROW_GAP_STEPS`, `SECTION_GAP_STEPS`,
`GROUP_GAP_STEPS`, `row_1`, `row_2`, `header`; measured from the laid-out tree by `tests/sidebar_rhythm.rs`):

| What | Value |
| --- | --- |
| A one-line row (New thread, Needs you, Briefing, Rounds, Add project…, Show more) | 36 (`metrics.row`) |
| A two-line row (a thread, a cat in the Cattains) | 52 (a row and one step either side) |
| A section or group header (Cattains, Threads, a project) | 32 (`control.sm`: its buttons are this high) |
| Gap between any two stacked rows (a cat and its nested threads too), a header and its first row (the Threads header and its first project too), a list and Add project… | 2 (half a step) |
| Gap between project groups | 12 (3 steps) |
| Gap between sections (the nav, the Cattains when a cat is at work, Threads with Add project…) | 20 (5 steps) |

So a header sits 2 points over its own rows and 12 under the section above: it belongs to what is below it. Add project… is a row like the others, a row gap under the list. A section header is a label, not a row: it has no wash at rest, under
the pointer or when pressed (only the keyboard's focus ring), because a wash on a full-width pill reads as the selected row. The
earlier build gave headers the row material, so a header the pointer rested on after a click (to fold or open it) looked selected,
and its pill ran into the first thread row under it (a 1 point gap).

The sidebar is 256 points wide (`view::SIDEBAR`, fixed: a resizable split is an upstream ask).

## Threads

The section under the Cattains (`sidebar::threads_section`, `threads.md`). Header: "Threads", the same small semibold second-ink step as
"Cattains", with View (three lines of different lengths) and New thread (+) as ghost buttons at its right. Under it one group per
project: the project's name in the sm medium step in the second ink, a chevron, an amber count when something waits, and on hover
or focus its New thread and its menu; the group's header is a label, so it takes no wash. The header "Threads" and a project's name
differ in weight only, because a second weight of colour would add chrome: the owner asked for less.

A thread's row is 52 points, two lines, in the sidebar's own row material (hover, focus ring only for the keys, the open thread a
raised pebble). It leads with the **status mark** (`status_mark.rs`): 12 points in a 16 point slot, an amber dot for Needs you, a red
dot for Failed, a small turning ring in the identity teal for Working, nothing for Idle. No cat's portrait leads a row any more: a
face means a Cattain. Line 1 is the title (base; semibold while unread, regular once read) and at the right the age in the xs step in
the faintest ink that reads, or "Working" in teal; under the pointer a quiet menu (three dots, no wash at rest) takes its place. Line
2 is the xs step: `Needs you: …` amber, `Failed: …` red, else the agent's last line in the second ink. In the ungrouped list the
project's name sits at the right of line 2 in the faint ink, cut at 88 points. The mark, the amber and the red are each taken toward
their strong ink until they read against the panel (tested in both looks). Nothing in a row is a portrait, a card or a badge.

## The Cattains

The sidebar's section for the cats at work, under the nav and above Threads (`sidebar::cattains`). Header: "Cattains", the small
semibold step in the second ink, a control high (32), no wash. A row is the 28 point portrait (ring while it works, amber dot when it
needs the person), the cat's name (base, medium; semibold while its page is open, raised pebble) and one line in the xs step: the
working task's title in the second ink, `needs you: Run a command` in amber, `working on 2` in the identity teal. A cat with several
tasks at work nests them under it as the ordinary task rows, stepped in 12 points (narrower, so the pebble and the hit area step in
too); they sit a row gap apart like every row. Napping cats are not listed and with none at work the section is absent: no header, no
empty line, and Threads rises to a group gap under the nav. The word is the owner's: Cattains. A Cattain's nested threads are the ordinary thread rows (status mark, no portrait), stepped in 12 points.

## Rounds

A round is a raised card (the same material and corner as a Needs you card) led by its cat's 44 point portrait; the switch, Run now and
the menu sit at the card's top right. "Next:" is the identity teal while the round will run, amber when something stops it ("Turned
itself off", "mochi is retired", "Folder missing"), the ink while it is only paused or off. One primary button at a time: New round…
is the page's, and while every round is paused Resume rounds is, with New round… stepping back to secondary. The icon is Neko's own
(`icons::ROUNDS`, two arrows chasing round); a thread a round made starts unread (its title is bold until it is opened).

## The Briefing

The page a person reads over coffee (`briefing.md`): the date in the small faint step, the window as the page title (xl, "Since
yesterday 18:00") and the headline under it at the lg step in the second ink. Then sections, each
headed at the base step with its count (amber for Needs you, the identity teal for Still working, faint otherwise):

- **Needs you** is the Needs you card itself: raised, led by a small amber or red dot.
- **Finished** and **Rounds** and **Still working** are *wells* (the pressed-in surface), one per section, rows
  separated by a hairline: calmer than the cards, because nothing in them must be done. A round line is led by a 24 point
  portrait; a finished or working row of a thread has no portrait or name. How it ended ("Finished", "Could not finish", "Stopped", "Interrupted") is the sm step in the second ink with the time in tabular
  figures; the title is md semibold; the sentence base in the second ink; the files, branch and "Undone" in sm. "Open task" is a ghost
  button at the right of the who line on every row, so every row has the same way in; Review and Undo are secondary buttons on the
  row's last line, only when the run offers them. Red is only for "could not finish" and a round that turned itself off.
- The totals are one quiet sm line, the cost note under it in xs; **Mark as read** is the one primary button, with its key in a
  keycap beside it (words, not symbols: the keycap's face has no ⌘ glyph and falls back to another font).
- Caught up is rune-ui's Empty (a check in a small well, "You're caught up"); with a request still waiting, its card follows under
  "Still waiting for you".

The Briefing's line on New thread is a well too, one row high, with the Briefing's own glyph (a sun rising over a line, `icons::BRIEFING`) and Open
briefing as a secondary button.

## Plugins

A plugin has no face: it is led by a plug (`icons::PLUGIN`, Neko's own, because rune-ui's set has none) on a quiet ground, the page's paper taken a small step toward the ink (`plugins_view::tile_ground`), so no cat's colour is borrowed and the glyph reads at 4.5:1 (tested). In a Needs you card the tile takes the room of a cat's 44 point portrait and carries its amber dot; in Settings it is 40 points. A card in Settings is the slab the agents' cards are; the approval block inside it is a well. The command line is mono, whole and wrapped (never cut), in its own well; names of what a plugin is handed are the sm step in the second ink. Check results are quiet: a check mark in the second ink for "Connected: 6 tools", red only for a failed test, the tool names in mono sm. The Add sheet is rune-ui's Dialog at 560 points: a text area, then what the paste will add (one well per server), notes, one quiet line; many servers scroll inside the sheet so Add stays on screen at 760 × 560.

## Kittens

A kitten has a number and no face, so nothing in its look borrows a cat's colour. **The card** is the receipt's material (raised, lit from above in dark) with one row of 56 points per kitten and a hairline between rows, so three kittens are one card, not three. Line 1 is `Kitten 1` (semibold, first ink), its title (second ink, cut at the end), and at the right its mark and word; line 2 is "Shared folder" in the second ink and, quieter, the live last line in the third; Open is a small secondary button. Marks are the kit's neutral ones: the turning ring (second ink) for Working, a hollow ring for Queued, a tick for Done, red only for Failed, amber for a request or a question. The word is the state's only colour: the identity colour for Working, amber, red, else the second ink. **The still ring** is the working ring held still in the second ink: it is what Waiting looks like (the thread's portrait in the sidebar and the Cattains, the waiting line, a cat's list), because a thread that rests is not busy and must not turn. **The wake note** is a centred line in the xs step, second ink, with a hairline each side, the quietest thing in the conversation: it says why the thread is running again, and the report under it is one fold like a step's (closed, a hollow dot, "Report from Kitten 1"), opening to the summary as Markdown in the second ink, the files in mono, one a line, and the branch. **The waiting line** is the conversation's last line: the still ring, "Waiting on", and each name in the identity ink as a link (no button padding, so the sentence reads as one). **The kitten's own view** is the ordinary conversation with its head left out (no portrait, no cat name), a quiet line where the composer was, and Back and Stop as small buttons; its band is `Fix login / Kitten 1`, the thread's title a ghost button and the kitten in the heading's own weight. A request there and in Needs you is led by the number on the plugin tile's quiet ground (4.5:1), never a cat.

## The composer while it works

Quiet, because the person is mid-thought. The messages that wait are **one block** above the box in the hover tint, radius `lg`, one line
per message (the control height plus four points), a hairline between lines, no border of its own: the kind in medium weight and ink
muted ("Steered", "Queued", "Interrupting"), the first line of the words in `sm` and ink faint, "waiting to be read" at the right of a
steered one, and for a queued one the ghost buttons Send now and Edit and a × (Discard). More than two queued fold behind "3 queued"
and a chevron. The **activity bar** is one line of the same height with no fill: the still ring, "2 kittens working · 1 command
running" in `sm` ink muted, a chevron; open, a line each (name in ink, the title faint, how long at the right, a × that stops it), under
the bar and above the box. The **send mode** is a chip in the toolbar ("Steer" and the up-down mark), and a faint hint line under the
box ("Return steers · ⌥Return queues · ⌘Return interrupts") shows only while words are typed and the agent works. Nothing else changes: at
rest the composer is what it was. Rendered round three of three (`composer-*` and `activity-*` scenes); the first round had a stripe per
message and a fill per row, which read as three small cards, so they became one block.

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

A portrait in its state (`faces::portrait_in`, rune-ui's `Portrait` with `.ringed(true)` and `.state(..)`) takes the same room in
every state: a turning ring in the identity colour while the cat works (at least 2.5 points of stroke, a still arc under Reduce
Motion), an amber dot when it needs the person, a red dot when it could not go on. Ring and dot are taken toward their strong ink
until they reach 4.5:1 and 3:1 against the surface they sit on, so the amber dot is a little lighter on a light card than it was
when Neko chose it by hand.

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

rune-ui 0.10.0 gave Neko the portrait, the turn header and well bubble, neutral marks, the diff's figures and foot, the theme's
identity ramp, type steps and scrim. What Neko still builds itself, and what would let it stop, is in
`docs/knowledge/rune-ui-in-neko.md`, "What rune-ui cannot do here".

## The right pane

Native and calm: a hairline at its edge (rune-ui's Split handle), the sidebar's panel material full height (not an inset card, so it reads as
part of the window's frame), no heavy cards inside it. Its header is the band's line: rune-ui's Tabs in the Pane variant (the words in
the second ink, the chosen one semibold with a 2 point underline), the pane button at the right. Sections are a small heading (sm,
semibold, the second ink) with a faint count, 24 points apart; rows are 32 to 44 points, hover on the row material, with the right slot swapping its quiet state word or age for
the row's actions under the pointer (as a thread's row does). Needs you leads with an amber dot, a kitten with its status mark and its number in semibold, the files in mono with
only the figures that say something. Overview has no card around anything. Tabs share one body that fades in when it replaces
another. The sheet under 1100 points is rune-ui's Sheet (the floating material, rounded on its free edges, a scrim) holding the same
tabs. Renders: `scripts/snap.sh` pictures `pane-overview*`, `review*`, `pane-changes-*` and `pane-kitten-*`.

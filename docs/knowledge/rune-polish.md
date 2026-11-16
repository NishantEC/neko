## Rune polish: the web UI's tokens, the sessions column, Home and its art (2026-11-16)

Code: `apps/neko-rune/`. Spec (the owner's decisions, implemented as written): `docs/design/rune/polish-spec.md`. The first
build looked right on test data and wrong on the owner's real data ("looks really off", "doesn't match our own UI"), so
this pass was built **against a copy of the real database** and checked against the web UI side by side.

### How a screen is verified (`scripts/polish-pairs.sh`)

`sqlite3 .backup` of the live `neko.db` into `/tmp/neko-polish-data` (read-only on the live side), the isolated `web_harness`
on that copy (it refuses any folder outside `/tmp`), the Rune half headless (`--snap`, 1440x900 at 2x, `NEKO_RUNE_SOCKET`,
`NEKO_RUNE_OPEN_TASK`, `NEKO_RUNE_VIEW=inbox|settings|...`, `NEKO_RUNE_ACCOUNT=1` for the Mac account's name), the web half in the
shared Chrome (`scripts/polish-web.mjs`). Real-data pictures live in `/tmp/neko-polish` and are **never committed**; the
committed ones are synthetic (`scripts/snap-polish.sh` → `docs/design/rune/polish-*.png`). Headless harness runs now use the real
clock (a seed is as old as its run), and a window **opens on Home**, not on the first session.

### The tokens are the web's CSS (`theme.rs`)

`web/src/themes/builtin.generated.css` (neutral, neutral-light) and `styles/globals.css` are parsed by the unit tests and compared
with `DARK`/`LIGHT`: background, card, popover, sidebar, foreground, strong, muted, code, primary, the four meanings, **border
(white 7% dark, black 8% light)**, the font stacks (`FONT_SANS` is the web's with `Inter` for `"Inter Variable"`, `FONT_MONO`
verbatim) and `--radius: 1.25rem` (the window radius, 20). Faint is `muted-foreground/70` laid on the background. The spec's own
numbers: hover foreground 4%, selected 7%, the field border 10%, row radius 8, the focus ring Iris at 60% (the web's ring is 65/55:
the spec wins), and the **sidebar = background + 2% HSL lightness** in both appearances (so the web's own `--sidebar` value stays in
the table, unused for painting).

### Grouping (`groups.rs`, `link/model.rs`)

| Group | In it | Shown |
| --- | --- | --- |
| Needs you | waiting on the person with activity in 7 days; failed within 48 h | 8, then "Show N more" |
| Running | planning, building, reviewing, queued | all |
| Recent | completed, failed (48 h to 7 d) or cancelled within 7 d | 5, then "View all" |
| Waiting a while | waiting on the person, older than 7 d | folded, with a count |

Boundaries are inclusive (exactly 48 h and exactly 7 d stay in) and tested at each edge. A task is in one group only; older work
that is not waiting on you is in none and lives in "View all" (`FullView::AllSessions`). **Waiting on the person** is
`TaskRow::wait` (`Wait`), computed when the snapshot is summarised: plan approval, a question (the agent's "Needs your input before
building:" as the newest real event, or a blocked goal), a review/merge, a failure, a goal at its turn/budget cap, anything else.
**Progress** is a live goal's turns of its cap, else the plan's `- [x]`/`- [ ]` steps, else none (the ring turns). Two choices
the spec did not name: Queued counts as Running (it runs next), and a goal paused by the person is not a wait on them.

### Notices are not chat (`activity.rs`)

`neko_chat::post_notice` marks nothing on the message, so the client tells one by its **shape**: Neko's role, no `started_ms`,
not pending/failed/queued, text parts only, no tool calls, and the message before it is not the person's. The daemon test
`a_notice_has_the_shape_the_native_client_folds_out_of_the_chat` pins the shape. Notices become one 32 point Activity line above
the chat (the latest, "+N", pressing it opens the list in place); the chat draws the last 30 messages and "Earlier messages" adds
30 (`CHAT_KEPT` is 120). The real chat of the copied data was 22 update notices around two real turns.

### The sessions column and the rest (`view/sessions.rs`, `view.rs`)

248 wide (232 to 360), 12 padding, every row 32, 8 gaps, header, search (opens ⌘K), "New session", Routines/Calendar/Workspaces with
counts (the routines page's and the calendar's answers are asked once a minute), the scrolling list, the 40 point account footer
(Mac account picture via `dscl`, else initials on Iris; the menu holds Settings, Usage, Plugins, plugin pages, the web UI, Quit).
Reason icons are muted and amber only while the row is hovered (an icon over an icon with `group_opacity`); the progress ring is a
16 px icon, 2 px track at 12%, the arc cached per 2%; the indeterminate arc turns on `LoopSpec::rotation(1.2)`. The traffic lights
are at (12, 21): 14 point buttons centred on the 28 point line every header row (sidebar, session, Neko) is centred on; the sidebar
header's words start at 80. Drag regions are the header rows. The icon set is `icons.rs` only, stroke 1.5, tested.

### Home and its art (`view/home.rs`, `art.rs`, `art_gpu.rs`)

A 680 column about 40% down: the mark and "Good evening, {first name}" at 34, a status line whose parts filter the sessions
column, and the composer card (radius 16, text 3 to 12 lines; "+" attach, workspace with its branch from `.git/HEAD`, runtime,
Plan/Build, model "Auto", Send). **Build** is `CreateTask` then `StartTask`; Plan waits for approval as every ticket does. The old
"keep going until a check passes" switch is gone from the form (`/goal` in chat does it).

The art is one fragment shader at an effect slot: sky, 10%-of-cells stars on their own 3 to 7 s period, a moon, a sea with a
drifting pattern and a shimmering glade, a sitting cat on a rock facing the moon, all a smooth value field thresholded by a Bayer 8x8
matrix in Iris (80% dots in dark; light is dawn, Iris at 35%, a near-black cat). It is drawn **one texel per dither cell** into a
small target and expanded by a blit (the first version, a texel per device pixel, cost 2.2 ms). The clock runs only while the window
is active and Reduce Motion is off; Home hidden means no slot, no frame. A 70% scrim of the window colour keeps the words at AA
(`art.rs` tests the worst dot under the scrim: strong, base and muted all pass).
**Measured** (`NEKO_RUNE_ART_BENCH=1`, timestamp queries, scene pass plus blit, 2560x1600, 360 steady frames after 120 of GPU
warm-up): 0.32 to 0.78 ms mean, 0.14 to 0.65 ms median across runs on a quiet and a loaded Mac, empty pass 0.07 ms. Almost all
of it is writing 4 megapixels of noise, not arithmetic (removing the sea or the sky changed nothing). `tests/art.rs` asserts the
median is at most 1.0 ms.

### The P3b review's open item, fixed

The web panel opens the person's browser for another website **only within 1 s of a trusted click or keypress in the view**
(`GESTURE_SCRIPT` over wry's IPC, `isTrusted` only; `external_decision`); otherwise it blocks, logging once per address. Download
links save to `~/Downloads` under their own name, never a path, never over a file (`downloads_destination`), and the window says
"Saved to Downloads" (a three-second toast). Not run in a real window (see P3b's note).

### Consistency, tested (`tests/consistency.rs`, `tests/polish.rs`)

Every `.focus_ring(` is 2 at offset 2; only the hover curve (120 ms ease-out, set once on the root), the panel's 180 ms Snappy
spring (a slide-and-fade transition), the progress ring and the art animate, and a source scan names the files allowed to; the
Neko column uses 32 point rows and 12 padding; the session header is centred on the sidebar header's line.

### Known differences from the web UI that remain on purpose

The web's icon rail and tab strip (Rune has the sessions column and no tabs), the web's reading column (centred, ~760) against
Rune's session text starting at the region's left, the Neko column against the web's Home chat. See `/tmp/neko-polish/INDEX.md`
of the run that produced the pictures for what was still different when it was written.

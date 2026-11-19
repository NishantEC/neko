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

### A folded sessions column has a way back (2026-11-19, `view.rs`, `tests/sidebar.rs`)

The owner folded the column and found no control to reopen it. `band_px` (the header row of every session-region view, one place) now
starts with the **"Show sidebar" button** while `SIDEBAR_FOLDED` (a thread-local `view()` sets each frame, because the 14 callers of `band`
have no `App`) is true: a spacer of `80 - 2 - padding - 8` so the button's left edge is exactly 80 from the window's (2 is the region's margin,
8 the row's gap), then the 28 point button (`sidebar-show`, glyph `icons::SIDEBAR_LEFT`, tooltip "Show sidebar ⌘\"), so whatever the
header holds moves right of it; the session title line (padding 20) and the plugin page head use `band_px(t, 20.0)` for that reason. The
open column's fold button (`sidebar-fold`) now says "Hide sidebar ⌘\". **⌘\** (`lib.rs` `on_key`, listed in `shortcuts.rs` as "Show or hide
the sessions column") toggles it with the older ⌘B; ⌘\ was not bound to anything, so it is the shortcut used. `layout::Saved` gained
`sessions_open` (`#[serde(default)]` true, so a file from before keeps the column open). Scenes `sessions-collapsed` and
`home-sessions-collapsed` go through the structure crawl and the "every scene lays out" test (which lets those two have no `sessions`).

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

### The Home composer is frosted glass (2026-11-19, `glass.rs`, `view/composer.rs` `card_on`, `tests/glass.rs`)

The owner's screenshot showed the card as a flat translucent box over the dither; they asked for real glass with crisp words on top.
The card (`home-composer`) has no fill or border of its own now. Its first child is the **backing** (`glass::backing`, key
`composer-glass`): an element pinned to the card's four edges with radius 16, `El::backdrop_blur(20.0)` (Rune's own in-window
backdrop blur, `rune/src/render/backdrop_blur.rs`: at that element in painter order it copies what the window has drawn so far, the art's
effect slot and the scrim included, blurs it at half resolution inside the rect plus the blur's reach and composites it inside the
rounded corners; sigma is in points, so Rune scales it by the window scale), the window colour at 55% (dark) or 60% (light) as the
fill, the 1 px border at foreground 10% and a 1 px highlight child (`composer-glass-highlight`) along the top edge at foreground 6%. The field and
the bar come after it in the same column, so the words and controls draw above the glass at full strength. We used Rune's element path
rather than `rune-fx`'s `FxRenderer::blur_backdrop`: that one needs a hook at an `effect_backdrop` slot and our own pass handling, and
the element path already does the clip, tint, border and ordering. The floating "Show sidebar" button on Home is the same backing at radius 8
(`sidebar-show-glass`, no highlight). At most four elements blur per frame (`MAX_BACKDROP_BLURS`); Home uses two.

**Words.** Rune has no text shadow, so `glass::shadowed` draws the same text twice: first a copy in black 35% (dark) or white 50%
(light), `blur(4.0)` (CSS blur 8 is sigma 4), pinned one point lower and hidden from accessibility, then the words. The greeting and both
status parts use it; the scrim is unchanged.

**Fallbacks.** `App::glass_mode` is Opaque when `glass::system_reduce_transparency()` (NSWorkspace's
`accessibilityDisplayShouldReduceTransparency`, read each Home build; `NEKO_RUNE_REDUCE_TRANSPARENCY=1` stands for it in checks, as Rune's
`RUNE_INCREASE_CONTRAST` does; the scene `home-opaque` sets `App::opaque_glass`) or when `App::backdrop_ok` is false. The compositor sets
that cell each frame from whether the window's target has `COPY_SRC` (`render_art`), so a device that cannot copy its target shows the
opaque card one frame after it first draws. The opaque backing (`composer-opaque`, `sidebar-show-opaque`) is the window colour with the
foreground at 4% laid on it, opaque, with the same border.

**Static art.** Rune blurs on each frame it draws and has no way to hold a blur result across frames, so under Reduce Motion (or an
inactive window) the art's clock stands still and no frame is asked for, but a frame that something else asks for (the caret, a hover)
re-runs the blur at that frame's cost. A literal "compute once" would need a cache layer, which would hide the art from the blur (a
cached subtree's backdrop is only what that layer has drawn). Not done; the cost is the figure below, once per such frame.

**Budget, measured** (release build, 2560x1600, `NEKO_RUNE_ART_BENCH=1`, the Mac under other agents' load, so the art alone drifted
between runs): the isolated art bench (scene pass plus blit) 0.78 to 0.99 ms median over six runs. `viewers/model3d_gpu.rs` `frame_bench`
now also times the whole Home frame (art scene pass, blit, glass, every word) encoded and submitted 12 to a batch and waited for, wall
ms per frame, so it is GPU-bound at this size: 1.37 / 1.69 / 1.76 ms with the glass against 1.40 / 1.58 / 1.57 ms with the opaque card
(`NEKO_RUNE_REDUCE_TRANSPARENCY=1`), three paired runs. **The glass adds about 0.1 ms (-0.03, +0.11, +0.20), so art + glass is about
0.9 ms median on a quiet Mac and can touch 1.1 ms when the GPU is busy with other clients**; the art alone does the same (`tests/art.rs`
asserts its median at most 1.0). The whole-frame figure is not the budget (it counts the sidebar and the Neko column too); only the
glass-minus-opaque difference is read from it. A debug build's number is the CPU's, not the GPU's.

**Tests** (`tests/glass.rs`): scene `home` has `composer-glass` (radius 16, tint 0.55 / 0.60, the highlight) with the request field,
Send and Plan/Build after it in paint order; the Home button is glass and the session one is not; `home-opaque` is flat and the
expected colour in both themes; a capture of the real binary against `home-art-probe` (the same scene with the backings left out, a test
hook `App::glass_probe`) shows the card's pixels differ from the raw art, its fine detail is under half (0.009 against 11.4 dark, 0.001
against 2.5 light), and the art outside the card is untouched; typed words keep 13.0:1 (dark) and 14.4:1 (light) against the glass; the
setting env var draws the card flat.

### The P3b review's open item, fixed

The web panel opens the person's browser for another website **only within 1 s of a trusted click or keypress in the view**
(`GESTURE_SCRIPT` over wry's IPC, `isTrusted` only; `external_decision`); otherwise it blocks, logging once per address. Download
links save to `~/Downloads` under their own name, never a path, never over a file (`downloads_destination`), and the window says
"Saved to Downloads" (a three-second toast). Not run in a real window (see P3b's note).

### Consistency, tested (`tests/consistency.rs`, `tests/polish.rs`)

Every `.focus_ring(` is 2 at offset 2; only the hover curve (120 ms ease-out, set once on the root), the panel's 180 ms Snappy
spring (a slide-and-fade transition), the progress ring and the art animate, and a source scan names the files allowed to; the
Neko column uses 32 point rows and 12 padding; the session header is centred on the sidebar header's line.

### The session-view parity pass (2026-11-16, `tests/parity.rs`)

Built against the same copied real data, from the owner's review of the pairs. What changed, where it lives and why:

- **One text column** (`view.rs` `TEXT_COLUMN` 760, `TEXT_PAD` 32): turns, the question card and the composer sit in one centred column; the
  header, the goal row and the region's fill stay full width. The card inside is 696 wide (760 less the two 32 sides).
- **Header** (`view/session.rs`): one 32 row, glyph, title (`ty.brand`, 14 semibold, ellipsis), a state badge (`state_badge`: Needs approval,
  Needs answer, Planning, Building, Reviewing in amber, Failed in coral; Queued, Ready to review, Completed and Cancelled have none here, the
  brief names only amber and coral), the runtime chip ("Claude · default", `Detail::runtime` from the newest `runtime_selections` of `task:<id>`),
  the "..." menu (Propose parallel work, Copy link, Open in web UI, Stop, Mark as unread) and the panel toggle. The raw ticket text is gone.
  `Call::ProposeSplit` is the web's own button (`Command::ProposeSplit`, read-only proposal), enabled by `Detail::can_split` (the web's
  `canProposeSplit`). Copy link puts the token-less `http://127.0.0.1:<port>/#/tasks?task=<id>&view=full` on the clipboard, never the signed one.
- **Question and approval card** (`view/work.rs` `note`): amber 8% fill, 1 px amber border at 30%, radius 12, padding 12/16, the `?` icon at the
  left, the first line clamped to two lines (`max_lines(2)`, the ellipsis ends the second) and "Full text" at the bottom right opens it in place.
  The primary button is inside, bottom left: "Answer" (focuses the composer) for a question, "Approve plan" for a plan. A stop is the same card in coral.
- **Crew hand-off lines** (`view/turn.rs` `handoff_row`, `turns_for`): `Builder -> Neko  (glyph) NEK-6F70 : title`, when one agent's turn follows
  another's, as the web's `HandoffRow` does; Neko's column uses the same row and the web's `shortId` (`NEK-` and four characters), not the source key.
- **One composer** (`view/composer.rs`): the session's reply box, Home's new-session box and Neko's ask box are one card (radius 16, 1 px border at
  foreground 10%, padding 16, three lines minimum) with a bar of "+" attach, the mode control (steer mode / Ask-Plan / Plan-Build), a model chip and a
  32 px round Send (Iris with text, foreground 10% without). The text area is Rune's `text_input` in all three (no kit field chrome inside the card).
  Plan prefixes Neko's words with the web's `PLAN_PREFIX`. The model chip is a read-only label (the runtime the conversation last ran on):
  the daemon changes a conversation's runtime only through `SetConversationRuntime`, which the sessions column does not offer a control for yet.
- **Context panel tabs**: Plan (`plan.rs` `steps`: the `- [x]`/`- [ ]` lines, else numbered or bulleted ones; the first open step is active while the
  task builds) and Info (workspace, branch, pull request, runtime, created, updated, the ticket's source link), after Terminal. They are
  "tabs the person asks for" (`Tab::on_request`): they never open the panel by themselves (Info is always there), so auto-open reads
  `panel_content_tabs`. The icon-only rule under 440 is unchanged; at 440 to ~520 the six labels are tight (see the INDEX).
- **Terminal tab**: a 32 row per command (`$`, the command in 12.5 mono cut in the middle to the panel's width, an exit chip: muted "0", coral
  otherwise), output in 12 mono collapsed to 12 lines with "Show all N lines" / "Show fewer", the home folder written `~` (`terminal::tilde`).
- **Neko column**: the ask box is the shared composer; the crew line says "No agents working · 27 in Inbox" when nothing needs you but the Inbox has items.
- **Routines**: grouped under workspace headers (12.5 medium muted, count), a status chip after the name from the last run (Quiet, Acted in mint 12%,
  Failed in coral 12%), the cadence in a fixed 180 column, the next due right-aligned, the web's 36 x 21 switch (now every `ui::switch`).
  The last run and the catch-up note are in the expanded details.
- **Settings**: Appearance (System/Light/Dark through `GetAppearance`/`SetAppearance`, the daemon's own setting, which the window follows:
  `App::appearance_dark`), Permissions (`permissions.rs`: Accessibility, Full Disk Access and Screen Recording asked of macOS without prompting,
  Calendar and Reminders from what the daemon already reports; "Open System Settings" opens the pane), Diagnostics (versions, data folder, daemon,
  "Copy diagnostics"), and one row each for Rulebook and Quick panel ("Open in web UI"). The ⌘F hint is a trailing hint that shows only while the
  field is empty; the clear button only while it has text.
- **Pictures**: `NEKO_RUNE_TAB=plan|info|terminal|...` and `NEKO_RUNE_SECTION=permissions` pick a panel tab and a Settings section for
  `polish-pairs.sh`, which now also makes the "settings-permissions" pair and no longer needs the installed app's `web-live` folder.
- **Allow list** (`tests/allow_list.rs`): new reads `GetAppearance`; new writes `ProposeSplit` and `SetAppearance`, both controls the web has.

### Known differences from the web UI that remain on purpose

The web's icon rail and tab strip (Rune has the sessions column and no tabs), the web's Chat/Result/Changes/Plan/Details tabs in the header
(Rune's are the context panel's tabs), the Neko column against the web's Home chat, the read-only model chip, the Permissions section reading
this app's own access where the web says "Managed in the native app". See `/tmp/neko-polish/INDEX.md` of the run that produced the pictures for
what was still different when it was written.

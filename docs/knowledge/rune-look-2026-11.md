## Rune looks like the approved prototype (2026-11-21)

The owner tried the cats slice and said every bit looked very different from the mock-ups: the app still wore the rejected design
(a floating sidebar card, pebble controls, the old nav). This slice replaced the look and the shell. **The prototype is the only source of
truth for looks**: `~/Documents/neko-design/prototype-2026-11-21/` (`_src/app.css` for values, `shots/` for the 79 approved pictures). No
new back-end feature: everything drawn is read from data the client already had.

### Where the tokens live

`apps/neko-rune/src/theme.rs` has the prototype's table, dark and light, exactly (a test pins every hex): chrome `#0c0c0d`/`#ececee` (the
window and the sidebar, `Surfaces::sidebar`), surface `#141416`/`#ffffff` (the content, `Surfaces::window`), raised `#1b1b1e`/`#f5f5f7`
(`Surfaces::card`: the composer, a card, a popover), text / text-2 / text-3 as `Greys::base / muted / faint` (`#ededef`, `#a0a0a7`, `#6b6b73`
dark; `#18181b`, `#55555c`, `#7d7d85` light; the third is a share of the second laid on the surface), `run` blue as `Meaning::iris`
(`#8ab4ff`/`#3d6fe0`), amber `#f2b24c`/`#d48a12` with its text and wash (`amber_text`, `amber_bg`), green, red, the ink (`Materials::ink_*`,
`#f4f4f5` on `#111`; `#18181b` on white). Washes: hover 4% (3.5% light), selected 7% (5.5%), tint 5.5% (4.5%), hairline 7%, strong hairline
11% (12%). Radii: row 8, button 7, card 12, composer and sheet 14, pill 999. The web's CSS is no longer parsed. A theme the person picks
still maps onto the same tokens (`from_hex`), with flat materials.

There are **no gradients, glows, lit edges or glass**: `Materials` is flat (the pebble is the tint, the ink is one colour), `webkit::pebble`,
`ink`, `well` and the new `outlined` are fills and hairlines. Buttons are 28 high, radius 7, 12.5 words (`webkit::button`); the switch is
34 x 20; the tabs are the prototype's segmented well; a settings group is the raised colour at radius 12.

Prototype widgets are in `view/look.rs` (dot, working ring, merge glyph, task glyph, section label, small bordered action, pill, full-width
segmented control, composer chip and send, `**bold**` briefing text); the sidebar, Home and the cat page compose them and draw no chrome.

### The shell (`view.rs`, `view/sessions.rs`)

The window is the chrome colour; the sidebar (248, flat, 45 above its first row, 10 at the sides) sits on it with no card; the content
(`session::region`) is flush with the window's edges and paints one hairline on its left; a cat's panel is 372 on the chrome colour with its
own hairline. Edges resize through 7 point handles laid over the hairline (`layout::HANDLE`, keys `grip-*`). `border_sides` takes **[top,
right, bottom, left]** (CSS order). **There is no Neko column** unless the person opens it (`Layout::neko_off`, true at every launch;
`set_neko_open(true)` clears it); `App::headless_neko()` is the window with the column, for the tests of what the column does, and the
scenes `live` and `running` switch it on. Home draws no art any more (`art.rs`, `glass.rs` stay with their unit tests).

Sidebar, top to bottom: brand row (mark, "Neko", the workspace switcher at the right), Home (count = what needs you, amber), Search ⌘K,
Calendar, Tasks (all tasks), the Cats section (header with a plus, up to three pinned faces, the open cat's name, area and nested tasks, the
other cats as rows), then Rounds, Settings (count = Setup items that need you) and the footer ("<first name>" and `$today` from the usage
ledger's figure in the Routines answer). The account menu (Usage, Plugins, Pull requests, Stop all work, the web UI, Quit) opens from the
footer. Setup pages are reached from Settings' rail (its last group); the old Needs you / Running / Recent lists are gone from the sidebar:
needs-you is on Home and as the amber dot on a cat's face. A task shows in the sidebar only under its cat (right-click menu, unseen dot and
status words kept). Offline status is a quiet line in the sidebar (`sessions-status`).

### Home and the cat page

`view/home.rs` (data in `briefing.rs`): greeting 28, status line (working / needs you / spend today), the Briefing (the morning digest's
headline, `**bold**` honoured), the composer (attach, "Let Neko choose" or a cat, the model chip, round send; with no cats the picker names
the workspace; the Plan/Build and runtime selects are gone from the form, the mode stays Plan), a row of cat cards, Needs you (an Inbox item
has Use default, Answer…, Later, Dismiss; a task waiting on a decision has Open), Ready to merge (Merge), Done today. `view/cats.rs`: the
page is the prototype's bar (48), the cat's Briefing file as its first message (else the quiet "next build" line), "Try an example request"
and a static composer (slice 2 makes it live), and the panel is Overview / Files / Rounds with the prototype's sections. Faces are still the
vector ones (`cat_face.rs`, one place to swap).

### Measured against the prototype

`apps/neko-rune/scripts/look-pairs.sh` renders the scenes `look-home` and `look-cat` (the prototype's placeholder data: Asha, miso, mochi,
tofu, the same titles) at 1440x900, 1x, and `look-diff.mjs` compares each with the matching prototype picture (pixelmatch, threshold 0.1,
needs `pngjs` and `pixelmatch` in `/tmp/neko-look/tools`). Differing pixels, the old UI on the same scenes, then now:

| Pair | before dark / light | now dark / light |
| --- | --- | --- |
| Home (`home-0`) | 12.9% / 10.5% | 2.6% / 2.6% |
| Cat page (`cat-miso-0`) | 2.6% / 2.5% | 1.8% / 1.8% |
| Sidebar region (home) | 7.9% / 7.6% | 3.0% / 3.0% |
| Sidebar region (cat) | 8.5% / 7.6% | 3.0% / 3.0% |

A mostly flat page understates a placement error, so the pictures matter too (`/tmp/neko-look/*-pair.png`: Rune, prototype, difference).
Known differences: the vector faces and the Neko mark are not the prototype's illustrations or white square; the workspace switcher says "All
workspaces" where the prototype says "Acme"; a need is a task row with one Open button where the prototype shows the question card's four
buttons (an Inbox item has all four); the cat's cost reads $0.00 (the ledger has no per-cat figure); Done rows show no per-task cost; the
cat page's composer is static until slice 2; Settings keeps its existing page layout under the new tokens (the prototype's Settings page,
Issues, Rounds detail and the question card are later slices).

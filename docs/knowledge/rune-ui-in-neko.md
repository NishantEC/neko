# rune-ui in Neko (S0, 2026-11-26; v0.10.0 since 2026-11-27)

rune-ui v0.10.0, on Rune `53f6d36a96439d357a6f2a2560383d7b9e9ec9c2`. Its components are copied into `apps/neko/src/ui/` by its CLI
and recorded in `apps/neko/rune-ui.lock`.

## Installing

Build the CLI from a shallow clone of the tag in a cache folder, never from a working copy:

```sh
git clone --depth 1 --branch v0.8.1 https://github.com/NishantEC/rune-ui ~/Library/Caches/neko/rune-ui-src  # privacy-ok: rune-ui's public repository
cd ~/Library/Caches/neko/rune-ui-src && cargo build --release -p rune-ui-cli
cd apps/neko && <cli> init --git https://github.com/NishantEC/rune-ui --tag v0.8.1  # privacy-ok: rune-ui's public repository
<cli> add sidebar scroll_area button tooltip prompt_input message conversation empty kbd popup font_figtree
```

`init` wrote the `rune`, `rune-kit`, `rune-ui-style`, `rune-ui-themes` dependencies and Rune's `[patch.crates-io]` table (eight
crates, all at the same rev). `add` brought `base`, `icons`, `agent` and `font_inter` along.

The polish round moved to v0.9.0: both git dependencies on `tag = "v0.9.0"`, `rune-ui update`, then `rune-ui add task`. The CLI is
built from the tag in `$NEKO_TARGET/rune-ui-src` into `$NEKO_TARGET/cli`.

0.10.0 (2026-11-27): both git dependencies on `tag = "v0.10.0"`, `rune-ui update` (a three-way merge into the copies; `.rune-ui/base`
keeps what was last written), then `rune-ui add portrait receipt reorder`. Rune itself is taken with `default-features = false` (see
"Licences"). `receipt` and `reorder` are installed and not used yet (below).

The undo slice added `diff` with the same CLI (`rune-ui add diff`).

Workspaces added `dropdown_menu`, `dialog`, `alert_dialog`, `input`, `segmented_control` and `select` (the last is installed but not
used; see below) with the same CLI.

S1 added `response`, `reasoning` (with `shimmer`), `tool`, `confirmation`, `badge`, `radio_group` and, after the first real run,
`context` with the same CLI (`rune-ui add …`); `response` turned on rune-kit's `markdown` feature (pulldown-cmark, MIT). Rune's `native-file-dialog` feature brings rfd (MIT).

## Rules

- **Never hand-edit a copy.** `tests/rune_ui.rs` hashes every file the lock lists and fails on drift, and fails on a file among the
  copies the lock does not know. Fix a component upstream, then `rune-ui update`.
- What a component lacks is added on the built `El` in Neko's own code, never in the copy: the task rows are `SidebarItem`s with
  `.role("option")`, `.position_in_set`, `.focus_selected` and an `on_key` added after `build`.
- **Theme:** `apps/neko/themes/neko.json` is rune-ui's default with a base radius of 9.6 (drawn as 12: continuous corners are drawn 1.25 times larger), continuous corners, Figtree, type scale 1.00,
  Comfortable density (1.0), shadow 1.0 (elevation as the default) and material 100% (texture 1.0). Loaded with
  `Theme::from_json`, given Reduce Motion every frame.
- **Fonts:** `AppSpec::fonts` = `rune_ui_style::fonts::install(ui::FONTS)` (Figtree and Inter). Every visible text is Figtree
  (tested).
- **Tooltips** are Rune's own (`El::help`); the view sets `cx.help_look(ui::tooltip::look(&t))`.
- **Overlays:** the view mounts `ui::base::overlay_root` once at the root (S0 opens none).

## Rune's Dock issue

At this rev Rune makes a symbol image on a short-lived launch thread, and the Dock may then call the app "Application Not
Responding". `dock::register_on_main_thread` asks for a symbol image on the main thread before Rune starts, for a real window
only (tested by reading `lib.rs`).

## What Neko uses from 0.10.0 (its own stand-ins are gone)

- **Theme file** (`apps/neko/themes/neko.json`): the teal is the theme's `color.identity` ramp and `role.identity` (the `blue` ramp is the
  kit's blue again; `info`, `accent` and `ring` point at the identity ramp), House's type scale is `type.<step>`, the dialog scrim is
  `number.scrim` (0.18 light, 0.55 dark; black). `view::theme` is `Theme::from_json` and Reduce Motion, nothing else.
  `base.keys-font` is left out: the keys' face is then the theme's own, Figtree (Kbd passes the Figtree test).
- **Portrait** (`faces::portrait`, `faces::portrait_in`): `Portrait::new(key, name, size).face(drawing).ground(..)` (or `.image(..)` for the
  development art), `.ringed(true)`, `.state(Activity::..)`, `.under(surface)`, `.decorative(true)`. Neko keeps the drawn faces, their
  grounds, `wash` and the art override. Every portrait takes a key.
- **Message**: the person's words are `Message::user(..).bubble(Bubble::Well)` with `reveal_actions`; a turn's head is `Message::header`.
- **SidebarItem**: `.button(true)` (New task, Add workspace, Show more) and `.trail(pill)` (Needs you).
- **Marks**: `agent::mark_with(.., Marks::Neutral)` for a step's and an answered request's mark, `Reasoning` with `.icon(None)` for a
  thought, `agent::hollow_dot` for the run line. A denied answer is the dash, as a denied step is (it was a cross).
- **Diff** in Review: `.stats`, `.hide_zero(true)`, `.selectable(true)`, `.foot(..)`; no copy of its header or lines.
- **Context**: `.placement(Placement::TopStart)` and `.limit_assumed(..)`.
- **Status::Stopped** for a stopped task's mark on a cat's page.
- Still from 0.9.0: Message's reveal and gaps, Context's ring, `Response` `selectable` and `muted`, `Confirmation`'s `why` and
  `destructive` (unused: Neko's request card is its own, below), `Task`'s facts (unused since the receipt), `SidebarItem` `lead`,
  `list_row` and `description`.

## What rune-ui cannot do here (for upstream)

Neko tried each of these on 0.10.0 and kept its own, because the render got worse. Each says what would let it go.

- **The Cattains' and threads' two-line rows** (`sidebar::face_row`, `thread_row`): `SidebarItem::subtitle` is plain words in one ink. Neko's second line is
  parts in their own inks (the state word in its colour, the round's mark before it). Ask: `SidebarItem::subtitle_el(El)`.
- **A resizable split** (the sidebar's edge): the sidebar is fixed at 256 points because rune-ui has no component that drags an edge
  between two regions with a minimum and a maximum (208 and 360 for the sidebar). Ask: `Split` with `min`, `max`, a saved width and a
  keyboard way to move it. Neko will take it the day it lands.
- **A thread's row** (`sidebar::thread_row`, `row_frame`) is the same ask as the Cattains' rows (`subtitle_el`), plus a trailing slot
  that swaps its content under the pointer (the age gives way to a menu).
- **The run receipt** (`turn::receipt`): `Receipt` draws the failed sentence in the plain ink (Neko's is red: "miso could not
  finish", and muted once undone), has no branch glyph, puts the note above the files, and cannot say "and N more" when the run
  changed more files than the list holds. Ask: `Receipt::sentence_ink(Color)` (or a tone), `.branch_icon`, `.note_after_files`, `.more(n)`.
- **The composer's portrait in the margin** (`task_view`): `PromptInput::lead` puts it inside the field, which makes the one-line
  composer taller and moves the text off the reading edge. Ask: a lead that hangs in the margin (`lead_outside`).
- **Tool and Confirmation** (`turn::step`, `turn::request`): `Tool` repeats a given line at the top of the fold, adds the time after an
  exit note, and says "Failed" where Neko says "Could not run" (and drops "It did not finish."). `Confirmation`'s answered mark is the
  icon's full size, two points more than a step's tick, so its words do not line up with the steps'. Ask: `Tool::failed_note`,
  `marks_size`.
- **DropdownMenu names** (`view::menu_named`): the open surface is named "Menu" now, not nothing; Neko names it for its list ("Cats",
  "Thread options"). Ask: `DropdownMenu::name(..)`.
- **Shortcuts in menu rows** (`Row::shortcut`): the row's keys are drawn in the keycap face, but `shortcut_hint` makes the element carry
  its text in Inter, and `tests/structure.rs` ("Figtree everywhere") fails on it. Neko tried ⌥⌘↑ and ⌥⌘↓ on Move up and Move down
  and took them out again. Ask: the hint's text in the theme's keycap face.
- **Reorder** for the projects: the groups have different heights (a header and its threads), and `Reorder` takes rows of one pitch.
  Ask: variable pitch. Until then projects are ordered by the menu and ⌥⌘↑ and ⌥⌘↓.
- **Select** is installed and unused by choice: the composer's workspace and cat pickers are chips that open a menu with the current
  row checked, as a Mac pop-up button does; 0.10's Select now says "2 of 5" but is a field-shaped control.
- `Status::Stopped` gave the stopped mark; `stopped_card` and the appended Review and Undo row no longer exist (the receipt replaced
  them), so there was nothing to delete for `Task::secondary`.

## Licences

Rune, rune-ui: MIT. Figtree, Inter: OFL 1.1. The crates are listed in `THIRD_PARTY_LICENSES/crates.md`
(`python3 scripts/licenses.py` rewrites it and fails on a licence not reviewed); their own licence files, which name their
authors, are written into the bundle only (`build-app.sh`), so the privacy check stays strict. Through Rune also come permissive crates under
Zlib, BSD-2/3-Clause, ISC, Unicode-3.0 and Unlicense. No MPL-2.0 crate ships any more (0.10.0, 2026-11-27): the `symphonia` and
`opuscule` crates (Rune's Ogg audio decoders, MPL-2.0) came through Rune's default `ogg-audio` feature, and `apps/neko/Cargo.toml`
now takes Rune with `default-features = false` and names the six other defaults it uses (`accessibility`, `dbus`,
`embedded-fonts`, `hunspell`, `metal-backend`, `svg`) and `native-file-dialog`. `cargo tree -i symphonia` and `-i opuscule` find
no package, `Cargo.lock` has neither, and `scripts/licenses.py` no longer lists MPL-2.0 among the licences it accepts, so one that
comes back fails the check. Neko plays no audio. Cargo unifies features: any new dependency that turns Rune's defaults back on
brings them back, and the licence script will say so.

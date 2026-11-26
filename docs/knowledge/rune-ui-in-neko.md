# rune-ui in Neko (S0, 2026-11-26; v0.9.0 since the polish round)

rune-ui v0.9.0, on Rune `53f6d36a96439d357a6f2a2560383d7b9e9ec9c2`. Its components are copied into `apps/neko/src/ui/` by its CLI
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

## What Neko uses from 0.9.0 (its own stand-ins are gone)

- **Message:** the person's Copy shows on hover or focus with `reveal_actions` and takes no room while hidden (`App::reveals`).
  A run is one assistant turn: its blocks are `BLOCK_GAP` apart, turns `TURN_GAP` (tested: nothing sits further apart).
- **Context:** the ring face and the kit's popover, with the note line for a subscription only (`task_view::cost_note`).
- **Tool** with `label` (Neko's phrases), `note` ("exit 1") and `Status::Denied`; **Reasoning**; **FoldLine** `indent(false)`.
- **Response** `selectable` and `muted` (interim words: words with a step after them, and every word inside a folded run).
- **Confirmation** `why` and `destructive`; **Task** (the run card) with only changes and a branch.
- **SidebarItem** `lead` (the task's mark, `agent::mark`), `list_row` and `description`. Hit areas are role `none`: the
  accessibility test no longer allows unnamed buttons.
- `RadioGroup` descriptions wrap to the group's width; Neko had no workaround to remove.

## What rune-ui cannot do here (for upstream)

After 0.9.0:

- `task::Run` has no "Stopped" status: a stopped or interrupted run's pill would say "Skipped". Neko keeps a small card of its own
  for those two states (`stopped_card`). Ask: `Status::Stopped`, or a status word on `Run`.
- `Context`'s popover opens `TopEnd`; with the ring at the composer's left it hangs out over the sidebar. Ask: `.placement(..)`.
- `Context::Usage` cannot say the window was assumed (not reported). Ask: `limit_assumed: bool` or a second note.
- `SidebarItem` has no button form: the New task row adds `.role("button")` on the built row.

From the workspaces slice:

- `Select`'s options are `option`s in a `listbox` without `position_in_set` (and the current one says `unselected`), so Neko's
  accessibility rules fail on it. The composer's picker is a DropdownMenu with checked rows instead (a Mac pop-up button is a
  menu too). Ask: `position_in_set(i + 1, n)` and `selected` on each option in `popup::Popup::surface` for role `listbox`.
- `popup::mount` makes the open surface a modal with no name. Neko adds `.label(..)` on the built surface (`view::menu_named`).
  Ask: `Popup::surface` labels its content with `self.name`, or `DropdownMenu::name(..)`.
- `popup::Row::shortcut` sets its keys in Inter, which Neko's "Figtree everywhere" rule refuses; Move up and Move down show no
  keys. Ask: the shortcut in the theme's face (or its keys font from the theme).
- No sortable list: reordering workspaces is by keyboard and menu only. Ask: a drag-to-reorder helper for a column of rows.

From the undo slice (`diff` added with the CLI):

- `Diff` draws its lines as plain text: they cannot be selected. Ask: `Diff::selectable(bool)` passing `.selectable(true)` to each
  line's words (and one `selection_group` over the lines). Neko offers Copy diff in each open file's foot instead.
- `Diff`'s header counts the lines it is given, so a folded file (no lines yet) or a cut one shows the wrong `+n −n`. Neko gives a
  folded file that many empty marks. Ask: `Diff::stats(added, removed)` to set the counts.
- `Diff` has no place for a file's actions or note (renamed from, binary, Show all, Copy path, Reveal). Neko appends a foot row to
  the built slab. Ask: `Diff::foot(El)` or `actions(..)`, and a `binary`/`note` line style.
- `Task` (the run card) has one primary action only. Neko appends a row with Review and Undo to the built card. Ask:
  `Task::secondary(words, f)` or an actions row.

Still open from S0: `SidebarItem` rows outside a list are `role("link")`.

## Licences

Rune, rune-ui: MIT. Figtree, Inter: OFL 1.1. The crates are listed in `THIRD_PARTY_LICENSES/crates.md`
(`python3 scripts/licenses.py` rewrites it and fails on a licence not reviewed); their own licence files, which name their
authors, are written into the bundle only (`build-app.sh`), so the privacy check stays strict. Through Rune also come permissive crates under
Zlib, BSD-2/3-Clause, ISC, Unicode-3.0 and Unlicense, and two MPL-2.0 crates: `symphonia` and `opuscule`, Rune's Ogg audio
decoders. MPL-2.0 is a weak, file-level copyleft. They arrive because `rune-ui-style` depends on `rune` with its default features,
which include `ogg-audio`; Neko cannot turn it off. Upstream fix: `rune-ui-style` and `rune-ui-themes` depend on `rune` with
`default-features = false` (or Rune drops `ogg-audio` from its defaults). Open question for the owner before shipping.

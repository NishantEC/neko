# rune-ui in Neko (S0, 2026-11-26)

rune-ui v0.8.0, on Rune `53f6d36a96439d357a6f2a2560383d7b9e9ec9c2`. Its components are copied into `apps/neko/src/ui/` by its CLI
and recorded in `apps/neko/rune-ui.lock`.

## Installing

Build the CLI from a shallow clone of the tag in a cache folder, never from a working copy:

```sh
git clone --depth 1 --branch v0.8.0 https://github.com/NishantEC/rune-ui ~/Library/Caches/neko/rune-ui-src  # privacy-ok: rune-ui's public repository
cd ~/Library/Caches/neko/rune-ui-src && cargo build --release -p rune-ui-cli
cd apps/neko && <cli> init --git https://github.com/NishantEC/rune-ui --tag v0.8.0  # privacy-ok: rune-ui's public repository
<cli> add sidebar scroll_area button tooltip prompt_input message conversation empty kbd popup font_figtree
```

`init` wrote the `rune`, `rune-kit`, `rune-ui-style`, `rune-ui-themes` dependencies and Rune's `[patch.crates-io]` table (eight
crates, all at the same rev). `add` brought `base`, `icons`, `agent` and `font_inter` along.

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

## What rune-ui cannot do here (for upstream)

- `PromptInput` has no spell check switch (its inner `text_input` never calls `.spellcheck`).
- Hit areas (`hit(<key>)`) are unnamed buttons in the accessibility tree.
- `SidebarItem` rows are `role("link")` with `checked` for the current page; a list of choosable rows wants `option` with
  `focus_selected` (Neko adds those on the built row).

## Licences

Rune, rune-ui: MIT. Figtree, Inter: OFL 1.1. The crates are listed in `THIRD_PARTY_LICENSES/crates.md`
(`python3 scripts/licenses.py` rewrites it and fails on a licence not reviewed); their own licence files, which name their
authors, are written into the bundle only (`build-app.sh`), so the privacy check stays strict. Through Rune also come permissive crates under
Zlib, BSD-2/3-Clause, ISC, Unicode-3.0 and Unlicense, and two MPL-2.0 crates: `symphonia` and `opuscule`, Rune's Ogg audio
decoders. MPL-2.0 is a weak, file-level copyleft. They arrive because `rune-ui-style` depends on `rune` with its default features,
which include `ogg-audio`; Neko cannot turn it off. Upstream fix: `rune-ui-style` and `rune-ui-themes` depend on `rune` with
`default-features = false` (or Rune drops `ogg-audio` from its defaults). Open question for the owner before shipping.

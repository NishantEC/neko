//! rune-ui registry: font_figtree 0.5.0 (a font). Copied into your app by `rune-ui add font_figtree`; yours to change.
//!
//! Figtree by Erik Kennedy, under the SIL Open Font License 1.1 (`OFL.txt` beside this file; keep it with the font). A friendly geometric sans with round dots and a small x-height. Variable weight 300 to 900. Its x-height is smaller than Inter's, so the ramp is drawn 8% larger (`size_adjust`).
//! A theme names it as `"font": { "$value": "figtree" }` once the app has installed it at start-up:
//! `AppSpec { fonts: rune_ui_style::fonts::install(ui::FONTS), .. }`. Source and version: this item's `item.toml`.

use rune_ui_style::Font;

/// Figtree, 61 KB.
pub const FONT: Font = Font::embedded("figtree", "Figtree", "Figtree", &[include_bytes!("Figtree-Variable.ttf")]).size_adjust(1.08);

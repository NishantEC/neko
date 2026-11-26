//! rune-ui registry: font_inter 0.5.0 (a font). Copied into your app by `rune-ui add font_inter`; yours to change.
//!
//! Inter by Rasmus Andersson, under the SIL Open Font License 1.1 (`OFL.txt` beside this file; keep it with the font). The neo-grotesque the type ramp is tuned in; static 400, 500 and 600 (the same files Rune embeds). Every other font item needs it: Rune drops its own Inter once an app registers fonts.
//! A theme names it as `"font": { "$value": "inter" }` once the app has installed it at start-up:
//! `AppSpec { fonts: rune_ui_style::fonts::install(ui::FONTS), .. }`. Source and version: this item's `item.toml`.

use rune_ui_style::Font;

/// Inter, 1219 KB.
pub const FONT: Font = Font::embedded("inter", "Inter", "Inter", &[include_bytes!("Inter-Regular.ttf"), include_bytes!("Inter-Medium.ttf"), include_bytes!("Inter-SemiBold.ttf")]);

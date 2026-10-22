import SwiftUI

/// The quick panel's colours for each theme the daemon can select. Mirrors the
/// Rust built-in themes (`crates/neko/src/theme.rs`) by id.
struct PaletteTheme {
    let light: Bool
    let surface: Color
    let alpha: Double
    let selected: Color
    let text: Color
    static func named(_ id: String) -> PaletteTheme {
        let values: [String: (Bool, UInt32, Double, UInt32, UInt32)] = [
            "neutral": (false, 0x0d0d0d, 0.64, 0x2f2f2f, 0xe8e8e8),
            "ember": (false, 0x350915, 0.86, 0x7b3820, 0xf9eee0),
            "catnap": (false, 0x4b115b, 0.9, 0xa0186f, 0xfff2fd),
            "glass": (false, 0x0f1a2b, 0.64, 0x2e4466, 0xf2f6fc),
            "catppuccin-mocha": (false, 0x1e1e2e, 0.84, 0x45475a, 0xcdd6f4),
            "catppuccin-macchiato": (false, 0x24273a, 0.84, 0x494d64, 0xcad3f5),
            "catppuccin-frappe": (false, 0x303446, 0.86, 0x51576d, 0xc6d0f5),
            "catppuccin-latte": (true, 0xeff1f5, 0.88, 0xccd0da, 0x4c4f69),
            "gruvbox-dark": (false, 0x282828, 0.85, 0x504945, 0xfbf1c7),
            "gruvbox-light": (true, 0xfbf1c7, 0.9, 0xd5c4a1, 0x282828),
            "solarized-dark": (false, 0x002b36, 0.86, 0x0d4553, 0xfdf6e3),
            "solarized-light": (true, 0xfdf6e3, 0.9, 0xd9d2ba, 0x002b36),
            "nord": (false, 0x2e3440, 0.85, 0x434c5e, 0xeceff4),
            "tokyo-night": (false, 0x24283b, 0.85, 0x3b4261, 0xc0caf5),
            "rose-pine": (false, 0x191724, 0.86, 0x403d52, 0xe0def4),
            "rose-pine-dawn": (true, 0xfaf4ed, 0.9, 0xcecacd, 0x555076),
            "dracula": (false, 0x282a36, 0.85, 0x44475a, 0xf8f8f2),
            "everforest-dark": (false, 0x2d353b, 0.86, 0x3d484d, 0xd3c6aa)
        ]
        let value = values[id] ?? values["neutral"]!
        func color(_ hex: UInt32) -> Color { Color.adaptive(light: hex, dark: hex) }
        return PaletteTheme(light: value.0, surface: color(value.1), alpha: value.2, selected: color(value.3), text: color(value.4))
    }
}

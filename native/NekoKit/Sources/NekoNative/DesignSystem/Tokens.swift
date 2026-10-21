import SwiftUI

// The single owner of every visual value in the native client. Screens read
// these tokens; they never repeat a number or a colour. Rules and the
// component catalogue: docs/design/principles.md. The guard tests in
// Tests/NekoNativeTests/DesignSystemGuardTests.swift keep literals out of
// everything outside this folder.

// MARK: - Colour

/// Neutral surfaces, text and lines, in true neutral grey (chroma 0). Each
/// surface step has one job: `sunken` sits below the canvas (inputs, code,
/// wells), `panel` and `card` rise above it. Small secondary text stays
/// readable on the brightest content surface and on a selected row in both
/// appearances (`text4` on `selected`: 4.53:1 light, 5.04:1 dark; do not
/// raise `selected` above 0.065 without darkening `text4`).
enum N {
    static let canvas = Color.adaptive(light: 0xF6F6F6, dark: 0x0E0E0E)
    static let panel = Color.adaptive(light: 0xFBFBFB, dark: 0x151515)
    static let card = Color.adaptive(light: 0xFFFFFF, dark: 0x1A1A1A)
    static let sunken = Color.adaptive(light: 0xF0F0F0, dark: 0x0A0A0A)
    /// Attachment chips, on the accent's hue so they rhyme with it.
    static let attachment = Color.adaptive(light: 0xEEEFF8, dark: 0x252530)
    static let selected = Color.primary.opacity(0.065)
    /// The user's own messages: a faint wash of the accent over a card.
    static let accentWash = Color.adaptive(light: 0x6554E8, dark: 0x6A5CED, lightOpacity: 0.10, darkOpacity: 0.16)
    static let line = Color.adaptive(light: 0xE3E3E3, dark: 0x2A2A2A)
    static let lineStrong = Color.adaptive(light: 0xBEBEBE, dark: 0x4A4A4A)
    static let text = Color(nsColor: .labelColor)
    static let text2 = Color.adaptive(light: 0x3D3D3D, dark: 0xD6D6D6)
    static let text3 = Color.adaptive(light: 0x5B5B5B, dark: 0xB1B1B1)
    static let text4 = Color.adaptive(light: 0x6C6C6C, dark: 0x989898)
    /// A strong fill that flips with the appearance (the composer's send button), and text on it.
    static let inverse = Color.adaptive(light: 0x29292D, dark: 0xECEDEF)
    static let onInverse = Color.adaptive(light: 0xFFFFFF, dark: 0x19191C)
    /// A sidebar row, matching macOS's medium source-list row.
    static let rowHeight: CGFloat = 28
}

/// Colour marks actions and state; content stays neutral. One meaning each.
/// The status quartet shares one OKLCH lightness per appearance (about 0.52
/// light, 0.80 dark), so no state shouts louder than another.
enum NekoStyle {
    /// Iris, neko's one identity colour (oklch 0.55–0.57, 0.21, h 282): the
    /// primary action, selection and focus. White on it: 5.27 light, 4.81
    /// dark. Brightening it in dark mode means moving `onAccent` to near-black.
    static let accent = Color.adaptive(light: 0x6554E8, dark: 0x6A5CED)
    /// Accent-coloured text and links on a surface.
    static let accentText = Color.adaptive(light: 0x5D4ED6, dark: 0xA5A8F8)
    static let onAccent = Color.white
    static let amber = Color.adaptive(light: 0x915C08, dark: 0xF2B966) // needs you
    static let sky = Color.adaptive(light: 0x196EA9, dark: 0x80C7F8)   // ready to review
    static let mint = Color.adaptive(light: 0x187C49, dark: 0x75D5A0)  // done
    static let coral = Color.adaptive(light: 0xBA3535, dark: 0xF58B84) // failed
    /// Text and glyphs on a filled status colour.
    static let onStatus = Color.adaptive(light: 0xFFFFFF, dark: 0x111111)
    static let lilac = Color.oklch(0.74, 0.09, 300)                    // workspace tag only
    static let rose = Color.oklch(0.74, 0.09, 350)                     // workspace tag only
}

/// Workspace marker colours, assigned by position.
func workspaceColor(_ index: Int) -> Color { [NekoStyle.mint, NekoStyle.sky, NekoStyle.lilac, NekoStyle.amber, NekoStyle.rose, NekoStyle.coral][index % 6] }

/// Pastel sample tints for Design lab's preview artifacts and orb.
enum PearlTint {
    static let mist = Color(red: 0.84, green: 0.86, blue: 0.93)
    static let lilac = Color(red: 0.83, green: 0.74, blue: 0.91)
    static let mint = Color(red: 0.72, green: 0.87, blue: 0.82)
    static let lavender = Color(red: 0.90, green: 0.87, blue: 0.96)
    static let aqua = Color(red: 0.78, green: 0.93, blue: 0.94)
}

// MARK: - Type

/// Text steps. Pick the role (`body`, `meta`, `heading`) where one fits; use
/// `of(_:weight:)` for symbol sizes and the few places a role does not.
enum NekoTextStep: CGFloat, CaseIterable {
    case micro = 10, small = 11, caption = 12, meta = 13, callout = 14, body = 15
    case subtitle = 17, title = 22, headline = 24, display = 32, hero = 40
}

/// Type roles. Display and title use SF Pro Rounded, neko's voice; body and
/// UI text stay SF Pro. Shells apply `NekoTracking` and `NekoLeading` with them.
enum NekoFont {
    static let display = Font.system(size: NekoTextStep.display.rawValue, weight: .bold, design: .rounded)
    static let title = Font.system(size: NekoTextStep.title.rawValue, weight: .semibold, design: .rounded)
    /// A section heading inside a page, one step above body.
    static let section = Font.system(size: NekoTextStep.subtitle.rawValue, weight: .semibold)
    /// Row titles.
    static let heading = Font.system(size: NekoTextStep.body.rawValue, weight: .semibold)
    static let body = Font.system(size: NekoTextStep.body.rawValue)
    static let chat = body
    static let meta = Font.system(size: NekoTextStep.meta.rawValue)
    static let caption = Font.system(size: NekoTextStep.caption.rawValue)
    static let label = Font.system(size: NekoTextStep.small.rawValue, weight: .medium)
    static let mono = Font.system(size: NekoTextStep.caption.rawValue, weight: .regular, design: .monospaced)
    static func of(_ step: NekoTextStep, weight: Font.Weight = .regular, design: Font.Design = .default) -> Font {
        .system(size: step.rawValue, weight: weight, design: design)
    }
    /// A size derived from a component's own dimension (glyphs inside a badge or ring).
    static func scaled(_ size: CGFloat, weight: Font.Weight = .regular, design: Font.Design = .default) -> Font {
        .system(size: size, weight: weight, design: design)
    }
}

/// Letter spacing in points: large type tracks tight, small capitals track open.
enum NekoTracking {
    static let display: CGFloat = -0.5   // about −0.016em at 32pt
    static let title: CGFloat = -0.2     // about −0.01em at 22pt
    static let overline: CGFloat = 0.6   // about +0.05em at 11pt
}

/// Extra space between wrapped lines. SF at 15pt sets an 18pt line, so
/// `body` is about 1.47 and `reading` about 1.6.
enum NekoLeading {
    static let tight: CGFloat = 2
    static let body: CGFloat = 4
    /// Paragraphs read for more than a glance: saved text, long messages.
    static let reading: CGFloat = 6
}

// MARK: - Geometry

/// Corner radii, smallest to largest. Every rounded shape uses one of these.
/// Nested shapes are concentric: outer = inner + padding (the composer's
/// `hero` 28 is its `md` 10 chips plus an 18pt inset).
enum NekoRadius {
    static let micro: CGFloat = 3     // status swatches, tiny markers
    static let xs: CGFloat = 5        // key caps
    static let sm: CGFloat = 8        // chips, tags, list rows, icon tiles
    static let md: CGFloat = 10       // controls, small cards, attachments
    static let lg: CGFloat = 16       // cards and blocks inside a page
    static let xl: CGFloat = 20       // panels, grouped sections
    static let hero: CGFloat = 28     // the composer and the largest floating surfaces
}

/// Spacing steps for stacks and padding.
enum NekoSpace {
    static let hair: CGFloat = 2
    static let xxs: CGFloat = 4
    static let xs: CGFloat = 6
    static let sm: CGFloat = 8
    static let md: CGFloat = 12
    static let lg: CGFloat = 16
    static let xl: CGFloat = 20
    static let xxl: CGFloat = 24
    static let page: CGFloat = 32
}

/// Page geometry. Board columns and native chrome fill their container instead.
enum NekoLayout {
    static let pageWidth: CGFloat = 880
    static let readingWidth: CGFloat = 760
    static let pageInset: CGFloat = 40
    /// Between sections: 2.5× the 16pt gap inside one, so groups read by space alone.
    static let sectionGap: CGFloat = 40
    /// A page title to its message.
    static let introGap: CGFloat = 6
    static let rowInset: CGFloat = 18
    static let messageGap: CGFloat = 24
    static let authorGap: CGFloat = 8
    static let messageInset: CGFloat = 16
    /// The side margin macOS draws inside a grouped Form.
    static let groupedFormInset: CGFloat = 20
    /// A square icon-only button in chrome (the sidebar's New task).
    static let iconButton: CGFloat = 28
}

/// How wide a page's content column may grow.
enum NekoPageWidth {
    case management, reading, full
    var points: CGFloat {
        switch self {
        case .management: NekoLayout.pageWidth
        case .reading: NekoLayout.readingWidth
        case .full: .infinity
        }
    }
}

/// Fixed sheet widths, so every modal of one kind is the same size.
enum NekoSheetWidth {
    case narrow, standard, wide
    var points: CGFloat {
        switch self {
        case .narrow: 480
        case .standard: 620
        case .wide: 720
        }
    }
}

// MARK: - Colour constructors

extension Color {
    /// AppKit resolves these in the hosting window's appearance, including sheets.
    /// Opacities let a wash (a tint over whatever surface is below) differ by appearance.
    static func adaptive(light: UInt32, dark: UInt32, lightOpacity: Double = 1, darkOpacity: Double = 1) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            let rgb = isDark ? dark : light
            return NSColor(srgbRed: Double((rgb >> 16) & 0xFF) / 255,
                           green: Double((rgb >> 8) & 0xFF) / 255,
                           blue: Double(rgb & 0xFF) / 255, alpha: isDark ? darkOpacity : lightOpacity)
        })
    }
    /// OKLCH → sRGB (Björn Ottosson). Out-of-gamut channels are clamped.
    static func oklch(_ l: Double, _ c: Double, _ hDegrees: Double, opacity: Double = 1) -> Color {
        let h = hDegrees * .pi / 180
        let a = c * cos(h), b = c * sin(h)
        let l_ = l + 0.3963377774 * a + 0.2158037573 * b
        let m_ = l - 0.1055613458 * a - 0.0638541728 * b
        let s_ = l - 0.0894841775 * a - 1.2914855480 * b
        let L = l_ * l_ * l_, M = m_ * m_ * m_, S = s_ * s_ * s_
        let r = 4.0767416621 * L - 3.3077115913 * M + 0.2309699292 * S
        let g = -1.2684380046 * L + 2.6097574011 * M - 0.3413193965 * S
        let bl = -0.0041960863 * L - 0.7034186147 * M + 1.7076147010 * S
        return Color(.sRGBLinear, red: min(max(r, 0), 1), green: min(max(g, 0), 1), blue: min(max(bl, 0), 1), opacity: opacity)
    }
}

import SwiftUI

// The single owner of every visual value in the native client. Screens read
// these tokens; they never repeat a number or a colour. Rules and the
// component catalogue: docs/design/principles.md. The guard tests in
// Tests/NekoNativeTests/DesignSystemGuardTests.swift keep literals out of
// everything outside this folder.

// MARK: - Colour

/// Neutral surfaces, text and lines. Small secondary text stays readable on
/// the brightest content surface in both appearances.
enum N {
    static let canvas = Color.adaptive(light: 0xF7F8FA, dark: 0x101011)
    static let panel = Color.adaptive(light: 0xFFFFFF, dark: 0x19191C)
    static let card = Color.adaptive(light: 0xFFFFFF, dark: 0x1D1D20)
    static let attachment = Color.adaptive(light: 0xF0EEF3, dark: 0x292631)
    static let selected = Color.primary.opacity(0.065)
    static let line = Color.adaptive(light: 0xE2E2E7, dark: 0x2A2A2E)
    static let lineStrong = Color.adaptive(light: 0xC8C8CE, dark: 0x48484E)
    static let text = Color(nsColor: .labelColor)
    static let text2 = Color.adaptive(light: 0x424954, dark: 0xD7D7DC)
    static let text3 = Color.adaptive(light: 0x606875, dark: 0xB9B9C0)
    static let text4 = Color.adaptive(light: 0x666E7A, dark: 0xA5A5AD)
    /// A strong fill that flips with the appearance (the composer's send button), and text on it.
    static let inverse = Color.adaptive(light: 0x29292D, dark: 0xECEDEF)
    static let onInverse = Color.adaptive(light: 0xFFFFFF, dark: 0x19191C)
    static let rowHeight: CGFloat = 30
}

/// Colour marks actions and state; content stays neutral. One meaning each.
enum NekoStyle {
    static let accent = Color(nsColor: .systemBlue)
    static let amber = Color.adaptive(light: 0x895600, dark: 0xE8BB68) // needs approval
    static let sky = Color.adaptive(light: 0x1765A9, dark: 0x8FC4EF)   // ready to review
    static let mint = Color.adaptive(light: 0x247A50, dark: 0x83D3AC)  // done
    static let coral = Color.adaptive(light: 0xB23838, dark: 0xF19E98) // failed
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
    case subtitle = 17, title = 20, headline = 24, display = 28, hero = 34
}

enum NekoFont {
    static let display = Font.system(size: NekoTextStep.display.rawValue, weight: .semibold)
    static let title = Font.system(size: NekoTextStep.title.rawValue, weight: .semibold)
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

// MARK: - Geometry

/// Corner radii, smallest to largest. Every rounded shape uses one of these.
enum NekoRadius {
    static let micro: CGFloat = 3     // status swatches, tiny markers
    static let xs: CGFloat = 4        // key caps
    static let sm: CGFloat = 6        // chips, tags, list rows
    static let md: CGFloat = 10       // controls, small cards, attachments
    static let lg: CGFloat = 14       // cards and blocks inside a page
    static let xl: CGFloat = 18       // panels, composer, grouped sections
    static let hero: CGFloat = 24     // the largest floating surfaces
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
    static let pageInset: CGFloat = 32
    static let sectionGap: CGFloat = 28
    static let rowInset: CGFloat = 18
    static let messageGap: CGFloat = 24
    static let authorGap: CGFloat = 8
    static let messageInset: CGFloat = 16
    /// The side margin macOS draws inside a grouped Form.
    static let groupedFormInset: CGFloat = 20
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
    static func adaptive(light: UInt32, dark: UInt32) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let rgb = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? dark : light
            return NSColor(srgbRed: Double((rgb >> 16) & 0xFF) / 255,
                           green: Double((rgb >> 8) & 0xFF) / 255,
                           blue: Double(rgb & 0xFF) / 255, alpha: 1)
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

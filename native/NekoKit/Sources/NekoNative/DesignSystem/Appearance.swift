import SwiftUI

/// The user's light/dark choice. `system` follows macOS.
enum NekoAppearance: String, CaseIterable, Identifiable {
    case system, light, dark
    var id: String { rawValue }
    var title: String {
        switch self { case .system: "System"; case .light: "Light"; case .dark: "Dark" }
    }
    var colorScheme: ColorScheme? {
        switch self { case .system: nil; case .light: .light; case .dark: .dark }
    }
}

/// Selectable looks for Home. Content stays neutral in every one.
enum NekoLook: String, CaseIterable, Identifiable {
    case system, ambient, dense, mascot
    var id: String { rawValue }
    var title: String {
        switch self {
        case .system: "Neutral"
        case .ambient: "Ambient"
        case .dense: "Neutral (legacy)"
        case .mascot: "Ambient with mascot"
        }
    }
}

private struct NekoLookKey: EnvironmentKey { static let defaultValue: NekoLook = .ambient }
extension EnvironmentValues {
    var nekoLook: NekoLook {
        get { self[NekoLookKey.self] }
        set { self[NekoLookKey.self] = newValue }
    }
}

/// The Metal shaders bundled with the app (`native/Shaders`). Nil in tests or
/// when the library is missing, so every caller falls back to a plain fill.
enum GemShaders {
    static let library: ShaderLibrary? = Bundle.main.url(forResource: "Neko", withExtension: "metallib").map { ShaderLibrary(url: $0) }
}

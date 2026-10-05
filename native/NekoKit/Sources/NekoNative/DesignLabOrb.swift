import SwiftUI

/// A decorative Metal surface alongside system glass; never a shader on glass itself.
/// Only pointer/press events change its uniforms. There is no idle render timer.
struct DesignLabOrb: View {
    var color: Color = Color(red: 0.84, green: 0.86, blue: 0.93)
    var size: CGFloat = 28
    @Environment(\.designLabPearlFocus) private var focus
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    var body: some View {
        surface
            .frame(width: size, height: size)
            .overlay { Circle().strokeBorder(.white.opacity(contrast == .increased ? 0.9 : 0.3), lineWidth: 0.5) }
            .shadow(color: .black.opacity(0.16), radius: size * 0.08, y: size * 0.06)
            .accessibilityHidden(true)
    }

    @ViewBuilder private var surface: some View {
        if let library = GemShaders.library, !reduceTransparency, contrast != .increased {
            let pointer = reduceMotion ? CGPoint.zero : focus.point
            Circle().fill(.white)
                .colorEffect(library.pearlOrb(
                    .float2(Float(size), Float(size)),
                    .float2(Float(pointer.x), Float(pointer.y)),
                    .float(focus.hovered ? 1 : 0),
                    .color(color), .color(Color(red: 0.78, green: 0.93, blue: 0.94)),
                    .color(.white)
                ))
        } else {
            Circle().fill(color)
        }
    }
}

struct DesignLabPearlFocus: Equatable {
    var point = CGPoint.zero
    var hovered = false
}
private struct DesignLabPearlFocusKey: EnvironmentKey {
    static let defaultValue = DesignLabPearlFocus()
}
extension EnvironmentValues {
    var designLabPearlFocus: DesignLabPearlFocus {
        get { self[DesignLabPearlFocusKey.self] }
        set { self[DesignLabPearlFocusKey.self] = newValue }
    }
}

/// Owns feedback at the whole hit target, including the text beside each bead.
struct DesignLabHoverStyle: ButtonStyle {
    var circular = false
    func makeBody(configuration: Configuration) -> some View {
        DesignLabHoverLabel(configuration: configuration, circular: circular)
    }
}

private struct DesignLabHoverLabel: View {
    let configuration: ButtonStyleConfiguration
    let circular: Bool
    @State private var focus = DesignLabPearlFocus()
    @State private var bounds = CGSize(width: 1, height: 1)
    @Environment(\.isEnabled) private var enabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let hovered = enabled && focus.hovered
        let scale = reduceMotion || !enabled ? 1.0 : configuration.isPressed ? 0.97 : hovered ? 1.025 : 1.0
        configuration.label
            .environment(\.designLabPearlFocus, enabled ? focus : DesignLabPearlFocus())
            .background {
                if !circular {
                    RoundedRectangle(cornerRadius: 11).fill(.primary.opacity(hovered ? 0.085 : 0.025))
                }
            }
            .scaleEffect(scale)
            .animation(reduceMotion ? nil : .smooth(duration: 0.18), value: scale)
            .onGeometryChange(for: CGSize.self) { $0.size } action: { bounds = $0 }
            .onContinuousHover { phase in
                switch phase {
                case .active(let location):
                    focus = DesignLabPearlFocus(point: CGPoint(
                        x: min(1, max(-1, 2 * location.x / max(1, bounds.width) - 1)),
                        y: min(1, max(-1, 2 * location.y / max(1, bounds.height) - 1))
                    ), hovered: true)
                case .ended: focus = DesignLabPearlFocus()
                }
            }
            .onDisappear { focus = DesignLabPearlFocus() }
    }
}

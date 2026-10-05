import SwiftUI

/// Static shading keeps the tactile identity without a renderer or per-frame work.
struct DesignLabOrb: View {
    var color: Color = Color(white: 0.22)
    var size: CGFloat = 28

    var body: some View {
        Circle()
            .fill(RadialGradient(
                stops: [
                    .init(color: .white.opacity(0.95), location: 0),
                    .init(color: color.opacity(0.65), location: 0.22),
                    .init(color: color, location: 0.50),
                    .init(color: .black.opacity(0.75), location: 0.88),
                    .init(color: color, location: 1)
                ],
                center: .init(x: 0.28, y: 0.22), startRadius: 0, endRadius: size * 0.85
            ))
            .overlay { Circle().strokeBorder(.white.opacity(0.18), lineWidth: 0.5) }
            .frame(width: size, height: size)
            .shadow(color: color.opacity(0.18), radius: 3, y: 3)
            .accessibilityHidden(true)
    }
}

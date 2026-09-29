import SwiftUI

/// Draws one frame of an activity's pixel grid: a soft bloom under the lit
/// cells, faint "off" cells for structure, then crisp lit cells on top.
enum PixelGlyphRenderer {
    static func draw(_ activity: NekoActivity, t: Double, glyph rect: CGRect, in context: inout GraphicsContext) {
        let n = activity.grid
        let gap = rect.width * (n >= 5 ? 0.07 : 0.09)
        let cell = (rect.width - gap * Double(n - 1)) / Double(n)
        let color = activity.color
        var cells: [(CGRect, Double)] = []
        for y in 0..<n {
            for x in 0..<n {
                guard let value = activity.intensity(x: x, y: y, t: t) else { continue }
                let frame = CGRect(x: rect.minX + Double(x) * (cell + gap), y: rect.minY + Double(y) * (cell + gap), width: cell, height: cell)
                cells.append((frame, min(1, max(0, value))))
            }
        }
        let corner = cell * 0.26
        context.drawLayer { bloom in
            bloom.addFilter(.blur(radius: cell * 1.1))
            for (frame, value) in cells where value > 0.2 {
                bloom.fill(Path(roundedRect: frame.insetBy(dx: -cell * 0.2, dy: -cell * 0.2), cornerRadius: corner), with: .color(color.opacity(min(1, value * 1.1))))
            }
        }
        for (frame, value) in cells {
            let shape = Path(roundedRect: frame, cornerRadius: corner)
            context.fill(shape, with: .color(color.opacity(0.16 + 0.84 * value)))
            if value > 0.5 {
                // A little white in the core makes the brightest cells look lit, not painted.
                context.fill(shape, with: .color(.white.opacity((value - 0.5) * 0.5)))
            }
        }
    }
}

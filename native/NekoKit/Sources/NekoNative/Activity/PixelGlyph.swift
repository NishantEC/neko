import SwiftUI

/// A small glowing pixel grid that shows what Neko is doing. Decorative:
/// the surrounding text always states the activity for VoiceOver.
struct PixelGlyph: View {
    let activity: NekoActivity
    var size: CGFloat = 16
    var animated = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Room around the grid so the bloom is not clipped by the canvas.
    private var bleed: CGFloat { size * 0.75 }
    private var running: Bool { animated && activity.animates && !reduceMotion }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: !running)) { timeline in
            Canvas { context, canvas in
                // Reduce Motion (or a settled state) shows one representative frame.
                let t = running ? timeline.date.timeIntervalSinceReferenceDate : 0.9
                let glyph = CGRect(x: bleed, y: bleed, width: canvas.width - bleed * 2, height: canvas.height - bleed * 2)
                PixelGlyphRenderer.draw(activity, t: t, glyph: glyph, in: &context)
            }
        }
        .frame(width: size + bleed * 2, height: size + bleed * 2)
        .padding(-bleed)
        .accessibilityHidden(true)
    }
}

#Preview {
    HStack(spacing: 24) {
        ForEach(NekoActivity.allCases) { PixelGlyph(activity: $0, size: 22) }
    }.padding(40).background(.black)
}

import SwiftUI

/// Text with a slow highlight sweeping across it while work is under way.
struct ShimmerText: View {
    let text: String
    var active = true
    var font: Font = .system(size: 14, weight: .medium)
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var running: Bool { active && !reduceMotion }

    var body: some View {
        Text(text).font(font).foregroundStyle(N.text2)
            .overlay {
                TimelineView(.animation(minimumInterval: 1 / 30, paused: !running)) { timeline in
                    let cycle = 1.9
                    let phase = timeline.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: cycle) / cycle
                    let center = -0.35 + phase * 1.7
                    Text(text).font(font).foregroundStyle(N.text)
                        .mask {
                            LinearGradient(stops: [.init(color: .clear, location: 0), .init(color: .white, location: 0.5), .init(color: .clear, location: 1)],
                                           startPoint: UnitPoint(x: center - 0.35, y: 0.5), endPoint: UnitPoint(x: center + 0.35, y: 0.5))
                        }
                        .opacity(running ? 1 : 0)
                }
            }
    }
}

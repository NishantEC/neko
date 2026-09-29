import SwiftUI

/// What Neko is doing right now. Each activity has its own pixel motion as
/// well as its own colour, so states stay distinguishable without colour.
enum NekoActivity: String, CaseIterable, Identifiable, Sendable {
    case thinking, analyzing, reading, creating, debugging, watching, needsYou, ready, done, failed, idle

    var id: String { rawValue }

    var label: String {
        switch self {
        case .thinking: "Thinking"
        case .analyzing: "Analyzing"
        case .reading: "Reading"
        case .creating: "Creating"
        case .debugging: "Checking"
        case .watching: "Watching"
        case .needsYou: "Needs you"
        case .ready: "Ready to review"
        case .done: "Done"
        case .failed: "Needs attention"
        case .idle: "Waiting"
        }
    }

    /// One hue per activity. Chroma is high on purpose: these are the only
    /// saturated pixels in the app, which is what makes them read as "alive".
    var color: Color {
        switch self {
        case .thinking: .oklch(0.70, 0.17, 275)
        case .analyzing: .oklch(0.77, 0.16, 58)
        case .reading: .oklch(0.66, 0.21, 25)
        case .creating: .oklch(0.75, 0.15, 355)
        case .debugging: .oklch(0.72, 0.15, 242)
        case .watching: .oklch(0.78, 0.19, 148)
        case .needsYou: NekoStyle.amber
        case .ready: NekoStyle.sky
        case .done: NekoStyle.mint
        case .failed: NekoStyle.coral
        case .idle: N.text4
        }
    }

    /// A second hue for the bloom under an activity capsule.
    var companion: Color {
        switch self {
        case .thinking: .oklch(0.72, 0.15, 350)
        case .analyzing: .oklch(0.72, 0.16, 20)
        case .reading: .oklch(0.74, 0.14, 60)
        case .creating: .oklch(0.76, 0.14, 50)
        case .debugging: .oklch(0.72, 0.14, 200)
        case .watching: .oklch(0.76, 0.12, 190)
        default: color
        }
    }

    /// Cells per side of the pixel grid.
    var grid: Int {
        switch self {
        case .reading, .debugging, .watching: 5
        case .thinking, .analyzing, .creating, .ready: 4
        case .needsYou, .done, .failed, .idle: 3
        }
    }

    /// Settled states hold still; working states move.
    var animates: Bool { ![.done, .idle, .failed, .ready].contains(self) }

    /// Brightness of cell (x, y) at time t, from 0 (unlit) to 1. Nil means the
    /// cell is not part of this glyph's shape and is not drawn at all.
    func intensity(x: Int, y: Int, t: Double) -> Double? {
        switch self {
        case .thinking:
            // A pulse travelling down a staircase diagonal.
            guard abs(x - y) <= 1 else { return nil }
            let phase = (t * 5).truncatingRemainder(dividingBy: 9) - 1
            return max(0.08, 1 - abs(Double(x + y) - phase) / 2.4)
        case .analyzing:
            // A scanner bar sweeping back and forth.
            let period = 6.0, raw = (t * 4).truncatingRemainder(dividingBy: period)
            let head = raw < 3 ? raw : period - raw
            return max(0.08, 1 - abs(Double(x) - head) / 1.7) * (y == 0 || y == 3 ? 0.75 : 1)
        case .reading:
            // Lines of text, read one cell at a time.
            guard y % 2 == 0, y < 4 || x < 3 else { return nil }
            let cells = [0, 2, 4].map { $0 == 4 ? 3 : 5 }.reduce(0, +)
            let index = (y / 2) * 5 + x
            let cursor = (t * 9).truncatingRemainder(dividingBy: Double(cells + 4))
            if Double(index) > cursor { return 0 }
            return cursor - Double(index) < 1 ? 1 : 0.55
        case .creating:
            // A staircase that builds step by step, then clears.
            let steps = [(0, 3), (1, 3), (1, 2), (2, 2), (2, 1), (3, 1), (3, 0)]
            guard let index = steps.firstIndex(where: { $0 == (x, y) }) else { return nil }
            let built = Int(t * 6) % (steps.count + 4)
            if index > built { return 0 }
            return index == built ? 1 : 0.62
        case .debugging:
            // A dense corner that flickers as it tests, fading toward the far edge.
            let distance = Double(x + (4 - y))
            guard distance <= 6 else { return nil }
            let falloff = 1 - distance / 7
            let flicker = 0.45 + 0.55 * Self.noise(x, y, Int(t * 7))
            return falloff * flicker
        case .watching:
            // A light orbiting the edge, leaving a tail.
            let ring = Self.ringIndex(x, y, size: 5)
            guard let ring else { return x == 2 && y == 2 ? 0.25 : nil }
            let head = (t * 10).truncatingRemainder(dividingBy: 16)
            let behind = (head - Double(ring) + 16).truncatingRemainder(dividingBy: 16)
            return behind < 8 ? 1 - behind / 8 : 0.05
        case .needsYou:
            // A plus that breathes slowly, like a raised hand.
            guard x == 1 || y == 1 else { return nil }
            let breath = 0.55 + 0.45 * (0.5 + 0.5 * sin(t * 2.4))
            return x == 1 && y == 1 ? 1 : breath
        case .ready:
            let check = [(0, 2), (1, 3), (2, 2), (3, 1), (3, 0)]
            return check.contains(where: { $0 == (x, y) }) ? 1 : nil
        case .done:
            return 1
        case .failed:
            return x == y || x + y == 2 ? 1 : nil
        case .idle:
            return 0.18
        }
    }

    /// The activity a ticket shows for its daemon status.
    static func forTask(_ status: String) -> NekoActivity {
        switch status {
        case "Planning": .thinking
        case "Building": .creating
        case "Reviewing": .debugging
        case "AwaitingApproval": .needsYou
        case "ReadyForReview": .ready
        case "Completed": .done
        case "Failed": .failed
        default: .idle
        }
    }

    /// Position of a border cell going clockwise from the top-left, or nil
    /// for interior cells.
    static func ringIndex(_ x: Int, _ y: Int, size: Int) -> Int? {
        let last = size - 1
        if y == 0 { return x }
        if x == last { return last + y }
        if y == last { return 2 * last + (last - x) }
        if x == 0 { return 3 * last + (last - y) }
        return nil
    }

    /// Deterministic 0...1 noise so every frame is reproducible.
    static func noise(_ x: Int, _ y: Int, _ frame: Int) -> Double {
        let value = sin(Double(x * 127 + y * 311 + frame * 74) * 12.9898) * 43758.5453
        return value - value.rounded(.down)
    }
}

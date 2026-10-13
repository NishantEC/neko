import SwiftUI

struct ChatScrollMetrics: Equatable {
    var contentHeight: CGFloat = 0
    var originY: CGFloat = 0
    var viewportHeight: CGFloat = 0
}

struct ChatScrollFollowState {
    private(set) var followsLatest = true
    private var previous: ChatScrollMetrics?
    private var pinnedUntil: Date = .distantPast
    /// Sending a message always jumps to it, and the insertion animation's
    /// intermediate offsets must not read as the user scrolling away.
    mutating func pin(for seconds: TimeInterval = 1.2, now: Date = Date()) {
        followsLatest = true
        pinnedUntil = now.addingTimeInterval(seconds)
    }
    mutating func userInteracted() {
        pinnedUntil = .distantPast
        followsLatest = false
    }
    mutating func update(_ metrics: ChatScrollMetrics, now: Date = Date()) {
        if now < pinnedUntil { followsLatest = true; previous = metrics; return }
        // Reply growth must not look like the reader scrolling away. Only an
        // offset change or unchanged content size updates their follow intent.
        if let previous, abs(metrics.contentHeight - previous.contentHeight) < 1 || abs(metrics.originY - previous.originY) > 1 {
            followsLatest = metrics.contentHeight + metrics.originY <= metrics.viewportHeight + 80
        }
        previous = metrics
    }
}

enum ChatComposerClearance {
    static func bottomSpace(for composerHeight: CGFloat) -> CGFloat { composerHeight + 24 }
}

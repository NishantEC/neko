import AppKit
import SwiftUI

/// Observe reading intent without consuming scrolling, selection or disclosure clicks.
/// Geometry changes alone cannot distinguish a growing reply from a user gesture.
struct ChatScrollIntentObserver: NSViewRepresentable {
    var bottomExclusion: CGSize = .zero
    var onInteraction: () -> Void

    func makeNSView(context: Context) -> ObserverView {
        let view = ObserverView()
        view.bottomExclusion = bottomExclusion
        view.onInteraction = onInteraction
        return view
    }

    func updateNSView(_ view: ObserverView, context: Context) {
        view.bottomExclusion = bottomExclusion
        view.onInteraction = onInteraction
    }
    static func dismantleNSView(_ view: ObserverView, coordinator: ()) { view.stopObserving() }

    final class ObserverView: NSView {
        var bottomExclusion: CGSize = .zero
        var onInteraction: (() -> Void)?
        private var monitor: Any?
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            stopObserving()
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.scrollWheel, .leftMouseDown]) { [weak self] event in
                guard let self, let window = self.window, event.window === window,
                      !self.isHiddenOrHasHiddenAncestor,
                      TranscriptInteractionRegion.contains(self.convert(event.locationInWindow, from: nil),
                          bounds: self.bounds, composer: self.bottomExclusion, flipped: self.isFlipped) else { return event }
                if event.type == .leftMouseDown || event.scrollingDeltaY != 0 { self.onInteraction?() }
                return event
            }
        }
        func stopObserving() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }
    }
}

enum TranscriptInteractionRegion {
    static func contains(_ point: CGPoint, bounds: CGRect, composer: CGSize, flipped: Bool) -> Bool {
        let excluded = CGRect(x: bounds.midX - composer.width / 2,
                              y: flipped ? bounds.maxY - composer.height : bounds.minY,
                              width: composer.width, height: composer.height)
        return bounds.contains(point) && !excluded.contains(point)
    }
}

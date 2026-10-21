import AppKit
import SwiftUI

/// Observe reading intent without consuming scrolling, selection or disclosure
/// clicks. Geometry changes alone cannot distinguish a growing reply from a
/// user gesture, but clicks are not intent either: selecting text or opening a
/// disclosure must not stop following. Only a wheel or trackpad scroll toward
/// older content counts.
struct ChatScrollIntentObserver: NSViewRepresentable {
    var bottomExclusion: CGSize = .zero
    var onScrollBack: () -> Void

    func makeNSView(context: Context) -> ObserverView {
        let view = ObserverView()
        view.bottomExclusion = bottomExclusion
        view.onScrollBack = onScrollBack
        return view
    }

    func updateNSView(_ view: ObserverView, context: Context) {
        view.bottomExclusion = bottomExclusion
        view.onScrollBack = onScrollBack
    }
    static func dismantleNSView(_ view: ObserverView, coordinator: ()) { view.stopObserving() }

    final class ObserverView: NSView {
        var bottomExclusion: CGSize = .zero
        var onScrollBack: (() -> Void)?
        private var monitor: Any?
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            stopObserving()
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.scrollWheel]) { [weak self] event in
                guard let self, let window = self.window, event.window === window,
                      !self.isHiddenOrHasHiddenAncestor,
                      ChatScrollIntent.isScrollingBack(deltaY: event.scrollingDeltaY, phase: event.phase),
                      TranscriptInteractionRegion.contains(self.convert(event.locationInWindow, from: nil),
                          bounds: self.bounds, composer: self.bottomExclusion, flipped: self.isFlipped) else { return event }
                self.onScrollBack?()
                return event
            }
        }
        func stopObserving() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }
    }
}

enum ChatScrollIntent {
    /// A positive delta moves the document toward its top, older content.
    /// The phase that ends a gesture carries a stale delta and says nothing new.
    static func isScrollingBack(deltaY: CGFloat, phase: NSEvent.Phase) -> Bool {
        deltaY > 0 && !phase.contains(.ended)
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

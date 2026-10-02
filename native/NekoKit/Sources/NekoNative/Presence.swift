import AppKit
import Combine
import SwiftUI
import NekoKit

/// What the floating capsule should say, from one snapshot and the previous
/// one. Pure, so the linger rule is testable without a window.
struct PresenceState: Equatable {
    let activity: NekoActivity
    let label: String
    /// Outcome states disappear on their own after this long.
    let lingers: Bool

    static let lingerSeconds: Double = 6

    static func from(snapshot: JSONValue, previous: JSONValue?) -> PresenceState? {
        let tasks = snapshot["tasks"].array
        let running = tasks.filter { ["Queued", "Planning", "Building", "Reviewing"].contains($0["status"].string) }
        let replying = snapshot["conversation"].array.contains { $0["pending"].bool }
        let toolWaiting = snapshot["conversation"].array.contains { $0["pending"].bool && $0["tool_calls"].array.contains { $0["status"].string == "awaiting_approval" } }
        if toolWaiting { return PresenceState(activity: .needsYou, label: "Neko needs your OK for a tool", lingers: false) }
        if let task = running.first {
            let activity: NekoActivity = task["status"].string == "Building" ? .creating : task["status"].string == "Reviewing" ? .debugging : .thinking
            let more = running.count > 1 ? " +\(running.count - 1) more" : ""
            return PresenceState(activity: activity, label: "\(friendlyTaskStatus(task["status"].string)): \(task["title"].string.prefix(40))\(more)", lingers: false)
        }
        if replying { return PresenceState(activity: .thinking, label: "Neko is replying", lingers: false) }
        // Something just finished: show how it went, briefly.
        if let previous {
            let before = Dictionary(previous["tasks"].array.map { ($0["id"].string, $0["status"].string) }, uniquingKeysWith: { a, _ in a })
            for task in tasks where ["Queued", "Planning", "Building", "Reviewing"].contains(before[task["id"].string] ?? "") {
                switch task["status"].string {
                case "ReadyForReview": return PresenceState(activity: .ready, label: "Ready to review: \(task["title"].string.prefix(40))", lingers: true)
                case "AwaitingApproval": return PresenceState(activity: .needsYou, label: "Plan ready: \(task["title"].string.prefix(40))", lingers: true)
                case "Failed": return PresenceState(activity: .failed, label: "Needs attention: \(task["title"].string.prefix(40))", lingers: true)
                default: continue
                }
            }
        }
        return nil
    }
}

/// A small floating capsule in the corner while Neko works, Kibu-style:
/// it appears for work, lingers to show the outcome, then hides. It never
/// takes focus; clicking it opens Neko.
@MainActor final class PresenceController {
    static let shared = PresenceController()
    static let defaultsKey = "neko.presence.enabled"
    private var panel: NSPanel?
    private var subscription: AnyCancellable?
    private var previous: JSONValue?
    private var hideTask: Task<Void, Never>?
    private var shown: PresenceState?
    private var demoExpanded = false

    static var enabled: Bool {
        get { UserDefaults.standard.object(forKey: defaultsKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: defaultsKey) }
    }

    func start(model: AppModel) {
        guard subscription == nil else { return }
        // Development only: NEKO_PRESENCE_DEMO=1 shows a sample chip to check its look.
        if let demo = ProcessInfo.processInfo.environment["NEKO_PRESENCE_DEMO"], ["1", "expanded"].contains(demo) {
            demoExpanded = demo == "expanded"
            show(PresenceState(activity: .thinking, label: "Neko is replying", lingers: false))
            return
        }
        subscription = model.$snapshot.receive(on: RunLoop.main).sink { [weak self] snapshot in
            self?.update(snapshot)
        }
    }

    func refresh() { if !Self.enabled { hide() } }

    private func update(_ snapshot: JSONValue) {
        defer { previous = snapshot }
        guard Self.enabled, previous != nil else { return }
        guard let state = PresenceState.from(snapshot: snapshot, previous: previous) else {
            // Keep a lingering outcome on screen until its timer ends.
            if shown?.lingers != true { hide() }
            return
        }
        show(state)
    }

    private func show(_ state: PresenceState) {
        hideTask?.cancel()
        if state != shown {
            let panel = self.panel ?? makePanel()
            // Size the window for the expanded label plus the glow on every
            // side, so neither is clipped. Transparent margins pass clicks through.
            let expanded = NSHostingView(rootView: PresenceCapsule(state: state, forceExpanded: true)).fittingSize
            panel.contentView = NSHostingView(rootView: PresenceCapsule(state: state, forceExpanded: demoExpanded))
            if let screen = NSScreen.main?.visibleFrame {
                let inset = PresenceCapsule.glowRoom
                panel.setFrame(NSRect(x: screen.maxX - expanded.width - 20 + inset, y: screen.minY + 20 - inset,
                                      width: expanded.width, height: expanded.height), display: true)
            }
            panel.orderFrontRegardless()
            shown = state
        }
        if state.lingers {
            hideTask = Task { [weak self] in
                try? await Task.sleep(for: .seconds(PresenceState.lingerSeconds))
                guard !Task.isCancelled else { return }
                self?.hide()
            }
        }
    }

    private func hide() {
        hideTask?.cancel()
        panel?.orderOut(nil)
        shown = nil
    }

    private func makePanel() -> NSPanel {
        let panel = NSPanel(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.level = .floating
        panel.acceptsMouseMovedEvents = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = false
        self.panel = panel
        return panel
    }
}

private struct PresenceCapsule: View {
    let state: PresenceState
    var forceExpanded = false
    /// Space around the chip for its glow, which blurs ~16pt past the edge.
    static let glowRoom: CGFloat = 28
    @State private var hovering = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private var expanded: Bool { forceExpanded || hovering }
    var body: some View {
        HStack(spacing: 0) {
            Spacer(minLength: 0)
            Button {
                NotificationCenter.default.post(name: .nekoOpenWorkspace, object: nil)
            } label: {
                HStack(spacing: 10) {
                    PixelGlyph(activity: state.activity, size: 16)
                    if expanded {
                        ShimmerText(text: state.label, active: state.activity.animates)
                            .lineLimit(1)
                            .fixedSize()
                            .transition(.opacity.combined(with: .move(edge: .trailing)))
                    }
                }
                .padding(.leading, expanded ? 12 : 10).padding(.trailing, expanded ? 16 : 10).padding(.vertical, 10)
                .background {
                    Capsule()
                        .fill(LinearGradient(colors: [state.activity.color, state.activity.companion], startPoint: .leading, endPoint: .trailing))
                        .blur(radius: 14)
                        .opacity(state.activity.animates ? 0.5 : 0.3)
                        .offset(y: 5)
                        .padding(.horizontal, expanded ? 10 : 2)
                }
                .background(Color.black.opacity(0.88), in: Capsule())
                .overlay { Capsule().strokeBorder(.white.opacity(0.09)) }
                .contentShape(Capsule())
            }
            .buttonStyle(.plain)
            .background(AlwaysHover { inside in
                withAnimation(reduceMotion ? nil : .snappy(duration: 0.28, extraBounce: 0.05)) { hovering = inside }
            })
            .help(state.label)
            .accessibilityLabel(state.label)
        }
        .padding(Self.glowRoom)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
        .environment(\.colorScheme, .dark)
    }
}

/// Hover that works while Neko is in the background. SwiftUI's onHover only
/// tracks in the active app, and this chip mostly shows when Neko is not.
private struct AlwaysHover: NSViewRepresentable {
    let changed: (Bool) -> Void
    func makeNSView(context: Context) -> TrackingView { TrackingView(changed: changed) }
    func updateNSView(_ view: TrackingView, context: Context) { view.changed = changed }
    final class TrackingView: NSView {
        var changed: (Bool) -> Void
        init(changed: @escaping (Bool) -> Void) { self.changed = changed; super.init(frame: .zero) }
        required init?(coder: NSCoder) { nil }
        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            trackingAreas.forEach(removeTrackingArea)
            addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self))
        }
        override func mouseEntered(with event: NSEvent) { changed(true) }
        override func mouseExited(with event: NSEvent) { changed(false) }
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
    }
}

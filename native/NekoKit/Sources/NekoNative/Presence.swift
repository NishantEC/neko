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

    static var enabled: Bool {
        get { UserDefaults.standard.object(forKey: defaultsKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: defaultsKey) }
    }

    func start(model: AppModel) {
        guard subscription == nil else { return }
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
            let host = NSHostingView(rootView: PresenceCapsule(state: state))
            panel.contentView = host
            let size = host.fittingSize
            if let screen = NSScreen.main?.visibleFrame {
                panel.setFrame(NSRect(x: screen.maxX - size.width - 20, y: screen.minY + 20, width: size.width, height: size.height), display: true)
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
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = false
        self.panel = panel
        return panel
    }
}

private struct PresenceCapsule: View {
    let state: PresenceState
    var body: some View {
        Button {
            NotificationCenter.default.post(name: .nekoOpenWorkspace, object: nil)
        } label: {
            ActivityCapsule(activity: state.activity, label: state.label)
        }
        .buttonStyle(.plain)
        .padding(8)
        .help("Open Neko")
        .environment(\.colorScheme, .dark)
    }
}

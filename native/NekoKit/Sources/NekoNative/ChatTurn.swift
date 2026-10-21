import SwiftUI
import NekoKit

/// One chat message, redrawn only when what it shows changes. The chat sits
/// inside a view that observes the whole app state, which changes every few
/// seconds while agents work; rebuilding every visible message on each change
/// re-parsed its markdown and re-measured its height, and a one-pixel
/// difference made the transcript scroll again. Nothing inside observes the
/// app model, or this equality would be bypassed.
struct ChatTurn<Content: View>: View, Equatable {
    /// Everything this message shows, and nothing else.
    let key: JSONValue
    @ViewBuilder let content: () -> Content
    nonisolated static func == (a: Self, b: Self) -> Bool { a.key == b.key }
    var body: some View { content() }
}

/// What a message shows besides its own text, gathered once per render
/// rather than searched for by every message.
struct ChatTurnRelated {
    private let snapshot: JSONValue
    private let receipts: [String: [JSONValue]]
    private let tasks: [String: JSONValue]
    private let watches: [String: JSONValue]
    private let workspaces: [String: String]
    private let connectionLabels: [String: String]
    private let connections: JSONValue

    init(snapshot: JSONValue) {
        self.snapshot = snapshot
        receipts = Dictionary(grouping: snapshot["mcp"]["receipts"].array, by: { $0["run_id"].string })
        tasks = Dictionary(snapshot["tasks"].array.map { ($0.recordID, $0) }, uniquingKeysWith: { first, _ in first })
        watches = Dictionary(snapshot["mcp"]["responsibilities"].array.map { ($0.recordID, $0) }, uniquingKeysWith: { first, _ in first })
        workspaces = Dictionary(snapshot["workspaces"].array.map { ($0.recordID, $0["name"].string) }, uniquingKeysWith: { first, _ in first })
        connectionLabels = Dictionary(snapshot["mcp"]["connections"].array.map { ($0.recordID, $0["label"].string) }, uniquingKeysWith: { first, _ in first })
        connections = .array(snapshot["mcp"]["connections"].array.map { .array([.string($0.recordID), $0["label"]]) })
    }

    func receipts(for message: JSONValue) -> [JSONValue] { receipts["chat:\(message.recordID)"] ?? [] }
    func task(_ id: String) -> JSONValue { tasks[id] ?? .null }
    func watch(_ id: String) -> JSONValue? { watches[id] }
    func workspaceName(_ id: String) -> String { workspaces[id] ?? "" }
    func connectionName(_ id: String) -> String { connectionLabels[id] ?? "" }
    func actionTitles(_ action: JSONValue) -> (task: String?, merge: String?) { ChatActionCard.titles(for: action, snapshot: snapshot) }

    /// `extra` carries presentation that depends on neighbours (whether the
    /// speaker line shows), so a change there redraws the message too.
    func key(for message: JSONValue, extra: JSONValue = .null) -> JSONValue {
        let linked = message["ticket_ids"].array.map { id -> JSONValue in
            let task = tasks[id.string] ?? .null
            return .array([task["title"], task["status"], .string(workspaces[task["workspace_id"].string] ?? "")])
        }
        let watched = message["responsibility_ids"].array.map { watches[$0.string] ?? .null }
        let actions = message["actions"].array.map { action -> JSONValue in
            let titles = actionTitles(action)
            return .array([titles.task.map(JSONValue.string) ?? .null, titles.merge.map(JSONValue.string) ?? .null])
        }
        let usesConnections = !watched.isEmpty || !message["tool_calls"].array.isEmpty || receipts["chat:\(message.recordID)"] != nil
        return .object([
            "message": message,
            "receipts": .array(receipts["chat:\(message.recordID)"] ?? []),
            "tasks": .array(linked),
            "watches": .array(watched),
            "actions": .array(actions),
            // Tool rows and watch rows name their connections, so the labels are part of what they show.
            "connections": usesConnections ? connections : .null,
            "extra": extra,
        ])
    }
}

/// The turn rhythm: a new exchange opens with a wide gap, the reply follows
/// close, so a question and its answer read as one group.
enum ChatRhythm {
    static func gap(isUser: Bool, isFirst: Bool) -> CGFloat {
        isFirst ? 0 : isUser ? NekoChat.turnGap : NekoChat.replyGap
    }
    /// The speaker line shows only when the speaker changes.
    static func showsSpeaker(_ speaker: String, previous: String?) -> Bool { speaker != previous }
}

/// Turns already faded in, so a lazy stack bringing a row back on screen
/// never fades it again. A class, so recording one invalidates nothing.
@MainActor final class ChatFadeLedger {
    private var shown: Set<String> = []
    func shouldFade(_ id: String, fresh: Bool) -> Bool { fresh && !shown.contains(id) }
    func record(_ id: String) { shown.insert(id) }
}

/// A single short fade for a turn that arrived while the chat was open.
/// Older turns, and turns scrolled back into view, appear at once.
struct ChatTurnFade: ViewModifier {
    let id: String
    let fresh: Bool
    let ledger: ChatFadeLedger
    @State private var shown = false
    func body(content: Content) -> some View {
        let fades = ledger.shouldFade(id, fresh: fresh)
        content
            .opacity(fades && !shown ? 0 : 1)
            .onAppear {
                guard fades else { return }
                ledger.record(id)
                withNekoAnimation(.reveal) { shown = true }
            }
    }
}

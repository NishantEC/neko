import SwiftUI
import NekoKit

struct ChatToolActivitySummary {
    let calls: [JSONValue]
    let receipts: [JSONValue]
    var showsReceiptsSeparately: Bool { calls.isEmpty }
    var count: Int { showsReceiptsSeparately ? receipts.count : calls.count }
    var succeeded: Int { showsReceiptsSeparately ? receipts.filter { $0["success"].bool }.count : calls.filter { $0["status"].string == "succeeded" }.count }
    var failed: Int { showsReceiptsSeparately ? receipts.filter { !$0["success"].bool }.count : calls.filter { ["failed", "denied"].contains($0["status"].string) }.count }
    var awaitingApproval: Int { calls.filter { $0["status"].string == "awaiting_approval" }.count }
    var title: String { title(connections: []) }
    /// Each call as plain values, for rows that must not observe the app model.
    func values(connectionName: (String) -> String) -> [ChatToolCallValue] {
        if showsReceiptsSeparately {
            return receipts.map {
                ChatToolCallValue(id: $0.recordID, tool: $0["tool_name"].string, connection: connectionName($0["connection_id"].string),
                                  status: $0["success"].bool ? "succeeded" : "failed", arguments: "")
            }
        }
        return calls.map {
            ChatToolCallValue(id: $0.recordID, tool: $0["tool_name"].string, connection: connectionName($0["connection_id"].string),
                              status: $0["status"].string, arguments: $0["arguments_json"].string)
        }
    }
    /// "Used 3 tools · Linear, Sentry · 1 failed": what the work touched, in
    /// one line. At most two connection names, then "+N".
    func title(connections: [String]) -> String {
        var parts = ["Used \(count) tool\(count == 1 ? "" : "s")"]
        var names: [String] = []
        for name in connections where !name.isEmpty && !names.contains(name) { names.append(name) }
        if !names.isEmpty {
            parts.append(names.prefix(2).joined(separator: ", ") + (names.count > 2 ? " +\(names.count - 2)" : ""))
        }
        if awaitingApproval > 0 { parts.append("\(awaitingApproval) waiting for you") }
        else if failed > 0 { parts.append("\(failed) failed") }
        return parts.joined(separator: " · ")
    }
}

/// One tool call, as plain values: rows never observe the app model, so a
/// workbench tick does not redraw them.
struct ChatToolCallValue: Identifiable, Equatable {
    let id: String
    let tool: String
    let connection: String
    let status: String
    let arguments: String
}

/// The tool calls one Neko reply made, folded into one quiet line. A request
/// waiting on approval is answered in the dock; here it only points there.
struct ChatToolActivity: View {
    let activity: ChatToolActivitySummary
    /// The connection label for an id, or "" when it is gone.
    let connectionName: (String) -> String
    @State private var open = false

    private var calls: [ChatToolCallValue] { activity.values(connectionName: connectionName) }

    var body: some View {
        let all = calls
        let finished = all.filter { $0.status != "awaiting_approval" }
        VStack(alignment: .leading, spacing: NekoSpace.xxs) {
            if !finished.isEmpty {
                ChatWorkFold(label: activity.title(connections: all.map(\.connection)), failed: activity.failed > 0, open: $open) {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(finished) { ChatToolCallRow(call: $0) }
                    }
                    .padding(.leading, NekoSpace.lg)
                }
            }
            if activity.awaitingApproval > 0 {
                Label("Waiting for your approval", systemImage: "arrow.down")
                    .labelStyle(.titleAndIcon)
                    .font(NekoFont.meta).foregroundStyle(NekoStyle.amber)
                    .frame(minHeight: NekoChat.workRow)
                    .accessibilityLabel("Waiting for your approval, answered below the conversation")
            }
        }
    }
}

/// One finished tool call: status, tool, connection and outcome on one line;
/// its request opens inline from a hover button.
struct ChatToolCallRow: View {
    let call: ChatToolCallValue
    @State private var hover = false
    @State private var showArguments = false
    var body: some View {
        let failed = call.status == "failed" || call.status == "denied"
        VStack(alignment: .leading, spacing: NekoSpace.xxs) {
            HStack(spacing: NekoSpace.sm) {
                Image(systemName: failed ? "exclamationmark.circle" : "checkmark")
                    .foregroundStyle(failed ? NekoStyle.amber : N.text4)
                    .frame(width: 14).accessibilityHidden(true)
                Text(call.tool).font(NekoFont.mono).foregroundStyle(N.text2).lineLimit(1)
                if !call.connection.isEmpty { Text("· \(call.connection)").foregroundStyle(N.text4).lineLimit(1) }
                Spacer(minLength: NekoSpace.sm)
                if !call.arguments.isEmpty {
                    Button { withNekoAnimation(.reveal) { showArguments.toggle() } } label: {
                        Image(systemName: "curlybraces").frame(width: NekoSpace.xl, height: NekoSpace.xl).contentShape(Rectangle())
                    }
                    .nekoButton(.quiet).foregroundStyle(N.text3)
                    .opacity(hover || showArguments ? 1 : 0)
                    .help(showArguments ? "Hide the request" : "Show the request")
                    .accessibilityLabel(showArguments ? "Hide the request" : "Show the request")
                }
                Text(call.status.replacingOccurrences(of: "_", with: " ").capitalized).foregroundStyle(failed ? NekoStyle.amber : N.text4)
            }
            .font(NekoFont.caption)
            .frame(minHeight: NekoChat.workRow)
            if showArguments {
                Text(call.arguments).font(NekoFont.mono).foregroundStyle(N.text2).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(NekoSpace.sm)
                    .nekoFill(N.panel, radius: NekoRadius.md)
            }
        }
        .contentShape(Rectangle())
        .onHover { inside in withNekoAnimation(.hover) { hover = inside } }
    }
}

enum ChatToolArguments {
    /// The first three arguments of a tool request, in one readable line.
    static func summary(_ json: String) -> String {
        guard let value = try? JSONDecoder().decode(JSONValue.self, from: Data(json.utf8)) else { return json }
        return value.object.sorted { $0.key < $1.key }.prefix(3).map { key, value in
            let text: String
            switch value { case .string(let s): text = s; case .number(let n): text = n.formatted(); case .bool(let b): text = b ? "yes" : "no"; default: text = "…" }
            return "\(key): \(text.prefix(40))"
        }.joined(separator: " · ")
    }
}

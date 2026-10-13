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
    var title: String {
        let noun = showsReceiptsSeparately ? "tool receipt" : "tool call"
        let warning = awaitingApproval > 0 ? " · \(awaitingApproval) needs approval" : failed > 0 ? " · \(failed) failed" : ""
        return "\(count) \(noun)\(count == 1 ? "" : "s")\(warning)"
    }
}

/// The tool calls one Neko reply made: finished calls and receipts folded away,
/// and any request still waiting on approval shown open with its answers.
struct ChatToolActivity: View {
    @ObservedObject var model: AppModel
    let activity: ChatToolActivitySummary
    let turn: String
    let pending: Bool
    /// Requests waiting on approval across the whole conversation.
    let awaitingApprovals: Int
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.sm) {
            if activity.showsReceiptsSeparately || activity.calls.contains(where: { $0["status"].string != "awaiting_approval" }) {
                DisclosureGroup {
                VStack(alignment: .leading, spacing: 0) {
                    if activity.showsReceiptsSeparately {
                        ForEach(activity.receipts, id: \.recordID) { receipt in
                            HStack(spacing: 7) {
                                Image(systemName: receipt["success"].bool ? "checkmark" : "exclamationmark.triangle")
                                Text(receipt["tool_name"].string)
                                Spacer()
                                Text(receipt["success"].bool ? "Done" : "Failed")
                            }
                            .font(NekoFont.caption)
                            .foregroundStyle(receipt["success"].bool ? N.text3 : NekoStyle.amber)
                            .padding(.vertical, 7)
                            Divider().opacity(0.4)
                        }
                    } else {
                        ForEach(activity.calls.filter { $0["status"].string != "awaiting_approval" }, id: \.recordID) { call in
                            ChatToolCallRow(model: model, call: call, turn: turn, pending: pending, awaitingApprovals: awaitingApprovals)
                        }
                    }
                }.padding(.top, NekoSpace.sm)
            } label: {
                Label(activity.title, systemImage: activity.failed > 0 ? "exclamationmark.circle" : pending ? "circle.dotted" : "checkmark.circle")
                    .font(NekoFont.meta)
                    .foregroundStyle(activity.failed > 0 || activity.awaitingApproval > 0 ? NekoStyle.amber : N.text3)
                }
            } else {
                Label(activity.title, systemImage: "hand.raised")
                    .font(NekoFont.of(.caption, weight: .medium)).foregroundStyle(NekoStyle.amber)
            }
            ForEach(activity.calls.filter { $0["status"].string == "awaiting_approval" }, id: \.recordID) { call in
                ChatToolCallRow(model: model, call: call, turn: turn, pending: pending, awaitingApprovals: awaitingApprovals)
            }
        }
        .padding(.top, NekoSpace.xxs)
    }
}

/// One tool call: its status, its request, and Allow/Deny while it waits.
struct ChatToolCallRow: View {
    @ObservedObject var model: AppModel
    let call: JSONValue
    let turn: String
    let pending: Bool
    /// Requests waiting on approval across the whole conversation.
    let awaitingApprovals: Int
    var body: some View {
        let status = call["status"].string
        let connection = model.snapshot["mcp"]["connections"].array.first { $0.recordID == call["connection_id"].string }?["label"].string ?? "Removed connection"
        VStack(alignment: .leading, spacing: NekoSpace.xs) {
            HStack(spacing: NekoSpace.sm) {
                Image(systemName: status == "failed" || status == "denied" ? "exclamationmark.circle" : status == "awaiting_approval" ? "hand.raised" : "checkmark")
                    .foregroundStyle(status == "failed" || status == "denied" || status == "awaiting_approval" ? NekoStyle.amber : N.text4)
                    .frame(width: 14)
                Text(call["tool_name"].string).fontWeight(.medium)
                Text("· \(connection)").foregroundStyle(N.text4).lineLimit(1)
                Spacer(minLength: 8)
                Text(status.replacingOccurrences(of: "_", with: " ").capitalized).foregroundStyle(N.text4)
            }.font(NekoFont.caption)
            DisclosureGroup("Request details") {
                Text(call["arguments_json"].string).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
            }.font(NekoFont.of(.small)).foregroundStyle(N.text4).padding(.leading, 22)
            if pending && call["status"].string == "awaiting_approval" {
                // Keyboard answers only when exactly one request is waiting, so ⌘1/⌘2 is never ambiguous.
                let only = awaitingApprovals == 1
                HStack {
                    Button("Deny", role: .destructive) { decide(false) }.keyboardShortcut(only ? KeyboardShortcut("2", modifiers: .command) : nil)
                    Button("Allow this request") { decide(true) }.nekoButton(.primary).keyboardShortcut(only ? KeyboardShortcut("1", modifiers: .command) : nil)
                    if only { Text("⌘1 allow · ⌘2 deny").font(NekoFont.of(.small)).foregroundStyle(N.text4) }
                }
                .padding(.leading, 22).disabled(model.busy)
            }
            Divider().opacity(0.4)
        }.padding(.vertical, 5)
    }
    private func decide(_ approve: Bool) {
        Task { await model.workbench(.command("DecideChatTool", ["turn_id": .string(turn), "call_id": call["id"], "approve": .bool(approve)])) }
    }
}

/// The one decision a conversation is waiting on, pinned above the composer:
/// a tool request to allow, a plan to approve, or a result to accept.
struct ChatApprovalCard: View {
    let symbol: String
    let title: String
    let detail: String
    let secondary: (String, KeyboardShortcut?, () -> Void)
    let primary: (String, KeyboardShortcut?, () -> Void)
    let busy: Bool
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.md) {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: symbol).font(NekoFont.body).foregroundStyle(ReplyStyle.orange).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                    Text(title).font(NekoFont.heading).fixedSize(horizontal: false, vertical: true)
                    if !detail.isEmpty { Text(detail).font(NekoFont.meta).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true) }
                }
            }
            HStack(spacing: NekoSpace.sm) {
                Spacer(minLength: 0)
                Button(secondary.0, action: secondary.2).keyboardShortcut(secondary.1)
                Button(primary.0, action: primary.2).nekoButton(.primary).keyboardShortcut(primary.1)
            }
        }
        .controlSize(.small)
        .disabled(busy)
        .padding(NekoLayout.rowInset)
        .background(N.panel, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
    }
    /// The first three arguments of a tool request, in one readable line.
    static func argumentSummary(_ json: String) -> String {
        guard let value = try? JSONDecoder().decode(JSONValue.self, from: Data(json.utf8)) else { return json }
        return value.object.sorted { $0.key < $1.key }.prefix(3).map { key, value in
            let text: String
            switch value { case .string(let s): text = s; case .number(let n): text = n.formatted(); case .bool(let b): text = b ? "yes" : "no"; default: text = "…" }
            return "\(key): \(text.prefix(40))"
        }.joined(separator: " · ")
    }
}

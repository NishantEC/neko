import SwiftUI
import NekoKit

/// Something Neko, as chief of staff, proposes doing to a task. Nothing
/// happens until you press Do it; the daemon then runs the same command the
/// task's own buttons send.
struct ChatActionCard: View {
    @ObservedObject var model: AppModel
    let messageID: String
    let action: JSONValue
    @State private var deciding = false

    private var task: JSONValue { model.snapshot["tasks"].array.first { $0.recordID == action["task_id"].string } ?? .null }
    private var taskTitle: String { task == .null ? "A task that no longer exists" : TaskText(task: task, snapshot: model.snapshot).title }

    /// What the action does, in the person's words.
    static func verb(_ kind: String) -> String {
        switch kind {
        case "message": "Tell its agent"
        case "start": "Start"
        case "stop": "Stop"
        case "run_next": "Run next"
        case "approve": "Approve the plan for"
        case "accept": "Accept the result of"
        case "retry": "Try again"
        case "rename": "Rename"
        default: kind
        }
    }

    var body: some View {
        let status = action["status"].string
        VStack(alignment: .leading, spacing: NekoSpace.sm) {
            HStack(alignment: .firstTextBaseline, spacing: NekoSpace.sm) {
                Image(systemName: symbol(status)).foregroundStyle(tint(status)).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                    Text("\(Self.verb(action["kind"].string)) · \(taskTitle)").font(NekoFont.heading).lineLimit(2)
                    if !action["text"].string.isEmpty {
                        Text(action["kind"].string == "rename" ? "New name: \(action["text"].string)" : "“\(action["text"].string)”")
                            .font(NekoFont.body).foregroundStyle(N.text2).textSelection(.enabled)
                    }
                    if status == "Failed" && !action["note"].string.isEmpty {
                        Text(action["note"].string).font(NekoFont.meta).foregroundStyle(NekoStyle.amber)
                    }
                }
                Spacer(minLength: NekoSpace.md)
                switch status {
                case "Proposed":
                    Button("Not now") { decide(false) }.nekoButton(.quiet).disabled(deciding)
                    Button("Do it") { decide(true) }.nekoButton(.primary).disabled(deciding || task == .null)
                case "Done": Text("Done").font(NekoFont.meta).foregroundStyle(NekoStyle.mint)
                case "Declined": Text("Skipped").font(NekoFont.meta).foregroundStyle(N.text3)
                default: Text("Didn't work").font(NekoFont.meta).foregroundStyle(NekoStyle.amber)
                }
            }
            if task != .null {
                Button("Open task") { model.openAgent(task.recordID, from: .home) }.nekoButton(.quiet).font(NekoFont.meta)
            }
        }
        .padding(NekoLayout.rowInset)
        .frame(maxWidth: .infinity, alignment: .leading)
        .nekoCard(padding: 0, radius: NekoRadius.xl)
        .accessibilityElement(children: .contain)
    }

    private func symbol(_ status: String) -> String {
        switch status {
        case "Done": "checkmark.circle.fill"
        case "Declined": "minus.circle"
        case "Failed": "exclamationmark.triangle.fill"
        default: "arrow.right.circle"
        }
    }
    private func tint(_ status: String) -> Color {
        switch status {
        case "Done": NekoStyle.mint
        case "Failed": NekoStyle.amber
        case "Declined": N.text4
        default: NekoStyle.accent
        }
    }
    private func decide(_ accept: Bool) {
        deciding = true
        Task {
            await model.workbench(.command("DecideChatAction", ["message_id": .string(messageID), "action_id": action["id"], "accept": .bool(accept)]))
            deciding = false
        }
    }
}

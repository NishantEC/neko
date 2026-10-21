import SwiftUI
import NekoKit

/// Something Neko, as chief of staff, proposes doing to a task. Nothing
/// happens until you press Do it; the daemon then runs the same command the
/// task's own buttons send. Built from values, never the app model, so a
/// workbench tick does not redraw it.
struct ChatActionCard: View {
    let action: JSONValue
    /// The task's title, or nil when the task no longer exists.
    let taskTitle: String?
    /// For a merge: the title of the task it merges into, or nil when gone.
    var mergeTarget: String? = nil
    /// Answers the proposal; returns when the daemon has.
    let decide: (Bool) async -> Void
    let open: () -> Void
    @State private var deciding = false

    /// The titles an action card shows, looked up once by the caller.
    static func titles(for action: JSONValue, snapshot: JSONValue) -> (task: String?, merge: String?) {
        let tasks = snapshot["tasks"].array
        let task = tasks.first { $0.recordID == action["task_id"].string }
        let target = action["kind"].string == "merge" ? tasks.first { $0.recordID == action["text"].string } : nil
        return (task.map { TaskText(task: $0, snapshot: snapshot).title }, target.map { TaskText(task: $0, snapshot: snapshot).title })
    }

    /// What the action does, in the person's words.
    static func verb(_ kind: String) -> String {
        switch kind {
        case "message": "Tell its agent"
        case "start": "Start"
        case "stop": "Stop"
        case "close": "Close"
        case "merge": "Merge as a duplicate"
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
                    Text("\(Self.verb(action["kind"].string)) · \(taskTitle ?? "A task that no longer exists")").font(NekoFont.of(.callout, weight: .semibold)).lineLimit(2)
                    if action["kind"].string == "merge" {
                        Text("Into: \(mergeTarget ?? "a task that no longer exists")").font(NekoFont.meta).foregroundStyle(N.text2)
                    } else if !action["text"].string.isEmpty {
                        Text(action["kind"].string == "rename" ? "New name: \(action["text"].string)" : "“\(action["text"].string)”")
                            .font(NekoFont.meta).lineSpacing(NekoLeading.tight).foregroundStyle(N.text2).textSelection(.enabled)
                    }
                    if status == "Failed" && !action["note"].string.isEmpty {
                        Text(action["note"].string).font(NekoFont.meta).foregroundStyle(NekoStyle.amber)
                    }
                }
                Spacer(minLength: NekoSpace.md)
                switch status {
                case "Proposed":
                    Button("Not now") { answer(false) }.nekoButton(.quiet).disabled(deciding)
                    Button("Do it") { answer(true) }.nekoButton(.primary).disabled(deciding || taskTitle == nil)
                case "Done": Text("Done").font(NekoFont.meta).foregroundStyle(NekoStyle.mint)
                case "Declined": Text("Skipped").font(NekoFont.meta).foregroundStyle(N.text3)
                default: Text("Didn't work").font(NekoFont.meta).foregroundStyle(NekoStyle.amber)
                }
            }
            if taskTitle != nil {
                Button("Open task", action: open).nekoButton(.quiet).font(NekoFont.meta)
            }
        }
        .padding(NekoChat.blockInset)
        .frame(maxWidth: .infinity, alignment: .leading)
        // A hairline outline and no fill or shadow: the card is part of the
        // conversation, not a surface lifted above it.
        .overlay { RoundedRectangle(cornerRadius: NekoRadius.md, style: .continuous).strokeBorder(N.line) }
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
    private func answer(_ accept: Bool) {
        deciding = true
        Task {
            await decide(accept)
            deciding = false
        }
    }
}

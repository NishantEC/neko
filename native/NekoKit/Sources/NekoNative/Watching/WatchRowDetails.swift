import SwiftUI
import NekoKit

struct WatchRowDetails: View {
    @ObservedObject var model: AppModel
    let item: JSONValue
    let onEdit: () -> Void

    private var sources: String {
        item["connection_ids"].array.map { Watching.label(model, connection: $0.string) }.joined(separator: ", ")
    }
    private var workspace: String {
        model.workspaces.first { $0.recordID == item["workspace_id"].string }?["name"].string ?? "Unavailable"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            ManagementSavedText(text: item["instruction"].string)
            VStack(alignment: .leading, spacing: 12) {
                LabeledContent("Status", value: item["enabled"].bool ? "Watching" : "Paused")
                LabeledContent("Last checked", value: item["last_attempt_ms"] == .null ? "Not checked yet" : relativeTime(item["last_attempt_ms"].int))
                if item["failures"].int > 0 {
                    Label("\(item["failures"].int) consecutive failed \(item["failures"].int == 1 ? "check" : "checks")", systemImage: "exclamationmark.triangle")
                        .foregroundStyle(NekoStyle.amber)
                }
                LabeledContent("Sources", value: sources.isEmpty ? "No sources selected" : sources)
                if model.selectedWorkspace == nil { LabeledContent("Workspace", value: workspace) }
                Text(item["prepare_low_risk"].bool ? "Low-risk local fixes allowed · no publication" : "Plans require approval before local work")
                    .foregroundStyle(N.text3)
            }.font(NekoFont.meta)
            if !item["last_result"].string.isEmpty {
                Divider()
                Text("Last check result").font(NekoFont.heading)
                ManagementSavedText(text: item["last_result"].string)
            }
            HStack {
                Button("Check now") { submit(model, nested("Mcp", "Wake", ["responsibility_id": item["id"]])) }
                    .disabled(!item["enabled"].bool || model.busy)
                Button("Edit watch", action: onEdit).disabled(model.busy)
            }.controlSize(.regular)
        }
        .font(NekoFont.body).foregroundStyle(N.text2).padding(.leading, 24)
    }
}

import SwiftUI
import NekoKit

struct WatchRow: View {
    @ObservedObject var model: AppModel
    let item: JSONValue
    let onEdit: () -> Void
    @State private var expanded = false
    @State private var confirmingActivation = false

    private var status: String {
        if !item["enabled"].bool { return item["failures"].int > 0 ? "Paused · last check failed" : "Paused" }
        if item["failures"].int > 0 { return "Needs attention" }
        return item["last_attempt_ms"] == .null ? "First check queued" : "Checked \(relativeTime(item["last_attempt_ms"].int))"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 12) {
                WatchRowHeader(instruction: item["instruction"].string, status: status,
                               needsAttention: item["enabled"].bool && item["failures"].int > 0,
                               expanded: $expanded)
                Toggle("Watch \(item["instruction"].string)", isOn: Binding(
                    get: { item["enabled"].bool },
                    set: { enabled in setEnabled(enabled) }
                ))
                .labelsHidden().toggleStyle(.switch).controlSize(.mini).disabled(model.busy)
                actions
            }
            if expanded { WatchRowDetails(model: model, item: item, onEdit: onEdit) }
        }
        .padding(.vertical, NekoLayout.rowInset)
        .overlay(alignment: .top) { N.line.frame(height: 1) }
        .modifier(WatchActivationConfirmation(model: model, item: item, isPresented: $confirmingActivation))
    }

    private var actions: some View {
        Menu("Watch actions", systemImage: "ellipsis") {
            Button("Check now") { submit(model, nested("Mcp", "Wake", ["responsibility_id": item["id"]])) }
                .disabled(!item["enabled"].bool)
            Button("Edit", action: onEdit)
            Button("Delete", role: .destructive) { Watching.remove(model, item) }
        }
        .labelStyle(.iconOnly).menuStyle(.borderlessButton).fixedSize().disabled(model.busy)
    }

    private func setEnabled(_ enabled: Bool) {
        if enabled {
            if Watching.ungranted(model, item).isEmpty { confirmingActivation = true }
            else { Watching.reviewAccess(model, item) }
        } else {
            submit(model, nested("Mcp", "SaveResponsibility", ["responsibility": replacing(item, ["enabled": .bool(false)])]))
        }
    }
}

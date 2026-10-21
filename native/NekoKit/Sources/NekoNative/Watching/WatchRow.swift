import SwiftUI
import NekoKit

struct WatchRow: View {
    @ObservedObject var model: AppModel
    let item: JSONValue
    let onEdit: () -> Void
    @State private var expanded = false
    @State private var confirmingActivation = false
    @State private var renaming = false
    @State private var confirmingDelete = false

    private var status: String {
        if !item["enabled"].bool { return item["failures"].int > 0 ? "Paused · last check failed" : "Paused" }
        if item["failures"].int > 0 { return "Needs attention" }
        return item["last_attempt_ms"] == .null ? "First check queued" : "Checked \(relativeTime(item["last_attempt_ms"].int))"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.md) {
            HStack(alignment: .top, spacing: NekoSpace.md) {
                let title = Watching.title(item)
                WatchRowHeader(title: title, instruction: title == item["instruction"].string ? nil : item["instruction"].string, status: status,
                               needsAttention: item["enabled"].bool && item["failures"].int > 0,
                               expanded: $expanded)
                Toggle("Watch \(Watching.title(item))", isOn: Binding(
                    get: { item["enabled"].bool },
                    set: { enabled in setEnabled(enabled) }
                ))
                .labelsHidden().toggleStyle(.switch).controlSize(.regular).disabled(model.busy)
                actions
            }
            if expanded { WatchRowDetails(model: model, item: item, onEdit: onEdit) }
        }
        .padding(.vertical, NekoSpace.md)
        .overlay(alignment: .top) { Hairline() }
        .accessibilityElement(children: .contain)
        .nekoConfirmDestructive(isPresented: $confirmingDelete, title: "Delete “\(Watching.title(item))”?",
                                message: "Neko stops checking it. Tasks it already filed stay.", action: "Delete watch") {
            Watching.remove(model, item)
        }
        .modifier(WatchActivationConfirmation(model: model, item: item, isPresented: $confirmingActivation))
        .renameAlert("Rename watch", isPresented: $renaming, current: item["name"].string) { name in
            submit(model, nested("Mcp", "RenameResponsibility", ["responsibility_id": item["id"], "name": .string(name)]))
        }
    }

    private var actions: some View {
        Menu("Watch actions", systemImage: "ellipsis") {
            Button("Check now") { submit(model, nested("Mcp", "Wake", ["responsibility_id": item["id"]])) }
                .disabled(!item["enabled"].bool)
            Button("Rename…") { renaming = true }
            Button("Edit", action: onEdit)
            Button("Delete watch…", role: .destructive) { confirmingDelete = true }
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

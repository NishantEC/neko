import SwiftUI
import NekoKit

struct SuggestedResponsibilityCard: View {
    @ObservedObject var model: AppModel
    let item: JSONValue
    var onEdit: (() -> Void)? = nil
    @State private var expanded = false
    @State private var confirmingActivation = false

    var body: some View {
        let tools = item["connection_ids"].array.map { Watching.label(model, connection: $0.string) }
        let missing = Watching.ungranted(model, item)
        let live = item["enabled"].bool
        VStack(alignment: .leading, spacing: 18) {
            DisclosureGroup(isExpanded: $expanded) {
                VStack(alignment: .leading, spacing: 14) {
                    ManagementSavedText(text: item["instruction"].string)
                    Text("Sources: \(tools.isEmpty ? "None selected" : tools.joined(separator: ", "))")
                    Text(item["prepare_low_risk"].bool ? "May prepare low-risk local fixes; no publication." : "Local work waits for approval.")
                    if !missing.isEmpty {
                        Label("Connect or discover tools for \(missing.joined(separator: ", ")) to start.", systemImage: "exclamationmark.triangle").foregroundStyle(NekoStyle.amber)
                    }
                }.font(NekoFont.meta).foregroundStyle(N.text3).padding(.top, 14)
            } label: {
                VStack(alignment: .leading, spacing: 8) {
                    Text(verbatim: item["instruction"].string).font(NekoFont.heading).foregroundStyle(N.text).lineLimit(3)
                    Text(missing.isEmpty ? "Ready to review" : "Connection access needed").font(NekoFont.meta).foregroundStyle(missing.isEmpty ? N.text3 : NekoStyle.amber)
                }
            }
            HStack(spacing: 8) {
                if live {
                    Label("Watching", systemImage: "checkmark").font(NekoFont.meta).foregroundStyle(N.text2)
                } else {
                    Button(missing.isEmpty ? "Turn on…" : "Review connections") {
                        if missing.isEmpty { confirmingActivation = true }
                        else { Watching.reviewAccess(model, item) }
                    }
                }
                if let onEdit { Button("Edit", action: onEdit) }
                Spacer()
                if !live { Button("Dismiss") { Watching.remove(model, item) } }
            }.controlSize(.regular).disabled(model.busy)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, NekoLayout.rowInset)
        .overlay(alignment: .top) { N.line.frame(height: 1) }
        .accessibilityElement(children: .contain)
        .modifier(WatchActivationConfirmation(model: model, item: item, isPresented: $confirmingActivation))
    }
}

import SwiftUI
import NekoKit

/// Explain the existing grant at activation; the command and authority are unchanged.
struct WatchActivationConfirmation: ViewModifier {
    @ObservedObject var model: AppModel
    let item: JSONValue
    @Binding var isPresented: Bool

    func body(content: Content) -> some View {
        content.confirmationDialog("Turn on this watch?", isPresented: $isPresented, titleVisibility: .visible) {
            Button("Turn on watch") { Watching.turnOn(model, item) }
            Button("Cancel", role: .cancel) {}
        } message: {
            let sources = item["connection_ids"].array.map { Watching.label(model, connection: $0.string) }.joined(separator: ", ")
            let workspace = model.workspaces.first { $0.recordID == item["workspace_id"].string }?["name"].string ?? "this workspace"
            Text("\(item["instruction"].string)\n\nAllows unattended checks every 10 minutes using \(sources.isEmpty ? "the selected connections" : sources) in \(workspace). Connected tools may read or change data. \(item["prepare_low_risk"].bool ? "May prepare low-risk local fixes. Nothing is published." : "Local work waits for your approval.")")
        }
    }
}

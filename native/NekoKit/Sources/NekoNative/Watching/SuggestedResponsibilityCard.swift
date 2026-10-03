import SwiftUI
import NekoKit

/// One suggestion Neko made: what it will look at, what it will and won't do.
struct SuggestedResponsibilityCard: View {
    @ObservedObject var model: AppModel
    let item: JSONValue
    var onEdit: (() -> Void)? = nil
    var body: some View {
        let tools = item["connection_ids"].array.map { Watching.label(model, connection: $0.string) }
        let workspace = model.workspaces.first { $0.recordID == item["workspace_id"].string }?["name"].string ?? "Workspace"
        let missing = Watching.ungranted(model, item)
        let live = item["enabled"].bool
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                PixelGlyph(activity: live ? .watching : .creating, size: 12, animated: live)
                Text(item["instruction"].string).font(.system(size: 13.5, weight: .medium)).foregroundStyle(N.text)
                    .fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
            }
            VStack(alignment: .leading, spacing: 4) {
                promise("checkmark", "Reads \(tools.joined(separator: ", ")) every 10 minutes in \(workspace) and brings what matters to Home.")
                promise("hand.raised", item["prepare_low_risk"].bool ? "May prepare low-risk local fixes; never publishes or messages anyone." : "Never changes, posts or replies to anything. Plans wait for your approval.")
                if !missing.isEmpty {
                    promise("exclamationmark.circle", "Connect or discover tools for \(missing.joined(separator: ", ")) before this check can run.", color: NekoStyle.amber)
                }
            }
            HStack(spacing: 8) {
                if live {
                    Label("Watching", systemImage: "checkmark").font(.system(size: 12, weight: .medium)).foregroundStyle(NekoStyle.mint)
                } else {
                    Button(missing.isEmpty ? "Turn on" : "Review connections") {
                        if missing.isEmpty { Watching.turnOn(model, item) }
                        else { Watching.reviewAccess(model, item) }
                    }.nekoPrimaryButton().controlSize(.small)
                }
                if let onEdit { Button("Edit", action: onEdit).controlSize(.small) }
                if !live { Button("Dismiss") { Watching.remove(model, item) }.controlSize(.small) }
            }.disabled(model.busy)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(N.card.opacity(0.6), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(N.line))
    }
    private func promise(_ symbol: String, _ text: String, color: Color = N.text3) -> some View {
        Label { Text(text).fixedSize(horizontal: false, vertical: true) } icon: { Image(systemName: symbol).frame(width: 14) }
            .font(.system(size: 12)).foregroundStyle(color)
    }
}

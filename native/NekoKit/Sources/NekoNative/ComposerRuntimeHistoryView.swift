import SwiftUI

struct ComposerRuntimeHistoryView: View {
    let history: RuntimeSelectionHistory
    let preferences: RuntimeSelectionPreferences
    let catalog: ModelCatalog

    var body: some View {
        DisclosureGroup {
            VStack(alignment: .leading, spacing: 6) {
                if !history.reason.isEmpty { Text(history.reason) }
                Text("Requested settings; the served speed is not confirmed.").foregroundStyle(.secondary)
            }.font(.caption).frame(maxWidth: .infinity, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
        } label: {
            VStack(alignment: .leading, spacing: 3) {
                Text("Last run requested").font(.caption).foregroundStyle(.secondary)
                Text(RuntimeSelectionLabel.compact(history.runtime, catalog: catalog)).font(.caption)
                if history.differs(from: preferences) {
                    Text("Settings changed since this run.").font(.caption2).foregroundStyle(.secondary)
                }
            }
        }
    }
}

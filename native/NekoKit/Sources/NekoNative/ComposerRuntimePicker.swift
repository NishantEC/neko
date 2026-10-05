import SwiftUI
import NekoKit

/// Production adapter. A request checks and saves only this captured conversation.
struct ComposerRuntimePicker: View {
    @ObservedObject var model: AppModel
    // Injected by ComposerRuntimeScope above ViewThatFits; sibling pickers share it.
    let selection: RuntimeSelectionState
    private var conversationID: String { selection.conversationID }
    var body: some View {
        ComposerRuntimeControls(
            selection: selection,
            history: .latest(in: model.snapshot, conversationID: conversationID),
            refresh: { Task { await load(refresh: true) } },
            apply: { Task { await selection.apply { target, preferences in
                try await ComposerRuntimePicker.save(model, conversationID: target, preferences: preferences)
            } } }
        )
    }
    private func load(refresh: Bool) async {
        await selection.load { try await AgentModelCatalog.fetch(model, refresh: refresh) }
    }
    @MainActor static func save(_ model: AppModel, conversationID: String, preferences: RuntimeSelectionPreferences) async throws -> RuntimeSelectionConfirmation {
        let reply = try await model.request(RuntimeSelectionScope.request(conversationID: conversationID, preferences: preferences))
        guard case .object = reply["Workbench"]["conversation_runtime"],
              let confirmed = RuntimeSelectionConfirmation.read(in: reply["Workbench"], conversationID: conversationID, requireRevision: true),
              confirmed.preferences == preferences else {
            throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "The daemon did not confirm these conversation settings. Refresh before trying again."])
        }
        await model.refresh()
        return confirmed
    }
}

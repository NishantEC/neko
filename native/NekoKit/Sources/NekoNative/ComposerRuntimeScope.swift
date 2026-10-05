import SwiftUI

/// Owns one selection above the composer's responsive branches. The caller keys this
/// view by conversation ID, so navigation releases its draft while a captured save finishes.
struct ComposerRuntimeScope<Content: View>: View {
    @ObservedObject var model: AppModel
    let conversationID: String
    @ViewBuilder let content: (RuntimeSelectionState) -> Content
    @State private var selection: RuntimeSelectionState

    init(model: AppModel, conversationID: String, @ViewBuilder content: @escaping (RuntimeSelectionState) -> Content) {
        self.model = model
        self.conversationID = conversationID
        self.content = content
        let confirmed = RuntimeSelectionConfirmation.read(in: model.snapshot, conversationID: conversationID) ?? .init()
        _selection = State(initialValue: RuntimeSelectionState(
            conversationID: conversationID, preferences: confirmed.preferences, revision: confirmed.revision
        ))
    }
    private var confirmation: RuntimeSelectionConfirmation? {
        .read(in: model.snapshot, conversationID: conversationID)
    }
    private func sync(_ value: RuntimeSelectionConfirmation?) {
        if let value { selection.sync(value.preferences, revision: value.revision) }
    }
    var body: some View {
        content(selection)
            .task {
                sync(confirmation)
                await selection.load { try await AgentModelCatalog.fetch(model) }
            }
            .onChange(of: confirmation) { _, value in sync(value) }
    }
}

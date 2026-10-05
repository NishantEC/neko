import Foundation
import Observation
import NekoKit

/// One instance per conversation, or one isolated instance owned by Design Lab.
@MainActor @Observable final class RuntimeSelectionState {
    let conversationID: String
    var catalog = ModelCatalog()
    private(set) var confirmed = RuntimeSelectionConfirmation()
    var saved: RuntimeSelectionPreferences { confirmed.preferences }
    var savedRevision: UInt64 { confirmed.revision }
    @ObservationIgnored private var deferredConfirmation: RuntimeSelectionConfirmation?
    var draft = RuntimeSelectionPreferences()
    private(set) var saving = false
    private(set) var loading = false
    private(set) var loaded = false
    private(set) var catalogError: String?
    private(set) var applyError: String?
    private(set) var status: String?
    private(set) var reconciliation: [String] = []

    init(conversationID: String, preferences: RuntimeSelectionPreferences = .init(), revision: UInt64 = 0, catalog: ModelCatalog? = nil) {
        self.conversationID = conversationID
        self.confirmed = .init(preferences: preferences, revision: revision)
        self.draft = preferences
        if let catalog { self.catalog = catalog; loaded = true }
    }
    var dirty: Bool { draft != saved }
    var effortOptions: [CatalogRuntimeOption] { RuntimeSelectionLabel.options(draft, catalog: catalog, speed: false) }
    var extraSpeedOptions: [CatalogRuntimeOption] { RuntimeSelectionLabel.options(draft, catalog: catalog, speed: true) }
    var selectedModel: CatalogModel? {
        guard let provider = draft.provider, let id = draft.model else { return nil }
        return catalog.model(provider, id)
    }
    var validationMessage: String? {
        guard loaded, !draft.isAutomatic else { return nil }
        let candidates = catalog.sources.filter(\.ready).filter { draft.provider == nil || $0.provider == draft.provider }
            .flatMap(\.models).filter(\.usable).filter { draft.model == nil || $0.id == draft.model }
        let compatible = candidates.contains { model in
            (draft.reasoningEffort == nil || (model.effortOptions ?? []).contains { $0.id == draft.reasoningEffort }) &&
            (draft.serviceTier == nil || draft.serviceTier == "default" || (model.speedOptions ?? []).contains { $0.id == draft.serviceTier })
        }
        return compatible ? nil : "No connected model advertises these pins. Choose a model, reset an axis to Auto, or refresh models."
    }
    var canApply: Bool { dirty && !saving && !loading && validationMessage == nil && (loaded || draft.isAutomatic) }

    func sync(_ preferences: RuntimeSelectionPreferences, revision: UInt64) {
        guard revision > savedRevision else { return }
        let incoming = RuntimeSelectionConfirmation(preferences: preferences, revision: revision)
        if saving {
            if revision > (deferredConfirmation?.revision ?? savedRevision) { deferredConfirmation = incoming }
            return
        }
        let wasDirty = dirty
        confirmed = incoming
        if !wasDirty { draft = preferences }
        else { status = "Saved settings changed elsewhere. Your unapplied draft is kept here." }
    }
    func select(provider: String, model: CatalogModel) {
        reconciliation = draft.selectModel(provider: provider, model: model)
        applyError = nil
        status = nil
    }
    func reset() {
        draft = .init()
        reconciliation = []
        applyError = nil
        status = nil
    }
    func discard() {
        draft = saved
        reconciliation = []
        applyError = nil
        status = nil
    }
    func load(using fetch: () async throws -> ModelCatalog) async {
        guard !loading else { return }
        loading = true
        catalogError = nil
        defer { loading = false }
        do { catalog = try await fetch(); loaded = true }
        catch { catalogError = error.localizedDescription }
    }
    func apply(using save: (String, RuntimeSelectionPreferences) async throws -> RuntimeSelectionConfirmation) async {
        guard canApply else { return }
        let submitted = draft
        let target = conversationID
        saving = true
        applyError = nil
        status = nil
        defer {
            saving = false
            if let deferred = deferredConfirmation {
                deferredConfirmation = nil
                sync(deferred.preferences, revision: deferred.revision)
            }
        }
        do {
            let accepted = try await save(target, submitted)
            guard accepted.revision > savedRevision else {
                throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "The daemon returned an outdated settings revision. Refresh before trying again."])
            }
            confirmed = accepted
            if draft == submitted { draft = accepted.preferences }
            status = "Settings applied."
            reconciliation = []
        } catch {
            applyError = error.localizedDescription
        }
    }
    func applySample() {
        guard canApply else { return }
        confirmed = .init(preferences: draft, revision: savedRevision + 1) // Local preview only.
        status = "Sample settings applied."
        reconciliation = []
        applyError = nil
    }
}

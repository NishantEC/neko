import Foundation

enum RuntimeSelectionLabel {
    static func model(_ preferences: RuntimeSelectionPreferences, catalog: ModelCatalog) -> String {
        guard let provider = preferences.provider, let id = preferences.model else { return "Neko decides" }
        let label = catalog.model(provider, id)?.label ?? id
        // Presentation-only shortening; capabilities and wire IDs always come from metadata.
        return label.replacingOccurrences(of: "^GPT[- ]?[0-9]+(?:\\.[0-9]+)?[- ]", with: "", options: [.regularExpression, .caseInsensitive])
    }
    static func effort(_ preferences: RuntimeSelectionPreferences, catalog: ModelCatalog) -> String {
        guard let id = preferences.reasoningEffort else { return "Auto" }
        return options(preferences, catalog: catalog, speed: false).first { $0.id == id }?.label ?? id.capitalized
    }
    static func speed(_ preferences: RuntimeSelectionPreferences, catalog: ModelCatalog) -> String {
        guard let id = preferences.serviceTier else { return "Auto" }
        if id == "default" { return "Normal" }
        return options(preferences, catalog: catalog, speed: true).first { $0.id == id }?.label ?? id
    }
    static func compact(_ preferences: RuntimeSelectionPreferences, catalog: ModelCatalog) -> String {
        var parts = [model(preferences, catalog: catalog)]
        if preferences.reasoningEffort != nil { parts.append(effort(preferences, catalog: catalog)) }
        if preferences.serviceTier != nil { parts.append(speed(preferences, catalog: catalog)) }
        return parts.joined(separator: " · ")
    }
    static func options(_ preferences: RuntimeSelectionPreferences, catalog: ModelCatalog, speed: Bool) -> [CatalogRuntimeOption] {
        let models: [CatalogModel]
        if let provider = preferences.provider, let id = preferences.model {
            models = catalog.model(provider, id).map { [$0] } ?? []
        } else {
            models = catalog.sources.filter(\.ready).flatMap(\.models).filter(\.usable)
        }
        var seen = Set<String>()
        return models.flatMap { speed ? ($0.speedOptions ?? []) : ($0.effortOptions ?? []) }
            .filter { seen.insert($0.id).inserted }
    }
}

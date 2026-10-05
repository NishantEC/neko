/// Illustrative metadata only. Never used by production or sent to a provider.
enum RuntimeSelectionSample {
    static let catalog = ModelCatalog(sources: [
        ModelSource(provider: "sample", label: "Sample models", connection: "Local preview", status: .ready, defaultModel: "sample-astra", models: [
            CatalogModel(id: "sample-astra", label: "Astra", description: "Sample capable model with three speed choices.",
                         effortOptions: [
                            .init(id: "low", label: "Low", description: "A quick pass for straightforward work."),
                            .init(id: "medium", label: "Medium", description: "Balanced depth for everyday tasks."),
                            .init(id: "high", label: "High", description: "More reasoning for difficult decisions."),
                         ], defaultEffort: "medium", speedOptions: [
                            .init(id: "priority", label: "Fast", description: "Sample priority service."),
                            .init(id: "rush", label: "Express", description: "A third sample tier; preserved as advertised."),
                         ], defaultSpeed: "default"),
            CatalogModel(id: "sample-quick", label: "Quick", description: "Sample model with one extra speed tier.",
                         effortOptions: [.init(id: "low", label: "Low"), .init(id: "medium", label: "Medium")],
                         defaultEffort: "low", speedOptions: [.init(id: "priority", label: "Fast")], defaultSpeed: "default"),
            CatalogModel(id: "sample-fixed", label: "Simple", description: "Sample model without effort or speed controls.",
                         effortOptions: [], speedOptions: []),
        ])
    ])
}

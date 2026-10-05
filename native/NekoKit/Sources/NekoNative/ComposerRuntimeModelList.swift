import SwiftUI

struct ComposerRuntimeModelList: View {
    @Bindable var selection: RuntimeSelectionState
    let selected: () -> Void
    @State private var query = ""

    private func matches(_ model: CatalogModel, source: ModelSource) -> Bool {
        query.isEmpty || [model.id, model.label, model.description ?? "", source.label].contains {
            $0.localizedStandardContains(query)
        }
    }
    private var hasMatches: Bool {
        selection.catalog.sources.contains { source in source.models.contains { matches($0, source: source) } }
    }
    var body: some View {
        LazyVStack(alignment: .leading, spacing: 12) {
            TextField("Search models", text: $query).textFieldStyle(.roundedBorder)
                .accessibilityLabel("Search available models")
            Button {
                selection.draft.automaticModel()
                selected()
            } label: {
                HStack {
                    Text("Neko decides").fontWeight(.medium)
                    Spacer()
                    if selection.draft.model == nil { Image(systemName: "checkmark") }
                }.contentShape(Rectangle())
            }.buttonStyle(.borderless).accessibilityHint("Reset only the model to Auto; keep effort and speed pins")
            ForEach(selection.catalog.sources) { source in
                let models = source.models.filter { matches($0, source: source) }
                if !models.isEmpty || (!source.ready && query.isEmpty) {
                    LazyVStack(alignment: .leading, spacing: 8) {
                        Text(source.label + (source.connection.isEmpty ? "" : " · " + source.connection))
                            .font(.caption).foregroundStyle(.secondary)
                        if !source.ready { Text(source.note ?? "This source is not ready.").font(.caption).foregroundStyle(.secondary) }
                        ForEach(models) { model in
                            Button {
                                selection.select(provider: source.provider, model: model)
                                selected()
                            } label: {
                                HStack(alignment: .top) {
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(model.label).fontWeight(.medium)
                                        if let detail = model.reason ?? model.description {
                                            Text(detail).font(.caption).foregroundStyle(.secondary)
                                                .fixedSize(horizontal: false, vertical: true)
                                        }
                                        if !model.usable { Text("Unavailable").font(.caption).foregroundStyle(.secondary) }
                                    }
                                    Spacer(minLength: 8)
                                    if selection.draft.provider == source.provider && selection.draft.model == model.id {
                                        Image(systemName: "checkmark").accessibilityLabel("Selected")
                                    }
                                }.padding(.vertical, 5).contentShape(Rectangle())
                            }.buttonStyle(.borderless).disabled(!source.ready || !model.usable)
                        }
                    }
                }
            }
            if !hasMatches && !selection.loading {
                Text(query.isEmpty ? "No models available. Refresh models or connect a provider in Settings → AI." : "No models match this search.")
                    .font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

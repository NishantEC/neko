import SwiftUI

struct ComposerRuntimeEffortControl: View {
    @Bindable var selection: RuntimeSelectionState
    var showsTitle = false
    private var options: [CatalogRuntimeOption] { selection.effortOptions }
    private var sliderValue: Binding<Double> {
        Binding(get: {
            guard let effort = selection.draft.reasoningEffort,
                  let index = options.firstIndex(where: { $0.id == effort }) else { return 0 }
            return Double(index + 1)
        }, set: { value in
            let index = Int(value.rounded()) - 1
            selection.draft.reasoningEffort = options.indices.contains(index) ? options[index].id : nil
        })
    }
    private var defaultHelp: String {
        if let model = selection.selectedModel, let value = model.defaultEffort,
           let option = options.first(where: { $0.id == value }) { return "Model default: \(option.label). Auto lets Neko choose effort for the task." }
        return "Auto lets Neko choose effort for the task."
    }
    var body: some View {
        if !options.isEmpty {
            VStack(alignment: .leading, spacing: 5) {
                if showsTitle {
                    HStack {
                        Text("Effort").foregroundStyle(.secondary)
                        Spacer()
                        Text(RuntimeSelectionLabel.effort(selection.draft, catalog: selection.catalog))
                        Button("Auto") { selection.draft.reasoningEffort = nil }
                            .buttonStyle(.borderless).disabled(selection.draft.reasoningEffort == nil)
                            .accessibilityLabel("Reset effort to Auto")
                    }
                }
                Slider(value: sliderValue, in: 0...Double(options.count), step: 1) {
                    Text("Reasoning effort")
                }
                .labelsHidden()
                .accessibilityLabel("Reasoning effort")
                .accessibilityValue(RuntimeSelectionLabel.effort(selection.draft, catalog: selection.catalog))
                .accessibilityHint("Discrete advertised levels, starting with Auto")
                .help(defaultHelp)
                HStack {
                    Text("Auto")
                    Spacer()
                    Text(options.last?.label ?? "")
                }.font(.caption2).foregroundStyle(.secondary).accessibilityHidden(true)
                if let option = options.first(where: { $0.id == selection.draft.reasoningEffort }), let description = option.description {
                    Text(description).font(.caption).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        } else if selection.draft.reasoningEffort != nil {
            HStack {
                Text("Effort pin unavailable").font(.caption).foregroundStyle(NekoStyle.amber)
                Spacer()
                Button("Reset effort to Auto") { selection.draft.reasoningEffort = nil }.buttonStyle(.borderless)
            }
        }
    }
}

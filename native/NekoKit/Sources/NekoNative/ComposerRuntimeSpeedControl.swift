import SwiftUI

struct ComposerRuntimeSpeedControl: View {
    @Bindable var selection: RuntimeSelectionState
    private var options: [CatalogRuntimeOption] { selection.extraSpeedOptions }
    private var faster: Binding<Bool> {
        Binding(get: { selection.draft.serviceTier == options.first?.id && selection.draft.serviceTier != nil },
                set: { selection.draft.serviceTier = $0 ? options.first?.id : "default" })
    }
    private var tier: Binding<String> {
        Binding(get: { selection.draft.serviceTier ?? "auto" },
                set: { selection.draft.serviceTier = $0 == "auto" ? nil : $0 })
    }
    private var details: String {
        let description = options.first(where: { $0.id == selection.draft.serviceTier })?.description
        let defaultID = selection.selectedModel?.defaultSpeed
        let defaultLabel = defaultID == "default" ? "Normal" : options.first(where: { $0.id == defaultID })?.label
        return [description, defaultLabel.map { "Model default: \($0)." }, "Extra speed may use more allowance. Auto respects the paid speed preference."].compactMap { $0 }.joined(separator: " ")
    }
    var body: some View {
        if !options.isEmpty {
            if options.count == 1, let option = options.first {
                HStack(spacing: 10) {
                    Toggle(option.label + " speed", isOn: faster).toggleStyle(.switch).controlSize(.small)
                        .accessibilityLabel("Speed: Normal or \(option.label)")
                    Spacer(minLength: 0)
                    if selection.draft.serviceTier == nil {
                        Menu("Auto") {
                            Button("Normal") { selection.draft.serviceTier = "default" }
                            Button(option.label) { selection.draft.serviceTier = option.id }
                        }.menuStyle(.borderlessButton).fixedSize()
                            .accessibilityLabel("Speed is Auto; choose a manual speed")
                    } else {
                        Text(RuntimeSelectionLabel.speed(selection.draft, catalog: selection.catalog)).font(.caption).foregroundStyle(.secondary)
                        Button("Auto") { selection.draft.serviceTier = nil }
                            .buttonStyle(.borderless).accessibilityLabel("Reset speed to Auto")
                    }
                }.help(details + " Off pins Normal; on pins \(option.label).")
            } else {
                Picker("Speed", selection: tier) {
                    Text("Auto").tag("auto")
                    Text("Normal").tag("default")
                    ForEach(options) { Text($0.label).tag($0.id) }
                }.pickerStyle(.menu).help(details)
            }
        } else if let tier = selection.draft.serviceTier {
            HStack {
                Text(tier == "default" ? "Speed: Normal" : "Speed pin unavailable").font(.caption)
                Spacer()
                Button("Auto") { selection.draft.serviceTier = nil }.buttonStyle(.borderless)
                    .accessibilityLabel("Reset speed to Auto")
            }
        }
    }
}

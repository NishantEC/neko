import SwiftUI

struct ComposerRuntimeSettingsView: View {
    @Bindable var selection: RuntimeSelectionState
    @Binding var page: ComposerRuntimeControls.Page
    let history: RuntimeSelectionHistory?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Button { page = .models } label: {
                HStack {
                    Text("Model").foregroundStyle(.secondary)
                    Spacer()
                    Text(RuntimeSelectionLabel.model(selection.draft, catalog: selection.catalog)).lineLimit(2)
                    Image(systemName: "chevron.right").font(.caption)
                }.contentShape(Rectangle())
            }
            .buttonStyle(.borderless)
            .help("Neko decides chooses a compatible model while keeping your effort and speed pins.")
            if page == .speed {
                ComposerRuntimeSpeedControl(selection: selection)
                ComposerRuntimeEffortControl(selection: selection, showsTitle: true)
            } else {
                ComposerRuntimeEffortControl(selection: selection)
                ComposerRuntimeSpeedControl(selection: selection)
            }
            if let model = selection.selectedModel, model.effortOptions == nil || model.speedOptions == nil {
                Text("Some capabilities are unknown. Refresh to check.").font(.caption).foregroundStyle(.secondary)
            }
            Toggle("Allow automatic paid speed", isOn: $selection.draft.allowPaidSpeed)
                .toggleStyle(.switch).controlSize(.small)
                .help("Off by default. When speed is Auto, allow Neko to choose a tier that may use more allowance.")
            Button("Reset to Neko decides", action: selection.reset).buttonStyle(.borderless)
                .help("Clear all manual pins and turn automatic paid speed off.")
            if let history {
                ComposerRuntimeHistoryView(history: history, preferences: selection.saved, catalog: selection.catalog)
            }
        }
    }
}

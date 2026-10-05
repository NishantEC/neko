import SwiftUI

struct ComposerRuntimePanel: View {
    @Bindable var selection: RuntimeSelectionState
    @Binding var page: ComposerRuntimeControls.Page
    let sample: Bool
    let history: RuntimeSelectionHistory?
    let refresh: () -> Void
    let apply: () -> Void

    private var title: String {
        switch page {
        case .models: "Model"
        case .speed: selection.draft.serviceTier == nil ? "Auto speed" : RuntimeSelectionLabel.speed(selection.draft, catalog: selection.catalog)
        case .effort: selection.draft.reasoningEffort == nil ? "Neko decides" : RuntimeSelectionLabel.effort(selection.draft, catalog: selection.catalog)
        }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                if page == .models {
                    Button("Back to effort", systemImage: "chevron.left") { page = .effort }
                        .labelStyle(.iconOnly).buttonStyle(.borderless)
                }
                Text(title).font(.headline).help(page == .effort ? "Reasoning effort" : title)
                Spacer()
                if sample { Text("Sample").font(.caption).foregroundStyle(.secondary) }
                if page == .effort && selection.draft.reasoningEffort != nil {
                    Button("Auto") { selection.draft.reasoningEffort = nil }
                        .buttonStyle(.borderless).disabled(selection.saving).accessibilityLabel("Reset effort to Auto")
                }
                Button("Refresh models", systemImage: "arrow.clockwise", action: refresh)
                    .labelStyle(.iconOnly).buttonStyle(.borderless).disabled(selection.loading || selection.saving)
            }
            if selection.loading {
                HStack { ProgressView().controlSize(.small); Text("Loading models…").font(.caption) }
            }
            if let error = selection.catalogError {
                Text("Models could not be loaded: \(error)").font(.caption).foregroundStyle(NekoStyle.amber)
                    .fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
            }
            Group {
                if page == .models {
                    ScrollView {
                        ComposerRuntimeModelList(selection: selection, selected: { page = .effort })
                    }.frame(height: 300)
                } else {
                    ComposerRuntimeSettingsView(selection: selection, page: $page, history: history)
                }
            }.disabled(selection.saving)
            ForEach(selection.reconciliation, id: \.self) { notice in
                Text(notice).font(.caption).foregroundStyle(NekoStyle.amber)
            }
            if let message = selection.validationMessage {
                Text(message).font(.caption).foregroundStyle(NekoStyle.amber)
            }
            Divider()
            if let error = selection.applyError {
                Text("Settings were not applied. \(error)").font(.caption).foregroundStyle(NekoStyle.amber)
                    .fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
            } else if let status = selection.status, !selection.dirty {
                Text(status).font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                if selection.saving {
                    ProgressView().controlSize(.small)
                    Text("Checking…").font(.caption)
                } else if selection.dirty {
                    Button("Discard", action: selection.discard).buttonStyle(.borderless)
                    Text("Not applied").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button(sample ? "Apply sample" : "Check & apply", action: apply)
                    .buttonStyle(.borderedProminent).disabled(!selection.canApply)
            }
            Text(sample ? "Local sample. No check or worker runs."
                 : "One short check may use allowance. Applies to the next turn or worker.")
                .font(.caption2).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                .help(sample ? "Sample catalog and local settings only."
                      : "Manual pins are checked before saving. Active work keeps its captured settings. Resetting to automatic settings does not run an inference check.")
        }
        .padding(16).frame(width: 350)
        .fixedSize(horizontal: false, vertical: true)
    }
}

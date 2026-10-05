import SwiftUI

struct ComposerRuntimeControls: View {
    enum Layout { case inline, separate }
    enum Page { case effort, models, speed }
    @Bindable var selection: RuntimeSelectionState
    var layout = Layout.inline
    var sample = false
    var history: RuntimeSelectionHistory?
    let refresh: () -> Void
    let apply: () -> Void
    @State private var showing = false
    @State private var page = Page.effort

    var body: some View {
        HStack(spacing: 4) {
            trigger(title: layout == .inline
                    ? RuntimeSelectionLabel.compact(selection.saved, catalog: selection.catalog)
                    : RuntimeSelectionLabel.model(selection.saved, catalog: selection.catalog),
                    page: layout == .inline ? .effort : .models, pearl: true,
                    lightning: layout == .inline && selection.saved.requestsExtraSpeed)
            if layout == .separate {
                if !selection.effortOptions.isEmpty || selection.saved.reasoningEffort != nil {
                    trigger(title: RuntimeSelectionLabel.effort(selection.saved, catalog: selection.catalog), page: .effort)
                }
                if !selection.extraSpeedOptions.isEmpty || selection.saved.serviceTier != nil {
                    trigger(title: RuntimeSelectionLabel.speed(selection.saved, catalog: selection.catalog), page: .speed,
                            lightning: selection.saved.requestsExtraSpeed)
                }
            }
        }
        .popover(isPresented: $showing, arrowEdge: .bottom) {
            ComposerRuntimePanel(selection: selection, page: $page, sample: sample, history: history,
                                 refresh: refresh, apply: apply)
        }
    }

    private func trigger(title: String, page destination: Page, pearl: Bool = false, lightning: Bool = false) -> some View {
        Button { page = destination; showing = true } label: {
            HStack(spacing: 6) {
                if pearl { DesignLabOrb(size: 20) }
                if selection.saving && pearl { ProgressView().controlSize(.mini).accessibilityLabel("Checking settings") }
                if lightning { Image(systemName: "bolt.fill").accessibilityHidden(true) }
                Text(title).lineLimit(1).truncationMode(.middle)
                Image(systemName: "chevron.down").font(.system(size: 8, weight: .semibold)).foregroundStyle(.secondary)
            }
            .font(NekoFont.meta.weight(.medium)).padding(.horizontal, 8).frame(height: 34)
            .contentShape(.rect(cornerRadius: 11))
        }
        .buttonStyle(DesignLabHoverStyle())
        .accessibilityLabel("\(sample ? "Sample settings" : "Conversation settings"): \(title)")
        .help(sample ? "Sample catalog; changes stay in this preview." : "Model, effort and speed for this conversation. Changes apply to the next turn or worker.")
    }
}

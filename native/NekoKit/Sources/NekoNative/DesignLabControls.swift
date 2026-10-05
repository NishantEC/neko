import SwiftUI

struct DesignLabControls: View {
    @Bindable var preview: DesignLabModel
    var expanded = false
    @State private var showingPlugins = false

    var body: some View {
        HStack(spacing: expanded ? 18 : 8) {
            ComposerRuntimeControls(
                selection: preview.runtime,
                layout: expanded ? .separate : .inline,
                sample: true,
                refresh: preview.refreshRuntimeSample,
                apply: preview.runtime.applySample
            )
            Button(action: openPlugins) {
                HStack(spacing: 7) {
                    Image(systemName: "puzzlepiece.extension").font(.system(size: 12))
                    Text("Plugins").font(.system(size: 12, weight: .medium))
                    Text(preview.plugins.count, format: .number)
                        .font(.system(size: 10, weight: .medium).monospacedDigit())
                        .padding(.horizontal, 5).padding(.vertical, 2)
                        .background(.primary.opacity(0.07), in: .rect(cornerRadius: 4))
                    Image(systemName: "chevron.down").font(.system(size: 8, weight: .semibold)).foregroundStyle(.secondary)
                }
                .padding(.horizontal, 10).frame(height: 34)
                .contentShape(.rect(cornerRadius: 11))
            }
            .buttonStyle(DesignLabHoverStyle()).fixedSize().accessibilityLabel("Preview plugins, \(preview.plugins.count) selected")
            .popover(isPresented: $showingPlugins, arrowEdge: .bottom) {
                DesignLabPluginMenu(preview: preview, close: { showingPlugins = false })
            }
        }
    }

    private func openPlugins() { showingPlugins.toggle() }
}

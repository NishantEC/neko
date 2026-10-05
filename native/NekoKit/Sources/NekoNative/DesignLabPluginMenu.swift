import SwiftUI

struct DesignLabPluginMenu: View {
    let preview: DesignLabModel
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Plugins for this chat").font(.headline)
            Text("Sample capabilities · preview only").font(.caption).foregroundStyle(.secondary)
            Divider()
            ForEach(DesignLabModel.Plugin.allCases) { plugin in
                Toggle(isOn: Binding(get: { preview.plugins.contains(plugin) }, set: { _ in preview.toggle(plugin) })) {
                    Label(plugin.rawValue, systemImage: plugin.symbol)
                        .font(.system(size: 14)).padding(.vertical, 4)
                }
            }
            Divider()
            Button("Explore sample plugins…") { close(); preview.showingLibrary = true }
                .buttonStyle(.borderless)
        }.padding(22).frame(width: 300)
    }
}

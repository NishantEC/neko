import SwiftUI

struct DesignLabPluginLibrary: View {
    let preview: DesignLabModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack {
                VStack(alignment: .leading, spacing: 6) {
                    Text("A little more capable.").font(.title2.weight(.semibold))
                    Text("Plugin library · sample content").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            Divider()
            ForEach(DesignLabModel.Plugin.allCases) { plugin in
                HStack(spacing: 16) {
                    Image(systemName: plugin.symbol).font(.system(size: 20)).foregroundStyle(.purple)
                        .frame(width: 44, height: 44)
                        .background(.purple.opacity(0.08), in: .rect(cornerRadius: 13))
                    VStack(alignment: .leading, spacing: 6) {
                        Text(plugin.rawValue).font(.headline)
                        Text(plugin.detail).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button(preview.plugins.contains(plugin) ? "Enabled" : "Enable") { preview.toggle(plugin) }
                        .accessibilityLabel("\(preview.plugins.contains(plugin) ? "Disable" : "Enable") \(plugin.rawValue) in preview")
                }
            }
            Text("These switches change the preview only. No packages or connections are installed.")
                .font(.caption).foregroundStyle(.secondary)
        }.padding(30).frame(width: 540)
    }
}

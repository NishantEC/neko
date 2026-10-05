import SwiftUI

struct DesignLabAgentMenu: View {
    let preview: DesignLabModel
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Agents on this task").font(.headline)
            Text("Sample team · Ship dark mode").font(.caption).foregroundStyle(.secondary)
            Divider()
            ForEach(DesignLabModel.Agent.allCases) { agent in
                Button { close(); preview.selectedAgent = agent } label: {
                    HStack(spacing: 12) {
                        DesignLabOrb(color: agent.color, size: 28)
                        VStack(alignment: .leading, spacing: 5) {
                            Text(agent.rawValue).font(.system(size: 14, weight: .semibold))
                            Text(agent.activity).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        }
                        Spacer()
                        Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
                    }.padding(.vertical, 6).contentShape(Rectangle())
                }.buttonStyle(.plain)
            }
        }.padding(22).frame(width: 360)
    }
}

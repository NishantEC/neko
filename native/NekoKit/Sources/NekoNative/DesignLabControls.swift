import SwiftUI

struct DesignLabControls: View {
    @Bindable var preview: DesignLabModel
    var expanded = false
    @State private var showingAgents = false
    @State private var showingPlugins = false

    var body: some View {
        HStack(spacing: expanded ? 18 : 8) {
            if expanded {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 24) {
                        ForEach(DesignLabModel.Agent.allCases) { agent in
                            Button { preview.selectedAgent = agent } label: {
                                HStack(spacing: 9) {
                                    DesignLabOrb(color: agent.color, size: 28)
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(agent.rawValue).font(.system(size: 12, weight: .semibold))
                                        Text(agent.status).font(.system(size: 10)).foregroundStyle(.secondary)
                                    }
                                }.padding(.vertical, 2).contentShape(Rectangle())
                            }.buttonStyle(.plain).help("Open \(agent.rawValue) sample agent")
                        }
                    }
                    agentButton
                }
                Spacer(minLength: 0)
            } else { agentButton }
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
                .background(.primary.opacity(0.035), in: .rect(cornerRadius: 11))
                .contentShape(.rect(cornerRadius: 11))
            }
            .buttonStyle(.plain).fixedSize().accessibilityLabel("Preview plugins, \(preview.plugins.count) selected")
            .popover(isPresented: $showingPlugins, arrowEdge: .bottom) {
                DesignLabPluginMenu(preview: preview, close: { showingPlugins = false })
            }
        }
    }

    private var agentButton: some View {
        Button(action: openAgents) {
            HStack(spacing: 8) {
                HStack(spacing: -3) {
                    ForEach(DesignLabModel.Agent.allCases) { agent in DesignLabOrb(color: agent.color, size: 19) }
                }
                Text("3 agents").font(.system(size: 12, weight: .medium))
                Image(systemName: "chevron.down").font(.system(size: 8, weight: .semibold)).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10).frame(height: 34)
            .background(.primary.opacity(0.035), in: .rect(cornerRadius: 11))
            .contentShape(.rect(cornerRadius: 11))
        }
        .buttonStyle(.plain).fixedSize().accessibilityLabel("Preview agents, 3 working")
        .popover(isPresented: $showingAgents, arrowEdge: .bottom) {
            DesignLabAgentMenu(preview: preview, close: { showingAgents = false })
        }
    }

    private func openAgents() { showingPlugins = false; showingAgents.toggle() }
    private func openPlugins() { showingAgents = false; showingPlugins.toggle() }
}

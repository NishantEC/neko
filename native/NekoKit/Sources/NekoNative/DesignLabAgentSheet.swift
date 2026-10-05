import SwiftUI

struct DesignLabAgentSheet: View {
    let agent: DesignLabModel.Agent
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            HStack(spacing: 14) {
                DesignLabOrb(color: agent.color, size: 40)
                VStack(alignment: .leading, spacing: 5) {
                    Text(agent.title).font(.title2.weight(.semibold))
                    Text("\(agent.role) · \(agent.status) · Sample").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            Divider()
            Text(agent.activity).font(.system(size: 18)).lineSpacing(5)
            Text("This is the agent detail interaction in the native design study. The working app’s agents are unchanged.")
                .font(.body).foregroundStyle(.secondary)
        }.padding(30).frame(width: 460)
    }
}

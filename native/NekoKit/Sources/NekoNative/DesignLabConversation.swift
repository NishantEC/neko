import SwiftUI

struct DesignLabConversation: View {
    let messages: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: 28) {
            HStack {
                Label("SAMPLE CONVERSATION", systemImage: "circle.dotted")
                    .font(.system(size: 10, weight: .semibold)).tracking(1.1).foregroundStyle(.secondary)
                Spacer()
                Text("Try typing, sending and opening the controls.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Spacer(minLength: 70)
                Text("Ship dark mode: audit the colours, update the tokens, fix the contrast and add a toggle.")
                    .font(.system(size: 16)).lineSpacing(5)
                    .padding(18)
                    .background(Color.primary.opacity(0.045), in: .rect(cornerRadius: 20))
            }
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 10) {
                    DesignLabOrb(size: 30)
                    Text("Neko").font(.system(size: 15, weight: .semibold))
                    Text("Just now").font(.caption).foregroundStyle(.secondary)
                }
                Text("A quieter canvas. A little more depth.")
                    .font(.system(size: 28, weight: .semibold)).tracking(-0.6)
                Text("The team is tracing colours back to shared tokens. Your tools and agents stay within reach while the conversation gets the space.")
                    .font(.system(size: 16)).lineSpacing(6).foregroundStyle(.secondary)
            }
            DesignLabArtifact()
            HStack(spacing: 10) {
                Image(systemName: "checkmark.circle.fill").foregroundStyle(NekoStyle.mint)
                Text("Three focused agents. One place to follow the work.").font(.system(size: 14))
                Spacer()
            }.padding(.vertical, 8)
            ForEach(Array(messages.enumerated()), id: \.offset) { _, message in
                HStack {
                    Spacer(minLength: 70)
                    Text(message).font(.system(size: 16)).textSelection(.enabled)
                        .padding(18).background(Color.primary.opacity(0.045), in: .rect(cornerRadius: 20))
                }
                Label("Added to the local preview", systemImage: "checkmark")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

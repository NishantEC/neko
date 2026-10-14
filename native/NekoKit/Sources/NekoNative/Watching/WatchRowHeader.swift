import SwiftUI

struct WatchRowHeader: View {
    let title: String
    /// The full instruction, shown under a short name; nil when the title is the instruction.
    let instruction: String?
    let status: String
    let needsAttention: Bool
    @Binding var expanded: Bool

    var body: some View {
        Button { expanded.toggle() } label: {
            HStack(alignment: .top, spacing: NekoSpace.md) {
                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    .font(NekoFont.label).foregroundStyle(N.text3)
                    .frame(width: 12, height: 20).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: NekoSpace.sm) {
                    Text(verbatim: title).font(NekoFont.heading).foregroundStyle(N.text)
                        .lineLimit(instruction == nil ? 3 : 1).multilineTextAlignment(.leading)
                    if let instruction {
                        Text(verbatim: instruction).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(1)
                    }
                    Text(status).font(NekoFont.meta).foregroundStyle(needsAttention ? NekoStyle.amber : N.text3)
                }.frame(maxWidth: .infinity, alignment: .leading)
            }.contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(expanded ? "Expanded" : "Collapsed")
        .accessibilityHint(expanded ? "Collapse watch details" : "Show sources and full check result")
    }
}

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
            HStack(alignment: .firstTextBaseline, spacing: NekoSpace.md) {
                Image(systemName: "chevron.right")
                    .font(NekoFont.label).foregroundStyle(N.text3)
                    .rotationEffect(.degrees(expanded ? 90 : 0))
                    .nekoAnimation(.reveal, value: expanded)
                    .frame(width: NekoLayout.listGlyphSlot).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                    Text(verbatim: title).font(NekoFont.of(.callout, weight: .medium)).foregroundStyle(N.text)
                        .lineLimit(instruction == nil ? 3 : 1).multilineTextAlignment(.leading).help(title)
                    if let instruction {
                        Text(verbatim: instruction).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(1).help(instruction)
                    }
                    if needsAttention {
                        NekoStatusLabel(tone: .attention, symbol: "exclamationmark.triangle", text: "Last check failed")
                    } else {
                        Text(status).font(NekoFont.meta).foregroundStyle(N.text3)
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }.contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(expanded ? "Expanded" : "Collapsed")
        .accessibilityHint(expanded ? "Collapse watch details" : "Show sources and full check result")
    }
}

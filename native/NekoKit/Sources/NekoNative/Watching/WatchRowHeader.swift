import SwiftUI

struct WatchRowHeader: View {
    let instruction: String
    let status: String
    let needsAttention: Bool
    @Binding var expanded: Bool

    var body: some View {
        Button { expanded.toggle() } label: {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    .font(NekoFont.label).foregroundStyle(N.text3)
                    .frame(width: 12, height: 20).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 8) {
                    Text(verbatim: instruction).font(NekoFont.heading).foregroundStyle(N.text)
                        .lineLimit(3).multilineTextAlignment(.leading)
                    Text(status).font(NekoFont.meta).foregroundStyle(needsAttention ? NekoStyle.amber : N.text3)
                }.frame(maxWidth: .infinity, alignment: .leading)
            }.contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(expanded ? "Expanded" : "Collapsed")
        .accessibilityHint(expanded ? "Collapse watch details" : "Show sources and full check result")
    }
}

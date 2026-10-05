import SwiftUI

struct WatchRowHeader: View {
    let instruction: String
    let status: String
    let needsAttention: Bool
    @Binding var expanded: Bool

    var body: some View {
        Button { expanded.toggle() } label: {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 10, weight: .semibold)).foregroundStyle(N.text3)
                    .frame(width: 12, height: 18).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 5) {
                    Text(instruction).font(NekoFont.body).foregroundStyle(N.text)
                        .lineLimit(2).multilineTextAlignment(.leading)
                    Text(status).font(NekoFont.meta).foregroundStyle(needsAttention ? NekoStyle.amber : N.text3)
                }.frame(maxWidth: .infinity, alignment: .leading)
            }.contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityHint(expanded ? "Collapse watch details" : "Show sources and full check result")
    }
}

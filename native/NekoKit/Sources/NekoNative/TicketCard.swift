import SwiftUI

struct TicketCard: View {
    let id: String
    let title: String
    let workspace: String
    let status: String
    let updated: String
    let highlighted: Bool
    /// Why a stopped ticket stopped, shown under its title.
    var note: String? = nil
    @State private var hover = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .top, spacing: 12) {
                StatusGlyph(status: status, size: 14).padding(.top, 2).accessibilityHidden(true)
                Text(title).font(NekoFont.heading).foregroundStyle(.primary).lineSpacing(3).lineLimit(3)
                    .multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
            }
            if let note {
                Text(note).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(2).multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
            }
            VStack(alignment: .leading, spacing: 6) {
                Text(friendlyTaskStatus(status)).font(NekoFont.meta).foregroundStyle(.secondary)
                HStack(spacing: 8) {
                    Text(workspace).lineLimit(1)
                    Spacer(minLength: 4)
                    Text(updated).monospacedDigit().lineLimit(1)
                }.font(NekoFont.meta).foregroundStyle(.secondary)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            RoundedRectangle(cornerRadius: NekoRadius.xl).fill(N.panel)
                .overlay(RoundedRectangle(cornerRadius: NekoRadius.xl).fill(hover ? N.selected : .clear))
        }
        .overlay(RoundedRectangle(cornerRadius: NekoRadius.xl).strokeBorder(highlighted ? NekoStyle.accent : N.line))
        .contentShape(RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
        .help(id)
        .nekoAnimation(.hover, value: hover)
        .onHover { hover = $0 }
    }
}

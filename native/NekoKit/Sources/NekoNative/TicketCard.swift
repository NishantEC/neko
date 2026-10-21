import SwiftUI

/// A task on the board: a block inside a page, not a panel.
struct TicketCard: View {
    let id: String
    let title: String
    /// Source, repository and pull request on one line.
    let detail: String
    let status: String
    let updated: String
    let highlighted: Bool
    /// Why a stopped ticket stopped, shown under its title.
    var note: String? = nil
    @State private var hover = false
    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous) }
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.sm) {
            HStack(alignment: .firstTextBaseline, spacing: NekoSpace.sm) {
                StatusGlyph(status: status, size: 14).accessibilityHidden(true)
                Text(verbatim: title).font(NekoFont.of(.callout, weight: .medium)).foregroundStyle(N.text).lineLimit(2)
                    .multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
                    .help(title)
            }
            // Only where it says more than the column header does; a running
            // task's pulsing glyph already says it is running.
            if TaskStage(status: status) == .needsYou {
                NekoStatusLabel(tone: .attention, symbol: "hand.raised", text: friendlyTaskStatus(status))
            }
            if let note {
                Text(note).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(2).multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true).help(note)
            }
            HStack(spacing: NekoSpace.sm) {
                Text(detail).lineLimit(1)
                Spacer(minLength: NekoSpace.xxs)
                Text(updated).monospacedDigit().lineLimit(1)
            }.font(NekoFont.caption).foregroundStyle(N.text3)
        }
        .padding(NekoSpace.md)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            shape.fill(N.panel).overlay(shape.fill(hover ? N.selected : .clear))
        }
        .overlay(shape.strokeBorder(highlighted ? NekoStyle.accent : N.line))
        .contentShape(shape)
        .help(id)
        .nekoAnimation(.hover, value: hover)
        .onHover { hover = $0 }
    }
}

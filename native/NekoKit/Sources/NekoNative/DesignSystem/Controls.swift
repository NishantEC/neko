import SwiftUI

// MARK: - Buttons

/// What a button does, which decides how loud it is. Native button styles
/// underneath, so focus rings, keyboard activation and accessibility are the
/// platform's own.
enum NekoButtonKind {
    /// The one action a screen exists for. At most one per view.
    case primary
    /// Every other action.
    case secondary
    /// Inline actions in dense rows and chrome-adjacent spots.
    case quiet
    /// Removes or stops something. Pair with `Button(role: .destructive)`.
    case destructive
}

extension View {
    @ViewBuilder func nekoButton(_ kind: NekoButtonKind) -> some View {
        switch kind {
        case .primary: self.buttonStyle(.borderedProminent)
        case .secondary: self.buttonStyle(.bordered)
        case .quiet: self.buttonStyle(.borderless)
        case .destructive: self.buttonStyle(.bordered).tint(NekoStyle.coral)
        }
    }
}

// MARK: - Small labels

/// A count on a sidebar row or tab, in the accent colour.
struct NekoBadge: View {
    let count: Int
    var body: some View {
        if count > 0 {
            Text(String(count)).font(NekoFont.of(.small, weight: .semibold).monospacedDigit()).foregroundStyle(N.canvas)
                .padding(.horizontal, NekoSpace.xs).frame(height: 16).background(NekoStyle.accent, in: Capsule())
                .accessibilityLabel("\(count) need you")
        }
    }
}

/// A tinted symbol tile, for setting rows and empty states.
struct NekoIconTile: View {
    let symbol: String
    var tint: Color = NekoStyle.accent
    var size: CGFloat = 40
    var body: some View {
        Image(systemName: symbol).font(NekoFont.scaled(size * 0.5))
            .foregroundStyle(tint).frame(width: size, height: size)
            .background(tint.opacity(0.12), in: RoundedRectangle(cornerRadius: NekoRadius.md, style: .continuous))
            .accessibilityHidden(true)
    }
}

// MARK: - Status

/// Linear-style status ring: fills as work advances, pulses while running.
struct StatusGlyph: View {
    let status: String
    var size: CGFloat = 14
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pulse = false
    var color: Color { taskColor(status) }
    var progress: Double {
        switch status { case "Queued": 0; case "Planning": 0.25; case "Building": 0.5; case "Reviewing": 0.75; case "AwaitingApproval": 0.5; case "ReadyForReview": 0.5; default: 1 }
    }
    var body: some View {
        ZStack {
            switch status {
            case "Completed":
                Circle().fill(color); Image(systemName: "checkmark").font(NekoFont.scaled(size * 0.5, weight: .bold)).foregroundStyle(.black)
            case "Cancelled":
                Circle().strokeBorder(Color.secondary, lineWidth: 1.4); Image(systemName: "xmark").font(NekoFont.scaled(size * 0.42, weight: .bold)).foregroundStyle(.secondary)
            case "Failed":
                Circle().fill(color); Image(systemName: "exclamationmark").font(NekoFont.scaled(size * 0.5, weight: .bold)).foregroundStyle(.black)
            default:
                Circle().strokeBorder(color, style: StrokeStyle(lineWidth: 1.4, dash: status == "Queued" ? [1.6, 1.6] : []))
                Circle().trim(from: 0, to: progress).stroke(color, style: StrokeStyle(lineWidth: size * 0.36)).rotationEffect(.degrees(-90)).padding(size * 0.3)
            }
        }.frame(width: size, height: size)
            .opacity(isLive && pulse ? 0.55 : 1)
            .onAppear { if isLive { withNekoAnimation(.pulse) { pulse = !reduceMotion } } }
            .accessibilityLabel(friendlyTaskStatus(status))
    }
    private var isLive: Bool { ["Planning", "Building", "Reviewing"].contains(status) }
}

func taskColor(_ status: String) -> Color {
    switch status {
    case "AwaitingApproval": NekoStyle.amber
    case "ReadyForReview": NekoStyle.sky
    case "Completed": NekoStyle.mint
    case "Failed": NekoStyle.coral
    case "Cancelled": .secondary
    default: N.text3
    }
}

func friendlyTaskStatus(_ status: String) -> String {
    switch status {
    case "AwaitingApproval": "Needs approval"
    case "ReadyForReview": "Ready to review"
    case "Queued": "Queued"
    case "Planning": "Making a plan"
    case "Building": "Preparing a fix"
    case "Reviewing": "Checking the result"
    case "Completed": "Completed"
    case "Cancelled": "Cancelled"
    case "Failed": "Stopped"
    default: status
    }
}

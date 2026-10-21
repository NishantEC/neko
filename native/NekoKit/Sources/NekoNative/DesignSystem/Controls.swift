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

/// How many things need you, on a sidebar row or tab. Amber, because needing
/// you is amber's one meaning.
struct NekoBadge: View {
    let count: Int
    var body: some View {
        if count > 0 {
            Text(String(count)).font(NekoFont.of(.small, weight: .bold).monospacedDigit()).foregroundStyle(NekoStyle.onStatus)
                .padding(.horizontal, NekoSpace.xs).frame(height: 18).background(NekoStyle.amber, in: Capsule())
                .accessibilityLabel("\(count) need you")
        }
    }
}

/// A symbol on a quiet sunken tile, for setting rows and empty states.
/// Neutral by default; pass a status `tint` only when the tile means that state.
struct NekoIconTile: View {
    let symbol: String
    var tint: Color = N.text2
    var size: CGFloat = 32
    var body: some View {
        let shape = RoundedRectangle(cornerRadius: NekoRadius.sm, style: .continuous)
        Image(systemName: symbol).font(NekoFont.scaled(size * 0.47, weight: .medium))
            .foregroundStyle(tint).frame(width: size, height: size)
            .background(N.sunken, in: shape)
            .overlay(shape.strokeBorder(N.line))
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
                Circle().fill(color); Image(systemName: "checkmark").font(NekoFont.scaled(size * 0.5, weight: .bold)).foregroundStyle(NekoStyle.onStatus)
            case "Cancelled":
                Circle().strokeBorder(Color.secondary, lineWidth: 1.5); Image(systemName: "xmark").font(NekoFont.scaled(size * 0.42, weight: .bold)).foregroundStyle(.secondary)
            case "Failed":
                Circle().fill(color); Image(systemName: "exclamationmark").font(NekoFont.scaled(size * 0.5, weight: .bold)).foregroundStyle(NekoStyle.onStatus)
            default:
                // A 2pt dash still reads as dashed at 14pt on a 2× display.
                Circle().strokeBorder(color, style: StrokeStyle(lineWidth: 1.5, dash: status == "Queued" ? [2, 2] : []))
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

import AppKit

/// What the menu bar icon says without opening the menu. Needing you
/// outranks working: one is waiting on the user now, the other is not.
enum MenuBarState: Equatable {
    case idle, working, needsYou(Int)

    init(_ summary: StatusSummary) {
        if summary.needsAttention > 0 { self = .needsYou(summary.needsAttention) }
        else if summary.working > 0 { self = .working }
        else { self = .idle }
    }

    /// Read by VoiceOver in place of the picture.
    var accessibilityLabel: String {
        switch self {
        case .idle: "Neko"
        case .working: "Neko, working"
        case .needsYou(let count): "Neko, \(count) \(count == 1 ? "ticket needs" : "tickets need") you"
        }
    }
}

/// Draws the menu bar icon as a template image: the cat, plus a filled dot
/// when something needs you or a ring while work runs. Template images take
/// the menu bar's own tint in light, dark and highlighted states.
enum MenuBarIcon {
    @MainActor static func image(for state: MenuBarState) -> NSImage {
        let size = NSSize(width: 20, height: 16)
        let image = NSImage(size: size, flipped: false) { rect in
            let configuration = NSImage.SymbolConfiguration(pointSize: 14, weight: .regular)
            if let cat = NSImage(systemSymbolName: "cat", accessibilityDescription: nil)?.withSymbolConfiguration(configuration) {
                let catSize = cat.size
                cat.draw(in: NSRect(x: 0, y: (rect.height - catSize.height) / 2, width: catSize.width, height: catSize.height))
            }
            let mark = NSRect(x: rect.width - 6, y: rect.height - 6, width: 5, height: 5)
            switch state {
            case .idle: break
            case .needsYou: NSColor.black.setFill(); NSBezierPath(ovalIn: mark).fill()
            case .working:
                NSColor.black.setStroke()
                let ring = NSBezierPath(ovalIn: mark.insetBy(dx: 0.5, dy: 0.5)); ring.lineWidth = 1; ring.stroke()
            }
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = state.accessibilityLabel
        return image
    }
}

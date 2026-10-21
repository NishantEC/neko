import SwiftUI
import NekoKit

/// What the Mac can take right now, in words: the line under the sidebar.
struct MachineStatus {
    let health: JSONValue
    enum Level { case ok, limited, holding }
    var known: Bool { health != .null }
    var level: Level {
        if !health["holding"].string.isEmpty { return .holding }
        return health["cap"] == .null ? .ok : .limited
    }
    var title: String {
        guard known else { return "Reading the Mac…" }
        switch level {
        case .holding: return "New work is on hold"
        case .limited: return health["night"].bool ? "Night: one task at a time" : "Memory tight: one at a time"
        case .ok: return "Mac OK · load \(Int(health["load"].double.rounded()))"
        }
    }
    var color: Color {
        switch level {
        case .holding: NekoStyle.coral
        case .limited: NekoStyle.amber
        case .ok: NekoStyle.mint
        }
    }
    /// The detail rows, in order, as (label, value).
    var facts: [(String, String)] {
        let h = health
        return [
            ("Load", String(format: "%.1f on %d cores", h["load"].double, h["cores"].int)),
            ("Memory", "\(h["pressure"].string.capitalized) · \(String(format: "%.1f", h["compressed_gb"].double)) GB compressed · swap \(String(format: "%.1f of %.1f", h["swap_used_gb"].double, h["swap_total_gb"].double)) GB"),
            ("Disk", String(format: "%.0f GB free", h["free_disk_gb"].double)),
            ("Cooling", h["kernel_task_cpu"].double >= 50 ? String(format: "macOS is throttling (kernel_task %.0f%%)", h["kernel_task_cpu"].double) : "Normal"),
        ]
    }
}

struct MachineStrip: View {
    /// `MachineHealth` as the daemon reports it.
    let health: JSONValue
    @State private var open = false
    var body: some View {
        let status = MachineStatus(health: health)
        Button { open.toggle() } label: {
            HStack(spacing: NekoSpace.sm) {
                Circle().fill(status.known ? status.color : N.text4).frame(width: 7, height: 7).accessibilityHidden(true)
                Text(status.title).font(NekoFont.meta).foregroundStyle(status.level == .ok ? N.text3 : status.color).lineLimit(1)
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("How much the Mac can take right now")
        .popover(isPresented: $open, arrowEdge: .trailing) { details(status) }
        .accessibilityLabel("Mac health: \(status.title)")
    }

    private func details(_ status: MachineStatus) -> some View {
        VStack(alignment: .leading, spacing: NekoSpace.md) {
            Text(status.title).font(NekoFont.heading)
            if !status.health["holding"].string.isEmpty {
                Text(status.health["holding"].string).foregroundStyle(NekoStyle.coral).fixedSize(horizontal: false, vertical: true)
            } else if status.health["night"].bool {
                Text("01:00–06:00: Neko runs one task at a time, and new work waits while the load is above 12.").foregroundStyle(N.text2).fixedSize(horizontal: false, vertical: true)
            }
            if status.health["slowed"].int > 0 {
                Text("Slowed \(status.health["slowed"].int) build and test \(status.health["slowed"].int == 1 ? "process" : "processes") so the Mac keeps up. Nothing was stopped.")
                    .font(NekoFont.meta).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
            }
            Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: NekoSpace.lg, verticalSpacing: NekoSpace.sm) {
                ForEach(status.facts, id: \.0) { fact in
                    GridRow { Text(fact.0).foregroundStyle(N.text3); Text(fact.1) }
                }
            }
            let agents = status.health["agents"].array.prefix(6)
            if !agents.isEmpty {
                Divider()
                Text("Memory held by agents").font(NekoFont.meta).foregroundStyle(N.text3)
                ForEach(Array(agents.enumerated()), id: \.offset) { _, agent in
                    HStack {
                        Text(agent["name"].string)
                        Spacer(minLength: NekoSpace.lg)
                        Text("\(agent["processes"].int) · \(Self.size(agent["mb"].int))").foregroundStyle(N.text3).monospacedDigit()
                    }
                }
            }
        }
        .font(NekoFont.body)
        .padding(NekoLayout.rowInset)
        .frame(width: 380, alignment: .leading)
    }

    static func size(_ mb: Int) -> String { mb >= 1024 ? String(format: "%.1f GB", Double(mb) / 1024) : "\(mb) MB" }
}

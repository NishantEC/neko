import SwiftUI
import NekoKit

/// All tasks as a tree: Neko on top, the watches and tasks it answers for,
/// and the agent runs under each task. See `AgentTree` for where each link
/// comes from.
struct AgentTreeView: View {
    let tree: AgentTree
    /// Opens a task (a task card, or the task a run belongs to).
    let open: (String) -> Void
    let openHome: () -> Void
    let openWatching: () -> Void
    /// Expanded or collapsed by hand; everything else uses `expandsByDefault`.
    @State private var overrides: [String: Bool] = [:]

    var body: some View {
        ScrollView([.vertical, .horizontal]) {
            VStack(alignment: .leading, spacing: NekoSpace.xl) {
                summary
                AgentTreeBranch(node: tree.root, isExpanded: isExpanded, toggle: toggle, choose: choose)
            }
            .padding(NekoLayout.pageInset)
            .frame(maxWidth: .infinity, alignment: .topLeading)
        }
        .scrollIndicators(.automatic)
    }

    private var summary: some View {
        HStack(spacing: NekoSpace.lg) {
            Label("Agents", systemImage: "sparkles").font(NekoFont.heading)
            Spacer(minLength: NekoSpace.lg)
            Text("\(tree.tasks) \(tree.tasks == 1 ? "task" : "tasks")").foregroundStyle(.secondary)
            Text("\(tree.running) running").foregroundStyle(tree.running > 0 ? NekoStyle.accent : .secondary)
            Text(AgentTreeFormat.cost(tree.costUSD)).foregroundStyle(.primary)
                .help("What runs reported spending. Codex runs report no cost, so they are not counted.")
        }
        .font(NekoFont.meta.monospacedDigit())
        .frame(maxWidth: AgentTreeMetrics.summaryWidth, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    private func isExpanded(_ node: AgentNode) -> Bool {
        overrides[node.id] ?? Self.expandsByDefault(node)
    }
    private func toggle(_ node: AgentNode) {
        withNekoAnimation(.press) { overrides[node.id] = !isExpanded(node) }
    }
    private func choose(_ node: AgentNode) {
        switch node.kind {
        case .neko: openHome()
        case .watch: openWatching()
        case .task, .run: if let id = node.taskID { open(id) }
        }
    }

    /// Neko is always open; a watch or task opens only while an agent below
    /// it is working, so live work is visible and the rest stays one card.
    static func expandsByDefault(_ node: AgentNode) -> Bool {
        switch node.kind {
        case .neko: true
        case .watch, .task: node.descendants.contains { $0.kind == .run && $0.state.isLive }
        case .run: false
        }
    }
}

enum AgentTreeMetrics {
    static let cardWidth: CGFloat = 360
    static let summaryWidth: CGFloat = 760
    static let icon: CGFloat = 28
    static let line: CGFloat = 1.5
    /// Indent of a child level; the connector runs down its left edge.
    static let indent: CGFloat = NekoSpace.xxl
    /// Where the connector meets a child card: the middle of its first line.
    static let elbow: CGFloat = NekoSpace.lg + icon / 2
    /// The connector sits under the centre of the parent's icon.
    static let lineInset: CGFloat = NekoSpace.lg + icon / 2 - line / 2
}

private struct AgentTreeBranch: View {
    let node: AgentNode
    let isExpanded: (AgentNode) -> Bool
    let toggle: (AgentNode) -> Void
    let choose: (AgentNode) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            AgentNodeCard(node: node, expanded: isExpanded(node), toggle: { toggle(node) }, choose: { choose(node) })
            if isExpanded(node) && !node.children.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(node.children.enumerated()), id: \.element.id) { index, child in
                        HStack(alignment: .top, spacing: 0) {
                            Color.clear.frame(width: AgentTreeMetrics.indent)
                            AgentTreeBranch(node: child, isExpanded: isExpanded, toggle: toggle, choose: choose)
                                .padding(.top, NekoSpace.sm)
                        }
                        .background(alignment: .leading) {
                            TreeConnector(last: index == node.children.count - 1, elbow: NekoSpace.sm + AgentTreeMetrics.elbow)
                                .stroke(N.lineStrong, style: StrokeStyle(lineWidth: AgentTreeMetrics.line, lineCap: .round, lineJoin: .round))
                                .frame(width: AgentTreeMetrics.indent)
                                .accessibilityHidden(true)
                        }
                    }
                }
                .padding(.leading, AgentTreeMetrics.lineInset)
            }
        }
    }
}

/// A line down the left edge that turns right into one child card, and
/// carries on down unless this child is the last.
private struct TreeConnector: Shape {
    let last: Bool
    let elbow: CGFloat
    func path(in rect: CGRect) -> Path {
        let radius = min(NekoRadius.sm, rect.width / 2)
        let x = rect.minX
        let y = rect.minY + elbow
        var path = Path()
        path.move(to: CGPoint(x: x, y: rect.minY))
        path.addLine(to: CGPoint(x: x, y: y - radius))
        path.addQuadCurve(to: CGPoint(x: x + radius, y: y), control: CGPoint(x: x, y: y))
        path.addLine(to: CGPoint(x: rect.maxX, y: y))
        if !last {
            path.move(to: CGPoint(x: x, y: y - radius))
            path.addLine(to: CGPoint(x: x, y: rect.maxY))
        }
        return path
    }
}

private struct AgentNodeCard: View {
    let node: AgentNode
    let expanded: Bool
    let toggle: () -> Void
    let choose: () -> Void
    @State private var hover = false

    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.sm) {
            HStack(alignment: .top, spacing: NekoSpace.sm) {
                icon
                VStack(alignment: .leading, spacing: NekoSpace.hair) {
                    Text(node.name).font(NekoFont.heading).lineLimit(2).fixedSize(horizontal: false, vertical: true)
                    if !node.subtitle.isEmpty {
                        Text(node.subtitle).font(NekoFont.meta).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
                Spacer(minLength: NekoSpace.xs)
                AgentStatePill(state: node.state)
            }
            if !node.tools.isEmpty || !node.work.isEmpty {
                HStack(alignment: .firstTextBaseline, spacing: NekoSpace.xs) {
                    ForEach(node.tools, id: \.self) { tool in
                        Text(tool).font(NekoFont.label).foregroundStyle(N.text2)
                            .padding(.horizontal, NekoSpace.xs).padding(.vertical, NekoSpace.hair)
                            .background(N.attachment, in: RoundedRectangle(cornerRadius: NekoRadius.xs, style: .continuous))
                    }
                    if !node.work.isEmpty {
                        Text(node.work).font(NekoFont.meta).foregroundStyle(N.text2).lineLimit(2)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            if !footer.isEmpty || !node.children.isEmpty {
            HStack(spacing: NekoSpace.xs) {
                Text(footer).font(NekoFont.meta.monospacedDigit()).foregroundStyle(.secondary).lineLimit(1)
                Spacer(minLength: NekoSpace.xs)
                if !node.children.isEmpty {
                    Button(action: toggle) {
                        HStack(spacing: NekoSpace.xxs) {
                            Text(String(node.children.count)).monospacedDigit()
                            Image(systemName: expanded ? "chevron.up" : "chevron.down").font(NekoFont.of(.micro, weight: .semibold))
                        }
                        .font(NekoFont.meta).foregroundStyle(.secondary)
                        .padding(.horizontal, NekoSpace.xs).padding(.vertical, NekoSpace.hair)
                        .contentShape(Rectangle())
                    }
                    .nekoButton(.quiet)
                    .help(expanded ? "Hide what it started" : "Show what it started")
                    .accessibilityLabel(expanded ? "Hide \(node.children.count) below" : "Show \(node.children.count) below")
                }
            }
            }
        }
        .padding(NekoSpace.lg)
        .frame(width: AgentTreeMetrics.cardWidth, alignment: .leading)
        .background {
            RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous).fill(N.panel)
                .overlay(RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous).fill(hover ? N.selected : .clear))
        }
        .overlay(RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous).strokeBorder(N.line))
        .opacity(dimmed ? 0.6 : 1)
        .contentShape(RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous))
        .onTapGesture(perform: choose)
        .onHover { hover = $0 }
        .nekoAnimation(.hover, value: hover)
        .help(node.origin.map { "\($0). Click to open." } ?? "Click to open.")
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityAction(named: "Open", choose)
    }

    /// Finished runs step back so the live ones read first.
    private var dimmed: Bool { node.kind == .run && !node.state.isLive }

    private var footer: String {
        var parts: [String] = []
        if let cost = node.costUSD { parts.append(AgentTreeFormat.cost(cost)) }
        if let ms = node.durationMs { parts.append(AgentTreeFormat.duration(ms)) }
        if let origin = node.origin { parts.append(origin) }
        return parts.joined(separator: " · ")
    }

    @ViewBuilder private var icon: some View {
        Group {
            if node.kind == .neko {
                BrandMark(size: AgentTreeMetrics.icon)
            } else {
                Image(systemName: symbol).font(NekoFont.of(.callout, weight: .medium)).foregroundStyle(N.text2)
                    .frame(width: AgentTreeMetrics.icon, height: AgentTreeMetrics.icon)
                    .background(N.attachment, in: RoundedRectangle(cornerRadius: NekoRadius.sm, style: .continuous))
            }
        }
        .accessibilityHidden(true)
    }

    private var symbol: String {
        switch node.kind {
        case .neko: "sparkles"
        case .watch: "binoculars"
        case .task: "checklist"
        case .run:
            switch node.name {
            case "Scout": "map"
            case "Builder": "hammer"
            case "Reviewer": "checkmark.seal"
            case "Recovery": "lifepreserver"
            default: "cpu"
            }
        }
    }
}

private struct AgentStatePill: View {
    let state: AgentNode.State
    var body: some View {
        HStack(spacing: NekoSpace.xxs) {
            if state.isLive {
                ProgressView().controlSize(.mini)
            } else {
                Image(systemName: state.symbol).font(NekoFont.of(.micro, weight: .bold))
            }
            Text(state.title).lineLimit(1)
        }
        .font(NekoFont.label)
        .foregroundStyle(color)
        .padding(.horizontal, NekoSpace.xs).padding(.vertical, NekoSpace.xxs)
        .background(color.opacity(0.12), in: RoundedRectangle(cornerRadius: NekoRadius.sm, style: .continuous))
        .fixedSize()
    }
    private var color: Color {
        switch state {
        case .running: NekoStyle.accent
        case .needsYou, .changesAsked, .failing: NekoStyle.amber
        case .review: NekoStyle.sky
        case .done, .passed, .watching: NekoStyle.mint
        case .failed: NekoStyle.coral
        case .upNext, .stopped, .paused: .secondary
        }
    }
}

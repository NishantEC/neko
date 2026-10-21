import SwiftUI
import NekoKit

/// All tasks as a tree: Neko on top, the watches and tasks it answers for,
/// and the agent runs under each task. See `AgentTree` for where each link
/// comes from. Drawn as an outline (Xcode's navigator, Linear's sub-issues):
/// one dense row per node, reachable and driven from the keyboard.
struct AgentTreeView: View {
    let tree: AgentTree
    /// Opens a task (a task row, or the task a run belongs to).
    let open: (String) -> Void
    let openHome: () -> Void
    let openWatching: () -> Void
    /// Expanded or collapsed by hand; everything else uses `expandsByDefault`.
    @State private var overrides: [String: Bool] = [:]
    /// The row the keyboard is on. Arrows move it, Return opens it.
    @FocusState private var focused: String?

    var body: some View {
        ScrollView(.vertical) {
            VStack(alignment: .leading, spacing: NekoSpace.md) {
                summary
                AgentTreeBranch(node: tree.root, isExpanded: isExpanded, toggle: toggle, choose: choose,
                                focused: $focused, move: move)
            }
            .frame(maxWidth: NekoLayout.pageWidth, alignment: .topLeading)
            .padding(NekoLayout.pageInset)
            .frame(maxWidth: .infinity, alignment: .top)
        }
        .scrollIndicators(.automatic)
        // Tab leaves the tree in one step; arrows move inside it.
        .focusSection()
    }

    private var summary: some View {
        Text(Self.summaryLine(tree))
            .font(NekoFont.meta.monospacedDigit()).foregroundStyle(N.text3)
            .help("What runs reported spending. Codex runs report no cost, so they are not counted.")
            .padding(.leading, AgentTreeMetrics.disclosure + NekoSpace.md)
    }

    /// "12 tasks · 2 running · $3.40 reported".
    static func summaryLine(_ tree: AgentTree) -> String {
        "\(tree.tasks) \(tree.tasks == 1 ? "task" : "tasks") · \(tree.running) running · \(AgentTreeFormat.cost(tree.costUSD)) reported"
    }

    private func isExpanded(_ node: AgentNode) -> Bool {
        overrides[node.id] ?? Self.expandsByDefault(node)
    }
    private func toggle(_ node: AgentNode) {
        guard !node.children.isEmpty else { return }
        withNekoAnimation(.reveal) { overrides[node.id] = !isExpanded(node) }
    }
    private func choose(_ node: AgentNode) {
        switch node.kind {
        case .neko: openHome()
        case .watch: openWatching()
        case .task, .run: if let id = node.taskID { open(id) }
        }
    }
    /// Moves the keyboard to the row above or below, in on-screen order.
    private func move(from node: AgentNode, by step: Int) {
        let order = Self.visibleOrder(tree.root, isExpanded: isExpanded)
        guard let index = order.firstIndex(of: node.id) else { return }
        if order.indices.contains(index + step) { focused = order[index + step] }
    }

    /// Node IDs top to bottom, skipping what is folded away.
    static func visibleOrder(_ node: AgentNode, isExpanded: (AgentNode) -> Bool) -> [String] {
        [node.id] + (isExpanded(node) ? node.children.flatMap { visibleOrder($0, isExpanded: isExpanded) } : [])
    }

    /// Neko is always open; a watch or task opens only while an agent below
    /// it is working, so live work is visible and the rest stays one row.
    static func expandsByDefault(_ node: AgentNode) -> Bool {
        switch node.kind {
        case .neko: true
        case .watch, .task: node.descendants.contains { $0.kind == .run && $0.state.isLive }
        case .run: false
        }
    }

    /// The status colour a node's state wears.
    static func tone(_ state: AgentNode.State) -> NekoTone {
        switch state {
        case .running: .running
        case .needsYou, .changesAsked, .failing: .attention
        case .review: .review
        case .done, .passed, .watching: .done
        case .failed: .failed
        case .upNext, .stopped, .paused: .neutral
        }
    }
}

enum AgentTreeMetrics {
    static let line: CGFloat = 1.5
    /// The disclosure column: a full pointer target.
    static let disclosure: CGFloat = NekoLayout.minimumTarget
    /// Indent of a child level, so a child's chevron sits under its parent's glyph.
    static let indent: CGFloat = NekoSpace.xl
    /// Where the connector meets a child row: the middle of its first line.
    static let elbow: CGFloat = NekoLayout.listRowHeight / 2
    /// The connector runs down the centre of the parent's disclosure column.
    static let lineInset: CGFloat = disclosure / 2 - line / 2
    /// Cost and duration columns, so numbers line up down the tree.
    static let figure: CGFloat = 64
}

private struct AgentTreeBranch: View {
    let node: AgentNode
    let isExpanded: (AgentNode) -> Bool
    let toggle: (AgentNode) -> Void
    let choose: (AgentNode) -> Void
    var focused: FocusState<String?>.Binding
    let move: (AgentNode, Int) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            AgentNodeRow(node: node, expanded: isExpanded(node), toggle: { toggle(node) }, choose: { choose(node) },
                         focused: focused, move: { move(node, $0) })
            if isExpanded(node) && !node.children.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(node.children.enumerated()), id: \.element.id) { index, child in
                        HStack(alignment: .top, spacing: 0) {
                            Color.clear.frame(width: AgentTreeMetrics.indent)
                            AgentTreeBranch(node: child, isExpanded: isExpanded, toggle: toggle, choose: choose, focused: focused, move: move)
                        }
                        .background(alignment: .leading) {
                            TreeConnector(last: index == node.children.count - 1, elbow: AgentTreeMetrics.elbow)
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

/// A line down the left edge that turns right into one child row, and
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

/// One node as an outline row: a disclosure button beside a focusable row.
/// Right arrow opens a branch, left closes it, up and down move, Return
/// opens it (the NSOutlineView conventions). The disclosure is a sibling of
/// the row, so VoiceOver reaches both.
private struct AgentNodeRow: View {
    let node: AgentNode
    let expanded: Bool
    let toggle: () -> Void
    let choose: () -> Void
    var focused: FocusState<String?>.Binding
    let move: (Int) -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            disclosure
            NekoListRow(title: node.name, detail: node.subtitle.isEmpty ? node.origin : node.subtitle,
                        note: node.state.isLive && !node.work.isEmpty ? node.work : nil, dimmed: dimmed) {
                icon
            } trailing: {
                trailing
            }
            .focusable()
            .focused(focused, equals: node.id)
            .onTapGesture { focused.wrappedValue = node.id; choose() }
            .onKeyPress(.return) { choose(); return .handled }
            .onKeyPress(.space) { choose(); return .handled }
            .onKeyPress(.rightArrow) {
                if !node.children.isEmpty && !expanded { toggle() } else { move(1) }
                return .handled
            }
            .onKeyPress(.leftArrow) {
                if expanded && !node.children.isEmpty { toggle() }
                return .handled
            }
            .onKeyPress(.downArrow) { move(1); return .handled }
            .onKeyPress(.upArrow) { move(-1); return .handled }
            .help(node.origin.map { "\($0). Return or click to open." } ?? "Return or click to open.")
            .accessibilityLabel(accessibilityText)
            .accessibilityAddTraits(.isButton)
            .accessibilityAction(named: "Open", choose)
        }
    }

    /// Finished runs step back so the live ones read first.
    private var dimmed: Bool { node.kind == .run && !node.state.isLive }

    @ViewBuilder private var disclosure: some View {
        if node.children.isEmpty {
            Color.clear.frame(width: AgentTreeMetrics.disclosure, height: NekoLayout.listRowHeight).accessibilityHidden(true)
        } else {
            Button(action: toggle) {
                Image(systemName: "chevron.right").font(NekoFont.of(.small, weight: .semibold)).foregroundStyle(N.text3)
                    .rotationEffect(.degrees(expanded ? 90 : 0))
                    .nekoAnimation(.reveal, value: expanded)
                    .frame(width: AgentTreeMetrics.disclosure, height: NekoLayout.listRowHeight)
                    .contentShape(Rectangle())
            }
            .nekoButton(.quiet)
            .help(expanded ? "Hide what it started" : "Show what it started")
            .accessibilityLabel(expanded ? "Hide \(node.children.count) below \(node.name)" : "Show \(node.children.count) below \(node.name)")
        }
    }

    private var trailing: some View {
        HStack(spacing: NekoSpace.sm) {
            ForEach(Array(node.tools.prefix(2)), id: \.self) { NekoTag($0) }
            if node.tools.count > 2 { NekoTag("+\(node.tools.count - 2)") }
            NekoStatusLabel(tone: AgentTreeView.tone(node.state), symbol: node.state.symbol, text: node.state.title, live: node.state.isLive)
            Text(node.costUSD.map(AgentTreeFormat.cost) ?? "")
                .frame(width: AgentTreeMetrics.figure, alignment: .trailing)
            Text(node.durationMs.map(AgentTreeFormat.duration) ?? "")
                .frame(width: AgentTreeMetrics.figure, alignment: .trailing)
        }
        .font(NekoFont.caption.monospacedDigit()).foregroundStyle(N.text3).lineLimit(1)
    }

    private var accessibilityText: String {
        var parts = [node.name]
        if !node.subtitle.isEmpty { parts.append(node.subtitle) }
        parts.append(node.state.title)
        if !node.tools.isEmpty { parts.append(node.tools.joined(separator: ", ")) }
        if let cost = node.costUSD { parts.append(AgentTreeFormat.cost(cost)) }
        if let ms = node.durationMs { parts.append(AgentTreeFormat.duration(ms)) }
        if node.state.isLive && !node.work.isEmpty { parts.append(node.work) }
        if !node.children.isEmpty { parts.append("\(node.children.count) below, \(expanded ? "shown" : "hidden")") }
        return parts.joined(separator: ", ")
    }

    @ViewBuilder private var icon: some View {
        Group {
            if node.kind == .neko {
                BrandMark(size: NekoLayout.listGlyphSlot)
            } else {
                Image(systemName: symbol).font(NekoFont.of(.caption, weight: .medium))
                    .frame(width: NekoLayout.listGlyphSlot, height: NekoLayout.listGlyphSlot)
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

import SwiftUI
import NekoKit

struct TicketsView: View {
    @ObservedObject var model: AppModel
    @State private var selected: String?
    private var includeStopped: Bool { model.includeStoppedAgents }
    @State private var dropTarget: String?
    private var search: String { model.agentSearch }
    @State private var confirmCancel: JSONValue?
    @AppStorage("neko.work.layout") private var layout = "board"
    /// Done starts as a narrow rail, so the columns that need you fit without scrolling.
    @AppStorage("neko.board.doneExpanded") private var doneExpanded = false
    struct Column { let id: String; let title: String; let color: Color; let statuses: [String] }
    /// Columns follow TaskStage, so the board, sidebar and counts agree.
    static let columns: [Column] = [
        Column(id: "approval", title: TaskStage.needsYou.title, color: TaskStage.needsYou.color, statuses: ["AwaitingApproval", "Failed"]),
        Column(id: "review", title: TaskStage.review.title, color: TaskStage.review.color, statuses: ["ReadyForReview"]),
        Column(id: "working", title: TaskStage.running.title, color: TaskStage.running.color, statuses: ["Planning", "Building", "Reviewing"]),
        Column(id: "upnext", title: TaskStage.upNext.title, color: TaskStage.upNext.color, statuses: ["Queued"]),
        Column(id: "done", title: TaskStage.done.title, color: TaskStage.done.color, statuses: ["Completed", "Cancelled"])
    ]
    /// The stage a sidebar group filter names, or nil for all tasks.
    private var stageFilter: TaskStage? { TaskStage(rawValue: model.agentFilter) }
    private func tasks(in column: Column) -> [JSONValue] {
        model.tasks.filter { task in
            (stageFilter.map { TaskStage(status: task["status"].string) == $0 } ?? true)
            && column.statuses.contains(task["status"].string)
            && (includeStopped || !["Failed", "Cancelled"].contains(task["status"].string))
            && (search.isEmpty || TaskText(task: task, snapshot: model.snapshot).title.localizedCaseInsensitiveContains(search) || task["title"].string.localizedCaseInsensitiveContains(search))
        }.sorted { $0["updated_at_ms"].int > $1["updated_at_ms"].int }
    }
    private var workspaceLabel: String { model.selectedWorkspace.flatMap { id in model.workspaces.first { $0.recordID == id }?["name"].string } ?? "All workspaces" }
    private var startsWithoutAsking: Bool { model.snapshot["start_without_approval"].bool }
    private var plansFirst: Bool { model.snapshot["plan_first"].bool }
    private var hasVisibleTasks: Bool { Self.columns.contains { !tasks(in: $0).isEmpty } }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if layout == "list" { list }
            else if layout == "tree" { tree }
            else if hasVisibleTasks { board }
            else { emptyState.frame(maxWidth: .infinity, maxHeight: .infinity) }
        }
        .navigationTitle(stageFilter?.title ?? "All tasks")
        .navigationSubtitle(Self.subtitle(workspace: workspaceLabel, visible: visibleTasks.count, total: model.tasks.count))
        .toolbar { listToolbar }
        .onChange(of: model.selectedWorkspace) { _, _ in selected = nil }
        .confirmationDialog("Cancel “\(confirmCancel?["title"].string ?? "")”?", isPresented: Binding(get: { confirmCancel != nil }, set: { if !$0 { confirmCancel = nil } })) {
            Button("Cancel task", role: .destructive) {
                if let task = confirmCancel, let done = Self.columns.first(where: { $0.id == "done" }) { run("CancelTask", task, to: done) }
                confirmCancel = nil
            }
            Button("Keep working", role: .cancel) { confirmCancel = nil }
        } message: {
            Text("It hasn’t been reviewed yet, so moving it to Done stops the work. Its files and worktree stay on disk.")
        }
        .onChange(of: selected) { _, id in if let id { model.openAgent(id, from: .tickets) } }

    }

    @ToolbarContentBuilder private var listToolbar: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            NekoSearchField(title: "Search tasks", text: $model.agentSearch).frame(width: 180)
        }.withoutSharedBackground()
        ToolbarItem(placement: .primaryAction) {
            GlassSegmented(selection: $layout, options: [
                .init(value: "list", title: "List", symbol: "list.bullet", help: "Show tasks as a list"),
                .init(value: "board", title: "Board", symbol: "rectangle.split.3x1", help: "Show tasks as a board"),
                .init(value: "tree", title: "Tree", symbol: "point.3.connected.trianglepath.dotted", help: "Show which agent started each task")
            ], iconOnly: true)
        }.withoutSharedBackground()
        ToolbarItem(placement: .primaryAction) {
            Menu {
                Toggle("Plan before working", isOn: Binding(get: { plansFirst }, set: { value in
                    Task { await model.workbench(.command("SetPlanFirst", ["enabled": .bool(value)])) }
                }))
                Text(plansFirst ? "Every task first writes a read-only plan for you to approve." : "Tasks start working right away. The agent stops for your approval only on critical work.")
                if plansFirst {
                    Toggle("Start tasks without asking", isOn: Binding(get: { startsWithoutAsking }, set: { value in
                        Task { await model.workbench(.command("SetStartWithoutApproval", ["enabled": .bool(value)])) }
                    }))
                    Text("Planned chat and manual tasks begin working without waiting for approval.")
                }
                Divider()
                Toggle("Show failed and cancelled", isOn: $model.includeStoppedAgents)
                Button("Clear finished tasks") { Task { await model.workbench(.command("ClearFinishedTasks", ["workspace_id": model.selectedWorkspace.map { .string($0) } ?? .null])) } }
                    .disabled(!model.tasks.contains { ["Completed", "Cancelled"].contains($0["status"].string) })
            } label: {
                Image(systemName: "ellipsis.circle")
                    .frame(width: NekoControlMetrics.height(), height: NekoControlMetrics.height())
                    .contentShape(Rectangle())
            }
                .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
                .help("Work options")
        }
    }

    // MARK: Tree

    /// Every task the filters let through, whatever its column.
    private var visibleTasks: [JSONValue] { Self.columns.flatMap { tasks(in: $0) } }
    private var tree: some View {
        // Watches with no tasks would only add noise to a search or a stage filter.
        let filtered = !search.isEmpty || stageFilter != nil
        let built = AgentTree.build(snapshot: model.snapshot, tasks: visibleTasks, workspace: model.selectedWorkspace,
                                    showIdleWatches: !filtered, now: Int(Date().timeIntervalSince1970 * 1000))
        return AgentTreeView(tree: built,
                             open: { model.openAgent($0, from: .tickets) },
                             openHome: { model.requestedPage = .home },
                             openWatching: { model.requestedPage = .watching })
            .overlay { if built.root.children.isEmpty { emptyState } }
    }

    // MARK: List

    /// "athena · 4 of 12 tasks" while filters hide some, "athena · 12 tasks" otherwise.
    static func subtitle(workspace: String, visible: Int, total: Int) -> String {
        visible == total ? "\(workspace) · \(taskCount(total))" : "\(workspace) · \(visible) of \(taskCount(total))"
    }
    /// "1 task", "3 tasks", inflected by Foundation rather than `== 1 ?`.
    static func taskCount(_ count: Int) -> String {
        String(AttributedString(localized: "^[\(count) task](inflect: true)").characters)
    }

    private var list: some View {
        let shown = Self.columns.filter { !tasks(in: $0).isEmpty }
        return List(selection: Binding(get: { selected }, set: { selected = $0 })) {
            ForEach(shown, id: \.id) { column in
                let items = tasks(in: column)
                Section {
                    ForEach(items, id: \.recordID) { task in
                        listRow(task).tag(task.recordID)
                            .listRowInsets(EdgeInsets(top: 0, leading: NekoSpace.xs, bottom: 0, trailing: NekoSpace.xs))
                            .listRowSeparator(.hidden)
                    }
                } header: {
                    NekoListHeader(title: column.title, tone: NekoTone(stage: stage(column)), count: items.count,
                                   spaced: column.id != shown.first?.id)
                }
            }
        }
        .listStyle(.inset)
        .environment(\.defaultMinListRowHeight, NekoLayout.listRowHeight)
        .scrollContentBackground(.hidden)
        .contentMargins(.horizontal, NekoLayout.pageInset, for: .scrollContent)
        .contentMargins(.vertical, NekoSpace.md, for: .scrollContent)
        .frame(maxWidth: NekoLayout.pageWidth)
        .frame(maxWidth: .infinity)
        .overlay { if !hasVisibleTasks { emptyState } }
    }
    @ViewBuilder private var emptyState: some View {
        if search.isEmpty {
            NekoEmptyState(reason: .nothingYet, title: "No tasks yet", message: "Describe what you want done in Home and Neko files it here.", block: true) {
                Button("New task") { model.agentID = nil; model.requestedPage = .home; ComposerFocus.request() }.nekoButton(.primary)
                    .help("New task (⌘N)")
            }
        } else {
            NekoEmptyState(reason: .noMatches(search), block: true) {
                Button("Clear search") { model.agentSearch = "" }
            }
        }
    }
    private func stage(_ column: Column) -> TaskStage {
        TaskStage(status: column.statuses.first ?? "")
    }
    /// The line under a row's title: only for tasks blocked on you.
    private func note(_ task: JSONValue) -> String? {
        let status = task["status"].string
        guard TaskStage(status: status) == .needsYou else { return nil }
        return status == "Failed" ? TicketPresentation.stopReason(task) : TicketPresentation.waitingReason(task)
    }
    private func listRow(_ task: JSONValue) -> some View {
        let status = task["status"].string
        let text = TaskText(task: task, snapshot: model.snapshot)
        return NekoListRow(title: text.title, detail: text.detail, note: note(task), meta: relative(task["updated_at_ms"].int)) {
            StatusGlyph(status: status, size: 14)
        } trailing: {
            // Which workspace is only news when every workspace is showing.
            if model.selectedWorkspace == nil { NekoTag(workspaceName(task)) }
        }
        .contextMenu { moveMenu(task) }
    }

    // MARK: Board

    /// Columns drawn at full width; Done folds to a rail until opened.
    private var expandedColumns: [Column] { Self.columns.filter { $0.id != "done" || doneExpanded } }

    /// Shares the width between expanded columns, within the column limits.
    static func columnWidth(available: CGFloat, columns: Int, rail: Bool) -> CGFloat {
        let count = CGFloat(max(columns, 1))
        let used = 2 * NekoLayout.pageInset + (count - 1) * NekoSpace.lg + (rail ? NekoLayout.boardRail + NekoSpace.lg : 0)
        return min(max((available - used) / count, NekoLayout.boardColumnMin), NekoLayout.boardColumnMax)
    }

    private var board: some View {
        GeometryReader { geometry in
            let width = Self.columnWidth(available: geometry.size.width, columns: expandedColumns.count, rail: !doneExpanded)
            ScrollView(.horizontal) {
                HStack(alignment: .top, spacing: NekoSpace.lg) {
                    ForEach(expandedColumns, id: \.id) { column in
                        boardColumn(column, width: width)
                    }
                    if !doneExpanded, let done = Self.columns.first(where: { $0.id == "done" }) { doneRail(done) }
                }
                .padding(.horizontal, NekoLayout.pageInset).padding(.vertical, NekoLayout.sectionGap)
                .frame(minHeight: geometry.size.height, alignment: .top)
            }
            .scrollIndicators(.automatic)
        }
    }
    private func boardColumn(_ column: Column, width: CGFloat) -> some View {
        let items = tasks(in: column)
        return VStack(alignment: .leading, spacing: NekoSpace.sm) {
            NekoListHeader(title: column.title, tone: NekoTone(stage: stage(column)), count: items.count) {
                if column.id == "done" {
                    Button { withNekoAnimation(.reveal) { doneExpanded = false } } label: {
                        Image(systemName: "chevron.right").frame(minWidth: NekoLayout.minimumTarget, minHeight: NekoLayout.minimumTarget)
                            .contentShape(Rectangle())
                    }
                    .nekoButton(.quiet).help("Fold Done into a rail").accessibilityLabel("Hide Done column")
                }
            }
            .padding(.horizontal, NekoSpace.xs)
            ScrollView(.vertical) {
                LazyVStack(spacing: NekoSpace.sm) {
                    ForEach(items, id: \.recordID) { task in
                        card(task)
                            .draggable(task.recordID) { card(task).frame(width: width).opacity(0.9) }
                    }
                    if items.isEmpty {
                        Text(dropTarget == column.id ? "Drop to move here" : stage(column).emptyLine)
                            .font(NekoFont.meta).foregroundStyle(N.text3).multilineTextAlignment(.center)
                            .frame(maxWidth: .infinity, minHeight: NekoLayout.listRowTallHeight * 2)
                    }
                }.padding(.horizontal, NekoSpace.hair).padding(.bottom, NekoSpace.md)
            }.scrollIndicators(.automatic)
        }
        .frame(width: width).frame(maxHeight: .infinity, alignment: .top)
        .modifier(BoardDropTarget(column: column, dropTarget: $dropTarget, move: move))
    }
    /// Done, folded: its count and a way to open it. Still a drop target.
    private func doneRail(_ column: Column) -> some View {
        let count = tasks(in: column).count
        return Button { withNekoAnimation(.reveal) { doneExpanded = true } } label: {
            VStack(spacing: NekoSpace.sm) {
                Image(systemName: "chevron.left").font(NekoFont.of(.small, weight: .semibold)).foregroundStyle(N.text3)
                StatusGlyph(status: "Completed", size: 14).accessibilityHidden(true)
                Text(String(count)).font(NekoFont.caption.monospacedDigit()).foregroundStyle(N.text3)
            }
            .padding(.vertical, NekoSpace.sm)
            .frame(width: NekoLayout.boardRail).frame(maxHeight: .infinity, alignment: .top)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Show Done")
        .accessibilityLabel("Done, \(Self.taskCount(count)). Show column")
        .modifier(BoardDropTarget(column: column, dropTarget: $dropTarget, move: move))
    }
    private func card(_ task: JSONValue) -> some View {
        let status = task["status"].string
        let text = TaskText(task: task, snapshot: model.snapshot)
        return Button { selected = task.recordID } label: {
            TicketCard(id: text.sourceTitle ?? text.title, title: text.title, detail: text.detail, status: status, updated: relative(task["updated_at_ms"].int), highlighted: selected == task.recordID, note: status == "Failed" ? TicketPresentation.stopReason(task) : TicketPresentation.waitingReason(task))
        }
        .buttonStyle(.plain)
        .contextMenu { moveMenu(task) }
        // Starts with the visible title, so Voice Control can say what it sees.
        .accessibilityLabel("\(text.title), \(workspaceName(task)), \(friendlyTaskStatus(status))")
        .accessibilityHint("Open the task. Drag to another column to move it.")
    }

    // MARK: Moving tickets

    /// The command a move asks for, or why the move can't happen.
    static func moveCommand(from status: String, to column: String) -> (command: String?, reason: String?) {
        switch (column, status) {
        case ("working", "AwaitingApproval"), ("working", "Failed"), ("working", "Cancelled"),
             ("upnext", "AwaitingApproval"), ("upnext", "Failed"), ("upnext", "Cancelled"): return ("StartTask", nil)
        case ("done", "ReadyForReview"): return ("CompleteTask", nil)
        case ("done", "AwaitingApproval"), ("done", "Queued"), ("done", "Planning"), ("done", "Building"), ("done", "Reviewing"):
            return ("CancelTask", nil)
        case ("approval", "Queued"), ("approval", "Planning"), ("approval", "Building"), ("approval", "Reviewing"):
            return (nil, "Work that has started can’t go back to waiting. Cancel it, or let it finish.")
        case ("review", _): return (nil, "Neko moves a task to review once its work and checks are done.")
        case ("working", "ReadyForReview"): return (nil, "Add a note on the task to send it back with changes.")
        default: return (nil, nil)
        }
    }
    @discardableResult private func move(_ id: String, to column: Column) -> Bool {
        dropTarget = nil
        guard let task = model.tasks.first(where: { $0.recordID == id }) else { return false }
        let status = task["status"].string
        if column.statuses.contains(status) { return false }
        let plan = Self.moveCommand(from: status, to: column.id)
        if let reason = plan.reason { model.notice = reason; return false }
        guard let command = plan.command else { return false }
        if command == "CancelTask" { confirmCancel = task; return true }
        run(command, task, to: column)
        return true
    }
    private func run(_ command: String, _ task: JSONValue, to column: Column) {
        let id = task.recordID
        let title = task["title"].string
        Task {
            if await model.workbench(.command(command, ["task_id": .string(id)])) {
                switch command {
                case "CancelTask": model.notice = "Cancelled “\(title)”. Its files and worktree stay on disk."
                case "StartTask":
                    model.notice = "Started “\(title)”. Neko finds its repository, plans, then builds."
                    await reportIfItStops(id, title: title)
                default: model.notice = "Moved “\(title)” to \(column.title)."
                }
            }
        }
    }
    /// A started ticket that fails within a few seconds would otherwise just
    /// jump back to its old column. Say why instead.
    private func reportIfItStops(_ id: String, title: String) async {
        for _ in 0..<30 {
            try? await Task.sleep(for: .seconds(1))
            guard let task = model.tasks.first(where: { $0.recordID == id }) else { return }
            switch task["status"].string {
            case "Failed":
                model.notice = "“\(title)” stopped: \(TicketPresentation.stopReason(task))"
                selected = id
                return
            case "Building", "Reviewing", "ReadyForReview", "Completed": return
            default: continue
            }
        }
    }
    @ViewBuilder private func moveMenu(_ task: JSONValue) -> some View {
        Button("Open") { selected = task.recordID }
        if task["status"].string == "Queued" {
            Button("Run next") { Task { await model.workbench(.command("PrioritizeTask", ["task_id": .string(task.recordID)])) } }
        }
        Divider()
        ForEach(Self.columns, id: \.id) { column in
            let status = task["status"].string
            let plan = Self.moveCommand(from: status, to: column.id)
            if !column.statuses.contains(status), plan.command != nil {
                Button(column.id == "done" && plan.command == "CancelTask" ? "Cancel task" : "Move to \(column.title)") { move(task.recordID, to: column) }
            }
        }
    }

    private func workspaceName(_ task: JSONValue) -> String { model.workspaces.first { $0.recordID == task["workspace_id"].string }?["name"].string ?? "Workspace" }
    private func relative(_ ms: Int) -> String {
        guard ms > 0 else { return "" }
        return Date(timeIntervalSince1970: Double(ms) / 1000).formatted(.relative(presentation: .named, unitsStyle: .abbreviated))
    }
}

/// A board column (or the folded Done rail) that accepts a dragged task.
/// Plain at rest; only the one under a dragged card lights up.
private struct BoardDropTarget: ViewModifier {
    let column: TicketsView.Column
    @Binding var dropTarget: String?
    let move: (String, TicketsView.Column) -> Bool
    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous)
        content
            .background(dropTarget == column.id ? column.color.opacity(0.08) : .clear, in: shape)
            .overlay(shape.strokeBorder(dropTarget == column.id ? column.color.opacity(0.5) : .clear))
            .dropDestination(for: String.self) { ids, _ in
                guard let id = ids.first else { return false }
                return move(id, column)
            } isTargeted: { inside in
                withNekoAnimation(.press) { dropTarget = inside ? column.id : (dropTarget == column.id ? nil : dropTarget) }
            }
    }
}

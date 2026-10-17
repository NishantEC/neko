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
            else if hasVisibleTasks { board }
            else { emptyState.frame(maxWidth: .infinity, maxHeight: .infinity) }
        }
        .navigationTitle(stageFilter?.title ?? "All tasks")
        .navigationSubtitle("\(workspaceLabel) · \(model.tasks.count) \(model.tasks.count == 1 ? "task" : "tasks")")
        .toolbar { listToolbar }
        .onChange(of: model.selectedWorkspace) { _, _ in selected = nil }
        .confirmationDialog("Cancel “\(confirmCancel?["title"].string ?? "")”?", isPresented: Binding(get: { confirmCancel != nil }, set: { if !$0 { confirmCancel = nil } })) {
            Button("Cancel task", role: .destructive) {
                if let task = confirmCancel, let done = Self.columns.first(where: { $0.id == "done" }) { run("CancelTask", task, to: done) }
                confirmCancel = nil
            }
            Button("Keep Working", role: .cancel) { confirmCancel = nil }
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
                .init(value: "board", title: "Board", symbol: "rectangle.split.3x1", help: "Show tasks as a board")
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

    // MARK: List

    private var list: some View {
        List(selection: Binding(get: { selected }, set: { selected = $0 })) {
            ForEach(Self.columns, id: \.id) { column in
                let items = tasks(in: column)
                if !items.isEmpty {
                    Section {
                        ForEach(items, id: \.recordID) { task in
                            listRow(task).tag(task.recordID)
                                .listRowInsets(EdgeInsets(top: 14, leading: NekoLayout.rowInset, bottom: 14, trailing: NekoLayout.rowInset))
                                .listRowSeparator(.hidden)
                        }
                    } header: {
                        HStack(spacing: 8) {
                            Text(column.title).font(NekoFont.heading)
                            Text(String(items.count)).font(NekoFont.meta.monospacedDigit()).foregroundStyle(.secondary)
                        }
                        .padding(.top, 16).padding(.bottom, 8)
                        .accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
                    }
                }
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .contentMargins(.horizontal, NekoLayout.pageInset, for: .scrollContent)
        .contentMargins(.vertical, 12, for: .scrollContent)
        .frame(maxWidth: NekoLayout.pageWidth)
        .frame(maxWidth: .infinity)
        .overlay { if !hasVisibleTasks { emptyState } }
    }
    private var emptyState: some View {
        ContentUnavailableView {
            Label(search.isEmpty ? "No tasks here yet" : "No matching tasks", systemImage: "bubble.left.and.bubble.right")
        } description: {
            Text(search.isEmpty ? "Ask Neko for work in Home. Tasks that match will appear here." : "No task matches “\(search)”.")
        } actions: {
            if search.isEmpty {
                Button("Open Home") { model.requestedPage = .home }
            } else {
                Button("Clear search") { model.agentSearch = "" }
            }
        }
        .padding(NekoLayout.pageInset)
    }
    private func listRow(_ task: JSONValue) -> some View {
        let status = task["status"].string
        let note = status == "Failed" ? TicketPresentation.stopReason(task) : TicketPresentation.waitingReason(task)
        return HStack(alignment: .top, spacing: 16) {
            StatusGlyph(status: status, size: 16).padding(.top, 2).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 8) {
                Text(task["title"].string).font(NekoFont.heading).lineSpacing(3).lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                Text("\(workspaceName(task)) · \(friendlyTaskStatus(status))").font(NekoFont.meta).foregroundStyle(.secondary).lineLimit(1)
                if let note {
                    Text(note).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 16)
            Text(relative(task["updated_at_ms"].int)).font(NekoFont.meta.monospacedDigit()).foregroundStyle(.secondary)
                .fixedSize().padding(.top, 2)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .contextMenu { moveMenu(task) }
    }

    // MARK: Board

    private var board: some View {
        ScrollView(.horizontal) {
            HStack(alignment: .top, spacing: NekoLayout.sectionGap) {
                ForEach(Self.columns, id: \.id) { column in
                    let items = tasks(in: column)
                    VStack(alignment: .leading, spacing: 18) {
                        HStack(spacing: 8) {
                            Circle().fill(column.color).frame(width: 6, height: 6).accessibilityHidden(true)
                            Text(column.title).font(NekoFont.heading)
                            Text(String(items.count)).font(NekoFont.meta.monospacedDigit()).foregroundStyle(.secondary)
                            Spacer(minLength: 0)
                        }.padding(.horizontal, 6).frame(minHeight: 32)
                        .accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
                        ScrollView(.vertical) {
                            LazyVStack(spacing: 14) {
                                ForEach(items, id: \.recordID) { task in
                                    card(task)
                                        .draggable(task.recordID) { card(task).frame(width: 300).opacity(0.9) }
                                }
                                if items.isEmpty {
                                    Text(dropTarget == column.id ? "Drop to move here" : "Nothing here")
                                        .font(NekoFont.meta).foregroundStyle(.secondary).frame(maxWidth: .infinity, minHeight: 112)
                                }
                            }.padding(.horizontal, 2).padding(.bottom, 12)
                        }.scrollIndicators(.never)
                    }
                    .frame(width: 300).frame(maxHeight: .infinity, alignment: .top)
                    // Columns are plain; only the one under a dragged card lights up.
                    .background(dropTarget == column.id ? column.color.opacity(0.08) : .clear, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous).strokeBorder(dropTarget == column.id ? column.color.opacity(0.5) : .clear))
                    .dropDestination(for: String.self) { ids, _ in
                        guard let id = ids.first else { return false }
                        return move(id, to: column)
                    } isTargeted: { inside in
                        withNekoAnimation(.press) { dropTarget = inside ? column.id : (dropTarget == column.id ? nil : dropTarget) }
                    }
                }
            }.padding(.horizontal, NekoLayout.pageInset).padding(.vertical, NekoLayout.sectionGap)
        }.scrollIndicators(.never)
    }
    private func card(_ task: JSONValue) -> some View {
        let status = task["status"].string
        return Button { selected = task.recordID } label: {
            let text = TaskText(task: task, snapshot: model.snapshot)
            TicketCard(id: text.sourceTitle ?? text.title, title: text.title, detail: text.detail, status: status, updated: relative(task["updated_at_ms"].int), highlighted: selected == task.recordID, note: status == "Failed" ? TicketPresentation.stopReason(task) : TicketPresentation.waitingReason(task))
        }
        .buttonStyle(.plain)
        .contextMenu { moveMenu(task) }
        .accessibilityLabel("\(task["title"].string), \(workspaceName(task)), \(friendlyTaskStatus(status))")
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

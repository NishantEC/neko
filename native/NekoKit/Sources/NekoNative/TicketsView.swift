import SwiftUI
import NekoKit

enum TicketPresentation {
    /// The planner's question for a ticket waiting on you, if it asked one.
    static func waitingReason(_ task: JSONValue) -> String? {
        guard task["status"].string == "AwaitingApproval" else { return nil }
        let decision = task["supervision"]
        if decision["action"].string == "AskUser" { return "Needs your input: " + decision["reason"].string }
        if decision["action"].string == "Skip" { return "Nothing to build: " + decision["reason"].string }
        if let event = task["events"].array.last(where: { ["supervisor", "coordinator"].contains($0["role"].string) }) {
            let message = event["message"].string
            for prefix in ["Needs your input before building: ", "Nothing to build: "] where message.hasPrefix(prefix) {
                return (prefix.hasPrefix("Needs") ? "Needs your input: " : "Nothing to build: ") + message.dropFirst(prefix.count)
            }
        }
        return nil
    }
    /// The newest daemon message on a stopped ticket, in one readable line.
    static func stopReason(_ task: JSONValue) -> String {
        let message = task["events"].array.reversed().first { !["user", "note"].contains($0["role"].string) }?["message"].string ?? ""
        let line = message.trimmingCharacters(in: .whitespacesAndNewlines)
        if line.contains("not a git repository") {
            return "This ticket isn’t linked to a Git repository yet, so Neko couldn’t make its working copy."
        }
        if line.contains("daemon restart") { return "Stopped when Neko restarted. Start it again to continue." }
        if line.hasPrefix("Independent verification failed:") {
            return "Review found problems: " + line.dropFirst("Independent verification failed:".count).trimmingCharacters(in: .whitespaces).prefix(380)
        }
        if line.hasPrefix("Verifier returned a malformed verdict") { return "The reviewer’s answer couldn’t be read. Start it again to re-review." }
        return line.isEmpty ? "Neko stopped without a reason. Start it again to retry." : String(line.prefix(400))
    }
    struct Review: Decodable {
        let passed: Bool
        let findings: [String]
        let files: [String]
        let tests: [String]
        let summary: String
        private enum CodingKeys: String, CodingKey { case passed, findings, files, tests, summary }
        /// Findings arrive either as plain strings or as {severity, path, finding}
        /// objects; both render as one readable line.
        struct Finding: Decodable {
            let text: String
            init(from decoder: Decoder) throws {
                if let plain = try? decoder.singleValueContainer().decode(String.self) { text = plain; return }
                let value = try JSONValue(from: decoder)
                let severity = value["severity"].string, path = value["path"].string
                let body = value["finding"].string.isEmpty ? value["message"].string : value["finding"].string
                text = [severity.isEmpty ? nil : severity.capitalized, path.isEmpty ? nil : (path as NSString).lastPathComponent].compactMap { $0 }.joined(separator: " · ") + (severity.isEmpty && path.isEmpty ? "" : ": ") + body
            }
        }
        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            passed = try c.decode(Bool.self, forKey: .passed)
            // Every field is required: a malformed reply must never read as a pass.
            findings = try c.decode([Finding].self, forKey: .findings).map { $0.text }
            files = try c.decode([String].self, forKey: .files)
            tests = try c.decode([String].self, forKey: .tests)
            summary = try c.decode(String.self, forKey: .summary)
        }
    }
    static func review(_ result: String) -> (body: String, verdict: Review?) {
        guard let range = result.range(of: "\n\nIndependent review:\n", options: .backwards),
              let data = String(result[range.upperBound...]).data(using: .utf8),
              let verdict = try? JSONDecoder().decode(Review.self, from: data) else { return (result, nil) }
        return (String(result[..<range.lowerBound]), verdict)
    }
    /// Saved plans and results survive replies/retries. Only the ticket's current
    /// phase decides whether they belong at the latest end or in previous work.
    struct Conversation {
        let foldsHistory: Bool
        let showsPlan: Bool
        let planTitle: String
        let outcome: Outcome
    }
    enum Outcome: Equatable {
        case ready, completed, reviewing, stopped, previous

        var belongsInHistory: Bool { self == .previous }
        var reviewIsCurrent: Bool { self == .ready || self == .completed }
        var title: String {
            switch self {
            case .ready, .completed: "Result"
            case .reviewing: "Result under review"
            case .stopped: "Last recorded result"
            case .previous: "Previous result"
            }
        }
        var note: String {
            switch self {
            case .ready: "Ready for your review"
            case .completed: "Ticket completed"
            case .reviewing: "Independent review is in progress. This output has not cleared the current review."
            case .stopped: "Work stopped. This saved output may be from an earlier attempt."
            case .previous: "Saved from earlier work; it does not describe the current attempt."
            }
        }
    }
    static func conversation(_ ticket: JSONValue) -> Conversation {
        let status = ticket["status"].string
        let outcome: Outcome
        switch status {
        case "ReadyForReview": outcome = .ready
        case "Completed": outcome = .completed
        case "Reviewing": outcome = .reviewing
        case "Failed", "Cancelled": outcome = .stopped
        default: outcome = .previous
        }
        return Conversation(
            foldsHistory: ["ReadyForReview", "Completed", "Failed", "Cancelled"].contains(status),
            showsPlan: ["Planning", "AwaitingApproval"].contains(status)
                && !ticket["plan"].string.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            planTitle: status == "Planning" ? "Saved plan · being revised" : "Plan",
            outcome: outcome
        )
    }
    static func canProposeSplit(_ ticket: JSONValue, splits: [JSONValue]) -> Bool {
        ticket["status"].string == "AwaitingApproval" && !splits.contains { split in
            split["parent_id"] == ticket["id"] || split["subtasks"].array.contains { $0["task_id"] == ticket["id"] }
        }
    }
}

struct TicketsView: View {
    @ObservedObject var model: AppModel
    @State private var selected: String?
    private var includeStopped: Bool { model.includeStoppedAgents }
    @State private var dropTarget: String?
    private var search: String { model.agentSearch }
    @State private var confirmCancel: JSONValue?
    @AppStorage("neko.work.layout") private var layout = "board"
    struct Column { let id: String; let title: String; let color: Color; let statuses: [String] }
    static let columns: [Column] = [
        Column(id: "approval", title: "Needs you", color: NekoStyle.amber, statuses: ["AwaitingApproval", "Failed"]),
        Column(id: "working", title: "Working", color: NekoStyle.accent, statuses: ["Queued", "Planning", "Building", "Reviewing"]),
        Column(id: "review", title: "Ready to review", color: NekoStyle.sky, statuses: ["ReadyForReview"]),
        Column(id: "done", title: "Done", color: NekoStyle.mint, statuses: ["Completed", "Cancelled"])
    ]
    private func tasks(in column: Column) -> [JSONValue] {
        model.tasks.filter { task in
            AgentSidebarGroup.matches(model.agentFilter, task: task)
            && column.statuses.contains(task["status"].string)
            && (includeStopped || !["Failed", "Cancelled"].contains(task["status"].string))
            && (search.isEmpty || task["title"].string.localizedCaseInsensitiveContains(search))
        }.sorted { $0["updated_at_ms"].int > $1["updated_at_ms"].int }
    }
    private var workspaceLabel: String { model.selectedWorkspace.flatMap { id in model.workspaces.first { $0.recordID == id }?["name"].string } ?? "All workspaces" }
    private var startsWithoutAsking: Bool { model.snapshot["start_without_approval"].bool }
    private var hasVisibleTasks: Bool { Self.columns.contains { !tasks(in: $0).isEmpty } }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if layout == "list" { list }
            else if hasVisibleTasks { board }
            else { emptyState.frame(maxWidth: .infinity, maxHeight: .infinity) }
        }
        .navigationTitle(model.agentFilter == "all" ? "All agents" : (AgentSidebarGroup(rawValue: model.agentFilter)?.title ?? "All agents"))
        .navigationSubtitle("\(workspaceLabel) · \(model.tasks.count) \(model.tasks.count == 1 ? "ticket" : "tickets")\(startsWithoutAsking ? " · starts without asking" : "")")
        .toolbar { listToolbar }
        .onChange(of: model.selectedWorkspace) { _, _ in selected = nil }
        .confirmationDialog("Cancel “\(confirmCancel?["title"].string ?? "")”?", isPresented: Binding(get: { confirmCancel != nil }, set: { if !$0 { confirmCancel = nil } })) {
            Button("Cancel Ticket", role: .destructive) {
                if let task = confirmCancel { run("CancelTask", task, to: Self.columns[3]) }
                confirmCancel = nil
            }
            Button("Keep Working", role: .cancel) { confirmCancel = nil }
        } message: {
            Text("It hasn’t been reviewed yet, so moving it to Done stops the work. Its files and worktree stay on disk.")
        }
        .onChange(of: selected) { _, id in if let id { model.openAgent(id, from: "Tickets") } }

    }

    @ToolbarContentBuilder private var listToolbar: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            NekoSearchField(title: "Search tickets", text: $model.agentSearch).frame(width: 180)
        }.withoutSharedBackground()
        ToolbarItem(placement: .primaryAction) {
            GlassSegmented(selection: $layout, options: [
                .init(value: "list", title: "List", symbol: "list.bullet", help: "Show tickets as a list"),
                .init(value: "board", title: "Board", symbol: "rectangle.split.3x1", help: "Show tickets as a board")
            ], iconOnly: true)
        }.withoutSharedBackground()
        ToolbarItem(placement: .primaryAction) {
            Menu {
                Toggle("Start tickets without asking", isOn: Binding(get: { startsWithoutAsking }, set: { value in
                    Task { await model.workbench(.command("SetStartWithoutApproval", ["enabled": .bool(value)])) }
                }))
                Text("New tickets from chat begin working once planned. Watched sources keep their own rules. Nothing is pushed without you.")
                Divider()
                Toggle("Show failed and cancelled", isOn: $model.includeStoppedAgents)
                Button("Clear finished tickets") { Task { await model.workbench(.command("ClearFinishedTasks", ["workspace_id": model.selectedWorkspace.map { .string($0) } ?? .null])) } }
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
            Label(search.isEmpty ? "No agents here yet" : "No matching agents", systemImage: "bubble.left.and.bubble.right")
        } description: {
            Text(search.isEmpty ? "Ask Neko for work in Home. Agents matching this view will appear here." : "No ticket matches “\(search)”.")
        } actions: {
            if search.isEmpty {
                Button("Open Home") { NotificationCenter.default.post(name: .nekoNavigate, object: "Home") }
            } else {
                Button("Clear search") { model.agentSearch = "" }
            }
        }
        .padding(NekoLayout.pageInset)
    }
    private func listRow(_ task: JSONValue) -> some View {
        let status = task["status"].string
        let note = status == "Failed" ? TicketPresentation.stopReason(task) : TicketPresentation.waitingReason(task)
        return HStack(alignment: .top, spacing: 14) {
            StatusGlyph(status: status, size: 16).padding(.top, 2).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 7) {
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
                    .background(dropTarget == column.id ? column.color.opacity(0.08) : .clear, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(dropTarget == column.id ? column.color.opacity(0.5) : .clear))
                    .dropDestination(for: String.self) { ids, _ in
                        guard let id = ids.first else { return false }
                        return move(id, to: column)
                    } isTargeted: { inside in
                        withAnimation(.snappy(duration: 0.15)) { dropTarget = inside ? column.id : (dropTarget == column.id ? nil : dropTarget) }
                    }
                }
            }.padding(.horizontal, NekoLayout.pageInset).padding(.vertical, NekoLayout.sectionGap)
        }.scrollIndicators(.never)
    }
    private func card(_ task: JSONValue) -> some View {
        let status = task["status"].string
        return Button { selected = task.recordID } label: {
            TicketCard(id: "NEK-" + String(task.recordID.prefix(4)).uppercased(), title: task["title"].string, workspace: workspaceName(task), status: status, updated: relative(task["updated_at_ms"].int), highlighted: selected == task.recordID, note: status == "Failed" ? TicketPresentation.stopReason(task) : TicketPresentation.waitingReason(task))
        }
        .buttonStyle(.plain)
        .contextMenu { moveMenu(task) }
        .accessibilityLabel("\(task["title"].string), \(workspaceName(task)), \(friendlyTaskStatus(status))")
        .accessibilityHint("Open the ticket. Drag to another column to move it.")
    }

    // MARK: Moving tickets

    /// The command a move asks for, or why the move can't happen.
    static func moveCommand(from status: String, to column: String) -> (command: String?, reason: String?) {
        switch (column, status) {
        case ("working", "AwaitingApproval"), ("working", "Failed"), ("working", "Cancelled"): return ("StartTask", nil)
        case ("done", "ReadyForReview"): return ("CompleteTask", nil)
        case ("done", "AwaitingApproval"), ("done", "Queued"), ("done", "Planning"), ("done", "Building"), ("done", "Reviewing"):
            return ("CancelTask", nil)
        case ("approval", "Queued"), ("approval", "Planning"), ("approval", "Building"), ("approval", "Reviewing"):
            return (nil, "Work that has started can’t go back to waiting. Cancel it, or let it finish.")
        case ("review", _): return (nil, "Neko moves a ticket to review once its work and checks are done.")
        case ("working", "ReadyForReview"): return (nil, "Add a note on the ticket to send it back with changes.")
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
        Divider()
        ForEach(Self.columns, id: \.id) { column in
            let status = task["status"].string
            let plan = Self.moveCommand(from: status, to: column.id)
            if !column.statuses.contains(status), plan.command != nil {
                Button(column.id == "done" && plan.command == "CancelTask" ? "Cancel Ticket" : "Move to \(column.title)") { move(task.recordID, to: column) }
            }
        }
    }

    private func workspaceName(_ task: JSONValue) -> String { model.workspaces.first { $0.recordID == task["workspace_id"].string }?["name"].string ?? "Workspace" }
    private func relative(_ ms: Int) -> String {
        guard ms > 0 else { return "" }
        return Date(timeIntervalSince1970: Double(ms) / 1000).formatted(.relative(presentation: .named, unitsStyle: .abbreviated))
    }
}

struct TicketDetail: View {
    @ObservedObject var model: AppModel
    let id: String
    /// Set when shown in a drawer, where there is no sheet to dismiss.
    var close: (() -> Void)? = nil
    var fullPage = false
    @State private var confirmDelete = false
    @State private var changes: (files: [String], patch: String, truncated: Bool)?
    @State private var changesError: String?
    @State private var loadingChanges = false
    private var changesSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Changes").font(NekoFont.heading)
                Spacer()
                Button(loadingChanges ? "Loading…" : (changes == nil ? "Show changes" : "Refresh")) { loadChanges() }.disabled(loadingChanges).controlSize(.small)
            }
            Text("What this ticket changed in its own worktree. Nothing here is in your repository until you bring it over.").font(.caption).foregroundStyle(.secondary)
            if let changesError { Text(changesError).font(.caption).foregroundStyle(NekoStyle.amber) }
            if let changes {
                if changes.truncated { Label("This diff is incomplete. Open the working folder to inspect the full change.", systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(NekoStyle.amber) }
                if changes.files.isEmpty { Text("No changes yet.").foregroundStyle(.secondary) }
                else {
                    Text("\(changes.files.count) \(changes.files.count == 1 ? "file" : "files"): \(changes.files.prefix(12).joined(separator: ", "))\(changes.files.count > 12 ? "…" : "")").font(.caption)
                    ScrollView([.vertical, .horizontal]) {
                        Text(changes.patch).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                    }.frame(maxHeight: 320).padding(NekoLayout.rowInset)
                        .background(N.panel, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                }
            }
        }
    }
    private func loadChanges() {
        loadingChanges = true
        Task {
            do {
                let reply = try await model.request(.command("TaskChanges", ["task_id": .string(id)]))["TaskChanges"]
                changes = (reply["files"].array.map(\.string), reply["patch"].string, reply["truncated"].bool)
                changesError = nil
            } catch { changesError = error.localizedDescription }
            loadingChanges = false
        }
    }
    @Environment(\.dismiss) private var dismiss
    private var ticket: JSONValue { model.snapshot["tasks"].array.first { $0.recordID == id } ?? .null }
    private var conversation: TicketPresentation.Conversation { TicketPresentation.conversation(ticket) }
    private var decisions: [JSONValue] {
        DecisionPresentation.latest(model.snapshot["decision_records"].array.filter { $0["task_id"].string == id })
    }
    private var hasPreviousWork: Bool {
        (conversation.foldsHistory && (!ticket["goal"].string.isEmpty || !ticket["events"].array.isEmpty))
            || (!conversation.showsPlan && !ticket["plan"].string.isEmpty)
            || (conversation.outcome.belongsInHistory && !ticket["result"].string.isEmpty)
            || !decisions.isEmpty
    }
    var body: some View {
        VStack(spacing: 0) {
        if fullPage { FullDiskAccessBanner() }
        ScrollView {
            VStack(alignment: .leading, spacing: NekoLayout.sectionGap + 8) {
                if !fullPage { HStack(alignment: .top) {
                    Text(ticket["title"].string).font(NekoFont.title).textSelection(.enabled)
                    Spacer()
                    Button("Close") { if let close { close() } else { dismiss() } }.keyboardShortcut(.cancelAction).hidden().frame(width: 0)
                }
                Text(friendlyTaskStatus(ticket["status"].string)).foregroundStyle(.secondary) }
                if hasPreviousWork { previousWork }
                if !conversation.foldsHistory { TicketThreadView(ticket: ticket) }
                if conversation.showsPlan { section(conversation.planTitle, ticket["plan"].string) }
                if !conversation.outcome.belongsInHistory {
                    TicketOutcomeReadback(result: ticket["result"].string, outcome: conversation.outcome, events: ticket["events"].array)
                }
                if ticket["status"].string == "Failed" { stoppedCallout }
                if let question = TicketPresentation.waitingReason(ticket) {
                    VStack(alignment: .leading, spacing: 12) {
                        Label("Neko is waiting on you", systemImage: "questionmark.bubble").font(NekoFont.heading).foregroundStyle(NekoStyle.amber)
                        Text(question).lineSpacing(4).textSelection(.enabled)
                        Text("Reply below so the agent can reassess its plan.").font(NekoFont.meta).foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading).padding(NekoLayout.rowInset)
                    .background(NekoStyle.amber.opacity(0.08), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                }
                DisclosureGroup {
                    VStack(alignment: .leading, spacing: NekoLayout.sectionGap) { detailsBody }.padding(.top, 16)
                } label: {
                    Text("Changes, evidence, risk & activity").font(NekoFont.meta).foregroundStyle(.secondary)
                }
                if !fullPage { HStack {
                    switch ticket["status"].string {
                    case "AwaitingApproval":
                        if TicketPresentation.waitingReason(ticket) == nil { action("Approve plan", "ApproveTask") }
                        if TicketPresentation.canProposeSplit(ticket, splits: model.snapshot["splits"].array) { action("Propose parallel work", "ProposeSplit") }
                    case "ReadyForReview": action("Accept locally", "CompleteTask")
                    case "Failed", "Cancelled": action("Start again", "StartTask")
                    default: EmptyView()
                    }
                    if !["Completed", "Cancelled", "Failed"].contains(ticket["status"].string) { action("Cancel task", "CancelTask") }
                    if ["Completed", "Cancelled", "Failed"].contains(ticket["status"].string) {
                        Button("Delete…", role: .destructive) { confirmDelete = true }
                    }
                }.disabled(model.busy) }
            }.font(NekoFont.body)
                .frame(maxWidth: NekoLayout.readingWidth, alignment: .leading)
                .padding(NekoLayout.pageInset)
                .frame(maxWidth: .infinity)
        }.frame(maxWidth: .infinity).defaultScrollAnchor(.bottom)
        TicketComposer(model: model, id: id, status: ticket["status"].string)
            .frame(maxWidth: NekoLayout.readingWidth)
            .padding(.horizontal, NekoLayout.pageInset).padding(.top, 12).padding(.bottom, 20)
        }.frame(maxWidth: .infinity)
                .toolbar { if fullPage { agentToolbar } }
                .confirmationDialog("Delete this ticket?", isPresented: $confirmDelete) {
                    Button("Delete ticket", role: .destructive) {
                        Task { if await model.workbench(.command("DeleteTask", ["task_id": .string(id)])) { if let close { close() } else { dismiss() } } }
                    }
                } message: {
                    Text("Removes the ticket, its subtasks and their history from Neko. Your files and the task’s worktree on disk stay as they are.")
                }
    }
    private var previousWork: some View {
        DisclosureGroup {
            VStack(alignment: .leading, spacing: NekoLayout.sectionGap) {
                if conversation.foldsHistory { TicketThreadView(ticket: ticket) }
                if !conversation.showsPlan { section("Saved plan", ticket["plan"].string) }
                if conversation.outcome.belongsInHistory && !ticket["result"].string.isEmpty {
                    TicketOutcomeReadback(result: ticket["result"].string, outcome: conversation.outcome, events: ticket["events"].array)
                }
                if !decisions.isEmpty {
                    DisclosureGroup("Decision history · \(decisions.count)") {
                        VStack(alignment: .leading, spacing: 12) {
                            ForEach(decisions, id: \.recordID) { DecisionCard(model: model, record: $0) }
                        }.padding(.top, 8)
                    }
                }
            }.padding(.top, 16)
        } label: {
            Label("Previous work & history", systemImage: "clock.arrow.circlepath").font(NekoFont.meta).foregroundStyle(.secondary)
        }
    }
    @ToolbarContentBuilder private var agentToolbar: some ToolbarContent {
        ToolbarItem(placement: .navigation) {
            Button { close?() } label: { Label("Neko", systemImage: "arrow.left") }
                .keyboardShortcut(.cancelAction)
                .help("Back to \(model.agentReturnPage == "Tickets" ? "All agents" : model.agentReturnPage)")
                .disabled(model.busy)
        }
        if ticket["status"].string == "AwaitingApproval" && TicketPresentation.waitingReason(ticket) == nil {
            ToolbarItem(placement: .primaryAction) { action("Approve", "ApproveTask").buttonStyle(.borderedProminent).disabled(model.busy) }
        }
        if ticket["status"].string == "ReadyForReview" {
            ToolbarItem(placement: .primaryAction) { action("Accept locally", "CompleteTask").buttonStyle(.borderedProminent).disabled(model.busy) }
        }
        if ["Failed", "Cancelled"].contains(ticket["status"].string) {
            ToolbarItem(placement: .primaryAction) { action("Start again", "StartTask").disabled(model.busy) }
        }
        if !["Completed", "Cancelled", "Failed"].contains(ticket["status"].string) {
            ToolbarItem(placement: .primaryAction) { action("Stop", "CancelTask").disabled(model.busy) }
        }
        ToolbarItem(placement: .primaryAction) {
            Menu {
                if TicketPresentation.waitingReason(ticket) == nil && TicketPresentation.canProposeSplit(ticket, splits: model.snapshot["splits"].array) { action("Propose parallel work", "ProposeSplit") }
                if !ticket["worktree"].string.isEmpty { Button("Reveal working folder") { NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: ticket["worktree"].string) } }
                if ["Completed", "Cancelled", "Failed"].contains(ticket["status"].string) { Button("Delete…", role: .destructive) { confirmDelete = true } }
            } label: {
                Label("Agent actions", systemImage: "ellipsis")
            }
            .disabled(model.busy)
        }
    }
    @ViewBuilder private var detailsBody: some View {
            let review = TicketPresentation.review(ticket["result"].string)
            if !ticket["worktree"].string.isEmpty { changesSection }
            if let verdict = review.verdict {
                DisclosureGroup("Reviewed files and checks") {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(Array(verdict.files.enumerated()), id: \.offset) { _, file in Label(file, systemImage: "doc").textSelection(.enabled) }
                        ForEach(Array(verdict.tests.enumerated()), id: \.offset) { _, test in Label(test, systemImage: "terminal").textSelection(.enabled) }
                    }.font(.caption)
                }
            }
            if !ticket["result"].string.isEmpty {
                DisclosureGroup("Full result and raw reviewer response") { Text(ticket["result"].string).font(.system(.caption, design: .monospaced)).textSelection(.enabled) }
            }
            if ticket["supervision"] != .null {
                let assessment = ticket["supervision"]
                Text("Risk assessment · \(assessment["risk"].string.capitalized)").font(.headline)
                ReadableText(text: assessment["reason"].string)
                ForEach(["evidence", "files", "tests", "sensitive_areas", "uncertainties"], id: \.self) { key in
                    if !assessment[key].array.isEmpty {
                        DisclosureGroup(key.replacingOccurrences(of: "_", with: " ").capitalized) { ForEach(assessment[key].array, id: \.self) { item in ReadableText(text: item.string) } }
                    }
                }
            }
            ForEach(model.snapshot["splits"].array.filter { $0["parent_id"].string == id }, id: \.self) { split in
                Text(split["approved"].bool ? "Approved parallel work" : "Parallel proposal · approval required").font(.headline)
                if split["approved"].bool { Text(split["integrated"].bool ? "Child changes integrated into the parent worktree." : "Child changes are not yet integrated.").foregroundStyle(.secondary) }
                ForEach(Array(split["subtasks"].array.enumerated()), id: \.offset) { index, child in
                    VStack(alignment: .leading, spacing: 6) {
                        Text("\(index + 1). \(child["title"].string)").bold()
                        ReadableText(text: child["goal"].string)
                        Text(child["files"].array.map(\.string).joined(separator: "\n")).font(.system(.caption, design: .monospaced))
                        if !child["tests"].array.isEmpty { Text("Checks: " + child["tests"].array.map(\.string).joined(separator: ", ")).font(.caption) }
                        if !child["depends_on"].array.isEmpty { Text("After subtasks: " + child["depends_on"].array.map { String($0.int + 1) }.joined(separator: ", ")).font(.caption) }
                        if let task = model.snapshot["tasks"].array.first(where: { $0["id"] == child["task_id"] }) {
                            Text(friendlyTaskStatus(task["status"].string)).foregroundStyle(.secondary)
                            Button("Open subtask") { model.openAgent(task.recordID) }
                        } else { Text(split["approved"].bool ? "Subtask not available in this snapshot" : "Awaiting approval").foregroundStyle(.secondary) }
                    }.padding(NekoLayout.rowInset)
                        .background(N.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                }
            }
            if !ticket["worktree"].string.isEmpty { Button("Reveal working folder", systemImage: "folder") { NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: ticket["worktree"].string) } }
            Divider()
            DisclosureGroup("Activity · \(ticket["events"].array.count) events") {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(Array(ticket["events"].array.enumerated()), id: \.offset) { _, event in eventView(event) }
                }.padding(.top, 10)
            }
    }
    private func action(_ label: String, _ command: String) -> some View { Button(label) { Task { await model.workbench(.command(command, ["task_id": .string(id)])) } } }
    private var stoppedCallout: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("Why it stopped", systemImage: "exclamationmark.triangle").font(NekoFont.heading).foregroundStyle(NekoStyle.amber)
            Text(TicketPresentation.stopReason(ticket)).lineSpacing(4).textSelection(.enabled)
            HStack {
                Button("Choose folder…") { chooseFolder() }
                Text(model.snapshot["task_roots"][id].string.isEmpty ? "Pick the repository this ticket is about, then Neko starts it." : "Working in \(URL(fileURLWithPath: model.snapshot["task_roots"][id].string).lastPathComponent). Pick another repository to start over there.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(NekoLayout.rowInset)
        .background(NekoStyle.amber.opacity(0.08), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }
    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Use This Folder"
        panel.message = "Choose the Git repository for “\(ticket["title"].string)”."
        if let root = model.snapshot["workspace_folders"][ticket["workspace_id"].string].array.first?.string { panel.directoryURL = URL(fileURLWithPath: root) }
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task {
            guard await model.workbench(.command("SetTaskFolder", ["task_id": .string(id), "folder": .string(url.path)])) else { return }
            if await model.workbench(.command("StartTask", ["task_id": .string(id)])) {
                model.notice = "Started “\(ticket["title"].string)” in \(url.lastPathComponent)."
            }
        }
    }
    @ViewBuilder private func eventView(_ event: JSONValue) -> some View {
        let message = event["message"].string
        if let receipt = TicketCommandReceipt.from(event: event) {
            TicketCommandReceiptView(receipt: receipt, role: event["role"].string)
        } else {
            DisclosureGroup {
                Text(message).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
            } label: {
                VStack(alignment: .leading, spacing: 4) {
                    Text(event["role"].string.capitalized).font(.caption).foregroundStyle(.secondary)
                    Text(String(message.split(separator: "\n").first.map(String.init)?.prefix(100) ?? "Event".prefix(100))).font(.callout).lineLimit(2)
                }
            }
        }
    }
    @ViewBuilder private func section(_ title: String, _ text: String) -> some View {
        if !text.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                Text(title).font(NekoFont.heading).accessibilityAddTraits(.isHeader)
                ReadableText(text: text).font(NekoFont.chat).lineSpacing(4)
            }
        }
    }
}

/// The latest answer stays readable; large evidence lists live in the supporting
/// disclosure. Copy always preserves the full saved result, including raw review.
private struct TicketOutcomeReadback: View {
    let result: String
    let outcome: TicketPresentation.Outcome
    let events: [JSONValue]
    @State private var copied = false

    var body: some View {
        let review = TicketPresentation.review(result)
        VStack(alignment: .leading, spacing: 20) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(outcome.title).font(NekoFont.title).accessibilityAddTraits(.isHeader)
                    ChatAuthorLine(author: "Neko")
                }
                Spacer()
                if !result.isEmpty {
                    Button(copied ? "Copied" : "Copy result", systemImage: copied ? "checkmark" : "doc.on.doc") {
                        NSPasteboard.general.clearContents()
                        copied = NSPasteboard.general.setString(result, forType: .string)
                    }
                    .buttonStyle(.borderless).controlSize(.small).font(NekoFont.meta)
                    .help("Copy the full saved result, including the reviewer response")
                }
            }
            Text(outcome.note).font(NekoFont.meta).foregroundStyle(N.text2)
            if review.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text("No result text was saved for this ticket.").foregroundStyle(N.text2)
            } else {
                ReadableText(text: review.body).font(NekoFont.chat).lineSpacing(4)
            }
            Divider().padding(.top, 8)
            VStack(alignment: .leading, spacing: 14) {
                Text(outcome.reviewIsCurrent ? "Independent review" : "Recorded independent review").font(NekoFont.heading)
                if let verdict = review.verdict {
                    Label(verdict.passed ? "Reviewer reported passed" : "Reviewer found issues", systemImage: verdict.passed ? "checkmark.shield" : "exclamationmark.shield")
                        .font(NekoFont.body)
                        .foregroundStyle(!verdict.passed ? Color.orange : outcome.reviewIsCurrent ? Color.green : Color.secondary)
                    ReadableText(text: verdict.summary)
                    ForEach(Array(verdict.findings.enumerated()), id: \.offset) { _, finding in ReadableText(text: "• " + finding) }
                    Text("\(verdict.files.count) files reviewed · \(verdict.tests.count) checks reported").font(.caption).foregroundStyle(.secondary)
                    Text("This is the recorded reviewer response; the ticket status reflects the current verification gate.").font(.caption).foregroundStyle(.secondary)
                } else {
                    Text(outcome == .reviewing ? "No structured reviewer verdict yet." : "No structured reviewer verdict is available. Any saved review text remains in the result.")
                        .font(.callout).foregroundStyle(N.text2)
                }
            }.font(NekoFont.body)
            TicketCommandEvidenceView(events: events)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onChange(of: result) { _, _ in copied = false }
    }
}

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
    @Environment(\.ink) private var ink
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
        .padding(NekoLayout.rowInset)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(hover ? ink.raisedHover : ink.panel, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(highlighted ? NekoStyle.accent : .clear))
        .contentShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .help(id)
        .onHover { h in withAnimation(.easeOut(duration: 0.12)) { hover = h } }
    }
}

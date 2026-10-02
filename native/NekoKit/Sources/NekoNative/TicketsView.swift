import SwiftUI
import NekoKit

enum TicketPresentation {
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
    static func canProposeSplit(_ ticket: JSONValue, splits: [JSONValue]) -> Bool {
        ticket["status"].string == "AwaitingApproval" && !splits.contains { split in
            split["parent_id"] == ticket["id"] || split["subtasks"].array.contains { $0["task_id"] == ticket["id"] }
        }
    }
}

struct TicketsView: View {
    @ObservedObject var model: AppModel
    @State private var selected: String?
    @State private var includeStopped = true
    @State private var dropTarget: String?
    @State private var search = ""
    @State private var confirmCancel: JSONValue?
    @AppStorage("neko.work.layout") private var layout = "board"
    @AppStorage("neko.work.open") private var openAs = "drawer"
    var sidebarHidden = false
    private var collapsed: Bool { sidebarHidden }
    struct Column { let id: String; let title: String; let color: Color; let statuses: [String] }
    static let columns: [Column] = [
        Column(id: "approval", title: "Needs approval", color: NekoStyle.amber, statuses: ["AwaitingApproval", "Failed"]),
        Column(id: "working", title: "Working", color: NekoStyle.accent, statuses: ["Queued", "Planning", "Building", "Reviewing"]),
        Column(id: "review", title: "Ready to review", color: NekoStyle.sky, statuses: ["ReadyForReview"]),
        Column(id: "done", title: "Done", color: NekoStyle.mint, statuses: ["Completed", "Cancelled"])
    ]
    private func tasks(in column: Column) -> [JSONValue] {
        model.tasks.filter { task in
            column.statuses.contains(task["status"].string)
            && (includeStopped || !["Failed", "Cancelled"].contains(task["status"].string))
            && (search.isEmpty || task["title"].string.localizedCaseInsensitiveContains(search))
        }.sorted { $0["updated_at_ms"].int > $1["updated_at_ms"].int }
    }
    private var workspaceLabel: String { model.selectedWorkspace.flatMap { id in model.workspaces.first { $0.recordID == id }?["name"].string } ?? "All workspaces" }
    private var startsWithoutAsking: Bool { model.snapshot["start_without_approval"].bool }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            toolbar
            Divider().opacity(0.5)
            if layout == "list" { list } else { board }
        }
        .ignoresSafeArea(.container, edges: .top)
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
        .sheet(isPresented: Binding(get: { openAs == "modal" && selected != nil }, set: { if !$0 { selected = nil } })) {
            if let selected { TicketDetail(model: model, id: selected).frame(minWidth: 720, minHeight: 640) }
        }
        .inspector(isPresented: Binding(get: { openAs == "drawer" && selected != nil }, set: { if !$0 { selected = nil } })) {
            if let selected {
                VStack(spacing: 0) {
                    HStack(spacing: 6) {
                        Button { openAs = "modal" } label: { Image(systemName: "rectangle.center.inset.filled") }
                            .buttonStyle(.borderless).help("Open tickets in a window instead")
                        Spacer()
                        Button { self.selected = nil } label: { Image(systemName: "xmark") }
                            .buttonStyle(.borderless).help("Close")
                    }.padding(.horizontal, 14).frame(height: 36)
                    TicketDetail(model: model, id: selected, close: { self.selected = nil })
                }
                .inspectorColumnWidth(min: 340, ideal: 400, max: 760)
                .ignoresSafeArea(.container, edges: .top)
            }
        }
    }

    private var toolbar: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 1) {
                Text("Work").font(.system(size: 15, weight: .semibold))
                Text("\(workspaceLabel) · \(model.tasks.count) \(model.tasks.count == 1 ? "ticket" : "tickets")\(startsWithoutAsking ? " · starts without asking" : "")")
                    .font(.system(size: 11)).foregroundStyle(.secondary).lineLimit(1)
            }
            .layoutPriority(-1)
            Spacer(minLength: 12)
            TextField("Search tickets", text: $search).textFieldStyle(.roundedBorder).frame(minWidth: 90, maxWidth: 180).controlSize(.small)
            GlassSegmented(selection: $layout, options: [
                .init(value: "list", title: "List", symbol: "list.bullet", help: "Show tickets as a list"),
                .init(value: "board", title: "Board", symbol: "rectangle.split.3x1", help: "Show tickets as a board")
            ], iconOnly: true)
            GlassSegmented(selection: $openAs, options: [
                .init(value: "drawer", title: "Drawer", symbol: "sidebar.right", help: "Open tickets in a drawer beside the list"),
                .init(value: "modal", title: "Window", symbol: "rectangle.center.inset.filled", help: "Open tickets in a window on top")
            ], iconOnly: true)
            Menu {
                Toggle("Start tickets without asking", isOn: Binding(get: { startsWithoutAsking }, set: { value in
                    Task { await model.workbench(.command("SetStartWithoutApproval", ["enabled": .bool(value)])) }
                }))
                Text("New tickets from chat begin working once planned. Watched sources keep their own rules. Nothing is pushed without you.")
                Divider()
                Toggle("Show failed and cancelled", isOn: $includeStopped)
                Button("Clear finished tickets") { Task { await model.workbench(.command("ClearFinishedTasks", ["workspace_id": model.selectedWorkspace.map { .string($0) } ?? .null])) } }
                    .disabled(!model.tasks.contains { ["Completed", "Cancelled"].contains($0["status"].string) })
            } label: { Image(systemName: "ellipsis.circle") }
                .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
                .help("Work options")
        }
        .controlSize(.small)
        .padding(.leading, collapsed ? 150 : 20).padding(.trailing, 16)
        .frame(height: 52)
        .animation(.snappy(duration: 0.25), value: collapsed)
    }

    // MARK: List

    private var list: some View {
        List(selection: Binding(get: { selected }, set: { selected = $0 })) {
            ForEach(Self.columns, id: \.id) { column in
                let items = tasks(in: column)
                if !items.isEmpty {
                    Section("\(column.title) · \(items.count)") {
                        ForEach(items, id: \.recordID) { task in listRow(task, color: column.color).tag(task.recordID) }
                    }
                }
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .overlay { if Self.columns.allSatisfy({ tasks(in: $0).isEmpty }) { ContentUnavailableView("No tickets", systemImage: "tray", description: Text(search.isEmpty ? "Ask Neko for work in Today, and its tickets appear here." : "No ticket matches “\(search)”.")) } }
    }
    private func listRow(_ task: JSONValue, color: Color) -> some View {
        let status = task["status"].string
        return HStack(spacing: 10) {
            Circle().fill(ticketStatusColor(status)).frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 2) {
                Text(task["title"].string).font(.system(size: 13, weight: .medium)).lineLimit(1)
                Text("\(workspaceName(task)) · \(friendlyTaskStatus(status))").font(.system(size: 11)).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            Text(relative(task["updated_at_ms"].int)).font(.system(size: 11)).foregroundStyle(.tertiary)
        }
        .padding(.vertical, 3)
        .contextMenu { moveMenu(task) }
    }

    // MARK: Board

    private var board: some View {
        ScrollView(.horizontal) {
            HStack(alignment: .top, spacing: 14) {
                ForEach(Self.columns, id: \.id) { column in
                    let items = tasks(in: column)
                    VStack(alignment: .leading, spacing: 8) {
                        HStack(spacing: 8) {
                            Circle().fill(column.color).frame(width: 8, height: 8)
                            Text(column.title).font(.system(size: 13, weight: .semibold))
                            Text(String(items.count)).font(.system(size: 12).monospacedDigit()).foregroundStyle(.secondary)
                            Spacer(minLength: 0)
                        }.padding(.horizontal, 6).frame(height: 30)
                        .accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
                        ScrollView(.vertical) {
                            LazyVStack(spacing: 8) {
                                ForEach(items, id: \.recordID) { task in
                                    card(task)
                                        .draggable(task.recordID) { card(task).frame(width: 260).opacity(0.9) }
                                }
                                if items.isEmpty {
                                    Text(dropTarget == column.id ? "Drop to move here" : "Nothing here")
                                        .font(.system(size: 12)).foregroundStyle(.tertiary).frame(maxWidth: .infinity, minHeight: 60)
                                }
                            }.padding(6)
                        }.scrollIndicators(.never)
                    }
                    .frame(width: 280).frame(maxHeight: .infinity, alignment: .top)
                    .background(dropTarget == column.id ? column.color.opacity(0.10) : Color.white.opacity(0.025), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(dropTarget == column.id ? column.color.opacity(0.6) : Color.white.opacity(0.05)))
                    .dropDestination(for: String.self) { ids, _ in
                        guard let id = ids.first else { return false }
                        return move(id, to: column)
                    } isTargeted: { inside in
                        withAnimation(.snappy(duration: 0.15)) { dropTarget = inside ? column.id : (dropTarget == column.id ? nil : dropTarget) }
                    }
                }
            }.padding(.horizontal, 20).padding(.vertical, 14)
        }.scrollIndicators(.never)
    }
    private func card(_ task: JSONValue) -> some View {
        let index = model.workspaces.firstIndex { $0.recordID == task["workspace_id"].string } ?? 0
        let status = task["status"].string
        return Button { selected = task.recordID } label: {
            TicketCard(id: "NEK-" + String(task.recordID.prefix(4)).uppercased(), title: task["title"].string, workspace: workspaceName(task), workspaceColor: workspaceColor(index), meta: friendlyTaskStatus(status), highlighted: selected == task.recordID, activity: .forTask(status))
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
        case ("working", "AwaitingApproval"): return ("ApproveTask", nil)
        case ("working", "Failed"), ("working", "Cancelled"): return ("RetryTask", nil)
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
        Task {
            if await model.workbench(.command(command, ["task_id": .string(id)])) {
                model.notice = command == "CancelTask" ? "Cancelled “\(task["title"].string)”. Its files and worktree stay on disk." : "Moved “\(task["title"].string)” to \(column.title)."
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
    @State private var note = ""
    @State private var childTicket: String?
    @State private var confirmDelete = false
    @State private var changes: (files: [String], patch: String)?
    @State private var changesError: String?
    @State private var loadingChanges = false
    private var changesSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Changes").font(.headline)
                Spacer()
                Button(loadingChanges ? "Loading…" : (changes == nil ? "Show changes" : "Refresh")) { loadChanges() }.disabled(loadingChanges).controlSize(.small)
            }
            Text("What this ticket changed in its own worktree. Nothing here is in your repository until you bring it over.").font(.caption).foregroundStyle(.secondary)
            if let changesError { Text(changesError).font(.caption).foregroundStyle(NekoStyle.amber) }
            if let changes {
                if changes.files.isEmpty { Text("No changes yet.").foregroundStyle(.secondary) }
                else {
                    Text("\(changes.files.count) \(changes.files.count == 1 ? "file" : "files"): \(changes.files.prefix(12).joined(separator: ", "))\(changes.files.count > 12 ? "…" : "")").font(.caption)
                    ScrollView([.vertical, .horizontal]) {
                        Text(changes.patch).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                    }.frame(maxHeight: 320).padding(8).nekoCard(padding: 0, radius: 8)
                }
            }
        }
    }
    private func loadChanges() {
        loadingChanges = true
        Task {
            do {
                let reply = try await model.request(.command("TaskChanges", ["task_id": .string(id)]))["TaskChanges"]
                changes = (reply["files"].array.map(\.string), reply["patch"].string)
                changesError = nil
            } catch { changesError = error.localizedDescription }
            loadingChanges = false
        }
    }
    @Environment(\.dismiss) private var dismiss
    private var ticket: JSONValue { model.snapshot["tasks"].array.first { $0.recordID == id } ?? .null }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack(alignment: .top) { Text(ticket["title"].string).font(.title.bold()); Spacer(); Button("Done") { if let close { close() } else { dismiss() } }.keyboardShortcut(.cancelAction) }
                Text(friendlyTaskStatus(ticket["status"].string)).foregroundStyle(.secondary)
                HStack {
                    switch ticket["status"].string {
                    case "AwaitingApproval":
                        action("Approve plan", "ApproveTask")
                        if TicketPresentation.canProposeSplit(ticket, splits: model.snapshot["splits"].array) { action("Propose parallel work", "ProposeSplit") }
                    case "ReadyForReview": action("Mark complete", "CompleteTask")
                    case "Failed", "Cancelled": action("Retry", "RetryTask")
                    default: EmptyView()
                    }
                    if !["Completed", "Cancelled", "Failed"].contains(ticket["status"].string) { action("Cancel task", "CancelTask") }
                    if ["Completed", "Cancelled", "Failed"].contains(ticket["status"].string) {
                        Button("Delete…", role: .destructive) { confirmDelete = true }
                    }
                }.disabled(model.busy)
                .confirmationDialog("Delete this ticket?", isPresented: $confirmDelete) {
                    Button("Delete ticket", role: .destructive) {
                        Task { if await model.workbench(.command("DeleteTask", ["task_id": .string(id)])) { dismiss() } }
                    }
                } message: {
                    Text("Removes the ticket, its subtasks and their history from Neko. Your files and the task’s worktree on disk stay as they are.")
                }
                section("Goal", ticket["goal"].string)
                section("Plan", ticket["plan"].string)
                let review = TicketPresentation.review(ticket["result"].string)
                section("Result", review.body)
                if !ticket["worktree"].string.isEmpty { changesSection }
                if let verdict = review.verdict {
                    VStack(alignment: .leading, spacing: 12) {
                        Label(verdict.passed ? "Reviewer reported passed" : "Reviewer found issues", systemImage: verdict.passed ? "checkmark.shield" : "exclamationmark.shield").font(.headline).foregroundStyle(verdict.passed ? .green : .orange)
                        ReadableText(text: verdict.summary)
                        if !verdict.findings.isEmpty { ForEach(Array(verdict.findings.enumerated()), id: \.offset) { _, finding in ReadableText(text: "• " + finding) } }
                        Text("\(verdict.files.count) files reviewed · \(verdict.tests.count) checks reported").font(.caption).foregroundStyle(.secondary)
                        DisclosureGroup("Reviewed files and checks") {
                            VStack(alignment: .leading, spacing: 8) {
                                ForEach(Array(verdict.files.enumerated()), id: \.offset) { _, file in Label(file, systemImage: "doc").textSelection(.enabled) }
                                ForEach(Array(verdict.tests.enumerated()), id: \.offset) { _, test in Label(test, systemImage: "terminal").textSelection(.enabled) }
                            }.font(.caption)
                        }
                        Text("This summarizes the reviewer response. The ticket status reflects the daemon’s verification gate.").font(.caption).foregroundStyle(.secondary)
                    }.padding(16).nekoCard(padding: 0, radius: 12)
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
                                Button("Open subtask") { childTicket = task.recordID }
                            } else { Text(split["approved"].bool ? "Subtask not available in this snapshot" : "Awaiting approval").foregroundStyle(.secondary) }
                        }.padding(12).nekoCard(padding: 0, radius: 12)
                    }
                }
                if !ticket["worktree"].string.isEmpty { Button("Reveal working folder", systemImage: "folder") { NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: ticket["worktree"].string) } }
                Divider()
                DisclosureGroup("Activity · \(ticket["events"].array.count) events") {
                    VStack(alignment: .leading, spacing: 12) {
                        ForEach(Array(ticket["events"].array.enumerated()), id: \.offset) { _, event in eventView(event) }
                    }.padding(.top, 10)
                }
                TextField("Add a note…", text: $note, axis: .vertical).lineLimit(2...5).textFieldStyle(.roundedBorder)
                Button("Add note") { let text = note; Task { if await model.workbench(.command("AddTicketNote", ["task_id": .string(id), "text": .string(text)])) { note = "" } } }.disabled(note.isEmpty || model.busy)
            }.padding(28).frame(maxWidth: 900, alignment: .leading)
        }.frame(maxWidth: .infinity)
            .sheet(isPresented: Binding(get: { childTicket != nil }, set: { if !$0 { childTicket = nil } })) {
                if let childTicket { TicketDetail(model: model, id: childTicket).frame(minWidth: 650, minHeight: 600) }
            }
            .onChange(of: id) { _, _ in note = ""; childTicket = nil }
    }
    private func action(_ label: String, _ command: String) -> some View { Button(label) { Task { await model.workbench(.command(command, ["task_id": .string(id)])) } } }
    private func eventView(_ event: JSONValue) -> some View {
        let message = event["message"].string
        let prefix = "VERIFICATION_COMMAND "
        let receipt = message.hasPrefix(prefix) ? (try? JSONDecoder().decode(JSONValue.self, from: Data(message.dropFirst(prefix.count).utf8))) : nil
        let heading: String
        if let receipt { heading = "\(receipt["exit_code"] == .number(0) ? "Succeeded" : "Failed or incomplete") · \(receipt["command"].string)" }
        else { heading = String(message.split(separator: "\n").first.map(String.init)?.prefix(100) ?? "Event".prefix(100)) }
        return DisclosureGroup {
            Text(message).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
        } label: {
            VStack(alignment: .leading, spacing: 4) {
                Text(event["role"].string.capitalized).font(.caption).foregroundStyle(.secondary)
                Text(heading).font(.callout).lineLimit(2)
            }
        }
    }
    @ViewBuilder private func section(_ title: String, _ text: String) -> some View { if !text.isEmpty { Text(title).font(.headline); ReadableText(text: text) } }
}

struct TicketCard: View {
    let id: String
    let title: String
    let workspace: String
    let workspaceColor: Color
    let meta: String
    let highlighted: Bool
    var activity: NekoActivity = .idle
    @State private var hover = false
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Text(id).font(.system(size: 12)).foregroundStyle(N.text4)
                Spacer()
                Label { Text(meta).lineLimit(1) } icon: { PixelGlyph(activity: activity, size: 10) }
                    .font(.system(size: 11)).foregroundStyle(activity.animates ? N.text2 : N.text4)
            }
            Text(title).font(.system(size: 13, weight: .medium)).foregroundStyle(N.text).lineSpacing(2).lineLimit(3).multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 6) {
                RoundedRectangle(cornerRadius: 2, style: .continuous).fill(workspaceColor).frame(width: 8, height: 8)
                Text(workspace).font(.system(size: 12)).foregroundStyle(N.text3).lineLimit(1)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(hover ? N.selected : N.card, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(highlighted ? NekoStyle.accent.opacity(0.35) : Color.white.opacity(hover ? 0.1 : 0.06)))
        .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .onHover { h in withAnimation(.easeOut(duration: 0.12)) { hover = h } }
    }
}

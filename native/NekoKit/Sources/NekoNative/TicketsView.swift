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
    @Environment(\.sidebarCollapsed) private var collapsed
    private let columns: [(title: String, icon: String, color: Color, statuses: [String])] = [
        ("Needs approval", "circle.lefthalf.filled", NekoStyle.amber, ["AwaitingApproval", "Failed"]),
        ("In progress", "circle.dotted", NekoStyle.accent, ["Queued", "Planning", "Building", "Reviewing"]),
        ("Ready to review", "checkmark.circle", NekoStyle.sky, ["ReadyForReview"]),
        ("Done", "checkmark.circle.fill", NekoStyle.mint, ["Completed", "Cancelled"])
    ]
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            PanelHeader(title: "Tickets", crumb: model.selectedWorkspace.flatMap { id in model.workspaces.first { $0.recordID == id }?["name"].string } ?? "All workspaces") { EmptyView() }
            HStack(spacing: 8) {
                Button(includeStopped ? "Showing failed & cancelled" : "Hiding failed & cancelled") { includeStopped.toggle() }.controlSize(.small).glassButton()
                Spacer()
                Text("\(model.tasks.count) \(model.tasks.count == 1 ? "ticket" : "tickets")").font(.system(size: 12)).foregroundStyle(N.text4)
            }
            .padding(.leading, collapsed ? 150 : 20).padding(.trailing, 20)
            .frame(height: 52).overlay(alignment: .bottom) { N.line.frame(height: 1) }
            .animation(.snappy(duration: 0.25), value: collapsed)
            ScrollView(.horizontal) {
                HStack(alignment: .top, spacing: 16) {
                    ForEach(columns, id: \.title) { column in
                        let tasks = model.tasks.filter { column.statuses.contains($0["status"].string) && (includeStopped || !["Failed", "Cancelled"].contains($0["status"].string)) }.sorted { $0["updated_at_ms"].int > $1["updated_at_ms"].int }
                        VStack(alignment: .leading, spacing: 8) {
                            HStack(spacing: 8) {
                                StatusGlyph(status: column.statuses[0], size: 14)
                                Text(column.title).font(.system(size: 13, weight: .medium)).foregroundStyle(N.text)
                                Text(String(tasks.count)).font(.system(size: 13).monospacedDigit()).foregroundStyle(N.text4)
                                Spacer(minLength: 0)
                            }.padding(.horizontal, 4).frame(height: 32)
                            .accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
                            ScrollView(.vertical) {
                                LazyVStack(spacing: 8) {
                                    ForEach(tasks, id: \.recordID) { task in card(task, color: column.color) }
                                    if tasks.isEmpty {
                                        Text("Nothing here yet").font(.system(size: 12)).foregroundStyle(N.text4).frame(maxWidth: .infinity, alignment: .leading).padding(16)
                                            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(Color.white.opacity(0.08), style: StrokeStyle(lineWidth: 1, dash: [4, 4])))
                                    }
                                }
                            }.scrollIndicators(.never)
                        }.frame(width: 264).frame(maxHeight: .infinity, alignment: .top)
                    }
                }.padding(.horizontal, 20).padding(.vertical, 16)
            }.scrollIndicators(.never)
        }.onChange(of: model.selectedWorkspace) { _, _ in selected = nil }
            .sheet(isPresented: Binding(get: { selected != nil }, set: { if !$0 { selected = nil } })) {
                if let selected { TicketDetail(model: model, id: selected).frame(minWidth: 680, minHeight: 620) }
            }
    }
    private func card(_ task: JSONValue, color: Color) -> some View {
        let index = model.workspaces.firstIndex { $0.recordID == task["workspace_id"].string } ?? 0
        let workspace = model.workspaces.first { $0.recordID == task["workspace_id"].string }?["name"].string ?? "Workspace"
        let status = task["status"].string
        return Button { selected = task.recordID } label: {
            TicketCard(id: "NEK-" + String(task.recordID.prefix(4)).uppercased(), title: task["title"].string, workspace: workspace, workspaceColor: workspaceColor(index), meta: friendlyTaskStatus(status), highlighted: status == "AwaitingApproval")
        }.buttonStyle(.plain).accessibilityLabel("\(task["title"].string), \(workspace), \(friendlyTaskStatus(status))").accessibilityHint("Open ticket details and available actions")
    }
    private func emptyText(_ column: String) -> String {
        switch column {
        case "Needs approval": "Nothing waiting on you."
        case "In progress": "New plans will appear here."
        case "Ready to review": "Finished work lands here for your review."
        default: "Completed work stays here."
        }
    }
}

struct TicketDetail: View {
    @ObservedObject var model: AppModel
    let id: String
    @State private var note = ""
    @State private var childTicket: String?
    @Environment(\.dismiss) private var dismiss
    private var ticket: JSONValue { model.snapshot["tasks"].array.first { $0.recordID == id } ?? .null }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack(alignment: .top) { Text(ticket["title"].string).font(.title.bold()); Spacer(); Button("Done") { dismiss() }.keyboardShortcut(.cancelAction) }
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
                }.disabled(model.busy)
                section("Goal", ticket["goal"].string)
                section("Plan", ticket["plan"].string)
                let review = TicketPresentation.review(ticket["result"].string)
                section("Result", review.body)
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
    @State private var hover = false
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Text(id).font(.system(size: 12)).foregroundStyle(N.text4)
                Spacer()
                Text(meta).font(.system(size: 11)).foregroundStyle(N.text4).lineLimit(1)
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

import SwiftUI
import AppKit
import NekoKit

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
                        .background(N.panel, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
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
                    .background(NekoStyle.amber.opacity(0.08), in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
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
                .help("Back to \(model.agentReturnPage.title)")
                .disabled(model.busy)
        }
        if ticket["status"].string == "AwaitingApproval" && TicketPresentation.waitingReason(ticket) == nil {
            ToolbarItem(placement: .primaryAction) { action("Approve", "ApproveTask").nekoButton(.primary).disabled(model.busy) }
        }
        if ticket["status"].string == "ReadyForReview" {
            ToolbarItem(placement: .primaryAction) { action("Accept locally", "CompleteTask").nekoButton(.primary).disabled(model.busy) }
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
                        .background(N.card, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
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
        .background(NekoStyle.amber.opacity(0.08), in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
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
                    .nekoButton(.quiet).controlSize(.small).font(NekoFont.meta)
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

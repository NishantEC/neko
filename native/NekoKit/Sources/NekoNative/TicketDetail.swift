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
    @State private var showLog = false
    @State private var renaming = false
    @Environment(\.dismiss) private var dismiss
    private var ticket: JSONValue { model.snapshot["tasks"].array.first { $0.recordID == id } ?? .null }
    private var conversation: TicketPresentation.Conversation { TicketPresentation.conversation(ticket) }
    private var decisions: [JSONValue] {
        DecisionPresentation.latest(model.snapshot["decision_records"].array.filter { $0["task_id"].string == id })
    }
    var body: some View {
        VStack(spacing: 0) {
        if fullPage { FullDiskAccessBanner() }
        ScrollView {
            VStack(alignment: .leading, spacing: NekoLayout.sectionGap + 8) {
                if !fullPage { HStack(alignment: .top) {
                    Text(TaskText(task: ticket, snapshot: model.snapshot).title).font(NekoFont.title).textSelection(.enabled)
                    Spacer()
                    Button("Close") { if let close { close() } else { dismiss() } }.keyboardShortcut(.cancelAction).hidden().frame(width: 0)
                }
                Text(friendlyTaskStatus(ticket["status"].string)).foregroundStyle(.secondary) }
                TicketThreadView(ticket: ticket)
                if !ticket["plan"].string.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && conversation.outcome == .previous {
                    section(conversation.showsPlan ? conversation.planTitle : "Plan", ticket["plan"].string)
                }
                if let question = TicketPresentation.waitingReason(ticket) {
                    VStack(alignment: .leading, spacing: NekoSpace.md) {
                        Label("Neko is waiting on you", systemImage: "questionmark.bubble").font(NekoFont.heading).foregroundStyle(NekoStyle.amber)
                        Text(question).lineSpacing(4).textSelection(.enabled)
                        Text("Reply below so the agent can reassess its plan.").font(NekoFont.meta).foregroundStyle(N.text3)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading).padding(NekoLayout.rowInset)
                    .nekoFill(NekoStyle.amber.opacity(0.08), radius: NekoRadius.xl)
                }
                if ticket["status"].string == "Failed" { stoppedCallout }
                if !ticket["result"].string.isEmpty {
                    TicketOutcomeReadback(result: ticket["result"].string, outcome: conversation.outcome, events: ticket["events"].array)
                }
                if !ticket["worktree"].string.isEmpty { TicketFilesChanged(model: model, id: id) }
                TicketDeliveryCard(ticket: ticket)
                supporting
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
                .renameAlert("Rename task", isPresented: $renaming, current: ticket["display_title"].string) { title in
                    Task { await model.workbench(.command("RenameTask", ["task_id": .string(id), "title": .string(title)])) }
                }
                .confirmationDialog("Delete this task?", isPresented: $confirmDelete) {
                    Button("Delete task", role: .destructive) {
                        Task { if await model.workbench(.command("DeleteTask", ["task_id": .string(id)])) { if let close { close() } else { dismiss() } } }
                    }
                } message: {
                    Text("Removes the task, its subtasks and their history from Neko. Your files and the task’s worktree on disk stay as they are.")
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
                Button("Rename…") { renaming = true }
                if !ticket["worktree"].string.isEmpty { Button("Reveal working folder") { NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: ticket["worktree"].string) } }
                if ["Completed", "Cancelled", "Failed"].contains(ticket["status"].string) { Button("Delete…", role: .destructive) { confirmDelete = true } }
            } label: {
                Label("Task actions", systemImage: "ellipsis")
            }
            .disabled(model.busy)
        }
    }
    /// Risk, parallel work, decisions and the raw log: shown when present, in
    /// that order, below the result. Only the raw event log opens on click.
    @ViewBuilder private var supporting: some View {
        if ticket["supervision"] != .null {
            let assessment = ticket["supervision"]
            VStack(alignment: .leading, spacing: NekoSpace.sm) {
                Text("Risk · \(assessment["risk"].string.capitalized)").font(NekoFont.heading)
                ReadableText(text: assessment["reason"].string)
                ForEach(["evidence", "files", "tests", "sensitive_areas", "uncertainties"], id: \.self) { key in
                    if !assessment[key].array.isEmpty {
                        Text(key.replacingOccurrences(of: "_", with: " ").capitalized).font(NekoFont.meta).foregroundStyle(N.text3).padding(.top, NekoSpace.xxs)
                        ForEach(assessment[key].array, id: \.self) { item in ReadableText(text: "• " + item.string) }
                    }
                }
            }
        }
        ForEach(model.snapshot["splits"].array.filter { $0["parent_id"].string == id }, id: \.self) { split in
            VStack(alignment: .leading, spacing: NekoSpace.md) {
                Text(split["approved"].bool ? "Parallel work" : "Parallel proposal · approval required").font(NekoFont.heading)
                if split["approved"].bool { Text(split["integrated"].bool ? "Child changes integrated into this task's worktree." : "Child changes are not yet integrated.").foregroundStyle(N.text3) }
                ForEach(Array(split["subtasks"].array.enumerated()), id: \.offset) { index, child in
                    VStack(alignment: .leading, spacing: NekoSpace.xs) {
                        Text("\(index + 1). \(child["title"].string)").bold()
                        ReadableText(text: child["goal"].string)
                        Text(child["files"].array.map(\.string).joined(separator: "\n")).font(NekoFont.mono)
                        if !child["tests"].array.isEmpty { Text("Checks: " + child["tests"].array.map(\.string).joined(separator: ", ")).font(NekoFont.meta) }
                        if !child["depends_on"].array.isEmpty { Text("After subtasks: " + child["depends_on"].array.map { String($0.int + 1) }.joined(separator: ", ")).font(NekoFont.meta) }
                        if let task = model.snapshot["tasks"].array.first(where: { $0["id"] == child["task_id"] }) {
                            Text(friendlyTaskStatus(task["status"].string)).foregroundStyle(N.text3)
                            Button("Open subtask") { model.openAgent(task.recordID) }
                        } else { Text(split["approved"].bool ? "Subtask not available in this snapshot" : "Awaiting approval").foregroundStyle(N.text3) }
                    }.nekoCard(padding: NekoLayout.rowInset, radius: NekoRadius.xl)
                }
            }
        }
        if !decisions.isEmpty {
            VStack(alignment: .leading, spacing: NekoSpace.md) {
                Text("Decisions").font(NekoFont.heading)
                ForEach(decisions, id: \.recordID) { DecisionCard(model: model, record: $0) }
            }
        }
        HStack(spacing: NekoSpace.md) {
            if !ticket["worktree"].string.isEmpty { Button("Reveal working folder", systemImage: "folder") { NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: ticket["worktree"].string) }.nekoButton(.quiet) }
            Button(showLog ? "Hide activity log" : "Show activity log · \(ticket["events"].array.count)", systemImage: "list.bullet") { withNekoAnimation(.reveal) { showLog.toggle() } }.nekoButton(.quiet)
        }.font(NekoFont.meta)
        if showLog {
            VStack(alignment: .leading, spacing: NekoSpace.md) {
                ForEach(Array(ticket["events"].array.enumerated()), id: \.offset) { _, event in eventView(event) }
            }
        }
    }
    private func action(_ label: String, _ command: String) -> some View { Button(label) { Task { await model.workbench(.command(command, ["task_id": .string(id)])) } } }
    private var stoppedCallout: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("Why it stopped", systemImage: "exclamationmark.triangle").font(NekoFont.heading).foregroundStyle(NekoStyle.amber)
            Text(TicketPresentation.stopReason(ticket)).lineSpacing(4).textSelection(.enabled)
            HStack {
                Button("Choose folder…") { chooseFolder() }
                Text(model.snapshot["task_roots"][id].string.isEmpty ? "Pick the repository this task is about, then Neko starts it." : "Working in \(URL(fileURLWithPath: model.snapshot["task_roots"][id].string).lastPathComponent). Pick another repository to start over there.")
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
        if let receipt = TicketCommandReceipt.from(event: event) {
            TicketCommandRow(receipt: receipt)
        } else {
            VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                Text(event["role"].string.capitalized).font(NekoFont.meta).foregroundStyle(N.text3)
                Text(event["message"].string).font(NekoFont.mono).foregroundStyle(N.text2).textSelection(.enabled).lineLimit(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
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

/// The agent's result: what it says it did, then the independent review with
/// the checks it ran. The full saved text, including the raw review, copies.
private struct TicketOutcomeReadback: View {
    let result: String
    let outcome: TicketPresentation.Outcome
    let events: [JSONValue]
    @State private var copied = false

    var body: some View {
        let review = TicketPresentation.review(result)
        VStack(alignment: .leading, spacing: NekoSpace.lg) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                    Text(outcome.title).font(NekoFont.title).accessibilityAddTraits(.isHeader)
                    Text(outcome.note).font(NekoFont.meta).foregroundStyle(N.text3)
                }
                Spacer()
                Button(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc") {
                    NSPasteboard.general.clearContents()
                    copied = NSPasteboard.general.setString(result, forType: .string)
                }
                .nekoButton(.quiet).font(NekoFont.meta)
                .help("Copy the full saved result, including the reviewer response")
            }
            if review.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text("No result text was saved for this task.").foregroundStyle(N.text2)
            } else {
                ReadableText(text: review.body).font(NekoFont.chat).lineSpacing(4)
            }
            if let verdict = review.verdict {
                TicketReviewCard(verdict: verdict, current: outcome.reviewIsCurrent, receipts: events.compactMap(TicketCommandReceipt.from(event:)))
            } else if outcome == .reviewing {
                Label("The independent reviewer is checking this now.", systemImage: "hourglass").font(NekoFont.meta).foregroundStyle(N.text3)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onChange(of: result) { _, _ in copied = false }
    }
}

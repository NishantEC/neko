import SwiftUI
import AppKit
import NekoKit

/// The right-hand inspector for the ticket a chat is about: what it is, where
/// it runs, what it changed and what it needs from you.
struct TicketInspector: View {
    @ObservedObject var model: AppModel
    let id: String?
    let openFull: (String) -> Void
    @State private var changes: [String] = []
    @State private var changeCounts: [String: (Int, Int)] = [:]
    @State private var loadedFor: String?
    private var ticket: JSONValue { id.flatMap { id in model.snapshot["tasks"].array.first { $0.recordID == id } } ?? .null }
    var body: some View {
        Group {
            if ticket == .null {
                VStack(spacing: 8) {
                    Image(systemName: "tray").font(.system(size: 26)).foregroundStyle(.tertiary)
                    Text("No ticket yet").font(.system(size: 13, weight: .semibold))
                    Text("When Neko proposes work in this chat, its ticket shows here.").font(.system(size: 12)).foregroundStyle(.secondary).multilineTextAlignment(.center)
                }.padding(24).frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        header("Ticket")
                        Text(ticket["title"].string).font(.system(size: 13, weight: .semibold)).fixedSize(horizontal: false, vertical: true).padding(.bottom, 6)
                        row("Status") { Text(friendlyTaskStatus(ticket["status"].string)).foregroundStyle(ticketStatusColor(ticket["status"].string)) }
                        row("Workspace") { Text(workspaceName) }
                        if !folder.isEmpty { row("Folder") { Text((folder as NSString).lastPathComponent).help(folder) } }
                        row("Agent") { Text(AgentModelCatalog.label(provider: model.snapshot["agent_runtime"]["provider"].string, model: model.snapshot["agent_runtime"]["model"].string, catalog: ModelCatalog())) }
                        if let cents = budgetCents { row("Budget") { Text(String(format: "$%.2f per ticket", Double(cents) / 100)) } }
                        if !ticket["plan"].string.isEmpty {
                            header("Plan")
                            Text(planSummary).font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        }
                        if !ticket["worktree"].string.isEmpty {
                            header("Changes")
                            if changes.isEmpty { Text(loadedFor == id ? "No changes yet" : "Loading…").font(.system(size: 12)).foregroundStyle(.secondary) }
                            ForEach(changes.prefix(12), id: \.self) { file in
                                row((file as NSString).lastPathComponent, wide: true) {
                                    if let counts = changeCounts[file] {
                                        HStack(spacing: 4) {
                                            if counts.0 > 0 { Text("+\(counts.0)").foregroundStyle(ReplyStyle.green) }
                                            if counts.1 > 0 { Text("−\(counts.1)").foregroundStyle(ReplyStyle.red) }
                                        }.monospacedDigit()
                                    }
                                }.help(file)
                            }
                            if changes.count > 12 { Text("and \(changes.count - 12) more").font(.system(size: 11)).foregroundStyle(.tertiary) }
                        }
                        if let verdict = TicketPresentation.review(ticket["result"].string).verdict {
                            header("Checks")
                            row("Review") { Text(verdict.passed ? "No findings" : "\(verdict.findings.count) findings").foregroundStyle(verdict.passed ? ReplyStyle.green : ReplyStyle.orange) }
                            row("Checks run") { Text("\(verdict.tests.count)") }
                            row("Files read") { Text("\(verdict.files.count)") }
                        }
                        row("Publish") { Text("Not pushed") }.padding(.top, 10)
                        actions.padding(.top, 16)
                    }.padding(.horizontal, 16).padding(.bottom, 16)
                }
            }
        }
        .frame(minWidth: 240, idealWidth: 272)
        .task(id: id) { await loadChanges() }
        .onChange(of: ticket["status"].string) { _, _ in Task { await loadChanges() } }
    }
    private var folder: String { ticket["worktree"].string.isEmpty ? (model.snapshot["task_roots"][ticket.recordID].string) : ticket["worktree"].string }
    private var workspaceName: String { model.workspaces.first { $0.recordID == ticket["workspace_id"].string }?["name"].string ?? "Workspace" }
    private var budgetCents: Int? { model.snapshot["task_budget_cents"] == .null ? nil : model.snapshot["task_budget_cents"].int }
    private var planSummary: String {
        let lines = ticket["plan"].string.components(separatedBy: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        return lines.prefix(6).joined(separator: "\n") + (lines.count > 6 ? "\n…" : "")
    }
    @ViewBuilder private var actions: some View {
        let status = ticket["status"].string
        VStack(spacing: 8) {
            switch status {
            case "AwaitingApproval": wide("Approve Plan", "ApproveTask", prominent: true)
            case "ReadyForReview": wide("Mark Complete", "CompleteTask", prominent: true)
            case "Failed", "Cancelled": wide("Retry", "RetryTask", prominent: true)
            default: EmptyView()
            }
            HStack(spacing: 8) {
                Button { if let id { openFull(id) } } label: { Text("Open Ticket").frame(maxWidth: .infinity) }
                if !folder.isEmpty {
                    Button { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: folder)]) } label: { Text("Show in Finder").frame(maxWidth: .infinity) }
                }
            }
            if !["Completed", "Cancelled", "Failed"].contains(status) {
                Button(role: .destructive) { run("CancelTask") } label: { Text("Cancel Ticket").frame(maxWidth: .infinity) }.buttonStyle(.borderless).foregroundStyle(.red)
            }
        }.controlSize(.regular).disabled(model.busy)
    }
    private func wide(_ title: String, _ command: String, prominent: Bool) -> some View {
        Button { run(command) } label: { Text(title).frame(maxWidth: .infinity) }.buttonStyle(.borderedProminent)
    }
    private func run(_ command: String) {
        guard let id else { return }
        Task { await model.workbench(.command(command, ["task_id": .string(id)])) }
    }
    private func header(_ title: String) -> some View {
        Text(title).font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary).padding(.top, 14).padding(.bottom, 4)
    }
    private func row<Value: View>(_ label: String, wide: Bool = false, @ViewBuilder value: () -> Value) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(label).foregroundStyle(.secondary).lineLimit(1).frame(width: wide ? nil : 76, alignment: .leading)
            if wide { Spacer(minLength: 4) }
            value().lineLimit(1)
            if !wide { Spacer(minLength: 0) }
        }.font(.system(size: 12)).frame(minHeight: 22)
    }
    private func loadChanges() async {
        guard let id, !ticket["worktree"].string.isEmpty else { changes = []; loadedFor = id; return }
        guard let reply = try? await model.request(.command("TaskChanges", ["task_id": .string(id)]))["TaskChanges"] else { loadedFor = id; return }
        changes = reply["files"].array.map(\.string)
        var counts: [String: (Int, Int)] = [:]
        var current: String?
        for line in reply["patch"].string.components(separatedBy: "\n") {
            if line.hasPrefix("+++ ") { current = String(line.dropFirst(4)).replacingOccurrences(of: "b/", with: "", options: .anchored); continue }
            if line.hasPrefix("--- ") { continue }
            guard let file = current else { continue }
            if line.hasPrefix("+") { counts[file, default: (0, 0)].0 += 1 }
            if line.hasPrefix("-") { counts[file, default: (0, 0)].1 += 1 }
        }
        changeCounts = counts
        loadedFor = id
    }
}


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
                    Image(systemName: "tray").font(NekoFont.of(.headline)).foregroundStyle(.tertiary)
                    Text("No ticket yet").font(NekoFont.heading)
                    Text("When Neko proposes work in this chat, its ticket shows here.").font(NekoFont.meta).foregroundStyle(.secondary).multilineTextAlignment(.center)
                }.padding(24).frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        header("Ticket")
                        Text(ticket["title"].string).font(NekoFont.heading).fixedSize(horizontal: false, vertical: true).padding(.bottom, 12)
                        row("Status") { Text(friendlyTaskStatus(ticket["status"].string)).foregroundStyle(ticketStatusColor(ticket["status"].string)) }
                        row("Workspace") { Text(workspaceName) }
                        if !folder.isEmpty { row("Folder") { Text((folder as NSString).lastPathComponent).help(folder) } }
                        row("Agent") { Text(AgentModelCatalog.label(provider: model.snapshot["agent_runtime"]["provider"].string, model: model.snapshot["agent_runtime"]["model"].string, catalog: ModelCatalog())) }
                        if let cents = budgetCents { row("Budget") { Text(String(format: "$%.2f per ticket", Double(cents) / 100)) } }
                        if !ticket["plan"].string.isEmpty {
                            header("Plan")
                            Text(planSummary).font(NekoFont.meta).foregroundStyle(.secondary).lineSpacing(4).fixedSize(horizontal: false, vertical: true)
                        }
                        if !ticket["worktree"].string.isEmpty {
                            header("Changes")
                            if changes.isEmpty { Text(loadedFor == id ? "No changes yet" : "Loading…").font(NekoFont.caption).foregroundStyle(.secondary) }
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
                            if changes.count > 12 { Text("and \(changes.count - 12) more").font(NekoFont.of(.small)).foregroundStyle(.tertiary) }
                        }
                        if let verdict = TicketPresentation.review(ticket["result"].string).verdict {
                            header("Checks")
                            row("Review") { Text(verdict.passed ? "No findings" : "\(verdict.findings.count) findings").foregroundStyle(verdict.passed ? ReplyStyle.green : ReplyStyle.orange) }
                            row("Checks run") { Text("\(verdict.tests.count)") }
                            row("Files read") { Text("\(verdict.files.count)") }
                        }
                        row("Publish") { publishValue }.padding(.top, 10)
                        deliveryToggle
                        actions.padding(.top, 16)
                    }.padding(.horizontal, 20).padding(.bottom, 24)
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
            case "Failed", "Cancelled": wide("Start again", "StartTask", prominent: true)
            default: EmptyView()
            }
            HStack(spacing: 8) {
                Button { if let id { openFull(id) } } label: { Text("Open Ticket").frame(maxWidth: .infinity) }
                if !folder.isEmpty {
                    Button { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: folder)]) } label: { Text("Show in Finder").frame(maxWidth: .infinity) }
                }
            }
            if !["Completed", "Cancelled", "Failed"].contains(status) {
                Button(role: .destructive) { run("CancelTask") } label: { Text("Cancel Ticket").frame(maxWidth: .infinity) }.nekoButton(.quiet).foregroundStyle(.red)
            }
        }.controlSize(.regular).disabled(model.busy)
    }
    /// Neko delivers a reviewed result itself once the independent review
    /// passes: a pull request, or local main for repositories set to land
    /// there. The button retries, for example after signing in to GitHub.
    @ViewBuilder private var publishValue: some View {
        let link = ticket["pull_request"]["url"].string
        let landed = ticket["landed_commit"].string
        if !landed.isEmpty {
            Text("Landed on main · \(landed.prefix(7))").help(landed)
        } else if let url = URL(string: link), !link.isEmpty {
            Link("Pull request #\(url.lastPathComponent)", destination: url).help(link)
        } else if ["ReadyForReview", "Completed"].contains(ticket["status"].string) {
            Button(landsOnMain ? "Land on main" : "Open pull request") { run("PublishTask") }.buttonStyle(.link).disabled(model.busy)
        } else {
            Text("Not delivered")
        }
    }
    /// The ticket's own repository checkout, where delivery is configured.
    private var repository: String { model.snapshot["task_roots"][ticket.recordID].string }
    private var landsOnMain: Bool {
        guard !repository.isEmpty else { return false }
        let resolved = URL(fileURLWithPath: repository).resolvingSymlinksInPath().path
        return model.snapshot["land_on_main"].array.contains { $0.string == resolved || $0.string == repository }
    }
    @ViewBuilder private var deliveryToggle: some View {
        if !repository.isEmpty {
            row("Delivery") {
                Toggle("Land on main", isOn: Binding(get: { landsOnMain }, set: { enabled in
                    Task { await model.workbench(.command("SetLandOnMain", ["repository": .string(repository), "enabled": .bool(enabled)])) }
                }))
                .toggleStyle(.switch).controlSize(.mini).disabled(model.busy)
                .help("On: reviewed tickets in \((repository as NSString).lastPathComponent) fast-forward your local main, nothing pushed. Off: Neko opens a pull request.")
            }
        }
    }
    private func wide(_ title: String, _ command: String, prominent: Bool) -> some View {
        Button { run(command) } label: { Text(title).frame(maxWidth: .infinity) }.nekoButton(.primary)
    }
    private func run(_ command: String) {
        guard let id else { return }
        Task { await model.workbench(.command(command, ["task_id": .string(id)])) }
    }
    private func header(_ title: String) -> some View {
        Text(title).font(NekoFont.meta.weight(.medium)).foregroundStyle(.secondary).padding(.top, 24).padding(.bottom, 10)
    }
    private func row<Value: View>(_ label: String, wide: Bool = false, @ViewBuilder value: () -> Value) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(label).foregroundStyle(.secondary).lineLimit(1).frame(width: wide ? nil : 76, alignment: .leading)
            if wide { Spacer(minLength: 4) }
            value().lineLimit(1)
            if !wide { Spacer(minLength: 0) }
        }.font(NekoFont.meta).frame(minHeight: 28)
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

import SwiftUI
import NekoKit

/// Every pull request Neko opened, as one table of decisions: what is ready
/// to merge, what its agent says to close, what needs your call, and what is
/// still being worked on. Neko follows each one on GitHub
/// (`neko_core::pull_requests`); merging and closing wait for your click.
struct PullRequestRow: Identifiable, Equatable {
    enum Verdict: Int, CaseIterable, Comparable {
        case ready, close, needsYou, merging, working, unchecked, merged, closed

        static func < (a: Verdict, b: Verdict) -> Bool { a.rawValue < b.rawValue }

        var title: String {
            switch self {
            case .ready: "Ready to merge"
            case .close: "Suggested to close"
            case .needsYou: "Needs your call"
            case .merging: "Merging when checks pass"
            case .working: "Being updated"
            case .unchecked: "Not checked on GitHub yet"
            case .merged: "Merged"
            case .closed: "Closed"
            }
        }
        var tone: NekoTone {
            switch self {
            case .ready, .merged: .done
            case .close: .failed
            case .needsYou: .attention
            case .merging, .working: .running
            case .closed, .unchecked: .neutral
            }
        }
        var color: Color { tone.color }
        /// Every verdict colour comes with a symbol, so colour is never the only cue.
        var symbol: String {
            switch self {
            case .ready: "checkmark.circle"
            case .close, .closed: "xmark.circle"
            case .needsYou: "hand.raised"
            case .merging, .merged: "arrow.triangle.merge"
            case .working: "hammer"
            case .unchecked: "clock"
            }
        }
        /// Shown when nothing in the section is waiting, so it never reads as broken.
        var isDecision: Bool { [.ready, .close, .needsYou].contains(self) }
    }

    let id: String
    let title: String
    let url: String
    /// "#4618".
    let number: String
    let repository: String?
    let verdict: Verdict
    /// Why: the agent's reason to close, the question, or what is blocking.
    let reason: String?
    /// "Checks passing", "Checks failing: Test and Lint", "Checks running".
    let checks: String?
    let review: String?
    let conflicts: Bool
    /// Branch protection wants it up to date with its base first.
    let behind: Bool
    let draft: Bool
    let updated: Int
    /// The branch it merges into ("main").
    let base: String
    let checksFailing: Bool

    /// What it is, on one quiet line. Problems get their own line instead.
    var meta: String {
        [number.isEmpty ? nil : number, repository, review, draft ? "Draft" : nil, checksFailing ? nil : checks]
            .compactMap { $0 }.joined(separator: " · ")
    }

    /// What stands between it and merging, each with its own symbol.
    var problems: [(symbol: String, text: String)] {
        var out: [(String, String)] = []
        if checksFailing, let checks { out.append(("xmark.octagon", checks)) }
        if conflicts { out.append(("arrow.triangle.branch", "Conflicts with \(base)")) }
        if behind { out.append(("arrow.down.circle", "Behind \(base)")) }
        return out
    }

    /// "2 ready · 1 to close", only the parts that are not zero.
    static func subtitle(_ rows: [PullRequestRow]) -> String {
        let parts = [(Verdict.ready, "ready"), (.close, "to close"), (.needsYou, "need you")].compactMap { verdict, label -> String? in
            let count = rows.filter { $0.verdict == verdict }.count
            return count > 0 ? "\(count) \(label)" : nil
        }
        return parts.isEmpty ? "Nothing waiting on you" : parts.joined(separator: " · ")
    }

    init(task: JSONValue, snapshot: JSONValue) {
        let pr = task["pull_request"]
        let status = pr["status"]
        let text = TaskText(task: task, snapshot: snapshot)
        id = task.recordID
        title = text.title
        url = pr["url"].string
        number = text.pullRequest ?? ""
        repository = text.repository
        updated = task["updated_at_ms"].int
        conflicts = status["merge_state"].string == "DIRTY" || status["mergeable"].string == "CONFLICTING"
        base = status["base"].string.isEmpty ? "main" : status["base"].string
        checksFailing = status["checks"].string == "failing"
        let alsoOnBase = status["base_failing"].bool ? " (also failing on \(base))" : ""
        checks = switch status["checks"].string {
        case "passing": "Checks passing"
        case "failing": "Checks failing" + (status["failing"].array.isEmpty ? "" : ": " + status["failing"].array.map(\.string).joined(separator: ", ")) + alsoOnBase
        case "pending": "Checks running"
        case "none": "No checks"
        default: nil
        }
        behind = status["merge_state"].string == "BEHIND"
        draft = status["draft"].bool
        review = switch status["review"].string {
        case "APPROVED": "Approved"
        case "CHANGES_REQUESTED": "Changes requested"
        case "REVIEW_REQUIRED": "Review required"
        default: nil
        }
        let state = status["state"].string
        let taskStatus = task["status"].string
        let suggestion = pr["close_suggestion"].string
        switch true {
        case state == "MERGED": verdict = .merged; reason = nil
        case state == "CLOSED": verdict = .closed; reason = nil
        case ["Queued", "Planning", "Building", "Reviewing"].contains(taskStatus): verdict = .working; reason = nil
        // Until Neko has read it, it may already be merged or closed: no verdict.
        case state.isEmpty: verdict = .unchecked; reason = "Neko reads it on GitHub within a few minutes."
        case pr["merge_when_ready"].bool:
            verdict = .merging
            let waiting = pr["follow_up"]["merge_note"].string
            reason = waiting.isEmpty ? nil : "Waiting: " + waiting
        case !suggestion.isEmpty: verdict = .close; reason = suggestion
        case taskStatus == "Cancelled": verdict = .close; reason = "Its task was cancelled here, but the pull request is still open."
        case ["AwaitingApproval", "Failed"].contains(taskStatus):
            verdict = .needsYou
            reason = taskStatus == "Failed" ? TicketPresentation.stopReason(task) : TicketPresentation.waitingReason(task)
        default: verdict = .ready; reason = nil
        }
    }

    /// Every task with a pull request, decisions first.
    static func rows(snapshot: JSONValue, tasks: [JSONValue]) -> [PullRequestRow] {
        tasks.filter { !$0["pull_request"]["url"].string.isEmpty }
            .map { PullRequestRow(task: $0, snapshot: snapshot) }
            .sorted { ($0.verdict, -$0.updated) < ($1.verdict, -$1.updated) }
    }
}

struct PullRequestsView: View {
    @ObservedObject var model: AppModel
    @State private var closing: PullRequestRow?
    @State private var closeReason = ""
    @State private var confirmReview = false
    @State private var confirmMergeAll = false
    @State private var showFinished = false

    private var rows: [PullRequestRow] { PullRequestRow.rows(snapshot: model.snapshot, tasks: model.tasks) }
    private var ready: [PullRequestRow] { rows.filter { $0.verdict == .ready } }
    private var reviewable: Int { rows.filter { [.ready, .close].contains($0.verdict) }.count }

    var body: some View {
        let grouped = Dictionary(grouping: rows, by: \.verdict)
        let shown = PullRequestRow.Verdict.allCases.filter { verdict in
            !(grouped[verdict] ?? []).isEmpty && (showFinished || ![.merged, .closed].contains(verdict))
        }
        List {
            Text("Neko checks each pull request on GitHub every 3 minutes. Failing checks, conflicts and review comments go back to the task's agent; merging and closing wait for you.")
                .font(NekoFont.meta).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
                .padding(.bottom, NekoSpace.md)
                .listRowSeparator(.hidden)
            ForEach(shown, id: \.self) { verdict in
                let items = grouped[verdict] ?? []
                Section {
                    ForEach(items) { row($0).listRowSeparator(.hidden) }
                } header: {
                    NekoListHeader(title: verdict.title, tone: verdict.tone, count: items.count, spaced: verdict != shown.first)
                }
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .contentMargins(.horizontal, NekoLayout.pageInset, for: .scrollContent)
        .contentMargins(.vertical, NekoSpace.md, for: .scrollContent)
        .frame(maxWidth: NekoLayout.pageWidth).frame(maxWidth: .infinity)
        .overlay {
            if rows.isEmpty {
                NekoEmptyState("When a task's work passes its independent review, Neko opens a pull request and it appears here.",
                               title: "No pull requests yet", symbol: "arrow.triangle.pull", block: true)
            }
        }
        .navigationTitle("Pull requests")
        .navigationSubtitle(PullRequestRow.subtitle(rows))
        .toolbar { toolbar }
        .alert("Close \(closing?.number ?? "the pull request")?", isPresented: Binding(get: { closing != nil }, set: { if !$0 { closing = nil } })) {
            TextField("Why (posted as a comment)", text: $closeReason)
            Button("Close pull request", role: .destructive) {
                if let row = closing { send("ClosePullRequest", row, ["comment": .string(closeReason)], done: "Closing \(row.number). Its task is cancelled once GitHub confirms.") }
                closing = nil
            }.disabled(closeReason.trimmingCharacters(in: .whitespaces).isEmpty)
            Button("Cancel", role: .cancel) { closing = nil }
        } message: {
            Text("The comment goes on the pull request, so the next person knows why.")
        }
        .confirmationDialog("Re-check \(reviewable) open pull requests?", isPresented: $confirmReview) {
            Button("Re-check them") {
                Task {
                    if await model.workbench(.command("ReviewPullRequests", ["workspace_id": model.selectedWorkspace.map { .string($0) } ?? .null])) {
                        model.notice = "Each agent is re-checking its pull request. They run a few at a time, so this takes a while."
                    }
                }
            }
        } message: {
            Text("Each task's agent updates its pull request with main, re-checks the cause against the evidence, cuts it to the smallest fix, and says whether it is ready, should be closed, or needs your decision. Agents run a few at a time per repository.")
        }
        .confirmationDialog("Merge \(ready.count) pull requests?", isPresented: $confirmMergeAll) {
            Button("Merge \(ready.count)") {
                Task {
                    for row in ready { await model.workbench(.command("SetMergeWhenReady", ["task_id": .string(row.id), "enabled": .bool(true)])) }
                    model.notice = "Neko merges each one as soon as its checks pass."
                }
            }
        } message: {
            Text("Each is squashed with an empty body once its checks pass and nothing conflicts. A failure or a conflict stops that one and tells you.")
        }
    }

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            Button("Re-check all") { confirmReview = true }
                .disabled(reviewable == 0)
                .help("Have each agent re-check its open pull request and give a verdict")
            if !ready.isEmpty {
                // The page's one primary action; gone rather than disabled when there is nothing to merge.
                Button("Merge \(ready.count) ready") { confirmMergeAll = true }
                    .nekoButton(.primary)
                    .help("Merge every pull request in Ready to merge once its checks pass")
            }
            Menu {
                Button("Check GitHub now") { Task { await model.workbench(.command("CheckPullRequests")) } }
                Toggle("Show merged and closed", isOn: $showFinished)
                Divider()
                Section("Merge without an approving review") {
                    ForEach(repositories, id: \.path) { repo in
                        Toggle(repo.name, isOn: Binding(get: { repo.allowed }, set: { value in
                            Task { await model.workbench(.command("SetMergeWithoutReview", ["repository": .string(repo.path), "enabled": .bool(value)])) }
                        }))
                    }
                }
            } label: {
                Image(systemName: "ellipsis.circle")
                    .frame(width: NekoControlMetrics.height(), height: NekoControlMetrics.height())
                    .contentShape(Rectangle())
            }
            .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
            .help("Pull request options")
        }
    }

    /// Repositories with a pull request here, and whether Neko may merge
    /// them without the approving review GitHub requires.
    private var repositories: [(name: String, path: String, allowed: Bool)] {
        let allowed = Set(model.snapshot["merge_without_review"].array.map(\.string))
        let paths = Set(model.tasks.filter { !$0["pull_request"]["url"].string.isEmpty }.map { model.snapshot["task_roots"][$0.recordID].string }.filter { !$0.isEmpty })
        return paths.sorted().map { (URL(fileURLWithPath: $0).lastPathComponent, $0, allowed.contains($0)) }
    }

    private func row(_ row: PullRequestRow) -> some View {
        HStack(alignment: .top, spacing: NekoSpace.md) {
            Image(systemName: row.verdict.symbol).font(NekoFont.body).foregroundStyle(row.verdict.color)
                .frame(width: NekoLayout.listGlyphSlot)
                .accessibilityLabel(row.verdict.title)
            VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                // The title opens the task; the actions stay sibling buttons, never nested inside it.
                Button { model.openAgent(row.id, from: .pullRequests) } label: {
                    Text(verbatim: row.title).font(NekoFont.of(.callout, weight: .medium)).foregroundStyle(N.text)
                        .lineLimit(2).multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Open the task for \(row.number.isEmpty ? "this pull request" : row.number)")
                .accessibilityHint("Opens the task")
                if !row.meta.isEmpty {
                    Text(row.meta).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(1)
                }
                if !row.problems.isEmpty {
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: NekoSpace.xs) { problemLabels(row) }
                        VStack(alignment: .leading, spacing: NekoSpace.xxs) { problemLabels(row) }
                    }
                }
                if let reason = row.reason, !reason.isEmpty {
                    Text(reason).font(NekoFont.meta).foregroundStyle(N.text2).lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true).help(reason)
                }
            }
            Spacer(minLength: NekoSpace.lg)
            actions(row)
        }
        .padding(.vertical, NekoSpace.sm)
        .contextMenu {
            Button("Open on GitHub") { open(row.url) }
            Button("Open task") { model.openAgent(row.id, from: .pullRequests) }
        }
    }

    @ViewBuilder private func problemLabels(_ row: PullRequestRow) -> some View {
        ForEach(row.problems, id: \.text) { problem in
            NekoStatusLabel(tone: .attention, symbol: problem.symbol, text: problem.text)
        }
    }

    /// One prominent button per page (the toolbar's Merge): row actions stay
    /// secondary, so a list of ready pull requests isn't a wall of blue.
    @ViewBuilder private func actions(_ row: PullRequestRow) -> some View {
        HStack(spacing: NekoSpace.sm) {
            switch row.verdict {
            case .ready:
                closeButton(row).nekoButton(.quiet)
                mergeButton(row).nekoButton(.secondary)
            case .close:
                mergeButton(row).nekoButton(.quiet)
                closeButton(row).nekoButton(.secondary)
            case .merging:
                Button("Cancel merge") { send("SetMergeWhenReady", row, ["enabled": .bool(false)], done: "Won't merge \(row.number).") }
                    .nekoButton(.secondary).accessibilityLabel("Cancel merging \(row.number)")
            case .needsYou, .working, .unchecked, .merged, .closed:
                Button("Open task") { model.openAgent(row.id, from: .pullRequests) }.nekoButton(.secondary)
                    .accessibilityLabel("Open task for \(row.number)")
            }
            Button { open(row.url) } label: { Image(systemName: "arrow.up.right.square") }
                .nekoButton(.quiet).help("Open \(row.number) on GitHub").accessibilityLabel("Open \(row.number) on GitHub")
        }
        .fixedSize()
    }

    private func mergeButton(_ row: PullRequestRow) -> some View {
        Button("Merge") { send("SetMergeWhenReady", row, ["enabled": .bool(true)], done: "Merging \(row.number) once its checks pass.") }
            .accessibilityLabel("Merge \(row.number)")
    }
    private func closeButton(_ row: PullRequestRow) -> some View {
        Button("Close…") { startClosing(row) }.accessibilityLabel("Close \(row.number)…")
    }
    private func startClosing(_ row: PullRequestRow) {
        closeReason = row.reason ?? ""
        closing = row
    }
    private func send(_ name: String, _ row: PullRequestRow, _ fields: [String: JSONValue], done: String) {
        var fields = fields
        fields["task_id"] = .string(row.id)
        Task { if await model.workbench(.command(name, fields)) { model.notice = done } }
    }
    private func open(_ url: String) {
        if let link = URL(string: url) { NSWorkspace.shared.open(link) }
    }
}

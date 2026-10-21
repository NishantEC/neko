import SwiftUI
import AppKit
import NekoKit

// The pieces of a ticket page, laid out like a Codex task: what was asked,
// what the agent did, the checks it ran, the files it changed, what the
// reviewer concluded and where the work went. Everything a reviewer needs is
// on the page; only a single command's output or a long request opens on click.

/// The ticket's request: the issue it was filed from, as your opening turn.
/// A long request is clamped by height, not by lines: a line limit applies to
/// each paragraph separately, so many short paragraphs never clamped at all.
struct TicketRequestBlock: View {
    let text: String
    let createdAt: Int
    @State private var full = false
    private var long: Bool { text.count > 600 || text.components(separatedBy: .newlines).count > 10 }
    var body: some View {
        let clamped = long && !full
        VStack(alignment: .trailing, spacing: NekoSpace.xxs) {
            ChatBubble(timestamp: createdAt, label: "Request", spoken: text, note: "Request ·", noteSymbol: nil) {
                ReadableText(text: text)
                    .frame(maxHeight: clamped ? NekoChat.clampHeight : nil, alignment: .top)
                    .clipped()
                    .mask {
                        VStack(spacing: 0) {
                            Color.black
                            LinearGradient(colors: [.black, .black.opacity(clamped ? 0 : 1)], startPoint: .top, endPoint: .bottom)
                                .frame(height: NekoChat.clampFade)
                        }
                    }
            }
            if long {
                Button(full ? "Show less" : "Show full request") { withNekoAnimation(.reveal) { full.toggle() } }
                    .nekoButton(.quiet).font(NekoFont.meta)
            }
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
}

/// One command the agent ran: its outcome and command on one line; the
/// output opens in place when clicked.
struct TicketCommandRow: View {
    let receipt: TicketCommandReceipt
    @State private var open = false
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.xs) {
            Button { withNekoAnimation(.reveal) { open.toggle() } } label: {
                HStack(alignment: .firstTextBaseline, spacing: NekoSpace.sm) {
                    Image(systemName: symbol).foregroundStyle(tint).font(NekoFont.of(.caption, weight: .semibold)).frame(width: 14).accessibilityHidden(true)
                    Text(receipt.title).font(NekoFont.mono).foregroundStyle(N.text2).lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: NekoSpace.sm)
                    if let code = receipt.exitCode, code != 0 { Text("exit \(code)").font(NekoFont.mono).foregroundStyle(NekoStyle.coral) }
                }.contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(receipt.title), \(receipt.status)")
            .help(receipt.command ?? receipt.title)
            if open {
                Text(receipt.output.map { $0.isEmpty ? "No output." : $0 } ?? "Output was not saved.")
                    .font(NekoFont.mono).foregroundStyle(N.text2).textSelection(.enabled)
                    .lineLimit(40)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(NekoSpace.md)
                    .nekoFill(N.panel, radius: NekoRadius.md)
                    .padding(.leading, NekoSpace.xxl - NekoSpace.xxs)
            }
        }
    }
    private var symbol: String {
        if receipt.isIncomplete { return "questionmark.circle" }
        return receipt.failed ? "xmark.circle.fill" : "checkmark.circle.fill"
    }
    private var tint: Color {
        if receipt.isIncomplete { return N.text4 }
        return receipt.failed ? NekoStyle.coral : NekoStyle.mint
    }
}

/// What one agent ran during a stretch of work, folded into one line. The
/// newest stretch starts open on its latest commands; the rest are a click away.
struct TicketWorkLog: View {
    let author: String
    let commands: [TicketCommandReceipt]
    @State private var open: Bool
    @State private var all = false
    private let recent = 3

    init(author: String, commands: [TicketCommandReceipt], startsOpen: Bool = false) {
        self.author = author
        self.commands = commands
        _open = State(initialValue: startsOpen)
    }

    static func label(author: String, commands: [TicketCommandReceipt]) -> String {
        let failed = commands.filter(\.failed).count
        return "\(author) ran \(commands.count) \(commands.count == 1 ? "command" : "commands")" + (failed > 0 ? " · \(failed) failed" : "")
    }

    var body: some View {
        ChatWorkFold(label: Self.label(author: author, commands: commands), failed: commands.contains(where: \.failed), open: $open) {
            let hidden = all ? 0 : max(commands.count - recent, 0)
            VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                if hidden > 0 {
                    Button("Show \(hidden) earlier") { withNekoAnimation(.reveal) { all = true } }
                        .nekoButton(.quiet).font(NekoFont.meta)
                }
                // Keyed by position inside this stretch, which has a stable id
                // of its own: commands only append, so an opened row stays open.
                ForEach(Array(commands.enumerated().dropFirst(hidden)), id: \.offset) { _, receipt in
                    TicketCommandRow(receipt: receipt).frame(minHeight: NekoChat.commandRow)
                }
            }
            .padding(.leading, NekoSpace.lg)
        }
    }
}

/// What the ticket changed in its own worktree, loaded with the page.
struct TicketFilesChanged: View {
    @ObservedObject var model: AppModel
    let id: String
    @State private var changes: (files: [String], patch: String, truncated: Bool)?
    @State private var error: String?
    @State private var loading = false
    @State private var open = false
    /// One fixed-height row while it loads and until clicked: the diff loads
    /// after the page and must not push the conversation away from the reader.
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.sm) {
            HStack(spacing: NekoSpace.sm) {
                Button { if changes?.files.isEmpty == false { withNekoAnimation(.reveal) { open.toggle() } } } label: {
                    HStack(spacing: NekoSpace.xs) {
                        Image(systemName: "chevron.right").font(NekoFont.of(.micro, weight: .semibold))
                            .rotationEffect(.degrees(open ? 90 : 0)).frame(width: NekoSpace.md)
                            .opacity(changes?.files.isEmpty == false ? 1 : 0)
                        Text(heading).foregroundStyle(N.text2)
                        if let totals { Text(totals.added).foregroundStyle(NekoStyle.mint); Text(totals.removed).foregroundStyle(NekoStyle.coral) }
                        Spacer(minLength: 0)
                    }.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityValue(open ? "Expanded" : "Collapsed")
                Button("Reload changes", systemImage: "arrow.clockwise") { load() }
                    .labelStyle(.iconOnly).nekoButton(.quiet).disabled(loading).help("Reload changes")
            }
            .font(NekoFont.meta.monospacedDigit())
            .frame(height: NekoSpace.xxl + NekoSpace.xl)
            .padding(.horizontal, NekoChat.blockInset)
            .background(N.panel, in: RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous))
            if let error { Text(error).font(NekoFont.meta).foregroundStyle(NekoStyle.amber) }
            if open, let changes, !changes.files.isEmpty {
                if changes.truncated { Label("The diff is too long to show in full here.", systemImage: "exclamationmark.triangle").font(NekoFont.meta).foregroundStyle(NekoStyle.amber) }
                NativeDiffBlock(code: changes.patch, openFiles: changes.files.count <= 3 ? changes.files.count : 1).id(changes.patch)
            }
        }
        .task(id: id) { load() }
    }
    private var heading: String {
        guard let changes else { return loading ? "Files changed · loading" : "Files changed" }
        if changes.files.isEmpty { return "No changes in the worktree yet" }
        return "\(changes.files.count) \(changes.files.count == 1 ? "file" : "files") changed ·"
    }
    private var totals: (added: String, removed: String)? {
        guard let patch = changes?.patch, !patch.isEmpty else { return nil }
        let lines = patch.components(separatedBy: "\n")
        let added = lines.filter { $0.hasPrefix("+") && !$0.hasPrefix("+++") }.count
        let removed = lines.filter { $0.hasPrefix("-") && !$0.hasPrefix("---") }.count
        return ("+\(added)", "−\(removed)")
    }
    private func load() {
        loading = true
        Task {
            do {
                let reply = try await model.request(.command("TaskChanges", ["task_id": .string(id)]))["TaskChanges"]
                changes = (reply["files"].array.map(\.string), reply["patch"].string, reply["truncated"].bool)
                error = nil
            } catch { self.error = error.localizedDescription }
            loading = false
        }
    }
}

/// The independent reviewer's verdict, with the checks it ran and their outcome.
struct TicketReviewCard: View {
    let verdict: TicketPresentation.Review
    /// False when the recorded verdict predates the ticket's current attempt.
    let current: Bool
    let receipts: [TicketCommandReceipt]
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.md) {
            Label(verdict.passed ? "Independent review passed" : "Independent review found issues",
                  systemImage: verdict.passed ? "checkmark.shield.fill" : "exclamationmark.shield.fill")
                .font(NekoFont.heading)
                .foregroundStyle(!verdict.passed ? NekoStyle.amber : current ? NekoStyle.mint : N.text3)
            if !current { Text("Recorded for an earlier attempt.").font(NekoFont.meta).foregroundStyle(N.text3) }
            ReadableText(text: verdict.summary).lineSpacing(NekoLeading.body)
            ForEach(Array(verdict.findings.enumerated()), id: \.offset) { _, finding in
                Label { ReadableText(text: finding) } icon: { Image(systemName: "exclamationmark.circle").foregroundStyle(NekoStyle.amber) }
            }
            if !verdict.tests.isEmpty {
                VStack(alignment: .leading, spacing: NekoSpace.xs + NekoSpace.hair) {
                    Text("Checks").font(NekoFont.meta).foregroundStyle(N.text3)
                    ForEach(Array(verdict.tests.enumerated()), id: \.offset) { _, test in
                        if let receipt = Self.receipt(for: test, in: receipts) { TicketCommandRow(receipt: receipt) }
                        else {
                            HStack(spacing: NekoSpace.sm) {
                                Image(systemName: "questionmark.circle").foregroundStyle(N.text4).frame(width: 14).accessibilityHidden(true)
                                Text(test).font(NekoFont.mono).foregroundStyle(N.text2).lineLimit(1).truncationMode(.middle)
                            }.help("Reported by the reviewer; no saved run matches it exactly.")
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(NekoChat.blockInset).nekoFill(N.panel, radius: NekoRadius.lg)
    }
    /// The latest saved run of a check the reviewer reported.
    static func receipt(for test: String, in receipts: [TicketCommandReceipt]) -> TicketCommandReceipt? {
        let wanted = test.trimmingCharacters(in: .whitespacesAndNewlines)
        return receipts.last { receipt in
            guard let command = receipt.command?.trimmingCharacters(in: .whitespacesAndNewlines) else { return false }
            return command == wanted || command.hasSuffix("&& " + wanted)
        }
    }
}

/// Where the reviewed work went: a pull request, or a commit on local main.
struct TicketDeliveryCard: View {
    let ticket: JSONValue
    var body: some View {
        let pr = ticket["pull_request"]["url"].string
        let landed = ticket["landed_commit"].string
        if !pr.isEmpty || !landed.isEmpty {
            HStack(spacing: NekoSpace.md) {
                Image(systemName: pr.isEmpty ? "arrow.triangle.merge" : "arrow.triangle.pull").font(NekoFont.of(.subtitle)).foregroundStyle(NekoStyle.accent).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: NekoSpace.hair) {
                    Text(pr.isEmpty ? "Landed on main" : "Pull request \(Self.number(pr))").font(NekoFont.heading)
                    Text(pr.isEmpty ? String(landed.prefix(10)) : Self.repository(pr)).font(NekoFont.meta).foregroundStyle(N.text3).textSelection(.enabled)
                }
                Spacer()
                if let url = URL(string: pr), !pr.isEmpty {
                    Button("Open on GitHub", systemImage: "arrow.up.right") { NSWorkspace.shared.open(url) }.nekoButton(.secondary)
                }
            }
            .padding(NekoChat.blockInset).nekoFill(N.panel, radius: NekoRadius.lg)
        }
    }
    static func number(_ url: String) -> String { url.split(separator: "/").last.map { "#\($0)" } ?? "" }
    static func repository(_ url: String) -> String {
        let parts = url.split(separator: "/")
        return parts.count >= 4 ? "\(parts[2])/\(parts[3])" : url
    }
}

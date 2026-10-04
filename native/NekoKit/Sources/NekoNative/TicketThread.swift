import SwiftUI
import NekoKit

/// A ticket read as a conversation with its agent: your brief and replies,
/// what the agent said, its work steps folded together, and status lines.
enum TicketThread {
    enum Item: Equatable {
        case brief(String)
        case you(String)
        case agent(role: String, text: String)
        case steps(role: String, commands: [String])
        case status(String)
    }

    static func agentName(_ role: String) -> String {
        switch role {
        case "scout", "supervisor", "splitter": "Planner"
        case "builder": "Builder"
        case "reviewer": "Reviewer"
        case "learning": "Memory"
        default: "Neko"
        }
    }

    /// Events Neko records for itself; the thread shows their outcome elsewhere.
    private static func isPlumbing(_ message: String) -> Bool {
        message.hasPrefix("Command ") || message.hasPrefix("Advisory:") || message.hasPrefix("USAGE ")
            || message == "Updating workspace files" || message.hasPrefix("{") || message.hasPrefix("Decision: ")
            || message.hasPrefix("Needs your input before building:") || message.hasPrefix("Nothing to build:")
            || message == "Agent started in an isolated task worktree"
    }

    static func items(_ task: JSONValue) -> [Item] {
        var items: [Item] = []
        let goal = task["goal"].string.trimmingCharacters(in: .whitespacesAndNewlines)
        if !goal.isEmpty { items.append(.brief(goal)) }
        // The plan and result appear as their own cards; skip their echoes.
        let echoes = [task["plan"].string, task["result"].string].map { String($0.trimmingCharacters(in: .whitespacesAndNewlines).prefix(160)) }.filter { !$0.isEmpty }
        for event in task["events"].array {
            let role = event["role"].string
            let message = event["message"].string.trimmingCharacters(in: .whitespacesAndNewlines)
            if message.isEmpty { continue }
            if message.hasPrefix("VERIFICATION_COMMAND ") {
                let receipt = try? JSONDecoder().decode(JSONValue.self, from: Data(message.dropFirst("VERIFICATION_COMMAND ".count).utf8))
                let command = shortCommand(receipt?["command"].string ?? "")
                if case .steps(let r, var commands)? = items.last, r == role {
                    commands.append(command)
                    items[items.count - 1] = .steps(role: role, commands: commands)
                } else {
                    items.append(.steps(role: role, commands: [command]))
                }
                continue
            }
            if isPlumbing(message) { continue }
            switch role {
            case "note": items.append(.you(message))
            case "user", "system": items.append(.status(message))
            default:
                if echoes.contains(where: { message.hasPrefix($0) || $0.hasPrefix(String(message.prefix(160))) }) { continue }
                if message.hasPrefix("Working in ") || message == "Plan ready." || message.hasPrefix("Investigation complete.") {
                    items.append(.status(message))
                } else {
                    items.append(.agent(role: role, text: message))
                }
            }
        }
        return items
    }

    /// "/bin/zsh -lc 'rg -n foo'" reads as "rg -n foo".
    static func shortCommand(_ command: String) -> String {
        var text = command
        for shell in ["/bin/zsh -lc ", "/bin/bash -lc ", "bash -lc ", "zsh -lc "] where text.hasPrefix(shell) {
            text = String(text.dropFirst(shell.count))
            if let first = text.first, first == "'" || first == "\"", text.last == first { text = String(text.dropFirst().dropLast()) }
        }
        text = text.replacingOccurrences(of: "/Library/Developer/CommandLineTools/usr/bin/", with: "")
        return String(text.split(separator: "\n").first ?? "").prefix(140).description
    }
}

struct TicketThreadView: View {
    let ticket: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            ForEach(Array(TicketThread.items(ticket).enumerated()), id: \.offset) { _, item in row(item) }
            if let working = workingLine { workingRow(working) }
        }
    }

    private var workingLine: String? {
        switch ticket["status"].string {
        case "Queued": "Preparing this agent…"
        case "Planning": "Planner is reading the code…"
        case "Building": "Builder is making the change in its own copy…"
        case "Reviewing": "Reviewer is checking the change…"
        default: nil
        }
    }

    @ViewBuilder private func row(_ item: TicketThread.Item) -> some View {
        switch item {
        case .brief(let text):
            VStack(alignment: .leading, spacing: 6) {
                Label("Ticket", systemImage: "ticket").font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)
                ReadableText(text: text).font(.system(size: 13))
            }
            .padding(12).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        case .you(let text):
            HStack {
                Spacer(minLength: 60)
                Text(text).font(.system(size: 13)).textSelection(.enabled)
                    .padding(.horizontal, 12).padding(.vertical, 8)
                    .background(NekoStyle.accent.opacity(0.22), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            }
        case .agent(let role, let text):
            VStack(alignment: .leading, spacing: 4) {
                Text(TicketThread.agentName(role)).font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)
                ReadableText(text: text).font(.system(size: 13))
            }
        case .steps(let role, let commands):
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(commands.enumerated()), id: \.offset) { _, command in
                        Label(command, systemImage: "terminal").font(.system(size: 11, design: .monospaced)).foregroundStyle(.secondary).lineLimit(1).textSelection(.enabled)
                    }
                }.padding(.top, 4)
            } label: {
                Text("\(TicketThread.agentName(role)) ran \(commands.count) \(commands.count == 1 ? "step" : "steps")").font(.system(size: 12)).foregroundStyle(.secondary)
            }
        case .status(let text):
            HStack(spacing: 6) {
                Rectangle().fill(Color.primary.opacity(0.1)).frame(height: 1).frame(maxWidth: 24)
                Text(text).font(.system(size: 11)).foregroundStyle(.tertiary).lineLimit(2)
            }
        }
    }

    private func workingRow(_ text: String) -> some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small)
            Text(text).font(.system(size: 12)).foregroundStyle(.secondary)
        }
    }
}

/// Reply box pinned under a ticket's conversation.
struct TicketComposer: View {
    @ObservedObject var model: AppModel
    let id: String
    let status: String
    private var draft: Binding<String> {
        Binding(get: { model.ticketDrafts[id] ?? "" }, set: { model.ticketDrafts[id] = $0 })
    }
    private var text: String { model.ticketDrafts[id] ?? "" }
    private var sendLabel: String { status == "ReadyForReview" ? "Send feedback & rebuild" : "Send to agent" }
    private var placeholder: String {
        switch status {
        case "AwaitingApproval": "Answer the agent, or tell it what to change…"
        case "ReadyForReview": "Ask for changes…"
        case "Failed", "Cancelled": "Tell the agent how to try again…"
        case "Completed": "Reopen with a follow-up…"
        default: "Add direction; the agent reads it at its next step…"
        }
    }
    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField(placeholder, text: draft, axis: .vertical)
                .textFieldStyle(.plain).font(.system(size: 13)).lineLimit(1...6)
                .padding(.horizontal, 12).padding(.vertical, 9)
                .background(Color.primary.opacity(0.05), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color.primary.opacity(0.1)))
                .onSubmit(send)
            Button(sendLabel, systemImage: "arrow.up", action: send)
                .buttonStyle(.borderedProminent)
                .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.busy)
                .help(sendLabel + " (Return)")
        }
        .padding(12)
    }
    private func send() {
        let submittedDraft = text
        let reply = submittedDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !reply.isEmpty else { return }
        Task { if await model.workbench(.command("ReplyToTask", ["task_id": .string(id), "text": .string(reply)])) { if model.ticketDrafts[id] == submittedDraft { model.ticketDrafts[id] = "" } } }
    }
}

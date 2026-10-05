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
                ReadableText(text: text).font(.system(size: 13)).lineSpacing(3)
            }
            .padding(12).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        case .you(let text):
            HStack {
                Spacer(minLength: 60)
                Group {
                    if text.contains("![") { ReadableText(text: text) }
                    else { Text(text).textSelection(.enabled) }
                }.font(.system(size: 13)).lineSpacing(3)
                    .padding(.horizontal, 12).padding(.vertical, 8)
                    .background(NekoStyle.accent.opacity(0.22), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            }
        case .agent(let role, let text):
            VStack(alignment: .leading, spacing: 4) {
                Text(TicketThread.agentName(role)).font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)
                ReadableText(text: text).font(.system(size: 13)).lineSpacing(3)
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
                Text(text).font(.system(size: 13)).foregroundStyle(.secondary).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func workingRow(_ text: String) -> some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small)
            Text(text).font(.system(size: 13)).foregroundStyle(.secondary)
        }
    }
}

/// Reply box pinned under a ticket's conversation.
struct TicketComposer: View {
    @ObservedObject var model: AppModel
    let id: String
    let status: String
    private var sending: Bool { model.sendingTicketIDs.contains(id) }
    private var running: Bool { ["Queued", "Planning", "Building", "Reviewing"].contains(status) }
    private var submission: ComposerDraftStore<String>.Submission { model.ticketDrafts.submission(for: id) }
    private var payloadError: String? { ComposerPayload.validationError(submission.text, limit: 2048) }
    private var composerState: ComposerActionState {
        ComposerActionState(running: running, hasContent: !submission.text.isEmpty, submitting: sending,
                            blocked: model.busy || payloadError != nil, queues: false)
    }
    private var placeholder: String {
        switch status {
        case "AwaitingApproval": "Answer the agent, or tell it what to change…"
        case "ReadyForReview": "Ask a question or request changes…"
        case "Failed", "Cancelled": "Tell the agent how to try again…"
        case "Completed": "Reopen with a follow-up…"
        default: "Send new direction; the agent pauses and replans…"
        }
    }
    var body: some View {
        let targetID = id
        SharedComposer(
            text: Binding(get: { model.ticketDrafts.text(for: targetID) }, set: { model.ticketDrafts.set($0, for: targetID) }),
            attachments: model.ticketDrafts.attachments(for: targetID),
            placeholder: placeholder,
            accessibilityLabel: "Message to ticket agent",
            accessibilityHelp: "Return sends a reply to this agent. Shift Return adds a line. Command Return also sends a reply. New direction pauses active work and the agent reassesses it. Paste or drop images and files to attach them.",
            state: composerState, validationError: payloadError,
            onSend: send,
            onStop: { Task { await model.workbench(.command("CancelTask", ["task_id": .string(targetID)])) } },
            onAttach: { model.ticketDrafts.add($0, for: targetID) },
            onRemove: { model.ticketDrafts.remove($0, for: targetID) },
            onChooseAttachments: {
                ComposerAttachmentPicker.choose(attach: { model.ticketDrafts.add($0, for: targetID) }, onError: { model.error = $0 })
            },
            onError: { model.error = $0 }
        ) {
            ComposerRuntimeMenu(model: model)
        }.id(targetID).padding(12)
    }
    private func send() {
        guard !sending, !model.busy else { return }
        let submitted = submission
        guard !submitted.text.isEmpty else { return }
        if let error = ComposerPayload.validationError(submitted.text, limit: 2048) { model.error = error; return }
        model.sendingTicketIDs.insert(submitted.scope)
        Task {
            let saved = await model.workbench(.command("ReplyToTask", ["task_id": .string(submitted.scope), "text": .string(submitted.text)]))
            model.ticketDrafts.complete(submitted, succeeded: saved)
            model.sendingTicketIDs.remove(submitted.scope)
        }
    }
}

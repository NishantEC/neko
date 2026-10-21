import SwiftUI
import NekoKit

/// A ticket read as a conversation with its agent: your brief and replies,
/// what the agent said, its work steps folded together, and status lines.
enum TicketThread {
    enum Item: Equatable {
        case brief(String)
        case you(String)
        case agent(role: String, text: String)
        case steps(role: String, commands: [TicketCommandReceipt])
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

    static func items(_ task: JSONValue) -> [Item] { rows(task).map(\.item) }

    /// One thread item with a stable identity: the event that started it. An
    /// item keeps its id when a plan or result arrives and filters out an
    /// earlier echo, so per-row state (an opened command) stays on its row.
    struct Row: Identifiable, Equatable {
        let id: String
        let item: Item
        /// The speaker line shows only when the speaker changes.
        let showsSpeaker: Bool
        var isYours: Bool {
            switch item { case .brief, .you: true; default: false }
        }
    }

    static func rows(_ task: JSONValue) -> [Row] {
        var entries: [(start: Int, item: Item)] = []
        let goal = task["goal"].string.trimmingCharacters(in: .whitespacesAndNewlines)
        if !goal.isEmpty { entries.append((-1, .brief(goal))) }
        // The plan and result appear as their own cards; skip their echoes.
        let echoes = [task["plan"].string, task["result"].string].map { String($0.trimmingCharacters(in: .whitespacesAndNewlines).prefix(160)) }.filter { !$0.isEmpty }
        for (index, event) in task["events"].array.enumerated() {
            let role = event["role"].string
            let message = event["message"].string.trimmingCharacters(in: .whitespacesAndNewlines)
            if message.isEmpty { continue }
            if let receipt = TicketCommandReceipt.from(event: event) {
                if let last = entries.last, case .steps(let r, var commands) = last.item, r == role {
                    commands.append(receipt)
                    entries[entries.count - 1] = (last.start, .steps(role: role, commands: commands))
                } else {
                    entries.append((index, .steps(role: role, commands: [receipt])))
                }
                continue
            }
            if isPlumbing(message) { continue }
            switch role {
            case "note": entries.append((index, .you(message)))
            case "user", "system": entries.append((index, .status(message)))
            default:
                if echoes.contains(where: { message.hasPrefix($0) || $0.hasPrefix(String(message.prefix(160))) }) { continue }
                if message.hasPrefix("Working in ") || message == "Plan ready." || message.hasPrefix("Investigation complete.") {
                    entries.append((index, .status(message)))
                } else {
                    entries.append((index, .agent(role: role, text: message)))
                }
            }
        }
        var previous: String?
        return entries.map { entry in
            let speaker = speaker(entry.item)
            let shows = speaker.map { ChatRhythm.showsSpeaker($0, previous: previous) } ?? false
            if let speaker { previous = speaker }
            return Row(id: "\(kind(entry.item))#\(entry.start)", item: entry.item, showsSpeaker: shows)
        }
    }

    /// Who said an item; nil for a status line, which belongs to no one.
    static func speaker(_ item: Item) -> String? {
        switch item {
        case .brief, .you: "you"
        case .agent(let role, _), .steps(let role, _): agentName(role)
        case .status: nil
        }
    }

    /// The symbol that stands for an agent role in its speaker line.
    static func symbol(_ role: String) -> String {
        switch agentName(role) {
        case "Planner": "map"
        case "Builder": "hammer"
        case "Reviewer": "checkmark.shield"
        case "Memory": "brain"
        default: "sparkle"
        }
    }

    private static func kind(_ item: Item) -> String {
        switch item {
        case .brief: "brief"
        case .you: "you"
        case .agent: "agent"
        case .steps: "steps"
        case .status: "status"
        }
    }

    /// "/bin/zsh -lc 'rg -n foo'" reads as "rg -n foo".
    static func shortCommand(_ command: String) -> String {
        var text = command
        for shell in ["/bin/zsh -lc ", "/bin/bash -lc ", "bash -lc ", "zsh -lc "] where text.hasPrefix(shell) {
            text = String(text.dropFirst(shell.count))
            if let first = text.first, first == "'" || first == "\"", text.last == first { text = String(text.dropFirst().dropLast()) }
        }
        text = text.replacingOccurrences(of: "/Library/Developer/CommandLineTools/usr/bin/", with: "")
        // A leading cd into the task's own worktree says nothing; the command after it does.
        while text.hasPrefix("cd "), let range = text.range(of: " && ") {
            let target = text[text.index(text.startIndex, offsetBy: 3)..<range.lowerBound].trimmingCharacters(in: CharacterSet(charactersIn: "\"' "))
            guard target.contains("/neko/task-worktrees/") || target == "." else { break }
            text = String(text[range.upperBound...])
        }
        return String(text.split(separator: "\n").first ?? "").prefix(140).description
    }
}

/// A ticket's conversation rows, for a `ChatTranscript`. Each row is keyed by
/// the event that started it, never by its position.
struct TicketThreadView: View {
    let ticket: JSONValue
    var body: some View {
        let rows = TicketThread.rows(ticket)
        ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
            self.row(row, isLast: index == rows.count - 1)
                .padding(.top, ChatRhythm.gap(isUser: row.isYours, isFirst: index == 0))
                .id(row.id)
        }
        if let working = Self.working(ticket["status"].string) {
            ActivityCapsule(activity: working.activity, label: working.label, plain: true)
                .frame(minHeight: NekoSpace.xl + NekoSpace.hair, alignment: .leading)
                .padding(.top, NekoChat.replyGap)
        }
    }

    /// The live line under a running ticket, as the same indicator Home uses.
    static func working(_ status: String) -> (activity: NekoActivity, label: String)? {
        switch status {
        case "Queued": (.thinking, "Getting this task ready…")
        case "Planning": (.reading, "Planner is reading the code…")
        case "Building": (.analyzing, "Builder is making the change in its own copy…")
        case "Reviewing": (.analyzing, "Reviewer is checking the change…")
        default: nil
        }
    }

    @ViewBuilder private func row(_ row: TicketThread.Row, isLast: Bool) -> some View {
        switch row.item {
        case .brief(let text):
            TicketRequestBlock(text: text, createdAt: ticket["created_at_ms"].int)
        case .you(let text):
            ChatUserMessage(text: text)
        case .agent(let role, let text):
            VStack(alignment: .leading, spacing: NekoSpace.sm) {
                if row.showsSpeaker { ChatSpeaker(name: TicketThread.agentName(role), symbol: TicketThread.symbol(role)) }
                ReadableText(text: text).font(NekoFont.chat).lineSpacing(NekoLeading.reading)
            }
            .accessibilityElement(children: .contain)
        case .steps(let role, let commands):
            VStack(alignment: .leading, spacing: NekoSpace.sm) {
                if row.showsSpeaker { ChatSpeaker(name: TicketThread.agentName(role), symbol: TicketThread.symbol(role)) }
                TicketWorkLog(author: TicketThread.agentName(role), commands: commands, startsOpen: isLast)
            }
        case .status(let text):
            ChatStatusDivider(text: text)
        }
    }
}

/// Reply box pinned under a ticket's conversation.
struct TicketComposer: View {
    @ObservedObject var model: AppModel
    let id: String
    let status: String
    /// What the ticket is waiting on you for, inside the composer glass.
    var tray: AnyView? = nil
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
        default: "Redirect the agent. It pauses and replans…"
        }
    }
    var body: some View {
        let targetID = id
        ComposerRuntimeScope(model: model, conversationID: RuntimeSelectionScope.task(targetID)) { selection in
            SharedComposer(
                text: Binding(get: { model.ticketDrafts.text(for: targetID) }, set: { model.ticketDrafts.set($0, for: targetID) }),
                attachments: model.ticketDrafts.attachments(for: targetID),
                placeholder: placeholder,
                accessibilityLabel: "Message to this task’s agent",
                accessibilityHelp: "Return sends a reply to this agent. Shift Return adds a line. Command Return also sends a reply. New direction pauses active work and the agent reassesses it. Paste or drop images and files to attach them.",
                state: composerState, validationError: payloadError,
                onSend: send,
                onStop: { Task { await model.workbench(.command("CancelTask", ["task_id": .string(targetID)])) } },
                onAttach: { model.ticketDrafts.add($0, for: targetID) },
                onRemove: { model.ticketDrafts.remove($0, for: targetID) },
                onChooseAttachments: {
                    ComposerAttachmentPicker.choose(attach: { model.ticketDrafts.add($0, for: targetID) }, onError: { model.error = $0 })
                },
                onError: { model.error = $0 },
                tray: tray
            ) {
                ComposerRuntimePicker(model: model, selection: selection)
            }
        }.id(targetID)
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

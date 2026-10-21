import SwiftUI
import AppKit
@preconcurrency import ApplicationServices
import NekoKit

struct TodayView: View {
    @ObservedObject var model: AppModel
    @State private var escapeMonitor: Any?
    @State private var lastEscape: Date = .distantPast
    @State private var addingWorkspace = false
    @AppStorage("neko.composer.mode") private var mode = "ask"
    @State private var menu = ComposerMenuState()
    /// Built once: fresh closures on every pass would redraw every reply view reading them.
    @State private var replyActions = ReplyActions()
    /// Turns newer than this arrived while the chat was open and fade in.
    @State private var openedAt = Int(Date().timeIntervalSince1970 * 1000)
    @State private var fades = ChatFadeLedger()
    /// Which work logs the reader opened, and when this window watched a reply finish
    /// (for replies whose message carries no completion time).
    @State private var workLedger = ChatWorkLedger()
    @State private var finishedAt: [String: Int] = [:]
    private var workspaceName: String { model.selectedWorkspace.flatMap { id in model.workspaces.first { $0.recordID == id }?["name"].string } ?? "All workspaces" }
    private var scope: ChatDraftScope { Self.scope(model) }
    /// The draft scope for the model's current selection, also used outside a view pass.
    @MainActor static func scope(_ model: AppModel) -> ChatDraftScope {
        let profiles = model.snapshot["agent_profiles"]
        let profile: String
        if let scope = model.selectedWorkspace {
            profile = profiles["assignments"].array.first { $0["workspace_id"].string == scope }?["profile_id"].string ?? "default"
        } else { profile = profiles["active_profile_id"].string.isEmpty ? "default" : profiles["active_profile_id"].string }
        return ChatDraftScope(workspaceID: model.selectedWorkspace, profileID: profile)
    }
    private var draft: String {
        get { model.chatDrafts.text(for: scope) }
        nonmutating set { model.chatDrafts.set(newValue, for: scope) }
    }
    private var sending: Bool { model.sendingChatScopes.contains(scope) }
    private var attachments: [ComposerAttachment] { model.chatDrafts.attachments(for: scope) }
    private var workSummary: TodayWorkSummary { TodayWorkSummary(tasks: model.tasks, workspaceID: model.selectedWorkspace) }
    private var messages: [JSONValue] {
        model.snapshot["conversation"].array.filter {
            ($0["workspace_id"] == .null ? nil : $0["workspace_id"].string) == scope.workspaceID &&
            ($0["agent_profile_id"].string == scope.profileID)
        }
    }

    var body: some View {
        // Read once per pass; every row and tray below takes them as values.
        let messages = self.messages
        let related = ChatTurnRelated(snapshot: model.snapshot)
        VStack(spacing: 0) {
            FullDiskAccessBanner().padding(.bottom, NekoSpace.xxs)
            ChatTranscript(identity: scope, signal: signal(messages), anchorsSend: true) {
                transcript(messages, related)
            } dock: {
                composer(messages, related)
            }
        }
        .navigationTitle("Home")
        .navigationSubtitle(headerSubtitle(messages))
        .toolbar { todayToolbar(messages) }
        .environment(\.replyActions, replyActions)
        .onChange(of: signal(messages).pendingID) { old, new in
            if let old, new != old { finishedAt[old] = Int(Date().timeIntervalSince1970 * 1000) }
        }
        .onAppear {
            replyActions = ReplyActions(send: { [model] text in Self.post(text, model: model) },
                                        draft: { [model] text in model.chatDrafts.set(text, for: Self.scope(model)) })
            guard escapeMonitor == nil else { return }
            escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                guard event.keyCode == 53, !event.isARepeat else { return event }
                guard Self.pendingTurn(in: self.messages) != nil else { lastEscape = .distantPast; return event }
                if let editor = event.window?.firstResponder as? NSTextView, editor.hasMarkedText() { lastEscape = .distantPast; return event }
                if !menuItems.isEmpty { lastEscape = .distantPast; return event }
                let now = Date()
                if now.timeIntervalSince(lastEscape) < 0.6 { lastEscape = .distantPast; stopReply(); return nil }
                lastEscape = now
                return event
            }
        }
        .onDisappear { if let monitor = escapeMonitor { NSEvent.removeMonitor(monitor) }; escapeMonitor = nil }
        .sheet(isPresented: $addingWorkspace) { WorkspaceEditor(model: model) }
    }

    private func signal(_ messages: [JSONValue]) -> ChatTranscriptSignal {
        ChatTranscriptSignal(lastID: messages.last?.recordID,
                             lastIsUser: messages.last?["role"].string == "user",
                             pendingID: messages.last { $0["pending"].bool && $0["role"].string != "user" }?.recordID)
    }

    @ViewBuilder private func transcript(_ messages: [JSONValue], _ related: ChatTurnRelated) -> some View {
        if messages.isEmpty {
            VStack(alignment: .leading, spacing: NekoLayout.messageGap) {
                TodayHero(model: model, workSummary: workSummary, addingWorkspace: $addingWorkspace)
                if model.workspaces.isEmpty {
                    HowNekoWorks(model: model, addingWorkspace: $addingWorkspace) { draft = $0 }
                } else {
                    TodayBrief(model: model, workSummary: workSummary) { draft = $0 }
                }
            }
        }
        let awaiting = Self.awaitingApprovals(in: messages)
        // Your newest message and everything after it is one exchange, so the
        // chat can hold it at the top while the reply is written.
        let split = messages.lastIndex { $0["role"].string == "user" } ?? messages.count
        let indexed = Array(messages.enumerated())
        ForEach(indexed.prefix(split), id: \.element.recordID) { index, message in
            row(index, message, in: messages, related, awaiting)
        }
        if split < messages.count {
            ChatExchange {
                ForEach(indexed.suffix(from: split), id: \.element.recordID) { index, message in
                    row(index, message, in: messages, related, awaiting)
                }
            }
        }
    }

    private func row(_ index: Int, _ message: JSONValue, in messages: [JSONValue], _ related: ChatTurnRelated, _ awaiting: Int) -> some View {
        let isUser = message["role"].string == "user"
        let previous = index > 0 ? messages[index - 1]["role"].string : nil
        let speaker = ChatRhythm.showsSpeaker(isUser ? "user" : "neko", previous: previous.map { $0 == "user" ? "user" : "neko" })
        let extra: JSONValue = .array([.bool(speaker), .number(Double(message["pending"].bool ? awaiting : 0)),
                                       .number(Double(finishedAt[message.recordID] ?? 0))])
        return ChatTurn(key: related.key(for: message, extra: extra)) {
            messageView(message, related, showsSpeaker: speaker, retry: isUser ? nil : retryText(for: index, in: messages))
        }
        .equatable()
        .padding(.top, ChatRhythm.gap(isUser: isUser, isFirst: index == 0))
        .modifier(ChatTurnFade(id: message.recordID, fresh: message["at_ms"].int >= openedAt, ledger: fades))
        .id(message.recordID)
    }

    private func composer(_ messages: [JSONValue], _ related: ChatTurnRelated) -> some View {
        let targetScope = scope
        let items = menuItems
        return ComposerRuntimeScope(model: model, conversationID: RuntimeSelectionScope.home(targetScope)) { selection in
            SharedComposer(
                text: Binding(get: { model.chatDrafts.text(for: targetScope) }, set: { model.chatDrafts.set($0, for: targetScope) }),
                attachments: attachments,
                placeholder: "Ask Neko anything · / for commands · @ for context",
                accessibilityLabel: "Message to Neko",
                accessibilityHelp: "Return sends or queues. Shift Return adds a line. Command Return interrupts and sends. Option Return includes the previous app's selection. Paste or drop images and files to attach them.",
                state: composerState(messages), validationError: payloadError,
                onSend: { send() }, onStop: stopReply,
                onAttach: { model.chatDrafts.add($0, for: targetScope) },
                onRemove: { model.chatDrafts.remove($0, for: targetScope) },
                onChooseAttachments: chooseAttachments,
                onError: { model.error = $0 },
                onInterrupt: { send(interrupt: true) },
                onContext: { send(withContext: true) },
                onMenuKey: handleMenuKey,
                tray: tray(messages, related)
            ) {
                GlassSegmented(selection: $mode, options: [
                    .init(value: "ask", title: "Ask", help: "Ask: Neko answers now"),
                    .init(value: "plan", title: "Plan", help: "Plan: Neko proposes a task with a plan for your approval, without changing anything")
                ], size: .small)
                ComposerRuntimePicker(model: model, selection: selection)
            }
        }
        .id(targetScope)
        // The / and @ menu floats above the dock; it never changes the
        // dock's measured height, so the transcript does not move.
        .overlay(alignment: .top) {
            // A zero-height anchor on the composer's top edge, with the menu
            // hung from its bottom: the menu grows upward over the transcript
            // and can never cover the composer or run off the window.
            if !items.isEmpty {
                Color.clear.frame(height: 0)
                    .overlay(alignment: .bottom) {
                        ComposerMenuList(items: items, index: $menu.index)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.bottom, NekoSpace.sm)
                    }
            }
        }
        .task(id: menu.mentionQuery(in: draft)) {
            if let files = await ComposerMentionSearch.files(matching: menu.mentionQuery(in: draft), model: model) { menu.mentionFiles = files }
        }
        .onChange(of: draft) { _, _ in menu.index = 0 }
    }

    // MARK: Header, messages, approvals

    private func conversationTickets(_ messages: [JSONValue], _ related: ChatTurnRelated) -> [JSONValue] {
        messages.flatMap { $0["ticket_ids"].array.map(\.string) }.map(related.task).filter { $0 != .null }
    }
    private static func pendingApproval(in messages: [JSONValue]) -> (turn: String, call: JSONValue)? {
        for message in messages where message["pending"].bool {
            if let call = message["tool_calls"].array.first(where: { $0["status"].string == "awaiting_approval" }) { return (message.recordID, call) }
        }
        return nil
    }
    private func headerSubtitle(_ messages: [JSONValue]) -> String {
        let tickets = conversationTickets(messages, ChatTurnRelated(snapshot: model.snapshot))
        let status = Self.pendingTurn(in: messages) != nil ? "Working…"
            : Self.pendingApproval(in: messages) != nil || tickets.contains { ["AwaitingApproval", "ReadyForReview"].contains($0["status"].string) } ? "Waiting for you"
            : model.connected ? "Watching" : "Reconnecting…"
        return "\(workspaceName) · \(status)"
    }
    @ToolbarContentBuilder private func todayToolbar(_ messages: [JSONValue]) -> some ToolbarContent {
        if Self.pendingTurn(in: messages) != nil {
            ToolbarItem(placement: .primaryAction) {
                toolbarIcon("stop.fill", help: "Stop the current reply") { stopReply() }
            }
        }
    }
    private func toolbarIcon(_ symbol: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(NekoFont.meta).foregroundStyle(.secondary).frame(width: 32, height: 28).contentShape(Rectangle())
        }.buttonStyle(.plain).help(help).accessibilityLabel(help)
    }

    @ViewBuilder private func messageView(_ message: JSONValue, _ related: ChatTurnRelated, showsSpeaker: Bool, retry: String?) -> some View {
        if message["role"].string == "user" {
            ChatUserMessage(text: message["text"].string, timestamp: message["at_ms"].int, queued: message["queued"].bool)
        } else {
            VStack(alignment: .leading, spacing: NekoSpace.sm) {
                if showsSpeaker { ChatSpeaker(name: "Neko", timestamp: message["at_ms"].int) }
                if message["ticket_ids"].array.isEmpty &&
                    (message["text"].string.contains("I could not open every task") || message["text"].string.contains("No task was created")) {
                    Label {
                        Text("No task was created from this reply").foregroundStyle(N.text3)
                    } icon: {
                        Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(NekoStyle.amber)
                    }
                    .font(NekoFont.meta)
                }
                let text = message["text"].string
                let pending = message["pending"].bool
                if !text.isEmpty {
                    // A reply being written is drawn as Markdown as it arrives; only its tail redraws.
                    Group {
                        if pending { ChatStreamingText(text: text) } else { ReadableText(text: text) }
                    }
                    .font(NekoFont.chat).lineSpacing(NekoLeading.reading)
                }
                workLog(message, related, pending: pending, hasText: !text.isEmpty)
                if message["failed"].bool {
                    ReplyErrorCallout(message: "", retry: retry.map { text in { [model] in Self.post(text, model: model) } })
                }
                ForEach(message["remembered"].array, id: \.self) { memory in ReplyMemoryNote(text: memory.string) }
                ForEach(message["responsibility_ids"].array, id: \.self) { id in
                    if let item = related.watch(id.string) {
                        SuggestedResponsibilityCard(model: model, item: item)
                    }
                }
                ForEach(message["actions"].array, id: \.recordID) { action in
                    let titles = related.actionTitles(action)
                    ChatActionCard(action: action, taskTitle: titles.task, mergeTarget: titles.merge,
                                   decide: { [model] accept in
                                       await model.workbench(.command("DecideChatAction", ["message_id": .string(message.recordID), "action_id": action["id"], "accept": .bool(accept)]))
                                   },
                                   open: { [model] in model.openAgent(action["task_id"].string, from: .home) })
                }
                ForEach(message["ticket_ids"].array, id: \.self) { id in
                    let task = related.task(id.string)
                    ReplyTicketRow(title: task["title"].string.isEmpty ? "Task" : task["title"].string,
                                   status: task["status"].string,
                                   workspace: related.workspaceName(task["workspace_id"].string)) { [model] in
                        model.openAgent(id.string, from: .home)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .contain)
        }
    }
    /// What the reply did, as one line under its text: live while it works,
    /// a summary with how long it worked once it is done.
    @ViewBuilder private func workLog(_ message: JSONValue, _ related: ChatTurnRelated, pending: Bool, hasText: Bool) -> some View {
        let tools = ChatToolActivitySummary(calls: message["tool_calls"].array, receipts: related.receipts(for: message))
            .values(connectionName: related.connectionName).filter { $0.status != "awaiting_approval" }
        let steps = message["activity"].array.map(\.string)
        let summary = ChatWorkSummary(activity: steps, toolCount: tools.count,
                                      toolFailed: tools.filter { ["failed", "denied"].contains($0.status) }.count)
        if pending || summary.hasWork {
            let now = pending ? ChatActivity.describe(message, model: model) : (activity: NekoActivity.thinking, label: "")
            let label = ChatWorkLive.label(steps: steps, fallback: now.label, keepFallback: now.activity == .needsYou || now.activity == .reading)
            ChatWorkLog(id: message.recordID, summary: summary, tools: tools, running: pending,
                        startMs: message["started_ms"].int > 0 ? message["started_ms"].int : message["at_ms"].int,
                        completedMs: message["started_ms"].int > 0 && !pending ? message["at_ms"].int
                            : ChatDuration.completion(of: message) ?? finishedAt[message.recordID],
                        live: (now.activity, label), ledger: workLedger)
        } else if !hasText, !message["failed"].bool, message["ticket_ids"].array.isEmpty, message["actions"].array.isEmpty,
                  message["remembered"].array.isEmpty, message["responsibility_ids"].array.isEmpty, message["tool_calls"].array.isEmpty {
            Text("No reply came back.").font(NekoFont.meta).foregroundStyle(N.text3)
        }
    }
    private func retryText(for index: Int, in messages: [JSONValue]) -> String? {
        messages[..<index].last { $0["role"].string == "user" }?["text"].string
    }

    /// The one decision this conversation is waiting on, inside the dock. It
    /// alone owns ⌘1 and ⌘2.
    private func tray(_ messages: [JSONValue], _ related: ChatTurnRelated) -> AnyView? {
        let tickets = conversationTickets(messages, related)
        if let pending = Self.pendingApproval(in: messages) {
            let tool = pending.call["tool_name"].string
            let connection = related.connectionName(pending.call["connection_id"].string)
            let on = connection.isEmpty ? "" : " on \(connection)"
            return AnyView(ChatDockTray(
                symbol: "hand.raised.fill",
                title: Text("Allow ") + Text(tool).font(NekoChat.code) + Text("\(on)?"),
                announcement: "Allow \(tool)\(on)?",
                detail: ChatToolArguments.summary(pending.call["arguments_json"].string),
                fullDetail: pending.call["arguments_json"].string,
                secondary: ChatTrayButton(label: "Deny", shortcut: KeyboardShortcut("2", modifiers: .command), hint: "⌘2") { decide(pending, false) },
                primary: ChatTrayButton(label: "Allow", shortcut: KeyboardShortcut("1", modifiers: .command), hint: "⌘1") { decide(pending, true) },
                busy: model.busy))
        }
        if let task = tickets.last(where: { $0["status"].string == "AwaitingApproval" && TicketPresentation.waitingReason($0) == nil }) {
            let title = "Approve the plan for “\(task["title"].string)”?"
            return AnyView(ChatDockTray(
                symbol: "exclamationmark.triangle.fill", title: Text(title), announcement: title,
                detail: "Works in its own copy of the folder. Nothing is pushed.",
                secondary: ChatTrayButton(label: "Show task") { inspect(task.recordID) },
                primary: ChatTrayButton(label: "Approve plan", shortcut: KeyboardShortcut("1", modifiers: .command), hint: "⌘1") { ticketCommand("ApproveTask", task.recordID) },
                busy: model.busy))
        }
        if let task = tickets.last(where: { $0["status"].string == "ReadyForReview" }) {
            let title = "“\(task["title"].string)” is ready for review"
            return AnyView(ChatDockTray(
                symbol: "checkmark.seal.fill", tint: NekoStyle.sky, title: Text(title), announcement: title,
                detail: "Its changes and checks are in the agent chat.",
                secondary: ChatTrayButton(label: "Show task") { inspect(task.recordID) },
                primary: TicketPresentation.openPullRequest(task).map { url in
                    ChatTrayButton(label: "Open pull request", shortcut: KeyboardShortcut("1", modifiers: .command), hint: "⌘1") { NSWorkspace.shared.open(url) }
                } ?? ChatTrayButton(label: "Mark done", shortcut: KeyboardShortcut("1", modifiers: .command), hint: "⌘1") { ticketCommand("CompleteTask", task.recordID) },
                busy: model.busy))
        }
        if let decision = askingDecision {
            let title = DecisionPresentation.title(decision["action"].string)
            let reason = decision["reason"].string
            return AnyView(ChatDockTray(
                symbol: "questionmark.bubble", title: Text(title), announcement: title,
                detail: reason.components(separatedBy: .newlines).first ?? "", fullDetail: reason,
                primary: ChatTrayButton(label: "Review") { model.requestedPage = .memory }))
        }
        if let notice = toolAccessNotice {
            return AnyView(ChatDockTray(
                symbol: "wrench.and.screwdriver", title: Text("Tools aren’t ready"), announcement: notice,
                detail: notice,
                primary: ChatTrayButton(label: "Set up tools") {
                    if model.selectedWorkspace == nil, model.workspaces.count == 1 { model.selectedWorkspace = model.workspaces[0].recordID }
                    model.requestedPage = .tools
                }))
        }
        return nil
    }
    /// A decision Neko recorded that asks for yours and has no outcome yet.
    /// The rest live on Memory → Working style, not above the conversation.
    private var askingDecision: JSONValue? {
        DecisionPresentation.latest(model.snapshot["decision_records"].array.filter { model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace })
            .first { $0["action"].string == "ask_user" && $0["observed_outcome"].string.isEmpty }
    }
    private func decide(_ pending: (turn: String, call: JSONValue), _ approve: Bool) {
        Task { await model.workbench(.command("DecideChatTool", ["turn_id": .string(pending.turn), "call_id": pending.call["id"], "approve": .bool(approve)])) }
    }
    private func ticketCommand(_ command: String, _ id: String) {
        Task { await model.workbench(.command(command, ["task_id": .string(id)])) }
    }
    private func inspect(_ id: String) {
        model.openAgent(id, from: .home)
    }

    // MARK: / and @ menus

    private var menuItems: [ComposerMenuItem] {
        menu.items(for: draft, model: model, actions: ComposerMenuActions(
            replaceMention: { replacement in draft = ComposerMention.replacingTrailingMention(in: draft, with: replacement) },
            plan: { mode = "plan"; draft = "" },
            fill: { draft = $0 },
            send: { send() }
        ))
    }
    private func handleMenuKey(_ key: ComposerMenuKey) -> Bool {
        let items = menuItems
        guard !items.isEmpty else { return false }
        if key == .accept { items[menu.selected(of: items.count)].apply() }
        else { menu.navigate(key, count: items.count, draft: draft) }
        return true
    }

    private var payloadError: String? {
        ComposerPayload.validationError(ComposerPayload.homeText(model.chatDrafts.submission(for: scope).text, planOnly: mode == "plan"), limit: 4096)
    }
    private func composerState(_ messages: [JSONValue]) -> ComposerActionState {
        // The daemon runs one chat at a time across scopes, so any pending or
        // queued turn means this one queues.
        let occupied = model.snapshot["conversation"].array.contains { $0["pending"].bool || $0["queued"].bool }
        return ComposerActionState(running: occupied, hasContent: !model.chatDrafts.submission(for: scope).text.isEmpty,
                                   submitting: sending, blocked: model.busy || payloadError != nil, queues: true,
                                   canStop: Self.pendingTurn(in: messages) != nil)
    }
    private static func pendingTurn(in messages: [JSONValue]) -> JSONValue? {
        messages.last { $0["pending"].bool && $0["role"].string != "user" } ?? messages.last { $0["pending"].bool }
    }
    private func stopReply() {
        guard let turn = Self.pendingTurn(in: messages) else { return }
        Task { await model.workbench(.command("CancelChat", ["turn_id": turn["id"]])) }
    }

    private var toolAccessNotice: String? {
        let workspace = model.selectedWorkspace ?? (model.workspaces.count == 1 ? model.workspaces[0].recordID : "")
        guard !workspace.isEmpty else { return model.workspaces.count > 1 ? "Choose a workspace to use its tools." : nil }
        let connected = model.snapshot["mcp"]["connections"].array.filter {
            $0["enabled"].bool && $0["trusted"].bool && ($0["workspace_id"].string.isEmpty || $0["workspace_id"].string == workspace)
        }
        guard !connected.isEmpty else { return nil }
        let usable = connected.contains { !$0["tools"].array.isEmpty && $0["error"].string.isEmpty }
        return usable ? nil : "Connections are present, but no tools are available yet. Discover their tools in Tools & skills."
    }
    private static func awaitingApprovals(in messages: [JSONValue]) -> Int {
        messages.filter { $0["pending"].bool }.flatMap { $0["tool_calls"].array }.filter { $0["status"].string == "awaiting_approval" }.count
    }
    private func chooseAttachments() {
        let targetScope = scope
        ComposerAttachmentPicker.choose(attach: { model.chatDrafts.add($0, for: targetScope) }, onError: { model.error = $0 })
    }

    private func send(interrupt: Bool = false, withContext: Bool = false) {
        guard !sending, !model.busy else { return }
        let submission = model.chatDrafts.submission(for: scope)
        guard !submission.text.isEmpty else { return }
        if let command = SlashCommand.parse(submission.text), command.chatText == nil {
            runCommand(command)
            model.chatDrafts.complete(submission, succeeded: true)
            return
        }
        var text = ComposerPayload.homeText(submission.text, planOnly: mode == "plan")
        if withContext {
            if let snapshot = PreviousAppContext.shared.read() {
                text = PreviousAppContext.attach(snapshot, to: text)
            } else {
                model.notice = AXIsProcessTrusted() ? "No previous app to read from. Sent without context." : "Allow Accessibility in Settings → Permissions to include the previous app. Sent without context."
            }
        }
        guard Self.validateHomePayload(text, model: model) else { return }
        model.sendingChatScopes.insert(submission.scope)
        Task {
            let saved = await model.workbench(.command(interrupt ? "InterruptAndSendMessage" : "SendMessage", ["text": .string(text), "workspace_id": submission.scope.workspaceID.map(JSONValue.string) ?? .null]))
            model.chatDrafts.complete(submission, succeeded: saved)
            model.sendingChatScopes.remove(submission.scope)
        }
    }

    /// Sends text that came from a reply view (a choice, a form, Try again).
    @MainActor private static func post(_ text: String, model: AppModel) {
        let target = scope(model)
        guard !text.isEmpty, !model.sendingChatScopes.contains(target), validateHomePayload(text, model: model) else { return }
        model.sendingChatScopes.insert(target)
        Task {
            _ = await model.workbench(.command("SendMessage", ["text": .string(text), "workspace_id": target.workspaceID.map(JSONValue.string) ?? .null]))
            model.sendingChatScopes.remove(target)
        }
    }

    @MainActor private static func validateHomePayload(_ text: String, model: AppModel) -> Bool {
        guard let error = ComposerPayload.validationError(text, limit: 4096) else { return true }
        model.error = error
        return false
    }

    private func runCommand(_ command: SlashCommand) {
        let open = { (page: Page) in model.requestedPage = page }
        switch command {
        case .stop: Task { await StopAllWork.run(model) }
        case .clearFinished:
            Task {
                let before = model.snapshot["tasks"].array.count
                if await model.workbench(.command("ClearFinishedTasks", ["workspace_id": scope.workspaceID.map(JSONValue.string) ?? .null])) {
                    let removed = before - model.snapshot["tasks"].array.count
                    model.notice = removed == 0 ? "No finished tasks to clear." : "Cleared \(removed) finished \(removed == 1 ? "task" : "tasks"). Files and worktrees are untouched."
                }
            }
        case .memory: open(.memory)
        case .tickets: open(.tickets)
        case .models, .permissions: open(.settings)
        case .setup: model.onboarding = true
        case .help: draft = "/"
        case .remember, .forget, .recall: break
        }
    }
}

/// Seconds since Neko started on this reply, so a long turn visibly moves.
struct ChatElapsed: View {
    let since: Int
    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let seconds = max(0, Int(context.date.timeIntervalSince1970) - since / 1000)
            Text(ChatDuration.text(seconds: seconds))
                .font(NekoFont.meta).monospacedDigit().foregroundStyle(N.text4)
        }
        .accessibilityHidden(true)
    }
}

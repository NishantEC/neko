import SwiftUI
import AppKit
@preconcurrency import ApplicationServices
import NekoKit

private struct ChatComposerHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

private struct ChatScrollMetricsKey: PreferenceKey {
    static let defaultValue = ChatScrollMetrics()
    static func reduce(value: inout ChatScrollMetrics, nextValue: () -> ChatScrollMetrics) { value = nextValue() }
}

struct TodayView: View {
    @ObservedObject var model: AppModel
    @State private var scrollFollow = ChatScrollFollowState()
    @State private var escapeMonitor: Any?
    @State private var lastEscape: Date = .distantPast
    @State private var transcriptHeight: CGFloat = 0
    @State private var addingWorkspace = false
    @State private var composerHeight: CGFloat = 0
    @AppStorage("neko.composer.mode") private var mode = "ask"
    @State private var menu = ComposerMenuState()
    private var workspaceName: String { model.selectedWorkspace.flatMap { id in model.workspaces.first { $0.recordID == id }?["name"].string } ?? "All workspaces" }
    private var scope: ChatDraftScope {
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
    private var replying: Bool { pendingTurn != nil }
    private var chatQueueOccupied: Bool { model.snapshot["conversation"].array.contains { $0["pending"].bool || $0["queued"].bool } }
    private var attachments: [ComposerAttachment] { model.chatDrafts.attachments(for: scope) }
    private var workSummary: TodayWorkSummary { TodayWorkSummary(tasks: model.tasks, workspaceID: model.selectedWorkspace) }
    private var messages: [JSONValue] {
        model.snapshot["conversation"].array.filter {
            ($0["workspace_id"] == .null ? nil : $0["workspace_id"].string) == scope.workspaceID &&
            ($0["agent_profile_id"].string == scope.profileID)
        }
    }
    var body: some View {
        GeometryReader { outer in
        VStack(spacing: 0) {
        FullDiskAccessBanner().padding(.bottom, NekoSpace.xxs)
        HStack(spacing: 0) {
            ZStack(alignment: .bottom) {
                GeometryReader { viewport in
                ScrollViewReader { reader in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: NekoLayout.messageGap) {
                            if messages.isEmpty {
                                TodayHero(model: model, workSummary: workSummary, addingWorkspace: $addingWorkspace)
                                if model.workspaces.isEmpty {
                                    HowNekoWorks(model: model, addingWorkspace: $addingWorkspace) { draft = $0 }
                                } else {
                                    TodayBrief(model: model, workSummary: workSummary) { draft = $0 }
                                }
                            }
                            ForEach(DecisionPresentation.latest(model.snapshot["decision_records"].array.filter { model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace }).prefix(3), id: \.recordID) { DecisionCard(model: model, record: $0) }
                            let related = ChatTurnRelated(snapshot: model.snapshot)
                            ForEach(messages, id: \.recordID) { message in
                                ChatTurn(key: related.key(for: message, awaitingApprovals: message["pending"].bool ? awaitingApprovals : 0)) {
                                    messageView(message)
                                }
                                .equatable()
                                .id(message.recordID)
                                .transition(.opacity)
                            }
                            Color.clear.frame(height: ChatComposerClearance.bottomSpace(for: composerHeight)).id("bottom")
                        }.frame(maxWidth: NekoLayout.readingWidth, alignment: .leading)
                            .padding(.horizontal, NekoLayout.pageInset).padding(.top, NekoLayout.sectionGap).padding(.bottom, NekoSpace.lg)
                            .frame(maxWidth: .infinity, alignment: .center)
                            .background(GeometryReader { geometry in
                                Color.clear.preference(key: ChatScrollMetricsKey.self, value: ChatScrollMetrics(contentHeight: geometry.size.height, originY: geometry.frame(in: .named("chatTranscript")).minY, viewportHeight: viewport.size.height))
                            })
                    }.coordinateSpace(name: "chatTranscript")
                        .background(ChatScrollIntentObserver(
                            bottomExclusion: CGSize(width: min(NekoLayout.readingWidth, max(280, outer.size.width - NekoLayout.pageInset * 2)), height: composerHeight),
                            onInteraction: { scrollFollow.userInteracted() }
                        ))
                        .overlay(alignment: .bottom) {
                            if !scrollFollow.followsLatest {
                                Button("Jump to latest", systemImage: "arrow.down") {
                                    scrollFollow.pin()
                                    reader.scrollTo("bottom", anchor: .bottom)
                                }.nekoButton(.secondary).controlSize(.small)
                                    .padding(.bottom, composerHeight + 8)
                            }
                        }
                        .onPreferenceChange(ChatScrollMetricsKey.self) { metrics in
                            let resized = abs(metrics.contentHeight - transcriptHeight) > 1
                            transcriptHeight = metrics.contentHeight
                            scrollFollow.update(metrics)
                            // Layout has now measured the completed reply, so
                            // scroll to its new bottom rather than the old one.
                            if resized, scrollFollow.followsLatest { DispatchQueue.main.async { if scrollFollow.followsLatest { reader.scrollTo("bottom", anchor: .bottom) } } }
                        }
                        .onChange(of: messages.last) { _, _ in
                            if messages.last?["role"].string == "user" { scrollFollow.pin() }
                            guard scrollFollow.followsLatest else { return }
                            reader.scrollTo("bottom", anchor: .bottom)
                            DispatchQueue.main.async { if scrollFollow.followsLatest { reader.scrollTo("bottom", anchor: .bottom) } }
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { if scrollFollow.followsLatest { reader.scrollTo("bottom", anchor: .bottom) } }
                        }
                        .onChange(of: messages.count) { old, new in
                            guard new > old else { return }
                            guard messages.suffix(new - old).contains(where: { $0["role"].string == "user" }) else { return }
                            scrollFollow.pin()
                            DispatchQueue.main.async { if scrollFollow.followsLatest { reader.scrollTo("bottom", anchor: .bottom) } }
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { if scrollFollow.followsLatest { reader.scrollTo("bottom", anchor: .bottom) } }
                        }
                        .onChange(of: scope) { _, _ in
                            scrollFollow = ChatScrollFollowState()
                            reader.scrollTo("bottom", anchor: .bottom)
                        }
                        .onAppear {
                            reader.scrollTo("bottom", anchor: .bottom)
                            // The inspector and composer settle a moment later; follow them.
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { if scrollFollow.followsLatest { reader.scrollTo("bottom", anchor: .bottom) } }
                        }
                }
                }
                composer(availableWidth: outer.size.width)
            }
            // Keep scrolling transcript content below the page header.
            .clipped()
        }
        }
        }
        .navigationTitle("Home")
        .navigationSubtitle(headerSubtitle)
        .toolbar { todayToolbar }
        .environment(\.replyActions, ReplyActions(send: { text in post(text) }, draft: { text in draft = text }))
        .nekoAnimation(.appear, value: messages.count)
        .onAppear {
            guard escapeMonitor == nil else { return }
            escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                guard event.keyCode == 53, !event.isARepeat else { return event }
                guard pendingTurn != nil else { lastEscape = .distantPast; return event }
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
    private func composer(availableWidth: CGFloat) -> some View {
        let width = min(NekoLayout.readingWidth, max(280, availableWidth - NekoLayout.pageInset * 2))
        let targetScope = scope
        return VStack(alignment: .leading, spacing: NekoSpace.sm) {
            if let notice = toolAccessNotice {
                HStack(spacing: NekoSpace.sm) {
                    Image(systemName: "wrench.and.screwdriver").foregroundStyle(ReplyStyle.orange)
                    Text(notice).font(NekoFont.caption).foregroundStyle(.secondary)
                    Spacer()
                    Button("Set Up Tools") {
                        if model.selectedWorkspace == nil, model.workspaces.count == 1 { model.selectedWorkspace = model.workspaces[0].recordID }
                        model.requestedPage = .tools
                    }.controlSize(.small)
                }.padding(.horizontal, NekoSpace.md).padding(.vertical, 7)
                    .background(N.panel, in: RoundedRectangle(cornerRadius: NekoRadius.md))
            }
            let items = menuItems
            if !items.isEmpty { ComposerMenuList(items: items, index: $menu.index) }
            approvalBar
            ComposerRuntimeScope(model: model, conversationID: RuntimeSelectionScope.home(targetScope)) { selection in
                SharedComposer(
                    text: Binding(get: { model.chatDrafts.text(for: targetScope) }, set: { model.chatDrafts.set($0, for: targetScope) }),
                    attachments: attachments,
                    placeholder: "Ask Neko, type / for commands or @ to add context",
                    accessibilityLabel: "Message to Neko",
                    accessibilityHelp: "Return sends or queues. Shift Return adds a line. Command Return interrupts and sends. Option Return includes the previous app's selection. Paste or drop images and files to attach them.",
                    state: composerState, validationError: payloadError,
                    onSend: { send() }, onStop: stopReply,
                    onAttach: { model.chatDrafts.add($0, for: targetScope) },
                    onRemove: { model.chatDrafts.remove($0, for: targetScope) },
                    onChooseAttachments: chooseAttachments,
                    onError: { model.error = $0 },
                    onInterrupt: { send(interrupt: true) },
                    onContext: { send(withContext: true) },
                    onMenuKey: handleMenuKey
                ) {
                    GlassSegmented(selection: $mode, options: [
                        .init(value: "ask", title: "Ask", help: "Ask: Neko answers now"),
                        .init(value: "plan", title: "Plan", help: "Plan: Neko proposes a task with a plan for your approval, without changing anything")
                    ], size: .small)
                    ComposerRuntimePicker(model: model, selection: selection)
                }
            }.id(targetScope)
        }
        .frame(width: width)
        .padding(.bottom, 14).padding(.top, NekoSpace.lg)
        .frame(maxWidth: .infinity, alignment: .center)
        .background(alignment: .bottom) {
            LinearGradient(colors: [N.canvas.opacity(0), N.canvas.opacity(0.92)], startPoint: .top, endPoint: UnitPoint(x: 0.5, y: 0.45))
                .allowsHitTesting(false)
        }
        .background {
            GeometryReader { geometry in
                Color.clear.preference(key: ChatComposerHeightKey.self, value: geometry.size.height)
            }
        }
        .onPreferenceChange(ChatComposerHeightKey.self) { height in
            if abs(height - composerHeight) > 1 { composerHeight = height }
        }
        .task(id: menu.mentionQuery(in: draft)) {
            if let files = await ComposerMentionSearch.files(matching: menu.mentionQuery(in: draft), model: model) { menu.mentionFiles = files }
        }
        .onChange(of: draft) { _, _ in menu.index = 0 }
    }

    // MARK: Header, messages, approvals

    private var conversationTickets: [JSONValue] {
        let ids = messages.flatMap { $0["ticket_ids"].array.map(\.string) }
        return ids.compactMap { id in model.snapshot["tasks"].array.first { $0.recordID == id } }
    }
    private var pendingApproval: (turn: String, call: JSONValue)? {
        for message in messages where message["pending"].bool {
            if let call = message["tool_calls"].array.first(where: { $0["status"].string == "awaiting_approval" }) { return (message.recordID, call) }
        }
        return nil
    }
    private var headerSubtitle: String {
        let status = replying ? "Working…" : pendingApproval != nil || conversationTickets.contains { ["AwaitingApproval", "ReadyForReview"].contains($0["status"].string) } ? "Waiting for you" : model.connected ? "Watching" : "Reconnecting…"
        return "\(workspaceName) · \(status)"
    }
    @ToolbarContentBuilder private var todayToolbar: some ToolbarContent {
        if replying {
            ToolbarItem(placement: .primaryAction) {
                toolbarIcon("stop.fill", help: "Stop the current reply") {
                    if let current = messages.last(where: { $0["pending"].bool }) {
                        Task { await model.workbench(.command("CancelChat", ["turn_id": current["id"]])) }
                    }
                }
            }
        }
    }
    private func toolbarIcon(_ symbol: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(NekoFont.meta).foregroundStyle(.secondary).frame(width: 32, height: 28).contentShape(Rectangle())
        }.buttonStyle(.plain).help(help).accessibilityLabel(help)
    }
    @ViewBuilder private func messageView(_ message: JSONValue) -> some View {
        if message["role"].string == "user" {
            ChatUserMessage(text: message["text"].string, timestamp: message["at_ms"].int, queued: message["queued"].bool)
        } else {
            VStack(alignment: .leading, spacing: NekoLayout.authorGap) {
                ChatAuthorLine(author: "Neko", timestamp: message["at_ms"].int)
                if message["pending"].bool {
                    let now = ChatActivity.describe(message, model: model)
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small)
                        Text(now.label).font(NekoFont.meta).foregroundStyle(.secondary)
                    }.padding(.vertical, NekoSpace.xxs)
                }
                let receipts = model.snapshot["mcp"]["receipts"].array.filter { $0["run_id"].string == "chat:\(message.recordID)" }
                let activity = ChatToolActivitySummary(calls: message["tool_calls"].array, receipts: receipts)
                if activity.count > 0 {
                    ChatToolActivity(model: model, activity: activity, turn: message.recordID, pending: message["pending"].bool, awaitingApprovals: awaitingApprovals)
                }
                if message["ticket_ids"].array.isEmpty &&
                    (message["text"].string.contains("I could not open every task") || message["text"].string.contains("No task was created")) {
                    Label("No task was created from this reply", systemImage: "exclamationmark.triangle.fill")
                        .font(NekoFont.caption).foregroundStyle(ReplyStyle.orange)
                }
                if !message["text"].string.isEmpty {
                    ReadableText(text: message["text"].string).font(NekoFont.chat).lineSpacing(4)
                }
                if message["failed"].bool {
                    ReplyErrorCallout(message: "", retry: retryText(for: message).map { text in { post(text) } })
                }
                ForEach(message["remembered"].array, id: \.self) { memory in ReplyMemoryNote(text: memory.string) }
                ForEach(message["responsibility_ids"].array, id: \.self) { id in
                    if let item = model.snapshot["mcp"]["responsibilities"].array.first(where: { $0.recordID == id.string }) {
                        SuggestedResponsibilityCard(model: model, item: item)
                    }
                }
                ForEach(message["actions"].array, id: \.recordID) { action in
                    ChatActionCard(model: model, messageID: message.recordID, action: action)
                }
                ForEach(message["ticket_ids"].array, id: \.self) { id in
                    let task = model.snapshot["tasks"].array.first { $0.recordID == id.string } ?? .null
                    ReplyTicketRow(title: task["title"].string.isEmpty ? "Task" : task["title"].string,
                                   status: task["status"].string,
                                   workspace: model.workspaces.first { $0.recordID == task["workspace_id"].string }?["name"].string ?? "") {
                        model.openAgent(id.string, from: .home)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
    private func connectionName(_ id: String) -> String {
        model.snapshot["mcp"]["connections"].array.first { $0.recordID == id }?["label"].string ?? ""
    }
    private func retryText(for message: JSONValue) -> String? {
        guard let index = messages.firstIndex(where: { $0.recordID == message.recordID }) else { return nil }
        return messages[..<index].last { $0["role"].string == "user" }?["text"].string
    }
    @ViewBuilder private var approvalBar: some View {
        if let pending = pendingApproval {
            let connection = connectionName(pending.call["connection_id"].string)
            ChatApprovalCard(symbol: "hand.raised.fill",
                         title: "Allow \(pending.call["tool_name"].string)\(connection.isEmpty ? "" : " on \(connection)")?",
                         detail: ChatApprovalCard.argumentSummary(pending.call["arguments_json"].string),
                         secondary: ("Deny", KeyboardShortcut("2", modifiers: .command), { decide(pending, false) }),
                         primary: ("Allow", KeyboardShortcut("1", modifiers: .command), { decide(pending, true) }),
                         busy: model.busy)
        } else if let task = conversationTickets.last(where: { $0["status"].string == "AwaitingApproval" && TicketPresentation.waitingReason($0) == nil }) {
            ChatApprovalCard(symbol: "exclamationmark.triangle.fill",
                         title: "Approve the plan for “\(task["title"].string)”?",
                         detail: "Work starts in its own copy of the folder. Nothing is pushed.",
                         secondary: ("Show task", nil, { inspect(task.recordID) }),
                         primary: ("Approve Plan", KeyboardShortcut("1", modifiers: .command), { ticketCommand("ApproveTask", task.recordID) }),
                         busy: model.busy)
        } else if let task = conversationTickets.last(where: { $0["status"].string == "ReadyForReview" }) {
            ChatApprovalCard(symbol: "checkmark.seal.fill",
                         title: "“\(task["title"].string)” is ready for review",
                         detail: "Its changes and checks are in the agent chat.",
                         secondary: ("Show task", nil, { inspect(task.recordID) }),
                         primary: ("Accept locally", KeyboardShortcut("1", modifiers: .command), { ticketCommand("CompleteTask", task.recordID) }),
                         busy: model.busy)
        }
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
    private var composerState: ComposerActionState {
        ComposerActionState(running: chatQueueOccupied, hasContent: !model.chatDrafts.submission(for: scope).text.isEmpty,
                            submitting: sending, blocked: model.busy || payloadError != nil, queues: true, canStop: replying)
    }
    private var pendingTurn: JSONValue? { messages.last { $0["pending"].bool && $0["role"].string != "user" } ?? messages.last { $0["pending"].bool } }
    private func stopReply() {
        guard let turn = pendingTurn else { return }
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
    private var awaitingApprovals: Int {
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
        guard validateHomePayload(text) else { return }
        model.sendingChatScopes.insert(submission.scope)
        Task {
            let saved = await model.workbench(.command(interrupt ? "InterruptAndSendMessage" : "SendMessage", ["text": .string(text), "workspace_id": submission.scope.workspaceID.map(JSONValue.string) ?? .null]))
            model.chatDrafts.complete(submission, succeeded: saved)
            model.sendingChatScopes.remove(submission.scope)
        }
    }

    /// Sends text that came from a reply view (a choice, a form, Try Again).
    private func post(_ text: String) {
        let target = scope
        guard !text.isEmpty, !model.sendingChatScopes.contains(target), validateHomePayload(text) else { return }
        model.sendingChatScopes.insert(target)
        Task {
            _ = await model.workbench(.command("SendMessage", ["text": .string(text), "workspace_id": target.workspaceID.map(JSONValue.string) ?? .null]))
            model.sendingChatScopes.remove(target)
        }
    }

    private func validateHomePayload(_ text: String) -> Bool {
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

/// One chat message, redrawn only when what it shows changes. The chat sits
/// inside a view that observes the whole app state, which changes every few
/// seconds while agents work; rebuilding every visible message on each change
/// re-parsed its markdown and re-measured its height, and a one-pixel
/// difference made the transcript scroll again.
struct ChatTurn<Content: View>: View, Equatable {
    /// Everything this message shows, and nothing else.
    let key: JSONValue
    @ViewBuilder let content: () -> Content
    nonisolated static func == (a: Self, b: Self) -> Bool { a.key == b.key }
    var body: some View { content() }
}

/// What a message shows besides its own text, gathered once per render
/// rather than searched for by every message.
struct ChatTurnRelated {
    private let receipts: [String: [JSONValue]]
    private let tasks: [String: JSONValue]
    private let watches: [String: JSONValue]
    private let workspaces: [String: String]
    private let connections: JSONValue

    init(snapshot: JSONValue) {
        receipts = Dictionary(grouping: snapshot["mcp"]["receipts"].array, by: { $0["run_id"].string })
        tasks = Dictionary(snapshot["tasks"].array.map { ($0.recordID, $0) }, uniquingKeysWith: { first, _ in first })
        watches = Dictionary(snapshot["mcp"]["responsibilities"].array.map { ($0.recordID, $0) }, uniquingKeysWith: { first, _ in first })
        workspaces = Dictionary(snapshot["workspaces"].array.map { ($0.recordID, $0["name"].string) }, uniquingKeysWith: { first, _ in first })
        connections = .array(snapshot["mcp"]["connections"].array.map { .array([.string($0.recordID), $0["label"]]) })
    }

    func key(for message: JSONValue, awaitingApprovals: Int) -> JSONValue {
        let linked = message["ticket_ids"].array.map { id -> JSONValue in
            let task = tasks[id.string] ?? .null
            return .array([task["title"], task["status"], .string(workspaces[task["workspace_id"].string] ?? "")])
        }
        let watched = message["responsibility_ids"].array.map { watches[$0.string] ?? .null }
        return .object([
            "message": message,
            "receipts": .array(receipts["chat:\(message.recordID)"] ?? []),
            "tasks": .array(linked),
            "watches": .array(watched),
            // A watch row names its connections, so their labels are part of what it shows.
            "connections": watched.isEmpty ? .null : connections,
            "awaiting": .number(Double(awaitingApprovals)),
        ])
    }
}

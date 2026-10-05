import SwiftUI
import AppKit
@preconcurrency import ApplicationServices
import NekoKit

struct ChatScrollMetrics: Equatable {
    var contentHeight: CGFloat = 0
    var originY: CGFloat = 0
    var viewportHeight: CGFloat = 0
}

struct ChatScrollFollowState {
    private(set) var followsLatest = true
    private var previous: ChatScrollMetrics?
    private var pinnedUntil: Date = .distantPast
    /// Sending a message always jumps to it, and the insertion animation's
    /// intermediate offsets must not read as the user scrolling away.
    mutating func pin(for seconds: TimeInterval = 1.2, now: Date = Date()) {
        followsLatest = true
        pinnedUntil = now.addingTimeInterval(seconds)
    }
    mutating func userInteracted() {
        pinnedUntil = .distantPast
        followsLatest = false
    }
    mutating func update(_ metrics: ChatScrollMetrics, now: Date = Date()) {
        if now < pinnedUntil { followsLatest = true; previous = metrics; return }
        // Reply growth must not look like the reader scrolling away. Only an
        // offset change or unchanged content size updates their follow intent.
        if let previous, abs(metrics.contentHeight - previous.contentHeight) < 1 || abs(metrics.originY - previous.originY) > 1 {
            followsLatest = metrics.contentHeight + metrics.originY <= metrics.viewportHeight + 80
        }
        previous = metrics
    }
}

enum ChatComposerClearance {
    static func bottomSpace(for composerHeight: CGFloat) -> CGFloat { composerHeight + 24 }
}

private struct ChatComposerHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

private struct ChatScrollMetricsKey: PreferenceKey {
    static let defaultValue = ChatScrollMetrics()
    static func reduce(value: inout ChatScrollMetrics, nextValue: () -> ChatScrollMetrics) { value = nextValue() }
}

struct ChatToolActivitySummary {
    let calls: [JSONValue]
    let receipts: [JSONValue]
    var showsReceiptsSeparately: Bool { calls.isEmpty }
    var count: Int { showsReceiptsSeparately ? receipts.count : calls.count }
    var succeeded: Int { showsReceiptsSeparately ? receipts.filter { $0["success"].bool }.count : calls.filter { $0["status"].string == "succeeded" }.count }
    var failed: Int { showsReceiptsSeparately ? receipts.filter { !$0["success"].bool }.count : calls.filter { ["failed", "denied"].contains($0["status"].string) }.count }
    var awaitingApproval: Int { calls.filter { $0["status"].string == "awaiting_approval" }.count }
    var title: String {
        let noun = showsReceiptsSeparately ? "tool receipt" : "tool call"
        let warning = awaitingApproval > 0 ? " · \(awaitingApproval) needs approval" : failed > 0 ? " · \(failed) failed" : ""
        return "\(count) \(noun)\(count == 1 ? "" : "s")\(warning)"
    }
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
    @State private var menuIndex = 0
    @State private var menuDismissed: String?
    @State private var mentionFiles: [String] = []
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.nekoLook) private var look
    @Environment(\.ink) private var ink
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
        FullDiskAccessBanner().padding(.bottom, 4)
        HStack(spacing: 0) {
            ZStack(alignment: .bottom) {
                GeometryReader { viewport in
                ScrollViewReader { reader in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: NekoLayout.sectionGap) {
                            if messages.isEmpty {
                                todayHero
                                if model.workspaces.isEmpty { howNekoWorks } else { brief }
                            }
                            ForEach(DecisionPresentation.latest(model.snapshot["decision_records"].array.filter { model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace }).prefix(3), id: \.recordID) { DecisionCard(model: model, record: $0) }
                            ForEach(messages, id: \.recordID) { message in
                                messageView(message)
                                .id(message.recordID)
                                .transition(.opacity)
                            }
                            Color.clear.frame(height: ChatComposerClearance.bottomSpace(for: composerHeight)).id("bottom")
                        }.frame(maxWidth: NekoLayout.readingWidth, alignment: .leading)
                            .padding(.horizontal, NekoLayout.pageInset).padding(.top, NekoLayout.sectionGap).padding(.bottom, 16)
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
                                }.buttonStyle(.bordered).controlSize(.small)
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
        .animation(reduceMotion ? nil : .spring(response: 0.4, dampingFraction: 0.85), value: messages.count)
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
        return VStack(alignment: .leading, spacing: 8) {
            if let notice = toolAccessNotice {
                HStack(spacing: 8) {
                    Image(systemName: "wrench.and.screwdriver").foregroundStyle(ReplyStyle.orange)
                    Text(notice).font(.system(size: 12)).foregroundStyle(.secondary)
                    Spacer()
                    Button("Set Up Tools") {
                        if model.selectedWorkspace == nil, model.workspaces.count == 1 { model.selectedWorkspace = model.workspaces[0].recordID }
                        NotificationCenter.default.post(name: .nekoNavigate, object: "Tools & skills")
                    }.controlSize(.small)
                }.padding(.horizontal, 12).padding(.vertical, 7)
                    .background(ink.panel, in: RoundedRectangle(cornerRadius: 10))
            }
            let items = menuItems
            if !items.isEmpty { composerMenu(items) }
            approvalBar
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
                    .init(value: "plan", title: "Plan", help: "Plan: Neko proposes a ticket with a plan for your approval, without changing anything")
                ], size: .small)
                ComposerRuntimeMenu(model: model)
            }.id(targetScope)
        }
        .frame(width: width)
        .padding(.bottom, 14).padding(.top, 16)
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
        .task(id: mentionQuery) { await searchMentionFiles() }
        .onChange(of: draft) { _, _ in menuIndex = 0 }
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
            Image(systemName: symbol).font(.system(size: 13)).foregroundStyle(.secondary).frame(width: 32, height: 28).contentShape(Rectangle())
        }.buttonStyle(.plain).help(help).accessibilityLabel(help)
    }
    @ViewBuilder private func messageView(_ message: JSONValue) -> some View {
        if message["role"].string == "user" {
            HStack {
                Spacer(minLength: 32)
                VStack(alignment: .trailing, spacing: 8) {
                    ChatAuthorLine(author: "You", timestamp: message["at_ms"].int)
                    Group {
                        if message["text"].string.contains("![") { ReadableText(text: message["text"].string) }
                        else { Text(message["text"].string).textSelection(.enabled) }
                    }
                    .font(NekoFont.chat).lineSpacing(3)
                    .padding(.horizontal, NekoLayout.rowInset).padding(.vertical, 10)
                    .background(ink.raised, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    if message["queued"].bool { Label("Queued", systemImage: "clock").font(NekoFont.meta).foregroundStyle(.secondary) }
                }.frame(maxWidth: 560, alignment: .trailing)
            }
        } else {
            VStack(alignment: .leading, spacing: 10) {
                ChatAuthorLine(author: "Neko", timestamp: message["at_ms"].int)
                if message["pending"].bool {
                    let now = ChatActivity.describe(message, model: model)
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.mini)
                        Text(now.label).font(NekoFont.meta).foregroundStyle(.secondary)
                    }
                }
                let receipts = model.snapshot["mcp"]["receipts"].array.filter { $0["run_id"].string == "chat:\(message.recordID)" }
                let activity = ChatToolActivitySummary(calls: message["tool_calls"].array, receipts: receipts)
                if activity.count > 0 { toolActivity(activity, turn: message.recordID, pending: message["pending"].bool) }
                if message["ticket_ids"].array.isEmpty &&
                    (message["text"].string.contains("I could not open every ticket") || message["text"].string.contains("No ticket was created")) {
                    Label("No ticket was created from this reply", systemImage: "exclamationmark.triangle.fill")
                        .font(.system(size: 12)).foregroundStyle(ReplyStyle.orange)
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
                ForEach(message["ticket_ids"].array, id: \.self) { id in
                    let task = model.snapshot["tasks"].array.first { $0.recordID == id.string } ?? .null
                    ReplyTicketRow(title: task["title"].string.isEmpty ? "Ticket" : task["title"].string,
                                   status: task["status"].string,
                                   workspace: model.workspaces.first { $0.recordID == task["workspace_id"].string }?["name"].string ?? "") {
                        model.openAgent(id.string, from: "Home")
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
            approvalCard(symbol: "hand.raised.fill",
                         title: "Allow \(pending.call["tool_name"].string)\(connection.isEmpty ? "" : " on \(connection)")?",
                         detail: argumentSummary(pending.call["arguments_json"].string),
                         secondary: ("Deny", KeyboardShortcut("2", modifiers: .command), { decide(pending, false) }),
                         primary: ("Allow", KeyboardShortcut("1", modifiers: .command), { decide(pending, true) }))
        } else if let task = conversationTickets.last(where: { $0["status"].string == "AwaitingApproval" && TicketPresentation.waitingReason($0) == nil }) {
            approvalCard(symbol: "exclamationmark.triangle.fill",
                         title: "Approve the plan for “\(task["title"].string)”?",
                         detail: "Work starts in its own copy of the folder. Nothing is pushed.",
                         secondary: ("Show Ticket", nil, { inspect(task.recordID) }),
                         primary: ("Approve Plan", KeyboardShortcut("1", modifiers: .command), { ticketCommand("ApproveTask", task.recordID) }))
        } else if let task = conversationTickets.last(where: { $0["status"].string == "ReadyForReview" }) {
            approvalCard(symbol: "checkmark.seal.fill",
                         title: "“\(task["title"].string)” is ready for review",
                         detail: "Its changes and checks are in the agent chat.",
                         secondary: ("Show Ticket", nil, { inspect(task.recordID) }),
                         primary: ("Accept locally", KeyboardShortcut("1", modifiers: .command), { ticketCommand("CompleteTask", task.recordID) }))
        }
    }
    private func approvalCard(symbol: String, title: String, detail: String,
                              secondary: (String, KeyboardShortcut?, () -> Void),
                              primary: (String, KeyboardShortcut?, () -> Void)) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: symbol).font(NekoFont.body).foregroundStyle(ReplyStyle.orange).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text(title).font(NekoFont.heading).fixedSize(horizontal: false, vertical: true)
                    if !detail.isEmpty { Text(detail).font(NekoFont.meta).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true) }
                }
            }
            HStack(spacing: 8) {
                Spacer(minLength: 0)
                Button(secondary.0, action: secondary.2).keyboardShortcut(secondary.1)
                Button(primary.0, action: primary.2).buttonStyle(.borderedProminent).keyboardShortcut(primary.1)
            }
        }
        .controlSize(.small)
        .disabled(model.busy)
        .padding(.leading, 14).padding(.trailing, 10).padding(.vertical, 9)
        .background(ink.panel, in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(ink.lineStrong))
    }
    private func argumentSummary(_ json: String) -> String {
        guard let value = try? JSONDecoder().decode(JSONValue.self, from: Data(json.utf8)) else { return json }
        return value.object.sorted { $0.key < $1.key }.prefix(3).map { key, value in
            let text: String
            switch value { case .string(let s): text = s; case .number(let n): text = n.formatted(); case .bool(let b): text = b ? "yes" : "no"; default: text = "…" }
            return "\(key): \(text.prefix(40))"
        }.joined(separator: " · ")
    }
    private func decide(_ pending: (turn: String, call: JSONValue), _ approve: Bool) {
        Task { await model.workbench(.command("DecideChatTool", ["turn_id": .string(pending.turn), "call_id": pending.call["id"], "approve": .bool(approve)])) }
    }
    private func ticketCommand(_ command: String, _ id: String) {
        Task { await model.workbench(.command(command, ["task_id": .string(id)])) }
    }
    private func inspect(_ id: String) {
        model.openAgent(id, from: "Home")
    }

    // MARK: / and @ menus

    private struct ComposerMenuItem: Identifiable {
        let id: String
        let symbol: String
        let title: String
        let detail: String
        var shortcut = ""
        let apply: () -> Void
    }
    /// The "@word" being typed at the end of the draft, if any.
    private var mentionQuery: String? {
        guard let last = draft.split(separator: " ", omittingEmptySubsequences: false).last, last.hasPrefix("@"),
              !draft.hasSuffix(" "), menuDismissed != draft else { return nil }
        return String(last.dropFirst())
    }
    private var menuItems: [ComposerMenuItem] {
        guard menuDismissed != draft else { return [] }
        if let query = mentionQuery {
            let q = query.lowercased()
            var items: [ComposerMenuItem] = []
            for workspace in model.workspaces where q.isEmpty || workspace["name"].string.lowercased().contains(q) {
                items.append(ComposerMenuItem(id: "w:" + workspace.recordID, symbol: "folder", title: workspace["name"].string, detail: "Workspace") {
                    replaceMention(with: "Workspace: " + workspace["name"].string)
                })
            }
            for connection in model.snapshot["mcp"]["connections"].array where connection["enabled"].bool && (q.isEmpty || connection["label"].string.lowercased().contains(q)) {
                let count = connection["tools"].array.count
                items.append(ComposerMenuItem(id: "c:" + connection.recordID, symbol: "puzzlepiece.extension", title: connection["label"].string, detail: "\(count) \(count == 1 ? "tool" : "tools")") {
                    replaceMention(with: "@\(connection["label"].string) ")
                })
            }
            for path in mentionFiles {
                items.append(ComposerMenuItem(id: "f:" + path, symbol: "doc", title: (path as NSString).lastPathComponent, detail: ReplyFiles.location(path)) {
                    replaceMention(with: "\(path) ")
                })
            }
            return Array(items.prefix(9))
        }
        let text = draft.trimmingCharacters(in: .whitespaces).lowercased()
        guard text.hasPrefix("/"), !text.contains(" ") else { return [] }
        var items: [ComposerMenuItem] = []
        if "/plan".hasPrefix(text) {
            items.append(ComposerMenuItem(id: "/plan", symbol: "list.number", title: "/plan", detail: "Plan first, start after you approve") { mode = "plan"; draft = "" })
        }
        for entry in SlashCommand.suggestions(for: draft) {
            items.append(ComposerMenuItem(id: entry.name, symbol: "command", title: entry.name, detail: entry.detail, shortcut: entry.name == "/stop" ? "⌘⇧⎋" : "") {
                draft = ["/remember", "/forget"].contains(entry.name) ? entry.name + " " : entry.name
                if !["/remember", "/forget"].contains(entry.name) { send() }
            })
        }
        return items
    }
    private func composerMenu(_ items: [ComposerMenuItem]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                let selected = index == min(menuIndex, items.count - 1)
                Button { item.apply() } label: {
                    HStack(spacing: 10) {
                        Image(systemName: item.symbol).font(.system(size: 12)).frame(width: 16)
                            .foregroundStyle(selected ? Color.white : .secondary)
                        Text(item.title).font(.system(size: 13)).foregroundStyle(selected ? Color.white : .primary).frame(minWidth: 84, alignment: .leading)
                        Text(item.detail).font(.system(size: 12)).foregroundStyle(selected ? Color.white.opacity(0.8) : .secondary).lineLimit(1)
                        Spacer(minLength: 8)
                        if !item.shortcut.isEmpty { Text(item.shortcut).font(.system(size: 12)).foregroundStyle(selected ? Color.white.opacity(0.8) : .secondary) }
                    }
                    .padding(.horizontal, 10).frame(height: 24)
                    .background(selected ? Color(nsColor: .selectedContentBackgroundColor) : .clear, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .onHover { if $0 { menuIndex = index } }
            }
        }
        .padding(5)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.white.opacity(0.12)))
        .shadow(color: .black.opacity(0.35), radius: 16, y: 8)
    }
    private func handleMenuKey(_ key: ComposerMenuKey) -> Bool {
        let items = menuItems
        guard !items.isEmpty else { return false }
        switch key {
        case .up: menuIndex = (min(menuIndex, items.count - 1) - 1 + items.count) % items.count
        case .down: menuIndex = (min(menuIndex, items.count - 1) + 1) % items.count
        case .accept: items[min(menuIndex, items.count - 1)].apply()
        case .dismiss: menuDismissed = draft
        }
        return true
    }
    private func replaceMention(with replacement: String) {
        draft = ComposerMention.replacingTrailingMention(in: draft, with: replacement)
    }
    private func searchMentionFiles() async {
        guard let query = mentionQuery, query.count >= 2 else { mentionFiles = []; return }
        try? await Task.sleep(for: .milliseconds(180))
        guard !Task.isCancelled else { return }
        let safe = query.filter { $0.isLetter || $0.isNumber || "._- ".contains($0) }
        guard !safe.isEmpty else { mentionFiles = []; return }
        let ids = model.selectedWorkspace.map { [$0] } ?? model.workspaces.map(\.recordID)
        let folders = ids.flatMap { id -> [String] in
            let listed = model.snapshot["workspace_folders"][id].array.map(\.string)
            return listed.isEmpty ? [model.workspaces.first { $0.recordID == id }?["repository"].string ?? ""] : listed
        }.filter { !$0.isEmpty }
        guard !folders.isEmpty else { mentionFiles = []; return }
        let found = await Task.detached { () -> [String] in
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/mdfind")
            process.arguments = folders.prefix(6).flatMap { ["-onlyin", $0] } + ["kMDItemFSName == '\(safe)*'cd"]
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = FileHandle.nullDevice
            guard (try? process.run()) != nil else { return [] }
            let timer = DispatchWorkItem { if process.isRunning { process.terminate() } }
            DispatchQueue.global().asyncAfter(deadline: .now() + 1.5, execute: timer)
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            timer.cancel()
            let noisy = ["/node_modules/", "/.git/", "/target/", "/.build/", "/DerivedData/", "/Pods/", "/Library/", "/."]
            return String(decoding: data, as: UTF8.self).split(separator: "\n").map(String.init)
                .filter { path in !noisy.contains { path.contains($0) } }
                .prefix(5).map { $0 }
        }.value
        if !Task.isCancelled { mentionFiles = found }
    }

    private var payloadError: String? {
        ComposerPayload.validationError(ComposerPayload.homeText(model.chatDrafts.submission(for: scope).text, planOnly: mode == "plan"), limit: 4096)
    }
    private var composerState: ComposerActionState {
        ComposerActionState(running: chatQueueOccupied, hasContent: !model.chatDrafts.submission(for: scope).text.isEmpty,
                            submitting: sending, blocked: model.busy || payloadError != nil, queues: true, canStop: replying)
    }
    private var canSend: Bool { composerState.canSubmit }
    private var pendingTurn: JSONValue? { messages.last { $0["pending"].bool && $0["role"].string != "user" } ?? messages.last { $0["pending"].bool } }
    private func stopReply() {
        guard let turn = pendingTurn else { return }
        Task { await model.workbench(.command("CancelChat", ["turn_id": turn["id"]])) }
    }
    /// The user message that produced a failed reply, so Retry can resend it.
    private func retryText(before reply: JSONValue) -> String? {
        guard let index = messages.firstIndex(where: { $0.recordID == reply.recordID }) else { return nil }
        let text = messages[..<index].last { $0["role"].string == "user" }?["text"].string ?? ""
        return text.isEmpty ? nil : text
    }
    private func retry(_ text: String) {
        guard validateHomePayload(text) else { return }
        scrollFollow.pin()
        model.sendingChatScopes.insert(scope)
        let target = scope
        Task {
            _ = await model.workbench(.command("SendMessage", ["text": .string(text), "workspace_id": target.workspaceID.map(JSONValue.string) ?? .null]))
            model.sendingChatScopes.remove(target)
        }
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
    @ViewBuilder private var suggestions: some View {
        EmptyView()
    }
    private func suggestion(_ title: String, _ detail: String, _ prompt: String) -> some View {
        SuggestionRow(title: title, detail: detail) { draft = prompt }
    }
    @ViewBuilder private var legacySuggestions: some View {
        Button("Plan my next step", systemImage: "arrow.triangle.branch") { draft = "Help me decide the next small, useful step in this workspace. Read only and explain your reasoning." }.nekoGlassButton()
        Button("Review a change", systemImage: "doc.text.magnifyingglass") { draft = "Review the current changes in this workspace. Do not modify files; explain risks and missing tests." }.nekoGlassButton()
        Button("Remember a preference", systemImage: "bookmark") { draft = "Remember this preference: " }.nekoGlassButton()
    }
    private var greeting: some View {
        let h = Calendar.current.component(.hour, from: Date())
        let part = h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"
        let waiting = model.tasks.filter { ["AwaitingApproval", "ReadyForReview", "Failed"].contains($0["status"].string) }.count
        return VStack(alignment: .leading, spacing: 8) {
            if look == .mascot {
                BrandMark(size: 84).clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
                    .shadow(color: NekoStyle.accent.opacity(0.35), radius: 24, y: 10).padding(.bottom, 12)
            }
            Text(Date.now.formatted(.dateTime.weekday(.wide).day().month(.wide))).font(.system(size: 12)).foregroundStyle(N.text4)
            Text("\(part).").font(.system(size: look == .mascot ? 34 : look == .dense ? 20 : 26, weight: .semibold)).tracking(-0.5).foregroundStyle(N.text)
            Text(waiting == 0 ? (WatchingPresentation(connected: model.connected, snapshot: model.snapshot).active ? "Nothing needs you right now. I'm watching." : "Connect a work source and start a watch to get useful updates here.") : "\(waiting) \(waiting == 1 ? "thing needs" : "things need") you. Review it in All agents.")
                .font(.system(size: 14)).lineSpacing(4).foregroundStyle(N.text3)
        }
    }
    private var todayHero: some View {
        let h = Calendar.current.component(.hour, from: Date())
        let part = h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"
        let watched = model.snapshot["mcp"]["responsibilities"].array.filter {
            $0["enabled"].bool && (model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace)
        }
        return VStack(alignment: .leading, spacing: 12) {
            Text(Date.now.formatted(.dateTime.weekday(.wide).day().month(.wide)))
                .font(.system(size: 12, weight: .medium)).foregroundStyle(N.text4)
            Text(workSummary.needsYou.isEmpty ? "\(part). All quiet for now." : "\(part). There's work to review.")
                .font(NekoFont.title).foregroundStyle(N.text)
                .fixedSize(horizontal: false, vertical: true)
            Text(model.workspaces.isEmpty ? "Add a workspace to give Neko somewhere to start." : "Ask about your work, or let Neko watch for changes. You'll review anything it wants to do.")
                .font(.system(size: 14)).foregroundStyle(N.text3)
                .fixedSize(horizontal: false, vertical: true)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 140), alignment: .leading)], alignment: .leading, spacing: 8) {
                Label("\(workSummary.needsYou.count) needs you", systemImage: "hand.raised")
                Label("\(workSummary.working.count) in progress", systemImage: "circle.dotted")
                Label("\(watched.count) watching", systemImage: "eye")
            }
            .font(.system(size: 12)).foregroundStyle(N.text3)
            .padding(.top, 4)
            Button(watched.isEmpty ? "Start watching" : "Check now") {
                if watched.isEmpty {
                    if model.workspaces.isEmpty { addingWorkspace = true } else { NotificationCenter.default.post(name: .nekoNavigate, object: "Responsibilities") }
                } else {
                    for item in watched { Task { await model.workbench(.object(["Mcp": .command("Wake", ["responsibility_id": item["id"]])])) } }
                }
            }
            .nekoPrimaryButton()
            .accessibilityLabel(watched.isEmpty ? "Start watching" : "Check everything now")
        }
        .padding(.top, NekoLayout.sectionGap)
    }
    private var howNekoWorks: some View {
        let watching = model.snapshot["mcp"]["responsibilities"].array.contains { $0["enabled"].bool }
        return VStack(alignment: .leading, spacing: 0) {
            Text("How Neko works").font(.system(size: 12, weight: .medium)).foregroundStyle(N.text4).padding(.bottom, 8)
            LoopStep(number: 1, title: "Watch", detail: "Add a folder you work in, then the sources to keep an eye on: Linear, Sentry, GitHub, Slack.", done: !model.workspaces.isEmpty && watching,
                     action: model.workspaces.isEmpty ? "Add a workspace" : "Choose what to watch") {
                if model.workspaces.isEmpty { addingWorkspace = true } else { NotificationCenter.default.post(name: .nekoNavigate, object: "Responsibilities") }
            }
            LoopStep(number: 2, title: "Plan", detail: "When something changes, Neko reads it and drafts the next step the way you would. Read only.", done: !model.tasks.isEmpty, action: "Try a plan") {
                draft = "Look at this workspace and plan the most useful next step. Read only; explain your reasoning."
            }
            LoopStep(number: 3, title: "Ask you", detail: "Nothing changes without your approval. Plans and finished fixes wait for you in All agents.", done: model.tasks.contains { $0["status"].string == "Completed" }, action: "Open All agents") {
                NotificationCenter.default.post(name: .nekoNavigate, object: "Tickets")
            }
        }
    }
    @ViewBuilder private var brief: some View {
        let recent = Array((workSummary.needsYou + workSummary.working).prefix(5))
        VStack(alignment: .leading, spacing: 0) {
            Text(recent.isEmpty ? "Try asking" : "While you were away").font(.system(size: 12, weight: .medium)).foregroundStyle(N.text4).padding(.bottom, 8)
            if recent.isEmpty {
                BriefRow(dot: N.text4, title: "Plan my next step", meta: "Decide the next small, useful move · read only", action: "Ask") { draft = "Help me decide the next small, useful step in this workspace. Read only and explain your reasoning." }
                BriefRow(dot: N.text4, title: "Review a change", meta: "Risks and missing tests · no edits", action: "Ask") { draft = "Review the current changes in this workspace. Do not modify files; explain risks and missing tests." }
                BriefRow(dot: N.text4, title: "Remember a preference", meta: "Teach Neko how you work", action: "Ask") { draft = "Remember this preference: " }
            } else {
                ForEach(Array(recent), id: \.recordID) { task in
                    let status = task["status"].string
                    BriefRow(dot: taskColor(status), title: task["title"].string, meta: friendlyTaskStatus(status) + (task["goal"].string.isEmpty ? "" : " · " + task["goal"].string), action: status == "AwaitingApproval" ? "Review plan" : status == "ReadyForReview" ? "Review" : "Open", status: status) { model.openAgent(task.recordID, from: "Home") }
                }
            }
        }
    }
    private func toolActivity(_ activity: ChatToolActivitySummary, turn: String, pending: Bool) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if activity.showsReceiptsSeparately || activity.calls.contains(where: { $0["status"].string != "awaiting_approval" }) {
                DisclosureGroup {
                VStack(alignment: .leading, spacing: 0) {
                    if activity.showsReceiptsSeparately {
                        ForEach(activity.receipts, id: \.recordID) { receipt in
                            HStack(spacing: 7) {
                                Image(systemName: receipt["success"].bool ? "checkmark" : "exclamationmark.triangle")
                                Text(receipt["tool_name"].string)
                                Spacer()
                                Text(receipt["success"].bool ? "Done" : "Failed")
                            }
                            .font(.system(size: 12))
                            .foregroundStyle(receipt["success"].bool ? N.text3 : NekoStyle.amber)
                            .padding(.vertical, 7)
                            Divider().opacity(0.4)
                        }
                    } else {
                        ForEach(activity.calls.filter { $0["status"].string != "awaiting_approval" }, id: \.recordID) { call in
                            toolCall(call, turn: turn, pending: pending)
                        }
                    }
                }.padding(.top, 8)
            } label: {
                Label(activity.title, systemImage: activity.failed > 0 ? "exclamationmark.circle" : pending ? "circle.dotted" : "checkmark.circle")
                    .font(NekoFont.meta)
                    .foregroundStyle(activity.failed > 0 || activity.awaitingApproval > 0 ? NekoStyle.amber : N.text3)
                }
            } else {
                Label(activity.title, systemImage: "hand.raised")
                    .font(.system(size: 12, weight: .medium)).foregroundStyle(NekoStyle.amber)
            }
            ForEach(activity.calls.filter { $0["status"].string == "awaiting_approval" }, id: \.recordID) { call in
                toolCall(call, turn: turn, pending: pending)
            }
        }
        .padding(.top, 4)
    }
    private var awaitingApprovals: Int {
        messages.filter { $0["pending"].bool }.flatMap { $0["tool_calls"].array }.filter { $0["status"].string == "awaiting_approval" }.count
    }
    private func toolCall(_ call: JSONValue, turn: String, pending: Bool) -> some View {
        func decide(_ approve: Bool) { Task { await model.workbench(.command("DecideChatTool", ["turn_id": .string(turn), "call_id": call["id"], "approve": .bool(approve)])) } }
        let status = call["status"].string
        let connection = model.snapshot["mcp"]["connections"].array.first { $0.recordID == call["connection_id"].string }?["label"].string ?? "Removed connection"
        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image(systemName: status == "failed" || status == "denied" ? "exclamationmark.circle" : status == "awaiting_approval" ? "hand.raised" : "checkmark")
                    .foregroundStyle(status == "failed" || status == "denied" || status == "awaiting_approval" ? NekoStyle.amber : N.text4)
                    .frame(width: 14)
                Text(call["tool_name"].string).fontWeight(.medium)
                Text("· \(connection)").foregroundStyle(N.text4).lineLimit(1)
                Spacer(minLength: 8)
                Text(status.replacingOccurrences(of: "_", with: " ").capitalized).foregroundStyle(N.text4)
            }.font(.system(size: 12))
            DisclosureGroup("Request details") {
                Text(call["arguments_json"].string).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
            }.font(.system(size: 11)).foregroundStyle(N.text4).padding(.leading, 22)
            if pending && call["status"].string == "awaiting_approval" {
                // Keyboard answers only when exactly one request is waiting, so ⌘1/⌘2 is never ambiguous.
                let only = awaitingApprovals == 1
                HStack {
                    Button("Deny", role: .destructive) { decide(false) }.keyboardShortcut(only ? KeyboardShortcut("2", modifiers: .command) : nil)
                    Button("Allow this request") { decide(true) }.nekoPrimaryButton().keyboardShortcut(only ? KeyboardShortcut("1", modifiers: .command) : nil)
                    if only { Text("⌘1 allow · ⌘2 deny").font(.system(size: 11)).foregroundStyle(N.text4) }
                }
                .padding(.leading, 22).disabled(model.busy)
            }
            Divider().opacity(0.4)
        }.padding(.vertical, 5)
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
        let open = { (page: String) in NotificationCenter.default.post(name: .nekoNavigate, object: page) }
        switch command {
        case .stop: Task { await StopAllWork.run(model) }
        case .clearFinished:
            Task {
                let before = model.snapshot["tasks"].array.count
                if await model.workbench(.command("ClearFinishedTasks", ["workspace_id": scope.workspaceID.map(JSONValue.string) ?? .null])) {
                    let removed = before - model.snapshot["tasks"].array.count
                    model.notice = removed == 0 ? "No finished tickets to clear." : "Cleared \(removed) finished \(removed == 1 ? "ticket" : "tickets"). Files and worktrees are untouched."
                }
            }
        case .memory: open("Memory")
        case .tickets: open("Tickets")
        case .models, .permissions: open("Settings")
        case .setup: model.onboarding = true
        case .help: draft = "/"
        case .remember, .forget, .recall: break
        }
    }
}
struct SuggestionRow: View {
    let title: String
    let detail: String
    let action: () -> Void
    @State private var hover = false
    @Environment(\.ink) private var ink
    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Text(title).font(.system(size: 13, weight: .medium))
                Text("·").foregroundStyle(.tertiary)
                Text(detail).font(.system(size: 13)).foregroundStyle(.secondary).lineLimit(1)
                Spacer()
                Image(systemName: "arrow.right").font(.system(size: 11)).foregroundStyle(.tertiary).offset(x: hover ? 2 : 0).opacity(hover ? 1 : 0)
            }.padding(.horizontal, 10).padding(.vertical, 8)
            .background(hover ? ink.raised : .clear, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .contentShape(Rectangle())
        }.buttonStyle(.plain).onHover { h in withAnimation(.easeOut(duration: 0.12)) { hover = h } }
    }
}


struct BriefRow: View {
    let dot: Color
    let title: String
    let meta: String
    let action: String
    var status: String? = nil
    let perform: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: perform) {
            HStack(alignment: .top, spacing: 12) {
                Group {
                    if let status { StatusGlyph(status: status, size: 14) }
                    else { Image(systemName: "arrow.up.right").font(NekoFont.meta).foregroundStyle(dot) }
                }.frame(width: 16, height: 20).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.system(size: 13)).foregroundStyle(N.text).lineLimit(1)
                    Text(meta).font(.system(size: 12)).foregroundStyle(N.text3).lineLimit(1)
                }
                Spacer(minLength: 12)
                Text(action).font(.system(size: 12, weight: .medium)).foregroundStyle(hover ? N.text : N.text2)
                    .padding(.horizontal, 6).frame(height: 26)
                    .padding(.top, 3)
            }
            .padding(.vertical, 10).padding(.horizontal, 8)
            .background(hover ? Color.white.opacity(0.025) : .clear, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
            .overlay(alignment: .top) { N.line.frame(height: 1).padding(.horizontal, 8) }
            .padding(.horizontal, -8)
            .contentShape(Rectangle())
        }.buttonStyle(.plain).onHover { h in withAnimation(.easeOut(duration: 0.12)) { hover = h } }
    }
}

struct RailRow: View {
    let title: String
    let meta: String
    let status: String
    let open: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: open) {
            HStack(spacing: 10) {
                StatusGlyph(status: status, size: 14)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.system(size: 13)).foregroundStyle(N.text).lineLimit(1)
                    Text(meta).font(.system(size: 12)).foregroundStyle(N.text3).lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 20).frame(height: 52)
            .background(hover ? Color.white.opacity(0.025) : .clear)
            .overlay(alignment: .bottom) { Color.white.opacity(0.05).frame(height: 1) }
            .contentShape(Rectangle())
        }.buttonStyle(.plain).onHover { hover = $0 }
    }
}

extension Notification.Name { static let nekoNavigate = Notification.Name("neko.navigate") }

struct LoopStep: View {
    let number: Int
    let title: String
    let detail: String
    let done: Bool
    let action: String
    let perform: () -> Void
    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            ZStack {
                Circle().strokeBorder(done ? NekoStyle.mint : N.lineStrong, lineWidth: 1.5)
                if done { Image(systemName: "checkmark").font(.system(size: 10, weight: .bold)).foregroundStyle(NekoStyle.mint) }
                else { Text("\(number)").font(.system(size: 11, weight: .semibold).monospacedDigit()).foregroundStyle(N.text3) }
            }.frame(width: 22, height: 22).padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.system(size: 13.5, weight: .semibold)).foregroundStyle(N.text)
                Text(detail).font(.system(size: 12.5)).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 16)
            Button(action, action: perform).nekoGlassButton().controlSize(.small).padding(.top, 2)
        }
        .padding(.vertical, 12)
        .overlay(alignment: .top) { N.line.frame(height: 1) }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Step \(number), \(title)\(done ? ", done" : ""). \(detail)")
    }
}

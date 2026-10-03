import SwiftUI
import AppKit
@preconcurrency import ApplicationServices
import NekoKit

struct ChatDraftScope: Hashable {
    let workspaceID: String?
    let profileID: String
}

struct ScopedChatDrafts {
    struct Submission { let scope: ChatDraftScope; let text: String; let revision: Int }
    private var values: [ChatDraftScope: String] = [:]
    private var attached: [ChatDraftScope: [ComposerAttachment]] = [:]
    private var revisions: [ChatDraftScope: Int] = [:]
    func text(for scope: ChatDraftScope) -> String { values[scope] ?? "" }
    func attachments(for scope: ChatDraftScope) -> [ComposerAttachment] { attached[scope] ?? [] }
    mutating func add(_ attachment: ComposerAttachment, for scope: ChatDraftScope) {
        guard !attachments(for: scope).contains(attachment) else { return }
        attached[scope, default: []].append(attachment)
        revisions[scope, default: 0] += 1
    }
    mutating func remove(_ attachment: ComposerAttachment, for scope: ChatDraftScope) {
        attached[scope]?.removeAll { $0 == attachment }
        revisions[scope, default: 0] += 1
    }
    mutating func set(_ text: String, for scope: ChatDraftScope) {
        guard values[scope] != text else { return }
        values[scope] = text
        revisions[scope, default: 0] += 1
    }
    func submission(for scope: ChatDraftScope) -> Submission {
        let parts = [text(for: scope).trimmingCharacters(in: .whitespacesAndNewlines)] + attachments(for: scope).map(\.reference)
        return Submission(scope: scope, text: parts.filter { !$0.isEmpty }.joined(separator: "\n"), revision: revisions[scope, default: 0])
    }
    mutating func complete(_ submission: Submission, succeeded: Bool) {
        guard succeeded, revisions[submission.scope, default: 0] == submission.revision else { return }
        set("", for: submission.scope)
        attached[submission.scope] = []
    }
}

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
    @State private var ticket: String?
    @State private var addingWorkspace = false
    @State private var composerHeight: CGFloat = 0
    @State private var catalog = ModelCatalog()
    @State private var inspected: String?
    @AppStorage("neko.today.inspector") private var showInspector = true
    @AppStorage("neko.composer.mode") private var mode = "ask"
    @State private var menuIndex = 0
    @State private var menuDismissed: String?
    @State private var mentionFiles: [String] = []
    @Environment(\.sidebarCollapsed) private var sidebarCollapsed
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.nekoLook) private var look
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
    private var replying: Bool { model.snapshot["conversation"].array.contains { $0["pending"].bool } }
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
        todayHeader
        FullDiskAccessBanner().padding(.bottom, 4)
        HStack(spacing: 0) {
            ZStack(alignment: .bottom) {
                GeometryReader { viewport in
                ScrollViewReader { reader in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 24) {
                            if messages.isEmpty {
                                todayHero
                                if model.workspaces.isEmpty { howNekoWorks } else { brief }
                            }
                            ForEach(messages, id: \.recordID) { message in
                                messageView(message)
                                .id(message.recordID)
                                .transition(.asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity), removal: .opacity))
                            }
                            Color.clear.frame(height: ChatComposerClearance.bottomSpace(for: composerHeight)).id("bottom")
                        }.padding(.horizontal, 32).padding(.top, 12).padding(.bottom, 24).frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity, alignment: .center)
                            .background(GeometryReader { geometry in
                                Color.clear.preference(key: ChatScrollMetricsKey.self, value: ChatScrollMetrics(contentHeight: geometry.size.height, originY: geometry.frame(in: .named("chatTranscript")).minY, viewportHeight: viewport.size.height))
                            })
                    }.coordinateSpace(name: "chatTranscript")
                        .onPreferenceChange(ChatScrollMetricsKey.self) { metrics in
                            let resized = abs(metrics.contentHeight - transcriptHeight) > 1
                            transcriptHeight = metrics.contentHeight
                            scrollFollow.update(metrics)
                            // Layout has now measured the completed reply, so
                            // scroll to its new bottom rather than the old one.
                            if resized, scrollFollow.followsLatest { DispatchQueue.main.async { reader.scrollTo("bottom", anchor: .bottom) } }
                        }
                        .onChange(of: messages.last) { _, _ in
                            if messages.last?["role"].string == "user" { scrollFollow.pin() }
                            guard scrollFollow.followsLatest else { return }
                            reader.scrollTo("bottom", anchor: .bottom)
                            DispatchQueue.main.async { reader.scrollTo("bottom", anchor: .bottom) }
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { if scrollFollow.followsLatest { reader.scrollTo("bottom", anchor: .bottom) } }
                        }
                        .onChange(of: messages.count) { old, new in
                            guard new > old else { return }
                            guard messages.suffix(new - old).contains(where: { $0["role"].string == "user" }) else { return }
                            scrollFollow.pin()
                            DispatchQueue.main.async { reader.scrollTo("bottom", anchor: .bottom) }
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { reader.scrollTo("bottom", anchor: .bottom) }
                        }
                        .onChange(of: scope) { _, _ in
                            scrollFollow = ChatScrollFollowState()
                            reader.scrollTo("bottom", anchor: .bottom)
                        }
                        .onAppear {
                            reader.scrollTo("bottom", anchor: .bottom)
                            // The inspector and composer settle a moment later; follow them.
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { reader.scrollTo("bottom", anchor: .bottom) }
                        }
                }
                }
                composer(availableWidth: outer.size.width)
            }
            // Keep the transcript below the header row now that it reaches the window top.
            .clipped()
        }
        }
        }
        .environment(\.replyActions, ReplyActions(send: { text in post(text) }, draft: { text in draft = text }))
        // The inspector re-adds the toolbar inset; the header row is the toolbar.
        .ignoresSafeArea(.container, edges: .top)
        // Opens when you click a ticket in the conversation; ✕ closes it.
        .sidePanel(isPresented: showInspector && inspectedTicket != nil, key: "neko.today.panelWidth", range: 260...480, ideal: 300) {
            VStack(spacing: 0) {
                HStack {
                    Spacer()
                    Button { withAnimation(.snappy(duration: 0.25)) { showInspector = false } } label: {
                        Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary).frame(width: 24, height: 24).contentShape(Rectangle())
                    }.buttonStyle(.plain).help("Close").accessibilityLabel("Close ticket panel")
                }.padding(.horizontal, 10).padding(.top, 14)
                TicketInspector(model: model, id: inspectedTicket, openFull: { ticket = $0 })
            }
            .ignoresSafeArea(.container, edges: .top)
        }
        .onChange(of: scope) { _, _ in inspected = nil }
        .animation(reduceMotion ? nil : .spring(response: 0.4, dampingFraction: 0.85), value: messages.count)
        .task { catalog = await AgentModelCatalog.load(model) }
        .onAppear {
            guard escapeMonitor == nil else { return }
            escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                guard event.keyCode == 53, !event.isARepeat else { return event }
                guard pendingTurn != nil else { lastEscape = .distantPast; return event }
                let now = Date()
                if now.timeIntervalSince(lastEscape) < 0.6 { lastEscape = .distantPast; stopReply(); return nil }
                lastEscape = now
                return event
            }
        }
        .onDisappear { if let monitor = escapeMonitor { NSEvent.removeMonitor(monitor) }; escapeMonitor = nil }
        .sheet(isPresented: Binding(get: { ticket != nil }, set: { if !$0 { ticket = nil } })) {
            if let id = ticket { VStack { HStack { Spacer(); Button("Done") { ticket = nil }.keyboardShortcut(.cancelAction) }.padding(); TicketDetail(model: model, id: id) }.frame(minWidth: 650, minHeight: 600) }
        }.sheet(isPresented: $addingWorkspace) { WorkspaceEditor(model: model) }
    }
    private func composer(availableWidth: CGFloat) -> some View {
        let width = min(720, max(360, availableWidth - 96))
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
                }.padding(.horizontal, 12).padding(.vertical, 7).liquidGlass(radius: 12)
            }
            let items = menuItems
            if !items.isEmpty { composerMenu(items) }
            approvalBar
            VStack(alignment: .leading, spacing: 6) {
                if !attachments.isEmpty { attachmentStrip }
                ComposerView(
                    text: Binding(get: { draft }, set: { draft = $0 }),
                    onSubmit: { send() },
                    onInterruptAndSubmit: { send(interrupt: true) },
                    onAttach: { model.chatDrafts.add($0, for: scope) },
                    onError: { model.error = $0 },
                    onSubmitWithContext: { send(withContext: true) },
                    onMenuKey: { handleMenuKey($0) }
                ).id(scope)
                HStack(spacing: 8) {
                    Button(action: chooseAttachments) {
                        Image(systemName: "plus").font(.system(size: 12, weight: .semibold))
                            .foregroundStyle(.secondary).frame(width: 24, height: 24)
                            .background(Color.white.opacity(0.1), in: Circle())
                    }
                    .buttonStyle(.plain)
                    .help("Attach images or files; you can also paste or drop them")
                    .accessibilityLabel("Attach images or files")
                    GlassSegmented(selection: $mode, options: [
                        .init(value: "ask", title: "Ask", help: "Ask: Neko answers now"),
                        .init(value: "plan", title: "Plan", help: "Plan: Neko proposes a ticket with a plan for your approval, without changing anything")
                    ], size: .small)
                    runtimeMenu
                    Spacer(minLength: 8)
                    Button { send() } label: {
                        Group {
                            if sending { ProgressView().controlSize(.mini) }
                            else { Image(systemName: "arrow.up").font(.system(size: 12, weight: .bold)) }
                        }
                        .foregroundStyle(canSend ? Color.white : Color.secondary)
                        .frame(width: 26, height: 26)
                        .background(canSend ? NekoStyle.accent : Color.white.opacity(0.1), in: Circle())
                    }
                    .buttonStyle(.plain)
                    .disabled(!canSend)
                    .accessibilityLabel(sending ? "Sending message" : replying ? "Queue message" : "Send message")
                }
            }
            .padding(.horizontal, 14).padding(.top, 8).padding(.bottom, 8)
            .liquidGlass(radius: 20)
        }
        .frame(width: width)
        .padding(.bottom, 16).padding(.top, 28)
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
    private var inspectedTicket: String? { inspected ?? conversationTickets.last?.recordID }
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
    private var todayHeader: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 1) {
                Text("Today").font(.system(size: 15, weight: .semibold)).lineLimit(1)
                Text(headerSubtitle).font(.system(size: 11)).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 12)
            // Work in the sidebar already counts tickets that need you, and a
            // ticket's panel opens from the conversation, so only Stop lives here.
            if replying {
                toolbarIcon("stop.fill", help: "Stop the current reply") {
                    if let current = messages.last(where: { $0["pending"].bool }) { Task { await model.workbench(.command("CancelChat", ["turn_id": current["id"]])) } }
                }.padding(.horizontal, 2).frame(height: 28).liquidGlassCapsule()
            }
        }
        .padding(.leading, sidebarCollapsed ? 88 : 20).padding(.trailing, 14)
        .frame(height: 52)
    }
    private func toolbarIcon(_ symbol: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 13)).foregroundStyle(.secondary).frame(width: 32, height: 28).contentShape(Rectangle())
        }.buttonStyle(.plain).help(help).accessibilityLabel(help)
    }
    @ViewBuilder private func messageView(_ message: JSONValue) -> some View {
        if message["role"].string == "user" {
            HStack {
                Spacer(minLength: 120)
                VStack(alignment: .trailing, spacing: 4) {
                    Group {
                        if message["text"].string.contains("![") { ReadableText(text: message["text"].string) }
                        else { Text(message["text"].string).textSelection(.enabled) }
                    }
                    .font(.system(size: 13)).lineSpacing(2)
                    .padding(.horizontal, 12).padding(.vertical, 7)
                    .background(Color.white.opacity(0.11), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                    if message["queued"].bool { Text("Queued").font(.system(size: 11)).foregroundStyle(.tertiary) }
                }.frame(maxWidth: 520, alignment: .trailing)
            }
        } else {
            VStack(alignment: .leading, spacing: 10) {
                if message["pending"].bool {
                    let now = ChatActivity.describe(message, model: model)
                    ActivityCapsule(activity: now.activity, label: now.label, plain: true)
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
                    ReadableText(text: message["text"].string).font(.system(size: 13)).lineSpacing(3)
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
                        inspected = id.string
                        withAnimation(.snappy(duration: 0.25)) { showInspector = true }
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
        } else if let task = conversationTickets.last(where: { $0["status"].string == "AwaitingApproval" }) {
            approvalCard(symbol: "exclamationmark.triangle.fill",
                         title: "Approve the plan for “\(task["title"].string)”?",
                         detail: "Work starts in its own copy of the folder. Nothing is pushed.",
                         secondary: ("Show Ticket", nil, { inspect(task.recordID) }),
                         primary: ("Approve Plan", KeyboardShortcut("1", modifiers: .command), { ticketCommand("ApproveTask", task.recordID) }))
        } else if let task = conversationTickets.last(where: { $0["status"].string == "ReadyForReview" }) {
            approvalCard(symbol: "checkmark.seal.fill",
                         title: "“\(task["title"].string)” is ready for review",
                         detail: "Its changes and checks are in the inspector.",
                         secondary: ("Show Ticket", nil, { inspect(task.recordID) }),
                         primary: ("Mark Complete", KeyboardShortcut("1", modifiers: .command), { ticketCommand("CompleteTask", task.recordID) }))
        }
    }
    private func approvalCard(symbol: String, title: String, detail: String,
                              secondary: (String, KeyboardShortcut?, () -> Void),
                              primary: (String, KeyboardShortcut?, () -> Void)) -> some View {
        HStack(spacing: 10) {
            Image(systemName: symbol).font(.system(size: 16)).foregroundStyle(ReplyStyle.orange)
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                if !detail.isEmpty { Text(detail).font(.system(size: 11)).foregroundStyle(.secondary).lineLimit(1) }
            }
            Spacer(minLength: 8)
            Button(secondary.0, action: secondary.2).keyboardShortcut(secondary.1)
            Button(primary.0, action: primary.2).buttonStyle(.borderedProminent).keyboardShortcut(primary.1)
        }
        .controlSize(.regular)
        .disabled(model.busy)
        .padding(.leading, 14).padding(.trailing, 10).padding(.vertical, 9)
        .liquidGlass(radius: 14)
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
        inspected = id
        withAnimation(.snappy(duration: 0.25)) { showInspector = true }
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
                    model.selectedWorkspace = workspace.recordID
                    replaceMention(with: "")
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
        var words = draft.components(separatedBy: " ")
        if let last = words.last, last.hasPrefix("@") { words[words.count - 1] = replacement.trimmingCharacters(in: .whitespaces) }
        let joined = words.joined(separator: " ").trimmingCharacters(in: .whitespaces)
        draft = joined.isEmpty ? "" : joined + " "
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

    private var runtimeMenu: some View {
        Menu {
            ForEach(catalog.sources) { source in
                Section(source.connection) {
                    if source.ready {
                        Button(source.defaultTitle) { selectRuntime(source.provider) }
                        ForEach(source.models) { entry in
                            Button(menuTitle(entry)) { selectRuntime(source.provider, model: entry.id) }
                                .disabled(!entry.usable)
                                .help(entry.reason ?? entry.description ?? "")
                        }
                    } else if let note = source.note {
                        Text(note)
                    }
                }
            }
            Button("Refresh models") { Task { catalog = await AgentModelCatalog.load(model, refresh: true) } }
            Divider()
            Button("Model settings…") { NotificationCenter.default.post(name: .nekoNavigate, object: "Settings") }
        } label: {
            HStack(spacing: 4) {
                Text(runtimeLabel).font(.system(size: 12))
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 8, weight: .semibold))
            }.foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("Choose the agent runtime. Local providers need a running Ollama or LM Studio server.")
    }

    private var canSend: Bool { !model.chatDrafts.submission(for: scope).text.isEmpty && !sending && !model.busy }
    private var pendingTurn: JSONValue? { messages.last { $0["pending"].bool && $0["role"].string != "user" } ?? messages.last { $0["pending"].bool } }
    /// With an empty draft the send button becomes Stop; typing while a reply
    /// runs turns it back into Send so the new message queues.
    private var showsStop: Bool { pendingTurn != nil && !sending && model.chatDrafts.submission(for: scope).text.isEmpty }
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
        scrollFollow.pin()
        model.sendingChatScopes.insert(scope)
        let target = scope
        Task {
            _ = await model.workbench(.command("SendMessage", ["text": .string(text), "workspace_id": target.workspaceID.map(JSONValue.string) ?? .null]))
            model.sendingChatScopes.remove(target)
        }
    }

    private var runtimeLabel: String {
        let runtime = model.snapshot["agent_runtime"]
        return AgentModelCatalog.label(provider: runtime["provider"].string, model: runtime["model"].string, catalog: catalog)
    }

    private func menuTitle(_ entry: CatalogModel) -> String {
        let selected = model.snapshot["agent_runtime"]["model"].string == entry.id
        var title = (selected ? "✓ " : "") + entry.label
        if entry.recommended { title += " · Recommended" }
        if entry.access == .checked { title += " · Checked" }
        if !entry.usable { title += " · Unavailable" }
        return title
    }

    private func selectRuntime(_ provider: String, model selectedModel: String = "") {
        Task { await model.workbench(.command("SetAgentRuntime", ["runtime": .object(["provider": .string(provider), "model": .string(selectedModel)])])) }
    }

    private var attachmentStrip: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 6) {
                ForEach(attachments) { attachment in
                    HStack(spacing: 6) {
                        Image(systemName: attachment.isImage ? "photo" : "doc")
                        Text(attachment.name).lineLimit(1)
                        Button { model.chatDrafts.remove(attachment, for: scope) } label: {
                            Image(systemName: "xmark").font(.system(size: 9, weight: .bold))
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Remove \(attachment.name)")
                    }
                    .font(.system(size: 11)).foregroundStyle(N.text2)
                    .padding(.horizontal, 9).frame(height: 26).liquidGlassCapsule()
                }
            }
        }.scrollIndicators(.never)
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
            Text(waiting == 0 ? (WatchingPresentation(connected: model.connected, snapshot: model.snapshot).active ? "Nothing needs you right now. I'm watching." : "Connect a work source and start a watch to get useful updates here.") : "\(waiting) \(waiting == 1 ? "thing needs" : "things need") you. Review it in Work.")
                .font(.system(size: 14)).lineSpacing(4).foregroundStyle(N.text3)
        }
    }
    private var todayHero: some View {
        let h = Calendar.current.component(.hour, from: Date())
        let part = h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"
        let watched = model.snapshot["mcp"]["responsibilities"].array.filter {
            $0["enabled"].bool && (model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace)
        }
        return VStack(alignment: .leading, spacing: 18) {
            Text(Date.now.formatted(.dateTime.weekday(.wide).day().month(.wide)))
                .font(.system(size: 12, weight: .medium)).foregroundStyle(N.text4)
            Text(workSummary.needsYou.isEmpty ? "\(part). All quiet for now." : "\(part). There's work to review.")
                .font(.system(size: 28, weight: .semibold)).tracking(-0.7).foregroundStyle(N.text)
                .fixedSize(horizontal: false, vertical: true)
            Text(model.workspaces.isEmpty ? "Add a workspace to give Neko somewhere to start." : "Ask about your work, or let Neko watch for changes. You'll review anything it wants to do.")
                .font(.system(size: 14)).foregroundStyle(N.text3)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 18) {
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
        .padding(.top, 56)
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
            LoopStep(number: 3, title: "Ask you", detail: "Nothing changes without your approval. Plans and finished fixes wait for you in Work.", done: model.tasks.contains { $0["status"].string == "Completed" }, action: "Open Work") {
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
                    BriefRow(dot: taskColor(status), title: task["title"].string, meta: friendlyTaskStatus(status) + (task["goal"].string.isEmpty ? "" : " · " + task["goal"].string), action: status == "AwaitingApproval" ? "Review plan" : status == "ReadyForReview" ? "Review" : "Open") { ticket = task.recordID }
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
                Label(activity.title, systemImage: activity.failed > 0 ? "exclamationmark.circle" : "checkmark.circle")
                    .font(.system(size: 12, weight: .medium))
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
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        let selectedScope = scope
        panel.begin { response in
            guard response == .OK else { return }
            Task { @MainActor in
                for url in panel.urls {
                    do { model.chatDrafts.add(try ComposerAttachmentStore.saveFile(url), for: selectedScope) }
                    catch { model.error = error.localizedDescription }
                }
            }
        }
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
        var text = SlashCommand.parse(submission.text)?.chatText ?? submission.text
        if mode == "plan", SlashCommand.parse(submission.text) == nil {
            text = "Plan only, and change nothing yet: propose a ticket with a short step-by-step plan for my approval.\n\n" + text
        }
        if withContext {
            if let snapshot = PreviousAppContext.shared.read() {
                text = PreviousAppContext.attach(snapshot, to: text)
            } else {
                model.notice = AXIsProcessTrusted() ? "No previous app to read from. Sent without context." : "Allow Accessibility in Settings → Permissions to include the previous app. Sent without context."
            }
        }
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
        guard !text.isEmpty, !model.sendingChatScopes.contains(target) else { return }
        model.sendingChatScopes.insert(target)
        Task {
            _ = await model.workbench(.command("SendMessage", ["text": .string(text), "workspace_id": target.workspaceID.map(JSONValue.string) ?? .null]))
            model.sendingChatScopes.remove(target)
        }
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
    let perform: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: perform) {
            HStack(alignment: .top, spacing: 12) {
                Circle().fill(dot).frame(width: 6, height: 6).frame(width: 16, height: 20)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.system(size: 13)).foregroundStyle(N.text).lineLimit(1)
                    Text(meta).font(.system(size: 12)).foregroundStyle(N.text3).lineLimit(1)
                }
                Spacer(minLength: 12)
                Text(action).font(.system(size: 12, weight: .medium)).foregroundStyle(hover ? N.text : N.text2)
                    .padding(.horizontal, 12).frame(height: 26)
                    .liquidGlassCapsule(interactive: true)
                    .scaleEffect(hover ? 1.03 : 1)
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

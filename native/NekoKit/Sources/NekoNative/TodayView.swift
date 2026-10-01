import SwiftUI
import AppKit
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
    mutating func update(_ metrics: ChatScrollMetrics) {
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
    @State private var transcriptHeight: CGFloat = 0
    @State private var ticket: String?
    @State private var addingWorkspace = false
    @State private var composerHeight: CGFloat = 0
    @State private var availableModels: [AgentModel] = []
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
        PanelHeader(title: "Today", crumb: workspaceName) { StatusPill(text: model.connected ? "Watching" : "Reconnecting…", live: model.connected) }
        HStack(spacing: 0) {
            ZStack(alignment: .bottom) {
                GeometryReader { viewport in
                ScrollViewReader { reader in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 32) {
                            if messages.isEmpty {
                                todayHero
                                if model.workspaces.isEmpty { howNekoWorks } else { brief }
                            }
                            ForEach(messages, id: \.recordID) { message in
                                let mine = message["role"].string == "user"
                                HStack(alignment: .top, spacing: 12) {
                                if mine { Spacer(minLength: 80) }
                                VStack(alignment: .leading, spacing: 10) {
                                    HStack(spacing: 8) { Text(mine ? "You" : "Neko").font(NekoFont.heading); if message["queued"].bool { Text("Queued").font(.caption).foregroundStyle(N.text3) }; if message["pending"].bool { Spacer(); Button("Stop") { Task { await model.workbench(.command("CancelChat", ["turn_id": message["id"]])) } }.nekoGlassButton() } }
                                    if message["pending"].bool {
                                        let now = ChatActivity.describe(message, model: model)
                                        ActivityCapsule(activity: now.activity, label: now.label).padding(.bottom, 6)
                                    }
                                    if !mine && message["ticket_ids"].array.isEmpty &&
                                        (message["text"].string.contains("I could not open every ticket") || message["text"].string.contains("No ticket was created")) {
                                        Label("No ticket was created from this reply", systemImage: "exclamationmark.triangle")
                                            .font(.callout).foregroundStyle(NekoStyle.amber)
                                    }
                                    ReadableText(text: message["text"].string).lineSpacing(4)
                                    if message["failed"].bool { Label("This turn did not complete.", systemImage: "exclamationmark.triangle").foregroundStyle(.red) }
                                    let receipts = model.snapshot["mcp"]["receipts"].array.filter { $0["run_id"].string == "chat:\(message.recordID)" }
                                    let activity = ChatToolActivitySummary(calls: message["tool_calls"].array, receipts: receipts)
                                    if activity.count > 0 { toolActivity(activity, turn: message.recordID, pending: message["pending"].bool) }
                                    ForEach(message["remembered"].array, id: \.self) { memory in Label("Remembered: \(memory.string)", systemImage: "text.alignleft").font(.callout).foregroundStyle(.secondary) }
                                    ForEach(message["responsibility_ids"].array, id: \.self) { id in
                                        if let item = model.snapshot["mcp"]["responsibilities"].array.first(where: { $0.recordID == id.string }) {
                                            SuggestedResponsibilityCard(model: model, item: item)
                                        }
                                    }
                                    ForEach(message["ticket_ids"].array, id: \.self) { id in Button(model.snapshot["tasks"].array.first { $0.recordID == id.string }?["title"].string ?? "Open ticket", systemImage: "tray") { ticket = id.string } }
                                }
                                .padding(mine ? 12 : 0)
                                .background { if mine { RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.primary.opacity(0.05)).overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.primary.opacity(0.08))) } }
                                }
                                .id(message.recordID)
                                .transition(.asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity), removal: .opacity))
                            }
                            Color.clear.frame(height: ChatComposerClearance.bottomSpace(for: composerHeight)).id("bottom")
                        }.padding(.horizontal, 32).padding(.top, 48).padding(.bottom, 24).frame(maxWidth: 800, alignment: .leading).frame(maxWidth: .infinity, alignment: .center)
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
                            if scrollFollow.followsLatest { reader.scrollTo("bottom", anchor: .bottom) }
                        }
                        .onChange(of: scope) { _, _ in
                            scrollFollow = ChatScrollFollowState()
                            reader.scrollTo("bottom", anchor: .bottom)
                        }
                        .onAppear { reader.scrollTo("bottom", anchor: .bottom) }
                }
                }
                composer(availableWidth: outer.size.width)
            }
        }
        }
        }.animation(reduceMotion ? nil : .spring(response: 0.4, dampingFraction: 0.85), value: messages.count)
        .task { availableModels = await AgentModelCatalog.load() }
        .sheet(isPresented: Binding(get: { ticket != nil }, set: { if !$0 { ticket = nil } })) {
            if let id = ticket { VStack { HStack { Spacer(); Button("Done") { ticket = nil }.keyboardShortcut(.cancelAction) }.padding(); TicketDetail(model: model, id: id) }.frame(minWidth: 650, minHeight: 600) }
        }.sheet(isPresented: $addingWorkspace) { WorkspaceEditor(model: model) }
    }
    private func composer(availableWidth: CGFloat) -> some View {
        let cardWidth = min(720, max(360, availableWidth - 96))
        return VStack(alignment: .leading, spacing: 8) {
            if let notice = toolAccessNotice {
                HStack(spacing: 10) {
                    Image(systemName: "wrench.and.screwdriver").foregroundStyle(NekoStyle.amber)
                    Text(notice).font(.caption).foregroundStyle(N.text2)
                    Spacer()
                    Button("Set up tools") {
                        if model.selectedWorkspace == nil, model.workspaces.count == 1 { model.selectedWorkspace = model.workspaces[0].recordID }
                        NotificationCenter.default.post(name: .nekoNavigate, object: "Tools & skills")
                    }.controlSize(.small)
                }.padding(.horizontal, 12).padding(.vertical, 8).nekoCard(padding: 0, radius: 10)
            }
            ComposerView(
                text: Binding(get: { draft }, set: { draft = $0 }),
                onSubmit: { send() },
                onInterruptAndSubmit: { send(interrupt: true) },
                onAttach: { model.chatDrafts.add($0, for: scope) },
                onError: { model.error = $0 }
            ).id(scope).frame(width: cardWidth - 36, alignment: .leading)
            if !attachments.isEmpty { attachmentStrip }
            HStack(spacing: 8) {
                Button(action: chooseAttachments) {
                    Image(systemName: "plus").font(.system(size: 13, weight: .medium))
                        .foregroundStyle(N.text2).frame(width: 30, height: 30)
                        .background(N.selected.opacity(0.55), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                }
                .buttonStyle(.plain)
                .help("Attach images or files; you can also paste or drop them")
                .accessibilityLabel("Attach images or files")
                Menu {
                    Menu("OpenAI") {
                        Button("Default model") { selectRuntime("codex") }
                        ForEach(availableModels.filter(\.native)) { entry in
                            Button(entry.model) { selectRuntime("codex", model: entry.model) }
                        }
                    }
                    Button("Ollama · local") { selectRuntime("ollama") }
                    Button("LM Studio · local") { selectRuntime("lmstudio") }
                    if availableModels.contains(where: { !$0.native }) {
                        Divider()
                        ForEach(Array(Set(availableModels.filter { !$0.native }.map(\.provider))).sorted(), id: \.self) { provider in
                            Menu(provider.capitalized) {
                                ForEach(availableModels.filter { !$0.native && $0.provider == provider }) { entry in
                                    Button(entry.model) { selectRuntime("opencodex", model: entry.id) }
                                }
                            }
                        }
                    }
                    Button("Refresh models") {
                        Task { availableModels = await AgentModelCatalog.load() }
                    }
                    Divider()
                    Button("Model settings…") { NotificationCenter.default.post(name: .nekoNavigate, object: "Settings") }
                } label: {
                    Text(runtimeLabel)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(N.text3)
                    .padding(.horizontal, 8).frame(height: 30)
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .tint(N.text3)
                .fixedSize()
                .help("Choose the agent runtime. Local providers need a running Ollama or LM Studio server.")
                Spacer(minLength: 8)
                Button { send() } label: {
                    Group {
                        if sending { ProgressView().controlSize(.mini) }
                        else { Image(systemName: "arrow.up").font(.system(size: 13, weight: .semibold)) }
                    }
                    .foregroundStyle(canSend ? Color.white : N.text3)
                    .frame(width: 30, height: 30)
                    .background(canSend ? NekoStyle.accent : N.selected.opacity(0.65), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                }
                .buttonStyle(.plain)
                .disabled(!canSend)
                .accessibilityLabel(sending ? "Sending message" : replying ? "Queue message" : "Send message")
            }
            .frame(width: cardWidth - 36)
        }
        .padding(.horizontal, 18).padding(.top, 11).padding(.bottom, 10)
        .background(N.panel.opacity(0.45), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .liquidGlass(radius: 16)
        .padding(.bottom, 20)
        .frame(maxWidth: .infinity, alignment: .center)
        .background {
            GeometryReader { geometry in
                Color.clear.preference(key: ChatComposerHeightKey.self, value: geometry.size.height)
            }
        }
        .onPreferenceChange(ChatComposerHeightKey.self) { height in
            if abs(height - composerHeight) > 1 { composerHeight = height }
        }
    }

    private var canSend: Bool { !model.chatDrafts.submission(for: scope).text.isEmpty && !sending && !model.busy }

    private var runtimeLabel: String {
        let runtime = model.snapshot["agent_runtime"]
        let provider = runtime["provider"].string
        let name = provider == "ollama" ? "Ollama" : provider == "lmstudio" ? "LM Studio" : provider == "opencodex" ? "Connected model" : "Codex"
        let selectedModel = runtime["model"].string
        if provider == "opencodex", let slash = selectedModel.firstIndex(of: "/") {
            return "\(selectedModel[..<slash].capitalized) · \(selectedModel[selectedModel.index(after: slash)...])"
        }
        return selectedModel.isEmpty ? name : "\(name) · \(selectedModel)"
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
            Text(waiting == 0 ? (model.workspaces.isEmpty ? "I watch your work while you're away, plan the next step the way you would, and ask before I act." : "Nothing needs you right now. I'm watching.") : "\(waiting) \(waiting == 1 ? "thing needs" : "things need") you. Everything else is being watched.")
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
        let watching = !model.snapshot["mcp"]["responsibilities"].array.isEmpty
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
                HStack { Button("Deny", role: .destructive) { decide(false) }; Button("Allow this request") { decide(true) }.nekoPrimaryButton() }
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

    private func send(interrupt: Bool = false) {
        guard !sending, !model.busy else { return }
        let submission = model.chatDrafts.submission(for: scope)
        guard !submission.text.isEmpty else { return }
        model.sendingChatScopes.insert(submission.scope)
        Task {
            let saved = await model.workbench(.command(interrupt ? "InterruptAndSendMessage" : "SendMessage", ["text": .string(submission.text), "workspace_id": submission.scope.workspaceID.map(JSONValue.string) ?? .null]))
            model.chatDrafts.complete(submission, succeeded: saved)
            model.sendingChatScopes.remove(submission.scope)
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

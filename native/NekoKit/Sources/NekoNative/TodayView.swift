import SwiftUI
import NekoKit

struct ChatDraftScope: Hashable {
    let workspaceID: String?
    let profileID: String
}

struct ScopedChatDrafts {
    struct Submission { let scope: ChatDraftScope; let text: String; let revision: Int }
    private var values: [ChatDraftScope: String] = [:]
    private var revisions: [ChatDraftScope: Int] = [:]
    func text(for scope: ChatDraftScope) -> String { values[scope] ?? "" }
    mutating func set(_ text: String, for scope: ChatDraftScope) {
        guard values[scope] != text else { return }
        values[scope] = text
        revisions[scope, default: 0] += 1
    }
    func submission(for scope: ChatDraftScope) -> Submission {
        Submission(scope: scope, text: text(for: scope), revision: revisions[scope, default: 0])
    }
    mutating func complete(_ submission: Submission, succeeded: Bool) {
        guard succeeded, revisions[submission.scope, default: 0] == submission.revision else { return }
        set("", for: submission.scope)
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

private struct ChatScrollMetricsKey: PreferenceKey {
    static let defaultValue = ChatScrollMetrics()
    static func reduce(value: inout ChatScrollMetrics, nextValue: () -> ChatScrollMetrics) { value = nextValue() }
}

struct TodayView: View {
    @ObservedObject var model: AppModel
    @State private var scrollFollow = ChatScrollFollowState()
    @State private var transcriptHeight: CGFloat = 0
    @State private var ticket: String?
    @State private var addingWorkspace = false
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
    private var messages: [JSONValue] {
        model.snapshot["conversation"].array.filter {
            ($0["workspace_id"] == .null ? nil : $0["workspace_id"].string) == scope.workspaceID &&
            ($0["agent_profile_id"].string == scope.profileID)
        }
    }
    var body: some View {
        VStack(spacing: 0) {
        PanelHeader(title: "Today", crumb: workspaceName) { StatusPill(text: model.connected ? "Watching" : "Reconnecting…", live: model.connected) }
        HStack(spacing: 0) {
            VStack(spacing: 0) {
                GeometryReader { viewport in
                ScrollViewReader { reader in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 32) {
                            if messages.isEmpty {
                                greeting
                                if model.workspaces.isEmpty {
                                    howNekoWorks
                                } else {
                                    brief
                                }
                            }
                            ForEach(messages, id: \.recordID) { message in
                                let mine = message["role"].string == "user"
                                HStack(alignment: .top, spacing: 12) {
                                if mine { Spacer(minLength: 80) } else { Avatar(role: "neko") }
                                VStack(alignment: .leading, spacing: 10) {
                                    HStack(spacing: 8) { Text(mine ? "You" : "Neko").font(NekoFont.heading); if message["pending"].bool { TypingDots(); Spacer(); Button("Stop") { Task { await model.workbench(.command("CancelChat", ["turn_id": message["id"]])) } }.nekoGlassButton() } }
                                    ReadableText(text: message["text"].string).lineSpacing(4)
                                    if message["failed"].bool { Label("This turn did not complete.", systemImage: "exclamationmark.triangle").foregroundStyle(.red) }
                                    ForEach(message["tool_calls"].array, id: \.recordID) { call in toolCall(call, turn: message.recordID, pending: message["pending"].bool) }
                                    ForEach(message["remembered"].array, id: \.self) { memory in Label("Remembered: \(memory.string)", systemImage: "text.alignleft").font(.callout).foregroundStyle(.secondary) }
                                    ForEach(model.snapshot["mcp"]["receipts"].array.filter { $0["run_id"].string == "chat:\(message.recordID)" }, id: \.recordID) { receipt in
                                        DisclosureGroup {
                                            Text("Receipt \(receipt.recordID)").font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                                        } label: { Label("\(receipt["tool_name"].string) · \(receipt["success"].bool ? "Completed" : "Failed")", systemImage: receipt["success"].bool ? "checkmark.circle" : "exclamationmark.circle").font(.caption).foregroundStyle(.secondary) }
                                    }
                                    ForEach(message["ticket_ids"].array, id: \.self) { id in Button(model.snapshot["tasks"].array.first { $0.recordID == id.string }?["title"].string ?? "Open ticket", systemImage: "tray") { ticket = id.string } }
                                }
                                .padding(mine ? 12 : 0)
                                .background { if mine { RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.primary.opacity(0.05)).overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.primary.opacity(0.08))) } }
                                if mine { Avatar(role: "user") }
                                }
                                .id(message.recordID)
                                .transition(.asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity), removal: .opacity))
                            }
                            Color.clear.frame(height: 1).id("bottom")
                        }.padding(.horizontal, 64).padding(.top, 48).padding(.bottom, 24).frame(maxWidth: 820, alignment: .leading).frame(maxWidth: .infinity, alignment: .leading)
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
                VStack(alignment: .leading, spacing: 12) {
                    ComposerView(text: Binding(get: { draft }, set: { draft = $0 }), onSubmit: send, onError: { model.error = $0 }).id(scope).frame(height: 52)
                    HStack(spacing: 8) {
                        WorkspaceMenu(model: model, addingWorkspace: $addingWorkspace).menuStyle(.button).buttonStyle(.borderless).controlSize(.small).fixedSize()
                        ComposerChip(text: "Read only")
                        Spacer()
                        Text("⌘↵").font(.system(size: 11)).foregroundStyle(N.text4)
                        Button { send() } label: {
                            Image(systemName: "arrow.up").font(.system(size: 13, weight: .bold)).foregroundStyle(N.canvas).frame(width: 30, height: 30).background(NekoStyle.accent, in: Circle()).shadow(color: NekoStyle.accent.opacity(0.5), radius: 8)
                        }.buttonStyle(.plain).opacity(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || sending || model.busy ? 0.35 : 1).disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || sending || model.busy).accessibilityLabel("Send message")
                    }
                }
                .padding(.horizontal, 16).padding(.top, 14).padding(.bottom, 12)
                .liquidGlass(radius: 18)
                .shadow(color: .black.opacity(0.3), radius: 20, y: 8)
                .padding(.horizontal, 64).padding(.bottom, 24).frame(maxWidth: 820, alignment: .leading).frame(maxWidth: .infinity, alignment: .leading)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    railHeader("Needs you", ["AwaitingApproval", "ReadyForReview", "Failed"])
                    taskGroup(["AwaitingApproval", "ReadyForReview", "Failed"])
                    railHeader("Working now", ["Queued", "Planning", "Building", "Reviewing"]).padding(.top, 12)
                    taskGroup(["Queued", "Planning", "Building", "Reviewing"])
                }
            }.scrollIndicators(.never).frame(width: 300)
            .overlay(alignment: .leading) { N.line.frame(width: 1) }
        }
        }.animation(reduceMotion ? nil : .spring(response: 0.4, dampingFraction: 0.85), value: messages.count)
        .sheet(isPresented: Binding(get: { ticket != nil }, set: { if !$0 { ticket = nil } })) {
            if let id = ticket { VStack { HStack { Spacer(); Button("Done") { ticket = nil }.keyboardShortcut(.cancelAction) }.padding(); TicketDetail(model: model, id: id) }.frame(minWidth: 650, minHeight: 600) }
        }.sheet(isPresented: $addingWorkspace) { WorkspaceEditor(model: model) }
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
    private func railHeader(_ title: String, _ statuses: [String]) -> some View {
        HStack {
            Text(title).font(.system(size: 12, weight: .medium)).foregroundStyle(N.text3)
            Spacer()
            Text(String(model.tasks.filter { statuses.contains($0["status"].string) }.count)).font(.system(size: 12).monospacedDigit()).foregroundStyle(N.text4)
        }.padding(.horizontal, 20).frame(height: 40)
    }
    @ViewBuilder private func taskGroup(_ statuses: [String]) -> some View {
        let tasks = model.tasks.filter { statuses.contains($0["status"].string) }
        if tasks.isEmpty {
            Text(statuses.contains("AwaitingApproval") ? "Nothing waiting on you." : "Nothing running right now.")
                .font(.system(size: 12)).foregroundStyle(N.text4).padding(.horizontal, 20).frame(height: 36, alignment: .leading)
        }
        ForEach(tasks, id: \.recordID) { task in
            RailRow(title: task["title"].string, meta: friendlyTaskStatus(task["status"].string) + " · " + (model.workspaces.first { $0.recordID == task["workspace_id"].string }?["name"].string ?? "Workspace"), status: task["status"].string) { ticket = task.recordID }
        }
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
        let recent = model.tasks.filter { !["Completed", "Cancelled"].contains($0["status"].string) }.prefix(5)
        if recent.isEmpty { howNekoWorks } else {
        VStack(alignment: .leading, spacing: 0) {
            Text(recent.isEmpty ? "Start with something small" : "While you were away").font(.system(size: 12, weight: .medium)).foregroundStyle(N.text4).padding(.bottom, 8)
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
    }
    private func toolCall(_ call: JSONValue, turn: String, pending: Bool) -> some View {
        func decide(_ approve: Bool) { Task { await model.workbench(.command("DecideChatTool", ["turn_id": .string(turn), "call_id": call["id"], "approve": .bool(approve)])) } }
        return VStack(alignment: .leading, spacing: 8) {
            Label(call["tool_name"].string, systemImage: "wrench.and.screwdriver").font(.callout.bold())
            Text("\(model.workspaces.first { $0.recordID == call["workspace_id"].string }?["name"].string ?? "Unknown workspace") · \(model.snapshot["mcp"]["connections"].array.first { $0.recordID == call["connection_id"].string }?["label"].string ?? "Removed connection")").font(.caption).foregroundStyle(.secondary)
            Text(call["status"].string.replacingOccurrences(of: "_", with: " ")).foregroundStyle(.secondary)
            DisclosureGroup("Request details") { Text(call["arguments_json"].string).font(.system(.caption, design: .monospaced)).textSelection(.enabled) }
            if pending && call["status"].string == "awaiting_approval" {
                HStack { Button("Deny", role: .destructive) { decide(false) }; Button("Allow this request") { decide(true) }.nekoPrimaryButton() }.disabled(model.busy)
            }
        }.padding(12).nekoCard(padding: 0, radius: 12)
    }
    private func send() {
        guard !sending, !model.busy, !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        let submission = model.chatDrafts.submission(for: scope)
        model.sendingChatScopes.insert(submission.scope)
        Task {
            let saved = await model.workbench(.command("SendMessage", ["text": .string(submission.text), "workspace_id": submission.scope.workspaceID.map(JSONValue.string) ?? .null]))
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


struct ComposerChip: View {
    let text: String
    var body: some View {
        Text(text).font(.system(size: 12)).foregroundStyle(N.text3).lineLimit(1)
            .padding(.horizontal, 8).padding(.vertical, 3)
            .liquidGlassCapsule()
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
            Button(action, action: perform).controlSize(.small).glassButton().padding(.top, 2)
        }
        .padding(.vertical, 12)
        .overlay(alignment: .top) { N.line.frame(height: 1) }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Step \(number), \(title)\(done ? ", done" : ""). \(detail)")
    }
}

import SwiftUI
import NekoKit

/// What Neko keeps an eye on. You describe it in plain words (or ask Neko to
/// suggest from your connected tools); Neko writes it up as a responsibility
/// that stays paused until you turn it on.
enum Watching {
    static let suggestPrompt = "Look at the tools connected to this workspace and suggest up to three things you should keep an eye on for me. Only suggest what those tools can actually check."

    /// Responsibilities Neko proposed in chat that the user has not acted on yet.
    @MainActor static func suggestedIDs(_ model: AppModel) -> Set<String> {
        let offered = Set(model.snapshot["conversation"].array.flatMap { $0["responsibility_ids"].array.map(\.string) })
        return Set(model.snapshot["mcp"]["responsibilities"].array.filter {
            offered.contains($0.recordID) && !$0["enabled"].bool && $0["last_attempt_ms"] == .null
        }.map(\.recordID))
    }

    /// A starting idea per connected tool, phrased the way a person would ask.
    static func idea(for label: String) -> String {
        let l = label.lowercased()
        if l.contains("linear") { return "New Linear issues assigned to me, and anything of mine that becomes blocked" }
        if l.contains("sentry") { return "New or spiking Sentry errors in my projects" }
        if l.contains("github") { return "Pull requests waiting on my review, and failing checks on my own PRs" }
        if l.contains("slack") { return "Slack messages that mention me or are waiting on my reply" }
        if l.contains("gmail") || l.contains("mail") { return "Emails that need a reply from me today" }
        if l.contains("calendar") { return "Meetings today that need preparation" }
        if l.contains("notion") { return "Notion pages assigned to me or recently changed in my projects" }
        return "Anything new in \(label) that needs my attention"
    }

    @MainActor static func connections(_ model: AppModel, workspace: String?) -> [JSONValue] {
        model.snapshot["mcp"]["connections"].array.filter { c in
            c["enabled"].bool && (c["workspace_id"].string.isEmpty || workspace == nil || c["workspace_id"].string == workspace)
        }
    }

    @MainActor static func label(_ model: AppModel, connection id: String) -> String {
        model.snapshot["mcp"]["connections"].array.first { $0.recordID == id }?["label"].string ?? "Removed tool"
    }

    /// A tool can only be read once the workspace has granted at least one of its tools.
    @MainActor static func ungranted(_ model: AppModel, _ item: JSONValue) -> [String] {
        let workspace = item["workspace_id"].string
        let granted = Set(model.snapshot["mcp"]["grants"].array.filter { $0["workspace_id"].string == workspace }.map { $0["connection_id"].string })
        return item["connection_ids"].array.map(\.string).filter { !granted.contains($0) }.map { label(model, connection: $0) }
    }

    @MainActor static func turnOn(_ model: AppModel, _ item: JSONValue) {
        Task {
            let saved = await model.workbench(nested("Mcp", "SaveResponsibility", ["responsibility": replacing(item, ["enabled": .bool(true)])]))
            if saved { await model.workbench(nested("Mcp", "Wake", ["responsibility_id": item["id"]])) }
        }
    }

    @MainActor static func remove(_ model: AppModel, _ item: JSONValue) {
        submit(model, nested("Mcp", "RemoveResponsibility", ["responsibility_id": item["id"]]))
    }
}

/// One suggestion Neko made: what it will look at, what it will and won't do.
struct SuggestedResponsibilityCard: View {
    @ObservedObject var model: AppModel
    let item: JSONValue
    var onEdit: (() -> Void)? = nil
    var body: some View {
        let tools = item["connection_ids"].array.map { Watching.label(model, connection: $0.string) }
        let workspace = model.workspaces.first { $0.recordID == item["workspace_id"].string }?["name"].string ?? "Workspace"
        let missing = Watching.ungranted(model, item)
        let live = item["enabled"].bool
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: live ? "eye" : "sparkles").font(.system(size: 12)).foregroundStyle(live ? NekoStyle.mint : N.text3)
                Text(item["instruction"].string).font(.system(size: 13.5, weight: .medium)).foregroundStyle(N.text)
                    .fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
            }
            VStack(alignment: .leading, spacing: 4) {
                promise("checkmark", "Reads \(tools.joined(separator: ", ")) every 10 minutes in \(workspace) and brings what matters to Today.")
                promise("hand.raised", item["prepare_low_risk"].bool ? "May prepare low-risk local fixes; never publishes or messages anyone." : "Never changes, posts or replies to anything. Plans wait for your approval.")
                if !missing.isEmpty {
                    promise("exclamationmark.circle", "Give Neko read access to \(missing.joined(separator: ", ")) in Tools & skills first.", color: NekoStyle.amber)
                }
            }
            HStack(spacing: 8) {
                if live {
                    Label("Watching", systemImage: "checkmark").font(.system(size: 12, weight: .medium)).foregroundStyle(NekoStyle.mint)
                } else {
                    Button("Turn on") { Watching.turnOn(model, item) }.nekoPrimaryButton().controlSize(.small)
                }
                if let onEdit { Button("Edit", action: onEdit).controlSize(.small) }
                if !live { Button("Dismiss") { Watching.remove(model, item) }.controlSize(.small) }
            }.disabled(model.busy)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(N.card.opacity(0.6), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(N.line))
    }
    private func promise(_ symbol: String, _ text: String, color: Color = N.text3) -> some View {
        Label { Text(text).fixedSize(horizontal: false, vertical: true) } icon: { Image(systemName: symbol).frame(width: 14) }
            .font(.system(size: 12)).foregroundStyle(color)
    }
}

struct ResponsibilitiesView: View {
    @ObservedObject var model: AppModel
    @State private var draft: ManagementDraft?
    @State private var ask = ""
    @State private var target: String?
    @State private var askedAt: Int?

    private var workspaceID: String? { model.selectedWorkspace ?? target ?? model.homeWorkspaceID ?? model.workspaces.first?.recordID }
    private var all: [JSONValue] {
        model.snapshot["mcp"]["responsibilities"].array.filter { model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace }
    }
    private var suggestedIDs: Set<String> { Watching.suggestedIDs(model) }
    private var sending: Bool { model.sendingChatScopes.contains { $0.workspaceID == workspaceID } }
    /// Neko's reply to what was asked from this page, while it is fresh.
    private var reply: JSONValue? {
        guard let askedAt else { return nil }
        return model.snapshot["conversation"].array.last {
            $0["role"].string == "neko" && $0["at_ms"].int >= askedAt - 2000 && $0["workspace_id"].string == (workspaceID ?? "")
        }
    }

    var body: some View {
        ManagementScroll {
            PageIntro(title: "What Neko watches", message: "Tell Neko what to keep an eye on, or let it suggest from your connected tools. It checks every 10 minutes, brings anything important to Today, and never changes anything without asking.") { EmptyView() }
            if model.workspaces.isEmpty {
                EmptyRow(text: "Add a workspace first, then tell Neko what to watch in it.")
            } else {
                askBox
                if let reply { replyView(reply) }
                suggestedSection
                watchingSection
            }
        }.sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .responsibility, original: item.value, workspace: item.workspace ?? model.selectedWorkspace, profileID: profileFor(model))
        }
    }

    // MARK: Ask
    private var askBox: some View {
        let tools = Watching.connections(model, workspace: workspaceID)
        return VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: "eye").font(.system(size: 14)).foregroundStyle(N.text3).padding(.top, 3)
                TextField("Tell Neko what to watch… for example “new Linear issues assigned to me”", text: $ask, axis: .vertical)
                    .textFieldStyle(.plain).font(.system(size: 14)).lineLimit(1...4)
                    .onSubmit { send(ask) }
            }
            if !tools.isEmpty { ideas(tools) }
            askFooter(tools)
        }
        .padding(16)
        .background(N.card.opacity(0.6), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(N.lineStrong))
    }

    private func ideas(_ tools: [JSONValue]) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(tools, id: \.recordID) { tool in
                    let idea = Watching.idea(for: tool["label"].string)
                    Button { ask = idea } label: { Label(idea, systemImage: "plus").lineLimit(1) }
                        .buttonStyle(.bordered).controlSize(.small).help("Use this as a starting point")
                }
            }
        }
    }

    private func askFooter(_ tools: [JSONValue]) -> some View {
        HStack(spacing: 8) {
            if model.selectedWorkspace == nil, model.workspaces.count > 1 {
                Picker("Workspace", selection: Binding(get: { workspaceID ?? "" }, set: { target = $0 })) {
                    ForEach(model.workspaces, id: \.recordID) { Text($0["name"].string).tag($0.recordID) }
                }.labelsHidden().fixedSize().controlSize(.small)
            }
            Text(tools.isEmpty ? "No tools connected here yet. Add one in Tools & skills." : "Uses \(tools.map { $0["label"].string }.joined(separator: ", "))")
                .font(.system(size: 12)).foregroundStyle(N.text4).lineLimit(1)
            Spacer()
            if sending { TypingDots() }
            Button("Suggest from my tools", systemImage: "sparkles") { send(Watching.suggestPrompt) }
                .controlSize(.small).disabled(sending || model.busy || tools.isEmpty)
            Button("Ask Neko") { send(ask) }.nekoPrimaryButton().controlSize(.small)
                .disabled(sending || model.busy || ask.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .keyboardShortcut(.return, modifiers: .command)
        }
    }

    private func replyView(_ message: JSONValue) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Avatar(role: "neko")
            VStack(alignment: .leading, spacing: 6) {
                if message["pending"].bool {
                    HStack(spacing: 8) { Text("Looking through your tools").font(.system(size: 13)).foregroundStyle(N.text3); TypingDots() }
                } else {
                    ReadableText(text: message["text"].string)
                }
            }
            Spacer()
            if !message["pending"].bool {
                Button { askedAt = nil } label: { Image(systemName: "xmark") }.buttonStyle(.plain).foregroundStyle(N.text4).accessibilityLabel("Hide reply")
            }
        }
    }

    // MARK: Lists
    @ViewBuilder private var suggestedSection: some View {
        let suggested = all.filter { suggestedIDs.contains($0.recordID) }
        if !suggested.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                sectionTitle("Suggested", detail: "Paused until you turn them on")
                ForEach(suggested, id: \.recordID) { item in
                    SuggestedResponsibilityCard(model: model, item: item) { draft = ManagementDraft(value: item, workspace: item["workspace_id"].string) }
                }
            }
        }
    }

    private var watchingSection: some View {
        let items = all.filter { !suggestedIDs.contains($0.recordID) }
        let on = items.filter { $0["enabled"].bool }.count
        return VStack(alignment: .leading, spacing: 4) {
            HStack {
                sectionTitle("Watching", detail: items.isEmpty ? nil : "\(on) on · \(items.count) total")
                Spacer()
                Button("Write one yourself", systemImage: "plus") { draft = ManagementDraft(value: .object([:]), workspace: workspaceID) }.controlSize(.small)
            }
            if items.isEmpty {
                EmptyRow(text: "Nothing watched yet. Ask above, or tap an idea to start.")
            }
            ForEach(items, id: \.recordID) { item in row(item) }
        }
    }

    private func sectionTitle(_ title: String, detail: String?) -> some View {
        HStack(spacing: 8) {
            Text(title).font(.system(size: 13, weight: .semibold)).foregroundStyle(N.text)
            if let detail { Text(detail).font(.system(size: 12)).foregroundStyle(N.text4) }
        }.accessibilityAddTraits(.isHeader)
    }

    private func status(_ item: JSONValue) -> String {
        let tools = item["connection_ids"].array.map { Watching.label(model, connection: $0.string) }.joined(separator: ", ")
        let workspace = model.selectedWorkspace == nil ? (model.workspaces.first { $0.recordID == item["workspace_id"].string }?["name"].string ?? "") : ""
        let state = item["failures"].int > 0 ? "Needs attention" : item["enabled"].bool ? "Last checked \(relativeTime(item["last_attempt_ms"].int))" : "Paused"
        return [state, tools, workspace].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    private func row(_ item: JSONValue) -> some View {
        let failing = item["failures"].int > 0
        return HStack(alignment: .top, spacing: 12) {
            Toggle("", isOn: Binding(get: { item["enabled"].bool }, set: { enabled in
                submit(model, nested("Mcp", "SaveResponsibility", ["responsibility": replacing(item, ["enabled": .bool(enabled)])]))
            })).toggleStyle(.switch).controlSize(.mini).labelsHidden().accessibilityLabel("Watching on")
            VStack(alignment: .leading, spacing: 3) {
                Text(item["instruction"].string).font(.system(size: 13)).foregroundStyle(item["enabled"].bool ? N.text : N.text3).textSelection(.enabled)
                Text(status(item)).font(.system(size: 12)).foregroundStyle(failing ? NekoStyle.coral : N.text4).lineLimit(1)
                if !item["last_result"].string.isEmpty {
                    Text(item["last_result"].string).font(.system(size: 12)).foregroundStyle(N.text4).lineLimit(2)
                }
            }
            Spacer()
            Menu("More") {
                Button("Check now") { submit(model, nested("Mcp", "Wake", ["responsibility_id": item["id"]])) }.disabled(!item["enabled"].bool)
                Button("Edit") { draft = ManagementDraft(value: item, workspace: item["workspace_id"].string) }
                Button("Delete", role: .destructive) { Watching.remove(model, item) }
            }.controlSize(.small).fixedSize()
        }.padding(.vertical, 8).overlay(alignment: .top) { N.line.frame(height: 1) }
    }

    private func send(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !sending, !model.busy, let workspace = workspaceID else { return }
        let message = text == Watching.suggestPrompt ? text : "Keep an eye on this for me: \(text)"
        askedAt = Int(Date().timeIntervalSince1970 * 1000)
        let scope = ChatDraftScope(workspaceID: workspace, profileID: profileFor(model))
        model.sendingChatScopes.insert(scope)
        Task {
            let sent = await model.workbench(.command("SendMessage", ["text": .string(message), "workspace_id": .string(workspace)]))
            model.sendingChatScopes.remove(scope)
            if sent { ask = "" } else { askedAt = nil }
        }
    }
}

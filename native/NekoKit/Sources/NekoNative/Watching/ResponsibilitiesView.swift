import SwiftUI
import NekoKit

struct ResponsibilitiesView: View {
    @ObservedObject var model: AppModel
    @State private var draft: ManagementDraft?
    @State private var ask = ""
    @State private var target: String?
    @State private var askedAt: Int?
    @State private var showSuggestions = false
    @State private var showPaused = false
    @State private var showReply = false

    private var workspaceID: String? { model.selectedWorkspace ?? target ?? model.homeWorkspaceID ?? model.workspaces.first?.recordID }
    private var all: [JSONValue] {
        model.snapshot["mcp"]["responsibilities"].array.filter { model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace }
    }
    private var suggestedIDs: Set<String> { Watching.suggestedIDs(model) }
    private var active: [JSONValue] { all.filter { $0["enabled"].bool } }
    private var paused: [JSONValue] { all.filter { !$0["enabled"].bool && !suggestedIDs.contains($0.recordID) } }
    private var suggested: [JSONValue] { all.filter { suggestedIDs.contains($0.recordID) } }
    private var sending: Bool { model.sendingChatScopes.contains { $0.workspaceID == workspaceID } }
    private var reply: JSONValue? {
        guard let askedAt else { return nil }
        return model.snapshot["conversation"].array.last {
            $0["role"].string == "neko" && $0["at_ms"].int >= askedAt - 2000 && $0["workspace_id"].string == (workspaceID ?? "")
        }
    }

    var body: some View {
        ManagementScroll {
            PageIntro(title: "Watching", message: "Keep up with the sources that matter. Neko checks every 10 minutes while it’s running and brings relevant work to Home.") {
                Button("New watch", systemImage: "plus") {
                    draft = ManagementDraft(value: .object([:]), workspace: workspaceID)
                }.disabled(model.workspaces.isEmpty || model.busy)
            }
            if let error = model.error {
                DisclosureGroup {
                    Text(error).font(NekoFont.meta).textSelection(.enabled)
                } label: {
                    Label("Couldn’t update Watching", systemImage: "exclamationmark.triangle").font(NekoFont.body)
                }.foregroundStyle(NekoStyle.amber)
            }
            if model.workspaces.isEmpty {
                EmptyRow(text: model.connected ? "Add a workspace to create a watch." : "Waiting for your workspaces…")
            } else {
                watchingSection
                if !paused.isEmpty {
                    DisclosureGroup(isExpanded: $showPaused) { watchRows(paused) } label: {
                        sectionTitle("Paused", detail: "\(paused.count)")
                    }
                }
                if !suggested.isEmpty {
                    DisclosureGroup(isExpanded: $showSuggestions) {
                        VStack(spacing: 0) {
                            ForEach(suggested, id: \.recordID) { item in
                                SuggestedResponsibilityCard(model: model, item: item) {
                                    draft = ManagementDraft(value: item, workspace: item["workspace_id"].string)
                                }
                            }
                        }.padding(.top, 16)
                    } label: {
                        sectionTitle("Suggestions", detail: "\(suggested.count) to review")
                    }
                }
                askBox
                if let reply { replyView(reply) }
            }
        }
        .font(NekoFont.body)
        .controlSize(.regular)
        .sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .responsibility, original: item.value, workspace: item.workspace ?? model.selectedWorkspace, profileID: profileFor(model))
        }
    }

    private var watchingSection: some View {
        VStack(alignment: .leading, spacing: 16) {
            sectionTitle("Active watches", detail: "\(active.count)")
            if active.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Text("No active watches").font(NekoFont.heading).foregroundStyle(N.text2)
                    Text(paused.isEmpty ? "Describe a watch below, or review a suggestion." : "Resume a paused watch or describe a new one below.")
                        .font(NekoFont.body).foregroundStyle(N.text3)
                }.padding(.vertical, NekoLayout.rowInset)
            } else { watchRows(active) }
        }
    }

    private func watchRows(_ items: [JSONValue]) -> some View {
        VStack(spacing: 0) {
            ForEach(items, id: \.recordID) { item in
                WatchRow(model: model, item: item) {
                    draft = ManagementDraft(value: item, workspace: item["workspace_id"].string)
                }
            }
        }
    }

    private func sectionTitle(_ title: String, detail: String) -> some View {
        HStack(spacing: 8) {
            Text(title).font(NekoFont.heading).foregroundStyle(N.text)
            Text(detail).font(NekoFont.meta).foregroundStyle(N.text3).monospacedDigit()
        }.accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
    }

    private var askBox: some View {
        let tools = Watching.connections(model, workspace: workspaceID)
        return VStack(alignment: .leading, spacing: 18) {
            HStack {
                Text("Describe a watch").font(NekoFont.heading).foregroundStyle(N.text)
                Spacer()
                if !tools.isEmpty {
                    Menu("Use an idea", systemImage: "lightbulb") {
                        ForEach(tools, id: \.recordID) { tool in
                            Button(Watching.idea(for: tool["label"].string)) { ask = Watching.idea(for: tool["label"].string) }
                        }
                    }.fixedSize().controlSize(.regular)
                }
            }
            TextField("What should Neko keep an eye on?", text: $ask, axis: .vertical)
                .textFieldStyle(.plain).font(NekoFont.body).lineLimit(3...6).onSubmit { send(ask) }
                .padding(NekoLayout.rowInset)
                .background(N.panel, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(N.line))
                .accessibilityLabel("What should Neko keep an eye on?")
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) { workspacePicker; Spacer(minLength: 8); askActions(tools) }
                VStack(alignment: .leading, spacing: 10) { workspacePicker; askActions(tools) }
            }
            Text(tools.isEmpty ? "Connect a source in Tools & skills to start." : "\(tools.count) connected \(tools.count == 1 ? "source" : "sources") available. Review access before turning on a suggestion.")
                .font(NekoFont.meta).foregroundStyle(N.text3)
        }
        .padding(.top, 8)
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder private var workspacePicker: some View {
        if model.selectedWorkspace == nil, model.workspaces.count > 1 {
            Picker("Workspace", selection: Binding(get: { workspaceID ?? "" }, set: { target = $0 })) {
                ForEach(model.workspaces, id: \.recordID) { Text($0["name"].string).tag($0.recordID) }
            }.labelsHidden().frame(maxWidth: 240).controlSize(.regular)
        } else {
            Text(model.workspaces.first { $0.recordID == workspaceID }?["name"].string ?? "Workspace")
                .font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(1)
        }
    }

    private func askActions(_ tools: [JSONValue]) -> some View {
        HStack(spacing: 8) {
            if sending { ProgressView().controlSize(.small).accessibilityLabel("Asking Neko") }
            Button("Suggest from tools", systemImage: "sparkles") { send(Watching.suggestPrompt) }
                .disabled(sending || model.busy || tools.isEmpty)
            Button("Ask Neko") { send(ask) }.buttonStyle(.borderedProminent)
                .disabled(sending || model.busy || ask.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .keyboardShortcut(.return, modifiers: .command)
        }.controlSize(.regular).fixedSize()
    }

    private func replyView(_ message: JSONValue) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text(message["pending"].bool ? "Looking through your tools…" : "Neko’s reply").font(NekoFont.heading)
                Spacer()
                if !message["pending"].bool {
                    Button("Hide reply", systemImage: "xmark") { askedAt = nil }
                        .labelStyle(.iconOnly).buttonStyle(.plain).foregroundStyle(N.text3)
                }
            }
            if message["pending"].bool {
                ProgressView().controlSize(.small)
            } else {
                if !showReply { Text(message["text"].string).font(NekoFont.body).foregroundStyle(N.text2).lineLimit(3) }
                DisclosureGroup("Full reply", isExpanded: $showReply) {
                    ReadableText(text: message["text"].string).padding(.top, 8)
                }.font(NekoFont.meta)
            }
        }
    }

    private func send(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !sending, !model.busy, let workspace = workspaceID else { return }
        let message = text == Watching.suggestPrompt ? text : "Keep an eye on this for me: \(text)"
        askedAt = Int(Date().timeIntervalSince1970 * 1000)
        showReply = false
        let scope = ChatDraftScope(workspaceID: workspace, profileID: profileFor(model))
        model.sendingChatScopes.insert(scope)
        Task {
            let sent = await model.workbench(.command("SendMessage", ["text": .string(message), "workspace_id": .string(workspace)]))
            model.sendingChatScopes.remove(scope)
            if sent { ask = "" } else { askedAt = nil }
        }
    }
}

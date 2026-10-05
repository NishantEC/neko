import AppKit
import SwiftUI
import NekoKit

struct ChatDraftScope: Hashable {
    let workspaceID: String?
    let profileID: String
}

typealias ScopedChatDrafts = ComposerDraftStore<ChatDraftScope>

/// Navigation-owned draft state; acknowledgements clear only the submitted revision.
struct ComposerDraftStore<Key: Hashable> {
    struct Submission { let scope: Key; let text: String; let revision: Int }
    private var values: [Key: String] = [:]
    private var attached: [Key: [ComposerAttachment]] = [:]
    private var revisions: [Key: Int] = [:]
    subscript(key: Key) -> String? {
        get { values[key] }
        set { set(newValue ?? "", for: key) }
    }
    func text(for key: Key) -> String { values[key] ?? "" }
    func attachments(for key: Key) -> [ComposerAttachment] { attached[key] ?? [] }
    mutating func set(_ text: String, for key: Key) {
        guard values[key] != text else { return }
        values[key] = text
        revisions[key, default: 0] += 1
    }
    mutating func add(_ attachment: ComposerAttachment, for key: Key) {
        guard !attachments(for: key).contains(attachment) else { return }
        attached[key, default: []].append(attachment)
        revisions[key, default: 0] += 1
    }
    mutating func remove(_ attachment: ComposerAttachment, for key: Key) {
        attached[key]?.removeAll { $0 == attachment }
        revisions[key, default: 0] += 1
    }
    func submission(for key: Key) -> Submission {
        let parts = [text(for: key).trimmingCharacters(in: .whitespacesAndNewlines)] + attachments(for: key).map(\.reference)
        return Submission(scope: key, text: parts.filter { !$0.isEmpty }.joined(separator: "\n"), revision: revisions[key, default: 0])
    }
    mutating func complete(_ submission: Submission, succeeded: Bool) {
        guard succeeded, revisions[submission.scope, default: 0] == submission.revision else { return }
        set("", for: submission.scope)
        attached[submission.scope] = []
    }
}

struct ComposerActionState: Equatable {
    enum Primary: Equatable { case send, queue, stop, submitting }
    let primary: Primary
    let canSubmit: Bool
    let buttonEnabled: Bool
    init(running: Bool, hasContent: Bool, submitting: Bool, blocked: Bool, queues: Bool, canStop: Bool? = nil) {
        let stopsCurrentRun = running && (canStop ?? true)
        primary = submitting ? .submitting : stopsCurrentRun && !hasContent ? .stop : hasContent && running && queues ? .queue : .send
        canSubmit = hasContent && !submitting && !blocked
        buttonEnabled = !submitting && !blocked && (hasContent || stopsCurrentRun)
    }
}

enum ComposerPayload {
    static func validationError(_ text: String, limit: Int) -> String? {
        let bytes = text.utf8.count
        return bytes > limit ? "Message is \(bytes) bytes; the limit is \(limit), including attachment references and context. Shorten it before sending." : nil
    }
    static func homeText(_ raw: String, planOnly: Bool) -> String {
        let command = SlashCommand.parse(raw)
        let text = command?.chatText ?? raw
        return planOnly && command == nil
            ? "Plan only, and change nothing yet: propose a ticket with a short step-by-step plan for my approval.\n\n" + text
            : text
    }
}

enum ComposerMention {
    /// Deliberately only supports the existing trailing-word menu, not caret completion.
    static func replacingTrailingMention(in text: String, with replacement: String) -> String {
        var words = text.components(separatedBy: " ")
        if let last = words.last, last.hasPrefix("@") { words[words.count - 1] = replacement.trimmingCharacters(in: .whitespaces) }
        let joined = words.joined(separator: " ").trimmingCharacters(in: .whitespaces)
        return joined.isEmpty ? "" : joined + " "
    }
}

/// Presentation only. Each surface owns dispatch, validation and authority.
struct SharedComposer<Controls: View>: View {
    @Binding var text: String
    let attachments: [ComposerAttachment]
    let placeholder: String
    let accessibilityLabel: String
    let accessibilityHelp: String
    let state: ComposerActionState
    let validationError: String?
    let onSend: () -> Void
    let onStop: () -> Void
    let onAttach: (ComposerAttachment) -> Void
    let onRemove: (ComposerAttachment) -> Void
    let onChooseAttachments: () -> Void
    let onError: (String) -> Void
    var onInterrupt: (() -> Void)? = nil
    var onContext: (() -> Void)? = nil
    var onMenuKey: ((ComposerMenuKey) -> Bool)? = nil
    @ViewBuilder var controls: () -> Controls

    private var actionLabel: String {
        switch state.primary {
        case .send: "Send"
        case .queue: "Queue"
        case .stop: "Stop"
        case .submitting: "Sending"
        }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !attachments.isEmpty { ComposerAttachmentStrip(attachments: attachments, remove: onRemove) }
            ComposerView(
                text: $text,
                onSubmit: { if state.canSubmit { onSend() } },
                onInterruptAndSubmit: { if state.canSubmit { (onInterrupt ?? onSend)() } },
                onAttach: onAttach, onError: onError,
                onSubmitWithContext: onContext.map { action in { if state.canSubmit { action() } } },
                onMenuKey: onMenuKey,
                placeholder: placeholder, accessibilityLabel: accessibilityLabel, accessibilityHelp: accessibilityHelp
            )
            if let validationError {
                Text(validationError).font(.system(size: 11)).foregroundStyle(NekoStyle.amber).fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 8) {
                Button(action: onChooseAttachments) {
                    Image(systemName: "plus").font(.system(size: 13, weight: .medium)).frame(width: 26, height: 26)
                }.buttonStyle(.plain).foregroundStyle(.secondary)
                    .help("Attach images or files; you can also paste or drop them")
                    .accessibilityLabel("Attach images or files")
                controls()
                Spacer(minLength: 4)
                Button {
                    if state.primary == .stop { onStop() } else { onSend() }
                } label: {
                    HStack(spacing: 5) {
                        if state.primary == .submitting { ProgressView().controlSize(.mini) }
                        else { Image(systemName: state.primary == .stop ? "stop.fill" : state.primary == .queue ? "text.badge.plus" : "arrow.up") }
                        Text(actionLabel)
                    }.font(.system(size: 12, weight: .medium)).padding(.horizontal, 10).frame(height: 28)
                        .foregroundStyle(state.buttonEnabled ? Color.white : Color.secondary)
                        .background(state.buttonEnabled ? NekoStyle.accent : Color.primary.opacity(0.06), in: Capsule())
                }.buttonStyle(.plain).disabled(!state.buttonEnabled).accessibilityLabel(actionLabel)
            }
            Text(state.primary == .queue ? "Return to queue · Shift-Return for a new line" : "Return to send · Shift-Return for a new line")
                .font(.system(size: 10)).foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 14).padding(.vertical, 10)
        .liquidGlass(radius: 16)
    }
}

struct ComposerAttachmentStrip: View {
    let attachments: [ComposerAttachment]
    let remove: (ComposerAttachment) -> Void
    var body: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 6) {
                ForEach(attachments) { attachment in
                    HStack(spacing: 6) {
                        Image(systemName: attachment.isImage ? "photo" : "doc")
                        Text(attachment.name).lineLimit(1)
                        Button { remove(attachment) } label: { Image(systemName: "xmark").font(.system(size: 9, weight: .bold)) }
                            .buttonStyle(.plain).accessibilityLabel("Remove \(attachment.name)")
                    }.font(.system(size: 11)).foregroundStyle(.secondary)
                        .padding(.horizontal, 9).frame(height: 26).liquidGlassCapsule()
                }
            }
        }.scrollIndicators(.never)
    }
}

@MainActor enum ComposerAttachmentPicker {
    /// Capture the destination in these callbacks before presenting the panel.
    static func choose(attach: @escaping (ComposerAttachment) -> Void, onError: @escaping (String) -> Void) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        panel.begin { response in
            guard response == .OK else { return }
            Task { @MainActor in
                for url in panel.urls {
                    do { attach(try ComposerAttachmentStore.saveFile(url)) }
                    catch { onError(error.localizedDescription) }
                }
            }
        }
    }
}

enum ComposerRuntimeLabel {
    static func text(provider: String, model: String, catalog: ModelCatalog) -> String {
        let source = catalog.source(provider)
        let providerName = source?.label ?? (provider.isEmpty ? "Codex" : provider)
        let modelID = model.isEmpty ? source?.defaultModel ?? "" : model
        guard !modelID.isEmpty else { return providerName + " · Provider default" }
        let modelName = source?.models.first { $0.id == modelID }?.label ?? modelID
        return providerName + " · " + modelName
    }
}

struct ComposerRuntimeMenu: View {
    @ObservedObject var model: AppModel
    @State private var catalog = ModelCatalog()
    private var provider: String { model.snapshot["agent_runtime"]["provider"].string }
    private var selectedModel: String { model.snapshot["agent_runtime"]["model"].string }
    private var label: String {
        ComposerRuntimeLabel.text(provider: provider, model: selectedModel, catalog: catalog)
    }
    var body: some View {
        Menu {
            Text("Global runtime · applies to future runs")
            ForEach(catalog.sources) { source in
                Section(source.connection) {
                    if source.ready {
                        Button(source.defaultTitle) { select(source.provider) }
                        ForEach(source.models) { entry in
                            Button(title(entry, provider: source.provider)) { select(source.provider, model: entry.id) }
                                .disabled(!entry.usable).help(entry.reason ?? entry.description ?? "")
                        }
                    } else if let note = source.note { Text(note) }
                }
            }
            Button("Refresh models") { Task { catalog = await AgentModelCatalog.load(model, refresh: true) } }
            Divider()
            Button("Model settings…") { NotificationCenter.default.post(name: .nekoNavigate, object: "Settings") }
        } label: {
            HStack(spacing: 4) {
                Text(label).lineLimit(1).truncationMode(.middle)
                Text("Global").font(.system(size: 10)).foregroundStyle(.tertiary)
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 8, weight: .semibold))
            }.font(.system(size: 12)).foregroundStyle(.secondary)
        }.menuStyle(.borderlessButton).menuIndicator(.hidden).frame(maxWidth: 220, alignment: .leading)
            .disabled(model.busy).help("Global runtime for future runs. Active work keeps its current runtime.")
            .task { catalog = await AgentModelCatalog.load(model) }
    }
    private func title(_ entry: CatalogModel, provider source: String) -> String {
        let selected = (provider.isEmpty ? "codex" : provider) == source && selectedModel == entry.id
        return (selected ? "✓ " : "") + entry.label + (entry.recommended ? " · Recommended" : "") + (entry.access == .checked ? " · Checked" : "") + (!entry.usable ? " · Unavailable" : "")
    }
    private func select(_ provider: String, model selected: String = "") {
        Task { await model.workbench(.command("SetAgentRuntime", ["runtime": .object(["provider": .string(provider), "model": .string(selected)])])) }
    }
}

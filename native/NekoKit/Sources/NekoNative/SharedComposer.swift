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
    @Environment(\.ink) private var ink
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
            VStack(alignment: .leading, spacing: 12) {
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
                    Text(validationError).font(NekoFont.meta).foregroundStyle(NekoStyle.amber).fixedSize(horizontal: false, vertical: true)
                }
                HStack(alignment: .bottom, spacing: 12) {
                    Button("Attach files", systemImage: "plus", action: onChooseAttachments)
                        .labelStyle(.iconOnly).font(NekoFont.body)
                        .frame(width: 32, height: 32).contentShape(Rectangle())
                        .buttonStyle(.borderless).foregroundStyle(.secondary)
                        .help("Attach images or files; you can also paste or drop them")
                        .accessibilityLabel("Attach images or files")
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: 10) { controls() }
                        VStack(alignment: .leading, spacing: 6) { controls() }
                    }.frame(maxWidth: .infinity, alignment: .leading)
                    Spacer(minLength: 4)
                    Button {
                        if state.primary == .stop { onStop() } else { onSend() }
                    } label: {
                        HStack(spacing: 5) {
                            if state.primary == .submitting { ProgressView().controlSize(.mini) }
                            else { Image(systemName: state.primary == .stop ? "stop.fill" : state.primary == .queue ? "text.badge.plus" : "arrow.up") }
                            Text(actionLabel)
                        }.font(NekoFont.meta.weight(.medium)).padding(.horizontal, 14).frame(height: 34)
                            .foregroundStyle(state.buttonEnabled ? Color.white : Color.secondary)
                            .background(state.buttonEnabled ? NekoStyle.accent : Color.primary.opacity(0.06), in: Capsule())
                    }.buttonStyle(.plain).disabled(!state.buttonEnabled).accessibilityLabel(actionLabel)
                        .fixedSize(horizontal: true, vertical: false)
                }
            }
            .padding(.horizontal, 20).padding(.vertical, 14)
            .liquidGlass(radius: 24)
            Text(state.primary == .queue ? "Return to queue · Shift-Return for a new line" : "Return to send · Shift-Return for a new line")
                .font(NekoFont.meta).foregroundStyle(.secondary)
                .padding(.horizontal, NekoLayout.rowInset)
        }
    }
}

struct ComposerAttachmentStrip: View {
    @Environment(\.ink) private var ink
    let attachments: [ComposerAttachment]
    let remove: (ComposerAttachment) -> Void
    var body: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 6) {
                ForEach(attachments) { attachment in
                    HStack(spacing: 6) {
                        Image(systemName: attachment.isImage ? "photo" : "doc")
                        Text(attachment.name).lineLimit(1).frame(maxWidth: 180)
                        Button { remove(attachment) } label: { Image(systemName: "xmark").font(.system(size: 10, weight: .medium)).frame(width: 24, height: 24).contentShape(Rectangle()) }
                            .buttonStyle(.plain).accessibilityLabel("Remove \(attachment.name)")
                    }.font(NekoFont.meta).foregroundStyle(.secondary)
                        .padding(.leading, 9).padding(.trailing, 2).frame(height: 28)
                        .background(ink.raised, in: RoundedRectangle(cornerRadius: 8))
                        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(ink.line))
                        .help(attachment.name)
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
                Text("Global").font(NekoFont.meta).foregroundStyle(.secondary)
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 8, weight: .semibold))
            }.font(NekoFont.meta).foregroundStyle(.secondary)
        }.menuStyle(.borderlessButton).menuIndicator(.hidden).frame(maxWidth: 220, alignment: .leading)
            .accessibilityLabel("Global runtime: \(label)")
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

/// Shared message metadata; timestamps are shown only when the record has one.
struct ChatAuthorLine: View {
    let author: String
    var timestamp: Int = 0
    var body: some View {
        HStack(spacing: 8) {
            Text(author).font(NekoFont.heading).foregroundStyle(.primary)
            if timestamp > 0 {
                let date = Date(timeIntervalSince1970: Double(timestamp) / 1000)
                Text(date, format: .dateTime.hour().minute())
                    .font(NekoFont.meta.monospacedDigit()).foregroundStyle(.secondary)
                    .help(date.formatted(date: .abbreviated, time: .shortened))
            }
        }.accessibilityElement(children: .combine)
    }
}

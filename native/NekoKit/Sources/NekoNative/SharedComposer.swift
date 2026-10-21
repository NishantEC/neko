import AppKit
import Observation
import SwiftUI
import NekoKit

struct ChatDraftScope: Hashable {
    let workspaceID: String?
    let profileID: String
}

typealias ScopedChatDrafts = ComposerDraftStore<ChatDraftScope>

/// Navigation-owned draft state; acknowledgements clear only the submitted revision.
@MainActor @Observable final class ComposerDraftStore<Key: Hashable> {
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
    func set(_ text: String, for key: Key) {
        guard values[key] != text else { return }
        values[key] = text
        revisions[key, default: 0] += 1
    }
    func add(_ attachment: ComposerAttachment, for key: Key) {
        guard !attachments(for: key).contains(attachment) else { return }
        attached[key, default: []].append(attachment)
        revisions[key, default: 0] += 1
    }
    func remove(_ attachment: ComposerAttachment, for key: Key) {
        attached[key]?.removeAll { $0 == attachment }
        revisions[key, default: 0] += 1
    }
    func submission(for key: Key) -> Submission {
        let parts = [text(for: key).trimmingCharacters(in: .whitespacesAndNewlines)] + attachments(for: key).map(\.reference)
        return Submission(scope: key, text: parts.filter { !$0.isEmpty }.joined(separator: "\n"), revision: revisions[key, default: 0])
    }
    func complete(_ submission: Submission, succeeded: Bool) {
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
            ? "Plan only, and change nothing yet: propose a task with a short step-by-step plan for my approval.\n\n" + text
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
    /// A decision waiting on you, shown inside the same glass above the editor.
    var tray: AnyView? = nil
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
        VStack(alignment: .leading, spacing: NekoSpace.sm) {
            if let tray { tray }
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
            // One row with no width-dependent fallback: a second layout that
            // appears at some widths would change the dock's height, and the
            // transcript would jump with it. Long labels truncate instead.
            HStack(alignment: .center, spacing: NekoSpace.sm) {
                Button("Attach files", systemImage: "paperclip", action: onChooseAttachments)
                    .labelStyle(.iconOnly).font(NekoFont.body)
                    .frame(width: NekoChat.attachButton, height: NekoChat.attachButton)
                    .contentShape(Rectangle().inset(by: -NekoSpace.hair))
                    .nekoButton(.quiet).foregroundStyle(.secondary)
                    .help("Attach images or files; you can also paste or drop them")
                    .accessibilityLabel("Attach images or files")
                HStack(spacing: NekoSpace.sm) { controls() }
                    .lineLimit(1).truncationMode(.middle)
                    .layoutPriority(-1)
                Spacer(minLength: NekoSpace.xxs)
                ComposerPrimaryButton(label: actionLabel, symbol: state.primary == .stop ? "stop.fill" : state.primary == .queue ? "text.badge.plus" : "arrow.up",
                                      enabled: state.buttonEnabled, submitting: state.primary == .submitting) {
                    if state.primary == .stop { onStop() } else { onSend() }
                }
            }
        }
        .padding(NekoSpace.md)
        .modifier(ComposerSurface())
    }
}

struct ComposerAttachmentStrip: View {
    let attachments: [ComposerAttachment]
    let remove: (ComposerAttachment) -> Void
    var body: some View {
        ScrollView(.horizontal) {
            HStack(spacing: NekoSpace.sm) {
                ForEach(attachments) { attachment in
                    ComposerAttachmentChip(name: attachment.name, isImage: attachment.isImage) { remove(attachment) }
                }
            }
        }.scrollIndicators(.never)
    }
}

/// One native sampling surface behind the editor: the shared glass surface.
/// Accessibility changes replace the background only, preserving the live
/// NSTextView and its selection.
struct ComposerSurface: ViewModifier {
    func body(content: Content) -> some View {
        content.modifier(NekoGlassSurface(shape: RoundedRectangle(cornerRadius: NekoRadius.hero)))
    }
}

/// Plain controls sit inside the shared glass surface; no nested blur or shader.
struct ComposerPrimaryButton: View {
    let label: String
    var symbol = "arrow.up"
    var enabled = true
    var submitting = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Group {
                if submitting { ProgressView().controlSize(.mini) }
                else { Image(systemName: symbol).font(NekoFont.of(.body, weight: .semibold)) }
            }
            .frame(width: NekoChat.sendButton, height: NekoChat.sendButton)
            .foregroundStyle(enabled ? N.onInverse : N.text4)
            .background(enabled ? N.inverse : N.selected, in: Circle())
            .contentShape(Circle())
        }
        .buttonStyle(.plain).disabled(!enabled)
        .accessibilityLabel(label).help(label)
    }
}

struct ComposerAttachmentChip: View {
    let name: String
    let isImage: Bool
    var detail: String? = nil
    let remove: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: isImage ? "photo" : "doc")
                .font(NekoFont.of(.subtitle)).foregroundStyle(N.text3).frame(width: 24)
            VStack(alignment: .leading, spacing: NekoSpace.hair) {
                Text(name).font(NekoFont.meta.weight(.medium)).foregroundStyle(.primary)
                    .lineLimit(1).truncationMode(.middle)
                Text(detail ?? (isImage ? "Image" : "File"))
                    .font(NekoFont.caption).foregroundStyle(N.text3)
            }.frame(maxWidth: 210, alignment: .leading)
            Button(action: remove) {
                Image(systemName: "xmark").font(NekoFont.of(.micro, weight: .medium))
                    .frame(width: 24, height: 24).contentShape(Rectangle())
            }
            .buttonStyle(.plain).foregroundStyle(N.text3).accessibilityLabel("Remove \(name)")
        }
        .padding(10)
        .background(N.attachment, in: RoundedRectangle(cornerRadius: NekoRadius.md))
        .help(name)
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

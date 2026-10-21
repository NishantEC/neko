import AppKit
import SwiftUI

// The chat surfaces' own tokens and shells: Home and ticket chats share one
// column, one turn rhythm, one bubble, one speaker line, one block chrome and
// one dock. Screens compose these; they never repeat the numbers.

/// Chat geometry and type. Kept apart from `NekoLayout` so the chat can change
/// without touching every page.
enum NekoChat {
    /// The column: messages, rich blocks and the composer dock.
    static let chatWidth: CGFloat = 720
    /// Paragraphs, lists, quotes and headings; about 85 characters at 15pt.
    static let proseWidth: CGFloat = 640
    /// Before each of your turns: a new exchange.
    static let turnGap: CGFloat = 32
    /// From your message to the reply, and between items inside one exchange.
    static let replyGap: CGFloat = 12
    /// Between the parts of one reply (paragraph to code block, and so on).
    static let blockGap: CGFloat = 16

    static let bubbleInsetX: CGFloat = 14
    static let bubbleInsetY: CGFloat = 10
    /// Your bubble never takes more than this share of the column.
    static let bubbleFraction: CGFloat = 0.8
    /// A sent message rests this far below the top edge while the reply is written under it.
    static let anchorMargin: CGFloat = 0

    /// Rich blocks: header height and side inset.
    static let blockHeader: CGFloat = 36
    static let blockInset: CGFloat = 14

    /// Speaker line: fixed height and the avatar inside it.
    static let speakerHeight: CGFloat = 20
    static let avatar: CGFloat = 18

    /// One folded line of work (tools, commands), and one command inside it.
    static let workRow: CGFloat = 28
    static let commandRow: CGFloat = 24

    /// The composer dock and its tray share `NekoRadius.hero` (28): concentric
    /// with the 32pt send button inside 12pt of padding.
    static let sendButton: CGFloat = 32
    static let attachButton: CGFloat = 28
    static let menuRow: CGFloat = 32
    /// Rows the / and @ menu shows before it scrolls.
    static let menuMaxRows = 7
    static let menuTitleWidth: CGFloat = 96
    static let jumpButton: CGFloat = 32
    /// A collapsed request on a ticket, and the fade over its cut edge.
    static let clampHeight: CGFloat = 220
    static let clampFade: CGFloat = 32
    /// Inline images: never taller than this.
    static let imageHeight: CGFloat = 320

    /// Code, terminal output and diffs: 12pt mono beside 15pt prose read as fine print.
    static let code = Font.system(size: NekoTextStep.meta.rawValue, design: .monospaced)
    static let codeLineSpacing: CGFloat = 3
    /// The composer's NSTextView and its placeholder.
    static var editor: NSFont { NSFont.systemFont(ofSize: NekoTextStep.body.rawValue) }
}

extension View {
    /// Prose stays readable: capped at `NekoChat.proseWidth`, leading.
    func chatProse() -> some View {
        frame(maxWidth: NekoChat.proseWidth, alignment: .leading)
    }
}

// MARK: - Bubble

/// Your turn: a compact bubble on the right, its time revealed on hover in
/// space that is always reserved, so hovering never moves anything.
struct ChatBubble<Content: View>: View {
    var timestamp: Int = 0
    var label: String = "You"
    /// The message as VoiceOver reads it after the speaker and time.
    var spoken: String = ""
    /// Shown under the bubble with its time, always rather than on hover.
    var note: String? = nil
    var noteSymbol: String? = "clock"
    @ViewBuilder var content: () -> Content
    @State private var hover = false
    @Environment(\.colorSchemeContrast) private var contrast

    private var date: Date? { timestamp > 0 ? Date(timeIntervalSince1970: Double(timestamp) / 1000) : nil }
    var body: some View {
        ChatBubbleLane {
            VStack(alignment: .trailing, spacing: NekoSpace.xxs) {
                content()
                    .font(NekoFont.chat).lineSpacing(NekoLeading.body)
                    .multilineTextAlignment(.leading)
                    .padding(.horizontal, NekoChat.bubbleInsetX).padding(.vertical, NekoChat.bubbleInsetY)
                    .background {
                        let shape = RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous)
                        shape.fill(N.accentWash)
                            .overlay { if contrast == .increased { shape.strokeBorder(N.lineStrong) } }
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel(accessibility)
                HStack(spacing: NekoSpace.xs) {
                    if let note {
                        if let noteSymbol { Label(note, systemImage: noteSymbol).foregroundStyle(N.text3) }
                        else { Text(note).foregroundStyle(N.text3) }
                    }
                    if let date {
                        Text(date, format: .dateTime.hour().minute())
                            .monospacedDigit().foregroundStyle(N.text4)
                            .opacity(hover || note != nil ? 1 : 0)
                            .help(date.formatted(date: .abbreviated, time: .shortened))
                            .accessibilityHidden(true)
                    }
                }
                .font(NekoFont.caption)
                .frame(minHeight: NekoChat.speakerHeight - NekoSpace.xxs)
            }
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .contentShape(Rectangle())
        .onHover { inside in withNekoAnimation(.hover) { hover = inside } }
    }
    private var accessibility: String {
        let time = date.map { ", \($0.formatted(date: .omitted, time: .shortened))" } ?? ""
        return spoken.isEmpty ? label + time : "\(label)\(time): \(spoken)"
    }
}

/// Places one child against the trailing edge, at most `NekoChat.bubbleFraction`
/// of the width offered: text wraps at 80% of the column however wide it is.
struct ChatBubbleLane: Layout {
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let child = subviews.first else { return .zero }
        let width = proposal.width.flatMap { $0.isFinite ? $0 : nil }
        let size = child.sizeThatFits(ProposedViewSize(width: width.map { $0 * NekoChat.bubbleFraction }, height: proposal.height))
        return CGSize(width: width ?? size.width, height: size.height)
    }
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let child = subviews.first else { return }
        let size = child.sizeThatFits(ProposedViewSize(width: bounds.width * NekoChat.bubbleFraction, height: bounds.height))
        child.place(at: CGPoint(x: bounds.maxX - size.width, y: bounds.minY), anchor: .topLeading, proposal: ProposedViewSize(size))
    }
}

// MARK: - Speaker line

/// Who is speaking: the pearl for Neko, a role symbol for a ticket's agents.
/// Shown only when the speaker changes.
struct ChatSpeaker: View {
    let name: String
    /// An SF Symbol for an agent role; nil draws Neko's pearl.
    var symbol: String? = nil
    var timestamp: Int = 0
    var body: some View {
        HStack(spacing: NekoSpace.sm) {
            Group {
                if let symbol {
                    Image(systemName: symbol).font(NekoFont.of(.caption)).foregroundStyle(N.text3)
                        .frame(width: NekoChat.avatar, height: NekoChat.avatar)
                        .background(N.selected, in: Circle())
                } else {
                    DesignLabOrb(size: NekoChat.avatar)
                }
            }.accessibilityHidden(true)
            Text(name).font(NekoFont.of(.meta, weight: .semibold)).foregroundStyle(N.text)
            if timestamp > 0 {
                let date = Date(timeIntervalSince1970: Double(timestamp) / 1000)
                Text(date, format: .dateTime.hour().minute())
                    .font(NekoFont.caption.monospacedDigit()).foregroundStyle(N.text4)
                    .help(date.formatted(date: .abbreviated, time: .shortened))
            }
        }
        .frame(height: NekoChat.speakerHeight)
        .accessibilityElement(children: .combine)
    }
}

/// A system event inside a conversation ("Plan ready."), centred between rules.
struct ChatStatusDivider: View {
    let text: String
    var body: some View {
        HStack(spacing: NekoSpace.sm) {
            Hairline()
            Text(text).font(NekoFont.caption).foregroundStyle(N.text4)
                .lineLimit(2).multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .textSelection(.enabled)
                .layoutPriority(1)
            Hairline()
        }
        .frame(maxWidth: NekoChat.proseWidth)
        .frame(maxWidth: .infinity)
    }
}

// MARK: - Rich block

/// The one chrome every rich block in a reply shares: a 36pt header with a
/// caption label and quiet trailing actions, a hairline, then the content.
struct ChatBlock<Trailing: View, Content: View>: View {
    let label: String
    var mono = false
    /// `N.sunken` for code, output and diffs (a well below the canvas); `N.panel` otherwise.
    var fill: Color = N.panel
    @ViewBuilder var trailing: () -> Trailing
    @ViewBuilder var content: () -> Content
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: NekoSpace.sm) {
                Text(label).font(mono ? NekoChat.code : NekoFont.caption).foregroundStyle(N.text3).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: NekoSpace.sm)
                trailing()
            }
            .padding(.horizontal, NekoChat.blockInset)
            .frame(height: NekoChat.blockHeader)
            Hairline()
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(fill, in: RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous))
        .clipShape(RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous))
    }
}

extension ChatBlock where Trailing == EmptyView {
    init(label: String, mono: Bool = false, fill: Color = N.panel, @ViewBuilder content: @escaping () -> Content) {
        self.init(label: label, mono: mono, fill: fill, trailing: { EmptyView() }, content: content)
    }
}

/// Icon-only copy for a block header: the symbol confirms for 1.5 seconds.
struct ChatCopyButton: View {
    let text: () -> String
    var help = "Copy"
    @State private var copied = false
    var body: some View {
        Button {
            NSPasteboard.general.clearContents()
            copied = NSPasteboard.general.setString(text(), forType: .string)
        } label: {
            Image(systemName: copied ? "checkmark" : "doc.on.doc").font(NekoFont.caption)
                .frame(width: NekoSpace.xxl, height: NekoSpace.xxl).contentShape(Rectangle())
        }
        .nekoButton(.quiet).foregroundStyle(N.text3)
        .help(copied ? "Copied" : help).accessibilityLabel(copied ? "Copied" : help)
        .task(id: copied) {
            guard copied else { return }
            try? await Task.sleep(for: .seconds(1.5))
            copied = false
        }
    }
}

// MARK: - Dock tray

/// One answer button in a tray: its label, and the shortcut it owns alone.
struct ChatTrayButton {
    let label: String
    var shortcut: KeyboardShortcut? = nil
    /// Shown after the label ("Allow ⌘1"); the shortcut itself is not text.
    var hint: String? = nil
    let action: () -> Void
}

/// A decision inside the composer glass: a tool request, a plan to approve,
/// a question from the planner. It sits where your hands are and owns its
/// keyboard shortcuts; nothing in the transcript repeats them.
struct ChatDockTray: View {
    let symbol: String
    var tint: Color = NekoStyle.amber
    let title: Text
    /// What VoiceOver announces when the tray appears.
    let announcement: String
    var detail: String = ""
    /// Long text behind the one-line detail, shown on click.
    var fullDetail: String? = nil
    var secondary: ChatTrayButton? = nil
    var primary: ChatTrayButton? = nil
    var busy = false
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.sm) {
            HStack(alignment: .firstTextBaseline, spacing: NekoSpace.sm) {
                Image(systemName: symbol).font(NekoFont.body).foregroundStyle(tint).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: NekoSpace.hair) {
                    title.font(NekoFont.heading).lineLimit(1).truncationMode(.tail).help(announcement)
                    if !detail.isEmpty {
                        if let fullDetail, fullDetail != detail {
                            Button { withNekoAnimation(.reveal) { expanded.toggle() } } label: {
                                Text(expanded ? fullDetail : detail)
                                    .lineLimit(expanded ? nil : 1).truncationMode(.middle)
                                    .multilineTextAlignment(.leading)
                                    .fixedSize(horizontal: false, vertical: expanded)
                                    .frame(maxHeight: expanded ? NekoChat.clampHeight : nil, alignment: .top)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .help(expanded ? "Show less" : "Show the full text")
                        } else {
                            Text(detail).lineLimit(1).truncationMode(.middle).help(detail)
                        }
                    }
                }
                .font(NekoFont.meta).foregroundStyle(N.text3)
                Spacer(minLength: NekoSpace.sm)
                if let secondary { button(secondary).nekoButton(.quiet) }
                if let primary { button(primary).nekoButton(.primary) }
            }
            .controlSize(.small)
            .disabled(busy)
            Hairline().padding(.horizontal, -NekoSpace.md)
        }
        .accessibilityElement(children: .contain)
        .accessibilitySortPriority(1)
        .onAppear {
            NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested, userInfo: [
                .announcement: announcement,
                .priority: NSAccessibilityPriorityLevel.high.rawValue
            ])
        }
    }

    private func button(_ spec: ChatTrayButton) -> some View {
        Button(action: spec.action) {
            HStack(spacing: NekoSpace.xxs) {
                Text(spec.label)
                if let hint = spec.hint { Text(hint).monospacedDigit().opacity(0.7) }
            }
        }
        .keyboardShortcut(spec.shortcut)
    }
}

// MARK: - Jump to latest

/// Back to the newest turn: a glass dot, or a capsule when a reply arrived
/// while you were reading further up.
struct ChatJumpButton: View {
    var newReply = false
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Group {
                if newReply {
                    Label("New reply", systemImage: "arrow.down")
                        .font(NekoFont.of(.meta, weight: .semibold))
                        .padding(.horizontal, NekoSpace.md)
                        .frame(height: NekoChat.jumpButton)
                } else {
                    Image(systemName: "arrow.down").font(NekoFont.of(.callout, weight: .semibold))
                        .frame(width: NekoChat.jumpButton, height: NekoChat.jumpButton)
                }
            }
            .foregroundStyle(N.text)
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .liquidGlassCapsule(interactive: true)
        .help("Jump to latest")
        .accessibilityLabel(newReply ? "New reply. Jump to latest" : "Jump to latest")
    }
}

// MARK: - Folded work

/// The disclosure mark every folded line of work shares.
struct ChatWorkChevron: View {
    let open: Bool
    var body: some View {
        Image(systemName: "chevron.right").font(NekoFont.of(.micro, weight: .semibold))
            .foregroundStyle(N.text3)
            .rotationEffect(.degrees(open ? 90 : 0))
            .frame(width: NekoSpace.md)
            .accessibilityHidden(true)
    }
}

/// One quiet line standing for a stretch of work ("Used 3 tools · Linear"),
/// opening in place on click. No disclosure triangle, so the label is not
/// indented by AppKit.
struct ChatWorkFold<Content: View>: View {
    let label: String
    var failed = false
    /// Quiet text at the trailing edge ("Worked for 1m 12s").
    var trailing: String? = nil
    @Binding var open: Bool
    @ViewBuilder var content: () -> Content
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.xxs) {
            Button { withNekoAnimation(.reveal) { open.toggle() } } label: {
                HStack(spacing: NekoSpace.xs) {
                    ChatWorkChevron(open: open)
                    if failed {
                        Image(systemName: "exclamationmark.circle").foregroundStyle(NekoStyle.amber).accessibilityHidden(true)
                    }
                    Text(label).monospacedDigit().lineLimit(1)
                    Spacer(minLength: 0)
                    if let trailing { Text(trailing).monospacedDigit().foregroundStyle(N.text4).lineLimit(1) }
                }
                .font(NekoFont.meta).foregroundStyle(N.text3)
                .frame(minHeight: NekoChat.workRow)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(trailing.map { "\(label), \($0)" } ?? label)
            .accessibilityValue(open ? "Expanded" : "Collapsed")
            .accessibilityHint(open ? "Hide the details" : "Show the details")
            if open { content() }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

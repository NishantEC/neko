import SwiftUI

// Dense lists, defined once: a one-line row, a group header, a status label,
// a key cap and a tag. Every management list (All tasks, the agent tree,
// pull requests, tools, watches) composes these instead of its own padding.
// Rules: docs/design/principles.md §1–2 and §5.

// MARK: - Geometry

extension NekoLayout {
    /// A one-line list row. Always a minimum, never a fixed height, so larger
    /// text and longer translations grow the row instead of clipping it.
    static let listRowHeight: CGFloat = 36
    /// A row with a second line under its title.
    static let listRowTallHeight: CGFloat = 52
    /// The leading glyph column.
    static let listGlyphSlot: CGFloat = 20
    /// The trailing time column, wide enough for "12 min." in tabular digits.
    static let listMetaSlot: CGFloat = 52
    static let listHeaderHeight: CGFloat = 32
    /// Board columns share the width between these limits.
    static let boardColumnMin: CGFloat = 248
    static let boardColumnMax: CGFloat = 340
    /// A collapsed board column (Done).
    static let boardRail: CGFloat = 44
    /// Settings' section list.
    static let settingsSidebarWidth: CGFloat = 220
    /// The ⌘K quick panel.
    static let paletteWidth: CGFloat = 760
    static let paletteHeight: CGFloat = 500
    static let paletteRowHeight: CGFloat = 40
    /// The quick panel's footer bar.
    static let paletteBarHeight: CGFloat = 40
    /// Onboarding's centred introduction.
    static let introWidth: CGFloat = 480
    /// A long-form editor (the rulebook).
    static let editorMinHeight: CGFloat = 420
    /// The smallest pointer target WCAG 2.5.8 allows.
    static let minimumTarget: CGFloat = 24
}

// MARK: - Tone

/// What a status means, and so which colour it wears. Colour never appears
/// alone: every tone is shown with a symbol and a word (`NekoStatusLabel`).
enum NekoTone: Equatable {
    case attention, review, running, done, failed, neutral

    var color: Color {
        switch self {
        case .attention: NekoStyle.amber
        case .review: NekoStyle.sky
        case .running: NekoStyle.accent
        case .done: NekoStyle.mint
        case .failed: NekoStyle.coral
        case .neutral: N.text3
        }
    }

    init(stage: TaskStage) {
        switch stage {
        case .needsYou: self = .attention
        case .review: self = .review
        case .running: self = .running
        case .upNext: self = .neutral
        case .done: self = .done
        }
    }
}

// MARK: - Rows

/// A Linear-dense list row: a fixed glyph slot, the title with its detail
/// inline after it, a trailing slot and a tabular time column. An optional
/// note sits on a second line under the title, for rows that need you.
struct NekoListRow<Glyph: View, Trailing: View>: View {
    let title: String
    var detail: String? = nil
    var note: String? = nil
    /// The trailing time ("3 min. ago"), in tabular digits.
    var meta: String? = nil
    /// Finished items step back by colour, never by opacity, so a status
    /// label beside them keeps its contrast.
    var dimmed = false
    var titleLines = 1
    @ViewBuilder var glyph: () -> Glyph
    @ViewBuilder var trailing: () -> Trailing
    @State private var hover = false

    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.hair) {
            HStack(spacing: NekoSpace.md) {
                glyph()
                    .foregroundStyle(dimmed ? N.text3 : N.text2)
                    .frame(width: NekoLayout.listGlyphSlot, alignment: .center)
                HStack(alignment: .firstTextBaseline, spacing: NekoSpace.sm) {
                    Text(verbatim: title).font(NekoFont.of(.callout, weight: .medium))
                        .foregroundStyle(dimmed ? N.text3 : N.text)
                        .lineLimit(titleLines).multilineTextAlignment(.leading)
                        .help(title)
                    if let detail, !detail.isEmpty {
                        Text(verbatim: detail).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(1)
                            .layoutPriority(-1)
                    }
                }
                Spacer(minLength: NekoSpace.sm)
                trailing()
                if let meta {
                    Text(meta).font(NekoFont.caption.monospacedDigit()).foregroundStyle(N.text3).lineLimit(1)
                        .frame(minWidth: NekoLayout.listMetaSlot, alignment: .trailing)
                }
            }
            if let note, !note.isEmpty {
                Text(verbatim: note).font(NekoFont.meta).foregroundStyle(N.text2).lineLimit(1)
                    .help(note)
                    .padding(.leading, NekoLayout.listGlyphSlot + NekoSpace.md)
            }
        }
        .padding(.horizontal, NekoSpace.md)
        .frame(maxWidth: .infinity, minHeight: note?.isEmpty == false ? NekoLayout.listRowTallHeight : NekoLayout.listRowHeight,
               alignment: .leading)
        .background(hover ? N.selected : .clear, in: RoundedRectangle(cornerRadius: NekoRadius.sm, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: NekoRadius.sm, style: .continuous))
        .onHover { hover = $0 }
        .nekoAnimation(.hover, value: hover)
        .accessibilityElement(children: .combine)
    }
}

extension NekoListRow where Trailing == EmptyView {
    init(title: String, detail: String? = nil, note: String? = nil, meta: String? = nil, dimmed: Bool = false, titleLines: Int = 1,
         @ViewBuilder glyph: @escaping () -> Glyph) {
        self.init(title: title, detail: detail, note: note, meta: meta, dimmed: dimmed, titleLines: titleLines, glyph: glyph, trailing: { EmptyView() })
    }
}

/// A group header in a dense list: a tone dot, the title and its count, and
/// an optional trailing view. `spaced` adds the gap that separates one group
/// from the one above it (at least twice the rhythm inside a group).
struct NekoListHeader<Trailing: View>: View {
    let title: String
    var tone: NekoTone = .neutral
    var count: Int? = nil
    var spaced = false
    @ViewBuilder var trailing: () -> Trailing

    var body: some View {
        HStack(spacing: NekoSpace.sm) {
            Circle().fill(tone.color).frame(width: NekoSpace.xs, height: NekoSpace.xs).accessibilityHidden(true)
            Text(title).font(NekoFont.of(.meta, weight: .semibold)).foregroundStyle(N.text)
            if let count { Text(String(count)).font(NekoFont.caption.monospacedDigit()).foregroundStyle(N.text3) }
            Spacer(minLength: NekoSpace.sm)
            trailing().font(NekoFont.meta).foregroundStyle(N.text3)
        }
        .frame(minHeight: NekoLayout.listHeaderHeight)
        .padding(.top, spaced ? NekoSpace.xxl : 0)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

extension NekoListHeader where Trailing == EmptyView {
    init(title: String, tone: NekoTone = .neutral, count: Int? = nil, spaced: Bool = false) {
        self.init(title: title, tone: tone, count: count, spaced: spaced, trailing: { EmptyView() })
    }
}

// MARK: - Labels

/// A status in its tone's colour, always with a symbol and a word.
struct NekoStatusLabel: View {
    let tone: NekoTone
    let symbol: String
    let text: String
    /// Live work shows a small spinner in place of the symbol.
    var live = false
    var body: some View {
        HStack(spacing: NekoSpace.xxs) {
            if live {
                ProgressView().controlSize(.mini).accessibilityHidden(true)
            } else {
                Image(systemName: symbol).font(NekoFont.of(.micro, weight: .bold)).accessibilityHidden(true)
            }
            Text(text).font(NekoFont.label).lineLimit(1)
        }
        .foregroundStyle(tone.color)
        .padding(.horizontal, NekoSpace.xs).padding(.vertical, NekoSpace.hair)
        .background(tone.color.opacity(0.12), in: RoundedRectangle(cornerRadius: NekoRadius.sm, style: .continuous))
        .fixedSize()
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text)
    }
}

/// A neutral tag: a workspace, a tool, a mode name.
struct NekoTag: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        Text(verbatim: text).font(NekoFont.label).foregroundStyle(N.text2).lineLimit(1)
            .padding(.horizontal, NekoSpace.xs).padding(.vertical, NekoSpace.hair)
            .nekoFill(N.attachment, radius: NekoRadius.xs)
            .fixedSize()
    }
}

/// One key of a shortcut. A shortcut is one cap per key (⌘ K), as Raycast shows it.
struct NekoKeycap: View {
    let key: String
    init(_ key: String) { self.key = key }
    var body: some View {
        Text(key).font(NekoFont.label).foregroundStyle(N.text2)
            .padding(.horizontal, NekoSpace.xxs).padding(.vertical, NekoSpace.hair)
            .frame(minWidth: NekoSpace.lg + NekoSpace.hair)
            .nekoFill(N.selected, radius: NekoRadius.xs, stroke: N.line)
            .accessibilityLabel(Self.spokenName(key))
    }
    /// What VoiceOver says for a key symbol.
    static func spokenName(_ key: String) -> String {
        switch key {
        case "⌘": "Command"
        case "⌥": "Option"
        case "⌃": "Control"
        case "⇧": "Shift"
        case "↵", "⏎": "Return"
        case "⎋": "Escape"
        case "⌫": "Delete"
        case "↑": "Up arrow"
        case "↓": "Down arrow"
        default: key
        }
    }
}

// MARK: - Steps

/// How far through a short flow you are: one capsule per step.
struct NekoStepIndicator: View {
    let current: Int
    let total: Int
    var body: some View {
        HStack(spacing: NekoSpace.xs) {
            ForEach(1...max(total, 1), id: \.self) { step in
                Capsule().fill(step <= current ? NekoStyle.accent : N.lineStrong)
                    .frame(width: NekoSpace.xxl, height: NekoSpace.xxs)
            }
        }
        .nekoAnimation(.select, value: current)
        .accessibilityElement()
        .accessibilityLabel("Step \(current) of \(total)")
    }
}

// MARK: - Confirming destructive actions

extension View {
    /// Asks before something that can't be taken back: a title naming what
    /// goes, a message saying what stays, and a destructive button named for
    /// the action ("Delete watch"), beside Cancel.
    func nekoConfirmDestructive(isPresented: Binding<Bool>, title: String, message: String, action: String,
                                perform: @escaping () -> Void) -> some View {
        confirmationDialog(title, isPresented: isPresented, titleVisibility: .visible) {
            Button(action, role: .destructive, action: perform)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(message)
        }
    }

    /// The same, for one item of a list: set `item` to ask about it.
    func nekoConfirmDestructive<Item>(item: Binding<Item?>, title: @escaping (Item) -> String, message: String, action: String,
                                      perform: @escaping (Item) -> Void) -> some View {
        let shown = Binding(get: { item.wrappedValue != nil }, set: { if !$0 { item.wrappedValue = nil } })
        return confirmationDialog(item.wrappedValue.map(title) ?? "", isPresented: shown, titleVisibility: .visible, presenting: item.wrappedValue) { value in
            Button(action, role: .destructive) { perform(value); item.wrappedValue = nil }
            Button("Cancel", role: .cancel) { item.wrappedValue = nil }
        } message: { _ in
            Text(message)
        }
    }
}

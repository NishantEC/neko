import SwiftUI

// Screens compose these shells and supply only content and actions. A screen
// never sets its own outer padding, width, background, title font or header
// layout; change the shell instead, so every screen moves together.

// MARK: - Page

/// A management page: title and message, toolbar actions, and a bounded
/// scrolling column of sections.
struct NekoPage<Actions: View, Content: View>: View {
    let title: String
    var message: String? = nil
    var width: NekoPageWidth = .management
    @ViewBuilder var actions: () -> Actions
    @ViewBuilder var content: () -> Content
    var body: some View {
        // Eager stacks rather than a List: a List row can merge several
        // controls into one accessibility element on macOS.
        ScrollView {
            VStack(alignment: .leading, spacing: NekoLayout.sectionGap) {
                NekoPageIntro(title: title, message: message)
                content()
            }
            .frame(maxWidth: width.points, alignment: .leading)
            .padding(NekoLayout.pageInset)
            .frame(maxWidth: .infinity, alignment: .top)
        }
        .font(NekoFont.body)
        .foregroundStyle(N.text)
        .nekoButton(.secondary)
        .controlSize(.regular)
        .accessibilityElement(children: .contain)
        .navigationTitle(title)
        .toolbar { ToolbarItemGroup(placement: .primaryAction) { actions() } }
    }
}

extension NekoPage where Actions == EmptyView {
    init(title: String, message: String? = nil, width: NekoPageWidth = .management, @ViewBuilder content: @escaping () -> Content) {
        self.init(title: title, message: message, width: width, actions: { EmptyView() }, content: content)
    }
}

/// The title block at the top of a page or a page-like pane.
struct NekoPageIntro: View {
    let title: String
    var message: String? = nil
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.sm + NekoSpace.hair) {
            Text(title).font(NekoFont.display).foregroundStyle(N.text).accessibilityAddTraits(.isHeader)
            if let message {
                Text(message).font(NekoFont.body).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: NekoLayout.readingWidth, alignment: .leading)
    }
}

// MARK: - Section

/// A titled group inside a page: an optional status dot and detail, trailing
/// actions that wrap under the title when space is tight, and its content.
/// `grouped` puts the content on an opaque card.
struct NekoSection<Trailing: View, Content: View>: View {
    let title: String
    var marker: Color? = nil
    var detail: String? = nil
    var grouped = false
    @ViewBuilder var trailing: () -> Trailing
    @ViewBuilder var content: () -> Content
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.lg) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: NekoSpace.md) {
                    heading.fixedSize(horizontal: true, vertical: false)
                    Spacer(minLength: NekoSpace.md)
                    trailing().fixedSize()
                }
                VStack(alignment: .leading, spacing: NekoSpace.sm + NekoSpace.hair) {
                    heading
                    trailing()
                }
            }
            .controlSize(.regular)
            VStack(alignment: .leading, spacing: NekoSpace.lg, content: content)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(grouped ? NekoLayout.rowInset : 0)
                .background { if grouped { RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous).fill(N.card) } }
        }
        .accessibilityElement(children: .contain)
    }
    private var heading: some View {
        HStack(alignment: .firstTextBaseline, spacing: NekoSpace.sm) {
            if let marker { Circle().fill(marker).frame(width: 6, height: 6).accessibilityHidden(true) }
            Text(title).font(NekoFont.heading).foregroundStyle(N.text)
            if let detail { Text(detail).font(NekoFont.meta).foregroundStyle(N.text3) }
        }
        .accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
    }
}

extension NekoSection where Trailing == EmptyView {
    init(title: String, marker: Color? = nil, detail: String? = nil, grouped: Bool = false, @ViewBuilder content: @escaping () -> Content) {
        self.init(title: title, marker: marker, detail: detail, grouped: grouped, trailing: { EmptyView() }, content: content)
    }
}

// MARK: - Rows and items

/// Items separated by dividers, each with the same vertical breathing room.
struct NekoDividedList<Data: RandomAccessCollection, ID: Hashable, Item: View>: View {
    let data: Data
    let id: KeyPath<Data.Element, ID>
    @ViewBuilder var item: (Data.Element) -> Item
    init(_ data: Data, id: KeyPath<Data.Element, ID>, @ViewBuilder item: @escaping (Data.Element) -> Item) {
        self.data = data; self.id = id; self.item = item
    }
    var body: some View {
        let last = data.last.map { $0[keyPath: id] }
        ForEach(data, id: id) { element in
            item(element).controlSize(.regular).padding(.vertical, NekoSpace.md)
            if element[keyPath: id] != last { Divider() }
        }
    }
}

/// The line under an item: what it is on the left, what you can do on the right.
struct NekoItemFooter<Actions: View>: View {
    let meta: String
    @ViewBuilder var actions: () -> Actions
    var body: some View {
        HStack(spacing: NekoSpace.sm) {
            Text(meta).font(NekoFont.meta).foregroundStyle(N.text3)
            Spacer(minLength: NekoSpace.sm)
            actions()
        }
    }
}

/// A label, optional explanation and optional symbol, with a control on the right.
struct NekoRow<Control: View>: View {
    var symbol: String? = nil
    let title: String
    var detail: String? = nil
    @ViewBuilder var control: () -> Control
    var body: some View {
        HStack(spacing: NekoSpace.lg) {
            if let symbol { NekoIconTile(symbol: symbol) }
            VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                Text(title).font(symbol == nil ? NekoFont.body : NekoFont.heading).foregroundStyle(N.text)
                if let detail {
                    Text(detail).font(NekoFont.meta).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .combine)
            control()
        }
    }
}

/// An on/off setting with its explanation.
struct NekoToggleRow: View {
    let title: String
    let help: String
    @Binding var isOn: Bool
    var disabled = false
    var body: some View {
        Toggle(isOn: $isOn) {
            VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                Text(title).font(NekoFont.body).foregroundStyle(N.text)
                Text(help).font(NekoFont.meta).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
            }.frame(maxWidth: .infinity, alignment: .leading)
        }
        .toggleStyle(.switch).controlSize(.regular).disabled(disabled)
        .accessibilityLabel(title).accessibilityHint(help)
    }
}

// MARK: - Sheet

/// Every modal: a header, the body, an error line when there is one, and a
/// footer with Cancel on the left and the sheet's own actions on the right.
struct NekoSheet<Content: View, Actions: View>: View {
    let title: String
    var message: String? = nil
    var width: NekoSheetWidth = .standard
    var error: String? = nil
    var cancelTitle = "Cancel"
    var onCancel: (() -> Void)? = nil
    @ViewBuilder var content: () -> Content
    @ViewBuilder var actions: () -> Actions
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.xl) {
            VStack(alignment: .leading, spacing: NekoSpace.sm) {
                Text(title).font(NekoFont.title).foregroundStyle(N.text).accessibilityAddTraits(.isHeader)
                if let message {
                    Text(message).font(NekoFont.body).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
                }
            }
            content()
            if let error, !error.isEmpty {
                Text(error).font(NekoFont.meta).foregroundStyle(NekoStyle.coral).textSelection(.enabled)
            }
            HStack {
                Button(cancelTitle) { if let onCancel { onCancel() } else { dismiss() } }.keyboardShortcut(.cancelAction)
                Spacer()
                actions()
            }
        }
        .font(NekoFont.body).controlSize(.regular)
        .padding(NekoLayout.pageInset).frame(width: width.points).background(N.canvas)
    }
}

/// The sheet's main action: prominent, and bound to Return.
struct NekoPrimaryAction: View {
    let title: String
    var disabled = false
    let action: () -> Void
    var body: some View {
        Button(title, action: action).nekoButton(.primary).keyboardShortcut(.defaultAction).disabled(disabled)
    }
}

// MARK: - Empty states

/// Why a list is empty. Each reason says how to move on.
enum NekoEmptyReason: Equatable {
    /// Nothing exists yet. The message says how to make one.
    case nothingYet
    /// Something exists, but not for this search or filter.
    case noMatches(String)
    /// It could not be loaded. Pair with a retry action.
    case unavailable
}

/// An empty list's explanation, as a quiet line inside a section (`inline`)
/// or a centred block that fills a page or pane.
struct NekoEmptyState<Action: View>: View {
    var reason: NekoEmptyReason = .nothingYet
    var title: String? = nil
    var message: String? = nil
    var symbol: String? = nil
    var block = false
    @ViewBuilder var action: () -> Action
    var body: some View {
        if block {
            VStack(spacing: NekoSpace.md) {
                Image(systemName: resolvedSymbol).font(NekoFont.of(.headline, weight: .light))
                    .foregroundStyle(N.text3).padding(.bottom, NekoSpace.xxs).accessibilityHidden(true)
                if let title { Text(title).font(NekoFont.title) }
                Text(resolvedMessage).font(NekoFont.body).foregroundStyle(N.text3).multilineTextAlignment(.center).lineSpacing(3).frame(maxWidth: 360)
                action().padding(.top, NekoSpace.xxs)
            }.padding(NekoSpace.page).frame(maxWidth: .infinity)
        } else {
            HStack(spacing: NekoSpace.sm) {
                Text(resolvedMessage).font(NekoFont.body).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                action()
            }.frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, NekoSpace.xxs)
        }
    }
    private var resolvedMessage: String {
        if let message { return message }
        switch reason {
        case .nothingYet: return "Nothing here yet."
        case .noMatches(let query): return query.isEmpty ? "Nothing matches these filters." : "Nothing matches “\(query)”."
        case .unavailable: return "This couldn’t be loaded."
        }
    }
    private var resolvedSymbol: String {
        if let symbol { return symbol }
        switch reason {
        case .nothingYet: return "tray"
        case .noMatches: return "magnifyingglass"
        case .unavailable: return "exclamationmark.triangle"
        }
    }
}

extension NekoEmptyState where Action == EmptyView {
    init(_ message: String? = nil, reason: NekoEmptyReason = .nothingYet, title: String? = nil, symbol: String? = nil, block: Bool = false) {
        self.init(reason: reason, title: title, message: message, symbol: symbol, block: block) { EmptyView() }
    }
}

// MARK: - Notices

/// The notice and error strip that floats above page content.
struct NekoNotice<Actions: View>: View {
    enum Kind { case success, warning }
    let kind: Kind
    let text: String
    var dismiss: () -> Void
    @ViewBuilder var actions: () -> Actions
    var body: some View {
        HStack(spacing: NekoSpace.sm + NekoSpace.hair) {
            Image(systemName: kind == .success ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                .foregroundStyle(kind == .success ? NekoStyle.mint : NekoStyle.amber).accessibilityHidden(true)
            Text(text).font(NekoFont.body).textSelection(.enabled).lineLimit(2)
            Spacer()
            actions()
            Button(action: dismiss) { Image(systemName: "xmark").font(NekoFont.of(.small, weight: .bold)) }
                .buttonStyle(.plain).accessibilityLabel("Dismiss")
        }
        .padding(.horizontal, NekoLayout.rowInset).padding(.vertical, NekoSpace.md + NekoSpace.hair)
        .liquidGlass(radius: NekoRadius.xl)
        .frame(maxWidth: NekoLayout.readingWidth)
        .padding(.horizontal, NekoSpace.xxl).padding(.top, NekoSpace.sm)
        .frame(maxWidth: .infinity)
    }
}

extension NekoNotice where Actions == EmptyView {
    init(kind: Kind, text: String, dismiss: @escaping () -> Void) {
        self.init(kind: kind, text: text, dismiss: dismiss) { EmptyView() }
    }
}

// MARK: - Saved text

/// Saved instructions and evidence are inert text, never executable chat blocks.
/// Verbatim rendering also preserves file references whose targets no longer exist.
struct NekoSavedText: View {
    let text: String
    var body: some View {
        Text(verbatim: text)
            .font(NekoFont.body).foregroundStyle(N.text2).textSelection(.enabled).lineLimit(nil)
            .lineSpacing(3)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A brief readback with the complete saved content available on demand.
struct NekoReadMore: View {
    let text: String
    var disclosure = "Read more"
    @State private var expanded = false
    private var needsDisclosure: Bool { text.count > 240 || text.components(separatedBy: .newlines).count > 3 }
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.sm) {
            if needsDisclosure {
                if !expanded {
                    Text(verbatim: text.count > 240
                        ? String(text.prefix(240)).trimmingCharacters(in: .whitespacesAndNewlines) + "…"
                        : text)
                        .lineLimit(3).lineSpacing(3).foregroundStyle(N.text2)
                }
                DisclosureGroup(disclosure, isExpanded: $expanded) {
                    NekoSavedText(text: text).padding(.top, NekoSpace.xs)
                }.font(NekoFont.meta)
            } else {
                NekoSavedText(text: text)
            }
        }.font(NekoFont.body).textSelection(.enabled)
    }
}

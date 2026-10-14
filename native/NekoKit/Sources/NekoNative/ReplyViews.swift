import SwiftUI
import AppKit
import NekoKit

/// Lets a reply view answer for the person: send a message or fill the composer.
struct ReplyActions: Sendable {
    var send: @MainActor @Sendable (String) -> Void = { _ in }
    var draft: @MainActor @Sendable (String) -> Void = { _ in }
}
private struct ReplyActionsKey: EnvironmentKey { static let defaultValue = ReplyActions() }
extension EnvironmentValues {
    var replyActions: ReplyActions {
        get { self[ReplyActionsKey.self] }
        set { self[ReplyActionsKey.self] = newValue }
    }
}

/// Native macOS values for the chat surface, shared across light and dark modes.
enum ReplyStyle {
    static let groupFill = N.card
    static let groupStroke = N.line
    static let codeFill = N.panel
    static let hairline = N.line
    static let green = NekoStyle.mint
    static let red = NekoStyle.coral
    static let orange = NekoStyle.amber
    static let body = NekoFont.body
    static let small = NekoFont.meta
    static let caption = NekoFont.meta
    static let mono = NekoFont.mono
    static let terminalLanguages: Set<String> = ["sh", "bash", "zsh", "shell", "console", "terminal", "log", "output", "text-output"]
    static let viewKinds: Set<String> = ["neko-chart", "neko-choices", "neko-form", "neko-plan", "neko-files", "neko-sources"]
}

extension View {
    /// An opaque group for related controls or evidence. Prose stays on the canvas.
    func replyGroup(padding: CGFloat = NekoLayout.rowInset) -> some View {
        self.padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(ReplyStyle.groupFill, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
    }
}

/// Home and ticket replies share one quiet, readable user-message treatment.
/// Keep saved text literal; attachment rendering follows the existing path.
struct ChatUserMessage: View {
    let text: String
    var timestamp: Int = 0
    var queued = false

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            Spacer(minLength: NekoLayout.pageInset)
            VStack(alignment: .trailing, spacing: NekoLayout.authorGap) {
                ChatAuthorLine(author: "You", timestamp: timestamp)
                Group {
                    if text.contains("![") { ReadableText(text: text) }
                    else { Text(text).textSelection(.enabled) }
                }
                .font(NekoFont.chat).lineSpacing(4)
                .multilineTextAlignment(.leading)
                .padding(NekoLayout.messageInset)
                .background(N.card, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
                if queued { Label("Queued", systemImage: "clock").font(NekoFont.meta).foregroundStyle(.secondary) }
            }.frame(maxWidth: 640, alignment: .trailing)
        }.frame(maxWidth: .infinity, alignment: .trailing)
    }
}

// MARK: - Structured blocks

struct ReplyBlockView: View {
    let kind: String
    let source: String
    private var parsed: JSONValue { (try? JSONDecoder().decode(JSONValue.self, from: Data(source.utf8))) ?? .null }
    var body: some View {
        let value = parsed
        Group {
            if value == .null {
                NativeTerminalBlock(language: kind, code: source)
            } else {
                switch kind {
                case "neko-chart": ReplyChart(value: value)
                case "neko-choices": ReplyChoices(value: value)
                case "neko-form": ReplyForm(value: value)
                case "neko-plan": ReplyPlan(value: value)
                case "neko-files": ReplyFiles(value: value)
                case "neko-sources": ReplySources(items: value["sources"].array.map(\.string))
                default: NativeTerminalBlock(language: kind, code: source)
                }
            }
        }
    }
}

struct ReplyChart: View {
    let value: JSONValue
    private var bars: [(label: String, value: Double, highlight: Bool)] {
        value["bars"].array.compactMap { bar in
            guard case .number(let number) = bar["value"], number.isFinite else { return nil }
            return (bar["label"].string, number, bar["highlight"].bool)
        }
    }
    var body: some View {
        let peak = max(bars.map(\.value).max() ?? 1, 1)
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline) {
                Text(value["title"].string.isEmpty ? "Chart" : value["title"].string).font(NekoFont.heading)
                Spacer()
                if !value["source"].string.isEmpty { Text(value["source"].string).font(ReplyStyle.caption).foregroundStyle(.secondary) }
            }
            ForEach(Array(bars.enumerated()), id: \.offset) { _, bar in
                HStack(spacing: 10) {
                    Text(bar.label).font(ReplyStyle.caption).foregroundStyle(.secondary)
                        .frame(width: 110, alignment: .trailing).lineLimit(1)
                    GeometryReader { geometry in
                        RoundedRectangle(cornerRadius: NekoRadius.micro, style: .continuous)
                            .fill(bar.highlight ? ReplyStyle.orange : NekoStyle.accent.opacity(0.85))
                            .frame(width: max(2, geometry.size.width * bar.value / peak))
                    }.frame(height: 12)
                    Text(format(bar.value) + value["unit"].string).font(ReplyStyle.caption.monospacedDigit())
                        .foregroundStyle(bar.highlight ? ReplyStyle.orange : .primary.opacity(0.75))
                        .frame(width: 64, alignment: .leading)
                }.frame(minHeight: 24)
            }
        }
        .replyGroup()
        .accessibilityElement(children: .combine)
    }
    private func format(_ number: Double) -> String {
        number.rounded() == number ? number.formatted(.number.grouping(.automatic)) : number.formatted(.number.precision(.fractionLength(1)))
    }
}

struct ReplyChoices: View {
    let value: JSONValue
    @State private var selected: Int?
    @State private var sent = false
    @Environment(\.replyActions) private var actions
    var body: some View {
        let options = value["options"].array
        VStack(alignment: .leading, spacing: 18) {
            Text(value["question"].string).font(NekoFont.heading)
            VStack(alignment: .leading, spacing: 12) {
                ForEach(Array(options.enumerated()), id: \.offset) { index, option in
                    Button { selected = index } label: {
                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                            Image(systemName: selected == index ? "largecircle.fill.circle" : "circle")
                                .foregroundStyle(selected == index ? NekoStyle.accent : .secondary)
                            VStack(alignment: .leading, spacing: 5) {
                                Text(option["label"].string).font(ReplyStyle.body)
                                if !option["detail"].string.isEmpty { Text(option["detail"].string).font(ReplyStyle.caption).foregroundStyle(.secondary) }
                            }
                            Spacer(minLength: 0)
                        }.padding(.vertical, 4).contentShape(Rectangle())
                    }.buttonStyle(.plain).disabled(sent)
                        .accessibilityAddTraits(selected == index ? .isSelected : [])
                }
            }
            HStack {
                Spacer()
                Button("Other…") { actions.draft("") }.disabled(sent)
                Button(sent ? "Sent" : "Continue") {
                    guard let selected, options.indices.contains(selected) else { return }
                    sent = true
                    actions.send(options[selected]["label"].string)
                }
                .nekoButton(.primary)
                .keyboardShortcut(.defaultAction)
                .disabled(selected == nil || sent)
            }.controlSize(.small)
        }
        .replyGroup()
    }
}

struct ReplyForm: View {
    let value: JSONValue
    @State private var values: [Int: String] = [:]
    @State private var toggles: [Int: Bool] = [:]
    @State private var sent = false
    @Environment(\.replyActions) private var actions
    private var fields: [JSONValue] { value["fields"].array }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if !value["title"].string.isEmpty { Text(value["title"].string).font(NekoFont.heading) }
            VStack(spacing: 0) {
                ForEach(Array(fields.enumerated()), id: \.offset) { index, field in
                    HStack {
                        Text(field["label"].string).font(ReplyStyle.body)
                        Spacer(minLength: 12)
                        control(index, field)
                    }
                    .padding(.horizontal, NekoLayout.rowInset).padding(.vertical, 8).frame(minHeight: 48)
                    if index < fields.count - 1 { Divider().opacity(0.6) }
                }
            }.replyGroup(padding: 0)
            HStack {
                Spacer()
                Button(sent ? "Sent" : (value["submit"].string.isEmpty ? "Send" : value["submit"].string)) { submit() }
                    .nekoButton(.primary).disabled(sent)
            }.controlSize(.small)
        }
        .disabled(sent)
    }
    @ViewBuilder private func control(_ index: Int, _ field: JSONValue) -> some View {
        switch field["kind"].string {
        case "toggle":
            Toggle("", isOn: Binding(get: { toggles[index] ?? field["value"].bool }, set: { toggles[index] = $0 }))
                .labelsHidden().toggleStyle(.switch).controlSize(.small)
        case "choice":
            let options = field["options"].array.map(\.string)
            Picker("", selection: Binding(get: { values[index] ?? (field["value"].string.isEmpty ? options.first ?? "" : field["value"].string) }, set: { values[index] = $0 })) {
                ForEach(options, id: \.self) { Text($0).tag($0) }
            }.labelsHidden().fixedSize().controlSize(.small)
        default:
            TextField("", text: Binding(get: { values[index] ?? field["value"].string }, set: { values[index] = $0 }), prompt: Text(field["placeholder"].string))
                .textFieldStyle(.roundedBorder).multilineTextAlignment(.trailing).frame(width: 180).controlSize(.small)
        }
    }
    private func submit() {
        var lines = [value["title"].string.isEmpty ? "Here are the details:" : value["title"].string + ":"]
        for (index, field) in fields.enumerated() {
            let answer: String
            switch field["kind"].string {
            case "toggle": answer = (toggles[index] ?? field["value"].bool) ? "Yes" : "No"
            case "choice": answer = values[index] ?? (field["value"].string.isEmpty ? field["options"].array.first?.string ?? "" : field["value"].string)
            default: answer = values[index] ?? field["value"].string
            }
            lines.append("- \(field["label"].string): \(answer.isEmpty ? "(left empty)" : answer)")
        }
        sent = true
        actions.send(lines.joined(separator: "\n"))
    }
}

struct ReplyPlan: View {
    let value: JSONValue
    @State private var sent = false
    @Environment(\.replyActions) private var actions
    var body: some View {
        let steps = value["steps"].array.map { $0.string.isEmpty ? $0["title"].string : $0.string }
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .firstTextBaseline) {
                Text(value["title"].string.isEmpty ? "Plan" : value["title"].string).font(NekoFont.heading)
                Spacer()
                Text("\(steps.count) \(steps.count == 1 ? "step" : "steps")\(value["estimate"].string.isEmpty ? "" : " · " + value["estimate"].string)")
                    .font(ReplyStyle.caption).foregroundStyle(.secondary)
            }
            VStack(spacing: 0) {
                ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                        Text("\(index + 1)").font(ReplyStyle.caption.monospacedDigit()).foregroundStyle(.secondary).frame(width: 20, alignment: .trailing)
                        Text(step).font(ReplyStyle.body).lineSpacing(3).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 0)
                    }.padding(.horizontal, NekoLayout.rowInset).padding(.vertical, 12)
                    if index < steps.count - 1 { Divider().opacity(0.6) }
                }
            }.replyGroup(padding: 0)
            HStack {
                Spacer()
                Button("Change Plan…") { actions.draft("Change the plan: ") }.disabled(sent)
                Button(sent ? "Sent" : "Go Ahead") { sent = true; actions.send("Go ahead with this plan.") }
                    .nekoButton(.primary).disabled(sent)
            }.controlSize(.small)
        }
    }
}

struct ReplyFiles: View {
    let value: JSONValue
    @State private var selection: String?
    private var files: [(path: String, note: String)] {
        value["files"].array.compactMap { item in
            let path = item.string.isEmpty ? item["path"].string : item.string
            let expanded = (path as NSString).expandingTildeInPath
            guard expanded.hasPrefix("/"), FileManager.default.fileExists(atPath: expanded) else { return nil }
            return (expanded, item["note"].string)
        }
    }
    var body: some View {
        let rows = files
        if !rows.isEmpty {
            VStack(spacing: 2) {
                ForEach(rows, id: \.path) { file in
                    let selected = selection == file.path
                    HStack(spacing: 10) {
                        Image(nsImage: NSWorkspace.shared.icon(forFile: file.path)).resizable().frame(width: 28, height: 28)
                        VStack(alignment: .leading, spacing: 1) {
                            Text((file.path as NSString).lastPathComponent).font(ReplyStyle.body).lineLimit(1)
                                .foregroundStyle(selected ? Color(nsColor: .alternateSelectedControlTextColor) : .primary)
                            Text(file.note.isEmpty ? ReplyFiles.location(file.path) : ReplyFiles.location(file.path) + " · " + file.note)
                                .font(ReplyStyle.caption).foregroundStyle(selected ? Color(nsColor: .alternateSelectedControlTextColor).opacity(0.8) : .secondary).lineLimit(1)
                        }
                        Spacer(minLength: 8)
                        Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: file.path)]) }
                            .nekoButton(.quiet).font(ReplyStyle.caption).foregroundStyle(selected ? Color(nsColor: .alternateSelectedControlTextColor) : .secondary)
                    }
                    .padding(.horizontal, 10).padding(.vertical, 12)
                    .background(selected ? Color(nsColor: .selectedContentBackgroundColor) : .clear, in: RoundedRectangle(cornerRadius: NekoRadius.md, style: .continuous))
                    .contentShape(Rectangle())
                    .onTapGesture(count: 2) { NSWorkspace.shared.open(URL(fileURLWithPath: file.path)) }
                    .onTapGesture { selection = file.path }
                    .help(file.path)
                }
            }.replyGroup(padding: 8)
        }
    }
    static func location(_ path: String) -> String {
        let parent = (path as NSString).deletingLastPathComponent
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let relative = parent.hasPrefix(home) ? String(parent.dropFirst(home.count)).trimmingCharacters(in: CharacterSet(charactersIn: "/")) : parent
        return relative.isEmpty ? "Home" : relative.split(separator: "/").suffix(2).joined(separator: " › ")
    }
}

struct ReplySources: View {
    let items: [String]
    var body: some View {
        if !items.isEmpty {
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                        Text(item).font(ReplyStyle.caption).textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }.padding(.top, 6)
            } label: {
                Label("Sources · \(items.count)", systemImage: "text.quote")
                    .font(ReplyStyle.caption).foregroundStyle(.secondary)
            }
        }
    }
}

// MARK: - Steps, errors, memory, tickets

struct ReplySteps: View {
    let calls: [JSONValue]
    let receipts: [JSONValue]
    let connectionName: (String) -> String
    let workedFor: String?
    @State private var expanded = false
    private struct Row: Identifiable { let id: String; let status: String; let tool: String; let detail: String }
    private var rows: [Row] {
        if calls.isEmpty {
            return receipts.map { Row(id: $0.recordID, status: $0["success"].bool ? "succeeded" : "failed", tool: $0["tool_name"].string, detail: "") }
        }
        return calls.filter { $0["status"].string != "awaiting_approval" }.map { call in
            Row(id: call.recordID, status: call["status"].string, tool: call["tool_name"].string, detail: connectionName(call["connection_id"].string))
        }
    }
    var body: some View {
        let items = rows
        if !items.isEmpty {
            let failed = items.filter { ["failed", "denied"].contains($0.status) }.count
            VStack(alignment: .leading, spacing: 6) {
                Button { withNekoAnimation(.select) { expanded.toggle() } } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.right").font(NekoFont.of(.micro, weight: .semibold))
                            .rotationEffect(.degrees(expanded ? 90 : 0)).frame(width: 10)
                        Text([workedFor, "\(items.count) \(items.count == 1 ? "step" : "steps")", failed > 0 ? "\(failed) failed" : nil].compactMap { $0 }.joined(separator: " · "))
                            .font(NekoFont.of(.caption, weight: .medium))
                    }.foregroundStyle(failed > 0 ? ReplyStyle.orange : .secondary).contentShape(Rectangle())
                }.buttonStyle(.plain)
                if expanded {
                    VStack(spacing: 0) {
                        ForEach(items) { row in
                            HStack(spacing: 8) {
                                Image(systemName: symbol(row.status)).symbolRenderingMode(.hierarchical)
                                    .foregroundStyle(color(row.status)).font(NekoFont.caption).frame(width: 14)
                                Text(verb(row.status)).font(ReplyStyle.small).foregroundStyle(.secondary).frame(width: 64, alignment: .leading)
                                Text(row.tool).font(ReplyStyle.small).lineLimit(1)
                                if !row.detail.isEmpty { Text("· " + row.detail).font(ReplyStyle.small).foregroundStyle(.tertiary).lineLimit(1) }
                                Spacer(minLength: 0)
                            }.padding(.horizontal, NekoLayout.rowInset).frame(minHeight: 34)
                        }
                    }.padding(.vertical, 4).replyGroup(padding: 0)
                }
            }
        }
    }
    private func symbol(_ status: String) -> String {
        switch status { case "failed", "denied": "xmark.circle.fill"; case "running", "approved": "circle.dotted"; default: "checkmark.circle.fill" }
    }
    private func color(_ status: String) -> Color {
        switch status { case "failed", "denied": ReplyStyle.red; case "running", "approved": NekoStyle.accent; default: ReplyStyle.green }
    }
    private func verb(_ status: String) -> String {
        switch status { case "failed": "Failed"; case "denied": "Denied"; case "running", "approved": "Running"; default: "Used" }
    }
}

struct ReplyErrorCallout: View {
    let message: String
    let retry: (() -> Void)?
    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "exclamationmark.circle").foregroundStyle(ReplyStyle.red).font(NekoFont.of(.subtitle))
            VStack(alignment: .leading, spacing: 10) {
                Text("This reply didn’t finish").font(NekoFont.heading)
                if !message.isEmpty { Text(message).font(ReplyStyle.small).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true) }
                if let retry { Button("Try Again", action: retry).nekoButton(.primary).controlSize(.small) }
            }
            Spacer(minLength: 0)
        }
        .padding(NekoLayout.rowInset)
        .background(ReplyStyle.red.opacity(0.08), in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
    }
}

struct ReplyMemoryNote: View {
    let text: String
    var body: some View {
        Label { Text("Remembered: \(text)").font(ReplyStyle.small).foregroundStyle(.secondary) } icon: {
            Image(systemName: "brain").font(NekoFont.of(.small)).foregroundStyle(NekoStyle.accent)
        }
    }
}

struct ReplyTicketRow: View {
    let title: String
    let status: String
    let workspace: String
    let open: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: open) {
            HStack(alignment: .top, spacing: 12) {
                StatusGlyph(status: status, size: 16).padding(.top, 2).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 8) {
                    Text(title).font(NekoFont.heading).lineSpacing(3).lineLimit(2)
                    Text([workspace, friendlyTaskStatus(status)].filter { !$0.isEmpty }.joined(separator: " · ")).font(ReplyStyle.caption).foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                Image(systemName: "chevron.right").font(NekoFont.of(.micro, weight: .semibold)).foregroundStyle(.tertiary)
            }
            .multilineTextAlignment(.leading)
            .padding(.horizontal, 12).padding(.vertical, 16)
            .background(hover ? N.selected : .clear, in: RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous))
            .overlay(alignment: .top) { Hairline() }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .help("Open this task")
    }
}

func ticketStatusColor(_ status: String) -> Color {
    switch status {
    case "AwaitingApproval": NekoStyle.amber
    case "ReadyForReview": Color(nsColor: .systemBlue)
    case "Completed": NekoStyle.mint
    case "Failed": NekoStyle.coral
    case "Cancelled": Color.secondary
    default: NekoStyle.accent
    }
}

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

/// Native macOS values for the chat surface. Colors follow the system dark
/// palette so controls read as AppKit rather than as a web page.
enum ReplyStyle {
    static let groupFill = Color.white.opacity(0.045)
    static let groupStroke = Color.white.opacity(0.07)
    static let codeFill = Color(nsColor: NSColor(white: 0.08, alpha: 1))
    static let hairline = Color.white.opacity(0.07)
    static let green = Color(nsColor: .systemGreen)
    static let red = Color(nsColor: .systemRed)
    static let orange = Color(nsColor: .systemOrange)
    static let body = Font.system(size: 13)
    static let small = Font.system(size: 12)
    static let caption = Font.system(size: 11)
    static let mono = Font.system(size: 11, design: .monospaced)
    static let terminalLanguages: Set<String> = ["sh", "bash", "zsh", "shell", "console", "terminal", "log", "output", "text-output"]
    static let viewKinds: Set<String> = ["neko-chart", "neko-choices", "neko-form", "neko-plan", "neko-files", "neko-sources"]
}

extension View {
    /// The standard grouped container: a quiet fill and hairline, 8pt corners.
    func replyGroup(padding: CGFloat = 10) -> some View {
        self.padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(ReplyStyle.groupFill, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(ReplyStyle.groupStroke))
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
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(value["title"].string.isEmpty ? "Chart" : value["title"].string).font(.system(size: 12, weight: .semibold))
                Spacer()
                if !value["source"].string.isEmpty { Text(value["source"].string).font(ReplyStyle.caption).foregroundStyle(.secondary) }
            }
            ForEach(Array(bars.enumerated()), id: \.offset) { _, bar in
                HStack(spacing: 10) {
                    Text(bar.label).font(ReplyStyle.caption).foregroundStyle(.secondary)
                        .frame(width: 110, alignment: .trailing).lineLimit(1)
                    GeometryReader { geometry in
                        RoundedRectangle(cornerRadius: 3, style: .continuous)
                            .fill(bar.highlight ? ReplyStyle.orange : NekoStyle.accent.opacity(0.85))
                            .frame(width: max(2, geometry.size.width * bar.value / peak))
                    }.frame(height: 12)
                    Text(format(bar.value) + value["unit"].string).font(ReplyStyle.caption.monospacedDigit())
                        .foregroundStyle(bar.highlight ? ReplyStyle.orange : .primary.opacity(0.75))
                        .frame(width: 64, alignment: .leading)
                }.frame(height: 20)
            }
        }
        .replyGroup(padding: 12)
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
        VStack(alignment: .leading, spacing: 8) {
            Text(value["question"].string).font(.system(size: 13, weight: .semibold))
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(options.enumerated()), id: \.offset) { index, option in
                    Button { selected = index } label: {
                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                            Image(systemName: selected == index ? "largecircle.fill.circle" : "circle")
                                .foregroundStyle(selected == index ? NekoStyle.accent : .secondary)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(option["label"].string).font(ReplyStyle.body)
                                if !option["detail"].string.isEmpty { Text(option["detail"].string).font(ReplyStyle.caption).foregroundStyle(.secondary) }
                            }
                            Spacer(minLength: 0)
                        }.contentShape(Rectangle())
                    }.buttonStyle(.plain).disabled(sent)
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
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(selected == nil || sent)
            }.controlSize(.small)
        }
        .replyGroup(padding: 12)
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
        VStack(alignment: .leading, spacing: 8) {
            if !value["title"].string.isEmpty { Text(value["title"].string).font(.system(size: 13, weight: .semibold)) }
            VStack(spacing: 0) {
                ForEach(Array(fields.enumerated()), id: \.offset) { index, field in
                    HStack {
                        Text(field["label"].string).font(ReplyStyle.body)
                        Spacer(minLength: 12)
                        control(index, field)
                    }
                    .padding(.horizontal, 12).frame(minHeight: 34)
                    if index < fields.count - 1 { Divider().opacity(0.6) }
                }
            }.replyGroup(padding: 0)
            HStack {
                Spacer()
                Button(sent ? "Sent" : (value["submit"].string.isEmpty ? "Send" : value["submit"].string)) { submit() }
                    .buttonStyle(.borderedProminent).disabled(sent)
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
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(value["title"].string.isEmpty ? "Plan" : value["title"].string).font(.system(size: 13, weight: .semibold))
                Spacer()
                Text("\(steps.count) \(steps.count == 1 ? "step" : "steps")\(value["estimate"].string.isEmpty ? "" : " · " + value["estimate"].string)")
                    .font(ReplyStyle.caption).foregroundStyle(.secondary)
            }
            VStack(spacing: 0) {
                ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                    HStack(spacing: 8) {
                        Text("\(index + 1)").font(ReplyStyle.caption.monospacedDigit()).foregroundStyle(.tertiary).frame(width: 16, alignment: .trailing)
                        Text(step).font(ReplyStyle.body).textSelection(.enabled)
                        Spacer(minLength: 0)
                    }.padding(.horizontal, 10).frame(minHeight: 28)
                    if index < steps.count - 1 { Divider().opacity(0.6) }
                }
            }.replyGroup(padding: 0)
            HStack {
                Spacer()
                Button("Change Plan…") { actions.draft("Change the plan: ") }.disabled(sent)
                Button(sent ? "Sent" : "Go Ahead") { sent = true; actions.send("Go ahead with this plan.") }
                    .buttonStyle(.borderedProminent).disabled(sent)
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
                            Text(file.note.isEmpty ? ReplyFiles.location(file.path) : ReplyFiles.location(file.path) + " · " + file.note)
                                .font(ReplyStyle.caption).foregroundStyle(selected ? Color.white.opacity(0.8) : .secondary).lineLimit(1)
                        }
                        Spacer(minLength: 8)
                        Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: file.path)]) }
                            .buttonStyle(.borderless).font(ReplyStyle.caption).foregroundStyle(selected ? Color.white : .secondary)
                    }
                    .padding(.horizontal, 8).frame(height: 40)
                    .background(selected ? Color(nsColor: .selectedContentBackgroundColor) : .clear, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                    .contentShape(Rectangle())
                    .onTapGesture(count: 2) { NSWorkspace.shared.open(URL(fileURLWithPath: file.path)) }
                    .onTapGesture { selection = file.path }
                    .help(file.path)
                }
            }.replyGroup(padding: 4)
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
            HStack(spacing: 6) {
                Text("Sources").font(ReplyStyle.caption).foregroundStyle(.secondary)
                ForEach(items.prefix(8), id: \.self) { item in
                    Text(item).font(ReplyStyle.caption).foregroundStyle(NekoStyle.accent.opacity(0.95)).lineLimit(1)
                        .padding(.horizontal, 7).frame(height: 20)
                        .background(NekoStyle.accent.opacity(0.16), in: RoundedRectangle(cornerRadius: 5, style: .continuous))
                }
            }
        }
    }
}

// MARK: - Tables, diffs, terminal output

struct NativeTableBlock: View {
    let headers: [String]
    let rows: [[String]]
    @State private var sortColumn: Int?
    @State private var ascending = true
    @State private var copied = false
    @State private var availableWidth: CGFloat = 600
    private var columnWidth: CGFloat { max(160, (availableWidth - 20) / CGFloat(max(1, headers.count))) }
    private var sorted: [[String]] {
        guard let column = sortColumn else { return rows }
        return rows.sorted { a, b in
            let x = a.indices.contains(column) ? a[column] : "", y = b.indices.contains(column) ? b[column] : ""
            let nx = Double(x.filter { "0123456789.-−".contains($0) }.replacingOccurrences(of: "−", with: "-"))
            let ny = Double(y.filter { "0123456789.-−".contains($0) }.replacingOccurrences(of: "−", with: "-"))
            if let nx, let ny { return ascending ? nx < ny : nx > ny }
            let order = x.localizedStandardCompare(y)
            return ascending ? order == .orderedAscending : order == .orderedDescending
        }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Spacer()
                Button(copied ? "Copied" : "Copy as CSV") {
                    let csv = ([headers] + rows).map { $0.map { "\"\($0.replacingOccurrences(of: "\"", with: "\"\""))\"" }.joined(separator: ",") }.joined(separator: "\n")
                    NSPasteboard.general.clearContents()
                    copied = NSPasteboard.general.setString(csv, forType: .string)
                }.buttonStyle(.borderless).font(ReplyStyle.caption)
            }
            ScrollView(.horizontal) {
            VStack(spacing: 0) {
                HStack(spacing: 0) {
                    ForEach(Array(headers.enumerated()), id: \.offset) { index, header in
                        Button {
                            if sortColumn == index { ascending.toggle() } else { sortColumn = index; ascending = true }
                        } label: {
                            HStack(spacing: 3) {
                                Text(header).font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary)
                                if sortColumn == index { Image(systemName: ascending ? "chevron.up" : "chevron.down").font(.system(size: 8, weight: .bold)).foregroundStyle(.secondary) }
                            }.padding(.horizontal, 6).frame(width: columnWidth, alignment: .leading).contentShape(Rectangle())
                        }.buttonStyle(.plain)
                    }
                }.padding(.horizontal, 10).padding(.vertical, 8)
                Divider()
                ForEach(Array(sorted.enumerated()), id: \.offset) { index, row in
                    HStack(alignment: .top, spacing: 0) {
                        ForEach(Array(headers.indices), id: \.self) { column in
                            let cell = row.indices.contains(column) ? row[column] : ""
                            Text(inline(cell)).font(ReplyStyle.small.monospacedDigit())
                                .foregroundStyle(tone(cell, column: column))
                                .fixedSize(horizontal: false, vertical: true)
                                .padding(.horizontal, 6).frame(width: columnWidth, alignment: .leading)
                        }
                    }
                    .padding(.horizontal, 10).padding(.vertical, 8)
                    .background(index % 2 == 1 ? Color.white.opacity(0.035) : .clear)
                }
            }
            }
            .background(ReplyStyle.groupFill, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(ReplyStyle.groupStroke))
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .textSelection(.enabled)
        }
        .background(GeometryReader { geometry in
            Color.clear.onAppear { availableWidth = geometry.size.width }
                .onChange(of: geometry.size.width) { _, width in availableWidth = width }
        })
    }
    private func inline(_ value: String) -> AttributedString {
        (try? AttributedString(markdown: value, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(value)
    }
    private func tone(_ cell: String, column: Int) -> Color {
        guard column > 0 else { return .primary }
        let trimmed = cell.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("−") || (trimmed.hasPrefix("-") && trimmed.dropFirst().first?.isNumber == true) { return ReplyStyle.red }
        if trimmed.hasPrefix("+"), trimmed.dropFirst().first?.isNumber == true { return ReplyStyle.green }
        return .primary
    }
}

struct NativeDiffBlock: View {
    let code: String
    @State private var expanded: Set<Int> = [0]
    @State private var lineLimits: [Int: Int] = [:]
    @State private var copied = false
    private struct FileDiff { var name: String; var lines: [String]; var added: Int; var removed: Int }
    private var files: [FileDiff] {
        var result: [FileDiff] = []
        for line in code.components(separatedBy: "\n") {
            if line.hasPrefix("diff --git") || line.hasPrefix("+++ ") {
                let name = line.hasPrefix("+++ ") ? String(line.dropFirst(4)).replacingOccurrences(of: "b/", with: "", options: .anchored) : (line.split(separator: " ").last.map { String($0).replacingOccurrences(of: "b/", with: "", options: .anchored) } ?? "")
                if line.hasPrefix("+++ "), let last = result.indices.last, result[last].lines.isEmpty || result[last].name == name { result[last].name = name; continue }
                result.append(FileDiff(name: name, lines: [], added: 0, removed: 0)); continue
            }
            if line.hasPrefix("--- ") || line.hasPrefix("index ") || line.hasPrefix("new file") || line.hasPrefix("deleted file") { continue }
            if result.isEmpty { result.append(FileDiff(name: "Changes", lines: [], added: 0, removed: 0)) }
            result[result.count - 1].lines.append(line)
            if line.hasPrefix("+") { result[result.count - 1].added += 1 }
            if line.hasPrefix("-") { result[result.count - 1].removed += 1 }
        }
        return result
    }
    var body: some View {
        let all = files
        VStack(spacing: 0) {
            HStack {
                Text("Changes").font(ReplyStyle.caption).foregroundStyle(.secondary)
                Spacer()
                Button(copied ? "Copied" : "Copy full diff") {
                    NSPasteboard.general.clearContents()
                    copied = NSPasteboard.general.setString(code, forType: .string)
                }.buttonStyle(.borderless).font(ReplyStyle.caption)
            }.padding(.horizontal, 10).padding(.vertical, 6)
            ForEach(Array(all.enumerated()), id: \.offset) { index, file in
                if index > 0 { Divider().opacity(0.6) }
                Button {
                    if expanded.contains(index) { expanded.remove(index) } else { expanded.insert(index) }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: expanded.contains(index) ? "chevron.down" : "chevron.right").font(.system(size: 9, weight: .semibold)).foregroundStyle(.secondary).frame(width: 10)
                        Text((file.name as NSString).lastPathComponent).font(ReplyStyle.small)
                        Text((file.name as NSString).deletingLastPathComponent).font(ReplyStyle.caption).foregroundStyle(.tertiary).lineLimit(1)
                        Spacer()
                        if file.added > 0 { Text("+\(file.added)").font(ReplyStyle.caption.monospacedDigit()).foregroundStyle(ReplyStyle.green) }
                        if file.removed > 0 { Text("−\(file.removed)").font(ReplyStyle.caption.monospacedDigit()).foregroundStyle(ReplyStyle.red) }
                    }.padding(.horizontal, 10).frame(height: 28).contentShape(Rectangle())
                }.buttonStyle(.plain)
                if expanded.contains(index) {
                    Divider().opacity(0.6)
                    ScrollView(.horizontal) {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(file.lines.prefix(lineLimits[index, default: 400]).enumerated()), id: \.offset) { _, line in
                                Text(line.isEmpty ? " " : line)
                                    .font(ReplyStyle.mono)
                                    .foregroundStyle(line.hasPrefix("@@") ? Color.secondary : .primary.opacity(line.hasPrefix("+") || line.hasPrefix("-") ? 0.95 : 0.65))
                                    .padding(.horizontal, 10).frame(maxWidth: .infinity, minHeight: 17, alignment: .leading)
                                    .background(line.hasPrefix("+") ? ReplyStyle.green.opacity(0.13) : line.hasPrefix("-") ? ReplyStyle.red.opacity(0.14) : .clear)
                            }
                        }.padding(.vertical, 4).fixedSize(horizontal: true, vertical: false)
                    }.textSelection(.enabled)
                    let shown = min(file.lines.count, lineLimits[index, default: 400])
                    if shown < file.lines.count {
                        HStack {
                            Text("Showing \(shown) of \(file.lines.count) lines").foregroundStyle(.secondary)
                            Spacer()
                            Button("Show next \(min(400, file.lines.count - shown)) lines") { lineLimits[index] = shown + 400 }
                                .buttonStyle(.borderless)
                        }.font(ReplyStyle.caption).padding(10)
                    }
                }
            }
        }
        .background(ReplyStyle.codeFill, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(ReplyStyle.groupStroke))
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

struct NativeTerminalBlock: View {
    let language: String
    let code: String
    @State private var copied = false
    @State private var lineLimit = 300
    private var command: String? {
        let first = code.components(separatedBy: "\n").first?.trimmingCharacters(in: .whitespaces) ?? ""
        return first.hasPrefix("$ ") ? String(first.dropFirst(2)) : nil
    }
    private var output: [String] {
        let lines = code.components(separatedBy: "\n")
        return command == nil ? lines : Array(lines.dropFirst())
    }
    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text(command ?? (language.isEmpty ? "Output" : language)).font(ReplyStyle.mono).foregroundStyle(.secondary).lineLimit(1)
                Spacer()
                let failed = output.filter { $0.contains("✗") || $0.lowercased().contains("fail") || $0.contains("error") }.count
                if failed > 0 { Text("\(failed) failed").font(ReplyStyle.caption).foregroundStyle(ReplyStyle.red) }
                Button(copied ? "Copied" : "Copy") {
                    NSPasteboard.general.clearContents()
                    copied = NSPasteboard.general.setString(code, forType: .string)
                }.buttonStyle(.borderless).font(ReplyStyle.caption)
            }.padding(.horizontal, 10).frame(height: 28)
            Divider().opacity(0.6)
            ScrollView(.horizontal) {
                VStack(alignment: .leading, spacing: 1) {
                    ForEach(Array(output.prefix(lineLimit).enumerated()), id: \.offset) { _, line in
                        Text(line.isEmpty ? " " : line).font(ReplyStyle.mono).foregroundStyle(color(line))
                    }
                }.padding(10).fixedSize(horizontal: true, vertical: false)
            }.textSelection(.enabled)
            if output.count > lineLimit {
                HStack {
                    Text("Showing \(lineLimit) of \(output.count) lines").foregroundStyle(.secondary)
                    Spacer()
                    Button("Show next \(min(300, output.count - lineLimit)) lines") { lineLimit += 300 }
                        .buttonStyle(.borderless)
                }.font(ReplyStyle.caption).padding(10)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(ReplyStyle.codeFill, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(ReplyStyle.groupStroke))
    }
    private func color(_ line: String) -> Color {
        let text = line.trimmingCharacters(in: .whitespaces)
        if text.hasPrefix("✓") || text.hasPrefix("PASS") || text.hasPrefix("ok ") { return Color(nsColor: .systemGreen).opacity(0.9) }
        if text.hasPrefix("✗") || text.hasPrefix("FAIL") || text.lowercased().hasPrefix("error") { return Color(nsColor: .systemRed).opacity(0.95) }
        return .primary.opacity(0.75)
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
                Button { withAnimation(.snappy(duration: 0.2)) { expanded.toggle() } } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.right").font(.system(size: 9, weight: .semibold))
                            .rotationEffect(.degrees(expanded ? 90 : 0)).frame(width: 10)
                        Text([workedFor, "\(items.count) \(items.count == 1 ? "step" : "steps")", failed > 0 ? "\(failed) failed" : nil].compactMap { $0 }.joined(separator: " · "))
                            .font(.system(size: 12, weight: .medium))
                    }.foregroundStyle(failed > 0 ? ReplyStyle.orange : .secondary).contentShape(Rectangle())
                }.buttonStyle(.plain)
                if expanded {
                    VStack(spacing: 0) {
                        ForEach(items) { row in
                            HStack(spacing: 8) {
                                Image(systemName: symbol(row.status)).symbolRenderingMode(.palette)
                                    .foregroundStyle(.white, color(row.status)).font(.system(size: 12)).frame(width: 14)
                                Text(verb(row.status)).font(ReplyStyle.small).foregroundStyle(.secondary).frame(width: 64, alignment: .leading)
                                Text(row.tool).font(ReplyStyle.small).lineLimit(1)
                                if !row.detail.isEmpty { Text("· " + row.detail).font(ReplyStyle.small).foregroundStyle(.tertiary).lineLimit(1) }
                                Spacer(minLength: 0)
                            }.padding(.horizontal, 10).frame(height: 24)
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
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.circle.fill").symbolRenderingMode(.palette).foregroundStyle(.white, ReplyStyle.red).font(.system(size: 16))
            VStack(alignment: .leading, spacing: 6) {
                Text("This reply didn’t finish").font(.system(size: 13, weight: .semibold))
                if !message.isEmpty { Text(message).font(ReplyStyle.small).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true) }
                if let retry { Button("Try Again", action: retry).buttonStyle(.borderedProminent).controlSize(.small) }
            }
            Spacer(minLength: 0)
        }
        .padding(12)
        .background(ReplyStyle.red.opacity(0.10), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(ReplyStyle.red.opacity(0.25)))
    }
}

struct ReplyMemoryNote: View {
    let text: String
    var body: some View {
        Label { Text("Remembered: \(text)").font(ReplyStyle.small).foregroundStyle(.secondary) } icon: {
            Image(systemName: "brain").font(.system(size: 11)).foregroundStyle(NekoStyle.accent)
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
            HStack(spacing: 10) {
                Circle().fill(ticketStatusColor(status)).frame(width: 8, height: 8)
                VStack(alignment: .leading, spacing: 1) {
                    Text(title).font(ReplyStyle.body).lineLimit(1)
                    Text([workspace, friendlyTaskStatus(status)].filter { !$0.isEmpty }.joined(separator: " · ")).font(ReplyStyle.caption).foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(.tertiary)
            }
            .padding(.horizontal, 12).frame(height: 44)
            .background(hover ? Color.white.opacity(0.07) : ReplyStyle.groupFill, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(ReplyStyle.groupStroke))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .help("Open this agent’s conversation")
    }
}

func ticketStatusColor(_ status: String) -> Color {
    switch status {
    case "AwaitingApproval": Color(nsColor: .systemOrange)
    case "ReadyForReview": Color(nsColor: .systemBlue)
    case "Completed": Color(nsColor: .systemGreen)
    case "Failed": Color(nsColor: .systemRed)
    case "Cancelled": Color.secondary
    default: NekoStyle.accent
    }
}

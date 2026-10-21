import SwiftUI
import AppKit

// MARK: - Tables, diffs, terminal output

struct NativeTableBlock: View {
    let headers: [String]
    let rows: [[String]]
    @State private var sortColumn: Int?
    @State private var ascending = true
    @State private var copied = false
    /// Narrowest a column may get before the table scrolls sideways instead.
    static let minimumColumn: CGFloat = 120
    /// Each column's width once the table scrolls sideways.
    static let scrollingColumn: CGFloat = 160
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
    /// Laid out once, at the width it is given: flexible columns when they fit,
    /// otherwise fixed columns that scroll sideways. No measured width is
    /// stored, so the table never re-wraps (and jumps) after its first frame.
    var body: some View {
        ChatBlock(label: "Table") {
            Button(copied ? "Copied" : "Copy as CSV") {
                let csv = ([headers] + rows).map { $0.map { "\"\($0.replacingOccurrences(of: "\"", with: "\"\""))\"" }.joined(separator: ",") }.joined(separator: "\n")
                NSPasteboard.general.clearContents()
                copied = NSPasteboard.general.setString(csv, forType: .string)
            }.nekoButton(.quiet).font(NekoFont.caption)
        } content: {
            ViewThatFits(in: .horizontal) {
                grid(columnWidth: nil)
                ScrollView(.horizontal) { grid(columnWidth: Self.scrollingColumn) }
            }
            .textSelection(.enabled)
        }
    }

    private func grid(columnWidth: CGFloat?) -> some View {
        Grid(alignment: .topLeading, horizontalSpacing: 0, verticalSpacing: 0) {
            GridRow {
                ForEach(Array(headers.enumerated()), id: \.offset) { index, header in
                    Button {
                        if sortColumn == index { ascending.toggle() } else { sortColumn = index; ascending = true }
                    } label: {
                        HStack(spacing: NekoSpace.xxs) {
                            Text(header).font(NekoFont.of(.caption, weight: .semibold)).foregroundStyle(N.text3)
                            if sortColumn == index { Image(systemName: ascending ? "chevron.up" : "chevron.down").font(NekoFont.of(.micro, weight: .bold)).foregroundStyle(N.text3) }
                        }
                        .cell(columnWidth).contentShape(Rectangle())
                    }.buttonStyle(.plain)
                        .accessibilityLabel("Sort by \(header)")
                        .accessibilityValue(sortColumn == index ? (ascending ? "Ascending" : "Descending") : "Not sorted")
                }
            }
            Divider().gridCellUnsizedAxes(.horizontal)
            ForEach(Array(sorted.enumerated()), id: \.offset) { index, row in
                GridRow {
                    ForEach(Array(headers.indices), id: \.self) { column in
                        let cell = row.indices.contains(column) ? row[column] : ""
                        Text(inline(cell)).font(NekoFont.meta.monospacedDigit())
                            .foregroundStyle(tone(cell, column: column))
                            .fixedSize(horizontal: false, vertical: true)
                            .cell(columnWidth)
                    }
                }
                if index < rows.count - 1 { Divider().opacity(0.5).gridCellUnsizedAxes(.horizontal) }
            }
        }
        .padding(.horizontal, NekoSpace.xs)
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

private extension View {
    /// A table cell: flexible from `minimumColumn` up when `width` is nil, else fixed.
    func cell(_ width: CGFloat?) -> some View {
        padding(.horizontal, NekoSpace.xs).padding(.vertical, NekoSpace.sm)
            .frame(minWidth: width ?? NativeTableBlock.minimumColumn,
                   idealWidth: width ?? NativeTableBlock.minimumColumn,
                   maxWidth: width ?? .infinity, alignment: .leading)
    }
}

struct NativeDiffBlock: View {
    let code: String
    @State private var expanded: Set<Int>
    /// How many files start open; the rest open on click.
    init(code: String, openFiles: Int = 1) {
        self.code = code
        _expanded = State(initialValue: Set(0..<max(openFiles, 0)))
    }
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
                }.nekoButton(.quiet).font(ReplyStyle.caption)
            }.padding(.horizontal, NekoLayout.rowInset).padding(.vertical, 12)
            ForEach(Array(all.enumerated()), id: \.offset) { index, file in
                if index > 0 { Divider().opacity(0.6) }
                Button {
                    if expanded.contains(index) { expanded.remove(index) } else { expanded.insert(index) }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: expanded.contains(index) ? "chevron.down" : "chevron.right").font(NekoFont.of(.micro, weight: .semibold)).foregroundStyle(.secondary).frame(width: 10)
                        Text((file.name as NSString).lastPathComponent).font(ReplyStyle.small)
                        Text((file.name as NSString).deletingLastPathComponent).font(ReplyStyle.caption).foregroundStyle(.tertiary).lineLimit(1)
                        Spacer()
                        if file.added > 0 { Text("+\(file.added)").font(ReplyStyle.caption.monospacedDigit()).foregroundStyle(ReplyStyle.green) }
                        if file.removed > 0 { Text("−\(file.removed)").font(ReplyStyle.caption.monospacedDigit()).foregroundStyle(ReplyStyle.red) }
                    }.padding(.horizontal, NekoLayout.rowInset).frame(minHeight: 40).contentShape(Rectangle())
                }.buttonStyle(.plain)
                if expanded.contains(index) {
                    Divider().opacity(0.6)
                    ScrollView(.horizontal) {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(file.lines.prefix(lineLimits[index, default: 400]).enumerated()), id: \.offset) { _, line in
                                Text(line.isEmpty ? " " : line)
                                    .font(ReplyStyle.mono)
                                    .foregroundStyle(line.hasPrefix("@@") ? Color.secondary : .primary.opacity(line.hasPrefix("+") || line.hasPrefix("-") ? 0.95 : 0.85))
                                    .padding(.horizontal, NekoLayout.rowInset).frame(maxWidth: .infinity, minHeight: 20, alignment: .leading)
                                    .background(line.hasPrefix("+") ? ReplyStyle.green.opacity(0.13) : line.hasPrefix("-") ? ReplyStyle.red.opacity(0.14) : .clear)
                            }
                        }.padding(.vertical, 8).fixedSize(horizontal: true, vertical: false)
                    }.textSelection(.enabled)
                    let shown = min(file.lines.count, lineLimits[index, default: 400])
                    if shown < file.lines.count {
                        HStack {
                            Text("Showing \(shown) of \(file.lines.count) lines").foregroundStyle(.secondary)
                            Spacer()
                            Button("Show next \(min(400, file.lines.count - shown)) lines") { lineLimits[index] = shown + 400 }
                                .nekoButton(.quiet)
                        }.font(ReplyStyle.caption).padding(NekoLayout.rowInset)
                    }
                }
            }
        }
        .background(ReplyStyle.codeFill, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
        .clipShape(RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
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
                }.nekoButton(.quiet).font(ReplyStyle.caption)
            }.padding(.horizontal, NekoLayout.rowInset).frame(minHeight: 44)
            Divider().opacity(0.6)
            ScrollView(.horizontal) {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(output.prefix(lineLimit).enumerated()), id: \.offset) { _, line in
                        Text(line.isEmpty ? " " : line).font(ReplyStyle.mono).foregroundStyle(color(line))
                    }
                }.padding(NekoLayout.rowInset).fixedSize(horizontal: true, vertical: false)
            }.textSelection(.enabled)
            if output.count > lineLimit {
                HStack {
                    Text("Showing \(lineLimit) of \(output.count) lines").foregroundStyle(.secondary)
                    Spacer()
                    Button("Show next \(min(300, output.count - lineLimit)) lines") { lineLimit += 300 }
                        .nekoButton(.quiet)
                }.font(ReplyStyle.caption).padding(NekoLayout.rowInset)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(ReplyStyle.codeFill, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
        .clipShape(RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
    }
    private func color(_ line: String) -> Color {
        let text = line.trimmingCharacters(in: .whitespaces)
        if text.hasPrefix("✓") || text.hasPrefix("PASS") || text.hasPrefix("ok ") { return ReplyStyle.green }
        if text.hasPrefix("✗") || text.hasPrefix("FAIL") || text.lowercased().hasPrefix("error") { return ReplyStyle.red }
        return .primary.opacity(0.85)
    }
}

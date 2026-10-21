import SwiftUI
import AppKit

// MARK: - Tables, diffs, terminal output

/// A reply's table, drawn like T3 Code's: no card, 12pt cells on a 19.5pt line,
/// a semibold muted header, hairlines between rows. Columns are sized from
/// their content (`ChatTableLayout`), so a short column keeps its width and
/// only the long ones wrap.
struct NativeTableBlock: View {
    let headers: [String]
    let rows: [[String]]
    @State private var sortColumn: Int?
    @State private var ascending = true
    @State private var copied = false
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
    private var csv: String {
        ([headers] + rows).map { $0.map { "\"\($0.replacingOccurrences(of: "\"", with: "\"\""))\"" }.joined(separator: ",") }.joined(separator: "\n")
    }

    var body: some View {
        let body = sorted
        ChatTableLayout(columns: max(headers.count, 1)) {
            ForEach(Array(headers.enumerated()), id: \.offset) { index, header in
                Button {
                    if sortColumn == index { ascending.toggle() } else { sortColumn = index; ascending = true }
                } label: {
                    HStack(spacing: NekoSpace.xxs) {
                        Text(header).font(NekoFont.of(.caption, weight: .semibold)).foregroundStyle(N.text3)
                        if sortColumn == index { Image(systemName: ascending ? "chevron.up" : "chevron.down").font(NekoFont.of(.micro, weight: .bold)).foregroundStyle(N.text3) }
                    }
                    .cell(rule: true)
                }.buttonStyle(.plain)
                    .accessibilityLabel("Sort by \(header)")
                    .accessibilityValue(sortColumn == index ? (ascending ? "Ascending" : "Descending") : "Not sorted")
            }
            ForEach(Array(body.enumerated()), id: \.offset) { index, row in
                ForEach(Array(headers.indices), id: \.self) { column in
                    let cell = row.indices.contains(column) ? row[column] : ""
                    Text(MarkdownCache.tableCell(cell)).font(NekoFont.caption.monospacedDigit())
                        .lineSpacing(NekoChat.tableLeading)
                        .foregroundStyle(tone(cell, column: column))
                        .cell(rule: index < body.count - 1)
                }
            }
        }
        .textSelection(.enabled)
        .contextMenu {
            Button(copied ? "Copied" : "Copy table as CSV") {
                NSPasteboard.general.clearContents()
                copied = NSPasteboard.general.setString(csv, forType: .string)
            }
        }
    }
    private func tone(_ cell: String, column: Int) -> Color {
        guard column > 0 else { return .primary }
        let trimmed = cell.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("−") || (trimmed.hasPrefix("-") && trimmed.dropFirst().first?.isNumber == true) { return ReplyStyle.red }
        if trimmed.hasPrefix("+"), trimmed.dropFirst().first?.isNumber == true { return ReplyStyle.green }
        return .primary
    }
}

/// Lays out `columns` columns of cells row by row. A column's natural width is
/// its widest cell on one line. When everything fits, columns grow in
/// proportion to fill the width; when it does not, columns that already fit
/// their fair share keep their natural width and the long ones share what is
/// left in proportion, so five columns never become five tall slivers.
struct ChatTableLayout: Layout {
    let columns: Int
    /// No column is squeezed below this.
    static let minimumColumn: CGFloat = 56

    private func naturalWidths(_ subviews: Subviews) -> [CGFloat] {
        var widths = Array(repeating: CGFloat.zero, count: columns)
        for (index, view) in subviews.enumerated() {
            let column = index % columns
            widths[column] = max(widths[column], view.sizeThatFits(.unspecified).width)
        }
        return widths
    }

    static func columnWidths(natural: [CGFloat], available: CGFloat) -> [CGFloat] {
        let total = natural.reduce(0, +)
        guard total > 0, available.isFinite else { return natural }
        if total <= available { return natural.map { $0 * available / total } }
        var result = natural
        var open = Set(natural.indices)
        var remaining = available
        var settled = false
        while !settled, !open.isEmpty {
            settled = true
            let share = remaining / CGFloat(open.count)
            for index in open where natural[index] <= share {
                remaining -= natural[index]
                open.remove(index)
                settled = false
            }
        }
        let weight = open.reduce(0) { $0 + natural[$1] }
        for index in open { result[index] = max(minimumColumn, remaining * natural[index] / max(weight, 1)) }
        return result
    }

    private func rowHeights(_ subviews: Subviews, widths: [CGFloat]) -> [CGFloat] {
        let rows = (subviews.count + columns - 1) / columns
        return (0..<rows).map { row in
            (0..<columns).compactMap { column -> CGFloat? in
                let index = row * columns + column
                guard index < subviews.count else { return nil }
                return subviews[index].sizeThatFits(ProposedViewSize(width: widths[column], height: nil)).height
            }.max() ?? 0
        }
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let natural = naturalWidths(subviews)
        let width = proposal.width.flatMap { $0.isFinite ? $0 : nil } ?? natural.reduce(0, +)
        let widths = Self.columnWidths(natural: natural, available: width)
        return CGSize(width: width, height: rowHeights(subviews, widths: widths).reduce(0, +))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let widths = Self.columnWidths(natural: naturalWidths(subviews), available: bounds.width)
        let heights = rowHeights(subviews, widths: widths)
        var y = bounds.minY
        for (row, height) in heights.enumerated() {
            var x = bounds.minX
            for column in 0..<columns {
                let index = row * columns + column
                if index < subviews.count {
                    subviews[index].place(at: CGPoint(x: x, y: y), anchor: .topLeading, proposal: ProposedViewSize(width: widths[column], height: height))
                }
                x += widths[column]
            }
            y += height
        }
    }
}

private extension View {
    /// A table cell: padded, as wide as its column, with a hairline under it when `rule` is set.
    func cell(rule: Bool) -> some View {
        padding(.horizontal, NekoChat.tableCellX).padding(.vertical, NekoChat.tableCellY)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .overlay(alignment: .bottom) { if rule { Hairline() } }
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
        ChatBlock(label: "\(all.count) \(all.count == 1 ? "file" : "files") changed", fill: N.sunken) {
            ChatCopyButton(text: { code }, help: "Copy full diff")
        } content: {
            VStack(spacing: 0) {
                ForEach(Array(all.enumerated()), id: \.offset) { index, file in
                    if index > 0 { Divider().opacity(0.6) }
                    Button {
                        if expanded.contains(index) { expanded.remove(index) } else { expanded.insert(index) }
                    } label: {
                        HStack(spacing: NekoSpace.xs) {
                            Image(systemName: "chevron.right").font(NekoFont.of(.micro, weight: .semibold)).foregroundStyle(N.text3)
                                .rotationEffect(.degrees(expanded.contains(index) ? 90 : 0)).frame(width: NekoSpace.md)
                            Text((file.name as NSString).lastPathComponent).font(NekoFont.meta).foregroundStyle(N.text)
                            Text((file.name as NSString).deletingLastPathComponent).font(NekoFont.caption).foregroundStyle(N.text4).lineLimit(1).truncationMode(.middle)
                            Spacer()
                            if file.added > 0 { Text("+\(file.added)").font(NekoFont.caption.monospacedDigit()).foregroundStyle(NekoStyle.mint) }
                            if file.removed > 0 { Text("−\(file.removed)").font(NekoFont.caption.monospacedDigit()).foregroundStyle(NekoStyle.coral) }
                        }.padding(.horizontal, NekoChat.blockInset).frame(minHeight: NekoChat.blockHeader).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityValue(expanded.contains(index) ? "Expanded" : "Collapsed")
                    if expanded.contains(index) {
                        Divider().opacity(0.6)
                        ScrollView(.horizontal) {
                            VStack(alignment: .leading, spacing: 0) {
                                ForEach(Array(file.lines.prefix(lineLimits[index, default: 400]).enumerated()), id: \.offset) { _, line in
                                    Text(line.isEmpty ? " " : line)
                                        .font(NekoChat.code)
                                        .foregroundStyle(line.hasPrefix("@@") ? N.text3 : line.hasPrefix("+") || line.hasPrefix("-") ? N.text : N.text2)
                                        .padding(.horizontal, NekoChat.blockInset).frame(maxWidth: .infinity, minHeight: NekoSpace.xl, alignment: .leading)
                                        .background(line.hasPrefix("+") ? NekoStyle.mint.opacity(0.13) : line.hasPrefix("-") ? NekoStyle.coral.opacity(0.14) : .clear)
                                }
                            }.padding(.vertical, NekoSpace.sm).fixedSize(horizontal: true, vertical: false)
                        }.textSelection(.enabled)
                        let shown = min(file.lines.count, lineLimits[index, default: 400])
                        if shown < file.lines.count {
                            HStack {
                                Text("Showing \(shown) of \(file.lines.count) lines").foregroundStyle(N.text3)
                                Spacer()
                                Button("Show next \(min(400, file.lines.count - shown)) lines") { lineLimits[index] = shown + 400 }
                                    .nekoButton(.quiet)
                            }.font(NekoFont.caption).padding(NekoChat.blockInset)
                        }
                    }
                }
            }
        }
    }
}

/// Command output: one selectable text per block, coloured line by line,
/// rather than one view per line.
struct NativeTerminalBlock: View {
    let language: String
    let code: String
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
        let lines = output
        let failed = lines.filter { $0.contains("✗") || $0.lowercased().contains("fail") || $0.contains("error") }.count
        ChatBlock(label: command ?? (language.isEmpty ? "Output" : language), mono: command != nil, fill: N.sunken) {
            if failed > 0 { Text("\(failed) failed").font(NekoFont.caption.monospacedDigit()).foregroundStyle(NekoStyle.coral) }
            ChatCopyButton(text: { code })
        } content: {
            ScrollView(.horizontal) {
                Text(Self.colored(Array(lines.prefix(lineLimit))))
                    .font(NekoChat.code).lineSpacing(NekoChat.codeLineSpacing)
                    .padding(NekoChat.blockInset).fixedSize(horizontal: true, vertical: false)
            }.textSelection(.enabled)
            if lines.count > lineLimit {
                HStack {
                    Text("Showing \(lineLimit) of \(lines.count) lines").foregroundStyle(N.text3)
                    Spacer()
                    Button("Show next \(min(300, lines.count - lineLimit)) lines") { lineLimit += 300 }
                        .nekoButton(.quiet)
                }.font(NekoFont.caption).padding(NekoChat.blockInset)
            }
        }
    }
    static func colored(_ lines: [String]) -> AttributedString {
        var result = AttributedString()
        for (index, line) in lines.enumerated() {
            var run = AttributedString((line.isEmpty ? " " : line) + (index == lines.count - 1 ? "" : "\n"))
            run.foregroundColor = color(line)
            result += run
        }
        return result
    }
    private static func color(_ line: String) -> Color {
        let text = line.trimmingCharacters(in: .whitespaces)
        if text.hasPrefix("✓") || text.hasPrefix("PASS") || text.hasPrefix("ok ") { return NekoStyle.mint }
        if text.hasPrefix("✗") || text.hasPrefix("FAIL") || text.lowercased().hasPrefix("error") { return NekoStyle.coral }
        return N.text2
    }
}

import AppKit
import SwiftUI

enum NativeMarkdownBlock: Equatable {
    case paragraph(String), heading(Int, String), code(String, String), listItem(String, String), quote(String), image(String, String), divider
    case table([String], [[String]]), view(String, String)
}

enum NativeMarkdown {
    /// "| a | b |" → ["a", "b"].
    static func tableCells(_ line: String) -> [String] {
        let text = Array(line.trimmingCharacters(in: .whitespaces))
        var cells = [String]()
        var cell = ""
        var index = 0
        var trailingSeparator = false
        while index < text.count {
            let character = text[index]
            trailingSeparator = false
            if character == "\\", index + 1 < text.count {
                let next = text[index + 1]
                if next == "|" {
                    cell.append("|"); index += 2; continue
                }
                if next == "\\" {
                    cell.append(contentsOf: "\\\\"); index += 2; continue
                }
            }
            if character == "|" {
                cells.append(cell); cell = ""; trailingSeparator = true
            } else { cell.append(character) }
            index += 1
        }
        cells.append(cell)
        if text.first == "|" { cells.removeFirst() }
        if trailingSeparator { cells.removeLast() }
        return cells.map { $0.trimmingCharacters(in: .whitespaces) }
    }
    static func isTableSeparator(_ line: String) -> Bool {
        let cells = tableCells(line)
        return !cells.isEmpty && line.contains("-") && cells.allSatisfy { cell in !cell.isEmpty && cell.allSatisfy { "-:".contains($0) } }
    }
    static func parse(_ text: String) -> [NativeMarkdownBlock] {
        let lines = text.components(separatedBy: .newlines)
        var result: [NativeMarkdownBlock] = []
        var paragraph: [String] = []
        var fence: String?
        var language = ""
        var code: [String] = []
        func flush() {
            guard !paragraph.isEmpty else { return }
            result += splitImages(paragraph.joined(separator: "\n"))
            paragraph = []
        }
        var index = 0
        while index < lines.count {
            let line = lines[index]
            index += 1
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if let opening = fence {
                if trimmed.hasPrefix(opening), trimmed.dropFirst(opening.count).allSatisfy({ $0 == opening.first }) {
                    let body = code.joined(separator: "\n")
                    result.append(ReplyStyle.viewKinds.contains(language.lowercased()) ? .view(language.lowercased(), body) : .code(language, body))
                    fence = nil; code = []
                } else { code.append(line) }
                continue
            }
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                flush()
                let character = trimmed.first!
                let marker = String(trimmed.prefix(while: { $0 == character }))
                fence = marker
                language = trimmed.dropFirst(marker.count).trimmingCharacters(in: .whitespaces)
                continue
            }
            if trimmed.hasPrefix("|"), index < lines.count, isTableSeparator(lines[index]) {
                flush()
                let headers = tableCells(trimmed)
                index += 1
                var rows: [[String]] = []
                while index < lines.count, lines[index].trimmingCharacters(in: .whitespaces).hasPrefix("|") {
                    rows.append(tableCells(lines[index]))
                    index += 1
                }
                result.append(.table(headers, rows))
                continue
            }
            if trimmed.isEmpty { flush(); continue }
            let hashes = trimmed.prefix(while: { $0 == "#" }).count
            if (1...6).contains(hashes), trimmed.dropFirst(hashes).first == " " {
                flush(); result.append(.heading(hashes, String(trimmed.dropFirst(hashes + 1)))); continue
            }
            if ["---", "***", "___"].contains(trimmed) { flush(); result.append(.divider); continue }
            if trimmed.hasPrefix(">") {
                flush(); result.append(.quote(String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces))); continue
            }
            if let match = trimmed.range(of: #"^(?:[-+*]|[0-9]+[.)])\s+"#, options: .regularExpression) {
                flush()
                let marker = String(trimmed[match]).trimmingCharacters(in: .whitespaces)
                result.append(.listItem(marker.first?.isNumber == true ? marker : "•", String(trimmed[match.upperBound...])))
                continue
            }
            paragraph.append(line)
        }
        if fence != nil { result.append(.code(language, code.joined(separator: "\n"))) }
        flush()
        return result
    }

    private static func splitImages(_ text: String) -> [NativeMarkdownBlock] {
        guard let expression = try? NSRegularExpression(pattern: #"!\[([^\]]*)\]\(([^\n)]+)\)"#) else { return [.paragraph(text)] }
        let source = text as NSString
        let matches = expression.matches(in: text, range: NSRange(location: 0, length: source.length))
        var cursor = 0
        var result: [NativeMarkdownBlock] = []
        for match in matches {
            if match.range.location > cursor {
                let before = source.substring(with: NSRange(location: cursor, length: match.range.location - cursor)).trimmingCharacters(in: .whitespacesAndNewlines)
                if !before.isEmpty { result.append(.paragraph(before)) }
            }
            result.append(.image(source.substring(with: match.range(at: 1)), source.substring(with: match.range(at: 2))))
            cursor = NSMaxRange(match.range)
        }
        if cursor < source.length {
            let after = source.substring(from: cursor).trimmingCharacters(in: .whitespacesAndNewlines)
            if !after.isEmpty { result.append(.paragraph(after)) }
        }
        return result
    }
}

enum NativeAttachmentPath {
    static var directory: URL {
        let environment = ProcessInfo.processInfo.environment
        let base: URL
        if let override = environment["NEKO_DATA_DIR"], override.hasPrefix("/") { base = URL(fileURLWithPath: override) }
        else { base = URL(fileURLWithPath: (environment["HOME"] ?? "/tmp") + "/Library/Application Support/neko") }
        return base.appendingPathComponent("attachments", isDirectory: true)
    }
    static func resolve(_ reference: String, directory: URL = directory) -> URL? {
        let url: URL
        if reference.hasPrefix("/") { url = URL(fileURLWithPath: reference) }
        else {
            guard let parsed = URL(string: reference), parsed.isFileURL,
                  parsed.host == nil || parsed.host == "" || parsed.host == "localhost",
                  parsed.query == nil, parsed.fragment == nil else { return nil }
            url = parsed
        }
        let root = directory.standardizedFileURL.resolvingSymlinksInPath()
        // Resolve the containing directory separately: Foundation can leave an
        // intermediate symlink unresolved when the final file does not exist.
        let standardized = url.standardizedFileURL
        let resolved = standardized.deletingLastPathComponent().resolvingSymlinksInPath()
            .appendingPathComponent(standardized.lastPathComponent).resolvingSymlinksInPath()
        guard resolved.path.hasPrefix(root.path + "/"),
              ["png", "tiff", "tif", "jpg", "jpeg"].contains(resolved.pathExtension.lowercased()) else { return nil }
        return resolved
    }
}

struct ReadableText: View {
    let text: String
    var body: some View {
        let blocks = NativeMarkdown.parse(text)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { index, block in
                Group {
                switch block {
                case .paragraph(let value): inline(value)
                case .heading(let level, let value):
                    inline(value).font(level <= 2 ? NekoFont.title : NekoFont.heading)
                        .padding(.top, index == 0 ? 0 : 12).accessibilityAddTraits(.isHeader)
                case .code(let language, let code):
                    if language.lowercased() == "diff" || language.lowercased() == "patch" { NativeDiffBlock(code: code) }
                    else if ReplyStyle.terminalLanguages.contains(language.lowercased()) { NativeTerminalBlock(language: language, code: code) }
                    else { NativeCodeBlock(language: language, code: code) }
                case .table(let headers, let rows): NativeTableBlock(headers: headers, rows: rows)
                case .view(let kind, let source): ReplyBlockView(kind: kind, source: source)
                case .listItem(let marker, let value):
                    HStack(alignment: .top, spacing: 10) { Text(marker).foregroundStyle(.secondary).frame(minWidth: 20, alignment: .trailing); inline(value) }
                case .quote(let value):
                    HStack(alignment: .top, spacing: 14) { RoundedRectangle(cornerRadius: 2).fill(N.lineStrong).frame(width: 3); inline(value).foregroundStyle(.secondary) }
                        .padding(.vertical, 4).fixedSize(horizontal: false, vertical: true)
                case .image(let alt, let path): NativeAttachmentImage(alt: alt, reference: path)
                case .divider: Divider()
                }
                }.padding(.bottom, index == blocks.count - 1 ? 0 : blockSpacing(block))
            }
        }.frame(maxWidth: .infinity, alignment: .leading)
            .environment(\.openURL, OpenURLAction { url in
                ["http", "https", "mailto"].contains(url.scheme?.lowercased() ?? "") ? .systemAction : .discarded
            })
    }
    private func blockSpacing(_ block: NativeMarkdownBlock) -> CGFloat {
        switch block {
        case .listItem: 7
        case .heading: 10
        case .code, .table, .view, .image, .divider: 20
        default: 16
        }
    }
    private func inline(_ value: String) -> some View {
        Text((try? AttributedString(markdown: value, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(value))
            .textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct NativeCodeBlock: View {
    let language: String
    let code: String
    @State private var copied = false
    @Environment(\.ink) private var ink
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? "Code" : language).font(NekoFont.meta).foregroundStyle(.secondary)
                Spacer()
                Button(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc") {
                    NSPasteboard.general.clearContents()
                    copied = NSPasteboard.general.setString(code, forType: .string)
                }.buttonStyle(.borderless).font(NekoFont.meta).accessibilityLabel("Copy code")
            }.padding(.horizontal, NekoLayout.rowInset).padding(.vertical, 12)
            Divider()
            ScrollView(.horizontal) { Text(code).font(NekoFont.mono).lineSpacing(3).textSelection(.enabled).fixedSize(horizontal: true, vertical: false).padding(NekoLayout.rowInset) }
        }.background(ink.panel, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
    }
}

private struct NativeAttachmentImage: View {
    let alt: String
    let reference: String
    var body: some View {
        if let path = NativeAttachmentPath.resolve(reference),
           let values = try? path.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
           values.isRegularFile == true, let size = values.fileSize, size <= 32 * 1024 * 1024,
           let image = NSImage(contentsOf: path) {
            Image(nsImage: image).resizable().scaledToFit().frame(maxHeight: 420).clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous)).accessibilityLabel(alt.isEmpty ? "Attached image" : alt)
        } else {
            Label(alt.isEmpty ? "Image not loaded" : "\(alt) · image not loaded", systemImage: "photo").font(.callout).foregroundStyle(.secondary)
        }
    }
}

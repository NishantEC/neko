import AppKit
import ImageIO
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

/// Parsed markdown, remembered by text. A chat re-renders far more often
/// than its messages change, and parsing a long reply on every pass was
/// visible as stutter. Inline runs are cached too, per paragraph.
@MainActor enum MarkdownCache {
    private static var blocks: [String: [NativeMarkdownBlock]] = [:]
    private static var runs: [String: AttributedString] = [:]
    static func parsed(_ text: String) -> [NativeMarkdownBlock] {
        if let cached = blocks[text] { return cached }
        let parsed = NativeMarkdown.parse(text)
        if blocks.count >= 256 { blocks.removeAll(keepingCapacity: true) }
        blocks[text] = parsed
        return parsed
    }
    static func inline(_ text: String) -> AttributedString {
        if let cached = runs[text] { return cached }
        let parsed = (try? AttributedString(markdown: text, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(text)
        if runs.count >= 1024 { runs.removeAll(keepingCapacity: true) }
        runs[text] = parsed
        return parsed
    }
}

/// A reply's markdown. Prose (paragraphs, lists, quotes, headings) is capped
/// at the reading width; code, tables and diffs use the whole column.
struct ReadableText: View, Equatable {
    let text: String
    var body: some View {
        let blocks = MarkdownCache.parsed(text)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { index, block in
                Group {
                switch block {
                case .paragraph(let value): inline(value).chatProse()
                case .heading(let level, let value):
                    inline(value).font(Self.headingFont(level))
                        .padding(.top, index == 0 ? 0 : Self.headingSpace(level))
                        .accessibilityAddTraits(.isHeader)
                        .chatProse()
                case .code(let language, let code):
                    if language.lowercased() == "diff" || language.lowercased() == "patch" { NativeDiffBlock(code: code) }
                    else if ReplyStyle.terminalLanguages.contains(language.lowercased()) { NativeTerminalBlock(language: language, code: code) }
                    else { NativeCodeBlock(language: language, code: code) }
                case .table(let headers, let rows): NativeTableBlock(headers: headers, rows: rows)
                case .view(let kind, let source): ReplyBlockView(kind: kind, source: source)
                case .listItem(let marker, let value):
                    HStack(alignment: .firstTextBaseline, spacing: NekoSpace.xs) {
                        Text(marker).monospacedDigit().foregroundStyle(N.text3)
                            .frame(width: NekoChat.speakerHeight - NekoSpace.hair, alignment: .trailing)
                        inline(value)
                    }.chatProse()
                case .quote(let value):
                    HStack(alignment: .top, spacing: NekoSpace.md) {
                        RoundedRectangle(cornerRadius: NekoRadius.micro).fill(N.lineStrong).frame(width: NekoSpace.hair + 1)
                        inline(value).foregroundStyle(N.text2)
                    }
                    .padding(.vertical, NekoSpace.xxs).fixedSize(horizontal: false, vertical: true)
                    .chatProse()
                case .image(let alt, let path): NativeAttachmentImage(alt: alt, reference: path)
                case .divider: Divider()
                }
                }.padding(.bottom, index == blocks.count - 1 ? 0 : Self.blockSpacing(block))
            }
        }.frame(maxWidth: .infinity, alignment: .leading)
            .environment(\.openURL, OpenURLAction { url in
                ["http", "https", "mailto"].contains(url.scheme?.lowercased() ?? "") ? .systemAction : .discarded
            })
    }
    /// Three distinct levels: 22, 17, then body-size semibold.
    static func headingFont(_ level: Int) -> Font {
        switch level {
        case 1: NekoFont.title
        case 2: NekoFont.of(.subtitle, weight: .semibold)
        default: NekoFont.heading
        }
    }
    static func headingSpace(_ level: Int) -> CGFloat {
        switch level {
        case 1: NekoSpace.xl
        case 2: NekoSpace.lg
        default: NekoSpace.md
        }
    }
    static func blockSpacing(_ block: NativeMarkdownBlock) -> CGFloat {
        switch block {
        case .listItem: NekoSpace.xs
        case .heading: NekoSpace.sm
        case .code, .table, .view, .image, .divider: NekoChat.blockGap
        default: NekoSpace.md
        }
    }
    private func inline(_ value: String) -> some View {
        Text(MarkdownCache.inline(value))
            .textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct NativeCodeBlock: View {
    let language: String
    let code: String
    var body: some View {
        ChatBlock(label: language.isEmpty ? "Code" : language, fill: N.sunken) {
            ChatCopyButton(text: { code }, help: "Copy code")
        } content: {
            ScrollView(.horizontal) {
                Text(code).font(NekoChat.code).lineSpacing(NekoChat.codeLineSpacing).textSelection(.enabled)
                    .fixedSize(horizontal: true, vertical: false).padding(NekoChat.blockInset)
            }
        }
    }
}

/// Decoded chat images by path. Decoding happens off the main thread once;
/// a lazy stack bringing a row back reads the cache instead of the disk.
@MainActor enum ChatImageCache {
    private static let images: NSCache<NSString, NSImage> = {
        let cache = NSCache<NSString, NSImage>()
        cache.countLimit = 48
        return cache
    }()

    static func image(_ path: String) -> NSImage? { images.object(forKey: path as NSString) }

    static func load(_ url: URL) async -> NSImage? {
        let key = url.path
        if let cached = image(key) { return cached }
        let decoded = await Task.detached(priority: .userInitiated) { () -> NSImage? in
            guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
                  values.isRegularFile == true, let bytes = values.fileSize, bytes <= 32 * 1024 * 1024 else { return nil }
            return NSImage(contentsOf: url)
        }.value
        guard let decoded else { return nil }
        images.setObject(decoded, forKey: key as NSString)
        return decoded
    }

    /// Pixel size from the file header, known before the image decodes, so
    /// its frame is reserved. Nothing is decoded.
    nonisolated static func pixelSize(_ url: URL) -> CGSize? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? CGFloat,
              let height = properties[kCGImagePropertyPixelHeight] as? CGFloat, width > 0, height > 0 else { return nil }
        return CGSize(width: width, height: height)
    }
}

private struct NativeAttachmentImage: View {
    let alt: String
    let reference: String
    @State private var image: NSImage?
    @State private var aspect: CGFloat?
    @State private var failed = false
    var body: some View {
        let path = NativeAttachmentPath.resolve(reference)
        Group {
            if failed || path == nil {
                Label(alt.isEmpty ? "Image not loaded" : "\(alt) · image not loaded", systemImage: "photo").font(NekoFont.meta).foregroundStyle(N.text3)
            } else if let shown = image ?? path.flatMap({ ChatImageCache.image($0.path) }) {
                Image(nsImage: shown).resizable().scaledToFit()
                    .frame(maxHeight: NekoChat.imageHeight, alignment: .leading)
                    .clipShape(RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous))
                    .accessibilityLabel(alt.isEmpty ? "Attached image" : alt)
            } else {
                // The frame is reserved before the image decodes, so its
                // arrival does not shift the conversation.
                RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous).fill(N.sunken)
                    .aspectRatio(aspect ?? 4 / 3, contentMode: .fit)
                    .frame(maxHeight: NekoChat.imageHeight, alignment: .leading)
                    .accessibilityLabel(alt.isEmpty ? "Loading image" : "Loading \(alt)")
            }
        }
        .task(id: reference) {
            guard let path else { return }
            if aspect == nil {
                let size = await Task.detached { ChatImageCache.pixelSize(path) }.value
                if let size { aspect = size.width / size.height }
            }
            image = await ChatImageCache.load(path)
            failed = image == nil
        }
    }
}

import AppKit
import SwiftUI

enum NativeMarkdownBlock: Equatable {
    case paragraph(String), heading(Int, String), code(String, String), listItem(String, String), quote(String), image(String, String), divider
}

enum NativeMarkdown {
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
        for line in lines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if let opening = fence {
                if trimmed.hasPrefix(opening), trimmed.dropFirst(opening.count).allSatisfy({ $0 == opening.first }) {
                    result.append(.code(language, code.joined(separator: "\n")))
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
        VStack(alignment: .leading, spacing: 12) {
            ForEach(Array(NativeMarkdown.parse(text).enumerated()), id: \.offset) { _, block in
                switch block {
                case .paragraph(let value): inline(value)
                case .heading(let level, let value):
                    inline(value).font(level == 1 ? .title2.bold() : level == 2 ? .title3.bold() : .headline).accessibilityAddTraits(.isHeader)
                case .code(let language, let code): NativeCodeBlock(language: language, code: code)
                case .listItem(let marker, let value):
                    HStack(alignment: .top, spacing: 8) { Text(marker).frame(minWidth: 16, alignment: .trailing); inline(value) }
                case .quote(let value):
                    HStack(alignment: .top, spacing: 10) { RoundedRectangle(cornerRadius: 2).fill(.secondary.opacity(0.4)).frame(width: 3); inline(value).foregroundStyle(.secondary) }.fixedSize(horizontal: false, vertical: true)
                case .image(let alt, let path): NativeAttachmentImage(alt: alt, reference: path)
                case .divider: Divider()
                }
            }
        }.frame(maxWidth: .infinity, alignment: .leading)
            .environment(\.openURL, OpenURLAction { url in
                ["http", "https", "mailto"].contains(url.scheme?.lowercased() ?? "") ? .systemAction : .discarded
            })
    }
    private func inline(_ value: String) -> some View {
        Text((try? AttributedString(markdown: value, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(value)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct NativeCodeBlock: View {
    let language: String
    let code: String
    @State private var copied = false
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(language.isEmpty ? "Code" : language).font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc") {
                    NSPasteboard.general.clearContents()
                    copied = NSPasteboard.general.setString(code, forType: .string)
                }.buttonStyle(.borderless).accessibilityLabel("Copy code")
            }
            ScrollView(.horizontal) { Text(code).font(.system(.callout, design: .monospaced)).textSelection(.enabled).fixedSize(horizontal: true, vertical: false) }
        }.padding(12).background(.quaternary, in: RoundedRectangle(cornerRadius: 10))
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
            Image(nsImage: image).resizable().scaledToFit().frame(maxHeight: 420).clipShape(RoundedRectangle(cornerRadius: 10)).accessibilityLabel(alt.isEmpty ? "Attached image" : alt)
        } else {
            Label(alt.isEmpty ? "Image not loaded" : "\(alt) · image not loaded", systemImage: "photo").font(.callout).foregroundStyle(.secondary)
        }
    }
}

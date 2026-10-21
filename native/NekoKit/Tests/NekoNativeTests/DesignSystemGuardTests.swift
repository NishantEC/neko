import XCTest

/// Keeps docs/design/principles.md §2 true: every visual value comes from a
/// token in `Sources/NekoNative/DesignSystem/`. These tests read the source
/// files, so a screen that hard-codes a radius, font size, colour, curve or
/// button style fails here before it can drift from the rest of the app.
final class DesignSystemGuardTests: XCTestCase {
    /// Folders that own visual values. `Activity/` is the presence-glyph
    /// family; its hues are defined once in `NekoActivity.color`.
    private static let owners = ["DesignSystem/", "Activity/"]

    /// Raw padding and stack-spacing numbers outside the design system. This
    /// may only go down: lower it when a change removes some, never raise it.
    private static let spacingLiteralCeiling = 289

    private static var sourceRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/NekoNative")
    }

    /// Every screen file, as (path relative to Sources/NekoNative, lines).
    private static func screens() throws -> [(path: String, lines: [String])] {
        let root = sourceRoot
        guard let walker = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil) else { return [] }
        var files: [(String, [String])] = []
        for case let url as URL in walker where url.pathExtension == "swift" {
            let path = String(url.path.dropFirst(root.path.count + 1))
            if owners.contains(where: path.hasPrefix) { continue }
            files.append((path, try String(contentsOf: url, encoding: .utf8).components(separatedBy: "\n")))
        }
        return files.sorted { $0.0 < $1.0 }
    }

    /// Lines in screens matching `pattern`, as "path:line: text".
    private func offenders(_ pattern: String) throws -> [String] {
        let regex = try NSRegularExpression(pattern: pattern)
        return try Self.screens().flatMap { file in
            file.lines.enumerated().compactMap { index, line in
                let range = NSRange(line.startIndex..., in: line)
                return regex.firstMatch(in: line, range: range) == nil ? nil : "\(file.path):\(index + 1): \(line.trimmingCharacters(in: .whitespaces))"
            }
        }
    }

    private func assertNone(_ pattern: String, _ rule: String, file: StaticString = #filePath, line: UInt = #line) throws {
        let found = try offenders(pattern)
        XCTAssert(found.isEmpty, "\(rule)\n" + found.prefix(20).joined(separator: "\n"), file: file, line: line)
    }

    func testTheSourceTreeIsFound() throws {
        XCTAssertGreaterThan(try Self.screens().count, 40, "Guard tests are not reading the sources at \(Self.sourceRoot.path)")
    }

    func testCornerRadiiComeFromNekoRadius() throws {
        try assertNone(#"cornerRadius: *[0-9]"#, "Use a NekoRadius step, not a number.")
        try assertNone(#"(liquidGlass|nekoCard|nekoFill)\([^)]*radius: *[0-9]"#, "Use a NekoRadius step, not a number.")
    }

    func testTypeComesFromNekoFont() throws {
        try assertNone(#"\.system\(size:"#, "Use a NekoFont role or NekoFont.of(_:), not .system(size:).")
    }

    func testColoursComeFromTokens() throws {
        try assertNone(#"Color\.adaptive\(|\.oklch\(|Color\(red:|NSColor\(srgbRed|NSColor\(red:|#colorLiteral"#,
                       "Define colours in DesignSystem/Tokens.swift (N, NekoStyle) and use the token.")
    }

    func testAnimationsGoThroughNekoMotion() throws {
        try assertNone(#"withAnimation\("#, "Use withNekoAnimation(_:), which honours Reduce Motion.")
        try assertNone(#"\.animation\((\.|reduceMotion)"#, "Use .nekoAnimation(_:value:), which honours Reduce Motion.")
    }

    /// The chat surfaces (Home, tickets, composer, replies) take every type
    /// role from NekoFont. Other screens still use a few system roles.
    func testChatTypeComesFromNekoFont() throws {
        let chat = ["Chat", "Composer", "Reply", "Ticket", "Today", "SharedComposer", "SlashCommands", "RichText"]
        let regex = try NSRegularExpression(pattern: #"\.font\(\.(caption2?|headline|callout|subheadline|footnote|body|title\d?)\)"#)
        let found = try Self.screens().filter { file in chat.contains { file.path.hasPrefix($0) } }.flatMap { file in
            file.lines.enumerated().compactMap { index, line in
                regex.firstMatch(in: line, range: NSRange(line.startIndex..., in: line)) == nil ? nil : "\(file.path):\(index + 1)"
            }
        }
        XCTAssert(found.isEmpty, "Use a NekoFont role, not a system text style.\n" + found.joined(separator: "\n"))
    }

    func testButtonStylesComeFromNekoButton() throws {
        try assertNone(#"\.buttonStyle\(\.(bordered|borderedProminent|borderless)\)"#, "Use .nekoButton(.primary/.secondary/.quiet/.destructive).")
    }

    func testNothingScalesOnHover() throws {
        try assertNone(#"scaleEffect\(\s*hover"#, "Hover must not move the layout under the pointer (principles §6).")
    }

    func testRawSpacingOnlyShrinks() throws {
        let count = try offenders(#"(\.padding\((\.[a-zA-Z]+, *)?|spacing: *)[0-9]"#).count
        XCTAssertLessThanOrEqual(count, Self.spacingLiteralCeiling,
            "New raw padding/spacing numbers: use NekoSpace or NekoLayout. (\(count) > \(Self.spacingLiteralCeiling))")
        if count < Self.spacingLiteralCeiling {
            print("DesignSystemGuardTests: raw spacing fell to \(count); lower spacingLiteralCeiling to lock it in.")
        }
    }
}

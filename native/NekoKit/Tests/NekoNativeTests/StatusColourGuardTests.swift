import XCTest

/// Keeps docs/design/principles.md §5 true for status colour: screens use
/// `NekoStyle`'s tuned light/dark pairs, never system hues (system green fails
/// 4.5:1 on a light card), and never an ad-hoc `.shadow(radius:)`.
/// Kept apart from DesignSystemGuardTests so the two can move independently.
final class StatusColourGuardTests: XCTestCase {
    private static let owners = ["DesignSystem/", "Activity/"]
    /// Files outside the management redesign that still use a system hue.
    /// Remove an entry when its file is fixed; never add one.
    private static let pending: Set<String> = ["TicketInspector.swift", "DesignLabPluginLibrary.swift"]

    private static var sourceRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/NekoNative")
    }

    private func offenders(_ pattern: String) throws -> [String] {
        let regex = try NSRegularExpression(pattern: pattern)
        let root = Self.sourceRoot
        guard let walker = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil) else { return [] }
        var found: [String] = []
        for case let url as URL in walker where url.pathExtension == "swift" {
            let path = String(url.path.dropFirst(root.path.count + 1))
            if Self.owners.contains(where: path.hasPrefix) || Self.pending.contains(path) { continue }
            let lines = try String(contentsOf: url, encoding: .utf8).components(separatedBy: "\n")
            for (index, line) in lines.enumerated() where regex.firstMatch(in: line, range: NSRange(line.startIndex..., in: line)) != nil {
                found.append("\(path):\(index + 1): \(line.trimmingCharacters(in: .whitespaces))")
            }
        }
        return found.sorted()
    }

    func testTheSourceTreeIsFound() throws {
        XCTAssertTrue(FileManager.default.fileExists(atPath: Self.sourceRoot.appendingPathComponent("TicketsView.swift").path))
    }

    func testStatusColoursComeFromNekoStyle() throws {
        let found = try offenders(#"(foregroundStyle|tint|fill)\(\s*(Color)?\.(red|green|orange|yellow|blue|purple)\b|Color\.(red|green|orange|yellow)\b"#)
        XCTAssert(found.isEmpty, "Use NekoStyle.mint/amber/coral/sky, not a system hue.\n" + found.joined(separator: "\n"))
    }

    func testNoAdHocShadows() throws {
        let found = try offenders(#"\.shadow\(radius: *[0-9]"#)
        XCTAssert(found.isEmpty, "Depth comes from the design system's surfaces, not a raw shadow.\n" + found.joined(separator: "\n"))
    }
}

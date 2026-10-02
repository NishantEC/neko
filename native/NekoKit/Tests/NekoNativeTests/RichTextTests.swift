import XCTest
@testable import NekoNative

final class RichTextTests: XCTestCase {
    func testMarkdownBlocksPreserveCodeAndBasicStructure() {
        XCTAssertEqual(NativeMarkdown.parse("# Heading\n\nParagraph **bold**\n\n- One\n2. Two\n> Quote\n```swift\nlet x = 1\n![No image](file:///tmp/a.png)\n```"), [
            .heading(1, "Heading"), .paragraph("Paragraph **bold**"), .listItem("•", "One"), .listItem("2.", "Two"), .quote("Quote"), .code("swift", "let x = 1\n![No image](file:///tmp/a.png)")
        ])
        XCTAssertEqual(NativeMarkdown.parse("```\nunclosed"), [.code("", "unclosed")])
    }
    func testTablesAndReplyViewsBecomeTheirOwnBlocks() {
        XCTAssertEqual(NativeMarkdown.parse("Rates:\n| City | Δ |\n|---|--:|\n| Pune | −8.1 |\n| Delhi | +0.5 |\nDone."), [
            .paragraph("Rates:"), .table(["City", "Δ"], [["Pune", "−8.1"], ["Delhi", "+0.5"]]), .paragraph("Done.")
        ])
        // A pipe line without a separator row stays prose.
        XCTAssertEqual(NativeMarkdown.parse("| not a table |"), [.paragraph("| not a table |")])
        XCTAssertEqual(NativeMarkdown.parse("```neko-chart\n{\"bars\":[]}\n```\n```diff\n+a\n```"), [
            .view("neko-chart", "{\"bars\":[]}"), .code("diff", "+a")
        ])
    }
    func testInlineImageReferencesSplitWithoutLosingSurroundingText() {
        XCTAssertEqual(NativeMarkdown.parse("Look ![Image](file:///tmp/a path.png) now"), [.paragraph("Look"), .image("Image", "file:///tmp/a path.png"), .paragraph("now")])
        XCTAssertEqual(NativeMarkdown.parse("![Remote](https://example.com/image.png)"), [.image("Remote", "https://example.com/image.png")])
    }
    func testAttachmentPathRejectsRemoteTraversalSiblingsAndSymlinks() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("neko-richtext-" + UUID().uuidString)
        let directory = root.appendingPathComponent("attachments")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let image = directory.appendingPathComponent("image.png")
        XCTAssertEqual(NativeAttachmentPath.resolve(image.absoluteString, directory: directory), image.resolvingSymlinksInPath())
        XCTAssertNil(NativeAttachmentPath.resolve("https://example.com/image.png", directory: directory))
        XCTAssertNil(NativeAttachmentPath.resolve(root.appendingPathComponent("private.png").absoluteString, directory: directory))
        XCTAssertNil(NativeAttachmentPath.resolve(directory.path + "/../private.png", directory: directory))
        XCTAssertNil(NativeAttachmentPath.resolve(directory.path + "-other/image.png", directory: directory))
        XCTAssertNil(NativeAttachmentPath.resolve(directory.path + "/script.command", directory: directory))
        let outside = root.appendingPathComponent("outside")
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: directory.appendingPathComponent("escape"), withDestinationURL: outside)
        XCTAssertNil(NativeAttachmentPath.resolve(directory.appendingPathComponent("escape/image.png").absoluteString, directory: directory))
    }
}

import AppKit
import XCTest
@testable import NekoNative

@MainActor final class ComposerSizingTests: XCTestCase {
    func testSizingProbesDoNotResizeTheLiveEditor() throws {
        let scroll = ComposerScrollView(frame: NSRect(x: 0, y: 0, width: 720, height: 80))
        let text = NSTextView(frame: scroll.bounds)
        text.font = .systemFont(ofSize: 15)
        text.string = "Typing should use all the available space without moving the caret."
        scroll.documentView = text
        text.setSelectedRange(NSRange(location: 9, length: 4))
        let frame = text.frame
        let width = text.textContainer?.containerSize.width
        // SwiftUI probes minimum and ideal sizes before choosing a final size.
        _ = scroll.measuredSize(for: 100)
        _ = scroll.measuredSize(for: 350)
        XCTAssertEqual(text.frame, frame)
        XCTAssertEqual(text.textContainer?.containerSize.width, width)
        XCTAssertEqual(text.selectedRange(), NSRange(location: 9, length: 4))
    }

    func testActualLayoutUsesViewportWidthAndScrollsLongText() throws {
        let scroll = ComposerScrollView(frame: NSRect(x: 0, y: 0, width: 720, height: 160))
        let text = NSTextView(frame: NSRect(x: 0, y: 0, width: 100, height: 40))
        text.isVerticallyResizable = true
        text.isHorizontallyResizable = false
        text.textContainer?.widthTracksTextView = true
        text.textContainer?.lineFragmentPadding = 0
        text.textContainerInset = NSSize(width: 0, height: 6)
        text.font = .systemFont(ofSize: 15)
        text.string = String(repeating: "A multiline draft keeps its caret and scrolls normally.\n", count: 20)
        scroll.documentView = text
        XCTAssertEqual(scroll.measuredSize(for: 720)?.height, 160)
        scroll.layout()
        XCTAssertEqual(text.frame.width, scroll.contentSize.width)
        XCTAssertEqual(text.textContainer?.containerSize.width, scroll.contentSize.width)
        XCTAssertGreaterThan(text.frame.height, scroll.contentSize.height)
        scroll.setFrameSize(NSSize(width: 350, height: 160))
        scroll.layout()
        XCTAssertEqual(text.frame.width, scroll.contentSize.width)
        XCTAssertEqual(text.textContainer?.containerSize.width, scroll.contentSize.width)
    }

    func testMeasurementPreservesMarkedTextAndCountsTrailingNewline() throws {
        let scroll = ComposerScrollView(frame: NSRect(x: 0, y: 0, width: 720, height: 80))
        let text = NSTextView(frame: scroll.bounds)
        text.font = .systemFont(ofSize: 15)
        scroll.documentView = text
        text.string = "First\nSecond\nThird"
        let before = try XCTUnwrap(scroll.measuredSize(for: 720)).height
        text.string += "\n"
        XCTAssertGreaterThan(try XCTUnwrap(scroll.measuredSize(for: 720)).height, before)
        text.setMarkedText("かな", selectedRange: NSRange(location: 1, length: 0), replacementRange: NSRange(location: 0, length: 0))
        let marked = text.markedRange()
        let selection = text.selectedRange()
        let contents = text.string
        _ = scroll.measuredSize(for: 100)
        XCTAssertTrue(text.hasMarkedText())
        XCTAssertEqual(text.markedRange(), marked)
        XCTAssertEqual(text.selectedRange(), selection)
        XCTAssertEqual(text.string, contents)
    }
}

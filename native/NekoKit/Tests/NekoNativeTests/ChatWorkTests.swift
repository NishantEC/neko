import XCTest
import NekoKit
@testable import NekoNative

/// The chat's work log and live reply: the summary sentence, durations, fold
/// state, streaming Markdown splits and the sent-message runway.
@MainActor final class ChatWorkTests: XCTestCase {
    // MARK: Summary sentence

    func testNoWorkHasNoSentence() {
        XCTAssertNil(ChatWorkSummary(activity: []).sentence)
        XCTAssertFalse(ChatWorkSummary(activity: ["  "]).hasWork)
    }

    func testCommandsAndFailuresRead() {
        let summary = ChatWorkSummary(activity: ["Ran `git status`", "Ran `cargo test` · failed", "Ran `ls`"])
        XCTAssertEqual(summary.sentence, "Ran 3 commands · 1 failed")
        XCTAssertEqual(summary.steps.map(\.failed), [false, true, false])
        XCTAssertEqual(summary.steps[1].text, "Ran `cargo test`", "The failure is shown by the row, not repeated in its words")
    }

    func testSingularAndPriorityOrder() {
        let summary = ChatWorkSummary(activity: ["Read `a.swift`", "Edited `b.swift`", "Ran `make`"])
        XCTAssertEqual(summary.sentence, "Ran 1 command · made 1 edit · read 1 file")
    }

    func testToolCallsJoinTheSentence() {
        let summary = ChatWorkSummary(activity: ["Ran `ls`"], toolCount: 2, toolFailed: 1)
        XCTAssertEqual(summary.sentence, "Ran 1 command · used 2 tools · 1 failed")
    }

    func testLongSentencesAreCapped() {
        let summary = ChatWorkSummary(activity: ["Ran `a`", "Edited `b`", "Read `c`", "Searched `d`", "Did something"])
        XCTAssertEqual(summary.sentence, "Ran 1 command · made 1 edit · read 1 file · +2 more")
    }

    func testKindsFromVerbs() {
        XCTAssertEqual(ChatWorkSummary.kind(of: "Searched the code"), .search)
        XCTAssertEqual(ChatWorkSummary.kind(of: "Wrote `x`"), .edit)
        XCTAssertEqual(ChatWorkSummary.kind(of: "Pondered"), .other)
    }

    // MARK: Duration

    func testDurationText() {
        XCTAssertEqual(ChatDuration.text(seconds: 0), "0s")
        XCTAssertEqual(ChatDuration.text(seconds: 59), "59s")
        XCTAssertEqual(ChatDuration.text(seconds: 72), "1m 12s")
        XCTAssertEqual(ChatDuration.text(seconds: 3900), "1h 5m")
        XCTAssertEqual(ChatDuration.text(seconds: -4), "0s")
    }

    func testWorkedForNeedsAKnownEndAndEnoughTime() {
        XCTAssertEqual(ChatDuration.workedFor(startMs: 1_000_000, endMs: 1_072_000), "Worked for 1m 12s")
        XCTAssertNil(ChatDuration.workedFor(startMs: 1_000_000, endMs: nil), "No completion time, no line")
        XCTAssertNil(ChatDuration.workedFor(startMs: 0, endMs: 5_000))
        XCTAssertNil(ChatDuration.workedFor(startMs: 1_000_000, endMs: 1_001_000), "A one second turn is not worth a line")
        XCTAssertNil(ChatDuration.workedFor(startMs: 2_000_000, endMs: 1_000_000))
    }

    func testCompletionTimeComesFromTheMessage() {
        XCTAssertEqual(ChatDuration.completion(of: .object(["done_ms": .number(5000)])), 5000)
        XCTAssertNil(ChatDuration.completion(of: .object(["at_ms": .number(5000)])))
    }

    // MARK: Live label and fold state

    func testLiveLabelShowsTheNewestStep() {
        XCTAssertEqual(ChatWorkLive.label(steps: ["Ran `ls`", "Ran `git status`"], fallback: "Working…", keepFallback: false), "Ran git status")
        XCTAssertEqual(ChatWorkLive.label(steps: [], fallback: "Thinking…", keepFallback: false), "Thinking…")
        XCTAssertEqual(ChatWorkLive.label(steps: ["Ran `ls`"], fallback: "Waiting for your OK", keepFallback: true), "Waiting for your OK")
    }

    func testFoldStaysAsTheReaderLeftIt() {
        let ledger = ChatWorkLedger()
        XCTAssertFalse(ledger.isOpen("a"))
        XCTAssertTrue(ledger.toggle("a"))
        XCTAssertTrue(ledger.isOpen("a"), "A recycled row reads its choice back")
        XCTAssertFalse(ledger.isOpen("b"))
        XCTAssertFalse(ledger.toggle("a"))
        XCTAssertFalse(ledger.isOpen("a"))
        XCTAssertTrue(ChatWorkFoldPolicy.isOpen(userOpened: true, canExpand: true))
        XCTAssertFalse(ChatWorkFoldPolicy.isOpen(userOpened: true, canExpand: false), "Nothing to show, nothing to open")
        XCTAssertFalse(ChatWorkFoldPolicy.isOpen(userOpened: false, canExpand: true))
    }

    // MARK: Streaming Markdown

    func testSplitKeepsFinishedBlocksApartFromTheTail() {
        let parts = StreamingMarkdown.split("First para.\n\nSecond para.\n\nThird being wri")
        XCTAssertEqual(parts.stable, "First para.\n\nSecond para.")
        XCTAssertEqual(parts.tail, "Third being wri")
        XCTAssertEqual(StreamingMarkdown.split("one line so far").stable, "")
        XCTAssertEqual(StreamingMarkdown.split("a\n\n").tail, "")
    }

    func testSplitNeverCutsInsideAFence() {
        let text = "Intro.\n\n```swift\nlet a = 1\n\nlet b = 2\n"
        let parts = StreamingMarkdown.split(text)
        XCTAssertEqual(parts.stable, "Intro.")
        XCTAssertEqual(parts.tail, "```swift\nlet a = 1\n\nlet b = 2\n", "An unclosed fence stays whole in the tail")
        let closed = StreamingMarkdown.split(text + "```\n\nAfter")
        XCTAssertEqual(closed.tail, "After")
        XCTAssertTrue(closed.stable.hasSuffix("```"))
    }

    func testSplitRoundTripsToTheSameBlocksAsOneParse() {
        let text = "# Title\n\nSome **bold** text.\n\n- a\n- b\n\n```neko-chart\n{\"bars\":\n\nunfinished"
        let parts = StreamingMarkdown.split(text)
        let split = NativeMarkdown.parse(parts.stable) + NativeMarkdown.parse(parts.tail)
        XCTAssertEqual(split, NativeMarkdown.parse(text), "Streaming must draw what the finished reply draws")
    }

    func testMendingClosesOpenSpansButNotFences() {
        XCTAssertEqual(StreamingMarkdown.mended("a **bo"), "a **bo**")
        XCTAssertEqual(StreamingMarkdown.mended("run `git"), "run `git`")
        XCTAssertEqual(StreamingMarkdown.mended("fine **bold** and `code`"), "fine **bold** and `code`")
        let fenced = "```swift\nlet a = \"`\" **"
        XCTAssertEqual(StreamingMarkdown.mended(fenced), fenced, "Inside a fence nothing is Markdown")
    }

    func testReplacedTextIsFreshNotAFadeOfTheOldText() {
        XCTAssertEqual(StreamingMarkdown.settled(old: "Hello", new: "Hello wor"), "Hello")
        XCTAssertEqual(StreamingMarkdown.settled(old: "Hello", new: "Different"), "", "A replacement fades in whole")
        XCTAssertEqual(StreamingMarkdown.settled(old: "Longer text", new: "Longer"), "", "A shrinking text is a replacement")
        XCTAssertEqual(StreamingMarkdown.settled(old: "", new: "Hi"), "")
    }

    func testUnclosedViewFenceParsesWithoutCrashing() {
        for partial in ["```neko-chart\n{\"title\":\"x\",\"bars\":[{\"la", "```neko-choices\n", "~~~\nopen", "```"] {
            let parts = StreamingMarkdown.split(partial)
            _ = NativeMarkdown.parse(StreamingMarkdown.mended(parts.tail))
        }
    }

    // MARK: Sent message anchor

    func testRunwayShrinksAsTheReplyGrowsAndEndsWhenItFillsTheScreen() {
        func extra(_ exchange: CGFloat) -> CGFloat { ChatRunway.extra(viewport: 800, exchange: exchange, clearance: 150, topMargin: 0) }
        XCTAssertEqual(extra(100), 550)
        XCTAssertEqual(extra(400), 250)
        XCTAssertEqual(extra(650), 0)
        XCTAssertEqual(extra(2000), 0)
        XCTAssertEqual(ChatRunway.extra(viewport: 0, exchange: 100, clearance: 150, topMargin: 0), 0, "Nothing is measured yet")
    }

    func testSendingAnchorsUntilTheReplyFillsTheScreen() {
        let tracker = ChatScrollTracker()
        let now = Date()
        tracker.observe(ChatScrollMetrics(contentHeight: 1000, originY: -500, viewportHeight: 500), now: now)
        tracker.anchor("sent", now: now)
        XCTAssertEqual(tracker.actionAfterResize(dockHeight: 120, anchorHolds: true, now: now), .anchor("sent"))
        XCTAssertEqual(tracker.actionAfterResize(dockHeight: 120, anchorHolds: true, now: now.addingTimeInterval(0.5)), .anchor("sent"))
        // The reply has filled the room: back to following the newest line.
        XCTAssertEqual(tracker.actionAfterResize(dockHeight: 120, anchorHolds: false, now: now.addingTimeInterval(0.6)), .latest)
        XCTAssertNil(tracker.actionAfterResize(dockHeight: 120, anchorHolds: true, now: now.addingTimeInterval(0.7)).flatMap {
            if case .anchor = $0 { return $0 } else { return nil }
        }, "Once released, the anchor does not come back")
    }

    func testTheAnchorExpiresAndTheReaderCanBreakIt() {
        let tracker = ChatScrollTracker()
        let now = Date()
        tracker.anchor("sent", now: now)
        XCTAssertEqual(tracker.actionAfterResize(dockHeight: 120, anchorHolds: true, now: now.addingTimeInterval(5)), .latest)
        tracker.anchor("again", now: now)
        tracker.userScrolledBack()
        XCTAssertNil(tracker.actionAfterResize(dockHeight: 120, anchorHolds: true, now: now), "Scrolling away ends the hold")
    }
}

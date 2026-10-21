import AppKit
import XCTest
import NekoKit
@testable import NekoNative

/// The chat's follow-scroll model: what may change while the reader scrolls,
/// and how many scrolls a burst of changes produces.
@MainActor final class ChatTranscriptTests: XCTestCase {
    private func metrics(content: CGFloat, origin: CGFloat, viewport: CGFloat = 500) -> ChatScrollMetrics {
        ChatScrollMetrics(contentHeight: content, originY: origin, viewportHeight: viewport)
    }

    func testScrollingFramesChangeNothingTheViewDraws() {
        let tracker = ChatScrollTracker()
        tracker.observe(metrics(content: 2000, origin: -1500))
        let before = tracker.visible
        // A pure offset change, frame after frame, near the bottom.
        for step in 1...30 {
            let resized = tracker.observe(metrics(content: 2000, origin: -1500 + CGFloat(step)))
            XCTAssertFalse(resized, "An offset change is not a content change")
            XCTAssertEqual(tracker.visible, before, "Scrolling must not write view state every frame")
        }
    }

    func testJumpAppearsOnlyPastHalfAViewportFromTheBottom() {
        let tracker = ChatScrollTracker()
        tracker.observe(metrics(content: 2000, origin: -1500))
        tracker.observe(metrics(content: 2000, origin: -1350))
        XCTAssertFalse(tracker.follow.followsLatest)
        XCTAssertFalse(tracker.visible.showsJump, "150pt from the bottom is still reading the latest")
        tracker.observe(metrics(content: 2000, origin: -1000))
        XCTAssertTrue(tracker.visible.showsJump)
        tracker.pin()
        tracker.observe(metrics(content: 2000, origin: -1500))
        XCTAssertFalse(tracker.visible.showsJump)
    }

    func testScrollRequestsInOneRunloopCollapseIntoOneScroll() {
        let tracker = ChatScrollTracker()
        var scheduled: [@MainActor () -> Void] = []
        var performed: [ChatScrollAction] = []
        for action in [ChatScrollAction.latest, .latest, .top("a"), .latest] {
            tracker.request(action, schedule: { scheduled.append($0) }, perform: { performed.append($0) })
        }
        XCTAssertEqual(scheduled.count, 1)
        scheduled.forEach { $0() }
        XCTAssertEqual(performed, [.latest], "The last request wins")
        tracker.request(.top("b"), schedule: { scheduled.append($0) }, perform: { performed.append($0) })
        XCTAssertEqual(scheduled.count, 2, "A new runloop schedules again")
    }

    func testTallFinishedReplyLandsOnItsFirstLine() {
        let tracker = ChatScrollTracker()
        tracker.observe(metrics(content: 1000, origin: -500))
        tracker.replyFinished("reply")
        tracker.observe(metrics(content: 1900, origin: -500))
        XCTAssertEqual(tracker.actionAfterResize(dockHeight: 120), .top("reply"))
        XCTAssertFalse(tracker.follow.followsLatest, "Reading a long reply from its top is not following")
        XCTAssertNil(tracker.actionAfterResize(dockHeight: 120), "Later growth leaves the reader where they are")
    }

    func testShortFinishedReplyKeepsFollowingTheBottom() {
        let tracker = ChatScrollTracker()
        tracker.observe(metrics(content: 1000, origin: -500))
        tracker.replyFinished("reply")
        tracker.observe(metrics(content: 1120, origin: -500))
        XCTAssertEqual(tracker.actionAfterResize(dockHeight: 120), .latest)
        XCTAssertTrue(tracker.follow.followsLatest)
    }

    func testReplyFinishingWhileReadingHistoryOffersNewReply() {
        let tracker = ChatScrollTracker()
        tracker.observe(metrics(content: 3000, origin: -2500))
        tracker.observe(metrics(content: 3000, origin: -400))
        XCTAssertFalse(tracker.follow.followsLatest)
        tracker.replyFinished("reply")
        XCTAssertTrue(tracker.visible.newReply)
        tracker.observe(metrics(content: 3400, origin: -400))
        XCTAssertNil(tracker.actionAfterResize(dockHeight: 120), "Never pull a reader out of history")
        tracker.pin()
        XCTAssertFalse(tracker.visible.newReply, "Jumping to latest clears the notice")
    }

    func testOnlyScrollingTowardOlderContentCountsAsLeaving() {
        XCTAssertTrue(ChatScrollIntent.isScrollingBack(deltaY: 4, phase: .changed))
        XCTAssertTrue(ChatScrollIntent.isScrollingBack(deltaY: 4, phase: []), "A wheel mouse has no phase")
        XCTAssertFalse(ChatScrollIntent.isScrollingBack(deltaY: -4, phase: .changed), "Scrolling toward the latest is not leaving")
        XCTAssertFalse(ChatScrollIntent.isScrollingBack(deltaY: 4, phase: .ended))
        XCTAssertFalse(ChatScrollIntent.isScrollingBack(deltaY: 0, phase: .began))
    }

    func testTurnRhythmGroupsAQuestionWithItsAnswer() {
        XCTAssertEqual(ChatRhythm.gap(isUser: true, isFirst: true), 0)
        XCTAssertEqual(ChatRhythm.gap(isUser: true, isFirst: false), NekoChat.turnGap)
        XCTAssertEqual(ChatRhythm.gap(isUser: false, isFirst: false), NekoChat.replyGap)
        XCTAssertGreaterThanOrEqual(NekoChat.turnGap, NekoChat.replyGap * 2)
    }

    func testFadeLedgerFadesAFreshTurnOnce() {
        let ledger = ChatFadeLedger()
        XCTAssertFalse(ledger.shouldFade("old", fresh: false))
        XCTAssertTrue(ledger.shouldFade("new", fresh: true))
        ledger.record("new")
        XCTAssertFalse(ledger.shouldFade("new", fresh: true), "A lazy stack re-creating the row must not fade it again")
    }

    func testTicketRowsKeepTheirIdentityWhenAnEchoStartsBeingFiltered() throws {
        func task(result: String) throws -> JSONValue {
            try JSONDecoder().decode(JSONValue.self, from: Data("""
            {"goal":"Fix it","result":"\(result)","events":[
              {"role":"builder","message":"Done: guarded the call."},
              {"role":"builder","message":"VERIFICATION_COMMAND {\\"command\\":\\"swift test\\",\\"exit_code\\":0}"},
              {"role":"builder","message":"I also tidied the tests."},
              {"role":"note","message":"Thanks"}
            ]}
            """.utf8))
        }
        let before = TicketThread.rows(try task(result: ""))
        let after = TicketThread.rows(try task(result: "Done: guarded the call."))
        XCTAssertEqual(before.count, after.count + 1, "The result's echo drops out once the result arrives")
        let ids = Set(before.map(\.id))
        for row in after { XCTAssertTrue(ids.contains(row.id), "\(row.id) must keep the id it had before") }
        XCTAssertEqual(after.first { if case .steps = $0.item { true } else { false } }?.id,
                       before.first { if case .steps = $0.item { true } else { false } }?.id)
    }

    func testTicketSpeakerShowsOnlyWhenTheSpeakerChanges() throws {
        let task = try JSONDecoder().decode(JSONValue.self, from: Data("""
        {"goal":"Fix it","events":[
          {"role":"builder","message":"Looking at it."},
          {"role":"builder","message":"VERIFICATION_COMMAND {\\"command\\":\\"rg foo\\",\\"exit_code\\":0}"},
          {"role":"builder","message":"Found it."},
          {"role":"reviewer","message":"Checked."}
        ]}
        """.utf8))
        XCTAssertEqual(TicketThread.rows(task).map(\.showsSpeaker), [true, true, false, false, true])
        XCTAssertEqual(TicketThread.symbol("builder"), "hammer")
    }
}

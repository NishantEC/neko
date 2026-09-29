import XCTest
@testable import NekoNative

final class ChatDraftTests: XCTestCase {
    func testDraftsAreSeparateByWorkspaceAndProfile() {
        var drafts = ScopedChatDrafts()
        let first = ChatDraftScope(workspaceID: "one", profileID: "default")
        let second = ChatDraftScope(workspaceID: "two", profileID: "default")
        let profile = ChatDraftScope(workspaceID: "one", profileID: "other")
        drafts.set("First", for: first)
        drafts.set("Second", for: second)
        XCTAssertEqual(drafts.text(for: first), "First")
        XCTAssertEqual(drafts.text(for: second), "Second")
        XCTAssertEqual(drafts.text(for: profile), "")
        let submission = drafts.submission(for: first)
        drafts.complete(submission, succeeded: true)
        XCTAssertEqual(drafts.text(for: first), "")
        XCTAssertEqual(drafts.text(for: second), "Second")
    }

    func testCompletionPreservesNewEditsEvenWhenTextReturnsToOriginal() {
        var drafts = ScopedChatDrafts()
        let scope = ChatDraftScope(workspaceID: nil, profileID: "default")
        drafts.set("Original", for: scope)
        let submission = drafts.submission(for: scope)
        drafts.set("New edit", for: scope)
        drafts.set("Original", for: scope)
        drafts.complete(submission, succeeded: true)
        XCTAssertEqual(drafts.text(for: scope), "Original")
        drafts.complete(drafts.submission(for: scope), succeeded: false)
        XCTAssertEqual(drafts.text(for: scope), "Original")
    }

    func testReplyGrowthPreservesFollowButReadingEarlierStopsIt() {
        var follow = ChatScrollFollowState()
        follow.update(ChatScrollMetrics(contentHeight: 1000, originY: -500, viewportHeight: 500))
        follow.update(ChatScrollMetrics(contentHeight: 1400, originY: -500, viewportHeight: 500))
        XCTAssertTrue(follow.followsLatest, "Growing reply must not count as scrolling away")
        follow.update(ChatScrollMetrics(contentHeight: 1400, originY: -200, viewportHeight: 500))
        XCTAssertFalse(follow.followsLatest)
        follow.update(ChatScrollMetrics(contentHeight: 1800, originY: -200, viewportHeight: 500))
        XCTAssertFalse(follow.followsLatest, "New replies must not pull a reader out of history")
        follow.update(ChatScrollMetrics(contentHeight: 1800, originY: -1300, viewportHeight: 500))
        XCTAssertTrue(follow.followsLatest)
    }
}

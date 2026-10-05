import XCTest
@testable import NekoNative

@MainActor final class SharedComposerTests: XCTestCase {
    func testAnotherWorkspaceQueuesWithoutOfferingAnUnrelatedStop() {
        let empty = ComposerActionState(running: true, hasContent: false, submitting: false, blocked: false, queues: true, canStop: false)
        XCTAssertEqual(empty.primary, .send)
        XCTAssertFalse(empty.buttonEnabled)
        let draft = ComposerActionState(running: true, hasContent: true, submitting: false, blocked: false, queues: true, canStop: false)
        XCTAssertEqual(draft.primary, .queue)
        XCTAssertTrue(draft.canSubmit)
    }

    func testScrollBesideComposerIsObservedWithoutTreatingEditingAsScrollAway() {
        let bounds = CGRect(x: 0, y: 0, width: 1000, height: 800)
        let composer = CGSize(width: 720, height: 180)
        for flipped in [false, true] {
            let bottomY: CGFloat = flipped ? 750 : 50
            XCTAssertTrue(TranscriptInteractionRegion.contains(CGPoint(x: 50, y: bottomY), bounds: bounds, composer: composer, flipped: flipped))
            XCTAssertFalse(TranscriptInteractionRegion.contains(CGPoint(x: 500, y: bottomY), bounds: bounds, composer: composer, flipped: flipped))
            XCTAssertTrue(TranscriptInteractionRegion.contains(CGPoint(x: 500, y: 400), bounds: bounds, composer: composer, flipped: flipped))
        }
    }

    func testUserScrollAwayWinsDuringPostSendPinAndReplyGrowth() {
        var follow = ChatScrollFollowState()
        let sentAt = Date()
        follow.update(ChatScrollMetrics(contentHeight: 1400, originY: -900, viewportHeight: 500), now: sentAt)
        follow.pin(now: sentAt)
        follow.userInteracted()
        follow.update(ChatScrollMetrics(contentHeight: 1400, originY: -300, viewportHeight: 500), now: sentAt.addingTimeInterval(0.1))
        XCTAssertFalse(follow.followsLatest)
        follow.update(ChatScrollMetrics(contentHeight: 1800, originY: -300, viewportHeight: 500), now: sentAt.addingTimeInterval(0.2))
        XCTAssertFalse(follow.followsLatest)
        follow.pin(now: sentAt.addingTimeInterval(0.3))
        XCTAssertTrue(follow.followsLatest, "Jump to latest explicitly restores following")
    }

    func testTicketSubmissionDoesNotClearNewerTextOrAttachments() {
        let drafts = ComposerDraftStore<String>()
        drafts.set("Original", for: "a")
        let sent = drafts.submission(for: "a")
        drafts.set("Changed", for: "a")
        drafts.set("Original", for: "a")
        drafts.complete(sent, succeeded: true)
        XCTAssertEqual(drafts.text(for: "a"), "Original")
        let next = drafts.submission(for: "a")
        drafts.add(attachment, for: "a")
        drafts.complete(next, succeeded: true)
        XCTAssertEqual(drafts.attachments(for: "a"), [attachment])
        XCTAssertEqual(drafts.text(for: "a"), "Original")
    }

    func testSubmissionAndPickerCompletionRemainInOriginatingTicket() {
        let drafts = ComposerDraftStore<String>()
        drafts.set("First", for: "a")
        drafts.set("Second", for: "b")
        let sent = drafts.submission(for: "a")
        let pickerDestination = "a"
        drafts.add(attachment, for: pickerDestination)
        drafts.complete(sent, succeeded: true)
        XCTAssertEqual(drafts.text(for: "b"), "Second")
        XCTAssertTrue(drafts.attachments(for: "b").isEmpty)
        XCTAssertEqual(drafts.attachments(for: "a"), [attachment])
        drafts.complete(drafts.submission(for: "a"), succeeded: false)
        XCTAssertEqual(drafts.text(for: "a"), "First")
        drafts.complete(drafts.submission(for: "a"), succeeded: true)
        XCTAssertEqual(drafts.text(for: "a"), "")
        XCTAssertTrue(drafts.attachments(for: "a").isEmpty)
    }

    func testRunningEmptyHasStopButtonButReturnCannotSubmitOrStop() {
        let state = ComposerActionState(running: true, hasContent: false, submitting: false, blocked: false, queues: true)
        XCTAssertEqual(state.primary, .stop)
        XCTAssertTrue(state.buttonEnabled)
        XCTAssertFalse(state.canSubmit)
    }

    func testHomeQueuesButTicketRepliesKeepTheirOwnSendSemantics() {
        XCTAssertEqual(ComposerActionState(running: true, hasContent: true, submitting: false, blocked: false, queues: true).primary, .queue)
        XCTAssertEqual(ComposerActionState(running: true, hasContent: true, submitting: false, blocked: false, queues: false).primary, .send)
        let saving = ComposerActionState(running: true, hasContent: true, submitting: true, blocked: false, queues: true)
        XCTAssertEqual(saving.primary, .submitting)
        XCTAssertFalse(saving.canSubmit)
        XCTAssertFalse(saving.buttonEnabled)
        let blocked = ComposerActionState(running: true, hasContent: false, submitting: false, blocked: true, queues: true)
        XCTAssertFalse(blocked.buttonEnabled)
        XCTAssertEqual(ComposerActionState(running: false, hasContent: true, submitting: false, blocked: false, queues: true).primary, .send)
    }

    func testLimitsCountUTF8BytesIncludingAttachmentReferences() {
        let exact = String(repeating: "🙂", count: 512)
        XCTAssertNil(ComposerPayload.validationError(exact, limit: 2048))
        XCTAssertNotNil(ComposerPayload.validationError(exact + "é", limit: 2048))
        XCTAssertNil(ComposerPayload.validationError(String(repeating: "🙂", count: 1024), limit: 4096))
        XCTAssertNotNil(ComposerPayload.validationError(String(repeating: "🙂", count: 1025), limit: 4096))
        let drafts = ComposerDraftStore<String>()
        drafts.add(attachment, for: "a")
        let references = drafts.submission(for: "a").text
        XCTAssertEqual(references, attachment.reference)
        drafts.set(String(repeating: "x", count: 2048 - references.utf8.count - 1), for: "a")
        XCTAssertNil(ComposerPayload.validationError(drafts.submission(for: "a").text, limit: 2048))
        drafts.set(drafts.text(for: "a") + "x", for: "a")
        XCTAssertNotNil(ComposerPayload.validationError(drafts.submission(for: "a").text, limit: 2048))
    }

    func testHomeValidatesAfterPlanAndContextAreAdded() {
        let raw = String(repeating: "x", count: 4096)
        XCTAssertNil(ComposerPayload.validationError(raw, limit: 4096))
        let plan = ComposerPayload.homeText(raw, planOnly: true)
        XCTAssertTrue(plan.hasPrefix("Plan only, and change nothing yet:"))
        XCTAssertNotNil(ComposerPayload.validationError(plan, limit: 4096))
        XCTAssertNotNil(ComposerPayload.validationError(raw + "\nContext: é", limit: 4096))
        XCTAssertEqual(ComposerPayload.homeText("/remember use small tests", planOnly: true), "Remember that use small tests")
    }

    func testRuntimeDisplayDoesNotInventAModelOrMislabelUnknownProviders() {
        XCTAssertEqual(ComposerRuntimeLabel.text(provider: "claude", model: "", catalog: ModelCatalog()), "claude · Provider default")
        XCTAssertEqual(ComposerRuntimeLabel.text(provider: "opencodex", model: "xai/grok", catalog: ModelCatalog()), "opencodex · xai/grok")
        XCTAssertEqual(ComposerRuntimeLabel.text(provider: "", model: "", catalog: ModelCatalog()), "Codex · Provider default")
    }

    func testAttachmentRemovalChangesRevisionAndWorkspaceMentionDoesNotSwitchDraft() {
        let drafts = ScopedChatDrafts()
        let first = ChatDraftScope(workspaceID: "a", profileID: "default")
        let second = ChatDraftScope(workspaceID: "b", profileID: "default")
        drafts.set("Check @b", for: first)
        drafts.set("Unrelated", for: second)
        drafts.add(attachment, for: first)
        let sent = drafts.submission(for: first)
        drafts.remove(attachment, for: first)
        drafts.complete(sent, succeeded: true)
        XCTAssertEqual(drafts.text(for: first), "Check @b")
        drafts.set(ComposerMention.replacingTrailingMention(in: drafts.text(for: first), with: "Workspace B"), for: first)
        XCTAssertEqual(drafts.text(for: first), "Check Workspace B ")
        XCTAssertEqual(drafts.text(for: second), "Unrelated")
    }

    private var attachment: ComposerAttachment {
        ComposerAttachment(name: "résumé.pdf", reference: "[résumé.pdf](file:///tmp/saved.pdf)", isImage: false)
    }
}

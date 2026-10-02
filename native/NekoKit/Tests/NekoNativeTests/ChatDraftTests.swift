import XCTest
import NekoKit
@testable import NekoNative

final class ChatDraftTests: XCTestCase {
    func testTodayComposerUsesGlassWithoutOpaqueBacking() throws {
        let source = try String(contentsOf: URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/NekoNative/TodayView.swift"), encoding: .utf8)
        let composer = try XCTUnwrap(source.components(separatedBy: "private func composer(availableWidth: CGFloat)").dropFirst().first?
            .components(separatedBy: "private var canSend").first)
        XCTAssertTrue(composer.contains(".liquidGlass(radius: 16)"))
        XCTAssertFalse(composer.contains(".background(N.card"))
        XCTAssertFalse(composer.contains("Text(\"Message\")"))
        XCTAssertTrue(composer.contains(".menuIndicator(.hidden)"))
        XCTAssertFalse(source.contains("RadialGradient("))
    }

    func testSendButtonUsesFullComposerWidth() throws {
        let source = try String(contentsOf: URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/NekoNative/TodayView.swift"), encoding: .utf8)
        let composer = try XCTUnwrap(source.components(separatedBy: "private func composer(availableWidth: CGFloat)").dropFirst().first?
            .components(separatedBy: "private var canSend").first)
        XCTAssertTrue(composer.contains(".frame(width: cardWidth - 36)"))
    }

    func testProtectedFoldersListOnlyWorkspacesMacOSGuards() {
        let found = PermissionCatalog.protectedFolders(["/Users/a/Documents/neko", "/Users/a/Documents/x", "/Users/a/Code/y", "/Users/a/DesktopStuff", "/Users/a/Library/Mobile Documents/z"], home: "/Users/a")
        XCTAssertEqual(found.map(\.folder), ["Documents", "iCloud Drive"])
        XCTAssertEqual(found[0].workspaces.count, 2)
    }

    func testModelCatalogParsesDaemonSources() throws {
        let data = Data(#"{"sources":[{"provider":"codex","label":"Codex","connection":"ChatGPT · Pro","status":"ready","default_model":"gpt-a","models":[{"id":"gpt-a","label":"GPT A","recommended":true,"access":"listed","reasoning_efforts":["low"]},{"id":"gpt-b","label":"GPT B","access":"unavailable","reason":"No plan"}]},{"provider":"ollama","label":"Ollama","connection":"Ollama · not running","status":"not_running","note":"Start it"},{"provider":"","status":"ready"}],"read_at_ms":1}"#.utf8)
        let catalog = ModelCatalog.parse(try JSONDecoder().decode(JSONValue.self, from: data))
        XCTAssertEqual(catalog.sources.map(\.provider), ["codex", "ollama"])
        XCTAssertEqual(catalog.source("")?.defaultModel, "gpt-a")
        XCTAssertTrue(catalog.model("codex", "gpt-a")?.recommended == true)
        XCTAssertEqual(catalog.model("codex", "gpt-b")?.usable, false)
        XCTAssertEqual(catalog.source("ollama")?.status, .notRunning)
        XCTAssertEqual(AgentModelCatalog.label(provider: "codex", model: "gpt-a", catalog: catalog), "Codex · GPT A")
    }

    func testTodayComposerHasNoFocusHighlight() throws {
        let source = try String(contentsOf: URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/NekoNative/TodayView.swift"), encoding: .utf8)
        let composer = try XCTUnwrap(source.components(separatedBy: ".liquidGlass(radius: 16)").dropFirst().first?
            .components(separatedBy: ".frame(maxWidth: .infinity, alignment: .center)").first)
        XCTAssertFalse(composer.contains(".overlay"))
        XCTAssertFalse(composer.contains(".strokeBorder"))
        XCTAssertFalse(composer.contains(".padding(.horizontal, 32)"))
        XCTAssertFalse(source.contains("composerFocused"))
    }

    func testComposerUsesReturnToSendAndShiftReturnForNewline() throws {
        let source = try String(contentsOf: URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/NekoNative/ComposerView.swift"), encoding: .utf8)
        XCTAssertTrue(source.contains("override func keyDown(with event: NSEvent)"))
        XCTAssertTrue(source.contains("event.modifierFlags.contains(.shift)"))
        XCTAssertTrue(source.contains("interruptAndSubmit"))
    }

    func testTodayComposerOmitsScopeAndExplanationControls() throws {
        let source = try String(contentsOf: URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/NekoNative/TodayView.swift"), encoding: .utf8)
        let composer = try XCTUnwrap(source.components(separatedBy: "private func composer(availableWidth: CGFloat)").dropFirst().first?
            .components(separatedBy: ".liquidGlass(radius: 16)").first)
        XCTAssertFalse(composer.contains("WorkspaceMenu"))
        XCTAssertFalse(composer.contains("Read only"))
        XCTAssertFalse(composer.contains("sidebar.right"))
        XCTAssertFalse(composer.contains("⌘↵"))
    }

    func testFloatingComposerReservesItsMeasuredHeightInTranscript() {
        XCTAssertEqual(ChatComposerClearance.bottomSpace(for: 0), 24)
        XCTAssertEqual(ChatComposerClearance.bottomSpace(for: 120), 144)
        XCTAssertEqual(ChatComposerClearance.bottomSpace(for: 220), 244)
    }

    func testComposerHeightGrowsAndStopsAtReadableLimit() {
        XCTAssertEqual(ComposerLayout.height(forTextHeight: 16), 40)
        XCTAssertEqual(ComposerLayout.height(forTextHeight: 84), 96)
        XCTAssertEqual(ComposerLayout.height(forTextHeight: 300), 160)
    }

    func testComposerWidthTracksAvailableSpace() {
        XCTAssertEqual(ComposerLayout.textWidth(for: 720), 720)
        XCTAssertEqual(ComposerLayout.textWidth(for: 280), 280)
    }

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

    func testAttachmentsStayWithTheirDraftAndJoinOnlyOnSubmission() {
        var drafts = ScopedChatDrafts()
        let first = ChatDraftScope(workspaceID: "one", profileID: "default")
        let second = ChatDraftScope(workspaceID: "two", profileID: "default")
        let attachment = ComposerAttachment(name: "report.pdf", reference: "[report.pdf](file:///tmp/report.pdf)", isImage: false)
        drafts.set("Check this", for: first)
        drafts.add(attachment, for: first)
        XCTAssertEqual(drafts.attachments(for: first), [attachment])
        XCTAssertTrue(drafts.attachments(for: second).isEmpty)
        let submission = drafts.submission(for: first)
        XCTAssertEqual(submission.text, "Check this\n[report.pdf](file:///tmp/report.pdf)")
        drafts.complete(submission, succeeded: true)
        XCTAssertTrue(drafts.attachments(for: first).isEmpty)
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

    func testTodayWorkSummaryIncludesOnlySelectedWorkspaceAndActionableWork() {
        let tasks: [JSONValue] = [
            .object(["id": .string("a"), "workspace_id": .string("one"), "status": .string("AwaitingApproval")]),
            .object(["id": .string("b"), "workspace_id": .string("one"), "status": .string("Building")]),
            .object(["id": .string("c"), "workspace_id": .string("two"), "status": .string("ReadyForReview")]),
            .object(["id": .string("d"), "workspace_id": .string("one"), "status": .string("Completed")])
        ]
        let scoped = TodayWorkSummary(tasks: tasks, workspaceID: "one")
        XCTAssertEqual(scoped.needsYou.map(\.recordID), ["a"])
        XCTAssertEqual(scoped.working.map(\.recordID), ["b"])
        XCTAssertEqual(scoped.activeCount, 2)
        let all = TodayWorkSummary(tasks: tasks, workspaceID: nil)
        XCTAssertEqual(all.needsYou.map(\.recordID), ["a", "c"])
        XCTAssertEqual(all.activeCount, 3)
    }
}

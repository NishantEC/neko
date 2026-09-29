import XCTest
import NekoKit
@testable import NekoNative

private actor ControlledTransport {
    private var pending: [Int: CheckedContinuation<JSONValue, any Error>] = [:]
    private var count = 0
    func request(_ value: JSONValue) async throws -> JSONValue {
        let index = count; count += 1
        return try await withCheckedThrowingContinuation { pending[index] = $0 }
    }
    func waitFor(_ number: Int) async {
        while count < number { await Task.yield() }
    }
    func respond(_ index: Int, _ value: JSONValue) { pending.removeValue(forKey: index)?.resume(returning: value) }
    func fail(_ index: Int) { pending.removeValue(forKey: index)?.resume(throwing: NSError(domain: "Test", code: 1)) }
}

@MainActor final class AppModelTests: XCTestCase {
    func testClipboardConsentWaitsForExactAcknowledgment() throws {
        var state = ClipboardConsentState()
        XCTAssertFalse(state.begin(true))
        try state.load(.object(["ClipboardHistoryEnabled": .object(["enabled": .bool(false)])]))
        XCTAssertTrue(state.begin(true))
        XCTAssertFalse(state.begin(false))
        XCTAssertEqual(state.enabled, false)
        XCTAssertThrowsError(try state.finish(.object(["ClipboardHistoryEnabled": .object(["enabled": .bool(false)])])))
        XCTAssertEqual(state.enabled, false)
        XCTAssertTrue(state.begin(true))
        state.failed()
        XCTAssertEqual(state.enabled, false)
        XCTAssertTrue(state.begin(true))
        try state.finish(.object(["ClipboardHistoryEnabled": .object(["enabled": .bool(true)])]))
        XCTAssertEqual(state.enabled, true)
    }
    func testReconnectClearsTransportBanner() async {
        let transport = ControlledTransport()
        let model = AppModel { try await transport.request($0) }
        let failed = Task { await model.refresh() }
        await transport.waitFor(1)
        await transport.fail(0)
        await failed.value
        XCTAssertFalse(model.connected)
        XCTAssertNotNil(model.error)
        let recovered = Task { await model.refresh() }
        await transport.waitFor(2)
        await transport.respond(1, response("connected"))
        await recovered.value
        XCTAssertTrue(model.connected)
        XCTAssertNil(model.error)
    }

    func testReconnectPreservesFailedMutationEvenAcrossAnotherOutage() async {
        let transport = ControlledTransport()
        let model = AppModel { try await transport.request($0) }
        let mutation = Task { await model.workbench(.command("SaveMemory")) }
        await transport.waitFor(1)
        await transport.respond(0, .command("Error", ["message": .string("Memory was not saved")]))
        _ = await mutation.value
        let outage = Task { await model.refresh() }
        await transport.waitFor(2)
        await transport.fail(1)
        await outage.value
        XCTAssertEqual(model.error, "Memory was not saved")
        let recovered = Task { await model.refresh() }
        await transport.waitFor(3)
        await transport.respond(2, response("connected"))
        await recovered.value
        XCTAssertTrue(model.connected)
        XCTAssertEqual(model.error, "Memory was not saved")
        model.error = nil
        XCTAssertNil(model.error)
    }

    func testTodayNavigationKeepsDraftsAndPendingSubmissionInModel() {
        let model = AppModel { _ in .string("Activated") }
        let scope = ChatDraftScope(workspaceID: "workspace", profileID: "profile")
        var today: TodayView? = TodayView(model: model)
        today?.model.chatDrafts.set("Keep this draft", for: scope)
        today?.model.sendingChatScopes.insert(scope)
        today = nil
        let recreated = TodayView(model: model)
        XCTAssertEqual(recreated.model.chatDrafts.text(for: scope), "Keep this draft")
        XCTAssertTrue(recreated.model.sendingChatScopes.contains(scope))
    }
    func testReviewPresentationDoesNotInferPassFromMalformedResponse() {
        let valid = "Built it\n\nIndependent review:\n{\"passed\":true,\"findings\":[],\"files\":[\"a\"],\"tests\":[\"test\"],\"summary\":\"Checked\"}"
        XCTAssertEqual(TicketPresentation.review(valid).body, "Built it")
        XCTAssertEqual(TicketPresentation.review(valid).verdict?.passed, true)
        for raw in ["passed: true", "Result\n\nIndependent review:\n{\"passed\":true}", "Result\n\nIndependent review:\n{\"passed\":\"true\"}"] {
            XCTAssertNil(TicketPresentation.review(raw).verdict)
            XCTAssertEqual(TicketPresentation.review(raw).body, raw)
        }
    }
    func testStatusSummaryUsesAllWorkspacesAndOnlyActionableStatuses() {
        let tasks = ["AwaitingApproval", "ReadyForReview", "Failed", "Building", "Queued", "Completed", "Cancelled"].map { JSONValue.object(["status": .string($0)]) }
        let summary = StatusSummary(snapshot: .object(["tasks": .array(tasks)]))
        XCTAssertEqual(summary.needsAttention, 3)
        XCTAssertEqual(summary.working, 2)
        XCTAssertEqual(summary.badge, "3")
        XCTAssertNil(StatusSummary(snapshot: .null).badge)
    }
    func testProfileAllowsEmptyInstructionsButRequiresName() {
        XCTAssertTrue(ManagementKind.profile.hasRequiredContent(name: "Neko", text: ""))
        XCTAssertTrue(ManagementKind.profile.hasRequiredContent(name: "Neko", text: "  \n"))
        XCTAssertFalse(ManagementKind.profile.hasRequiredContent(name: "  ", text: "Instructions"))
        XCTAssertFalse(ManagementKind.schedule.hasRequiredContent(name: "Daily", text: ""))
        XCTAssertFalse(ManagementKind.memory.hasRequiredContent(name: "", text: ""))
        XCTAssertTrue(ManagementKind.memory.hasRequiredContent(name: "", text: "Remember this"))
    }
    func testSetupCompletionRequiresPositiveAcknowledgment() async {
        for reply: JSONValue in [.string("Activated"), .object(["OnboardingState": .object(["completed": .bool(false)])])] {
            let model = AppModel { _ in reply }
            model.onboarding = true
            await model.completeSetup()
            XCTAssertTrue(model.onboarding)
            XCTAssertNotNil(model.error)
        }
        let model = AppModel { _ in .object(["OnboardingState": .object(["completed": .bool(true)])]) }
        model.onboarding = true
        await model.completeSetup()
        XCTAssertFalse(model.onboarding)
        XCTAssertNil(model.error)
    }

    func testSkillRecoveryRespectsWorkspaceAndRetainsChangedGrants() {
        let enabled: JSONValue = .object(["path": .string("/skill"), "workspace_id": .string("a"), "content_hash": .string("old")])
        let other: JSONValue = .object(["path": .string("/skill"), "workspace_id": .string("b"), "content_hash": .string("new")])
        XCTAssertEqual(SkillPresentation.unavailable(enabled: [enabled], available: [other], workspace: "a"), [enabled])
        let available: JSONValue = .object(["path": .string("/skill"), "workspace_id": .null, "content_hash": .string("new")])
        XCTAssertTrue(SkillPresentation.unavailable(enabled: [enabled], available: [available], workspace: "a").isEmpty)
        XCTAssertEqual(SkillPresentation.enabledRecord(for: available, enabled: [enabled], workspace: "a"), enabled)
        XCTAssertNil(SkillPresentation.enabledRecord(for: available, enabled: [enabled], workspace: "b"))
    }

    func testWatchingRequiresCurrentWorkspaceToolGrantBeforeTurnOn() async {
        let model = AppModel { _ in XCTFail("Turn on must not send a command without a usable grant"); return .null }
        let item: JSONValue = .object(["id": .string("check"), "workspace_id": .string("a"), "connection_ids": .array([.string("admin")])])
        let tool: JSONValue = .object(["name": .string("search"), "schema_hash": .string("v1")])
        let connection: JSONValue = .object(["id": .string("admin"), "label": .string("Admin"), "enabled": .bool(true), "tools": .array([tool])])
        let grant: JSONValue = .object(["workspace_id": .string("a"), "connection_id": .string("admin"), "tool_name": .string("search"), "schema_hash": .string("v1")])
        model.snapshot = .object(["mcp": .object(["connections": .array([connection]), "grants": .array([])])])
        XCTAssertEqual(Watching.ungranted(model, item), ["Admin"])
        Watching.turnOn(model, item)
        XCTAssertEqual(model.selectedWorkspace, "a")
        XCTAssertFalse(model.busy)
        model.snapshot = .object(["mcp": .object(["connections": .array([connection]), "grants": .array([grant])])])
        XCTAssertTrue(Watching.ungranted(model, item).isEmpty)
        model.snapshot = .object(["mcp": .object(["connections": .array([connection]), "grants": .array([.object(["workspace_id": .string("b"), "connection_id": .string("admin"), "tool_name": .string("search"), "schema_hash": .string("v1")])])])])
        XCTAssertEqual(Watching.ungranted(model, item), ["Admin"])
    }

    func testSplitProposalGuardExcludesParentsAndChildren() {
        let task: JSONValue = .object(["id": .string("parent"), "status": .string("AwaitingApproval")])
        let split: JSONValue = .object(["parent_id": .string("parent"), "subtasks": .array([.object(["task_id": .string("child")])])])
        XCTAssertTrue(TicketPresentation.canProposeSplit(task, splits: []))
        XCTAssertFalse(TicketPresentation.canProposeSplit(task, splits: [split]))
        XCTAssertFalse(TicketPresentation.canProposeSplit(.object(["id": .string("child"), "status": .string("AwaitingApproval")]), splits: [split]))
        XCTAssertFalse(TicketPresentation.canProposeSplit(.object(["id": .string("new"), "status": .string("Building")]), splits: []))
    }
    func testSchedulePresetsAndCustomPreservation() {
        XCTAssertEqual(ScheduleRecurrence.daily.rule(hour: 9, minute: 30, weekday: "MO", interval: 4, custom: ""), "FREQ=DAILY;BYHOUR=9;BYMINUTE=30")
        XCTAssertEqual(ScheduleRecurrence.weekdays.rule(hour: 8, minute: 0, weekday: "MO", interval: 4, custom: ""), "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;BYHOUR=8;BYMINUTE=0")
        XCTAssertEqual(ScheduleRecurrence.weekly.rule(hour: 17, minute: 45, weekday: "FR", interval: 4, custom: ""), "FREQ=WEEKLY;BYDAY=FR;BYHOUR=17;BYMINUTE=45")
        XCTAssertEqual(ScheduleRecurrence.hourly.rule(hour: 9, minute: 0, weekday: "MO", interval: 6, custom: ""), "FREQ=HOURLY;INTERVAL=6")
        let custom = "FREQ=MONTHLY;BYMONTHDAY=15;COUNT=8"
        XCTAssertEqual(ScheduleRecurrence.custom.rule(hour: 9, minute: 0, weekday: "MO", interval: 4, custom: custom), custom)
        XCTAssertEqual(ScheduleRecurrence.summary("FREQ=WEEKLY;BYDAY=FR;BYHOUR=17;BYMINUTE=45"), "Every Friday at 17:45")
        XCTAssertEqual(ScheduleRecurrence.summary(custom), "Custom schedule")
    }
    private func response(_ marker: String) -> JSONValue { .object(["Workbench": .object(["marker": .string(marker)])]) }

    func testBusyMutationReturnsFalseEvenWhenEarlierSaveSucceeds() async {
        let transport = ControlledTransport()
        let model = AppModel { try await transport.request($0) }
        let first = Task { await model.workbench(.command("SaveMemory")) }
        await transport.waitFor(1)
        let second = await model.workbench(.command("SaveMemory"))
        XCTAssertFalse(second)
        XCTAssertTrue(model.busy)
        await transport.respond(0, response("saved"))
        let firstResult = await first.value
        XCTAssertTrue(firstResult)
        XCTAssertFalse(model.busy)
        XCTAssertNil(model.error)
        XCTAssertFalse(second, "A cleared global error must not turn a rejected save into success")
    }

    func testOldRefreshCannotReplaceMutationSnapshot() async {
        let transport = ControlledTransport()
        let model = AppModel { try await transport.request($0) }
        let refresh = Task { await model.refresh() }
        await transport.waitFor(1)
        let mutation = Task { await model.workbench(.command("SaveMemory")) }
        await transport.waitFor(2)
        await transport.respond(1, response("new"))
        _ = await mutation.value
        await transport.respond(0, response("old"))
        await refresh.value
        XCTAssertEqual(model.snapshot["marker"].string, "new")
    }

    func testStaleRefreshFailureDoesNotDisconnectSuccessfulMutation() async {
        let transport = ControlledTransport()
        let model = AppModel { try await transport.request($0) }
        let refresh = Task { await model.refresh() }
        await transport.waitFor(1)
        let mutation = Task { await model.workbench(.command("SaveMemory")) }
        await transport.waitFor(2)
        await transport.respond(1, response("new"))
        _ = await mutation.value
        await transport.fail(0)
        await refresh.value
        XCTAssertTrue(model.connected)
        XCTAssertNil(model.error)
    }

    func testOverlappingRefreshesKeepNewestRequest() async {
        let transport = ControlledTransport()
        let model = AppModel { try await transport.request($0) }
        let first = Task { await model.refresh() }
        await transport.waitFor(1)
        let second = Task { await model.refresh() }
        await transport.waitFor(2)
        await transport.respond(1, response("new"))
        await second.value
        await transport.respond(0, response("old"))
        await first.value
        XCTAssertEqual(model.snapshot["marker"].string, "new")
    }

    func testUnexpectedMutationResponseIsFailure() async {
        let model = AppModel { _ in .string("Activated") }
        let success = await model.workbench(.command("SaveMemory"))
        XCTAssertFalse(success)
        XCTAssertNotNil(model.error)
        XCTAssertFalse(model.busy)
    }

    func testStartupKeepsLoadingUntilOnboardingReadCompletes() async {
        let transport = ControlledTransport()
        let model = AppModel { try await transport.request($0) }
        let start = Task { await model.start() }
        await transport.waitFor(1)
        XCTAssertTrue(model.loadingSetup)
        await transport.respond(0, response("initial"))
        await transport.waitFor(2)
        XCTAssertTrue(model.loadingSetup)
        await transport.respond(1, .object(["OnboardingState": .object(["completed": .bool(false)])]))
        await start.value
        XCTAssertFalse(model.loadingSetup)
        XCTAssertTrue(model.onboarding)
    }

    func testUnavailableOnboardingStateDoesNotExposeWorkspace() async {
        let model = AppModel { request in
            if request == .string("GetOnboardingState") { return .string("Activated") }
            return .object(["Workbench": .object([:])])
        }
        await model.start()
        XCTAssertTrue(model.loadingSetup)
        XCTAssertNotNil(model.error)
    }
}

import XCTest
import NekoKit
@testable import NekoNative

private actor RuntimeApplyTransport {
    private var requests: [JSONValue] = []
    private var pending: CheckedContinuation<JSONValue, any Error>?
    private var currentSnapshot: JSONValue = .object(["conversation_runtime": .object([:])])

    func request(_ value: JSONValue) async throws -> JSONValue {
        requests.append(value)
        if value["Workbench"] == .string("Snapshot") { return .object(["Workbench": currentSnapshot]) }
        return try await withCheckedThrowingContinuation { pending = $0 }
    }
    func waitForRequest() async -> Bool {
        for _ in 0..<10000 {
            if pending != nil { return true }
            await Task.yield()
        }
        return false
    }
    func finish(_ response: JSONValue) {
        if case .object = response["Workbench"] { currentSnapshot = response["Workbench"] }
        pending?.resume(returning: response)
        pending = nil
    }
    func recorded() -> [JSONValue] { requests }
}

@MainActor final class ComposerRuntimeApplyTests: XCTestCase {
    private let target = "home:writer:workspace-a"
    private var catalog: ModelCatalog { RuntimeSelectionSample.catalog }
    private func reply(_ id: String, _ preferences: RuntimeSelectionPreferences, revision: UInt64 = 2) -> JSONValue {
        .object(["Workbench": .object(["conversation_runtime": .object([id: preferences.wire]), "conversation_runtime_revisions": .object([id: .number(Double(revision))])])])
    }

    func testApplyCapturesScopeChecksOnceAndDoesNotBlockTypingAcrossNavigation() async {
        let transport = RuntimeApplyTransport()
        let model = AppModel { try await transport.request($0) }
        let first = RuntimeSelectionState(conversationID: target, catalog: catalog)
        first.draft.reasoningEffort = "high"
        let submitted = first.draft
        let apply = Task { await first.apply { id, preferences in
            try await ComposerRuntimePicker.save(model, conversationID: id, preferences: preferences)
        } }
        let started = await transport.waitForRequest()
        XCTAssertTrue(started)
        XCTAssertTrue(first.saving)
        XCTAssertFalse(model.busy)
        // A new view owns a different state instance; the suspended save keeps its original scope.
        model.selectedWorkspace = "workspace-b"
        let second = RuntimeSelectionState(conversationID: "home:writer:workspace-b", catalog: catalog)
        second.draft.serviceTier = "default"
        let scope = ChatDraftScope(workspaceID: "workspace-b", profileID: "writer")
        model.chatDrafts.set("Typing remains available", for: scope)
        XCTAssertEqual(model.chatDrafts.text(for: scope), "Typing remains available")
        XCTAssertEqual(first.saved, .init())
        await first.apply { _, _ in XCTFail("A pending save must not send a second check"); return .init() }
        await transport.finish(reply(target, submitted))
        await apply.value
        XCTAssertFalse(first.saving)
        XCTAssertFalse(model.busy)
        XCTAssertEqual(first.saved, submitted)
        XCTAssertEqual(second.saved, .init())
        XCTAssertEqual(second.draft.serviceTier, "default")
        let requests = await transport.recorded()
        XCTAssertEqual(requests.count, 2, "Exactly one check/apply request followed by one snapshot refresh")
        XCTAssertEqual(requests.first, RuntimeSelectionScope.request(conversationID: target, preferences: submitted))
        XCTAssertEqual(requests.last, .object(["Workbench": .string("Snapshot")]))
        XCTAssertEqual(RuntimeSelectionPreferences.saved(in: model.snapshot, conversationID: target), submitted)
    }

    func testDaemonFailurePreservesSavedConfigurationAndPendingDraft() async {
        let previous = RuntimeSelectionPreferences(serviceTier: "default")
        let state = RuntimeSelectionState(conversationID: target, preferences: previous, catalog: catalog)
        state.draft.reasoningEffort = "high"
        let model = AppModel { _ in
            .object(["Error": .object(["message": .string("Settings changed during the check")])])
        }
        await state.apply { id, preferences in
            try await ComposerRuntimePicker.save(model, conversationID: id, preferences: preferences)
        }
        XCTAssertEqual(state.saved, previous)
        XCTAssertEqual(state.draft.reasoningEffort, "high")
        XCTAssertEqual(state.applyError, "Settings changed during the check")
        XCTAssertTrue(state.canApply)
        XCTAssertFalse(model.busy)
        XCTAssertNil(model.error, "Picker failures stay local rather than disabling the composer")
    }

    func testNewerDraftSurvivesCompletionAndPollingCannotOverwritePendingSave() async {
        let transport = RuntimeApplyTransport()
        let model = AppModel { try await transport.request($0) }
        let state = RuntimeSelectionState(conversationID: target, catalog: catalog)
        state.draft.reasoningEffort = "high"
        let submitted = state.draft
        let apply = Task { await state.apply { id, preferences in
            try await ComposerRuntimePicker.save(model, conversationID: id, preferences: preferences)
        } }
        let started = await transport.waitForRequest()
        XCTAssertTrue(started)
        state.sync(.init(serviceTier: "default"), revision: 1)
        XCTAssertEqual(state.saved, .init())
        state.draft.reasoningEffort = "low"
        await transport.finish(reply(target, submitted))
        await apply.value
        XCTAssertEqual(state.saved.reasoningEffort, "high")
        XCTAssertEqual(state.draft.reasoningEffort, "low")
        XCTAssertTrue(state.dirty)
    }

    func testResetAcceptsRemovedPreferenceEntry() async {
        let state = RuntimeSelectionState(conversationID: target, preferences: .init(reasoningEffort: "high"), catalog: catalog)
        state.reset()
        let response: JSONValue = .object(["Workbench": .object([
            "conversation_runtime": .object([:]), "conversation_runtime_revisions": .object([target: .number(3)])
        ])])
        let model = AppModel { _ in response }
        await state.apply { id, preferences in
            try await ComposerRuntimePicker.save(model, conversationID: id, preferences: preferences)
        }
        XCTAssertEqual(state.saved, .init())
        XCTAssertNil(state.applyError)
        XCTAssertFalse(state.dirty)
    }

    func testMalformedOrDifferentConfirmationCannotReplacePreviousSettings() async {
        for response: JSONValue in [
            .object(["Workbench": .object([:])]),
            .object(["Workbench": .object(["conversation_runtime": .object([:])])]),
        ] {
            let previous = RuntimeSelectionPreferences(serviceTier: "default")
            let state = RuntimeSelectionState(conversationID: target, preferences: previous, catalog: catalog)
            state.draft.reasoningEffort = "high"
            let model = AppModel { _ in response }
            await state.apply { id, preferences in
                try await ComposerRuntimePicker.save(model, conversationID: id, preferences: preferences)
            }
            XCTAssertEqual(state.saved, previous)
            XCTAssertNotNil(state.applyError)
        }
    }

    func testRejectedSaveReplaysNewestPollAndDiscardUsesWinningSettings() async {
        let transport = RuntimeApplyTransport()
        let model = AppModel { try await transport.request($0) }
        let previous = RuntimeSelectionPreferences(serviceTier: "default")
        let state = RuntimeSelectionState(conversationID: target, preferences: previous, revision: 4, catalog: catalog)
        state.draft.reasoningEffort = "high"
        let localDraft = state.draft
        let apply = Task { await state.apply { id, preferences in
            try await ComposerRuntimePicker.save(model, conversationID: id, preferences: preferences)
        } }
        let started = await transport.waitForRequest()
        XCTAssertTrue(started)
        // Another instance of this conversation won its save while this instance was checking.
        let winner = RuntimeSelectionPreferences(reasoningEffort: "low")
        state.sync(winner, revision: 6)
        state.sync(.init(reasoningEffort: "medium"), revision: 5)
        XCTAssertEqual(state.savedRevision, 4)
        await transport.finish(.object(["Error": .object(["message": .string("Settings changed during the check")])]))
        await apply.value
        XCTAssertEqual(state.saved, winner)
        XCTAssertEqual(state.savedRevision, 6)
        XCTAssertEqual(state.draft, localDraft)
        XCTAssertNotNil(state.applyError)
        state.discard()
        XCTAssertEqual(state.draft, winner)
        state.sync(previous, revision: 4)
        XCTAssertEqual(state.saved, winner)
    }

    func testSuccessfulSaveReceiptWinsOverOlderPendingAndSubsequentPolls() async {
        let transport = RuntimeApplyTransport()
        let model = AppModel { try await transport.request($0) }
        let state = RuntimeSelectionState(conversationID: target, revision: 4, catalog: catalog)
        state.draft.reasoningEffort = "high"
        let submitted = state.draft
        let apply = Task { await state.apply { id, preferences in
            try await ComposerRuntimePicker.save(model, conversationID: id, preferences: preferences)
        } }
        let started = await transport.waitForRequest()
        XCTAssertTrue(started)
        state.sync(.init(reasoningEffort: "low"), revision: 5)
        await transport.finish(reply(target, submitted, revision: 7))
        await apply.value
        XCTAssertEqual(state.saved, submitted)
        XCTAssertEqual(state.draft, submitted)
        XCTAssertEqual(state.savedRevision, 7, "Use the response revision, not an increment guessed by the client")
        state.sync(.init(), revision: 6)
        XCTAssertEqual(state.saved, submitted)
        state.sync(submitted, revision: 8)
        XCTAssertEqual(state.savedRevision, 8, "No-op saves still advance the confirmed revision")
    }

    func testNewerConfirmationWinsOverDelayedSuccessfulReceipt() async {
        let transport = RuntimeApplyTransport()
        let model = AppModel { try await transport.request($0) }
        let state = RuntimeSelectionState(conversationID: target, revision: 4, catalog: catalog)
        state.draft.reasoningEffort = "high"
        let submitted = state.draft
        let apply = Task { await state.apply { id, preferences in
            try await ComposerRuntimePicker.save(model, conversationID: id, preferences: preferences)
        } }
        let started = await transport.waitForRequest()
        XCTAssertTrue(started)
        let later = RuntimeSelectionPreferences(reasoningEffort: "low")
        state.sync(later, revision: 9)
        await transport.finish(reply(target, submitted, revision: 8))
        await apply.value
        XCTAssertEqual(state.savedRevision, 9)
        XCTAssertEqual(state.saved, later)
        XCTAssertEqual(state.draft, later)
        XCTAssertFalse(state.dirty)
        state.sync(.init(serviceTier: "default"), revision: 10)
        XCTAssertEqual(state.savedRevision, 10)
        XCTAssertEqual(state.draft.serviceTier, "default")
        state.sync(submitted, revision: 8)
        XCTAssertEqual(state.savedRevision, 10)
    }

    func testConfirmationIncludesNoOpAndResetRevisionsAndRejectsInvalidReceipts() {
        let preferences = RuntimeSelectionPreferences(reasoningEffort: "high")
        let first = RuntimeSelectionConfirmation.read(in: reply(target, preferences, revision: 1)["Workbench"], conversationID: target)
        let noOp = RuntimeSelectionConfirmation.read(in: reply(target, preferences, revision: 2)["Workbench"], conversationID: target)
        XCTAssertNotEqual(first, noOp, "The picker must observe revisions even when preferences are unchanged")
        let reset: JSONValue = .object(["conversation_runtime": .object([:]), "conversation_runtime_revisions": .object([target: .number(3)])])
        XCTAssertEqual(RuntimeSelectionConfirmation.read(in: reset, conversationID: target), .init(preferences: .init(), revision: 3))
        XCTAssertNil(RuntimeSelectionConfirmation.read(in: .object([:]), conversationID: target, requireRevision: true))
        for invalid in [-1.0, 1.5, Double.infinity, 9_007_199_254_740_992] {
            let snapshot: JSONValue = .object(["conversation_runtime_revisions": .object([target: .number(invalid)])])
            XCTAssertNil(RuntimeSelectionConfirmation.read(in: snapshot, conversationID: target))
        }
    }

    func testCatalogRequestIsReadOnlyAndMalformedRepliesSurfaceAnError() async throws {
        let model = AppModel { value in
            XCTAssertEqual(value, .command("AgentModels", ["refresh": .bool(true)]))
            return .object(["AgentModels": .object(["sources": .array([])])])
        }
        let empty = try await AgentModelCatalog.fetch(model, refresh: true)
        XCTAssertTrue(empty.sources.isEmpty)
        let broken = AppModel { _ in .object(["AgentModels": .object([:])]) }
        do {
            _ = try await AgentModelCatalog.fetch(broken)
            XCTFail("Malformed metadata is an error, not an empty catalog")
        } catch {
            XCTAssertTrue(error.localizedDescription.contains("unreadable model catalog"))
        }
    }
}

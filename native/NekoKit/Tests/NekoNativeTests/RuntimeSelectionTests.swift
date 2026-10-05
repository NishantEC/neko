import XCTest
import NekoKit
@testable import NekoNative

@MainActor final class RuntimeSelectionTests: XCTestCase {
    private func json(_ text: String) throws -> JSONValue { try JSONDecoder().decode(JSONValue.self, from: Data(text.utf8)) }
    private var catalog: ModelCatalog { RuntimeSelectionSample.catalog }

    func testCatalogKeepsUnknownUnsupportedDescriptionsAndAllAdvertisedTiers() throws {
        let payload = try json("""
        {"sources":[{"provider":"codex","label":"Codex","connection":"Connected","status":"ready","models":[
          {"id":"old","label":"Old","reasoning_efforts":["high"]},
          {"id":"fixed","label":"Fixed","effort_options":[],"speed_options":[]},
          {"id":"rich","label":"Rich","effort_options":[{"id":"deep","label":"Deep","description":"Take more time"}],
           "default_effort":"deep","speed_options":[{"id":"priority","label":"Fast"},{"id":"rush","label":"Express"},{"id":"other","label":"Third extra"}],"default_speed":"default"}
        ]}]}
        """)
        let parsed = ModelCatalog.parse(payload)
        let old = try XCTUnwrap(parsed.model("codex", "old"))
        XCTAssertEqual(old.reasoningEfforts, ["high"])
        XCTAssertNil(old.effortOptions)
        XCTAssertNil(old.speedOptions)
        XCTAssertEqual(parsed.model("codex", "fixed")?.effortOptions, [])
        XCTAssertEqual(parsed.model("codex", "fixed")?.speedOptions, [])
        let rich = try XCTUnwrap(parsed.model("codex", "rich"))
        XCTAssertEqual(rich.effortOptions?.first?.description, "Take more time")
        XCTAssertEqual(rich.defaultEffort, "deep")
        XCTAssertEqual(rich.defaultSpeed, "default")
        XCTAssertEqual(rich.speedOptions?.map(\.id), ["priority", "rush", "other"])
    }

    func testExplicitUnsupportedMetadataDoesNotReuseLegacyEffortLadder() throws {
        let payload = try json("""
        {"sources":[{"provider":"codex","status":"ready","models":[
        {"id":"fixed","reasoning_efforts":["high","ultra"],"effort_options":[],"speed_options":[]}]}]}
        """)
        let state = RuntimeSelectionState(conversationID: "task:a", catalog: ModelCatalog.parse(payload))
        XCTAssertTrue(state.effortOptions.isEmpty)
        XCTAssertTrue(state.extraSpeedOptions.isEmpty)
        state.draft.reasoningEffort = "ultra"
        XCTAssertNotNil(state.validationMessage)
        XCTAssertFalse(state.canApply)
    }

    func testModelChangeReconcilesOnlyInvalidPinsAndReportsThem() throws {
        var pins = RuntimeSelectionPreferences(provider: "sample", model: "sample-astra", reasoningEffort: "high", serviceTier: "rush", allowPaidSpeed: true)
        let quick = try XCTUnwrap(catalog.model("sample", "sample-quick"))
        let notices = pins.selectModel(provider: "sample", model: quick)
        XCTAssertEqual(notices.count, 2)
        XCTAssertNil(pins.reasoningEffort)
        XCTAssertNil(pins.serviceTier)
        XCTAssertTrue(pins.allowPaidSpeed)
        pins.reasoningEffort = "low"
        pins.serviceTier = "default"
        XCTAssertTrue(pins.selectModel(provider: "sample", model: quick).isEmpty)
        XCTAssertEqual(pins.reasoningEffort, "low")
        XCTAssertEqual(pins.serviceTier, "default")
        pins.automaticModel()
        XCTAssertNil(pins.provider)
        XCTAssertNil(pins.model)
        XCTAssertEqual(pins.reasoningEffort, "low")
        XCTAssertEqual(pins.serviceTier, "default")
    }

    func testChangingToUnknownCapabilitiesResetsUnverifiablePins() {
        var pins = RuntimeSelectionPreferences(reasoningEffort: "high", serviceTier: "priority")
        let unknown = CatalogModel(id: "new", label: "New", reasoningEfforts: ["high"])
        XCTAssertEqual(pins.selectModel(provider: "codex", model: unknown).count, 2)
        XCTAssertNil(pins.reasoningEffort)
        XCTAssertNil(pins.serviceTier)
    }

    func testScopeEncodingPreservesProfileWorkspaceAndTaskIdentity() {
        XCTAssertEqual(RuntimeSelectionScope.home(.init(workspaceID: nil, profileID: "default")), "home:default:*")
        XCTAssertEqual(RuntimeSelectionScope.home(.init(workspaceID: "workspace-1", profileID: "writer")), "home:writer:workspace-1")
        XCTAssertEqual(RuntimeSelectionScope.task("task-1"), "task:task-1")
        let preferences = RuntimeSelectionPreferences(reasoningEffort: "high")
        let request = RuntimeSelectionScope.request(conversationID: "home:writer:workspace-1", preferences: preferences)
        XCTAssertEqual(request["Workbench"]["SetConversationRuntime"]["conversation_id"], .string("home:writer:workspace-1"))
        let wire = request["Workbench"]["SetConversationRuntime"]["preferences"]
        XCTAssertEqual(wire["provider"], .null)
        XCTAssertEqual(wire["model"], .null)
        XCTAssertEqual(wire["reasoning_effort"], .string("high"))
        XCTAssertEqual(wire["service_tier"], .null)
        XCTAssertEqual(wire["allow_paid_speed"], .bool(false))
    }

    func testPreferencesRoundTripAndDoNotCreateHalfAModelPin() {
        let preferences = RuntimeSelectionPreferences(provider: "sample", model: "sample-astra", reasoningEffort: "high", serviceTier: "rush", allowPaidSpeed: true)
        XCTAssertEqual(RuntimeSelectionPreferences.parse(preferences.wire), preferences)
        XCTAssertFalse(RuntimeSelectionPreferences.parse(.null).allowPaidSpeed)
        XCTAssertNil(RuntimeSelectionPreferences(provider: "codex").provider)
        XCTAssertNil(RuntimeSelectionPreferences(model: "orphan").model)
    }

    func testSelectedLabelIsHonestForPartialPinsAndNormalSpeed() {
        XCTAssertEqual(RuntimeSelectionLabel.compact(.init(), catalog: catalog), "Neko decides")
        XCTAssertEqual(RuntimeSelectionLabel.compact(.init(reasoningEffort: "high"), catalog: catalog), "Neko decides · High")
        let manual = RuntimeSelectionPreferences(provider: "sample", model: "sample-astra", reasoningEffort: "high")
        XCTAssertEqual(RuntimeSelectionLabel.compact(manual, catalog: catalog), "Astra · High")
        XCTAssertEqual(RuntimeSelectionLabel.compact(.init(serviceTier: "default"), catalog: catalog), "Neko decides · Normal")
        XCTAssertFalse(RuntimeSelectionPreferences(serviceTier: "default").requestsExtraSpeed)
        XCTAssertTrue(RuntimeSelectionPreferences(serviceTier: "rush").requestsExtraSpeed)
        XCTAssertEqual(RuntimeSelectionLabel.speed(.init(serviceTier: "rush"), catalog: catalog), "Express")
    }

    func testHistoryUsesLastMatchingDispatchAndDetectsPreferenceChange() throws {
        let snapshot = try json("""
        {"conversation_runtime":{"task:a":{"reasoning_effort":"high","allow_paid_speed":false}},"runtime_selections":[
          {"conversation_id":"task:a","run_id":"old","runtime":{"provider":"codex","model":"m","service_tier":"priority"},"preferences":{},"reason":"Old"},
          {"conversation_id":"task:a","run_id":"new","runtime":{"provider":"codex","model":"m","reasoning_effort":"low","service_tier":"default"},"preferences":{},"reason":"Small task","automatic":true,"selected_at_ms":123,"catalog_read_at_ms":100},
          {"conversation_id":"task:b","run_id":"other","runtime":{},"preferences":{}}
        ]}
        """)
        let history = try XCTUnwrap(RuntimeSelectionHistory.latest(in: snapshot, conversationID: "task:a"))
        XCTAssertEqual(history.runID, "new")
        XCTAssertEqual(history.runtime.serviceTier, "default")
        XCTAssertEqual(history.reason, "Small task")
        XCTAssertTrue(history.automatic)
        XCTAssertEqual(history.selectedAtMS, 123)
        XCTAssertEqual(history.catalogReadAtMS, 100)
        XCTAssertTrue(history.differs(from: .saved(in: snapshot, conversationID: "task:a")))
        XCTAssertFalse(history.differs(from: .init()))
        XCTAssertNil(RuntimeSelectionHistory.latest(in: snapshot, conversationID: "task:missing"))
    }

    func testSampleVariantsShareDraftAndRemainLocal() throws {
        let preview = DesignLabModel()
        XCTAssertFalse(preview.runtime.saved.allowPaidSpeed)
        preview.runtime.draft.reasoningEffort = "high"
        preview.layout = .separate
        XCTAssertEqual(preview.runtime.draft.reasoningEffort, "high")
        preview.runtime.applySample()
        XCTAssertEqual(preview.runtime.saved.reasoningEffort, "high")
        XCTAssertEqual(preview.runtime.status, "Sample settings applied.")
        preview.draft = "Sample message"
        preview.send()
        XCTAssertEqual(preview.messages, ["Sample message"])
        XCTAssertEqual(preview.runtime.saved.reasoningEffort, "high")
        preview.reset()
        XCTAssertEqual(preview.runtime.saved, .init())
        XCTAssertEqual(preview.runtime.draft, .init())
    }

    func testLoadingFailureStaysVisibleAndRefreshRecovers() async {
        let state = RuntimeSelectionState(conversationID: "task:a")
        await state.load { throw NSError(domain: "Test", code: 1, userInfo: [NSLocalizedDescriptionKey: "Offline"]) }
        XCTAssertFalse(state.loading)
        XCTAssertFalse(state.loaded)
        XCTAssertEqual(state.catalogError, "Offline")
        await state.load { catalog }
        XCTAssertTrue(state.loaded)
        XCTAssertNil(state.catalogError)
        XCTAssertEqual(state.extraSpeedOptions.map(\.id), ["priority", "rush"])
    }

    func testPollingPreservesAnUnappliedDraftAndDiscardRestoresLatestSaved() {
        let state = RuntimeSelectionState(conversationID: "task:a", catalog: catalog)
        state.draft.reasoningEffort = "high"
        state.sync(.init(serviceTier: "default"), revision: 1)
        XCTAssertEqual(state.draft.reasoningEffort, "high")
        XCTAssertNil(state.draft.serviceTier)
        XCTAssertEqual(state.saved.serviceTier, "default")
        state.discard()
        XCTAssertEqual(state.draft, state.saved)
        state.draft.allowPaidSpeed = true
        state.reset()
        XCTAssertFalse(state.draft.allowPaidSpeed)
        XCTAssertTrue(state.draft.isAutomatic)
    }
}

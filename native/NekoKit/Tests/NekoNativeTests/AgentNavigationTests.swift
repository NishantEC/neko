import XCTest
import NekoKit
@testable import NekoNative

@MainActor final class AgentNavigationTests: XCTestCase {
    func testAgentSwitchKeepsDraftAndReturnFilter() {
        let model = AppModel { _ in .null }
        model.agentFilter = TaskStage.needsYou.rawValue
        model.agentSearch = "login"
        model.includeStoppedAgents = false
        model.ticketDrafts["a"] = "Use the staging environment"
        model.openAgent("a", from: .tickets)
        model.openAgent("b")
        XCTAssertEqual(model.agentReturnPage, .tickets)
        model.closeAgent()
        XCTAssertNil(model.agentID)
        XCTAssertEqual(model.requestedPage, .tickets)
        XCTAssertEqual(model.agentFilter, TaskStage.needsYou.rawValue)
        XCTAssertEqual(model.agentSearch, "login")
        XCTAssertFalse(model.includeStoppedAgents)
        XCTAssertEqual(model.ticketDrafts["a"], "Use the staging environment")
    }
    func testExplicitAllAgentsClearsGroupWhileMemoryReturnStaysInMemory() {
        let model = AppModel { _ in .null }
        model.agentFilter = TaskStage.needsYou.rawValue
        model.showAgents()
        XCTAssertEqual(model.agentFilter, "all")
        model.openAgent("a", from: .memory)
        model.closeAgent()
        XCTAssertEqual(model.requestedPage, .memory)
    }
    func testEveryTaskLandsInExactlyOneStage() {
        let statuses = ["AwaitingApproval", "ReadyForReview", "Failed", "Queued", "Planning", "Building", "Reviewing", "Completed", "Cancelled"]
        let stages = statuses.map(TaskStage.init(status:))
        XCTAssertEqual(stages.filter { $0 == .needsYou }.count, 2)
        XCTAssertEqual(stages.filter { $0 == .running }.count, 3)
        XCTAssertEqual(stages.filter { $0 == .upNext }.count, 1)
        XCTAssertEqual(stages.count, statuses.count)
    }
    func testDecisionCorrectionRevisionReplacesOnlyItsEpisode() {
        func record(_ id: String, _ episode: String, _ version: Int) -> JSONValue {
            .object(["id": .string(id), "episode_id": .string(episode), "version": .number(Double(version)), "created_at_ms": .number(Double(version))])
        }
        let latest = DecisionPresentation.latest([record("a", "first", 1), record("b", "first", 2), record("c", "second", 1)])
        XCTAssertEqual(latest.map(\.recordID), ["b", "c"])
    }
    func testPreferenceMutationCarriesWorkspaceAndCurrentVersion() {
        let item: JSONValue = .object(["id": .string("p"), "workspace_id": .string("w"), "version": .number(3)])
        let body = DecisionPresentation.preferenceCommand("KeepPreference", item)["DecisionContext"]["KeepPreference"]
        XCTAssertEqual(body["workspace_id"], .string("w"))
        XCTAssertEqual(body["expected_version"], .number(3))
    }
    func testReplyAndNewPlanSupersedeTheEarlierQuestion() {
        let ticket: JSONValue = .object(["status": .string("AwaitingApproval"), "events": .array([
            .object(["role": .string("supervisor"), "message": .string("Needs your input before building: Which tenant?")]),
            .object(["role": .string("note"), "message": .string("Use staging")]),
            .object(["role": .string("supervisor"), "message": .string("Plan ready.")])
        ])])
        XCTAssertNil(TicketPresentation.waitingReason(ticket))
    }
    func testRecoverySupervisorQuestionIsVisibleAndNewPlanSupersedesIt() {
        let question: JSONValue = .object(["role": .string("coordinator"), "message": .string("Needs your input before building: Which deployment?\nTried: inspected local configuration")])
        let ticket: JSONValue = .object(["status": .string("AwaitingApproval"), "events": .array([question])])
        XCTAssertEqual(TicketPresentation.waitingReason(ticket), "Needs your input: Which deployment?\nTried: inspected local configuration")
        let planned: JSONValue = .object(["status": .string("AwaitingApproval"), "events": .array([question, .object(["role": .string("supervisor"), "message": .string("Plan ready.")])])])
        XCTAssertNil(TicketPresentation.waitingReason(planned))
    }
    func testQuestionAndNoWorkDoNotReadAsBuildablePlan() {
        for message in ["Needs your input before building: Which tenant?", "Nothing to build: Already fixed"] {
            let ticket: JSONValue = .object(["status": .string("AwaitingApproval"), "events": .array([.object(["role": .string("supervisor"), "message": .string(message)])])])
            XCTAssertNotNil(TicketPresentation.waitingReason(ticket))
        }
    }
}

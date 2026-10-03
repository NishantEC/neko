import XCTest
import NekoKit
@testable import NekoNative

@MainActor final class AgentNavigationTests: XCTestCase {
    func testAgentSwitchKeepsDraftAndReturnFilter() {
        let model = AppModel { _ in .null }
        model.agentFilter = "needs_you"
        model.agentSearch = "login"
        model.includeStoppedAgents = false
        model.ticketDrafts["a"] = "Use the staging environment"
        model.openAgent("a", from: "Tickets")
        model.openAgent("b")
        XCTAssertEqual(model.agentReturnPage, "Tickets")
        model.closeAgent()
        XCTAssertNil(model.agentID)
        XCTAssertEqual(model.requestedPage, "Tickets")
        XCTAssertEqual(model.agentFilter, "needs_you")
        XCTAssertEqual(model.agentSearch, "login")
        XCTAssertFalse(model.includeStoppedAgents)
        XCTAssertEqual(model.ticketDrafts["a"], "Use the staging environment")
    }
    func testSidebarGroupsAreDisjointAndNewestFirst() {
        let rows: [JSONValue] = ["AwaitingApproval", "ReadyForReview", "Failed", "Queued", "Planning", "Building", "Reviewing", "Completed", "Cancelled"].enumerated().map {
            .object(["id": .string("\($0.offset)"), "status": .string($0.element), "updated_at_ms": .number(Double($0.offset))])
        }
        let groups = AgentSidebarGroup.allCases.map { $0.tasks(in: rows) }
        XCTAssertEqual(groups.map(\.count), [3, 4, 2])
        XCTAssertEqual(Set(groups.flatMap { $0.map(\.recordID) }).count, rows.count)
        XCTAssertEqual(groups[0].map(\.recordID), ["2", "1", "0"])
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
    func testQuestionAndNoWorkDoNotReadAsBuildablePlan() {
        for message in ["Needs your input before building: Which tenant?", "Nothing to build: Already fixed"] {
            let ticket: JSONValue = .object(["status": .string("AwaitingApproval"), "events": .array([.object(["role": .string("supervisor"), "message": .string(message)])])])
            XCTAssertNotNil(TicketPresentation.waitingReason(ticket))
        }
    }
}

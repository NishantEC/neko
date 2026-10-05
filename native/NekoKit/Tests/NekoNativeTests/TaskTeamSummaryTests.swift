import XCTest
import NekoKit
@testable import NekoNative

final class TaskTeamSummaryTests: XCTestCase {
    private func task(_ id: String, _ status: String) -> JSONValue {
        .object(["id": .string(id), "status": .string(status)])
    }
    private func split(_ parent: String = "owner", approved: Bool = true, ids: [String?]) -> JSONValue {
        .object(["parent_id": .string(parent), "approved": .bool(approved),
                 "subtasks": .array(ids.map { .object(["task_id": $0.map(JSONValue.string) ?? .null]) })])
    }
    private func summary(tasks: [JSONValue], splits: [JSONValue]) -> TaskTeamSummary {
        TaskTeamSummary(parentID: "owner", snapshot: .object(["tasks": .array(tasks), "splits": .array(splits)]))
    }

    func testSingleTaskPhasesNeverBecomeThreeAgents() {
        for status in ["Planning", "Building", "Reviewing"] {
            let team = summary(tasks: [task("owner", status)], splits: [])
            XCTAssertEqual(team.label, "Neko")
            XCTAssertTrue(team.children.isEmpty)
        }
    }
    func testOnlyExistingApprovedChildrenBelongToThisTask() {
        let team = summary(tasks: [task("owner", "Queued"), task("a", "Building"), task("b", "Queued"), task("other", "Building")],
                           splits: [split(ids: ["a", "a", "b", "owner", "missing", nil]),
                                    split("elsewhere", ids: ["other"]), split(approved: false, ids: ["other"])])
        XCTAssertEqual(team.children.map { $0["id"].string }, ["a", "b"])
        XCTAssertEqual(team.label, "2 subtasks")
        XCTAssertEqual(team.working, 1)
        XCTAssertEqual(team.waiting, 1)
    }
    func testProposalDoesNotImplyWorkersExist() {
        XCTAssertTrue(summary(tasks: [task("a", "Building")], splits: [split(approved: false, ids: ["a"])]).children.isEmpty)
        XCTAssertTrue(summary(tasks: [], splits: [split(ids: [nil, "missing"])]).children.isEmpty)
    }
    func testSettledAndWaitingTasksAreNotCountedAsWorking() {
        let states = ["Planning", "Building", "Reviewing", "Queued", "AwaitingApproval", "ReadyForReview", "Completed", "Failed", "Cancelled", "Unknown"]
        let team = summary(tasks: states.map { task($0, $0) }, splits: [split(ids: states)])
        XCTAssertEqual(team.working, 3)
        XCTAssertEqual(team.waiting, 1)
        XCTAssertEqual(team.needsYou, 2)
        XCTAssertEqual(team.completed, 1)
        XCTAssertEqual(team.stopped, 2)
    }
}

import XCTest
import NekoKit
@testable import NekoNative

final class TicketConversationPresentationTests: XCTestCase {
    private func ticket(_ status: String, plan: String = "Earlier plan", result: String = "Earlier result") -> JSONValue {
        .object(["status": .string(status), "plan": .string(plan), "result": .string(result)])
    }

    func testSettledOutcomeFoldsHistoryAndOldPlan() {
        for status in ["ReadyForReview", "Completed"] {
            let presentation = TicketPresentation.conversation(ticket(status))
            XCTAssertTrue(presentation.foldsHistory)
            XCTAssertFalse(presentation.showsPlan)
            XCTAssertFalse(presentation.outcome.belongsInHistory)
            XCTAssertTrue(presentation.outcome.reviewIsCurrent)
        }
    }

    func testPreservedResultAfterReplyDoesNotReadAsCurrentSuccess() {
        let passedResult = "Earlier result\n\nIndependent review:\n{\"passed\":true,\"findings\":[],\"files\":[\"a.swift\"],\"tests\":[\"swift test\"],\"summary\":\"Earlier checks passed\"}"
        XCTAssertEqual(TicketPresentation.review(passedResult).verdict?.passed, true)
        for status in ["Queued", "Planning", "Building", "AwaitingApproval", "Unknown"] {
            let presentation = TicketPresentation.conversation(ticket(status, result: passedResult))
            XCTAssertTrue(presentation.outcome.belongsInHistory, status)
            XCTAssertFalse(presentation.outcome.reviewIsCurrent, status)
            XCTAssertFalse(presentation.foldsHistory, status)
        }
    }

    func testPlanAndNeededQuestionRemainAvailableWhileWaiting() {
        var waiting = ticket("AwaitingApproval", plan: "Investigate the correct tenant").object
        waiting["supervision"] = .object(["action": .string("AskUser"), "reason": .string("Which tenant?")])
        let task = JSONValue.object(waiting)
        XCTAssertTrue(TicketPresentation.conversation(task).showsPlan)
        XCTAssertEqual(TicketPresentation.waitingReason(task), "Needs your input: Which tenant?")
        let planning = TicketPresentation.conversation(ticket("Planning"))
        XCTAssertTrue(planning.showsPlan)
        XCTAssertEqual(planning.planTitle, "Saved plan · being revised")
        XCTAssertFalse(TicketPresentation.conversation(ticket("Planning", plan: " \n ")).showsPlan)
    }

    func testReviewingAndStoppedOutputNeverInheritEarlierPass() {
        for status in ["Reviewing", "Failed", "Cancelled"] {
            let presentation = TicketPresentation.conversation(ticket(status))
            XCTAssertFalse(presentation.outcome.belongsInHistory)
            XCTAssertFalse(presentation.outcome.reviewIsCurrent)
            XCTAssertFalse(presentation.showsPlan)
        }
        XCTAssertEqual(TicketPresentation.conversation(ticket("Reviewing")).outcome, .reviewing)
        XCTAssertTrue(TicketPresentation.conversation(ticket("Failed")).foldsHistory)
        XCTAssertTrue(TicketPresentation.conversation(ticket("Cancelled")).foldsHistory)
    }
}

import XCTest
@testable import NekoNative

final class WorkBoardTests: XCTestCase {
    func testDraggingAcrossColumnsMapsToTheRealTicketCommand() {
        XCTAssertEqual(TicketsView.moveCommand(from: "AwaitingApproval", to: "working").command, "ApproveTask")
        XCTAssertEqual(TicketsView.moveCommand(from: "Failed", to: "working").command, "RetryTask")
        XCTAssertEqual(TicketsView.moveCommand(from: "ReadyForReview", to: "done").command, "CompleteTask")
        // Running work dropped on Done is a cancel, which the board confirms first.
        XCTAssertEqual(TicketsView.moveCommand(from: "Building", to: "done").command, "CancelTask")
    }
    func testMovesNekoOwnsAreRefusedWithAReason() {
        let back = TicketsView.moveCommand(from: "Building", to: "approval")
        XCTAssertNil(back.command)
        XCTAssertNotNil(back.reason)
        XCTAssertNil(TicketsView.moveCommand(from: "AwaitingApproval", to: "review").command)
        XCTAssertNil(TicketsView.moveCommand(from: "Completed", to: "approval").command)
    }
}

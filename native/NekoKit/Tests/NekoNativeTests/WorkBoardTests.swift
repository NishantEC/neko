import XCTest
@testable import NekoNative
import NekoKit

final class WorkBoardTests: XCTestCase {
    func testDraggingAcrossColumnsMapsToTheRealTicketCommand() {
        // Dropping on Working starts the ticket: approve, or retry and build once planned.
        XCTAssertEqual(TicketsView.moveCommand(from: "AwaitingApproval", to: "working").command, "StartTask")
        XCTAssertEqual(TicketsView.moveCommand(from: "Failed", to: "working").command, "StartTask")
        XCTAssertEqual(TicketsView.moveCommand(from: "ReadyForReview", to: "done").command, "CompleteTask")
        // Running work dropped on Done is a cancel, which the board confirms first.
        XCTAssertEqual(TicketsView.moveCommand(from: "Building", to: "done").command, "CancelTask")
    }
    func testTicketHistoryReadsAsAConversation() throws {
        let task = try JSONDecoder().decode(JSONValue.self, from: Data("""
        {"goal":"Fix the login crash","plan":"1. Guard the profile call","result":"","events":[
          {"role":"system","message":"Queued for planning."},
          {"role":"scout","message":"Working in /repo/athena: its name appears in the ticket."},
          {"role":"supervisor","message":"Command in_progress"},
          {"role":"supervisor","message":"VERIFICATION_COMMAND {\\"command\\":\\"/bin/zsh -lc 'rg -n login'\\",\\"exit_code\\":0}"},
          {"role":"supervisor","message":"VERIFICATION_COMMAND {\\"command\\":\\"/bin/zsh -lc 'nl -ba a.ts'\\",\\"exit_code\\":0}"},
          {"role":"supervisor","message":"I traced the crash to an unguarded profile call."},
          {"role":"supervisor","message":"1. Guard the profile call"},
          {"role":"supervisor","message":"Needs your input before building: Which tenant reproduces it?"},
          {"role":"note","message":"Use staging tenant 42"}
        ]}
        """.utf8))
        XCTAssertEqual(TicketThread.items(task), [
            .brief("Fix the login crash"),
            .status("Queued for planning."),
            .status("Working in /repo/athena: its name appears in the ticket."),
            .steps(role: "supervisor", commands: ["rg -n login", "nl -ba a.ts"]),
            .agent(role: "supervisor", text: "I traced the crash to an unguarded profile call."),
            .you("Use staging tenant 42")
        ])
        XCTAssertEqual(TicketThread.agentName("supervisor"), "Planner")
    }
    func testMovesNekoOwnsAreRefusedWithAReason() {
        let back = TicketsView.moveCommand(from: "Building", to: "approval")
        XCTAssertNil(back.command)
        XCTAssertNotNil(back.reason)
        XCTAssertNil(TicketsView.moveCommand(from: "AwaitingApproval", to: "review").command)
        XCTAssertNil(TicketsView.moveCommand(from: "Completed", to: "approval").command)
    }
}

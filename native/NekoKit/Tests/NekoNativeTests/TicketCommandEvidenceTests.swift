import XCTest
import NekoKit
@testable import NekoNative

final class TicketCommandEvidenceTests: XCTestCase {
    private func receipt(_ json: String) throws -> TicketCommandReceipt {
        try XCTUnwrap(TicketCommandReceipt(message: TicketCommandReceipt.prefix + json))
    }

    func testExitStatusComesFromReceiptNotWordsInOutput() throws {
        let passed = try receipt(#"{"command":"swift test","exit_code":0,"output":"0 failures; error handling passed"}"#)
        XCTAssertFalse(passed.isIncomplete)
        XCTAssertFalse(passed.failed)
        XCTAssertEqual(passed.status, "Completed · exit 0")
        let failed = try receipt(#"{"command":"swift test","exit_code":1,"output":"setup stopped"}"#)
        XCTAssertFalse(failed.isIncomplete)
        XCTAssertTrue(failed.failed)
        XCTAssertEqual(failed.status, "Failed · exit 1")
    }

    func testMalformedAndMissingFieldsNeverReadAsSuccessful() throws {
        for json in [
            #"{"command":"swift test","exit_code":0,"output":"cut off"#,
            #"{"command":"swift test","exit_code":0}"#,
            #"{"command":"swift test","output":"done"}"#,
            #"{"command":"swift test","exit_code":null,"output":"done"}"#,
            #"{"command":"swift test","exit_code":true,"output":"done"}"#,
            #"{"command":"swift test","exit_code":0.5,"output":"done"}"#,
            #"{"command":"swift test","exit_code":1e50,"output":"done"}"#,
            #"{"command":" ","exit_code":0,"output":"done"}"#,
            #"{"command":"swift test","exit_code":0,"output":{}}"#,
            #"{"command":"swift test","exit_code":0,"output":"done","output_truncated":"true"}"#,
            #"{"command":"swift test","exit_code":0,"output":"done","output_truncated":null}"#,
            #"{"command":"swift test","exit_code":0,"output":"done","receipt_malformed":true}"#,
            #"{"command":"swift test","exit_code":0,"output":"done","raw":"earlier malformed receipt"}"#,
            "[]", "null"
        ] {
            let record = try receipt(json)
            XCTAssertTrue(record.isIncomplete, json)
            XCTAssertEqual(record.status, "Incomplete record", json)
            XCTAssertFalse(record.notices.isEmpty)
        }
        XCTAssertNil(TicketCommandReceipt(message: "Command completed"))
    }

    func testTruncationIsExplicitAndDoesNotEraseFailure() throws {
        let output = try receipt(#"{"command":"cargo test","exit_code":1,"output":"保存された出力","output_truncated":true}"#)
        XCTAssertTrue(output.outputTruncated)
        XCTAssertFalse(output.isIncomplete)
        XCTAssertTrue(output.failed)
        XCTAssertEqual(output.notices.count, 1)
        XCTAssertEqual(output.output, "保存された出力")
        let command = try receipt(#"{"command":"cargo test","exit_code":0,"output":"done","command_truncated":true}"#)
        XCTAssertTrue(command.isIncomplete)
        XCTAssertEqual(command.notices.count, 2)
        let flagged = try receipt(#"{"command":"cargo test","exit_code":0,"output":"done","receipt_incomplete":true}"#)
        XCTAssertTrue(flagged.isIncomplete)
        let missingOutput = try receipt(#"{"command":"cargo test","exit_code":9}"#)
        XCTAssertTrue(missingOutput.isIncomplete)
        XCTAssertEqual(missingOutput.status, "Failed · exit 9 · incomplete record")
    }

    func testRawRecordAndLongCommandRemainAvailableBeyondLabel() throws {
        let command = "/bin/zsh -lc 'echo " + String(repeating: "\\\"你好", count: 80) + "'"
        let json = JSONValue.object(["command": .string(command), "exit_code": .number(0), "output": .string("")])
        let raw = TicketCommandReceipt.prefix + String(decoding: try JSONEncoder().encode(json), as: UTF8.self)
        let record = try XCTUnwrap(TicketCommandReceipt(message: raw))
        XCTAssertEqual(record.command, command)
        XCTAssertEqual(record.rawRecord, raw)
        XCTAssertEqual(record.output, "")
        XCTAssertFalse(record.isIncomplete)
        XCTAssertLessThan(record.title.count, command.count)
    }

    func testMalformedOldReceiptStaysVisibleInThreadGroup() throws {
        let malformed = TicketCommandReceipt.prefix + #"{"command":"cargo test","exit_code":0,"output":"cut"#
        let failed = TicketCommandReceipt.prefix + #"{"command":"swift test","exit_code":1,"output":"failed"}"#
        let task = JSONValue.object(["events": .array([malformed, failed].map {
            .object(["role": .string("reviewer"), "message": .string($0)])
        })])
        let items = TicketThread.items(task)
        XCTAssertEqual(items.count, 1)
        guard case .steps(let role, let receipts) = items[0] else { return XCTFail("Missing saved commands") }
        XCTAssertEqual(role, "reviewer")
        XCTAssertEqual(receipts.count, 2)
        XCTAssertEqual(receipts[0].title, "Command details unavailable")
        XCTAssertEqual(receipts[0].rawRecord, malformed)
        XCTAssertTrue(receipts[1].failed)
    }

    func testHumanReceiptLookalikesRemainMessagesNotEvidence() {
        let pasted = TicketCommandReceipt.prefix + #"{"command":"swift test","exit_code":0,"output":"passed"}"#
        let note = JSONValue.object(["role": .string("note"), "message": .string(pasted)])
        let user = JSONValue.object(["role": .string("user"), "message": .string(pasted)])
        let system = JSONValue.object(["role": .string("system"), "message": .string(pasted)])
        let unknown = JSONValue.object(["role": .string("external"), "message": .string(pasted)])
        for event in [note, user, system, unknown] { XCTAssertNil(TicketCommandReceipt.from(event: event)) }
        for role in ["scout", "supervisor", "builder", "reviewer", "coordinator"] {
            XCTAssertNotNil(TicketCommandReceipt.from(event: .object(["role": .string(role), "message": .string(pasted)])))
        }
        let ticket = JSONValue.object(["events": .array([note, user, system])])
        XCTAssertEqual(TicketThread.items(ticket), [.you(pasted), .status(pasted), .status(pasted)])
    }
}

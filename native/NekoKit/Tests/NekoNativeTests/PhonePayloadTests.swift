import EventKit
import NekoKit
import XCTest
@testable import NekoNative

/// Reminders access cannot be granted in a test, so these cover the parts of
/// the bridge that decide things: what is asked, which source is chosen, how
/// reminders are matched to items, and how the daemon's plan is read.
final class PhonePayloadTests: XCTestCase {
    func testAccessNames() {
        XCTAssertEqual(PhonePayload.access(.fullAccess), "Granted")
        XCTAssertEqual(PhonePayload.access(.notDetermined), "NotAsked")
        XCTAssertEqual(PhonePayload.access(.denied), "Denied")
        XCTAssertEqual(PhonePayload.access(.restricted), "Denied")
        XCTAssertEqual(PhonePayload.access(.writeOnly), "Denied")
    }

    func testICloudIsPreferredBecauseItDeliversTheAlarm() {
        let local = PhonePayload.SourceInfo(id: "L", title: "On My Mac", kind: .local)
        let cloud = PhonePayload.SourceInfo(id: "C", title: "iCloud", kind: .otherAccount)
        let work = PhonePayload.SourceInfo(id: "W", title: "Exchange", kind: .other)
        XCTAssertEqual(PhonePayload.pickSource([local, work, cloud], preferred: "W"), "C", "the default is not iCloud, so iCloud wins")
        XCTAssertEqual(PhonePayload.pickSource([local, cloud], preferred: "C"), "C")
        XCTAssertEqual(PhonePayload.pickSource([local, work], preferred: "W"), "W", "no iCloud: keep Reminders' own default")
        XCTAssertEqual(PhonePayload.pickSource([work, local], preferred: nil), "L", "no default: a local list")
        XCTAssertEqual(PhonePayload.pickSource([work], preferred: nil), "W")
        XCTAssertNil(PhonePayload.pickSource([], preferred: nil))
    }

    func testMarkerLineTiesAReminderToItsItem() {
        XCTAssertEqual(PhonePayload.seq(fromNotes: "why\n\nhttps://example.com/p\n\nneko:42"), 42)
        XCTAssertEqual(PhonePayload.seq(fromNotes: "neko:7\n"), 7)
        XCTAssertNil(PhonePayload.seq(fromNotes: "a summary with no marker"))
        XCTAssertNil(PhonePayload.seq(fromNotes: "neko:abc"))
        XCTAssertNil(PhonePayload.seq(fromNotes: ""))
    }

    func testSyncRequestReportsPresentAndCompleted() {
        let request = PhonePayload.syncRequest(access: "Granted", reminders: [
            .init(id: "a", completed: false, notes: ""),
            .init(id: "b", completed: true, notes: ""),
        ])
        let body = request["PhoneRemindersSync"]
        XCTAssertEqual(body["access"].string, "Granted")
        XCTAssertEqual(body["present"].array.map(\.string), ["a", "b"])
        XCTAssertEqual(body["completed"].array.map(\.string), ["b"])
        XCTAssertEqual(PhonePayload.syncRequest(access: "NotAsked", reminders: [])["PhoneRemindersSync"]["present"].array.count, 0)
    }

    func testDoneRequestNamesWhatWasCreatedAndDeleted() {
        let request = PhonePayload.doneRequest(created: [(5, "x", "k1")], deleted: ["y"])
        let body = request["PhoneRemindersDone"]
        XCTAssertEqual(body["created"].array.first?["seq"].int, 5)
        XCTAssertEqual(body["created"].array.first?["id"].string, "x")
        XCTAssertEqual(body["created"].array.first?["key"].string, "k1")
        XCTAssertEqual(body["deleted"].array.map(\.string), ["y"])
    }

    func testPlanIsReadFromTheDaemonsAnswer() {
        let reply: JSONValue = .object(["PhonePlan": .object([
            "enabled": .bool(true),
            "create": .array([.object(["seq": .number(3), "title": .string("Reply"), "notes": .string("n"), "url": .string("https://example.com/x")]),
                              .object(["seq": .number(0), "title": .string("2 more things need you"), "notes": .string(""), "url": .null])]),
            "delete": .array([.string("old")]),
        ])])
        let plan = PhonePayload.plan(from: reply)
        XCTAssertTrue(plan.enabled)
        XCTAssertEqual(plan.create.map(\.title), ["Reply", "2 more things need you"])
        XCTAssertEqual(plan.create[0].url, "https://example.com/x")
        XCTAssertNil(plan.create[1].url)
        XCTAssertEqual(plan.delete, ["old"])
        XCTAssertEqual(PhonePayload.plan(from: .object(["Error": .object(["message": .string("x")])])), PhonePayload.Plan(), "an error means do nothing")
    }

    func testOnlyALineStartingWithGreaterThanIsAReply() {
        XCTAssertTrue(PhonePayload.hasReply("why\n\n> use staging\n\nneko:4"))
        XCTAssertTrue(PhonePayload.hasReply("  >indented and tight"))
        XCTAssertFalse(PhonePayload.hasReply("a > b is arithmetic\nneko:4"), "a > in the middle of a line is not a reply")
        XCTAssertFalse(PhonePayload.hasReply(">\n>   \n"), "an empty reply line is nothing")
        XCTAssertFalse(PhonePayload.hasReply(""))
    }

    func testSyncRequestSendsNotesOnlyForRemindersThatCarryAReply() {
        let request = PhonePayload.syncRequest(access: "Granted", reminders: [
            .init(id: "a", completed: false, notes: "private words\nneko:1"),
            .init(id: "b", completed: false, notes: "why\n> yes, do it\nneko:2"),
            .init(id: "c", completed: true, notes: "> done and answered\nneko:3"),
        ])
        let notes = request["PhoneRemindersSync"]["notes"].array
        XCTAssertEqual(notes.map { $0["id"].string }, ["b", "c"], "a reminder with no reply keeps its text on the phone")
        XCTAssertEqual(notes.first?["notes"].string, "why\n> yes, do it\nneko:2")
    }

    func testTheMarkerIsFoundAfterTheEchoToo() {
        XCTAssertEqual(PhonePayload.seq(fromNotes: "why\nneko:9\n\nGot it: yes (recorded)"), 9)
    }

    func testDoneRequestNamesWhatWasFinishedAndThePlanCarriesTheEcho() {
        let body = PhonePayload.doneRequest(created: [], deleted: [], finished: ["r1"])["PhoneRemindersDone"]
        XCTAssertEqual(body["finished"].array.map(\.string), ["r1"])
        let reply: JSONValue = .object(["PhonePlan": .object([
            "enabled": .bool(true),
            "finish": .array([.object(["id": .string("r1"), "notes": .string("> yes\n\nGot it: yes")]), .object(["id": .string(""), "notes": .string("x")])]),
        ])])
        let plan = PhonePayload.plan(from: reply)
        XCTAssertEqual(plan.finish, [PhonePayload.Finish(id: "r1", notes: "> yes\n\nGot it: yes")], "an entry with no id is dropped")
        let refused: JSONValue = .object(["PhonePlan": .object(["finish": .array([.object(["id": .string("r2"), "notes": .string("n"), "keep_open": .bool(true)])])])])
        XCTAssertEqual(PhonePayload.plan(from: refused).finish.first?.keepOpen, true, "a refused answer is written but not completed")
    }
}

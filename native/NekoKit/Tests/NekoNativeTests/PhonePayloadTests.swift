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
}

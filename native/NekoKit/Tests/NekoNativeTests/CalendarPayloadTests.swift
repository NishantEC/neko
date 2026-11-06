import AppKit
import EventKit
import NekoKit
import XCTest
@testable import NekoNative

/// EventKit access cannot be granted in a test, so these cover what the
/// bridge sends once it has events: the request shape, links and colours.
final class CalendarPayloadTests: XCTestCase {
    func testAccessNames() {
        XCTAssertEqual(CalendarPayload.access(.fullAccess), "Granted")
        XCTAssertEqual(CalendarPayload.access(.notDetermined), "NotAsked")
        XCTAssertEqual(CalendarPayload.access(.denied), "Denied")
        XCTAssertEqual(CalendarPayload.access(.restricted), "Denied")
        XCTAssertEqual(CalendarPayload.access(.writeOnly), "Denied", "write-only access cannot read events")
    }

    func testWindowIsSevenDaysBackToThirtyFiveAhead() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let now = Date(timeIntervalSince1970: 1_793_000_000) // a Wednesday afternoon
        let (start, end) = CalendarPayload.window(around: now, calendar: calendar)
        let today = calendar.startOfDay(for: now)
        XCTAssertEqual(start, today.addingTimeInterval(-7 * 86_400))
        XCTAssertEqual(end, today.addingTimeInterval(35 * 86_400))
    }

    func testColoursAreSRGBHex() {
        XCTAssertEqual(CalendarPayload.hex(NSColor(calibratedWhite: 1, alpha: 1)), "#FFFFFF")
        XCTAssertEqual(CalendarPayload.hex(NSColor(calibratedWhite: 0, alpha: 1)), "#000000")
        XCTAssertEqual(CalendarPayload.hex(nil), "")
    }

    func testMeetingLinks() {
        XCTAssertEqual(CalendarPayload.link(url: URL(string: "https://example.com/e/1"), location: nil, notes: nil), "https://example.com/e/1")
        XCTAssertEqual(
            CalendarPayload.link(url: nil, location: "Room 4", notes: "Agenda https://example.com/doc\nJoin: https://us02web.zoom.us/j/123?pwd=x"),
            "https://us02web.zoom.us/j/123?pwd=x", "a meeting link wins over another link")
        XCTAssertEqual(CalendarPayload.link(url: nil, location: "https://meet.google.com/abc-defg-hij", notes: nil), "https://meet.google.com/abc-defg-hij")
        XCTAssertEqual(CalendarPayload.link(url: nil, location: "https://example.com/room", notes: nil), "https://example.com/room")
        XCTAssertNil(CalendarPayload.link(url: URL(string: "message:abc"), location: "Office", notes: "Bring the slides"))
        XCTAssertNil(CalendarPayload.link(url: nil, location: "Office, see https://example.com/map", notes: nil), "a link inside a place name is not the event's link")
    }

    func testRequestShapeMatchesTheProtocol() {
        let start = Date(timeIntervalSince1970: 1_000)
        let item = CalendarPayload.Item(id: "e@1000", calendar: "Work", color: "#336699", title: "Standup", start: start, end: start.addingTimeInterval(900), allDay: false, location: nil, url: "https://meet.google.com/x")
        let request = CalendarPayload.request(items: [item], access: "Granted")
        let body = request["SetCalendarEvents"]
        XCTAssertEqual(body["source"].string, "macOS Calendar")
        XCTAssertEqual(body["access"].string, "Granted")
        let event = body["events"].array.first ?? .null
        XCTAssertEqual(event["start_ms"].int, 1_000_000)
        XCTAssertEqual(event["end_ms"].int, 1_900_000)
        XCTAssertEqual(event["all_day"].bool, false)
        XCTAssertEqual(event["url"].string, "https://meet.google.com/x")
        XCTAssertNil(event.object["location"], "absent fields are left out, not sent empty")
    }
}

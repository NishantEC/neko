import AppKit
import EventKit
import NekoKit

/// Sends the person's macOS Calendar to the daemon for the Calendar page.
/// EventKit already holds every account added in System Settings (Google,
/// iCloud, Exchange), so there is nothing to set up beyond allowing access,
/// which is asked once. Every 5 minutes, and 2 seconds after Calendar reports
/// a change, the events from 7 days ago to 35 days ahead are sent with
/// `SetCalendarEvents`; the daemon writes nothing when they are unchanged.
/// See docs/knowledge/calendar.md.
@MainActor final class CalendarBridge {
    static let shared = CalendarBridge()
    private static let interval: Duration = .seconds(300)
    private static let settle: Duration = .seconds(2)
    /// How often to look for the switch while Calendar is off.
    private static let idle: Duration = .seconds(20)
    private var enabled = false
    private let store = EKEventStore()
    private var task: Task<Void, Never>?
    private var pending: Task<Void, Never>?
    private var observer: NSObjectProtocol?
    private var lastAccess: String?
    private let client = DaemonClient()

    /// Only an installed app bundle may ask for Calendar access (same rule as
    /// `RoutineNotifier`): a `swift run` build has no usage description.
    private var canRun: Bool { Bundle.main.bundleURL.pathExtension == "app" && Bundle.main.bundleIdentifier != nil }

    func start() {
        guard task == nil, canRun else { return }
        observer = NotificationCenter.default.addObserver(forName: .EKEventStoreChanged, object: store, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.changed() }
        }
        task = Task { [weak self] in
            while !Task.isCancelled {
                // Calendar is a choice (first-run setup or Settings): macOS is
                // asked, and events are read, only while it is switched on.
                if await self?.refreshEnabled() == true {
                    await self?.askOnce()
                    await self?.sync()
                    do { try await Task.sleep(for: Self.interval) } catch { return }
                } else {
                    do { try await Task.sleep(for: Self.idle) } catch { return }
                }
            }
        }
    }

    /// Calendar posts a burst of changes while an account syncs; send once
    /// it settles.
    private func changed() {
        pending?.cancel()
        pending = Task { [weak self] in
            do { try await Task.sleep(for: Self.settle) } catch { return }
            await self?.sync()
        }
    }

    /// Asks the daemon whether the person switched Calendar on.
    private func refreshEnabled() async -> Bool {
        let reply = try? await client.request(.string("CalendarStatus"))
        enabled = CalendarPayload.isEnabled(reply)
        return enabled
    }

    /// EventKit shows its prompt only while access was never decided.
    private func askOnce() async {
        guard EKEventStore.authorizationStatus(for: .event) == .notDetermined else { return }
        let store = store
        _ = await withCheckedContinuation { (done: CheckedContinuation<Bool, Never>) in
            store.requestFullAccessToEvents { granted, _ in done.resume(returning: granted) }
        }
    }

    private func sync() async {
        guard enabled else { return }
        let access = CalendarPayload.access(EKEventStore.authorizationStatus(for: .event))
        // Access granted later in System Settings: the store must re-read.
        if access == "Granted", lastAccess != nil, lastAccess != "Granted" { store.reset() }
        lastAccess = access
        var items: [CalendarPayload.Item] = []
        if access == "Granted" {
            let (start, end) = CalendarPayload.window(around: Date())
            let predicate = store.predicateForEvents(withStart: start, end: end, calendars: nil)
            items = store.events(matching: predicate).prefix(CalendarPayload.maxEvents).map(CalendarPayload.item)
        }
        _ = try? await client.request(CalendarPayload.request(items: items, access: access))
    }
}

/// The parts of the bridge that do not need EventKit's permission, kept pure
/// so they are tested.
enum CalendarPayload {
    /// The daemon keeps 2000; a little more is read so its cap decides.
    static let maxEvents = 2500
    static let source = "macOS Calendar"
    private static let conferenceHosts = ["meet.google.com", "zoom.us", "zoom.com", "teams.microsoft.com", "teams.live.com", "webex.com", "whereby.com", "meet.jit.si", "chime.aws", "facetime.apple.com", "gotomeeting.com", "around.co"]

    struct Item: Equatable, Sendable {
        var id: String
        var calendar: String
        var color: String
        var title: String
        var start: Date
        var end: Date
        var allDay: Bool
        var location: String?
        var url: String?
    }

    /// The daemon's answer to `CalendarStatus`: is Calendar switched on?
    static func isEnabled(_ reply: JSONValue?) -> Bool {
        reply?["CalendarStatus"]["enabled"].bool ?? false
    }

    static func access(_ status: EKAuthorizationStatus) -> String {
        switch status {
        case .fullAccess: "Granted"
        case .notDetermined: "NotAsked"
        default: "Denied"
        }
    }

    /// Local midnight 7 days ago to local midnight 35 days ahead.
    static func window(around now: Date, calendar: Calendar = .current) -> (Date, Date) {
        let today = calendar.startOfDay(for: now)
        let start = calendar.date(byAdding: .day, value: -7, to: today) ?? today
        let end = calendar.date(byAdding: .day, value: 35, to: today) ?? today
        return (start, end)
    }

    static func hex(_ color: NSColor?) -> String {
        guard let rgb = color?.usingColorSpace(.sRGB) else { return "" }
        let byte = { (v: CGFloat) in Int((min(max(v, 0), 1) * 255).rounded()) }
        return String(format: "#%02X%02X%02X", byte(rgb.redComponent), byte(rgb.greenComponent), byte(rgb.blueComponent))
    }

    /// The event's own web URL, else the first meeting link in its location
    /// or notes, else a location that is itself a web link.
    static func link(url: URL?, location: String?, notes: String?) -> String? {
        if let url, isWeb(url) { return url.absoluteString }
        let found = [location, notes].compactMap { $0 }.flatMap(links)
        if let meeting = found.first(where: isMeeting) { return meeting.absoluteString }
        if let location, let url = URL(string: location.trimmingCharacters(in: .whitespacesAndNewlines)), isWeb(url), url.host != nil {
            return url.absoluteString
        }
        return nil
    }

    private static func isWeb(_ url: URL) -> Bool { ["http", "https"].contains(url.scheme?.lowercased() ?? "") }

    private static func isMeeting(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        return conferenceHosts.contains { host == $0 || host.hasSuffix("." + $0) }
    }

    private static func links(in text: String) -> [URL] {
        guard let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue) else { return [] }
        return detector.matches(in: text, range: NSRange(text.startIndex..., in: text)).compactMap(\.url).filter(isWeb)
    }

    static func item(_ event: EKEvent) -> Item {
        let start = event.startDate ?? Date()
        let base = event.eventIdentifier ?? event.calendarItemIdentifier
        let location = event.location?.trimmingCharacters(in: .whitespacesAndNewlines)
        return Item(
            // A repeating event's occurrences share one identifier.
            id: "\(base)@\(Int(start.timeIntervalSince1970))",
            calendar: event.calendar?.title ?? "",
            color: hex(event.calendar?.color),
            title: event.title ?? "",
            start: start,
            end: event.endDate ?? start,
            allDay: event.isAllDay,
            location: location?.isEmpty == false ? location : nil,
            url: link(url: event.url, location: event.location, notes: event.notes)
        )
    }

    static func request(items: [Item], access: String) -> JSONValue {
        let events: [JSONValue] = items.map { item in
            var fields: [String: JSONValue] = [
                "id": .string(item.id),
                "calendar": .string(item.calendar),
                "color": .string(item.color),
                "title": .string(item.title),
                "start_ms": .number((item.start.timeIntervalSince1970 * 1000).rounded()),
                "end_ms": .number((item.end.timeIntervalSince1970 * 1000).rounded()),
                "all_day": .bool(item.allDay),
            ]
            if let location = item.location { fields["location"] = .string(location) }
            if let url = item.url { fields["url"] = .string(url) }
            return .object(fields)
        }
        return .object(["SetCalendarEvents": .object(["events": .array(events), "source": .string(source), "access": .string(access)])])
    }
}

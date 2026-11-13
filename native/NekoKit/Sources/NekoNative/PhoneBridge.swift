import AppKit
import EventKit
import NekoKit

/// Puts a reminder in Neko's own "Neko" Reminders list for each thing that
/// needs the person, so their phone tells them (iCloud delivers the alarm),
/// and reports back when one is completed. The daemon decides what to push
/// and when; this only owns EventKit, the way `CalendarBridge` does.
///
/// Every 20 seconds, and 2 seconds after Reminders reports a change, it sends
/// `PhoneRemindersSync` (its access, the reminders in the list, the completed
/// ones) and carries out the plan it gets back. Access is asked only when the
/// plan says the person turned the channel on. See docs/knowledge/phone-push.md.
@MainActor final class PhoneBridge {
    static let shared = PhoneBridge()
    private static let interval: Duration = .seconds(20)
    private static let settle: Duration = .seconds(2)
    private let store = EKEventStore()
    private var task: Task<Void, Never>?
    private var pending: Task<Void, Never>?
    private var observer: NSObjectProtocol?
    private var lastAccess: String?
    private let client = DaemonClient()

    /// Only an installed app bundle may ask for Reminders access (same rule as
    /// `CalendarBridge`): a `swift run` build has no usage description.
    private var canRun: Bool { Bundle.main.bundleURL.pathExtension == "app" && Bundle.main.bundleIdentifier != nil }

    func start() {
        guard task == nil, canRun else { return }
        observer = NotificationCenter.default.addObserver(forName: .EKEventStoreChanged, object: store, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.changed() }
        }
        task = Task { [weak self] in
            while !Task.isCancelled {
                await self?.cycle()
                do { try await Task.sleep(for: Self.interval) } catch { return }
            }
        }
    }

    private func changed() {
        pending?.cancel()
        pending = Task { [weak self] in
            do { try await Task.sleep(for: Self.settle) } catch { return }
            await self?.cycle()
        }
    }

    private func access() -> String { PhonePayload.access(EKEventStore.authorizationStatus(for: .reminder)) }

    private var cycling = false
    private var again = false

    /// One cycle at a time: a change that arrives meanwhile runs one more.
    private func cycle() async {
        if cycling { again = true; return }
        cycling = true
        repeat {
            again = false
            await runCycle()
        } while again
        cycling = false
    }

    private func runCycle() async {
        var access = access()
        // Access granted later in System Settings: the store must re-read.
        if access == "Granted", let last = lastAccess, last != "Granted" { store.reset() }
        lastAccess = access
        var plan = await sync(access: access)
        if plan.enabled, access == "NotAsked" {
            // EventKit shows its prompt only while access was never decided,
            // and only now that the person turned the channel on.
            let store = store
            _ = await withCheckedContinuation { (done: CheckedContinuation<Bool, Never>) in
                store.requestFullAccessToReminders { granted, _ in done.resume(returning: granted) }
            }
            access = self.access()
            lastAccess = access
            plan = await sync(access: access)
        }
        guard access == "Granted", !plan.delete.isEmpty || !plan.create.isEmpty || !plan.finish.isEmpty else { return }
        let created = await apply(plan)
        let finished = finish(plan.finish)
        _ = try? await client.request(PhonePayload.doneRequest(created: created, deleted: plan.delete, finished: finished))
    }

    private func sync(access: String) async -> PhonePayload.Plan {
        var found: [PhonePayload.ReminderInfo] = []
        if access == "Granted", let list = neko() { found = await Self.read(store, list) }
        let request = PhonePayload.syncRequest(access: access, reminders: found)
        guard let reply = try? await client.request(request) else { return PhonePayload.Plan() }
        return PhonePayload.plan(from: reply)
    }

    /// Neko's list, if it exists. Never created just to look.
    private func neko() -> EKCalendar? {
        store.calendars(for: .reminder).first { $0.title == PhonePayload.listTitle }
    }

    private func makeList() -> EKCalendar? {
        let list = EKCalendar(for: .reminder, eventStore: store)
        list.title = PhonePayload.listTitle
        let sources = store.sources.map { PhonePayload.SourceInfo(id: $0.sourceIdentifier, title: $0.title, kind: PhonePayload.kind($0.sourceType)) }
        let preferred = store.defaultCalendarForNewReminders()?.source?.sourceIdentifier
        guard let chosen = PhonePayload.pickSource(sources, preferred: preferred), let source = store.sources.first(where: { $0.sourceIdentifier == chosen }) else { return nil }
        list.source = source
        do { try store.saveCalendar(list, commit: true) } catch { return nil }
        return list
    }

    /// Writes the daemon's echo into each answered reminder's notes and
    /// completes it (it stays in the list, completed, so the echo can be read).
    /// Returns the ids that were saved.
    private func finish(_ items: [PhonePayload.Finish]) -> [String] {
        var done: [String] = []
        for item in items {
            guard let reminder = store.calendarItem(withIdentifier: item.id) as? EKReminder else { continue }
            reminder.notes = item.notes
            // An answer that could not be used keeps its reminder open, with the reason in the notes.
            if !item.keepOpen { reminder.isCompleted = true }
            do {
                try store.save(reminder, commit: false)
                done.append(item.id)
            } catch { continue }
        }
        try? store.commit()
        return done
    }

    /// Carries out the plan and returns what was created.
    private func apply(_ plan: PhonePayload.Plan) async -> [(seq: Int, id: String, key: String)] {
        for id in plan.delete {
            if let reminder = store.calendarItem(withIdentifier: id) as? EKReminder { try? store.remove(reminder, commit: false) }
        }
        var created: [(seq: Int, id: String, key: String)] = []
        if !plan.create.isEmpty, let list = neko() ?? makeList() {
            // A reminder made before an earlier report was lost is reused.
            let known = await knownMarkers(in: list)
            for item in plan.create {
                if item.seq != 0, let id = known[item.seq] { created.append((item.seq, id, item.key)); continue }
                let reminder = EKReminder(eventStore: store)
                reminder.calendar = list
                reminder.title = item.title
                reminder.notes = item.notes
                if let link = item.url, let url = URL(string: link) { reminder.url = url }
                let due = Date().addingTimeInterval(5)
                reminder.dueDateComponents = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute, .second], from: due)
                reminder.addAlarm(EKAlarm(absoluteDate: due))
                do {
                    try store.save(reminder, commit: false)
                    created.append((item.seq, reminder.calendarItemIdentifier, item.key))
                } catch { created.append((item.seq, "", item.key)) }
            }
        }
        try? store.commit()
        return created
    }

    /// Item number to identifier for the reminders already in the list,
    /// found by the marker line in their notes.
    private func knownMarkers(in list: EKCalendar) async -> [Int: String] {
        var out: [Int: String] = [:]
        for info in await Self.read(store, list) where !info.completed {
            if let seq = PhonePayload.seq(fromNotes: info.notes) { out[seq] = info.id }
        }
        return out
    }

    private static func read(_ store: EKEventStore, _ list: EKCalendar) async -> [PhonePayload.ReminderInfo] {
        await withCheckedContinuation { (done: CheckedContinuation<[PhonePayload.ReminderInfo], Never>) in
            store.fetchReminders(matching: store.predicateForReminders(in: [list])) { reminders in
                done.resume(returning: (reminders ?? []).map { PhonePayload.ReminderInfo(id: $0.calendarItemIdentifier, completed: $0.isCompleted, notes: $0.notes ?? "") })
            }
        }
    }
}

/// The parts of the bridge that do not need Reminders permission, kept pure
/// so they are tested.
enum PhonePayload {
    static let listTitle = "Neko"
    /// Written on the last line of a reminder's notes, so a reminder can be
    /// matched to its item without trusting an identifier alone.
    static let markerPrefix = "neko:"

    struct ReminderInfo: Equatable, Sendable {
        var id: String
        var completed: Bool
        var notes: String
    }

    struct NewReminder: Equatable {
        var seq: Int
        var title: String
        var notes: String
        var url: String?
        var key = ""
    }

    /// A reminder the person answered in words: set these notes (the "Got it"
    /// echo) and complete it.
    struct Finish: Equatable {
        var id: String
        var notes: String
        var keepOpen = false
    }

    struct Plan: Equatable {
        var enabled = false
        var create: [NewReminder] = []
        var delete: [String] = []
        var finish: [Finish] = []
    }

    enum SourceKind: Equatable { case iCloud, otherAccount, local, other }

    struct SourceInfo: Equatable {
        var id: String
        var title: String
        var kind: SourceKind
    }

    static func access(_ status: EKAuthorizationStatus) -> String {
        switch status {
        case .fullAccess: "Granted"
        case .notDetermined: "NotAsked"
        default: "Denied"
        }
    }

    static func kind(_ type: EKSourceType) -> SourceKind {
        switch type {
        case .calDAV: .otherAccount
        case .local: .local
        default: .other
        }
    }

    /// Where Neko's list goes: the source Reminders already uses for new
    /// lists when it is iCloud, else any iCloud source, else that default,
    /// else a local list. iCloud is what delivers the alarm to a phone.
    static func pickSource(_ sources: [SourceInfo], preferred: String?) -> String? {
        let isCloud = { (s: SourceInfo) in s.title.lowercased().contains("icloud") && s.kind != .local }
        if let preferred, let source = sources.first(where: { $0.id == preferred }), isCloud(source) { return source.id }
        if let cloud = sources.first(where: isCloud) { return cloud.id }
        if let preferred, sources.contains(where: { $0.id == preferred }) { return preferred }
        return sources.first(where: { $0.kind == .local })?.id ?? sources.first?.id
    }

    /// The item number in a reminder's notes, if it has the marker line.
    /// The marker is the last line that has the prefix: after an echo the
    /// notes end with "Got it: ...", not with the marker.
    static func seq(fromNotes notes: String) -> Int? {
        let lines = notes.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }
        guard let last = lines.last(where: { $0.hasPrefix(markerPrefix) }) else { return nil }
        return Int(last.dropFirst(markerPrefix.count))
    }

    /// Whether the person wrote a reply: a line whose first character (after
    /// spaces) is `>`. Nothing Neko writes into a reminder starts that way.
    static func hasReply(_ notes: String) -> Bool {
        notes.split(whereSeparator: \.isNewline).contains { line in
            let trimmed = line.drop(while: { $0 == " " || $0 == "\t" })
            return trimmed.hasPrefix(">") && !trimmed.dropFirst().allSatisfy { $0 == " " || $0 == "\t" }
        }
    }

    static func syncRequest(access: String, reminders: [ReminderInfo]) -> JSONValue {
        .command("PhoneRemindersSync", [
            "access": .string(access),
            "present": .array(reminders.map { .string($0.id) }),
            "completed": .array(reminders.filter(\.completed).map { .string($0.id) }),
            // Only reminders that carry a reply line send their notes: the
            // daemon never sees the rest of anyone's Reminders text.
            "notes": .array(reminders.filter { hasReply($0.notes) }.map { .object(["id": .string($0.id), "notes": .string($0.notes)]) }),
        ])
    }

    static func doneRequest(created: [(seq: Int, id: String, key: String)], deleted: [String], finished: [String] = []) -> JSONValue {
        .command("PhoneRemindersDone", [
            "created": .array(created.map { .object(["seq": .number(Double($0.seq)), "id": .string($0.id), "key": .string($0.key)]) }),
            "deleted": .array(deleted.map { .string($0) }),
            "finished": .array(finished.map { .string($0) }),
        ])
    }

    /// The daemon's `PhonePlan` answer; anything else is an empty plan.
    static func plan(from reply: JSONValue) -> Plan {
        let body = reply["PhonePlan"]
        guard !body.object.isEmpty else { return Plan() }
        let create = body["create"].array.map { item in
            NewReminder(
                seq: item["seq"].int,
                title: item["title"].string,
                notes: item["notes"].string,
                url: item["url"].string.isEmpty ? nil : item["url"].string,
                key: item["key"].string)
        }
        let finish = body["finish"].array.map { Finish(id: $0["id"].string, notes: $0["notes"].string, keepOpen: $0["keep_open"].bool) }.filter { !$0.id.isEmpty }
        return Plan(enabled: body["enabled"].bool, create: create, delete: body["delete"].array.map(\.string), finish: finish)
    }
}

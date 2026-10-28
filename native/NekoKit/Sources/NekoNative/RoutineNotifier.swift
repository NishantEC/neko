import AppKit
import NekoKit
import UserNotifications

/// Turns routine "needs you" items into macOS notifications. Polls the daemon
/// every 30 seconds for items newer than the last one shown; a click opens the
/// Inbox. The first poll on a Mac only records where the list stands, so
/// installing Neko never replays old items.
@MainActor final class RoutineNotifier: NSObject, UNUserNotificationCenterDelegate {
    static let shared = RoutineNotifier()
    private static let seqKey = "neko.routineNotificationSeq"
    private static let perPoll = 5
    private var task: Task<Void, Never>?
    private weak var model: AppModel?

    /// Only an installed app bundle may use the notification centre.
    private var canNotify: Bool { Bundle.main.bundleURL.pathExtension == "app" && Bundle.main.bundleIdentifier != nil }

    func start(model: AppModel) {
        self.model = model
        guard task == nil, canNotify else { return }
        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.requestAuthorization(options: [.alert, .sound]) { _, _ in }
        task = Task { [weak self] in
            let client = DaemonClient()
            while !Task.isCancelled {
                await self?.poll(client)
                do { try await Task.sleep(for: .seconds(30)) } catch { return }
            }
        }
    }

    private func poll(_ client: DaemonClient) async {
        let defaults = UserDefaults.standard
        let known = defaults.object(forKey: Self.seqKey) as? Int
        let after = known ?? 0
        guard let reply = try? await client.request(.object(["RoutineNotifications": .object(["after": .number(Double(after))])])) else { return }
        let body = reply["RoutineNotifications"]
        guard body != .null else { return }
        let latest = body["latest"].int
        // A daemon whose list starts over (new data folder) is followed down.
        defer { defaults.set(latest, forKey: Self.seqKey) }
        guard known != nil else { return }
        let items = body["items"].array
        for notice in items.prefix(Self.perPoll) {
            let item = notice["item"]
            post(id: "routine-\(item["seq"].int)", title: "\(notice["scope"].string) · \(item["title"].string)", body: item["detail"].string)
        }
        if items.count > Self.perPoll {
            post(id: "routine-more-\(latest)", title: "Neko", body: "\(items.count - Self.perPoll) more things need you. Open the Inbox.")
        }
    }

    private func post(id: String, title: String, body: String) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: id, content: content, trigger: nil))
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        await MainActor.run {
            if OpenNeko.usesBrowser, OpenNeko.inBrowser(route: "inbox") { return }
            WebHomeRoute.open("inbox")
            model?.requestedPage = .homeWeb
            NotificationCenter.default.post(name: .nekoOpenWorkspace, object: nil)
        }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }
}

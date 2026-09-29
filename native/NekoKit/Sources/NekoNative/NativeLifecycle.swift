import AppKit
import Foundation
import NekoKit

struct DaemonRecoveryBackoff {
    private(set) var failures = 0
    mutating func failed() -> TimeInterval {
        failures = min(failures + 1, 6)
        return min(60, pow(2, Double(failures)))
    }
    mutating func recovered() { failures = 0 }
}

@MainActor enum NativeLifecycle {
    private static var daemon: Process?
    private static var recovery: Task<Void, Never>?

    /// The daemon owns singleton detection. Never kill an existing daemon and
    /// never rewrite HOME; development isolation stays in NEKO_DATA_DIR.
    static func startDaemon() {
        launchDaemonIfNeeded()
        guard recovery == nil else { return }
        recovery = Task {
            let client = DaemonClient()
            var backoff = DaemonRecoveryBackoff()
            var delay: TimeInterval = 5
            while !Task.isCancelled {
                do { try await Task.sleep(for: .seconds(delay)) } catch { return }
                do {
                    guard try await client.request(.string("Ping")) == .string("Pong") else {
                        delay = backoff.failed()
                        continue
                    }
                    backoff.recovered()
                    delay = 5
                } catch {
                    guard !Task.isCancelled else { return }
                    // Only a health read is retried. Never replay a mutation or
                    // terminate a live process that might still be doing work.
                    launchDaemonIfNeeded()
                    delay = backoff.failed()
                }
            }
        }
    }

    private static func launchDaemonIfNeeded() {
        guard daemon?.isRunning != true else { return }
        let override = ProcessInfo.processInfo.environment["NEKO_DAEMON_PATH"]
        let bundled = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/neko-daemon").path
        let executable = override ?? bundled
        guard FileManager.default.isExecutableFile(atPath: executable) else { return }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.environment = ProcessInfo.processInfo.environment
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do { try process.run(); daemon = process }
        catch { /* The connection UI reports daemon availability. */ }
    }
}

extension Notification.Name {
    static let nekoOpenWorkspace = Notification.Name("neko.openWorkspace")
    static let nekoOpenPreferences = Notification.Name("neko.openPreferences")
}

import AppKit
import NekoKit

/// Remove Neko from this Mac. Everything goes to the Trash, so it can be put
/// back; nothing is erased outright. Offered only from an installed .app.
@MainActor enum Uninstaller {
    static let launchAgentLabel = "com.neko.launcher"

    struct Plan: Equatable {
        var trash: [URL]
        var launchAgent: URL
        var daemonPath: String
        var bundleID: String?
    }

    static func plan(bundle: URL, home: URL, bundleID: String?, includeData: Bool) -> Plan {
        var trash = [bundle]
        if includeData { trash.append(home.appendingPathComponent("Library/Application Support/neko", isDirectory: true)) }
        return Plan(
            trash: trash,
            launchAgent: home.appendingPathComponent("Library/LaunchAgents/\(launchAgentLabel).plist"),
            daemonPath: bundle.appendingPathComponent("Contents/MacOS/neko-daemon").path,
            bundleID: bundleID
        )
    }

    static var available: Bool { Bundle.main.bundleURL.pathExtension == "app" }

    static func confirmAndRun(_ model: AppModel) {
        guard available else {
            model.error = "Uninstall is available from the installed Neko app."
            return
        }
        let alert = NSAlert()
        alert.messageText = "Uninstall Neko?"
        alert.informativeText = "Neko stops all work, removes its login item and shortcut, and moves the app to the Trash. Your repositories are not touched. You can restore anything from the Trash."
        let keepData = NSButton(checkboxWithTitle: "Also move Neko’s data to the Trash (memory, tickets, settings and task worktrees)", target: nil, action: nil)
        keepData.state = .off
        alert.accessoryView = keepData
        alert.addButton(withTitle: "Uninstall")
        alert.addButton(withTitle: "Cancel")
        alert.buttons.first?.hasDestructiveAction = true
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let plan = plan(bundle: Bundle.main.bundleURL, home: FileManager.default.homeDirectoryForCurrentUser, bundleID: Bundle.main.bundleIdentifier, includeData: keepData.state == .on)
        Task {
            _ = await model.workbench(.string("CancelAllWork"))
            let problems = run(plan)
            if !problems.isEmpty {
                let report = NSAlert()
                report.messageText = "Neko was mostly removed"
                report.informativeText = problems.joined(separator: "\n")
                report.runModal()
            }
            NSApp.terminate(nil)
        }
    }

    /// Returns what could not be done, in plain words.
    static func run(_ plan: Plan) -> [String] {
        var problems: [String] = []
        if FileManager.default.fileExists(atPath: plan.launchAgent.path) {
            launch("/bin/launchctl", ["unload", "-w", plan.launchAgent.path])
            if (try? FileManager.default.trashItem(at: plan.launchAgent, resultingItemURL: nil)) == nil {
                problems.append("Couldn’t remove the login item at \(plan.launchAgent.path).")
            }
        }
        launch("/usr/bin/pkill", ["-f", "^" + NSRegularExpression.escapedPattern(for: plan.daemonPath) + "( |$)"])
        if let id = plan.bundleID { launch("/usr/bin/tccutil", ["reset", "Accessibility", id]) }
        for url in plan.trash where FileManager.default.fileExists(atPath: url.path) {
            do { try FileManager.default.trashItem(at: url, resultingItemURL: nil) }
            catch { problems.append("Couldn’t move \(url.path) to the Trash: \(error.localizedDescription)") }
        }
        return problems
    }

    @discardableResult private static func launch(_ tool: String, _ arguments: [String]) -> Int32 {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: tool)
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do { try process.run(); process.waitUntilExit(); return process.terminationStatus } catch { return -1 }
    }
}


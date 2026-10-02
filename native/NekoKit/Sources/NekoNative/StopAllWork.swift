import AppKit
import NekoKit

/// One action that stops every unfinished ticket and Neko's reply. Worktrees
/// and files are kept; completed remote tool calls cannot be undone.
@MainActor enum StopAllWork {
    static let hotkeyID: UInt32 = 2
    static let shortcutLabel = "⌘⇧Esc"
    static func run(_ model: AppModel) async {
        let running = model.snapshot["tasks"].array.filter { !["ReadyForReview", "Completed", "Failed", "Cancelled"].contains($0["status"].string) }.count
        let replying = model.snapshot["conversation"].array.contains { $0["pending"].bool }
        guard running > 0 || replying else { NSSound.beep(); return }
        if await model.workbench(.string("CancelAllWork")) {
            model.notice = running == 0 ? "Stopped Neko’s reply." : "Stopped \(running) \(running == 1 ? "task" : "tasks"). Worktrees are kept."
        }
    }
}


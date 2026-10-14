import Foundation
import NekoKit

/// The one-line answer to "is anything waiting on me?", built from the same
/// stages as the sidebar and the board so every surface gives the same count.
struct StatusSummary: Equatable {
    let needsAttention: Int
    let working: Int
    let upNext: Int
    init(snapshot: JSONValue) {
        let counts = TaskCounts(tasks: snapshot["tasks"].array)
        needsAttention = counts.waitingOnYou
        working = counts[.running]
        upNext = counts[.upNext]
    }
    var attentionLabel: String { needsAttention == 0 ? "Nothing is waiting on you" : "\(needsAttention) \(needsAttention == 1 ? "task is" : "tasks are") waiting on you" }
    var workingLabel: String {
        let running = working == 0 ? "Nothing running" : "\(working) running"
        return upNext == 0 ? running : "\(running) · \(upNext) up next"
    }
    var badge: String? { needsAttention > 0 ? String(needsAttention) : nil }
}

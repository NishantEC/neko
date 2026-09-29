import NekoKit

/// Counts daemon-owned task state for the native menu. Includes all workspaces
/// so a selected workspace cannot conceal an approval needed elsewhere.
struct StatusSummary: Equatable {
    let needsAttention: Int
    let working: Int
    init(snapshot: JSONValue) {
        let tasks = snapshot["tasks"].array
        needsAttention = tasks.filter { ["AwaitingApproval", "ReadyForReview", "Failed"].contains($0["status"].string) }.count
        working = tasks.filter { ["Queued", "Planning", "Building", "Reviewing"].contains($0["status"].string) }.count
    }
    var attentionLabel: String { needsAttention == 0 ? "Nothing needs your attention" : "\(needsAttention) \(needsAttention == 1 ? "ticket needs" : "tickets need") your attention" }
    var workingLabel: String { working == 0 ? "No tasks running" : "\(working) \(working == 1 ? "task" : "tasks") in progress" }
    var badge: String? { needsAttention > 0 ? String(needsAttention) : nil }
}

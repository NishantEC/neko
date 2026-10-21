import NekoKit

/// Task membership, not a count of processes, models or phase sessions.
struct TaskTeamSummary {
    let children: [JSONValue]

    init(parentID: String, snapshot: JSONValue) {
        var seen = Set<String>()
        let tasks = snapshot["tasks"].array
        children = snapshot["splits"].array
            .filter { $0["parent_id"].string == parentID && $0["approved"].bool }
            .flatMap { $0["subtasks"].array }
            .compactMap { subtask in
                let id = subtask["task_id"].string
                guard !id.isEmpty, id != parentID, seen.insert(id).inserted else { return nil }
                return tasks.first { $0["id"].string == id }
            }
    }

    // The same buckets as `TaskStage`, so a team never disagrees with the board.
    var working: Int { count(["Planning", "Building", "Reviewing"]) }
    var waiting: Int { count(["Queued"]) }
    /// A decision only you can make: an approval, an answer, or a failure.
    var needsYou: Int { count(["AwaitingApproval", "Failed"]) }
    /// Finished and checked, waiting for you to look.
    var toReview: Int { count(["ReadyForReview"]) }
    var completed: Int { count(["Completed"]) }
    var stopped: Int { count(["Cancelled"]) }
    var label: String { children.isEmpty ? "Neko" : "\(children.count) \(children.count == 1 ? "subtask" : "subtasks")" }
    var activity: String {
        [(working, "working"), (waiting, "waiting"), (needsYou, "need you"), (toReview, "to review"),
         (completed, "done"), (stopped, "cancelled")]
            .filter { $0.0 > 0 }.map { "\($0.0) \($0.1)" }.joined(separator: " · ")
    }

    private func count(_ statuses: Set<String>) -> Int {
        children.filter { statuses.contains($0["status"].string) }.count
    }
}

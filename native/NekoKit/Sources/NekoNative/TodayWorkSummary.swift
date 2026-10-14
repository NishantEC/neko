import NekoKit

/// Home's view of the work, built from the same stages as the sidebar and
/// board: decisions first, then results to review, then what is moving.
struct TodayWorkSummary {
    /// Waiting on you: decisions (approvals, failures) first, then reviews.
    let needsYou: [JSONValue]
    /// Moving without you: running now, then up next.
    let working: [JSONValue]
    let counts: TaskCounts

    var activeCount: Int { needsYou.count + working.count }

    init(tasks: [JSONValue], workspaceID: String?) {
        let scoped = tasks.filter { workspaceID == nil || $0["workspace_id"].string == workspaceID }
        func stage(_ s: TaskStage) -> [JSONValue] { scoped.filter { TaskStage(status: $0["status"].string) == s } }
        needsYou = stage(.needsYou) + stage(.review)
        working = stage(.running) + stage(.upNext)
        counts = TaskCounts(tasks: scoped)
    }
}

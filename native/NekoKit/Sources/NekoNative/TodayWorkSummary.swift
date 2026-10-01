import NekoKit

struct TodayWorkSummary {
    let needsYou: [JSONValue]
    let working: [JSONValue]

    var activeCount: Int { needsYou.count + working.count }

    init(tasks: [JSONValue], workspaceID: String?) {
        let scoped = tasks.filter { workspaceID == nil || $0["workspace_id"].string == workspaceID }
        needsYou = scoped.filter { ["AwaitingApproval", "ReadyForReview", "Failed"].contains($0["status"].string) }
        working = scoped.filter { ["Queued", "Planning", "Building", "Reviewing"].contains($0["status"].string) }
    }
}

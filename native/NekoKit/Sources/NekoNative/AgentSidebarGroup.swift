import NekoKit

enum AgentSidebarGroup: String, CaseIterable, Identifiable {
    case needsYou = "needs_you", working, recent
    var id: String { rawValue }
    var title: String {
        switch self { case .needsYou: "Needs you"; case .working: "Working"; case .recent: "Recent" }
    }
    static func matches(_ filter: String, task: JSONValue) -> Bool {
        guard let group = Self(rawValue: filter) else { return true }
        let status = task["status"].string
        switch group {
        case .needsYou: return ["AwaitingApproval", "ReadyForReview", "Failed"].contains(status)
        case .working: return ["Queued", "Planning", "Building", "Reviewing"].contains(status)
        case .recent: return ["Completed", "Cancelled"].contains(status)
        }
    }
    func tasks(in tasks: [JSONValue]) -> [JSONValue] {
        tasks.filter { Self.matches(rawValue, task: $0) }.sorted { $0["updated_at_ms"].int > $1["updated_at_ms"].int }
    }
}

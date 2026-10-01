import SwiftUI
import NekoKit

/// What Neko keeps an eye on. You describe it in plain words (or ask Neko to
/// suggest from your connected tools); Neko writes it up as a responsibility.
/// Readable connected sources are activated by the daemon; explicit pauses stick.
enum Watching {
    static let suggestPrompt = "Look at the tools connected to this workspace and suggest up to three things you should keep an eye on for me. Only suggest what those tools can actually check."

    /// Responsibilities Neko proposed in chat that the user has not acted on yet.
    @MainActor static func suggestedIDs(_ model: AppModel) -> Set<String> {
        let offered = Set(model.snapshot["conversation"].array.flatMap { $0["responsibility_ids"].array.map(\.string) })
        return Set(model.snapshot["mcp"]["responsibilities"].array.filter {
            offered.contains($0.recordID) && !$0["enabled"].bool && $0["last_attempt_ms"] == .null
        }.map(\.recordID))
    }

    /// A starting idea per connected tool, phrased the way a person would ask.
    static func idea(for label: String) -> String {
        let l = label.lowercased()
        if l.contains("linear") { return "New Linear issues assigned to me, and anything of mine that becomes blocked" }
        if l.contains("sentry") { return "New or spiking Sentry errors in my projects" }
        if l.contains("github") { return "Pull requests waiting on my review, and failing checks on my own PRs" }
        if l.contains("slack") { return "Slack messages that mention me or are waiting on my reply" }
        if l.contains("gmail") || l.contains("mail") { return "Emails that need a reply from me today" }
        if l.contains("calendar") { return "Meetings today that need preparation" }
        if l.contains("notion") { return "Notion pages assigned to me or recently changed in my projects" }
        return "Anything new in \(label) that needs my attention"
    }

    @MainActor static func connections(_ model: AppModel, workspace: String?) -> [JSONValue] {
        model.snapshot["mcp"]["connections"].array.filter { c in
            c["enabled"].bool && (c["workspace_id"].string.isEmpty || workspace == nil || c["workspace_id"].string == workspace)
        }
    }

    @MainActor static func label(_ model: AppModel, connection id: String) -> String {
        model.snapshot["mcp"]["connections"].array.first { $0.recordID == id }?["label"].string ?? "Removed tool"
    }

    /// A responsibility can use a connected catalog in its workspace once turned on.
    @MainActor static func ungranted(_ model: AppModel, _ item: JSONValue) -> [String] {
        let workspace = item["workspace_id"].string
        let available = Set(model.snapshot["mcp"]["connections"].array.filter { connection in
            connection["enabled"].bool && connection["trusted"].bool && connection["error"].string.isEmpty &&
            (connection["workspace_id"].string.isEmpty || connection["workspace_id"].string == workspace) &&
            !connection["tools"].array.isEmpty
        }.map(\.recordID))
        return item["connection_ids"].array.map(\.string).filter { !available.contains($0) }.map { label(model, connection: $0) }
    }

    @MainActor static func reviewAccess(_ model: AppModel, _ item: JSONValue) {
        model.selectedWorkspace = item["workspace_id"].string
        NotificationCenter.default.post(name: .nekoNavigate, object: "Tools & skills")
    }

    @MainActor static func turnOn(_ model: AppModel, _ item: JSONValue) {
        guard ungranted(model, item).isEmpty else { reviewAccess(model, item); return }
        Task {
            let saved = await model.workbench(nested("Mcp", "SaveResponsibility", ["responsibility": replacing(item, ["enabled": .bool(true)])]))
            if saved { await model.workbench(nested("Mcp", "Wake", ["responsibility_id": item["id"]])) }
        }
    }

    @MainActor static func remove(_ model: AppModel, _ item: JSONValue) {
        submit(model, nested("Mcp", "RemoveResponsibility", ["responsibility_id": item["id"]]))
    }
}

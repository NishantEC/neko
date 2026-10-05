import Observation
import SwiftUI
import NekoKit

/// Deliberately has no AppModel, IPC or provider dependency. This page is a local design study.
@MainActor @Observable final class DesignLabModel {
    enum Layout: String, CaseIterable, Identifiable {
        case inline = "Inline", context = "Context"
        var id: Self { self }
    }

    enum Scenario: String, CaseIterable, Identifiable {
        case single = "One task", split = "Split task", review = "Review"
        var id: Self { self }
    }

    struct Agent: Identifiable {
        let id: String
        let title: String
        let role: String
        let status: String
        let activity: String
        let color: Color
    }

    enum Plugin: String, CaseIterable, Identifiable {
        case repository = "Repository tools", swift = "SwiftUI craft"
        var id: Self { self }
        var symbol: String { self == .repository ? "arrow.triangle.branch" : "swift" }
        var detail: String {
            self == .repository ? "Issues, pull requests and repository context" : "Native layouts and accessibility patterns"
        }
    }

    var layout = Layout.inline
    var scenario = Scenario.single
    var draft = "Keep the dark background neutral. No blue wash."
    var plugins: Set<Plugin> = Set(Plugin.allCases)
    var messages: [String] = []
    var attachmentName: String? = "appearance-reference.png"
    var error: String?
    var selectedAgent: Agent?
    var showingLibrary = false

    // Sample records use the real snapshot shape and the same membership reducer.
    // No runtime is started when changing this scenario.
    var sampleSnapshot: JSONValue {
        let status = scenario == .review ? "Reviewing" : scenario == .split ? "Queued" : "Building"
        var tasks: [JSONValue] = [.object(["id": .string("preview-main"), "title": .string("Ship dark mode"), "status": .string(status)])]
        var splits: [JSONValue] = []
        if scenario == .split {
            tasks += [
                .object(["id": .string("tokens"), "title": .string("Shared colour tokens"), "status": .string("Building")]),
                .object(["id": .string("settings"), "title": .string("Appearance control"), "status": .string("Queued")])
            ]
            splits = [.object(["parent_id": .string("preview-main"), "approved": .bool(true), "subtasks": .array([
                .object(["task_id": .string("tokens"), "depends_on": .array([])]),
                .object(["task_id": .string("settings"), "depends_on": .array([.number(0)])])
            ])])]
        }
        return .object(["tasks": .array(tasks), "splits": .array(splits)])
    }
    var team: TaskTeamSummary { TaskTeamSummary(parentID: "preview-main", snapshot: sampleSnapshot) }
    var owner: Agent {
        Agent(id: "preview-main", title: "Neko", role: "Task owner",
              status: scenario == .split ? "Waiting for subtasks" : scenario == .review ? "In review" : "Building",
              activity: scenario == .split ? "This task waits for its two subtasks, then combines and reviews their results."
                  : scenario == .review ? "A fresh reviewer is checking the result. Review is a phase of this task, not another subtask."
                  : "One saved agent session is updating the appearance. It plans and builds; a fresh reviewer checks the result.",
              color: Color(red: 0.84, green: 0.86, blue: 0.93))
    }
    var agents: [Agent] {
        team.children.map { task in
            let tokens = task["id"].string == "tokens"
            return Agent(id: task["id"].string, title: task["title"].string, role: "Subtask",
                         status: task["status"].string == "Queued" ? "Waiting" : task["status"].string,
                         activity: tokens ? "Replacing fixed colours with shared appearance tokens."
                             : "Waiting for Shared colour tokens before adding the appearance control.",
                         color: tokens ? Color(red: 0.83, green: 0.74, blue: 0.91) : Color(red: 0.72, green: 0.87, blue: 0.82))
        }
    }
    var visibleAgents: [Agent] { agents.isEmpty ? [owner] : agents }
    var teamActivity: String { team.children.isEmpty ? owner.status : team.activity }

    var canSend: Bool { !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    func send() {
        guard canSend else { return }
        messages.append(draft)
        draft = ""
        attachmentName = nil
    }

    func toggle(_ plugin: Plugin) {
        if !plugins.insert(plugin).inserted { plugins.remove(plugin) }
    }

    func reset() {
        draft = "Keep the dark background neutral. No blue wash."
        plugins = Set(Plugin.allCases)
        messages = []
        attachmentName = "appearance-reference.png"
        error = nil
        selectedAgent = nil
        showingLibrary = false
    }
}

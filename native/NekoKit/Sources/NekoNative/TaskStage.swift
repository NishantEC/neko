import SwiftUI
import NekoKit

/// Where a task stands, in the words the whole app uses. Every count, sidebar
/// group, board column and menu bar state comes from this one mapping, so two
/// places can never disagree about what "needs you" means.
enum TaskStage: String, CaseIterable, Identifiable {
    /// Blocked on a decision from you: an approval, an answer, or a failure.
    case needsYou
    /// Finished and checked; waiting for you to look at the result.
    case review
    /// An agent is working on it now.
    case running
    /// Waiting for its turn (the "tasks at once" limit).
    case upNext
    /// Accepted, landed or stopped.
    case done

    var id: String { rawValue }

    init(status: String) {
        switch status {
        case "AwaitingApproval", "Failed": self = .needsYou
        case "ReadyForReview": self = .review
        case "Planning", "Building", "Reviewing": self = .running
        case "Queued": self = .upNext
        default: self = .done
        }
    }

    var title: String {
        switch self {
        case .needsYou: "Needs you"
        case .review: "Ready to review"
        case .running: "Running"
        case .upNext: "Up next"
        case .done: "Done"
        }
    }

    var color: Color {
        switch self {
        case .needsYou: NekoStyle.amber
        case .review: NekoStyle.sky
        case .running: NekoStyle.accent
        case .upNext: N.text4
        case .done: NekoStyle.mint
        }
    }

    /// One plain sentence for an empty group, so it never reads as broken.
    var emptyLine: String {
        switch self {
        case .needsYou: "Nothing is waiting on you."
        case .review: "Nothing to review yet."
        case .running: "No agent is working right now."
        case .upNext: "Nothing is queued."
        case .done: "Nothing finished yet."
        }
    }
}

/// How many tasks are in each stage, from one snapshot.
struct TaskCounts: Equatable {
    private(set) var counts: [TaskStage: Int] = [:]
    init(tasks: [JSONValue]) {
        for task in tasks { counts[TaskStage(status: task["status"].string), default: 0] += 1 }
    }
    subscript(stage: TaskStage) -> Int { counts[stage, default: 0] }
    /// Things only you can move forward: decisions plus results to review.
    var waitingOnYou: Int { self[.needsYou] + self[.review] }
}

/// How a task reads anywhere it appears: a human title, where it came from,
/// which repository it touches and its pull request.
struct TaskText {
    let task: JSONValue
    let snapshot: JSONValue

    /// The short human title when Neko has written one, else the source title.
    var title: String {
        let display = task["display_title"].string.trimmingCharacters(in: .whitespacesAndNewlines)
        return display.isEmpty ? task["title"].string : display
    }
    /// The original source title, worth showing only when it differs.
    var sourceTitle: String? {
        let original = task["title"].string
        return original == title ? nil : original
    }
    var stage: TaskStage { TaskStage(status: task["status"].string) }
    /// The repository folder's name, e.g. "athena".
    var repository: String? {
        let root = snapshot["task_roots"][task.recordID].string
        return root.isEmpty ? nil : URL(fileURLWithPath: root).lastPathComponent
    }
    /// "Sentry · CONNECT-PROD-P5E" or "Linear · HEA-1427" when a watch filed it.
    var source: String? {
        guard let observed = snapshot["mcp"]["sources"].array.first(where: { $0["task_id"].string == task.recordID }) else { return nil }
        let label = snapshot["mcp"]["connections"].array.first { $0["id"] == observed["connection_id"] }?["label"].string ?? ""
        let name = Self.sourceName(label)
        // A short id a person would quote (HEA-1427) earns its place; a long
        // machine id or one the title already shows does not.
        let id = observed["external_id"].string
        let useful = id.count <= 12 && !title.contains(id)
        return [name, useful ? id : nil].compactMap { $0?.isEmpty == false ? $0 : nil }.joined(separator: " · ")
    }
    /// "#4616" for a task that opened a pull request.
    var pullRequest: String? {
        let url = task["pull_request"]["url"].string
        return url.isEmpty ? nil : url.split(separator: "/").last.map { "#\($0)" }
    }
    /// A connection label as a person would say it: "linear-personal" → "Linear".
    static func sourceName(_ label: String) -> String {
        let lower = label.lowercased()
        for known in ["sentry", "linear", "github", "jira", "slack", "notion", "figma", "paper"] where lower.contains(known) {
            return known.prefix(1).uppercased() + known.dropFirst()
        }
        return label
    }
    /// Everything secondary on one line: repository, pull request, source.
    var detail: String {
        [repository, pullRequest, source].compactMap { $0 }.joined(separator: " · ")
    }
}
